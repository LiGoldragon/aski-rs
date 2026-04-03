//! Expression parsers: atoms, operators, Pratt precedence, method calls, access.

use crate::ast::*;
use crate::lexer::Token;
use super::state::ParseState;
use super::pattern::parse_simple_pattern;

// ── Expression parser ───────────────────────────────────────────────

pub(crate) fn parse_expr(st: &mut ParseState) -> Result<Spanned<Expr>, String> {
    parse_or_expr(st)
}

fn parse_or_expr(st: &mut ParseState) -> Result<Spanned<Expr>, String> {
    let mut left = parse_and_expr(st)?;
    while st.peek() == Some(&Token::Or) {
        st.advance();
        let right = parse_and_expr(st)?;
        let span = left.span.start..right.span.end;
        left = Spanned::new(Expr::BinOp(Box::new(left), BinOp::Or, Box::new(right)), span);
    }
    Ok(left)
}

fn parse_and_expr(st: &mut ParseState) -> Result<Spanned<Expr>, String> {
    let mut left = parse_cmp_expr(st)?;
    while st.peek() == Some(&Token::And) {
        st.advance();
        let right = parse_cmp_expr(st)?;
        let span = left.span.start..right.span.end;
        left = Spanned::new(Expr::BinOp(Box::new(left), BinOp::And, Box::new(right)), span);
    }
    Ok(left)
}

fn parse_cmp_expr(st: &mut ParseState) -> Result<Spanned<Expr>, String> {
    let mut left = parse_add_expr(st)?;
    loop {
        let op = match st.peek() {
            Some(Token::DoubleEq) => BinOp::Eq,
            Some(Token::Neq) => BinOp::Neq,
            Some(Token::Gte) => BinOp::Gte,
            Some(Token::Lte) => BinOp::Lte,
            Some(Token::Gt) => BinOp::Gt,
            Some(Token::Lt) => BinOp::Lt,
            _ => break,
        };
        st.advance();
        let right = parse_add_expr(st)?;
        let span = left.span.start..right.span.end;
        left = Spanned::new(Expr::BinOp(Box::new(left), op, Box::new(right)), span);
    }
    Ok(left)
}

fn parse_add_expr(st: &mut ParseState) -> Result<Spanned<Expr>, String> {
    let mut left = parse_mul_expr(st)?;
    loop {
        let op = match st.peek() {
            Some(Token::Plus) => BinOp::Add,
            Some(Token::Minus) => BinOp::Sub,
            _ => break,
        };
        st.advance();
        let right = parse_mul_expr(st)?;
        let span = left.span.start..right.span.end;
        left = Spanned::new(Expr::BinOp(Box::new(left), op, Box::new(right)), span);
    }
    Ok(left)
}

fn parse_mul_expr(st: &mut ParseState) -> Result<Spanned<Expr>, String> {
    let mut left = parse_postfix_expr(st)?;
    loop {
        let op = match st.peek() {
            Some(Token::Star) => BinOp::Mul,
            Some(Token::Slash) => BinOp::Div,
            Some(Token::Percent) => BinOp::Rem,
            _ => break,
        };
        st.advance();
        let right = parse_postfix_expr(st)?;
        let span = left.span.start..right.span.end;
        left = Spanned::new(Expr::BinOp(Box::new(left), op, Box::new(right)), span);
    }
    Ok(left)
}

fn parse_postfix_expr(st: &mut ParseState) -> Result<Spanned<Expr>, String> {
    let mut base = parse_atom(st)?;

    // Dot access / method call chain
    loop {
        if st.peek() != Some(&Token::Dot) {
            break;
        }
        st.advance();
        // field or method name: pascal or camel
        let name = if let Some((n, _)) = st.eat_pascal() {
            n
        } else if let Some((n, _)) = st.eat_camel() {
            n
        } else {
            return Err(format!("expected field/method name after dot, got {:?}", st.peek()));
        };

        // Check for method call: name(args)
        if st.peek() == Some(&Token::LParen) {
            st.advance();
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
            let span = base.span.start..st.span_from(0).end;
            base = Spanned::new(Expr::MethodCall(Box::new(base), name, args), span);
        } else {
            let span = base.span.start..st.span_from(0).end;
            base = Spanned::new(Expr::Access(Box::new(base), name), span);
        }
    }

    Ok(base)
}

