//! Expression and statement parsing.
//!
//! This is engine-level parsing, not synth-driven.
//! Sigil-level syntax (@ : ~ ^ # .) is too token-granular for synth.
//! AskiWorld IS the parser here — reading tokens directly.

use crate::lexer::Token;
use super::aski_world::AskiWorld;
use super::tokens::TokenReader;

pub trait ParseExpr {
    /// Parse method content after key/: params, optional return type, body.
    fn parse_method_content(&mut self, reader: &mut TokenReader, method_node_id: i64) -> Result<(), String>;

    /// Parse a block body [stmts] — already inside the brackets.
    fn parse_body(&mut self, reader: &mut TokenReader, parent_id: i64) -> Result<(), String>;

    /// Parse a single expression (Pratt-style).
    fn parse_expr(&mut self, reader: &mut TokenReader, parent_id: i64) -> Result<i64, String>;

    /// Parse a single atom (leaf expression).
    fn parse_atom(&mut self, reader: &mut TokenReader, parent_id: i64) -> Result<i64, String>;

    /// Parse params until a body delimiter or type name is hit.
    fn parse_params(&mut self, reader: &mut TokenReader, parent_id: i64) -> Result<(), String>;
}

impl ParseExpr for AskiWorld {
    fn parse_method_content(&mut self, reader: &mut TokenReader, method_node_id: i64) -> Result<(), String> {
        // 1. Parse params
        self.parse_params(reader, method_node_id)?;

        // 2. Optional return type (PascalCase, Vec{T}, $Trait&Trait)
        // Accept if followed by body delimiter, close delimiter, or EOF
        reader.skip_newlines();
        let is_type_start = matches!(reader.peek(), Some(Token::PascalIdent(_)) | Some(Token::Dollar));
        if is_type_start {
            let saved = reader.pos;
            let type_name = reader.read_type()?;
            reader.skip_newlines();
            match reader.peek() {
                Some(Token::LBracket) | Some(Token::LBracketPipe) | Some(Token::LParenPipe)
                | Some(Token::RParen) | Some(Token::RPipeParen)
                | None => {
                    let id = self.make_node("ReturnType", &type_name, 0, 0);
                    self.add_child(method_node_id, id);
                }
                _ => {
                    reader.pos = saved;
                }
            }
        }

        // 3. Body
        reader.skip_newlines();
        if let Some(tok) = reader.peek() {
            match tok {
                Token::LBracket => {
                    reader.pos += 1;
                    let body_id = self.make_node("Block", "", 0, 0);
                    self.add_child(method_node_id, body_id);
                    self.parse_body(reader, body_id)?;
                    reader.expect_close(crate::synth::types::Delimiter::Bracket)?;
                }
                Token::LParenPipe => {
                    reader.pos += 1;
                    let body_id = self.make_node("MatchBody", "", 0, 0);
                    self.add_child(method_node_id, body_id);
                    self.parse_match_body(reader, body_id)?;
                    reader.expect_close(crate::synth::types::Delimiter::ParenPipe)?;
                }
                Token::LBracketPipe => {
                    reader.pos += 1;
                    let body_id = self.make_node("TailBlock", "", 0, 0);
                    self.add_child(method_node_id, body_id);
                    self.parse_body(reader, body_id)?;
                    reader.expect_close(crate::synth::types::Delimiter::BracketPipe)?;
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn parse_body(&mut self, reader: &mut TokenReader, parent_id: i64) -> Result<(), String> {
        loop {
            reader.skip_newlines();
            if reader.at_end() { break; }
            match reader.peek() {
                Some(Token::RBracket) | Some(Token::RPipeBracket) => break,
                _ => {}
            }
            let saved = reader.pos;
            match self.parse_statement(reader, parent_id) {
                Ok(_) => {
                    if reader.pos <= saved { reader.pos += 1; } // progress guard
                }
                Err(_) => break,
            }
        }
        Ok(())
    }

    fn parse_expr(&mut self, reader: &mut TokenReader, parent_id: i64) -> Result<i64, String> {
        let lhs = self.parse_atom(reader, parent_id)?;
        self.parse_infix(reader, parent_id, lhs, 0)
    }

    fn parse_atom(&mut self, reader: &mut TokenReader, _parent_id: i64) -> Result<i64, String> {
        reader.skip_newlines();
        let tok = reader.peek().ok_or("expected expression, got EOF")?;
        let start = reader.span_start();

        match tok.clone() {
            Token::PascalIdent(name) => {
                reader.pos += 1;
                // Check for Type:Variant qualified path
                if reader.peek() == Some(&Token::Colon) {
                    reader.pos += 1;
                    let variant = reader.read_name()?;
                    let id = self.make_node("TypePath", &format!("{}:{}", name, variant), start, reader.span_end());
                    Ok(id)
                } else if self.is_variant(&name) {
                    let _parent_domain = self.variant_of(&name).unwrap_or("").to_string();
                    let id = self.make_node("QualifiedVariant", &name, start, reader.span_end());
                    Ok(id)
                } else {
                    let id = self.make_node("BareName", &name, start, reader.span_end());
                    Ok(id)
                }
            }
            Token::CamelIdent(name) => {
                reader.pos += 1;
                let id = self.make_node("BareName", &name, start, reader.span_end());
                Ok(id)
            }
            Token::Integer(n) => {
                reader.pos += 1;
                let id = self.make_node("IntLit", &n.to_string(), start, reader.span_end());
                Ok(id)
            }
            Token::Float(s) => {
                reader.pos += 1;
                let id = self.make_node("FloatLit", &s, start, reader.span_end());
                Ok(id)
            }
            Token::StringLit(s) => {
                reader.pos += 1;
                let id = self.make_node("StringLit", &s, start, reader.span_end());
                Ok(id)
            }
            Token::At => {
                reader.pos += 1;
                let name = reader.read_name()?;
                // @Name.field or @Name
                let id = self.make_node("InstanceRef", &name, start, reader.span_end());
                Ok(id)
            }
            Token::Caret => {
                reader.pos += 1;
                let inner = self.parse_expr(reader, _parent_id)?;
                let id = self.make_node("Return", "", start, reader.span_end());
                self.add_child(id, inner);
                Ok(id)
            }
            Token::LParen => {
                reader.pos += 1;
                let inner = self.parse_expr(reader, _parent_id)?;
                reader.expect_close(crate::synth::types::Delimiter::Paren)?;
                let id = self.make_node("Group", "", start, reader.span_end());
                self.add_child(id, inner);
                Ok(id)
            }
            Token::LBracket => {
                reader.pos += 1;
                let id = self.make_node("InlineEval", "", start, 0);
                self.parse_body(reader, id)?;
                reader.expect_close(crate::synth::types::Delimiter::Bracket)?;
                Ok(id)
            }
            _ => Err(format!("unexpected token in expression: {:?}", tok)),
        }
    }

    fn parse_params(&mut self, reader: &mut TokenReader, parent_id: i64) -> Result<(), String> {
        loop {
            reader.skip_newlines();
            if reader.at_end() { break; }
            match reader.peek() {
                // Body delimiters end params
                Some(Token::LBracket) | Some(Token::LBracketPipe) | Some(Token::LParenPipe) => break,
                // Close paren ends params (we're inside a method ())
                Some(Token::RParen) | Some(Token::RPipeParen) => break,
                // Sigil params
                Some(Token::Colon) => {
                    reader.pos += 1;
                    reader.skip_newlines();
                    if reader.peek() == Some(&Token::At) {
                        reader.pos += 1;
                        let name = reader.read_name()?;
                        let id = self.make_node("BorrowParam", &name, 0, 0);
                        self.add_child(parent_id, id);
                    }
                }
                Some(Token::Tilde) => {
                    reader.pos += 1;
                    reader.skip_newlines();
                    if reader.peek() == Some(&Token::At) {
                        reader.pos += 1;
                        let name = reader.read_name()?;
                        let id = self.make_node("MutBorrowParam", &name, 0, 0);
                        self.add_child(parent_id, id);
                    }
                }
                Some(Token::At) => {
                    reader.pos += 1;
                    let name = reader.read_name()?;
                    // Check for explicit type (PascalIdent, Vec{T}, $Trait)
                    reader.skip_newlines();
                    match reader.peek() {
                        Some(Token::PascalIdent(_)) | Some(Token::Dollar) => {
                            let type_name = reader.read_type()?;
                            let id = self.make_node("NamedParam", &name, 0, 0);
                            let type_id = self.make_node("TypeRef", &type_name, 0, 0);
                            self.add_child(id, type_id);
                            self.add_child(parent_id, id);
                        }
                        _ => {
                            let id = self.make_node("OwnedParam", &name, 0, 0);
                            self.add_child(parent_id, id);
                        }
                    }
                }
                // PascalCase = return type (stop parsing params)
                Some(Token::PascalIdent(_)) => break,
                _ => break,
            }
        }
        Ok(())
    }
}

impl AskiWorld {
    fn parse_statement(&mut self, reader: &mut TokenReader, parent_id: i64) -> Result<(), String> {
        reader.skip_newlines();
        let start = reader.span_start();
        match reader.peek() {
            Some(Token::Caret) => {
                // Return statement: ^expr
                let expr_id = self.parse_expr(reader, parent_id)?;
                self.add_child(parent_id, expr_id);
                Ok(())
            }
            Some(Token::Hash) => {
                reader.pos += 1;
                reader.skip_newlines();
                if reader.peek() == Some(&Token::LBracket) {
                    // Bare loop: #[body] — loop until ^ break
                    reader.pos += 1;
                    let loop_id = self.make_node("Loop", "", start, 0);
                    let body_id = self.make_node("Block", "", 0, 0);
                    self.add_child(loop_id, body_id);
                    self.parse_body(reader, body_id)?;
                    reader.expect_close(crate::synth::types::Delimiter::Bracket)?;
                    self.add_child(parent_id, loop_id);
                } else {
                    // Iteration: #source [body]
                    let source_id = self.parse_expr(reader, parent_id)?;
                    let iter_id = self.make_node("Iteration", "", start, 0);
                    self.add_child(iter_id, source_id);
                    reader.skip_newlines();
                    if reader.peek() == Some(&Token::LBracket) {
                        reader.pos += 1;
                        let body_id = self.make_node("Block", "", 0, 0);
                        self.add_child(iter_id, body_id);
                        self.parse_body(reader, body_id)?;
                        reader.expect_close(crate::synth::types::Delimiter::Bracket)?;
                    }
                    self.add_child(parent_id, iter_id);
                }
                Ok(())
            }
            Some(Token::Tilde) => {
                // Mutable: ~@name ... (allocation or mutation)
                reader.pos += 1;
                reader.skip_newlines();
                if reader.peek() == Some(&Token::At) {
                    reader.pos += 1;
                    let name = reader.read_name()?;
                    reader.skip_newlines();
                    match reader.peek() {
                        Some(Token::Dot) => {
                            // ~@name.method (args) — mutation
                            reader.pos += 1;
                            let method = reader.read_name()?;
                            let mut_id = self.make_node("MutCall", &name, start, 0);
                            let method_id = self.make_node("MethodName", &method, 0, 0);
                            self.add_child(mut_id, method_id);
                            reader.skip_newlines();
                            // Parse args: [expr] or (expr)
                            if reader.peek() == Some(&Token::LBracket) {
                                reader.pos += 1;
                                let arg = self.parse_expr(reader, mut_id)?;
                                self.add_child(mut_id, arg);
                                reader.expect_close(crate::synth::types::Delimiter::Bracket)?;
                            } else if reader.peek() == Some(&Token::LParen) {
                                reader.pos += 1;
                                loop {
                                    reader.skip_newlines();
                                    if reader.peek() == Some(&Token::RParen) { reader.pos += 1; break; }
                                    if reader.at_end() { break; }
                                    let arg = self.parse_expr(reader, mut_id)?;
                                    self.add_child(mut_id, arg);
                                }
                            }
                            self.add_child(parent_id, mut_id);
                        }
                        _ => {
                            // ~@name (Type/ init) or ~@name :Type — mutable allocation
                            let alloc_id = self.make_node("MutAlloc", &name, start, 0);
                            self.parse_alloc_rest(reader, alloc_id)?;
                            self.add_child(parent_id, alloc_id);
                        }
                    }
                    Ok(())
                } else {
                    // Fallback: expression
                    let expr_id = self.parse_expr(reader, parent_id)?;
                    self.add_child(parent_id, expr_id);
                    Ok(())
                }
            }
            Some(Token::At) => {
                // @name ... — allocation or instance ref in expression
                let saved = reader.pos;
                reader.pos += 1;
                let name = reader.read_name()?;
                reader.skip_newlines();
                match reader.peek() {
                    Some(Token::LParen) | Some(Token::Colon) => {
                        // @name (Type/ init) or @name :Type — allocation
                        let alloc_id = self.make_node("Alloc", &name, start, 0);
                        self.parse_alloc_rest(reader, alloc_id)?;
                        self.add_child(parent_id, alloc_id);
                        Ok(())
                    }
                    _ => {
                        // Not an allocation — rewind and parse as expression
                        reader.pos = saved;
                        let expr_id = self.parse_expr(reader, parent_id)?;
                        self.add_child(parent_id, expr_id);
                        Ok(())
                    }
                }
            }
            _ => {
                let expr_id = self.parse_expr(reader, parent_id)?;
                self.add_child(parent_id, expr_id);
                Ok(())
            }
        }
    }

    fn parse_alloc_rest(&mut self, reader: &mut TokenReader, alloc_id: i64) -> Result<(), String> {
        reader.skip_newlines();
        match reader.peek() {
            Some(Token::Colon) => {
                // :Type — sub-type declaration
                reader.pos += 1;
                reader.skip_newlines();
                let type_name = reader.read_type()?;
                let type_id = self.make_node("TypeRef", &type_name, 0, 0);
                self.add_child(alloc_id, type_id);
            }
            Some(Token::LParen) => {
                // (Type/ args) — constructor
                reader.pos += 1;
                let constructor_id = self.parse_expr(reader, alloc_id)?;
                self.add_child(alloc_id, constructor_id);
                // If it was a Group, the close is already consumed
                // If not, we need to handle it
            }
            _ => {}
        }
        Ok(())
    }

    fn parse_infix(&mut self, reader: &mut TokenReader, _parent_id: i64, lhs: i64, min_bp: u8) -> Result<i64, String> {
        let mut lhs = lhs;
        loop {
            reader.skip_newlines();
            let (op, bp) = match reader.peek() {
                Some(Token::Plus) => ("+", (3, 4)),
                Some(Token::Minus) => ("-", (3, 4)),
                Some(Token::Star) => ("*", (5, 6)),
                Some(Token::Percent) => ("%", (5, 6)),
                Some(Token::DoubleEquals) => ("==", (1, 2)),
                Some(Token::NotEqual) => ("!=", (1, 2)),
                Some(Token::LessThan) => ("<", (1, 2)),
                Some(Token::GreaterThan) => (">", (1, 2)),
                Some(Token::LessThanOrEqual) => ("<=", (1, 2)),
                Some(Token::GreaterThanOrEqual) => (">=", (1, 2)),
                Some(Token::LogicalAnd) => ("&&", (0, 1)),
                Some(Token::LogicalOr) => ("||", (0, 1)),
                Some(Token::Question) => {
                    // Postfix: error propagation (try unwrap)
                    reader.pos += 1;
                    let try_id = self.make_node("TryUnwrap", "", 0, 0);
                    self.add_child(try_id, lhs);
                    lhs = try_id;
                    continue;
                }
                Some(Token::Dot) => {
                    // Postfix: field access or method call
                    reader.pos += 1;
                    let name = reader.read_name()?;
                    // Check for (args)
                    reader.skip_newlines();
                    if reader.peek() == Some(&Token::LParen) {
                        reader.pos += 1;
                        let call_id = self.make_node("MethodCall", &name, 0, 0);
                        self.add_child(call_id, lhs);
                        // Parse args until )
                        loop {
                            reader.skip_newlines();
                            if reader.peek() == Some(&Token::RParen) { reader.pos += 1; break; }
                            if reader.at_end() { break; }
                            let arg = self.parse_expr(reader, call_id)?;
                            self.add_child(call_id, arg);
                        }
                        lhs = call_id;
                        continue;
                    } else {
                        let access_id = self.make_node("FieldAccess", &name, 0, 0);
                        self.add_child(access_id, lhs);
                        lhs = access_id;
                        continue;
                    }
                }
                _ => break,
            };

            let (left_bp, right_bp) = bp;
            if left_bp < min_bp { break; }
            reader.pos += 1;

            let rhs = self.parse_atom(reader, _parent_id)?;
            let rhs = self.parse_infix(reader, _parent_id, rhs, right_bp)?;

            let bin_id = self.make_node("BinOp", op, 0, 0);
            self.add_child(bin_id, lhs);
            self.add_child(bin_id, rhs);
            lhs = bin_id;
        }
        Ok(lhs)
    }

    fn parse_match_body(&mut self, reader: &mut TokenReader, parent_id: i64) -> Result<(), String> {
        // Check for target expression: (| target/ arms |)
        // If the first token is NOT '(' (pattern start), parse as target expr until '/'
        reader.skip_newlines();
        if reader.peek() != Some(&Token::LParen) && reader.peek() != Some(&Token::RPipeParen) {
            // Parse target expression
            let target = self.parse_expr(reader, parent_id)?;
            reader.skip_newlines();
            if reader.peek() == Some(&Token::Slash) {
                reader.pos += 1; // consume /
                let target_id = self.make_node("MatchTarget", "", 0, 0);
                self.add_child(target_id, target);
                self.add_child(parent_id, target_id);
            }
        }

        // Parse arms: (pattern) result
        loop {
            reader.skip_newlines();
            if reader.at_end() { break; }
            if reader.peek() == Some(&Token::RPipeParen) { break; }

            if reader.peek() == Some(&Token::LParen) {
                reader.pos += 1;
                let arm_id = self.make_node("CommitArm", "", 0, 0);
                let pattern_id = self.make_node("Pattern", "", 0, 0);
                self.add_child(arm_id, pattern_id);
                // Parse patterns until ) — handle |, string literals, @binding
                loop {
                    reader.skip_newlines();
                    if reader.peek() == Some(&Token::RParen) { reader.pos += 1; break; }
                    if reader.at_end() { break; }
                    if reader.peek() == Some(&Token::Pipe) {
                        reader.pos += 1;
                        continue;
                    }
                    let pat = self.parse_atom(reader, pattern_id)?;
                    // Check for @binding after variant: (PascalIdent @name)
                    reader.skip_newlines();
                    if reader.peek() == Some(&Token::At) {
                        reader.pos += 1;
                        let bind_name = reader.read_name()?;
                        let bind_id = self.make_node("PatternBind", &bind_name, 0, 0);
                        self.add_child(pat, bind_id);
                    }
                    self.add_child(pattern_id, pat);
                }
                // Parse result expression
                let result = self.parse_expr(reader, arm_id)?;
                self.add_child(arm_id, result);
                self.add_child(parent_id, arm_id);
            } else {
                break;
            }
        }
        Ok(())
    }
}
