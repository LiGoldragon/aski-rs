//! ParseItem trait — parsing one synth item against the token stream.

use crate::lexer::Token;
use crate::synth::types::*;
use super::aski_world::AskiWorld;
use super::tokens::TokenReader;
use super::register::Register;
use super::parse_dialect::ParseDialect;

pub trait ParseItem {
    fn parse_item(&mut self, reader: &mut TokenReader, parent_id: i64, item: &Item) -> Result<(), String>;
}

impl ParseItem for AskiWorld {
    fn parse_item(&mut self, reader: &mut TokenReader, parent_id: i64, item: &Item) -> Result<(), String> {
        reader.skip_newlines();

        match item {
            Item::Cardinality { kind, inner } => {
                self.parse_cardinality(reader, parent_id, *kind, inner)
            }

            Item::DelimiterRule { delimiter, key, body } => {
                self.parse_delimiter_rule(reader, parent_id, *delimiter, key.as_deref(), body)
            }

            Item::Declare { casing, kind } => {
                self.parse_declare(reader, parent_id, *casing, kind)
            }

            Item::Reference { casing, kind } => {
                self.parse_reference(reader, parent_id, *casing, kind)
            }

            Item::Value => {
                reader.skip_newlines();
                let tok = reader.peek().ok_or("expected value, got EOF")?.clone();
                let start = reader.span_start();
                match tok {
                    Token::Integer(n) => {
                        reader.pos += 1;
                        let id = self.make_node("IntLit", &n.to_string(), start, reader.span_end());
                        self.add_child(parent_id, id);
                        Ok(())
                    }
                    Token::Float(ref s) => {
                        let s = s.clone();
                        reader.pos += 1;
                        let id = self.make_node("FloatLit", &s, start, reader.span_end());
                        self.add_child(parent_id, id);
                        Ok(())
                    }
                    Token::StringLit(ref s) => {
                        let s = s.clone();
                        reader.pos += 1;
                        let id = self.make_node("StringLit", &s, start, reader.span_end());
                        self.add_child(parent_id, id);
                        Ok(())
                    }
                    _ => Err(format!("expected value, got {:?}", tok)),
                }
            }

            Item::DialectRef(dialect_name) => {
                self.push_dialect(dialect_name);
                self.parse_dialect_rules(reader, parent_id)?;
                self.pop_dialect();
                Ok(())
            }

            Item::LiteralEscape { literal, inner } => {
                // Match each character in the literal as a separate token
                for ch in literal.chars() {
                    reader.expect_literal(&ch.to_string())?;
                }
                self.parse_item(reader, parent_id, inner)
            }

            Item::Or(alternatives) => {
                for alt in alternatives {
                    let snap = self.snapshot();
                    let saved = reader.pos;
                    if self.parse_item(reader, parent_id, alt).is_ok() {
                        return Ok(());
                    }
                    self.restore(&snap);
                    reader.pos = saved;
                }
                Err(format!("no or-alternative matched at pos {}", reader.pos))
            }

            Item::Sequence(items) => {
                for sub_item in items {
                    self.parse_item(reader, parent_id, sub_item)?;
                }
                Ok(())
            }

            Item::Literal(text) => {
                reader.expect_literal(text)
            }
        }
    }
}

// ── Private methods on AskiWorld for each item kind ──

impl AskiWorld {
    fn parse_cardinality(&mut self, reader: &mut TokenReader, parent_id: i64, kind: Card, inner: &Item) -> Result<(), String> {
        match kind {
            Card::One => self.parse_item(reader, parent_id, inner),
            Card::Optional => {
                let snap = self.snapshot();
                let saved = reader.pos;
                if self.parse_item(reader, parent_id, inner).is_err() {
                    self.restore(&snap);
                    reader.pos = saved;
                }
                Ok(())
            }
            Card::ZeroOrMore => {
                loop {
                    reader.skip_newlines();
                    if reader.at_end() { break; }
                    let saved_pos = reader.pos;
                    let snap = self.snapshot();
                    match self.parse_item(reader, parent_id, inner) {
                        Ok(()) => {
                            if reader.pos <= saved_pos { break; }
                        }
                        Err(_) => {
                            self.restore(&snap);
                            reader.pos = saved_pos;
                            break;
                        }
                    }
                }
                Ok(())
            }
            Card::OneOrMore => {
                self.parse_item(reader, parent_id, inner)?;
                loop {
                    reader.skip_newlines();
                    if reader.at_end() { break; }
                    let saved_pos = reader.pos;
                    let snap = self.snapshot();
                    match self.parse_item(reader, parent_id, inner) {
                        Ok(()) => {
                            if reader.pos <= saved_pos { break; }
                        }
                        Err(_) => {
                            self.restore(&snap);
                            reader.pos = saved_pos;
                            break;
                        }
                    }
                }
                Ok(())
            }
        }
    }

