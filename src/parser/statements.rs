use chumsky::prelude::*;

use crate::ast::*;
use crate::lexer::Token;
use super::tokens::*;
use super::expressions::expr_parser;

/// Statement in a function body.
/// Can be: binding declaration `@Name Type`, binding new `@Name.new([expr])`, or expression.
pub(crate) fn statement() -> impl Parser<Token, Spanned<Expr>, Error = Simple<Token>> + Clone {
    let expr = expr_parser();

    // v0.8 binding forms:

    // Struct field pair inside .new(): `FieldName(value)`
    let new_field_pair = pascal()
        .then(
            expr.clone()
                .delimited_by(tok(Token::LParen), tok(Token::RParen)),
        )
        .map_with_span(|(fname, val), span| {
            Spanned::new(Expr::StructConstruct(fname, vec![("_value".to_string(), val)]), span)
        });

    // Sub-type binding: `@Name Type.new(args)` — creates wrapper
    let subtype_new = tok(Token::At)
        .ignore_then(pascal())
        .then(type_ref())
        .then_ignore(tok(Token::Dot))
        .then_ignore(filter(|t: &Token| *t == Token::CamelIdent("new".into())))
        .then(
            choice((new_field_pair.clone(), expr.clone()))
                .separated_by(skip_newlines())
                .allow_trailing()
                .delimited_by(tok(Token::LParen), tok(Token::RParen)),
        )
        .map_with_span(|((name, tr), args), span| {
            Spanned::new(Expr::SubTypeNew(name, tr, args), span)
        });

    // Same-type binding: `@Name.new(args)` — name matches existing type
    // Inside .new(), args are either field pairs FieldName(val) or plain expressions
    let same_type_new = tok(Token::At)
        .ignore_then(pascal())
        .then_ignore(tok(Token::Dot))
        .then_ignore(filter(|t: &Token| *t == Token::CamelIdent("new".into())))
        .then(
            choice((new_field_pair, expr.clone()))
                .separated_by(skip_newlines())
                .allow_trailing()
                .delimited_by(tok(Token::LParen), tok(Token::RParen)),
        )
        .map_with_span(|(name, args), span| {
            Spanned::new(Expr::SameTypeNew(name, args), span)
        });

    // Mutable set: `~@Name.set(expr)`
    let mutable_set = tok(Token::Tilde)
        .ignore_then(tok(Token::At))
        .ignore_then(pascal())
        .then_ignore(tok(Token::Dot))
        .then_ignore(filter(|t: &Token| *t == Token::CamelIdent("set".into())))
        .then(
            expr.clone()
                .delimited_by(tok(Token::LParen), tok(Token::RParen)),
        )
        .map_with_span(|(name, val), span| {
            Spanned::new(Expr::MutableSet(name, Box::new(val)), span)
        });

    // Mutable binding: `~@Name Type.new(args)`
    let mutable_new = tok(Token::Tilde)
        .ignore_then(tok(Token::At))
        .ignore_then(pascal())
        .then(type_ref())
        .then_ignore(tok(Token::Dot))
        .then_ignore(filter(|t: &Token| *t == Token::CamelIdent("new".into())))
        .then(
            expr.clone()
                .separated_by(skip_newlines())
                .allow_trailing()
                .delimited_by(tok(Token::LParen), tok(Token::RParen)),
        )
        .map_with_span(|((name, tr), args), span| {
            Spanned::new(Expr::MutableNew(name, tr, args), span)
        });

    // Two-step declaration: `Name Type` (no @)
    let subtype_decl = pascal()
        .then(type_ref())
        .map_with_span(|(name, tr), span| {
            Spanned::new(Expr::SubTypeDecl(name, tr), span)
        });

    choice((
        mutable_set,
        mutable_new,
        subtype_new,
        same_type_new,
        subtype_decl,
        expr,
    ))
}

/// Function/method body: `[ stmts ]` or `[ ___ ]`
pub(crate) fn body() -> impl Parser<Token, Body, Error = Simple<Token>> + Clone {
    let stub_body = tok(Token::Stub).map(|_| Body::Stub);

    let block_body = skip_newlines()
        .ignore_then(
            statement()
                .separated_by(skip_newlines())
                .allow_trailing(),
        )
        .then_ignore(skip_newlines())
        .map(Body::Block);

    choice((stub_body, block_body))
        .delimited_by(tok(Token::LBracket), tok(Token::RBracket))
}

/// Const value body: `{value}` — uses braces
pub(crate) fn const_body() -> impl Parser<Token, Body, Error = Simple<Token>> + Clone {
    let block_body = skip_newlines()
        .ignore_then(
            expr_parser()
                .separated_by(skip_newlines())
                .allow_trailing(),
        )
        .then_ignore(skip_newlines())
        .map(Body::Block);

    block_body
        .delimited_by(tok(Token::LBrace), tok(Token::RBrace))
}
