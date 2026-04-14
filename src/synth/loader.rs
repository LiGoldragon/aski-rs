//! Synth loader — hardcoded parser for .synth files.
//!
//! This is the ONE hardcoded parser in the bootstrap.
//! It reads .synth files and produces Dialect structs.
//! Everything else is data-driven from the loaded dialects.

use std::collections::HashMap;
use std::fs;
use std::path::Path;
use super::types::*;

/// Load all .synth files from a directory. Returns dialect name → Dialect.
pub fn load_all(dir: &Path) -> Result<HashMap<String, Dialect>, String> {
    let mut dialects = HashMap::new();
    for entry in fs::read_dir(dir).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        if path.extension().map(|e| e == "synth").unwrap_or(false) {
            let name = path.file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("unknown")
                .to_string();
            let source = fs::read_to_string(&path).map_err(|e| e.to_string())?;
            let dialect = load_dialect(&name, &source)?;
            dialects.insert(name, dialect);
        }
    }
    Ok(dialects)
}

/// Load a single .synth file into a Dialect.
pub fn load_dialect(name: &str, source: &str) -> Result<Dialect, String> {
    let lines: Vec<&str> = source.lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty() && !l.starts_with(";;"))
        .collect();

    let mut rules: Vec<Rule> = Vec::new();
    let mut choice_alts: Vec<ChoiceAlternative> = Vec::new();

    for line in &lines {
        if line.starts_with("//") {
            let content = line[2..].trim();
            // Check for cardinality prefix on the alternative
            let (cardinality, items_str) = match content.chars().next() {
                Some('*') => (Card::ZeroOrMore, content[1..].trim()),
                Some('+') => (Card::OneOrMore, content[1..].trim()),
                Some('?') => (Card::Optional, content[1..].trim()),
                Some('!') => (Card::One, content[1..].trim()),
                _ => (Card::ZeroOrMore, content), // default: zero or more
            };
            let items = parse_spaced_items(items_str)?;
            choice_alts.push(ChoiceAlternative { items, cardinality });
        } else {
            // Flush pending ordered choice
            if !choice_alts.is_empty() {
                rules.push(Rule::OrderedChoice(std::mem::take(&mut choice_alts)));
            }
            let items = parse_items(line)?;
            if !items.is_empty() {
                rules.push(Rule::Sequential(items));
            }
        }
    }

    // Flush final ordered choice
    if !choice_alts.is_empty() {
        rules.push(Rule::OrderedChoice(choice_alts));
    }

    Ok(Dialect { name: name.to_string(), rules })
}

/// Parse a line of synth items (without adjacency info).
fn parse_items(input: &str) -> Result<Vec<Item>, String> {
    Ok(parse_spaced_items(input)?.into_iter().map(|si| si.item).collect())
}

