//! Statement parsing: bindings, returns, bodies, matching bodies.

use crate::ast::*;
use crate::lexer::Token;
use super::state::ParseState;
use super::types::parse_type_ref;
use super::expr::parse_expr;
use super::pattern::parse_match_pattern;

// ── Statement parser ────────────────────────────────────────────────

pub(crate) fn parse_statement(st: &mut ParseState) -> Result<Spanned<Expr>, String> {
    let start = st.save();

    // Mutable set: ~@Name.set(expr)
    // Mutable new: ~@Name Type.new(args)
    if st.peek() == Some(&Token::Tilde) {
        let save = st.save();
        st.advance();
        if st.eat(&Token::At) {
            if let Some((name, _)) = st.eat_pascal() {
                // Try mutable set: .set(expr)
                if st.peek() == Some(&Token::Dot) {
                    let save2 = st.save();
                    st.advance();
                    if let Some(Token::CamelIdent(m)) = st.peek() {
                        if m == "set" {
                            st.advance();
                            st.expect(&Token::LParen)?;
                            let val = parse_expr(st)?;
                            st.expect(&Token::RParen)?;
                            let span = st.span_from(start);
                            return Ok(Spanned::new(
                                Expr::MutableSet(name, Box::new(val)),
                                span,
                            ));
                        }
                    }
                    st.restore(save2);
                }

                // Try mutable new: Type/new(args)
                let save2 = st.save();
                if let Ok(tr) = parse_type_ref(st) {
                    if st.eat(&Token::Slash) || st.eat(&Token::Dot) {
                        if let Some(Token::CamelIdent(m)) = st.peek() {
                            if m == "new" {
                                st.advance();
                                let args = parse_new_args(st)?;
                                let span = st.span_from(start);
                                return Ok(Spanned::new(
                                    Expr::MutableNew(name, tr, args),
                                    span,
                                ));
                            }
                        }
                    }
                }
                st.restore(save2);
            }
        }
        st.restore(save);
    }

    // Sub-type new: @Name Type/new(args)
    // Same-type new: @Name/new(args)
    if st.peek() == Some(&Token::At) {
        let save = st.save();
        st.advance();
        if let Some((name, _)) = st.eat_pascal() {
            // Try sub-type: Type/new(args)
            let save2 = st.save();
            if let Ok(tr) = parse_type_ref(st) {
                if st.eat(&Token::Slash) || st.eat(&Token::Dot) {
                    if let Some(Token::CamelIdent(m)) = st.peek() {
                        if m == "new" {
                            st.advance();
                            let args = parse_new_args(st)?;
                            let span = st.span_from(start);
                            return Ok(Spanned::new(
                                Expr::SubTypeNew(name, tr, args),
                                span,
                            ));
                        }
                    }
                }
                st.restore(save2);
            } else {
                st.restore(save2);
            }

            // Try same-type: /new(args)
            if st.eat(&Token::Slash) || st.eat(&Token::Dot) {
                if let Some(Token::CamelIdent(m)) = st.peek() {
                    if m == "new" {
                        st.advance();
                        let args = parse_new_args(st)?;
                        let span = st.span_from(start);
                        return Ok(Spanned::new(
                            Expr::SameTypeNew(name, args),
                            span,
                        ));
                    }
                }
            }
        }
        st.restore(save);
    }

    // SubTypeDecl: PascalName Type (no @, no value)
    if let Some(Token::PascalIdent(_)) = st.peek() {
        let save = st.save();
        let (name, _) = st.eat_pascal().unwrap();
        // Must not be "StdOut" or "Main" — those are expr atoms
        if name != "StdOut" && name != "Main" {
            let save2 = st.save();
            if let Ok(tr) = parse_type_ref(st) {
                // Make sure we're not consuming something that's actually the next statement
                // A SubTypeDecl is: PascalName TypeRef and nothing follows on same line
                // (or the TypeRef is a simple named type, not ambiguous)
                // Heuristic: if what follows is NOT a dot, we treat it as SubTypeDecl
                if st.peek() != Some(&Token::Dot)
                    && st.peek() != Some(&Token::LParen)
                {
                    let span = st.span_from(save);
                    return Ok(Spanned::new(Expr::SubTypeDecl(name, tr), span));
                }
            }
            st.restore(save2);
        }
        st.restore(save);
    }

    // Fallback: expression
    parse_expr(st)
}

pub(crate) fn parse_new_args(st: &mut ParseState) -> Result<Vec<Spanned<Expr>>, String> {
    st.expect(&Token::LParen)?;
    let mut args = Vec::new();
    loop {
        st.skip_newlines();
        if st.peek() == Some(&Token::RParen) {
            st.advance();
            break;
        }
        // Try field pair: PascalName(expr)
        let save = st.save();
        if let Some((fname, _)) = st.eat_pascal() {
            if st.eat(&Token::LParen) {
                if let Ok(val) = parse_expr(st) {
                    if st.eat(&Token::RParen) {
                        let span = val.span.clone();
                        args.push(Spanned::new(
                            Expr::StructConstruct(fname, vec![("_value".to_string(), val)]),
                            span,
                        ));
                        st.skip_newlines();
                        continue;
                    }
                }
            }
            st.restore(save);
        }
        args.push(parse_expr(st)?);
        st.skip_newlines();
    }
    Ok(args)
}

