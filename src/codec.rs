//! Aski codec — bidirectional serialization between aski text and ordinal bytes.
//!
//! World is the top-level Domain containing all domains. The first byte of
//! any message is the World variant ordinal — which domain this data belongs to.
//! The remaining bytes are that domain's own ordinal data.
//!
//! encode: aski construction syntax → ordinal bytes (self-describing)
//! decode: ordinal bytes → aski construction syntax
//!
//! Round-trip: encode(decode(bytes)) == bytes
//! No type_name parameter — the data describes itself via World.

use crate::ir::World;

// ═══════════════════════════════════════════════════════════════
// World — the top-level Domain, derived from the DB
// ═══════════════════════════════════════════════════════════════

/// Query all top-level domains in declaration order.
/// These are World's variants — their ordinal is their position.
fn world_variants(db: &World) -> Result<Vec<(i32, String, String)>, String> {
    // All top-level nodes (parent=null) that are domains or structs, ordered by id
    let mut nodes: Vec<(i64, String, String)> = db.Node.iter()
        .filter(|(_, kind, _, parent, _, _, _)| parent.is_none() && (kind == "domain" || kind == "struct"))
        .map(|(id, kind, name, _, _, _, _)| (*id, kind.clone(), name.clone()))
        .collect();
    nodes.sort_by_key(|(id, _, _)| *id);
    Ok(nodes.into_iter().enumerate().map(|(ordinal, (_, kind, name))| {
        (ordinal as i32, name, kind)
    }).collect())
}

/// Look up a World variant name by ordinal.
fn world_variant_name(db: &World, ordinal: i32) -> Result<(String, String), String> {
    let variants = world_variants(db)?;
    variants.iter()
        .find(|(o, _, _)| *o == ordinal)
        .map(|(_, name, kind)| (name.clone(), kind.clone()))
        .ok_or_else(|| format!("no World variant at ordinal {ordinal}"))
}

/// Look up a World variant ordinal by name.
fn world_variant_ordinal(db: &World, name: &str) -> Result<(i32, String), String> {
    let variants = world_variants(db)?;
    variants.iter()
        .find(|(_, n, _)| n == name)
        .map(|(o, _, kind)| (*o, kind.clone()))
        .ok_or_else(|| format!("'{name}' is not a World variant"))
}

// ═══════════════════════════════════════════════════════════════
// Schema queries
// ═══════════════════════════════════════════════════════════════

fn type_kind(db: &World, type_name: &str) -> Result<Option<String>, String> {
    Ok(db.Node.iter()
        .find(|(_, kind, name, _, _, _, _)| (kind == "domain" || kind == "struct") && name == type_name)
        .map(|(_, kind, _, _, _, _, _)| kind.clone()))
}

fn struct_fields(db: &World, type_name: &str) -> Result<Vec<(String, String)>, String> {
    let fields = crate::ir::query_struct_fields(db, type_name)?;
    Ok(fields.into_iter().map(|(_, name, type_ref)| (name, type_ref)).collect())
}

fn variant_name(db: &World, domain: &str, ordinal: i32) -> Result<String, String> {
    let variants = crate::ir::query_domain_variants(db, domain)?;
    variants.iter()
        .find(|(o, _, _)| *o == ordinal)
        .map(|(_, name, _)| name.clone())
        .ok_or_else(|| format!("no variant at ordinal {ordinal} in {domain}"))
}

fn variant_ordinal(db: &World, domain: &str, name: &str) -> Result<i32, String> {
    let variants = crate::ir::query_domain_variants(db, domain)?;
    variants.iter()
        .find(|(_, n, _)| n == name)
        .map(|(o, _, _)| *o)
        .ok_or_else(|| format!("variant '{name}' not found in {domain}"))
}

// ═══════════════════════════════════════════════════════════════
// Decode: ordinal bytes → aski construction syntax
// ═══════════════════════════════════════════════════════════════