/// Parse a line of synth items WITH adjacency tracking.
fn parse_spaced_items(input: &str) -> Result<Vec<SpacedItem>, String> {
    let mut items: Vec<SpacedItem> = Vec::new();
    let mut chars = input.chars().peekable();
    let mut saw_space = true; // first item is never "adjacent"

    while let Some(&c) = chars.peek() {
        match c {
            ' ' | '\t' => { chars.next(); saw_space = true; continue; }
            _ => {}
        }
        let adjacent = !saw_space && !items.is_empty();
        saw_space = false;

        match c {
            // Cardinality prefix
            '+' | '*' | '?' => {
                let kind = match c {
                    '+' => Card::OneOrMore,
                    '*' => Card::ZeroOrMore,
                    '?' => Card::Optional,
                    _ => unreachable!(),
                };
                chars.next();
                let rest: String = chars.collect();
                let spaced = parse_spaced_items(&rest)?;
                let inner = if spaced.len() == 1 {
                    spaced.into_iter().next().unwrap().item
                } else if spaced.len() > 1 {
                    Item::Sequence(spaced)
                } else {
                    return Ok(items);
                };
                items.push(SpacedItem { item: Item::Cardinality { kind, inner: Box::new(inner) }, adjacent });
                return Ok(items);
            }

            // Dialect reference: <name>
            '<' => {
                chars.next();
                let name: String = chars.by_ref().take_while(|&c| c != '>').collect();
                items.push(SpacedItem { item: Item::DialectRef(name), adjacent });
            }

            // Literal escape: _..._  — content between _ is literal aski tokens
            '_' => {
                chars.next();
                let literal: String = chars.by_ref().take_while(|&c| c != '_').collect();
                if !literal.is_empty() {
                    let rest: String = chars.collect();
                    if rest.trim().is_empty() {
                        items.push(SpacedItem { item: Item::Literal(literal), adjacent });
                    } else {
                        let inner_items = parse_items(rest.trim())?;
                        if let Some(inner) = inner_items.into_iter().next() {
                            items.push(SpacedItem { item: Item::LiteralEscape {
                                literal,
                                inner: Box::new(inner),
                            }, adjacent });
                        }
                    }
                    return Ok(items);
                }
            }

            // Declare placeholder: @Name or @name
            '@' => {
                chars.next();
                // Check for operator capture: @+ @- @* @% @= @< @> @. @? @!
                match chars.peek() {
                    Some(&c) if "+-*%=<>.?!&|".contains(c) => {
                        let op = c.to_string();
                        chars.next();
                        items.push(SpacedItem { item: Item::Declare { casing: Casing::Camel, kind: op }, adjacent });
                    }
                    _ => {
                        let name = read_name(&mut chars);
                        if name == "value" {
                            items.push(SpacedItem { item: Item::Value, adjacent });
                        } else {
                            let casing = if name.starts_with(|c: char| c.is_uppercase()) {
                                Casing::Pascal
                            } else {
                                Casing::Camel
                            };
                            // Check for inline or: @name//@Name
                            if chars.peek() == Some(&'/') {
                                let mut peek_chars = chars.clone();
                                peek_chars.next();
                                if peek_chars.peek() == Some(&'/') {
                                    chars.next();
                                    chars.next();
                                    let left = Item::Declare { casing, kind: name };
                                    let rest: String = chars.collect();
                                    let right_items = parse_items(&rest)?;
                                    if let Some(right) = right_items.into_iter().next() {
                                        items.push(SpacedItem { item: Item::Or(vec![left, right]), adjacent });
                                    }
                                    return Ok(items);
                                }
                            }
                            items.push(SpacedItem { item: Item::Declare { casing, kind: name }, adjacent });
                        }
                    }
                }
            }

            // Reference placeholder: :Name or :name
            ':' => {
                chars.next();
                let name = read_name(&mut chars);
                let casing = if name.starts_with(|c: char| c.is_uppercase()) {
                    Casing::Pascal
                } else {
                    Casing::Camel
                };
                if chars.peek() == Some(&'/') {
                    let mut peek_chars = chars.clone();
                    peek_chars.next();
                    if peek_chars.peek() == Some(&'/') {
                        chars.next();
                        chars.next();
                        let left = Item::Reference { casing, kind: name };
                        let rest: String = chars.collect();
                        let right_items = parse_items(&rest)?;
                        if let Some(right) = right_items.into_iter().next() {
                            items.push(SpacedItem { item: Item::Or(vec![left, right]), adjacent });
                        }
                        return Ok(items);
                    }
                }
                items.push(SpacedItem { item: Item::Reference { casing, kind: name }, adjacent });
            }

            // Bare sigil literals
            '~' | '^' | '#' => {
                chars.next();
                items.push(SpacedItem { item: Item::Literal(c.to_string()), adjacent });
            }

            // Delimiter rules
            '(' => {
                chars.next();
                if chars.peek() == Some(&'|') {
                    chars.next();
                    let (key, body) = parse_delimiter_body(&mut chars, "|)")?;
                    items.push(SpacedItem { item: Item::DelimiterRule { delimiter: Delimiter::ParenPipe, key, body }, adjacent });
                } else {
                    let (key, body) = parse_delimiter_body(&mut chars, ")")?;
                    items.push(SpacedItem { item: Item::DelimiterRule { delimiter: Delimiter::Paren, key, body }, adjacent });
                }
            }
            '[' => {
                chars.next();
                if chars.peek() == Some(&'|') {
                    chars.next();
                    let (key, body) = parse_delimiter_body(&mut chars, "|]")?;
                    items.push(SpacedItem { item: Item::DelimiterRule { delimiter: Delimiter::BracketPipe, key, body }, adjacent });
                } else {
                    let (key, body) = parse_delimiter_body(&mut chars, "]")?;
                    items.push(SpacedItem { item: Item::DelimiterRule { delimiter: Delimiter::Bracket, key, body }, adjacent });
                }
            }
            '{' => {
                chars.next();
                if chars.peek() == Some(&'|') {
                    chars.next();
                    let (key, body) = parse_delimiter_body(&mut chars, "|}") ?;
                    items.push(SpacedItem { item: Item::DelimiterRule { delimiter: Delimiter::BracePipe, key, body }, adjacent });
                } else {
                    let (key, body) = parse_delimiter_body(&mut chars, "}")?;
                    items.push(SpacedItem { item: Item::DelimiterRule { delimiter: Delimiter::Brace, key, body }, adjacent });
                }
            }

            // Literal dot-prefixed: .set .new .method
            '.' => {
                chars.next();
                let name = read_name(&mut chars);
                items.push(SpacedItem { item: Item::Literal(format!(".{}", name)), adjacent });
            }

            // Literal slash-prefixed: /new
            '/' => {
                chars.next();
                if chars.peek() == Some(&'/') { break; }
                let name = read_name(&mut chars);
                if name.is_empty() {
                    items.push(SpacedItem { item: Item::Literal("/".to_string()), adjacent });
                } else {
                    items.push(SpacedItem { item: Item::Literal(format!("/{}", name)), adjacent });
                }
            }

            // Bare word — literal or keyword
            _ if c.is_alphanumeric() || c == '_' => {
                let name = read_name(&mut chars);
                items.push(SpacedItem { item: Item::Literal(name), adjacent });
            }

            _ => { chars.next(); }
        }
    }

    Ok(items)
}