// ── Body parser ─────────────────────────────────────────────────────

pub(crate) fn parse_body(st: &mut ParseState) -> Result<Body, String> {
    // Try matching body: (| ... |)
    if st.peek() == Some(&Token::CompositionOpen) {
        return parse_matching_body(st);
    }

    // Try tail body: [| ... |]
    if st.peek() == Some(&Token::IterOpen) {
        st.advance();
        let mut stmts = Vec::new();
        loop {
            st.skip_newlines();
            if st.peek() == Some(&Token::IterClose) {
                st.advance();
                break;
            }
            stmts.push(parse_statement(st)?);
            st.skip_newlines();
        }
        return Ok(Body::TailBlock(stmts));
    }

    // Computed body: [ ... ]
    st.expect(&Token::LBracket)?;
    st.skip_newlines();

    // Stub body
    if st.peek() == Some(&Token::Stub) {
        st.advance();
        st.skip_newlines();
        st.expect(&Token::RBracket)?;
        return Ok(Body::Stub);
    }

    let mut stmts = Vec::new();
    loop {
        st.skip_newlines();
        if st.peek() == Some(&Token::RBracket) {
            st.advance();
            break;
        }
        stmts.push(parse_statement(st)?);
        st.skip_newlines();
    }
    Ok(Body::Block(stmts))
}

pub(crate) fn parse_const_body(st: &mut ParseState) -> Result<Body, String> {
    st.expect(&Token::LBrace)?;
    st.skip_newlines();
    let mut stmts = Vec::new();
    loop {
        st.skip_newlines();
        if st.peek() == Some(&Token::RBrace) {
            st.advance();
            break;
        }
        stmts.push(parse_expr(st)?);
        st.skip_newlines();
    }
    Ok(Body::Block(stmts))
}

fn parse_matching_body(st: &mut ParseState) -> Result<Body, String> {
    st.expect(&Token::CompositionOpen)?;
    let mut arms = Vec::new();
    loop {
        st.skip_newlines();
        if st.peek() == Some(&Token::CompositionClose) {
            st.advance();
            break;
        }
        arms.push(parse_match_method_arm(st)?);
    }
    Ok(Body::MatchBody(arms))
}

fn parse_match_method_arm(st: &mut ParseState) -> Result<MatchMethodArm, String> {
    let start = st.save();
    st.skip_newlines();

    // Try destructure arm: [elements | @Rest] result
    if st.peek() == Some(&Token::LBracket) {
        let save = st.save();
        st.advance();
        let mut elements = Vec::new();
        let mut found_pipe = false;
        let mut rest_name = String::new();

        loop {
            st.skip_newlines();
            if st.peek() == Some(&Token::RBracket) {
                st.advance();
                break;
            }
            if st.peek() == Some(&Token::Pipe) {
                st.advance();
                st.skip_newlines();
                st.expect(&Token::At)?;
                let (rn, _) = st.eat_pascal().ok_or("expected name after @")?;
                rest_name = rn;
                found_pipe = true;
                st.skip_newlines();
                st.expect(&Token::RBracket)?;
                break;
            }
            // Parse element: @Name, _, PascalName
            if st.peek() == Some(&Token::At) {
                st.advance();
                let (n, _) = st.eat_pascal().ok_or("expected name after @")?;
                elements.push(DestructureElement::Binding(n));
            } else if st.eat(&Token::Underscore) {
                elements.push(DestructureElement::Wildcard);
            } else if let Some((n, _)) = st.eat_pascal() {
                elements.push(DestructureElement::ExactToken(n));
            } else {
                // Not a destructure arm
                break;
            }
            st.skip_newlines();
        }

        if found_pipe {
            st.skip_newlines();
            let body_expr = parse_expr(st)?;
            let span = st.span_from(start);
            return Ok(MatchMethodArm {
                kind: ArmKind::Destructure,
                patterns: vec![],
                body: vec![body_expr],
                destructure: Some((elements, rest_name)),
                span,
            });
        }

        // Try backtrack arm: [patterns] result
        st.restore(save);
        st.advance(); // [
        let mut patterns = Vec::new();
        loop {
            st.skip_newlines();
            if st.peek() == Some(&Token::RBracket) {
                st.advance();
                break;
            }
            patterns.push(parse_match_pattern(st)?);
            st.skip_newlines();
        }
        st.skip_newlines();
        let body_expr = parse_expr(st)?;
        let span = st.span_from(start);
        return Ok(MatchMethodArm {
            kind: ArmKind::Backtrack,
            patterns,
            body: vec![body_expr],
            destructure: None,
            span,
        });
    }

    // Commit arm: (patterns) result
    st.expect(&Token::LParen)?;
    let mut patterns = Vec::new();
    loop {
        st.skip_newlines();
        if st.peek() == Some(&Token::RParen) {
            st.advance();
            break;
        }
        patterns.push(parse_match_pattern(st)?);
        st.skip_newlines();
    }
    st.skip_newlines();
    let body_expr = parse_expr(st)?;
    let span = st.span_from(start);
    Ok(MatchMethodArm {
        kind: ArmKind::Commit,
        patterns,
        body: vec![body_expr],
        destructure: None,
        span,
    })
}