fn parse_atom(st: &mut ParseState) -> Result<Spanned<Expr>, String> {
    let start = st.save();

    // Stub: ___
    if st.peek() == Some(&Token::Stub) {
        let span = st.tokens[st.pos].span.clone();
        st.advance();
        return Ok(Spanned::new(Expr::Stub, span));
    }

    // Const reference: !Name
    if st.peek() == Some(&Token::Bang) {
        st.advance();
        let (name, _) = st.eat_pascal().ok_or("expected Pascal after !")?;
        let span = st.span_from(start);
        return Ok(Spanned::new(Expr::ConstRef(name), span));
    }

    // Return: ^expr
    if st.peek() == Some(&Token::Caret) {
        st.advance();
        let e = parse_expr(st)?;
        let span = st.span_from(start);
        return Ok(Spanned::new(Expr::Return(Box::new(e)), span));
    }

    // Yield: #expr
    if st.peek() == Some(&Token::Hash) {
        st.advance();
        let e = parse_expr(st)?;
        let span = st.span_from(start);
        return Ok(Spanned::new(Expr::Yield(Box::new(e)), span));
    }

    // Instance reference: @Name
    if st.peek() == Some(&Token::At) {
        st.advance();
        let (name, _) = st.eat_pascal().ok_or("expected Pascal after @")?;
        let span = st.span_from(start);
        return Ok(Spanned::new(Expr::InstanceRef(name), span));
    }

    // Match expression: (| targets arms |)
    if st.peek() == Some(&Token::CompositionOpen) {
        return parse_match_expr(st);
    }

    // Float literal (must come before integer)
    if let Some(Token::Float(s)) = st.peek() {
        let val = s.parse::<f64>().map_err(|e| format!("invalid float: {}", e))?;
        let span = st.tokens[st.pos].span.clone();
        st.advance();
        return Ok(Spanned::new(Expr::FloatLit(val), span));
    }

    // Integer literal
    if let Some(Token::Integer(v)) = st.peek() {
        let v = *v;
        let span = st.tokens[st.pos].span.clone();
        st.advance();
        return Ok(Spanned::new(Expr::IntLit(v), span));
    }

    // String literal
    if let Some(Token::StringLit(s)) = st.peek() {
        let s = s.clone();
        let span = st.tokens[st.pos].span.clone();
        st.advance();
        return Ok(Spanned::new(Expr::StringLit(s), span));
    }

    // Inline eval: [expr expr ...]
    if st.peek() == Some(&Token::LBracket) {
        st.advance();
        let mut exprs = Vec::new();
        loop {
            st.skip_newlines();
            if st.peek() == Some(&Token::RBracket) {
                st.advance();
                break;
            }
            exprs.push(parse_expr(st)?);
            st.skip_newlines();
        }
        let span = st.span_from(start);
        return Ok(Spanned::new(Expr::InlineEval(exprs), span));
    }

    // Grouping: (expr)
    if st.peek() == Some(&Token::LParen) {
        st.advance();
        let e = parse_expr(st)?;
        st.expect(&Token::RParen)?;
        let span = st.span_from(start);
        return Ok(Spanned::new(Expr::Group(Box::new(e)), span));
    }

    // StdOut: StdOut expr
    if let Some(Token::PascalIdent(s)) = st.peek() {
        if s == "StdOut" {
            st.advance();
            let e = parse_expr(st)?;
            let span = st.span_from(start);
            return Ok(Spanned::new(Expr::StdOut(Box::new(e)), span));
        }
    }

    // Struct construction: TypeName(Field(val) Field(val) ...) — needs at least one field pair
    // Also handles variant construction: Name(expr)
    if let Some(Token::PascalIdent(_)) = st.peek() {
        let save = st.save();
        let (name, _) = st.eat_pascal().unwrap();

        if st.peek() == Some(&Token::LParen) {
            // Try struct construction: Name(Field(val) ...)
            let save2 = st.save();
            st.advance(); // consume (
            // Try to parse as field pairs
            let mut field_pairs: Vec<(String, Spanned<Expr>)> = Vec::new();
            let mut is_struct = true;

            st.skip_newlines();
            // First, check if it looks like a field pair: Pascal(
            if let Some(Token::PascalIdent(_)) = st.peek() {
                let save3 = st.save();
                if let Some((fname, _)) = st.eat_pascal() {
                    if st.eat(&Token::LParen) {
                        if let Ok(val) = parse_expr(st) {
                            if st.eat(&Token::RParen) {
                                field_pairs.push((fname, val));
                                // Parse more field pairs
                                loop {
                                    st.skip_newlines();
                                    if st.peek() == Some(&Token::RParen) {
                                        st.advance();
                                        break;
                                    }
                                    let (fn2, _) = st.eat_pascal().ok_or_else(|| {
                                        is_struct = false;
                                        "not a struct".to_string()
                                    }).unwrap_or_default();
                                    if fn2.is_empty() || !st.eat(&Token::LParen) {
                                        is_struct = false;
                                        break;
                                    }
                                    match parse_expr(st) {
                                        Ok(val2) => {
                                            if !st.eat(&Token::RParen) {
                                                is_struct = false;
                                                break;
                                            }
                                            field_pairs.push((fn2, val2));
                                        }
                                        Err(_) => {
                                            is_struct = false;
                                            break;
                                        }
                                    }
                                }
                                if is_struct && !field_pairs.is_empty() {
                                    let span = st.span_from(save);
                                    return Ok(Spanned::new(
                                        Expr::StructConstruct(name, field_pairs),
                                        span,
                                    ));
                                }
                            }
                        }
                    }
                }
                st.restore(save3);
            }

            // Not a struct — restore to before the (
            st.restore(save2);
        }

        // Just a bare PascalCase name
        st.restore(save);
        let (name, _) = st.eat_pascal().unwrap();
        let span = st.span_from(start);
        return Ok(Spanned::new(Expr::BareName(name), span));
    }

    // camelCase bare name
    if let Some((name, _)) = st.eat_camel() {
        let span = st.span_from(start);
        return Ok(Spanned::new(Expr::BareName(name), span));
    }

    Err(format!(
        "expected expression, got {:?} at position {}",
        st.peek(),
        st.pos
    ))
}