/// Parse the inside of a delimiter rule. Reads until the closing delimiter.
/// Returns (key, body). Key is before /, body is after.
fn parse_delimiter_body(
    chars: &mut std::iter::Peekable<std::str::Chars>,
    close: &str,
) -> Result<(Option<Box<Item>>, Vec<Item>), String> {
    // Collect content until closing delimiter
    let mut content = String::new();
    let mut depth = 1;

    while let Some(&c) = chars.peek() {
        // Check for closing delimiter
        if close.len() == 2 {
            // Two-char close: |) |] |}
            let mut peek = chars.clone();
            if let Some(&c1) = peek.peek() {
                peek.next();
                if let Some(&c2) = peek.peek() {
                    let pair = format!("{}{}", c1, c2);
                    if pair == close && depth == 1 {
                        chars.next();
                        chars.next();
                        break;
                    }
                }
            }
        } else if close.len() == 1 {
            let close_char = close.chars().next().unwrap();
            if c == close_char && depth == 1 {
                chars.next();
                break;
            }
            // Track nesting
            match c {
                '(' | '[' | '{' => depth += 1,
                ')' | ']' | '}' => depth -= 1,
                _ => {}
            }
        }

        content.push(c);
        chars.next();
    }

    // v0.16: position defines meaning. First @Declare is the key, rest is body.
    let content = content.trim();

    if content.starts_with('@') {
        // First item is @Declare (the key), rest is body
        let all_items = parse_items(content)?;
        if all_items.is_empty() {
            Ok((None, Vec::new()))
        } else {
            let mut iter = all_items.into_iter();
            let key = iter.next().map(Box::new);
            let body: Vec<Item> = iter.collect();
            Ok((key, body))
        }
    } else {
        // No @key — bare content, no key
        let body = if content.is_empty() { Vec::new() }
            else { parse_items(content)? };
        Ok((None, body))
    }
}