    fn parse_delimiter_rule(
        &mut self,
        reader: &mut TokenReader,
        parent_id: i64,
        delimiter: Delimiter,
        key: Option<&Item>,
        body: &[Item],
    ) -> Result<(), String> {
        reader.skip_newlines();
        reader.expect_open(delimiter)?;
        let start = reader.span_start();

        let key_text = if let Some(key_item) = key {
            // Check casing matches the synth rule
            let text = match key_item {
                Item::Declare { casing: Casing::Pascal, .. } => reader.read_pascal()?,
                Item::Declare { casing: Casing::Camel, .. } => reader.read_camel()?,
                _ => reader.read_key()?,
            };
            reader.expect_slash()?;
            text
        } else {
            String::new()
        };

        let constructor = delimiter.open_str();
        let end = reader.span_end();
        let node_id = self.make_node(constructor, &key_text, start, end);
        self.add_child(parent_id, node_id);

        if let Some(key_item) = key {
            self.register_from_key(key_item, &key_text, delimiter);
        }

        for body_item in body {
            match body_item {
                Item::DialectRef(dialect_name) => {
                    self.push_dialect(dialect_name);
                    self.parse_dialect_until_close(reader, node_id, delimiter)?;
                    self.pop_dialect();
                }
                _ => {
                    self.parse_item(reader, node_id, body_item)?;
                }
            }
        }

        if body.is_empty() {
            // Check if this is a construct with expression content
            let key_kind = key.map(|k| match k {
                Item::Declare { kind, .. } => kind.as_str(),
                _ => "",
            }).unwrap_or("");

            match key_kind {
                "method" | "signature" | "foreignFunction" => {
                    use super::parse_expr::ParseExpr;
                    self.parse_method_content(reader, node_id)?;
                }
                // "Const" handled by synth body items (:Type @value)
                _ => {
                    reader.skip_until_close(delimiter);
                }
            }
        }

        reader.skip_newlines();
        reader.expect_close(delimiter)?;
        Ok(())
    }

    fn parse_declare(&mut self, reader: &mut TokenReader, parent_id: i64, casing: Casing, kind: &str) -> Result<(), String> {
        reader.skip_newlines();
        let name = match casing {
            Casing::Pascal => reader.read_pascal()?,
            Casing::Camel => reader.read_camel()?,
        };
        let start = reader.span_start();
        let end = reader.span_end();
        let node_id = self.make_node(kind, &name, start, end);
        self.add_child(parent_id, node_id);

        // Register the declared name based on its kind
        self.register_declared_name(kind, &name);

        Ok(())
    }

    fn parse_reference(&mut self, reader: &mut TokenReader, parent_id: i64, casing: Casing, kind: &str) -> Result<(), String> {
        reader.skip_newlines();
        let name = match casing {
            Casing::Pascal => reader.read_type()?,  // handles Vec{T}, $Trait&Trait
            Casing::Camel => reader.read_camel()?,
        };
        let start = reader.span_start();
        let end = reader.span_end();
        let node_id = self.make_node(kind, &name, start, end);
        self.add_child(parent_id, node_id);
        Ok(())
    }

    /// Register a name from a bare @Declare item (not inside a delimiter key).
    fn register_declared_name(&mut self, kind: &str, name: &str) {
        match kind {
            "Variant" => {
                // Parent is the most recent domain
                if let Some(parent) = self.known_types.iter().rev()
                    .find(|t| t.form == super::aski_world::TypeForm::Domain)
                {
                    let parent_name = parent.name.clone();
                    self.register_variant(name, &parent_name);
                }
            }
            "Domain" => self.register_domain(name),
            "Struct" => self.register_struct(name),
            "trait" => self.register_trait(name),
            "method" | "foreignFunction" => self.register_method(name),
            "export" | "Export" => self.exports.push(name.to_string()),
            _ => {}
        }
    }
}