fn parse_match_expr(st: &mut ParseState) -> Result<Spanned<Expr>, String> {
    let start = st.save();
    st.expect(&Token::CompositionOpen)?;

    st.skip_newlines();

    // Match targets: @Name ...
    let mut targets = Vec::new();
    while st.peek() == Some(&Token::At) {
        let tstart = st.save();
        st.advance();
        let (name, _) = st.eat_pascal().ok_or("expected Pascal after @ in match target")?;
        let tspan = st.span_from(tstart);
        targets.push(Spanned::new(Expr::InstanceRef(name), tspan));
        st.skip_newlines();
    }

    // Match arms: (patterns) result
    let mut arms = Vec::new();
    loop {
        st.skip_newlines();
        if st.peek() == Some(&Token::CompositionClose) {
            st.advance();
            break;
        }
        let arm = parse_match_arm(st)?;
        arms.push(arm);
    }

    let span = st.span_from(start);
    Ok(Spanned::new(
        Expr::MatchExpr(MatchExprData { targets, arms }),
        span,
    ))
}

fn parse_match_arm(st: &mut ParseState) -> Result<MatchArm, String> {
    let start = st.save();
    st.skip_newlines();

    // (patterns)
    st.expect(&Token::LParen)?;
    let mut patterns = Vec::new();
    loop {
        st.skip_newlines();
        if st.peek() == Some(&Token::RParen) {
            st.advance();
            break;
        }
        patterns.push(parse_simple_pattern(st)?);
        st.skip_newlines();
    }

    st.skip_newlines();

    // result: try variant construction PascalName(expr) first, then plain expr
    let body_expr = {
        let save = st.save();
        if let Some(Token::PascalIdent(_)) = st.peek() {
            let (name, _) = st.eat_pascal().unwrap();
            if st.peek() == Some(&Token::LParen) {
                st.advance();
                if let Ok(inner) = parse_expr(st) {
                    if st.eat(&Token::RParen) {
                        let span = st.span_from(save);
                        Spanned::new(
                            Expr::StructConstruct(name, vec![("_0".to_string(), inner)]),
                            span,
                        )
                    } else {
                        st.restore(save);
                        parse_expr(st)?
                    }
                } else {
                    st.restore(save);
                    parse_expr(st)?
                }
            } else {
                st.restore(save);
                parse_expr(st)?
            }
        } else {
            parse_expr(st)?
        }
    };

    let span = st.span_from(start);
    Ok(MatchArm {
        patterns,
        body: vec![body_expr],
        span,
    })
}