/// Read a name (alphanumeric + _) from the character stream.
fn read_name(chars: &mut std::iter::Peekable<std::str::Chars>) -> String {
    let mut name = String::new();
    while let Some(&c) = chars.peek() {
        if c.is_alphanumeric() || c == '_' || c == '-' {
            name.push(c);
            chars.next();
        } else {
            break;
        }
    }
    name
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_aski_synth() {
        let source = r#"
;; aski.synth
// !{@module <module>}
// *(@Domain <domain>)
// *(@trait <trait-decl>)
// *[@trait <trait-impl>]
// *{@Struct <struct>}
// *{|@Const :Type @value|}
// *(|@Ffi <ffi>|)
// ?[|<process>|]
"#;
        let dialect = load_dialect("aski", source).unwrap();
        assert_eq!(dialect.name, "aski");
        // Pure ordered choice (8 alternatives, no sequential)
        assert_eq!(dialect.rules.len(), 1);
        match &dialect.rules[0] {
            Rule::OrderedChoice(alts) => {
                assert_eq!(alts.len(), 8);
                assert_eq!(alts[0].cardinality, Card::One);        // !{@module}
                assert_eq!(alts[1].cardinality, Card::ZeroOrMore); // *(@Domain)
                assert_eq!(alts[7].cardinality, Card::Optional);   // ?[|<process>|]
            }
            _ => panic!("expected OrderedChoice"),
        }
    }

    #[test]
    fn load_domain_synth() {
        let source = r#"
;; domain.synth
// *@Variant
// *(@Variant :Type)
// *{@Variant <struct>}
"#;
        let dialect = load_dialect("domain", source).unwrap();
        assert_eq!(dialect.rules.len(), 1);
        match &dialect.rules[0] {
            Rule::OrderedChoice(alts) => assert_eq!(alts.len(), 3),
            _ => panic!("expected OrderedChoice"),
        }
    }

    #[test]
    fn load_statement_synth() {
        let source = r#"
// _@_@name :Type /_new (<expr>)
// _@_@name /_new (<expr>)
// _@_@name :Type
// _~@_@name .set (<expr>)
// _^_<expr>
// <expr>
"#;
        let dialect = load_dialect("statement", source).unwrap();
        assert_eq!(dialect.rules.len(), 1);
        match &dialect.rules[0] {
            Rule::OrderedChoice(alts) => assert_eq!(alts.len(), 6),
            _ => panic!("expected OrderedChoice"),
        }
    }

    #[test]
    fn load_param_synth() {
        let source = r#"
// _:@_Self
// _~@_Self
// _@_Self
// _:@_@name
// _~@_@name
// _@_@name :Type
// _@_@name
"#;
        let dialect = load_dialect("param", source).unwrap();
        assert_eq!(dialect.rules.len(), 1);
        match &dialect.rules[0] {
            Rule::OrderedChoice(alts) => {
                assert_eq!(alts.len(), 7);
                // First alt: _:@_Self — literal ":@" then literal "Self"
                match &alts[0].items[0].item {
                    Item::LiteralEscape { literal, .. } => assert_eq!(literal, ":@"),
                    ref other => panic!("expected LiteralEscape, got {:?}", other),
                }
            }
            _ => panic!("expected OrderedChoice"),
        }
    }

    #[test]
    fn load_module_synth() {
        let source = r#"
+@export//@Export
*[:Module +:import//:Import]
"#;
        let dialect = load_dialect("module", source).unwrap();
        assert_eq!(dialect.rules.len(), 2);
    }
}
