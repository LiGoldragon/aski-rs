//! Pattern parsing: match patterns, simple patterns, or-patterns, destructure.

use crate::ast::*;
use crate::lexer::Token;
use super::state::ParseState;

pub(crate) fn parse_simple_pattern(st: &mut ParseState) -> Result<Pattern, String> {
    if st.eat(&Token::Underscore) {
        return Ok(Pattern::Wildcard);
    }
    if let Some(Token::PascalIdent(s)) = st.peek() {
        let name = s.clone();
        match name.as_str() {
            "True" => {
                st.advance();
                return Ok(Pattern::BoolLit(true));
            }
            "False" => {
                st.advance();
                return Ok(Pattern::BoolLit(false));
            }
            _ => {
                st.advance();
                return Ok(Pattern::Variant(name));
            }
        }
    }
    Err(format!("expected pattern, got {:?}", st.peek()))
}

// ── Pattern parser (full, for matching bodies) ──────────────────────

pub(crate) fn parse_match_pattern(st: &mut ParseState) -> Result<Pattern, String> {
    if st.eat(&Token::Underscore) {
        return Ok(Pattern::Wildcard);
    }

    // String literal pattern
    if let Some(Token::StringLit(s)) = st.peek() {
        let s = s.clone();
        st.advance();
        return Ok(Pattern::StringLit(s));
    }

    // Instance bind: @Name
    if st.peek() == Some(&Token::At) {
        st.advance();
        let (name, _) = st.eat_pascal().ok_or("expected Pascal after @ in pattern")?;
        return Ok(Pattern::InstanceBind(name));
    }

    // Paren patterns: or-pattern, destructure, data-carrying
    if let Some(Token::PascalIdent(_)) = st.peek() {
        let (name, _) = st.eat_pascal().unwrap();

        // Data-carrying: Name(inner_pattern)
        if st.peek() == Some(&Token::LParen) {
            st.advance();
            let inner = parse_match_pattern(st)?;
            st.expect(&Token::RParen)?;
            return Ok(Pattern::DataCarrying(name, Box::new(inner)));
        }

        // Bool literal
        match name.as_str() {
            "True" => return Ok(Pattern::BoolLit(true)),
            "False" => return Ok(Pattern::BoolLit(false)),
            _ => {}
        }

        // Simple variant — but check if the whole thing is wrapped in a paren at a higher level
        return Ok(Pattern::Variant(name));
    }

    // Parens: could be or-pattern `(Fire | Air)`, destructure `(@Head | @Rest)`, etc.
    if st.peek() == Some(&Token::LParen) {
        st.advance();
        st.skip_newlines();

        // Try destructure: @Head | @Rest
        if st.peek() == Some(&Token::At) {
            let save = st.save();
            st.advance();
            if let Some((head_name, _)) = st.eat_pascal() {
                st.skip_newlines();
                if st.eat(&Token::Pipe) {
                    st.skip_newlines();
                    if st.eat(&Token::At) {
                        if let Some((tail_name, _)) = st.eat_pascal() {
                            st.expect(&Token::RParen)?;
                            return Ok(Pattern::Destructure {
                                head: vec![Pattern::InstanceBind(head_name)],
                                tail: Some(tail_name),
                            });
                        }
                    }
                }
            }
            st.restore(save);
            st.advance(); // re-consume the (
            st.skip_newlines();
        }

        // Try or-pattern: Name | Name ...
        if let Some(Token::PascalIdent(_)) = st.peek() {
            let save = st.save();
            let (first, _) = st.eat_pascal().unwrap();

            if st.peek() == Some(&Token::Pipe) {
                let mut variants = vec![Pattern::Variant(first)];
                while st.eat(&Token::Pipe) {
                    st.skip_newlines();
                    let (vname, _) = st.eat_pascal().ok_or("expected variant after |")?;
                    variants.push(Pattern::Variant(vname));
                }
                st.expect(&Token::RParen)?;
                return Ok(Pattern::Or(variants));
            }

            st.restore(save);
        }

        // Fallback: general pattern in parens — just parse inner
        let inner = parse_match_pattern(st)?;
        st.expect(&Token::RParen)?;
        return Ok(inner);
    }

    Err(format!("expected pattern, got {:?}", st.peek()))
}
