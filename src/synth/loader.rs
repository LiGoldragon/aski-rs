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
            let items = parse_items(items_str)?;
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

/// Parse a line of synth items.
fn parse_items(input: &str) -> Result<Vec<Item>, String> {
    let mut items = Vec::new();
    let mut chars = input.chars().peekable();

    while let Some(&c) = chars.peek() {
        match c {
            ' ' | '\t' => { chars.next(); }

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
                let inner_items = parse_items(&rest)?;
                if let Some(inner) = inner_items.into_iter().next() {
                    items.push(Item::Cardinality { kind, inner: Box::new(inner) });
                }
                return Ok(items);
            }

            // Dialect reference: <name>
            '<' => {
                chars.next();
                let name: String = chars.by_ref().take_while(|&c| c != '>').collect();
                items.push(Item::DialectRef(name));
            }

            // Literal escape: _..._  — content between _ is literal aski tokens
            '_' => {
                chars.next();
                let literal: String = chars.by_ref().take_while(|&c| c != '_').collect();
                if !literal.is_empty() {
                    // Parse everything after closing _ as synth
                    let rest: String = chars.collect();
                    if rest.trim().is_empty() {
                        // Just a literal, nothing after
                        items.push(Item::Literal(literal));
                    } else {
                        let inner_items = parse_items(rest.trim())?;
                        if let Some(inner) = inner_items.into_iter().next() {
                            items.push(Item::LiteralEscape {
                                literal,
                                inner: Box::new(inner),
                            });
                        }
                    }
                    return Ok(items);
                }
            }

            // Declare placeholder: @Name or @name
            '@' => {
                chars.next();
                let name = read_name(&mut chars);
                // @value is special — matches literal values (int, float, string)
                if name == "value" {
                    items.push(Item::Value);
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
                            chars.next(); // skip first /
                            chars.next(); // skip second /
                            let left = Item::Declare { casing, kind: name };
                            let rest: String = chars.collect();
                            let right_items = parse_items(&rest)?;
                            if let Some(right) = right_items.into_iter().next() {
                                items.push(Item::Or(vec![left, right]));
                            }
                            return Ok(items);
                        }
                    }
                    items.push(Item::Declare { casing, kind: name });
                }
            }

            // Reference placeholder: :Name or :name
            ':' => {
                chars.next();
                // Old :_ escape removed — use _..._ instead
                let name = read_name(&mut chars);
                let casing = if name.starts_with(|c: char| c.is_uppercase()) {
                    Casing::Pascal
                } else {
                    Casing::Camel
                };
                // Check for inline or
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
                            items.push(Item::Or(vec![left, right]));
                        }
                        return Ok(items);
                    }
                }
                items.push(Item::Reference { casing, kind: name });
            }

            // Bare sigil literals (old ~_ ^_ #_ escapes removed — use _..._ instead)
            '~' | '^' | '#' => {
                chars.next();
                items.push(Item::Literal(c.to_string()));
            }

            // Delimiter rules
            '(' => {
                chars.next();
                if chars.peek() == Some(&'|') {
                    chars.next(); // (|
                    let (key, body) = parse_delimiter_body(&mut chars, "|)")?;
                    items.push(Item::DelimiterRule {
                        delimiter: Delimiter::ParenPipe,
                        key, body,
                    });
                } else {
                    let (key, body) = parse_delimiter_body(&mut chars, ")")?;
                    items.push(Item::DelimiterRule {
                        delimiter: Delimiter::Paren,
                        key, body,
                    });
                }
            }
            '[' => {
                chars.next();
                if chars.peek() == Some(&'|') {
                    chars.next(); // [|
                    let (key, body) = parse_delimiter_body(&mut chars, "|]")?;
                    items.push(Item::DelimiterRule {
                        delimiter: Delimiter::BracketPipe,
                        key, body,
                    });
                } else {
                    let (key, body) = parse_delimiter_body(&mut chars, "]")?;
                    items.push(Item::DelimiterRule {
                        delimiter: Delimiter::Bracket,
                        key, body,
                    });
                }
            }
            '{' => {
                chars.next();
                if chars.peek() == Some(&'|') {
                    chars.next(); // {|
                    let (key, body) = parse_delimiter_body(&mut chars, "|}") ?;
                    items.push(Item::DelimiterRule {
                        delimiter: Delimiter::BracePipe,
                        key, body,
                    });
                } else {
                    let (key, body) = parse_delimiter_body(&mut chars, "}")?;
                    items.push(Item::DelimiterRule {
                        delimiter: Delimiter::Brace,
                        key, body,
                    });
                }
            }

            // Literal dot-prefixed: .set .new .method
            '.' => {
                chars.next();
                let name = read_name(&mut chars);
                items.push(Item::Literal(format!(".{}", name)));
            }

            // Literal slash-prefixed: /new
            '/' => {
                chars.next();
                if chars.peek() == Some(&'/') {
                    // This shouldn't happen mid-line (// is line-leading)
                    break;
                }
                // Key separator or literal /name
                let name = read_name(&mut chars);
                if name.is_empty() {
                    // bare / — key separator (inside delimiter rules)
                    items.push(Item::Literal("/".to_string()));
                } else {
                    items.push(Item::Literal(format!("/{}", name)));
                }
            }

            // Bare word — literal or keyword
            _ if c.is_alphanumeric() || c == '_' => {
                let name = read_name(&mut chars);
                if name == "___" {
                    items.push(Item::Literal("___".to_string()));
                } else {
                    items.push(Item::Literal(name));
                }
            }

            _ => { chars.next(); } // skip unknown
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

    // Split on / to get key and body — only at depth 0 (skip / inside nested delimiters)
    let content = content.trim();
    let slash_pos = {
        let mut depth = 0;
        let mut found = None;
        for (i, ch) in content.chars().enumerate() {
            match ch {
                '(' | '[' | '{' => depth += 1,
                ')' | ']' | '}' => depth -= 1,
                '/' if depth == 0 => { found = Some(i); break; }
                _ => {}
            }
        }
        found
    };
    if let Some(slash_pos) = slash_pos {
        let key_str = content[..slash_pos].trim();
        let body_str = content[slash_pos + 1..].trim();

        let key = if key_str.is_empty() {
            None
        } else {
            let key_items = parse_items(key_str)?;
            key_items.into_iter().next().map(Box::new)
        };

        let body = if body_str.is_empty() {
            Vec::new()
        } else {
            parse_items(body_str)?
        };

        Ok((key, body))
    } else {
        // No / — bare content, no key
        let body = if content.is_empty() {
            Vec::new()
        } else {
            parse_items(content)?
        };
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
// !{@module/ <module>}
// *(@Domain/ <domain>)
// *(@trait/ <trait-decl>)
// *[@trait/ <trait-impl>]
// *{@Struct/ <struct>}
// *{|@Const/ :Type @value|}
// *(|@Ffi/ <ffi>|)
// ?[|<process>|]
"#;
        let dialect = load_dialect("aski", source).unwrap();
        assert_eq!(dialect.name, "aski");
        // Pure ordered choice (8 alternatives, no sequential)
        assert_eq!(dialect.rules.len(), 1);
        match &dialect.rules[0] {
            Rule::OrderedChoice(alts) => {
                assert_eq!(alts.len(), 8);
                assert_eq!(alts[0].cardinality, Card::One);        // !{@module/}
                assert_eq!(alts[1].cardinality, Card::ZeroOrMore); // *(@Domain/)
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
// *(@Variant/ :Type)
// *{@Variant/ <struct>}
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
                match &alts[0].items[0] {
                    Item::LiteralEscape { literal, .. } => assert_eq!(literal, ":@"),
                    other => panic!("expected LiteralEscape, got {:?}", other),
                }
            }
            _ => panic!("expected OrderedChoice"),
        }
    }

    #[test]
    fn load_module_synth() {
        let source = r#"
+@export//@Export
*[:Module/ +:import//:Import]
"#;
        let dialect = load_dialect("module", source).unwrap();
        assert_eq!(dialect.rules.len(), 2);
    }
}
