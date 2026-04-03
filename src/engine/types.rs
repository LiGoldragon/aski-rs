//! Type reference and parameter parsers.

use crate::ast::*;
use crate::lexer::Token;
use super::state::ParseState;

// ── Type reference parser ───────────────────────────────────────────

pub(crate) fn parse_type_ref(st: &mut ParseState) -> Result<TypeRef, String> {
    // Self type
    if let Some(Token::PascalIdent(s)) = st.peek() {
        if s == "Self" {
            st.advance();
            return Ok(TypeRef::SelfType);
        }
    }

    // Trait bound: {|name&name...|}
    if st.peek() == Some(&Token::TraitBoundOpen) {
        st.advance();
        let (first, _) = st.eat_camel().ok_or("expected camel in trait bound")?;
        let mut bounds = vec![first];
        while st.eat(&Token::Ampersand) {
            let (b, _) = st.eat_camel().ok_or("expected camel after &")?;
            bounds.push(b);
        }
        st.expect(&Token::TraitBoundClose)?;
        let name = bounds.join("&");
        return Ok(TypeRef::Bound(TraitBound {
            name,
            bounds,
            span: 0..0,
        }));
    }

    // Named or parameterized
    if let Some((name, _)) = st.eat_pascal() {
        // Check for { params } — must backtrack if inner parse fails
        if st.peek() == Some(&Token::LBrace) {
            let save = st.save();
            st.advance();
            let mut params = Vec::new();
            let mut ok = true;
            loop {
                st.skip_newlines();
                if st.peek() == Some(&Token::RBrace) {
                    st.advance();
                    break;
                }
                match parse_type_ref_inner(st) {
                    Ok(tr) => params.push(tr),
                    Err(_) => {
                        ok = false;
                        break;
                    }
                }
            }
            if ok && !params.is_empty() {
                return Ok(TypeRef::Parameterized(name, params));
            }
            st.restore(save);
        }
        if name == "Self" {
            return Ok(TypeRef::SelfType);
        }
        return Ok(TypeRef::Named(name));
    }

    Err(format!("expected type reference, got {:?}", st.peek()))
}

fn parse_type_ref_inner(st: &mut ParseState) -> Result<TypeRef, String> {
    // Handles recursion for Parameterized inner types
    if let Some(Token::PascalIdent(s)) = st.peek() {
        if s == "Self" {
            st.advance();
            return Ok(TypeRef::SelfType);
        }
    }

    if st.peek() == Some(&Token::TraitBoundOpen) {
        st.advance();
        let (first, _) = st.eat_camel().ok_or("expected camel in trait bound")?;
        let mut bounds = vec![first];
        while st.eat(&Token::Ampersand) {
            let (b, _) = st.eat_camel().ok_or("expected camel after &")?;
            bounds.push(b);
        }
        st.expect(&Token::TraitBoundClose)?;
        let name = bounds.join("&");
        return Ok(TypeRef::Bound(TraitBound {
            name,
            bounds,
            span: 0..0,
        }));
    }

    if let Some((name, _)) = st.eat_pascal() {
        if st.peek() == Some(&Token::LBrace) {
            st.advance();
            let mut params = Vec::new();
            loop {
                st.skip_newlines();
                if st.peek() == Some(&Token::RBrace) {
                    st.advance();
                    break;
                }
                params.push(parse_type_ref_inner(st)?);
            }
            return Ok(TypeRef::Parameterized(name, params));
        }
        if name == "Self" {
            return Ok(TypeRef::SelfType);
        }
        return Ok(TypeRef::Named(name));
    }

    Err(format!("expected type ref inner, got {:?}", st.peek()))
}

pub(crate) fn parse_field_type_ref(st: &mut ParseState) -> Result<TypeRef, String> {
    // :Type = Borrowed
    if st.peek() == Some(&Token::Colon) {
        st.advance();
        let inner = parse_type_ref(st)?;
        return Ok(TypeRef::Borrowed(Box::new(inner)));
    }
    parse_type_ref(st)
}

// ── Parameter parser ────────────────────────────────────────────────

pub(crate) fn parse_param(st: &mut ParseState) -> Result<Param, String> {
    // :@Self
    if st.peek() == Some(&Token::Colon) {
        let save = st.save();
        st.advance();
        if st.eat(&Token::At) {
            if let Some((name, _)) = st.eat_pascal() {
                if name == "Self" {
                    return Ok(Param::BorrowSelf);
                }
                return Ok(Param::Borrow(name));
            }
        }
        st.restore(save);
    }

    // ~@Self or ~@Type
    if st.peek() == Some(&Token::Tilde) {
        let save = st.save();
        st.advance();
        if st.eat(&Token::At) {
            if let Some((name, _)) = st.eat_pascal() {
                if name == "Self" {
                    return Ok(Param::MutBorrowSelf);
                }
                return Ok(Param::MutBorrow(name));
            }
        }
        st.restore(save);
    }

    // @Self (owned) or @Name Type (named) or @Type (owned)
    if st.peek() == Some(&Token::At) {
        st.advance();
        let (name, _) = st.eat_pascal().ok_or("expected Pascal after @")?;
        if name == "Self" {
            return Ok(Param::OwnedSelf);
        }
        // Try to parse a type ref — if it works, it's Named
        let save = st.save();
        if let Ok(tr) = parse_type_ref(st) {
            return Ok(Param::Named(name, tr));
        }
        st.restore(save);
        return Ok(Param::Owned(name));
    }

    Err(format!("expected param, got {:?}", st.peek()))
}
