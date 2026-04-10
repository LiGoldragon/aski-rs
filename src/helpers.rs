//! Helper functions called by aski-generated code.
//! to_snake, to_rust_type, strip_vec, from_ordinal.

pub trait StringExt {
    fn to_snake(&self) -> String;
    fn to_rust_type(&self) -> String;
    fn to_param_type(&self) -> String;
    fn strip_vec(&self) -> String;
    fn all_fields_copy(&self) -> String;
    fn needs_pascal_alias(&self) -> String;
}

impl StringExt for String {
    fn to_snake(&self) -> String { to_snake(self) }
    fn to_rust_type(&self) -> String { to_rust_type(self) }
    fn to_param_type(&self) -> String { to_param_type(self) }
    fn strip_vec(&self) -> String { strip_vec(self) }
    fn all_fields_copy(&self) -> String { if all_fields_copy(self) { "Copy, ".into() } else { String::new() } }
    fn needs_pascal_alias(&self) -> String { needs_pascal_alias(self) }
}

impl StringExt for str {
    fn to_snake(&self) -> String { to_snake(self) }
    fn to_rust_type(&self) -> String { to_rust_type(self) }
    fn to_param_type(&self) -> String { to_param_type(self) }
    fn strip_vec(&self) -> String { strip_vec(self) }
    fn all_fields_copy(&self) -> String { if all_fields_copy(self) { "Copy, ".into() } else { String::new() } }
    fn needs_pascal_alias(&self) -> String { needs_pascal_alias(self) }
}

/// Check if a Rust type is Copy-eligible (no String, no Vec).
pub fn is_copy_type(t: &str) -> bool {
    match t {
        "I8" | "I16" | "I32" | "I64" | "U8" | "U16" | "U32" | "U64" |
        "F32" | "F64" | "Bool" => true,
        _ if t.starts_with("Vec{") => false,
        "String" => false,
        // Enum types (domains) are Copy
        _ => true,
    }
}

/// Check if all fields of a struct (given as comma-separated types) are Copy.
pub fn all_fields_copy(field_types: &str) -> bool {
    if field_types.is_empty() { return true; }
    field_types.split(',').all(|t| is_copy_type(t.trim()))
}

pub fn to_snake(s: &str) -> String {
    let mut out = String::new();
    for (i, ch) in s.chars().enumerate() {
        if ch.is_uppercase() && i > 0 {
            out.push('_');
        }
        out.push(ch.to_ascii_lowercase());
    }
    out
}

pub fn to_rust_type(t: &str) -> String {
    match t {
        "F32" => "f32".into(),
        "F64" => "f64".into(),
        "I8" => "i8".into(),
        "I16" => "i16".into(),
        "I32" => "i32".into(),
        "I64" => "i64".into(),
        "U8" => "u8".into(),
        "U16" => "u16".into(),
        "U32" => "u32".into(),
        "U64" => "u64".into(),
        "Bool" => "bool".into(),
        "String" => "String".into(),
        _ if t.starts_with("Vec{") && t.ends_with('}') => {
            format!("Vec<{}>", to_rust_type(&t[4..t.len() - 1]))
        }
        _ => t.to_string(),
    }
}

/// Returns the PascalCase from_str alias line if the variant is multi-word, empty otherwise.
pub fn needs_pascal_alias(name: &str) -> String {
    let snake = to_snake(name);
    if snake.contains('_') {
        // Multi-word: emit PascalCase alias
        format!("            \"{}\" => Some(Self::{}),\n", name, name)
    } else {
        String::new()
    }
}

pub fn to_param_type(t: &str) -> String {
    match t {
        "String" => "&str".into(),
        _ => to_rust_type(t),
    }
}

pub fn strip_vec(t: &str) -> String {
    if t.starts_with("Vec{") && t.ends_with('}') {
        t[4..t.len() - 1].to_string()
    } else {
        t.to_string()
    }
}
