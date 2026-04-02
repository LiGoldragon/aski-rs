use chumsky::prelude::*;

use crate::ast::*;
use crate::lexer::Token;
use super::tokens::*;

/// Expression parser (recursive).
pub(crate) fn expr_parser() -> impl Parser<Token, Spanned<Expr>, Error = Simple<Token>> + Clone {
    recursive(|expr| {
        let int_lit = integer()
            .map_with_span(|v, span| Spanned::new(Expr::IntLit(v), span));

        let float_lit = float()
            .map_with_span(|v, span| Spanned::new(Expr::FloatLit(v), span));

        let str_lit = string_lit()
            .map_with_span(|s, span| Spanned::new(Expr::StringLit(s), span));

        let stub = tok(Token::Stub)
            .map_with_span(|_, span| Spanned::new(Expr::Stub, span));

        // Const reference: `!Name`
        let const_ref = tok(Token::Bang)
            .ignore_then(pascal())
            .map_with_span(|name, span| Spanned::new(Expr::ConstRef(name), span));

        // Return: `^expr`
        let return_expr = tok(Token::Caret)
            .ignore_then(expr.clone())
            .map_with_span(|e, span| Spanned::new(Expr::Return(Box::new(e)), span));

        // Yield: `#expr`
        let yield_expr = tok(Token::Hash)
            .ignore_then(expr.clone())
            .map_with_span(|e, span| Spanned::new(Expr::Yield(Box::new(e)), span));

        // Instance reference: `@Type` — just the @ prefix + name
        let instance_ref = tok(Token::At)
            .ignore_then(pascal())
            .map_with_span(|name, span| Spanned::new(Expr::InstanceRef(name), span));

        // Comprehension: `[| @Source expr {guard} |]`
        let comp_guard = expr.clone()
            .delimited_by(tok(Token::LBrace), tok(Token::RBrace));

        // Filter only: [| source {guard} |]
        let comp_filter = tok(Token::ComprehensionOpen)
            .ignore_then(skip_newlines())
            .ignore_then(expr.clone())
            .then(skip_newlines().ignore_then(comp_guard.clone()))
            .then_ignore(skip_newlines())
            .then_ignore(tok(Token::ComprehensionClose))
            .map_with_span(|(source, guard), span| {
                Spanned::new(Expr::Comprehension {
                    source: Box::new(source),
                    output: None,
                    guard: Some(Box::new(guard)),
                }, span)
            });

        // Map + optional guard: [| source output {guard}? |]
        let comp_map = tok(Token::ComprehensionOpen)
            .ignore_then(skip_newlines())
            .ignore_then(expr.clone())
            .then(skip_newlines().ignore_then(expr.clone()))
            .then(skip_newlines().ignore_then(comp_guard).or_not())
            .then_ignore(skip_newlines())
            .then_ignore(tok(Token::ComprehensionClose))
            .map_with_span(|((source, output), guard), span| {
                Spanned::new(Expr::Comprehension {
                    source: Box::new(source),
                    output: Some(Box::new(output)),
                    guard: guard.map(|g| Box::new(g)),
                }, span)
            });

        // Source only: [| source |]
        let comp_source = tok(Token::ComprehensionOpen)
            .ignore_then(skip_newlines())
            .ignore_then(expr.clone())
            .then_ignore(skip_newlines())
            .then_ignore(tok(Token::ComprehensionClose))
            .map_with_span(|source, span| {
                Spanned::new(Expr::Comprehension {
                    source: Box::new(source),
                    output: None,
                    guard: None,
                }, span)
            });

        let comprehension = choice((comp_filter, comp_map, comp_source));

        // Inline evaluation: `[expr expr ...]` — Luna brackets
        let inline_eval = skip_newlines()
            .ignore_then(expr.clone())
            .separated_by(skip_newlines())
            .at_least(1)
            .allow_trailing()
            .then_ignore(skip_newlines())
            .delimited_by(tok(Token::LBracket), tok(Token::RBracket))
            .map_with_span(|exprs, span| Spanned::new(Expr::InlineEval(exprs), span));

        // Universal match: `(| target (pattern) result ... |)`
        let wildcard_pat = tok(Token::Underscore)
            .map(|_| Pattern::Wildcard);

        let variant_pat = pascal()
            .map(|name| match name.as_str() {
                "True" => Pattern::BoolLit(true),
                "False" => Pattern::BoolLit(false),
                _ => Pattern::Variant(name),
            });

        let single_pattern = choice((wildcard_pat, variant_pat));

        let arm_patterns = single_pattern
            .separated_by(skip_newlines())
            .at_least(1)
            .delimited_by(tok(Token::LParen), tok(Token::RParen));

        // Match arm body: a single expression
        let match_arm = skip_newlines()
            .ignore_then(arm_patterns)
            .then(skip_newlines().ignore_then(expr.clone()))
            .map_with_span(|(patterns, body_expr), span| {
                MatchArm { patterns, body: vec![body_expr], span }
            });

        // Match target
        let match_target = tok(Token::At)
            .ignore_then(pascal())
            .map_with_span(|name, span| Spanned::new(Expr::InstanceRef(name), span));

        let match_expr = skip_newlines()
            .ignore_then(match_target.separated_by(skip_newlines()).at_least(1))
            .then(
                match_arm.separated_by(skip_newlines()).at_least(1)
            )
            .then_ignore(skip_newlines())
            .delimited_by(tok(Token::CompositionOpen), tok(Token::CompositionClose))
            .map_with_span(|(targets, arms), span| {
                Spanned::new(
                    Expr::MatchExpr(MatchExprData { targets, arms }),
                    span,
                )
            });

        // Grouping: `(expr)`
        let group = expr
            .clone()
            .delimited_by(tok(Token::LParen), tok(Token::RParen))
            .map_with_span(|e, span| Spanned::new(Expr::Group(Box::new(e)), span));

        // StdOut: `StdOut expr` — recognized by name
        let stdout = filter(|t: &Token| *t == Token::PascalIdent("StdOut".into()))
            .ignore_then(expr.clone())
            .map_with_span(|e, span| Spanned::new(Expr::StdOut(Box::new(e)), span));

        // Struct field for construction: `FieldName(value)`
        let struct_field_pair = pascal()
            .then(
                expr.clone()
                    .delimited_by(tok(Token::LParen), tok(Token::RParen)),
            );

        // Struct construction: `TypeName(Field(val) Field(val) ...)`
        let struct_construct = pascal()
            .then(
                struct_field_pair
                    .separated_by(skip_newlines())
                    .at_least(1)
                    .delimited_by(tok(Token::LParen), tok(Token::RParen)),
            )
            .map_with_span(|(name, fields), span| {
                let field_pairs: Vec<(String, Spanned<Expr>)> = fields
                    .into_iter()
                    .map(|(fname, val)| (fname, val))
                    .collect();
                Spanned::new(Expr::StructConstruct(name, field_pairs), span)
            });

        // camelCase bare — method name reference (no free function calls in v0.8)
        let camel_name = camel()
            .map_with_span(|name, span| {
                Spanned::new(Expr::BareName(name), span)
            });

        // Bare PascalCase — variant or type reference
        let pascal_bare = pascal()
            .map_with_span(|name, span| {
                Spanned::new(Expr::BareName(name), span)
            });

        // Atom — the simplest expressions
        let atom = choice((
            stub,
            const_ref,
            return_expr,
            yield_expr,
            match_expr,
            float_lit,
            int_lit,
            str_lit,
            instance_ref,
            comprehension,
            inline_eval,
            group,
            stdout,
            struct_construct,
            pascal_bare,
            camel_name,
        ));

        // Field pair for method args: FieldName(value)
        let method_field_pair = pascal()
            .then(
                expr.clone()
                    .delimited_by(tok(Token::LParen), tok(Token::RParen)),
            )
            .map_with_span(|(fname, val), span| {
                Spanned::new(Expr::StructConstruct(fname, vec![("_value".to_string(), val)]), span)
            });

        // Dot access chain: `expr.name` or `expr.name(args)` (method call with args)
        let method_or_access = tok(Token::Dot)
            .ignore_then(choice((pascal(), camel())))
            .then(
                choice((method_field_pair, expr.clone()))
                    .separated_by(skip_newlines())
                    .allow_trailing()
                    .delimited_by(tok(Token::LParen), tok(Token::RParen))
                    .or_not(),
            );

        let dotted = atom
            .then(method_or_access.repeated())
            .foldl(|base, (field, args)| {
                let span = base.span.start..base.span.end + field.len() + 1;
                if let Some(args) = args {
                    Spanned::new(Expr::MethodCall(Box::new(base), field, args), span)
                } else {
                    Spanned::new(Expr::Access(Box::new(base), field), span)
                }
            });

        // Binary operators with proper precedence (PEMDAS)
        // Level 1 (highest): *, /, %
        let mul_op = choice((
            tok(Token::Star).to(BinOp::Mul),
            tok(Token::Slash).to(BinOp::Div),
            tok(Token::Percent).to(BinOp::Rem),
        ));

        let mul_expr = dotted
            .clone()
            .then(mul_op.then(dotted).repeated())
            .foldl(|left, (op, right)| {
                let span = left.span.start..right.span.end;
                Spanned::new(Expr::BinOp(Box::new(left), op, Box::new(right)), span)
            });

        // Level 2: +, -
        let add_op = choice((
            tok(Token::Plus).to(BinOp::Add),
            tok(Token::Minus).to(BinOp::Sub),
        ));

        let add_expr = mul_expr
            .clone()
            .then(add_op.then(mul_expr).repeated())
            .foldl(|left, (op, right)| {
                let span = left.span.start..right.span.end;
                Spanned::new(Expr::BinOp(Box::new(left), op, Box::new(right)), span)
            });

        // Level 3: comparison ==, !=, <, >, <=, >=
        let cmp_op = choice((
            tok(Token::Neq).to(BinOp::Neq),
            tok(Token::Gte).to(BinOp::Gte),
            tok(Token::Lte).to(BinOp::Lte),
            tok(Token::DoubleEq).to(BinOp::Eq),
            tok(Token::Gt).to(BinOp::Gt),
            tok(Token::Lt).to(BinOp::Lt),
        ));

        let cmp_expr = add_expr
            .clone()
            .then(cmp_op.then(add_expr).repeated())
            .foldl(|left, (op, right)| {
                let span = left.span.start..right.span.end;
                Spanned::new(Expr::BinOp(Box::new(left), op, Box::new(right)), span)
            });

        // Level 4: && (and)
        let and_expr = cmp_expr
            .clone()
            .then(tok(Token::And).to(BinOp::And).then(cmp_expr).repeated())
            .foldl(|left, (op, right)| {
                let span = left.span.start..right.span.end;
                Spanned::new(Expr::BinOp(Box::new(left), op, Box::new(right)), span)
            });

        // Level 5 (lowest): || (or)
        let or_expr = and_expr
            .clone()
            .then(tok(Token::Or).to(BinOp::Or).then(and_expr).repeated())
            .foldl(|left, (op, right)| {
                let span = left.span.start..right.span.end;
                Spanned::new(Expr::BinOp(Box::new(left), op, Box::new(right)), span)
            });

        or_expr
    })
}