/// Decode ordinal bytes to aski construction syntax.
/// First byte = World variant (which domain). Rest = that domain's data.
///
/// Example: decode(db, &[4, 5, 8, 8])
///   where World variant 4 = Placement
///   → "Placement(Body(Jupiter) Position(Sagittarius) HouseNum(Ninth))"
pub fn decode(db: &World, bytes: &[u8]) -> Result<String, String> {
    if bytes.is_empty() {
        return Err("empty bytes".to_string());
    }

    let world_ord = bytes[0] as i32;
    let (type_name, kind) = world_variant_name(db, world_ord)?;
    let ordinals: Vec<i32> = bytes[1..].iter().map(|b| *b as i32).collect();
    let mut cursor = 0;

    match kind.as_str() {
        "domain" => {
            if ordinals.is_empty() {
                // Domain with no further data — the World variant IS the value
                Ok(type_name)
            } else {
                let variant = variant_name(db, &type_name, ordinals[0])?;
                Ok(format!("{type_name}.{variant}"))
            }
        }
        "struct" => {
            let inner = decode_struct(db, &type_name, &ordinals, &mut cursor)?;
            Ok(inner)
        }
        _ => Err(format!("unknown kind {kind} for {type_name}")),
    }
}

fn decode_struct(
    db: &World,
    type_name: &str,
    ordinals: &[i32],
    cursor: &mut usize,
) -> Result<String, String> {
    let fields = struct_fields(db, type_name)?;
    let mut parts = Vec::new();
    for (field_name, field_type) in &fields {
        let val = decode_value(db, field_type, ordinals, cursor)?;
        parts.push(format!("{field_name}({val})"));
    }
    Ok(format!("{type_name}({})", parts.join(" ")))
}

fn decode_value(
    db: &World,
    type_name: &str,
    ordinals: &[i32],
    cursor: &mut usize,
) -> Result<String, String> {
    match type_kind(db, type_name)?.as_deref() {
        Some("domain") => {
            if *cursor >= ordinals.len() {
                return Err(format!("not enough bytes for domain {type_name}"));
            }
            let ordinal = ordinals[*cursor];
            *cursor += 1;
            variant_name(db, type_name, ordinal)
        }
        Some("struct") => {
            decode_struct(db, type_name, ordinals, cursor)
        }
        _ => {
            if *cursor >= ordinals.len() {
                return Err(format!("not enough bytes for {type_name}"));
            }
            let val = ordinals[*cursor];
            *cursor += 1;
            Ok(val.to_string())
        }
    }
}

// ═══════════════════════════════════════════════════════════════
// Encode: aski construction syntax → ordinal bytes
// ═══════════════════════════════════════════════════════════════

/// Encode aski construction syntax to ordinal bytes.
/// First byte = World variant ordinal. Rest = domain data.
///
/// Example: encode(db, "Placement(Body(Jupiter) Position(Sagittarius) HouseNum(Ninth))")
///   → [world_ord_of_Placement, 5, 8, 8]
pub fn encode(db: &World, aski_expr: &str) -> Result<Vec<u8>, String> {
    let tokens = tokenize(aski_expr)?;
    if tokens.is_empty() {
        return Err("empty expression".to_string());
    }

    // First token is the type name — look up in World
    let type_name = &tokens[0];
    let (world_ord, kind) = world_variant_ordinal(db, type_name)?;
    let mut bytes = vec![world_ord as u8];

    match kind.as_str() {
        "domain" => {
            // Bare domain name or Domain.Variant
            if tokens.len() > 1 {
                let mut cursor = 1; // skip type name
                // Check for Variant(data) or just Variant
                if cursor < tokens.len() && tokens[cursor] == "(" {
                    cursor += 1; // skip (
                }
                if cursor < tokens.len() {
                    let variant = &tokens[cursor];
                    let ord = variant_ordinal(db, type_name, variant)?;
                    bytes.push(ord as u8);
                }
            }
        }
        "struct" => {
            let mut cursor = 0;
            let field_bytes = encode_struct(db, type_name, &tokens, &mut cursor)?;
            bytes.extend(field_bytes);
        }
        _ => return Err(format!("unknown kind {kind}")),
    }

    Ok(bytes)
}

fn tokenize(expr: &str) -> Result<Vec<String>, String> {
    let mut tokens = Vec::new();
    let chars: Vec<char> = expr.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        match chars[i] {
            '(' => { tokens.push("(".to_string()); i += 1; }
            ')' => { tokens.push(")".to_string()); i += 1; }
            '.' => { tokens.push(".".to_string()); i += 1; }
            ' ' | '\n' | '\t' => { i += 1; }
            c if c.is_ascii_uppercase() => {
                let start = i;
                while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') {
                    i += 1;
                }
                tokens.push(chars[start..i].iter().collect());
            }
            c if c.is_ascii_digit() || c == '-' => {
                let start = i;
                while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.' || chars[i] == '-') {
                    i += 1;
                }
                tokens.push(chars[start..i].iter().collect());
            }
            c => return Err(format!("unexpected char '{c}' in expression")),
        }
    }
    Ok(tokens)
}

