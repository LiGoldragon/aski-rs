//! Full grammar engine parser for aski v0.10.
//!
//! Parses ALL aski syntax, producing the same AST types as the chumsky parser.
//! Uses the grammar engine's Rule/Arm/PatternElement data structures for simple
//! items, and programmatic Rust parsers for complex recursive constructs
//! (expressions, statements, bodies).
//!
//! This is wired as an ALTERNATIVE parser — both chumsky and grammar engine
//! can parse the same files. Eventually, grammar engine becomes primary.

use crate::ast::*;
use crate::lexer::Token;

// ── Token stream helpers ────────────────────────────────────────────

/// A token with its span (byte range in source).
#[derive(Debug, Clone)]
struct SpannedToken {
    token: Token,
    span: Span,
}

/// Parser state: a position in the token stream.
struct ParseState<'a> {
    tokens: &'a [SpannedToken],
    pos: usize,
}

impl<'a> ParseState<'a> {
    fn new(tokens: &'a [SpannedToken]) -> Self {
        Self { tokens, pos: 0 }
    }

    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.pos).map(|t| &t.token)
    }

    fn peek_span(&self) -> Span {
        if self.pos < self.tokens.len() {
            self.tokens[self.pos].span.clone()
        } else if !self.tokens.is_empty() {
            let end = self.tokens.last().unwrap().span.end;
            end..end
        } else {
            0..0
        }
    }

    fn at_end(&self) -> bool {
        self.pos >= self.tokens.len()
    }

    fn advance(&mut self) -> Option<&SpannedToken> {
        if self.pos < self.tokens.len() {
            let t = &self.tokens[self.pos];
            self.pos += 1;
            Some(t)
        } else {
            None
        }
    }

    fn expect(&mut self, expected: &Token) -> Result<Span, String> {
        match self.peek() {
            Some(t) if t == expected => {
                let span = self.tokens[self.pos].span.clone();
                self.pos += 1;
                Ok(span)
            }
            Some(t) => Err(format!(
                "expected {:?}, got {:?} at position {}",
                expected, t, self.pos
            )),
            None => Err(format!("expected {:?}, got end of input", expected)),
        }
    }

    fn skip_newlines(&mut self) {
        while let Some(Token::Newline) = self.peek() {
            self.pos += 1;
        }
    }

    fn save(&self) -> usize {
        self.pos
    }

    fn restore(&mut self, pos: usize) {
        self.pos = pos;
    }

    /// Span from `start` to current position.
    fn span_from(&self, start: usize) -> Span {
        let s = if start < self.tokens.len() {
            self.tokens[start].span.start
        } else if !self.tokens.is_empty() {
            self.tokens.last().unwrap().span.end
        } else {
            0
        };
        let e = if self.pos > 0 && self.pos <= self.tokens.len() {
            self.tokens[self.pos - 1].span.end
        } else {
            s
        };
        s..e
    }

    fn eat_pascal(&mut self) -> Option<(String, Span)> {
        if let Some(Token::PascalIdent(s)) = self.peek() {
            let s = s.clone();
            let span = self.tokens[self.pos].span.clone();
            self.pos += 1;
            Some((s, span))
        } else {
            None
        }
    }

    fn eat_camel(&mut self) -> Option<(String, Span)> {
        if let Some(Token::CamelIdent(s)) = self.peek() {
            let s = s.clone();
            let span = self.tokens[self.pos].span.clone();
            self.pos += 1;
            Some((s, span))
        } else {
            None
        }
    }

    fn eat(&mut self, expected: &Token) -> bool {
        if self.peek() == Some(expected) {
            self.pos += 1;
            true
        } else {
            false
        }
    }
}

// ── Type reference parser ───────────────────────────────────────────

