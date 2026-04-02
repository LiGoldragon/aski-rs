use chumsky::prelude::*;

use crate::ast::*;
use crate::lexer::Token;
use super::tokens::*;
use super::expressions::expr_parser;

/// Pattern inside a match arm.
/// v0.9: supports or-patterns, nested patterns, instance bind, string literals.
pub(crate) fn match_pattern() -> impl Parser<Token, Pattern, Error = Simple<Token>> + Clone {
    recursive(|pat| {
        let wildcard = tok(Token::Underscore).map(|_| Pattern::Wildcard);

        let string_pat = string_lit().map(Pattern::StringLit);

        let instance_bind = tok(Token::At)
            .ignore_then(pascal())
            .map(Pattern::InstanceBind);

        // Data-carrying variant with nested pattern: `Name(@Bind)` or `Name(SubPat)`
        let data_carrying = pascal()
            .then(
                pat.clone()
                    .delimited_by(tok(Token::LParen), tok(Token::RParen)),
            )
            .map(|(name, inner)| Pattern::DataCarrying(name, Box::new(inner)));

        // Sequence destructure in parens: `(@Head | @Rest)` — @-bindings with pipe
        let seq_destructure = tok(Token::At)
            .ignore_then(pascal())
            .then(
                tok(Token::Pipe)
                    .ignore_then(tok(Token::At).ignore_then(pascal()))
            )
            .delimited_by(tok(Token::LParen), tok(Token::RParen))
            .map(|(head_name, tail_name)| Pattern::Destructure {
                head: vec![Pattern::InstanceBind(head_name)],
                tail: Some(tail_name),
            });

        // Or-pattern in parens: `(Fire | Air)`
        let or_pattern = pascal()
            .then(
                tok(Token::Pipe)
                    .ignore_then(pascal())
                    .repeated()
                    .at_least(1),
            )
            .delimited_by(tok(Token::LParen), tok(Token::RParen))
            .map(|(first, rest)| {
                let mut variants = vec![Pattern::Variant(first)];
                for name in rest {
                    variants.push(Pattern::Variant(name));
                }
                Pattern::Or(variants)
            });

        let variant = pascal().map(|name| match name.as_str() {
            "True" => Pattern::BoolLit(true),
            "False" => Pattern::BoolLit(false),
            _ => Pattern::Variant(name),
        });

        choice((wildcard, string_pat, instance_bind, seq_destructure, or_pattern, data_carrying, variant))
    })
}

/// A single match method arm: commit `(patterns) result`,
/// backtrack `[patterns] result`, or destructure `[pattern | @Rest] result`.
pub(crate) fn match_method_arm() -> impl Parser<Token, MatchMethodArm, Error = Simple<Token>> + Clone {
    let patterns = match_pattern()
        .separated_by(skip_newlines())
        .at_least(1);

    // Commit arm: (patterns) result — no guards
    let commit_arm = patterns.clone()
        .delimited_by(tok(Token::LParen), tok(Token::RParen))
        .then(skip_newlines().ignore_then(expr_parser()))
        .map_with_span(|(pats, body_expr), span| {
            MatchMethodArm {
                kind: ArmKind::Commit,
                patterns: pats,
                body: vec![body_expr],
                destructure: None,
                span,
            }
        });

    // Backtrack arm: [patterns] result — no guards
    let backtrack_arm = patterns
        .delimited_by(tok(Token::LBracket), tok(Token::RBracket))
        .then(skip_newlines().ignore_then(expr_parser()))
        .map_with_span(|(pats, body_expr), span| {
            MatchMethodArm {
                kind: ArmKind::Backtrack,
                patterns: pats,
                body: vec![body_expr],
                destructure: None,
                span,
            }
        });

    // Destructure arm: [elements | @Rest] result
    // Elements are PascalCase tokens or @Name bindings, separated by spaces
    // | @Rest is mandatory — the `|` distinguishes destructure from backtrack
    let destr_element = choice((
        tok(Token::At).ignore_then(pascal()).map(DestructureElement::Binding),
        tok(Token::Underscore).map(|_| DestructureElement::Wildcard),
        pascal().map(DestructureElement::ExactToken),
    ));

    let destructure_arm = destr_element
        .separated_by(skip_newlines())
        .at_least(1)
        .then_ignore(skip_newlines())
        .then_ignore(tok(Token::Pipe))
        .then_ignore(skip_newlines())
        .then(tok(Token::At).ignore_then(pascal()))
        .delimited_by(tok(Token::LBracket), tok(Token::RBracket))
        .then(skip_newlines().ignore_then(expr_parser()))
        .map_with_span(|((elements, rest_name), body_expr), span| {
            MatchMethodArm {
                kind: ArmKind::Destructure,
                patterns: vec![],
                body: vec![body_expr],
                destructure: Some((elements, rest_name)),
                span,
            }
        });

    // Destructure before backtrack — destructure is more specific (has `|`)
    skip_newlines().ignore_then(choice((commit_arm, destructure_arm, backtrack_arm)))
}

/// Matching body: `(| arm1 arm2 ... |)` — pattern dispatch
pub(crate) fn matching_body() -> impl Parser<Token, Body, Error = Simple<Token>> + Clone {
    match_method_arm()
        .repeated()
        .at_least(1)
        .then_ignore(skip_newlines())
        .delimited_by(tok(Token::CompositionOpen), tok(Token::CompositionClose))
        .map(Body::MatchBody)
}