fn encode_struct(
    db: &World,
    type_name: &str,
    tokens: &[String],
    cursor: &mut usize,
) -> Result<Vec<u8>, String> {
    let fields = struct_fields(db, type_name)?;
    let mut bytes = Vec::new();

    // Skip type name and opening paren
    if *cursor < tokens.len() && tokens[*cursor] == type_name {
        *cursor += 1;
    }
    if *cursor < tokens.len() && tokens[*cursor] == "(" {
        *cursor += 1;
    }

    for (field_name, field_type) in &fields {
        // Skip field name and opening paren
        if *cursor < tokens.len() && tokens[*cursor] == *field_name {
            *cursor += 1;
        }
        if *cursor < tokens.len() && tokens[*cursor] == "(" {
            *cursor += 1;
        }
        let field_bytes = encode_value(db, &field_type, tokens, cursor)?;
        bytes.extend(field_bytes);
        // Skip closing paren
        if *cursor < tokens.len() && tokens[*cursor] == ")" {
            *cursor += 1;
        }
    }

    // Skip closing paren of struct
    if *cursor < tokens.len() && tokens[*cursor] == ")" {
        *cursor += 1;
    }

    Ok(bytes)
}

fn encode_value(
    db: &World,
    type_name: &str,
    tokens: &[String],
    cursor: &mut usize,
) -> Result<Vec<u8>, String> {
    match type_kind(db, type_name)?.as_deref() {
        Some("domain") => {
            if *cursor >= tokens.len() {
                return Err(format!("expected variant for {type_name}"));
            }
            let name = &tokens[*cursor];
            *cursor += 1;
            let ordinal = variant_ordinal(db, type_name, name)?;
            Ok(vec![ordinal as u8])
        }
        Some("struct") => {
            encode_struct(db, type_name, tokens, cursor)
        }
        _ => {
            if *cursor >= tokens.len() {
                return Err(format!("expected value for {type_name}"));
            }
            let val: i32 = tokens[*cursor].parse()
                .map_err(|_| format!("expected integer for {type_name}, got '{}'", tokens[*cursor]))?;
            *cursor += 1;
            Ok(vec![val as u8])
        }
    }
}