fn parse_type_ref(st: &mut ParseState) -> Result<TypeRef, String> {
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

fn parse_field_type_ref(st: &mut ParseState) -> Result<TypeRef, String> {
    // :Type = Borrowed
    if st.peek() == Some(&Token::Colon) {
        st.advance();
        let inner = parse_type_ref(st)?;
        return Ok(TypeRef::Borrowed(Box::new(inner)));
    }
    parse_type_ref(st)
}

// ── Parameter parser ────────────────────────────────────────────────

fn parse_param(st: &mut ParseState) -> Result<Param, String> {
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

// ── Expression parser ───────────────────────────────────────────────

fn parse_expr(st: &mut ParseState) -> Result<Spanned<Expr>, String> {
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

fn parse_simple_pattern(st: &mut ParseState) -> Result<Pattern, String> {
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

fn parse_match_pattern(st: &mut ParseState) -> Result<Pattern, String> {
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
        let save = st.save();
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

// ── Statement parser ────────────────────────────────────────────────

fn parse_statement(st: &mut ParseState) -> Result<Spanned<Expr>, String> {
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

                // Try mutable new: Type.new(args)
                let save2 = st.save();
                if let Ok(tr) = parse_type_ref(st) {
                    if st.eat(&Token::Dot) {
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

    // Sub-type new: @Name Type.new(args)
    // Same-type new: @Name.new(args)
    if st.peek() == Some(&Token::At) {
        let save = st.save();
        st.advance();
        if let Some((name, _)) = st.eat_pascal() {
            // Try sub-type: Type.new(args)
            let save2 = st.save();
            if let Ok(tr) = parse_type_ref(st) {
                if st.eat(&Token::Dot) {
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

            // Try same-type: .new(args)
            if st.eat(&Token::Dot) {
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

fn parse_new_args(st: &mut ParseState) -> Result<Vec<Spanned<Expr>>, String> {
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

fn parse_body(st: &mut ParseState) -> Result<Body, String> {
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

fn parse_const_body(st: &mut ParseState) -> Result<Body, String> {
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

// ── Method parsers ──────────────────────────────────────────────────

fn parse_method_sig(st: &mut ParseState) -> Result<MethodSig, String> {
    let start = st.save();
    let (name, _) = st.eat_camel().ok_or("expected camel for method sig")?;
    st.expect(&Token::LParen)?;
    let mut params = Vec::new();
    loop {
        st.skip_newlines();
        if st.peek() == Some(&Token::RParen) {
            st.advance();
            break;
        }
        params.push(parse_param(st)?);
        st.skip_newlines();
    }
    let output = parse_type_ref(st).ok();
    let span = st.span_from(start);
    Ok(MethodSig {
        name,
        params,
        output,
        span,
    })
}

fn parse_method_def(st: &mut ParseState) -> Result<MethodDef, String> {
    let start = st.save();
    let (name, _) = st.eat_camel().ok_or("expected camel for method def")?;
    st.expect(&Token::LParen)?;
    let mut params = Vec::new();
    loop {
        st.skip_newlines();
        if st.peek() == Some(&Token::RParen) {
            st.advance();
            break;
        }
        params.push(parse_param(st)?);
        st.skip_newlines();
    }
    let output = {
        let save = st.save();
        match parse_type_ref(st) {
            Ok(tr) => Some(tr),
            Err(_) => {
                st.restore(save);
                None
            }
        }
    };
    let body = parse_body(st)?;
    let span = st.span_from(start);
    Ok(MethodDef {
        name,
        params,
        output,
        body,
        span,
    })
}

// ── Item parsers ────────────────────────────────────────────────────

fn parse_variant(st: &mut ParseState) -> Result<Variant, String> {
    let start = st.save();
    let (name, _) = st.eat_pascal().ok_or("expected Pascal for variant")?;

    // Struct variant: Name { fields }
    if st.peek() == Some(&Token::LBrace) {
        st.advance();
        let mut fields = Vec::new();
        loop {
            st.skip_newlines();
            if st.peek() == Some(&Token::RBrace) {
                st.advance();
                break;
            }
            let fstart = st.save();
            let (fname, _) = st.eat_pascal().ok_or("expected field name")?;
            let ftype = parse_field_type_ref(st)?;
            let fspan = st.span_from(fstart);
            fields.push(Field {
                name: fname,
                type_ref: ftype,
                span: fspan,
            });
            st.skip_newlines();
        }
        let span = st.span_from(start);
        return Ok(Variant {
            name,
            wraps: None,
            fields: Some(fields),
            sub_variants: None,
            span,
        });
    }

    // Paren variants: inline domain or newtype wrap
    if st.peek() == Some(&Token::LParen) {
        st.advance();
        st.skip_newlines();

        // Count PascalCase names to distinguish inline domain from newtype
        let save = st.save();
        let mut pascal_names = Vec::new();
        loop {
            st.skip_newlines();
            if st.peek() == Some(&Token::RParen) {
                break;
            }
            if let Some((pname, _)) = st.eat_pascal() {
                pascal_names.push(pname);
            } else {
                break;
            }
            st.skip_newlines();
        }
        st.restore(save);

        if pascal_names.len() >= 2 {
            // Inline domain: Name (A B C)
            let mut sub_vars = Vec::new();
            loop {
                st.skip_newlines();
                if st.peek() == Some(&Token::RParen) {
                    st.advance();
                    break;
                }
                let svstart = st.save();
                let (svname, _) = st.eat_pascal().ok_or("expected variant name")?;
                let svspan = st.span_from(svstart);
                sub_vars.push(Variant {
                    name: svname,
                    wraps: None,
                    fields: None,
                    sub_variants: None,
                    span: svspan,
                });
                st.skip_newlines();
            }
            let span = st.span_from(start);
            return Ok(Variant {
                name,
                wraps: None,
                fields: None,
                sub_variants: Some(sub_vars),
                span,
            });
        } else {
            // Newtype wrap: Name (Type)
            let tr = parse_type_ref(st)?;
            st.expect(&Token::RParen)?;
            let span = st.span_from(start);
            return Ok(Variant {
                name,
                wraps: Some(tr),
                fields: None,
                sub_variants: None,
                span,
            });
        }
    }

    // Unit variant
    let span = st.span_from(start);
    Ok(Variant {
        name,
        wraps: None,
        fields: None,
        sub_variants: None,
        span,
    })
}

fn parse_domain_decl(st: &mut ParseState) -> Result<Item, String> {
    let start = st.save();
    let (name, _) = st.eat_pascal().ok_or("expected domain name")?;
    st.expect(&Token::LParen)?;
    let mut variants = Vec::new();
    loop {
        st.skip_newlines();
        if st.peek() == Some(&Token::RParen) {
            st.advance();
            break;
        }
        variants.push(parse_variant(st)?);
        st.skip_newlines();
    }
    let span = st.span_from(start);
    Ok(Item::Domain(DomainDecl {
        name,
        variants,
        span,
    }))
}

fn parse_struct_decl(st: &mut ParseState) -> Result<Item, String> {
    let start = st.save();
    let (name, _) = st.eat_pascal().ok_or("expected struct name")?;
    st.expect(&Token::LBrace)?;
    let mut fields = Vec::new();
    loop {
        st.skip_newlines();
        if st.peek() == Some(&Token::RBrace) {
            st.advance();
            break;
        }
        let fstart = st.save();
        let (fname, _) = st.eat_pascal().ok_or("expected field name")?;
        let ftype = parse_field_type_ref(st)?;
        let fspan = st.span_from(fstart);
        fields.push(Field {
            name: fname,
            type_ref: ftype,
            span: fspan,
        });
        st.skip_newlines();
    }
    let span = st.span_from(start);
    Ok(Item::Struct(StructDecl {
        name,
        fields,
        span,
    }))
}

fn parse_const_decl(st: &mut ParseState) -> Result<Item, String> {
    let start = st.save();
    st.expect(&Token::Bang)?;
    let (name, _) = st.eat_pascal().ok_or("expected const name")?;
    let tr = parse_type_ref(st)?;
    let value = if st.peek() == Some(&Token::LBrace) {
        Some(parse_const_body(st)?)
    } else {
        None
    };
    let span = st.span_from(start);
    Ok(Item::Const(ConstDecl {
        name,
        type_ref: tr,
        value,
        span,
    }))
}

fn parse_main_decl(st: &mut ParseState) -> Result<Item, String> {
    let start = st.save();
    // "Main" already matched by caller
    let body = parse_body(st)?;
    let span = st.span_from(start);
    Ok(Item::Main(MainDecl { body, span }))
}

fn parse_trait_decl(st: &mut ParseState) -> Result<Item, String> {
    let start = st.save();
    let (name, _) = st.eat_camel().ok_or("expected camel for trait name")?;
    st.expect(&Token::LParen)?;
    st.skip_newlines();

    // Optional supertraits (camelCase before the [methods] block)
    let mut supertraits = Vec::new();
    loop {
        let save = st.save();
        if let Some(Token::CamelIdent(_)) = st.peek() {
            // Peek ahead: if followed by another camel or [, it's a supertrait
            let (sname, _) = st.eat_camel().unwrap();
            // Check if next is [ or another camel — if so, supertrait
            if st.peek() == Some(&Token::LBracket)
                || matches!(st.peek(), Some(Token::CamelIdent(_)))
            {
                supertraits.push(sname);
                st.skip_newlines();
                continue;
            }
            st.restore(save);
        }
        break;
    }

    // [methods / constants]
    st.skip_newlines();
    st.expect(&Token::LBracket)?;
    let mut methods = Vec::new();
    let mut constants = Vec::new();
    loop {
        st.skip_newlines();
        if st.peek() == Some(&Token::RBracket) {
            st.advance();
            break;
        }
        // Trait constant: !Name Type {value}?
        if st.peek() == Some(&Token::Bang) {
            let cstart = st.save();
            st.advance();
            let (cname, _) = st.eat_pascal().ok_or("expected const name")?;
            let ctr = parse_type_ref(st)?;
            let cval = if st.peek() == Some(&Token::LBrace) {
                Some(parse_const_body(st)?)
            } else {
                None
            };
            let cspan = st.span_from(cstart);
            constants.push(ConstDecl {
                name: cname,
                type_ref: ctr,
                value: cval,
                span: cspan,
            });
            continue;
        }
        methods.push(parse_method_sig(st)?);
        st.skip_newlines();
    }
    st.skip_newlines();
    st.expect(&Token::RParen)?;
    let span = st.span_from(start);
    Ok(Item::Trait(TraitDecl {
        name,
        supertraits,
        methods,
        constants,
        span,
    }))
}

fn parse_impl_block(st: &mut ParseState) -> Result<Item, String> {
    let start = st.save();
    let (trait_name, _) = st.eat_camel().ok_or("expected camel for trait impl")?;
    st.expect(&Token::LBracket)?;
    let mut impls = Vec::new();
    loop {
        st.skip_newlines();
        if st.peek() == Some(&Token::RBracket) {
            st.advance();
            break;
        }
        impls.push(parse_type_impl(st)?);
        st.skip_newlines();
    }
    let span = st.span_from(start);
    Ok(Item::TraitImpl(TraitImplDecl {
        trait_name,
        impls,
        span,
    }))
}

fn parse_type_impl(st: &mut ParseState) -> Result<TypeImpl, String> {
    let start = st.save();
    let (target, _) = st.eat_pascal().ok_or("expected Pascal for impl target")?;
    st.expect(&Token::LBracket)?;
    let mut methods = Vec::new();
    let mut associated_types = Vec::new();
    let mut associated_constants = Vec::new();
    loop {
        st.skip_newlines();
        if st.peek() == Some(&Token::RBracket) {
            st.advance();
            break;
        }

        // Associated constant: !Name Type {value}?
        if st.peek() == Some(&Token::Bang) {
            let cstart = st.save();
            st.advance();
            let (cname, _) = st.eat_pascal().ok_or("expected const name")?;
            let ctr = parse_type_ref(st)?;
            let cval = if st.peek() == Some(&Token::LBrace) {
                Some(parse_const_body(st)?)
            } else {
                None
            };
            let cspan = st.span_from(cstart);
            associated_constants.push(ConstDecl {
                name: cname,
                type_ref: ctr,
                value: cval,
                span: cspan,
            });
            continue;
        }

        // Associated type: PascalName Type (PascalCase name followed by type ref,
        // but NOT followed by body/params — distinguish from method)
        if let Some(Token::PascalIdent(_)) = st.peek() {
            let save = st.save();
            let (aname, _) = st.eat_pascal().unwrap();
            if let Ok(concrete) = parse_type_ref(st) {
                // If next token is NOT ( or [ or (| — it's an associated type
                if st.peek() != Some(&Token::LParen)
                    && st.peek() != Some(&Token::LBracket)
                    && st.peek() != Some(&Token::CompositionOpen)
                {
                    let aspan = st.span_from(save);
                    associated_types.push(AssociatedTypeDef {
                        name: aname,
                        concrete_type: concrete,
                        span: aspan,
                    });
                    continue;
                }
            }
            st.restore(save);
        }

        // Method def
        methods.push(parse_method_def(st)?);
        st.skip_newlines();
    }
    let span = st.span_from(start);
    Ok(TypeImpl {
        target,
        methods,
        associated_types,
        associated_constants,
        span,
    })
}

fn parse_type_alias(st: &mut ParseState) -> Result<Item, String> {
    let start = st.save();
    let (name, _) = st.eat_pascal().ok_or("expected Pascal for type alias")?;
    let target = parse_type_ref(st)?;
    let span = st.span_from(start);
    Ok(Item::TypeAlias(TypeAliasDecl {
        name,
        target,
        span,
    }))
}

fn parse_grammar_element(st: &mut ParseState) -> Result<GrammarElement, String> {
    // <Name> — non-terminal
    if st.peek() == Some(&Token::Lt) {
        st.advance();
        let (name, _) = st.eat_pascal().ok_or("expected Pascal in non-terminal")?;
        st.expect(&Token::Gt)?;
        return Ok(GrammarElement::NonTerminal(name));
    }
    // @Name — binding
    if st.peek() == Some(&Token::At) {
        st.advance();
        let (name, _) = st.eat_pascal().ok_or("expected Pascal after @")?;
        return Ok(GrammarElement::Binding(name));
    }
    // | @Name — rest
    if st.peek() == Some(&Token::Pipe) {
        st.advance();
        st.skip_newlines();
        st.expect(&Token::At)?;
        let (name, _) = st.eat_pascal().ok_or("expected Pascal after | @")?;
        return Ok(GrammarElement::Rest(name));
    }
    // PascalCase — terminal
    if let Some((name, _)) = st.eat_pascal() {
        return Ok(GrammarElement::Terminal(name));
    }
    Err(format!("expected grammar element, got {:?}", st.peek()))
}

fn parse_grammar_rule(st: &mut ParseState) -> Result<Item, String> {
    let start = st.save();
    st.expect(&Token::Lt)?;
    let (name, _) = st.eat_pascal().ok_or("expected rule name")?;
    st.expect(&Token::Gt)?;
    st.skip_newlines();
    st.expect(&Token::LBracket)?;
    let mut arms = Vec::new();
    loop {
        st.skip_newlines();
        if st.peek() == Some(&Token::RBracket) {
            st.advance();
            break;
        }
        // Arm: [elements] result
        let arm_start = st.save();
        st.expect(&Token::LBracket)?;
        let mut pattern = Vec::new();
        loop {
            st.skip_newlines();
            if st.peek() == Some(&Token::RBracket) {
                st.advance();
                break;
            }
            pattern.push(parse_grammar_element(st)?);
            st.skip_newlines();
        }
        st.skip_newlines();
        let result = parse_expr(st)?;
        let arm_span = st.span_from(arm_start);
        arms.push(GrammarArm {
            pattern,
            result: vec![result],
            span: arm_span,
        });
        st.skip_newlines();
    }
    let span = st.span_from(start);
    Ok(Item::GrammarRule(GrammarRule { name, arms, span }))
}

// ── Module header parser ────────────────────────────────────────────

fn parse_module_header(st: &mut ParseState) -> Result<ModuleHeader, String> {
    let start = st.save();

    // () Sol — identity + exports
    st.expect(&Token::LParen)?;
    let (name, _) = st.eat_pascal().ok_or("expected module name")?;
    let mut exports = Vec::new();
    loop {
        st.skip_newlines();
        if st.peek() == Some(&Token::RParen) {
            st.advance();
            break;
        }
        if let Some((e, _)) = st.eat_pascal() {
            exports.push(e);
        } else if let Some((e, _)) = st.eat_camel() {
            exports.push(e);
        } else {
            break;
        }
    }

    st.skip_newlines();

    // [] Luna — imports (optional)
    let imports = if st.peek() == Some(&Token::LBracket) {
        st.advance();
        let mut imps = Vec::new();
        loop {
            st.skip_newlines();
            if st.peek() == Some(&Token::RBracket) {
                st.advance();
                break;
            }
            let imp_start = st.save();
            let (module, _) = st.eat_pascal().ok_or("expected module name in import")?;
            st.skip_newlines();
            st.expect(&Token::LParen)?;
            let items = if st.eat(&Token::Underscore) {
                ImportItems::Wildcard
            } else {
                let mut named = Vec::new();
                loop {
                    st.skip_newlines();
                    if st.peek() == Some(&Token::RParen) {
                        break;
                    }
                    if let Some((n, _)) = st.eat_pascal() {
                        named.push(n);
                    } else if let Some((n, _)) = st.eat_camel() {
                        named.push(n);
                    } else {
                        break;
                    }
                    st.skip_newlines();
                }
                ImportItems::Named(named)
            };
            st.expect(&Token::RParen)?;
            let imp_span = st.span_from(imp_start);
            imps.push(ImportEntry {
                module,
                items,
                span: imp_span,
            });
            st.skip_newlines();
        }
        imps
    } else {
        Vec::new()
    };

    st.skip_newlines();

    // {} Saturn — constraints (optional)
    let constraints = if st.peek() == Some(&Token::LBrace) {
        st.advance();
        let mut cons = Vec::new();
        loop {
            st.skip_newlines();
            if st.peek() == Some(&Token::RBrace) {
                st.advance();
                break;
            }
            if let Some((c, _)) = st.eat_pascal() {
                cons.push(c);
            } else {
                break;
            }
        }
        cons
    } else {
        Vec::new()
    };

    let span = st.span_from(start);
    Ok(ModuleHeader {
        name,
        exports,
        imports,
        constraints,
        span,
    })
}

// ── Top-level item dispatcher ───────────────────────────────────────

fn parse_item(st: &mut ParseState) -> Result<Spanned<Item>, String> {
    let start = st.save();
    st.skip_newlines();

    if st.at_end() {
        return Err("unexpected end of input".to_string());
    }

    // Const: !Name Type {value}
    if st.peek() == Some(&Token::Bang) {
        let item = parse_const_decl(st)?;
        let span = st.span_from(start);
        return Ok(Spanned::new(item, span));
    }

    // Grammar rule: <Name> [...]
    if st.peek() == Some(&Token::Lt) {
        let item = parse_grammar_rule(st)?;
        let span = st.span_from(start);
        return Ok(Spanned::new(item, span));
    }

    // Main: Main [body]
    if let Some(Token::PascalIdent(s)) = st.peek() {
        if s == "Main" {
            st.advance();
            let item = parse_main_decl(st)?;
            let span = st.span_from(start);
            return Ok(Spanned::new(item, span));
        }
    }

    // Domain: PascalName (variants)
    if let Some(Token::PascalIdent(_)) = st.peek() {
        let save = st.save();
        // Peek: Pascal then (
        let (_, _) = st.eat_pascal().unwrap();
        if st.peek() == Some(&Token::LParen) {
            st.restore(save);
            let item = parse_domain_decl(st)?;
            let span = st.span_from(start);
            return Ok(Spanned::new(item, span));
        }
        // Struct: PascalName { fields }
        if st.peek() == Some(&Token::LBrace) {
            st.restore(save);
            let item = parse_struct_decl(st)?;
            let span = st.span_from(start);
            return Ok(Spanned::new(item, span));
        }
        // Type alias: PascalName TypeRef (fallback — must come after domain/struct)
        st.restore(save);
        let item = parse_type_alias(st)?;
        let span = st.span_from(start);
        return Ok(Spanned::new(item, span));
    }

    // Trait decl or impl: camelCase
    if let Some(Token::CamelIdent(_)) = st.peek() {
        let save = st.save();
        let (name, _) = st.eat_camel().unwrap();
        st.skip_newlines();
        if st.peek() == Some(&Token::LParen) {
            // Trait declaration: name ([methods])
            st.restore(save);
            let item = parse_trait_decl(st)?;
            let span = st.span_from(start);
            return Ok(Spanned::new(item, span));
        }
        if st.peek() == Some(&Token::LBracket) {
            // Trait impl: name [Type [methods]]
            st.restore(save);
            let item = parse_impl_block(st)?;
            let span = st.span_from(start);
            return Ok(Spanned::new(item, span));
        }
        st.restore(save);
    }

    Err(format!(
        "expected item, got {:?} at position {}",
        st.peek(),
        st.pos
    ))
}

// ── Public API ──────────────────────────────────────────────────────

/// Parse aski source code using the grammar engine, producing the same AST
/// types as the chumsky parser.
pub fn parse_source(source: &str) -> Result<Vec<Spanned<Item>>, String> {
    let sf = parse_source_file(source)?;
    Ok(sf.items)
}

/// Parse a full source file (with optional module header).
pub fn parse_source_file(source: &str) -> Result<SourceFile, String> {
    let spanned_tokens = crate::lexer::lex(source).map_err(|errs| {
        errs.into_iter()
            .map(|e| e.to_string())
            .collect::<Vec<_>>()
            .join("\n")
    })?;

    let tokens: Vec<SpannedToken> = spanned_tokens
        .into_iter()
        .map(|s| SpannedToken {
            token: s.token,
            span: s.span,
        })
        .collect();

    let st = &mut ParseState::new(&tokens);
    st.skip_newlines();

    // Try parsing with module header first
    let header = {
        let save = st.save();
        if st.peek() == Some(&Token::LParen) {
            // Could be module header or domain with ( after something
            match parse_module_header(st) {
                Ok(h) => Some(h),
                Err(_) => {
                    st.restore(save);
                    None
                }
            }
        } else {
            None
        }
    };

    let mut items = Vec::new();
    loop {
        st.skip_newlines();
        if st.at_end() {
            break;
        }
        items.push(parse_item(st)?);
    }

    Ok(SourceFile { header, items })
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ge_parse_simple_domain() {
        let items = parse_source("Element (Fire Earth Air Water)").unwrap();
        assert_eq!(items.len(), 1);
        match &items[0].node {
            Item::Domain(d) => {
                assert_eq!(d.name, "Element");
                assert_eq!(d.variants.len(), 4);
                assert_eq!(d.variants[0].name, "Fire");
                assert_eq!(d.variants[3].name, "Water");
            }
            other => panic!("expected Domain, got {:?}", other),
        }
    }

    #[test]
    fn ge_parse_simple_struct() {
        let items = parse_source("Point { Horizontal F64 Vertical F64 }").unwrap();
        assert_eq!(items.len(), 1);
        match &items[0].node {
            Item::Struct(s) => {
                assert_eq!(s.name, "Point");
                assert_eq!(s.fields.len(), 2);
                assert_eq!(s.fields[0].name, "Horizontal");
            }
            other => panic!("expected Struct, got {:?}", other),
        }
    }

    #[test]
    fn ge_parse_const_decl() {
        let items = parse_source("!Pi F64 {3.14159265358979}").unwrap();
        assert_eq!(items.len(), 1);
        match &items[0].node {
            Item::Const(c) => {
                assert_eq!(c.name, "Pi");
                assert!(matches!(&c.type_ref, TypeRef::Named(n) if n == "F64"));
            }
            other => panic!("expected Const, got {:?}", other),
        }
    }

    #[test]
    fn ge_parse_main() {
        let items = parse_source("Main [ StdOut \"hello\" ]").unwrap();
        assert_eq!(items.len(), 1);
        assert!(matches!(&items[0].node, Item::Main(_)));
    }

    #[test]
    fn ge_parse_trait_decl() {
        let src = "classify ([element(:@Self) Element])";
        let items = parse_source(src).unwrap();
        assert_eq!(items.len(), 1);
        match &items[0].node {
            Item::Trait(t) => {
                assert_eq!(t.name, "classify");
                assert_eq!(t.methods.len(), 1);
                assert_eq!(t.methods[0].name, "element");
                assert!(matches!(&t.methods[0].params[0], Param::BorrowSelf));
            }
            other => panic!("expected Trait, got {:?}", other),
        }
    }

    #[test]
    fn ge_parse_trait_impl() {
        let src = "compute [Addition [ add(:@Self) U32 [ ^(@Self.Left + @Self.Right) ] ]]";
        let items = parse_source(src).unwrap();
        assert_eq!(items.len(), 1);
        match &items[0].node {
            Item::TraitImpl(ti) => {
                assert_eq!(ti.trait_name, "compute");
                assert_eq!(ti.impls.len(), 1);
                assert_eq!(ti.impls[0].target, "Addition");
                assert_eq!(ti.impls[0].methods.len(), 1);
                assert_eq!(ti.impls[0].methods[0].name, "add");
            }
            other => panic!("expected TraitImpl, got {:?}", other),
        }
    }

    #[test]
    fn ge_parse_matching_method() {
        let src = r#"describe [Element [
  describe(:@Self) Element (|
    (Fire)  Fire
    (Earth) Earth
    (Air)   Air
    (Water) Water
  |)
]]"#;
        let items = parse_source(src).unwrap();
        match &items[0].node {
            Item::TraitImpl(ti) => {
                match &ti.impls[0].methods[0].body {
                    Body::MatchBody(arms) => {
                        assert_eq!(arms.len(), 4);
                        assert!(matches!(arms[0].kind, ArmKind::Commit));
                    }
                    other => panic!("expected MatchBody, got {:?}", other),
                }
            }
            other => panic!("expected TraitImpl, got {:?}", other),
        }
    }

    #[test]
    fn ge_parse_same_type_binding() {
        let src = "Main [ @Radius.new(Value(5.0)) ]";
        let items = parse_source(src).unwrap();
        match &items[0].node {
            Item::Main(m) => {
                match &m.body {
                    Body::Block(stmts) => {
                        assert!(matches!(&stmts[0].node, Expr::SameTypeNew(name, _) if name == "Radius"));
                    }
                    other => panic!("expected Block, got {:?}", other),
                }
            }
            other => panic!("expected Main, got {:?}", other),
        }
    }

    #[test]
    fn ge_parse_subtype_binding() {
        let src = "Main [ @Area F64.new(42.0) ]";
        let items = parse_source(src).unwrap();
        match &items[0].node {
            Item::Main(m) => {
                match &m.body {
                    Body::Block(stmts) => {
                        assert!(matches!(&stmts[0].node, Expr::SubTypeNew(name, _, _) if name == "Area"));
                    }
                    other => panic!("expected Block, got {:?}", other),
                }
            }
            other => panic!("expected Main, got {:?}", other),
        }
    }

    #[test]
    fn ge_parse_type_alias() {
        let src = "SignList Vec{Sign}";
        let items = parse_source(src).unwrap();
        match &items[0].node {
            Item::TypeAlias(a) => {
                assert_eq!(a.name, "SignList");
                assert!(matches!(&a.target, TypeRef::Parameterized(n, _) if n == "Vec"));
            }
            other => panic!("expected TypeAlias, got {:?}", other),
        }
    }

    #[test]
    fn ge_parse_inline_domain() {
        let src = "Domain (One (A B C) Two)";
        let items = parse_source(src).unwrap();
        match &items[0].node {
            Item::Domain(d) => {
                assert_eq!(d.variants.len(), 2);
                assert!(d.variants[0].sub_variants.is_some());
                let subs = d.variants[0].sub_variants.as_ref().unwrap();
                assert_eq!(subs.len(), 3);
            }
            other => panic!("expected Domain, got {:?}", other),
        }
    }

    #[test]
    fn ge_parse_struct_variant() {
        let src = "Shape (Circle (F64) Rectangle { Width F64 Height F64 })";
        let items = parse_source(src).unwrap();
        match &items[0].node {
            Item::Domain(d) => {
                assert_eq!(d.variants.len(), 2);
                assert!(d.variants[0].wraps.is_some());
                assert!(d.variants[1].fields.is_some());
            }
            other => panic!("expected Domain, got {:?}", other),
        }
    }

    #[test]
    fn ge_parse_associated_type() {
        let src = r#"add [Point [
  Output Point
  add(:@Self @Rhs Point) Point [
    ^Point(Horizontal([@Self.Horizontal + @Rhs.Horizontal]) Vertical([@Self.Vertical + @Rhs.Vertical]))
  ]
]]"#;
        let items = parse_source(src).unwrap();
        match &items[0].node {
            Item::TraitImpl(ti) => {
                assert_eq!(ti.impls[0].associated_types.len(), 1);
                assert_eq!(ti.impls[0].associated_types[0].name, "Output");
                assert_eq!(ti.impls[0].methods.len(), 1);
            }
            other => panic!("expected TraitImpl, got {:?}", other),
        }
    }

    #[test]
    fn ge_parse_mutable_binding() {
        let src = "Main [ ~@Counter U32.new(0) ]";
        let items = parse_source(src).unwrap();
        match &items[0].node {
            Item::Main(m) => {
                match &m.body {
                    Body::Block(stmts) => {
                        assert!(matches!(&stmts[0].node, Expr::MutableNew(name, _, _) if name == "Counter"));
                    }
                    other => panic!("expected Block, got {:?}", other),
                }
            }
            other => panic!("expected Main, got {:?}", other),
        }
    }

    #[test]
    fn ge_parse_simple_aski_file() {
        let src = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/encoder/design/v0.9/examples/simple.aski"
        ))
        .unwrap();
        let items = parse_source(&src).unwrap();
        // simple.aski has many items — ensure we parse at least as many as chumsky
        let chumsky_items = crate::parser::parse_source(&src).unwrap();
        assert_eq!(
            items.len(),
            chumsky_items.len(),
            "grammar engine parsed {} items, chumsky parsed {} items",
            items.len(),
            chumsky_items.len(),
        );

        // Verify specific items match
        for (i, (ge, ch)) in items.iter().zip(chumsky_items.iter()).enumerate() {
            assert_eq!(
                std::mem::discriminant(&ge.node),
                std::mem::discriminant(&ch.node),
                "item {} type mismatch: ge={:?}, chumsky={:?}",
                i,
                ge.node,
                ch.node,
            );
        }
    }

    #[test]
    fn ge_parse_module_header() {
        let src = "(Chart Sign Planet computeChart)\n[Core (ParseState Token)]\n\nSign (Aries Taurus)";
        let sf = parse_source_file(src).unwrap();
        let header = sf.header.unwrap();
        assert_eq!(header.name, "Chart");
        assert_eq!(header.exports, vec!["Sign", "Planet", "computeChart"]);
        assert_eq!(header.imports.len(), 1);
        assert_eq!(header.imports[0].module, "Core");
        assert_eq!(sf.items.len(), 1);
    }

    #[test]
    fn ge_parse_grammar_rule() {
        let src = "<Truncate> [\n  [@Value] @Value.truncate\n]";
        let items = parse_source(src).unwrap();
        assert!(matches!(&items[0].node, Item::GrammarRule(gr) if gr.name == "Truncate"));
    }

    #[test]
    fn ge_parse_destructure_arm() {
        let src = "parse [Tokens [\n  parse(:@Self) Option{Pair{Token Tokens}} (|\n    [PascalIdent LParen @Variants RParen | @Rest]  @Rest\n  |)\n]]";
        let items = parse_source(src).unwrap();
        match &items[0].node {
            Item::TraitImpl(ti) => {
                match &ti.impls[0].methods[0].body {
                    Body::MatchBody(arms) => {
                        assert!(matches!(arms[0].kind, ArmKind::Destructure));
                        let (elements, rest) = arms[0].destructure.as_ref().unwrap();
                        assert_eq!(elements.len(), 4);
                        assert_eq!(rest, "Rest");
                    }
                    other => panic!("expected MatchBody, got {:?}", other),
                }
            }
            other => panic!("expected TraitImpl, got {:?}", other),
        }
    }

    #[test]
    fn ge_parse_borrowed_field() {
        let src = "Excerpt { Text :String Page U32 }";
        let items = parse_source(src).unwrap();
        match &items[0].node {
            Item::Struct(s) => {
                assert!(matches!(&s.fields[0].type_ref, TypeRef::Borrowed(_)));
            }
            other => panic!("expected Struct, got {:?}", other),
        }
    }

    #[test]
    fn ge_parse_supertraits() {
        let src = "describe (display classify [describe(:@Self) String])";
        let items = parse_source(src).unwrap();
        match &items[0].node {
            Item::Trait(t) => {
                assert_eq!(t.supertraits, vec!["display", "classify"]);
                assert_eq!(t.methods.len(), 1);
            }
            other => panic!("expected Trait, got {:?}", other),
        }
    }

    #[test]
    fn ge_parse_chart_aski_if_available() {
        let chart_src = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../astro-aski/aski/chart.aski"
        ));
        if let Ok(src) = chart_src {
            let sf = parse_source_file(&src).unwrap();
            assert!(sf.header.is_some(), "should have module header");
            // Compare with chumsky
            let chumsky_sf = crate::parser::parse_source_file(&src).unwrap();
            assert_eq!(
                sf.items.len(),
                chumsky_sf.items.len(),
                "grammar engine {} items vs chumsky {} items",
                sf.items.len(),
                chumsky_sf.items.len(),
            );
        }
    }
}