// ═══════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir;
    use crate::parser::parse_source;

    fn setup_world() -> World {
        let mut world = ir::create_world();
        let src = "\
Element (Fire Earth Air Water)
Modality (Cardinal Fixed Mutable)
Sign (Aries Taurus Gemini Cancer Leo Virgo Libra Scorpio Sagittarius Capricorn Aquarius Pisces)
Planet (Sun Moon Mercury Venus Mars Jupiter Saturn Uranus Neptune Pluto)
House (First Second Third Fourth Fifth Sixth Seventh Eighth Ninth Tenth Eleventh Twelfth)
Placement { Body Planet Position Sign HouseNum House }
ChartSummary { SunPlacement Placement MoonPlacement Placement DominantElement Element }";
        let items = parse_source(src).unwrap();
        ir::insert_ast(&mut world, &items).unwrap();
        ir::run_rules(&mut world);
        world
    }

    #[test]
    fn world_contains_all_types() {
        let db = setup_world();
        let variants = world_variants(&db).unwrap();
        let names: Vec<&str> = variants.iter().map(|(_, n, _)| n.as_str()).collect();
        assert!(names.contains(&"Element"));
        assert!(names.contains(&"Placement"));
        assert!(names.contains(&"ChartSummary"));
        assert_eq!(variants.len(), 7); // 5 enum-domains + 2 struct-domains
    }

    // ── Decode ──

    #[test]
    fn decode_domain_variant() {
        let db = setup_world();
        let (element_ord, _) = world_variant_ordinal(&db, "Element").unwrap();
        // World byte + variant byte
        let bytes = vec![element_ord as u8, 0]; // Fire
        assert_eq!(decode(&db, &bytes).unwrap(), "Element.Fire");
    }

    #[test]
    fn decode_struct_message() {
        let db = setup_world();
        let (placement_ord, _) = world_variant_ordinal(&db, "Placement").unwrap();
        // World byte + field ordinals
        let bytes = vec![placement_ord as u8, 5, 8, 8];
        assert_eq!(
            decode(&db, &bytes).unwrap(),
            "Placement(Body(Jupiter) Position(Sagittarius) HouseNum(Ninth))"
        );
    }

    #[test]
    fn decode_nested_struct() {
        let db = setup_world();
        let (chart_ord, _) = world_variant_ordinal(&db, "ChartSummary").unwrap();
        let bytes = vec![chart_ord as u8, 0, 0, 0, 1, 3, 3, 0];
        assert_eq!(
            decode(&db, &bytes).unwrap(),
            "ChartSummary(SunPlacement(Placement(Body(Sun) Position(Aries) HouseNum(First))) MoonPlacement(Placement(Body(Moon) Position(Cancer) HouseNum(Fourth))) DominantElement(Fire))"
        );
    }

    #[test]
    fn decode_empty_errors() {
        let db = setup_world();
        assert!(decode(&db, &[]).is_err());
    }

    #[test]
    fn decode_invalid_world_ordinal_errors() {
        let db = setup_world();
        assert!(decode(&db, &[255]).is_err());
    }

    // ── Encode ──

    #[test]
    fn encode_struct_message() {
        let db = setup_world();
        let (placement_ord, _) = world_variant_ordinal(&db, "Placement").unwrap();
        let bytes = encode(&db, "Placement(Body(Jupiter) Position(Sagittarius) HouseNum(Ninth))").unwrap();
        assert_eq!(bytes[0], placement_ord as u8);
        assert_eq!(&bytes[1..], &[5, 8, 8]);
    }

    #[test]
    fn encode_nested_struct() {
        let db = setup_world();
        let (chart_ord, _) = world_variant_ordinal(&db, "ChartSummary").unwrap();
        let bytes = encode(&db, "ChartSummary(SunPlacement(Placement(Body(Sun) Position(Aries) HouseNum(First))) MoonPlacement(Placement(Body(Moon) Position(Cancer) HouseNum(Fourth))) DominantElement(Fire))").unwrap();
        assert_eq!(bytes[0], chart_ord as u8);
        assert_eq!(&bytes[1..], &[0, 0, 0, 1, 3, 3, 0]);
    }

    #[test]
    fn encode_unknown_type_errors() {
        let db = setup_world();
        assert!(encode(&db, "Nonexistent(Foo(Bar))").is_err());
    }

    // ── Round-trip ──

    #[test]
    fn roundtrip_struct() {
        let db = setup_world();
        let aski = "Placement(Body(Jupiter) Position(Sagittarius) HouseNum(Ninth))";
        let bytes = encode(&db, aski).unwrap();
        let decoded = decode(&db, &bytes).unwrap();
        assert_eq!(decoded, aski);
    }

    #[test]
    fn roundtrip_nested() {
        let db = setup_world();
        let aski = "ChartSummary(SunPlacement(Placement(Body(Sun) Position(Aries) HouseNum(First))) MoonPlacement(Placement(Body(Moon) Position(Cancer) HouseNum(Fourth))) DominantElement(Fire))";
        let bytes = encode(&db, aski).unwrap();
        let decoded = decode(&db, &bytes).unwrap();
        assert_eq!(decoded, aski);
    }

    #[test]
    fn roundtrip_bytes_to_aski_to_bytes() {
        let db = setup_world();
        let (chart_ord, _) = world_variant_ordinal(&db, "ChartSummary").unwrap();
        let original = vec![chart_ord as u8, 0, 0, 0, 1, 3, 3, 0];
        let aski = decode(&db, &original).unwrap();
        let re_encoded = encode(&db, &aski).unwrap();
        assert_eq!(re_encoded, original);
    }

    #[test]
    fn roundtrip_all_placements() {
        let db = setup_world();
        for planet in 0u8..10 {
            for sign in 0u8..12 {
                let (ord, _) = world_variant_ordinal(&db, "Placement").unwrap();
                let bytes = vec![ord as u8, planet, sign, 0];
                let aski = decode(&db, &bytes).unwrap();
                let re_encoded = encode(&db, &aski).unwrap();
                assert_eq!(re_encoded, bytes, "round-trip failed for {aski}");
            }
        }
    }
}
