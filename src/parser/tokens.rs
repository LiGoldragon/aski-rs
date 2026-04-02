use chumsky::prelude::*;

use crate::ast::*;
use crate::lexer::Token;

/// Create a parser that matches a specific token.
pub(crate) fn tok(expected: Token) -> impl Parser<Token, Token, Error = Simple<Token>> + Clone {
    filter(move |t: &Token| *t == expected).labelled("token")
}

/// Match a PascalCase identifier and extract its string.
pub(crate) fn pascal() -> impl Parser<Token, String, Error = Simple<Token>> + Clone {
    filter_map(|span, t: Token| match t {
        Token::PascalIdent(s) => Ok(s),
        _ => Err(Simple::custom(span, "expected PascalCase identifier")),
    })
}

/// Match a camelCase identifier and extract its string.
pub(crate) fn camel() -> impl Parser<Token, String, Error = Simple<Token>> + Clone {
    filter_map(|span, t: Token| match t {
        Token::CamelIdent(s) => Ok(s),
        _ => Err(Simple::custom(span, "expected camelCase identifier")),
    })
}

/// Match an integer literal.
pub(crate) fn integer() -> impl Parser<Token, i64, Error = Simple<Token>> + Clone {
    filter_map(|span, t: Token| match t {
        Token::Integer(v) => Ok(v),
        _ => Err(Simple::custom(span, "expected integer")),
    })
}

/// Match a float literal.
pub(crate) fn float() -> impl Parser<Token, f64, Error = Simple<Token>> + Clone {
    filter_map(|span, t: Token| match t {
        Token::Float(s) => s
            .parse::<f64>()
            .map_err(|_| Simple::custom(span, "invalid float")),
        _ => Err(Simple::custom(span, "expected float")),
    })
}

/// Match a string literal.
pub(crate) fn string_lit() -> impl Parser<Token, String, Error = Simple<Token>> + Clone {
    filter_map(|span, t: Token| match t {
        Token::StringLit(s) => Ok(s),
        _ => Err(Simple::custom(span, "expected string literal")),
    })
}

/// Skip optional newlines.
pub(crate) fn skip_newlines() -> impl Parser<Token, (), Error = Simple<Token>> + Clone {
    tok(Token::Newline).repeated().ignored()
}

/// Type reference parser.
pub(crate) fn type_ref() -> impl Parser<Token, TypeRef, Error = Simple<Token>> + Clone {
    let self_type = filter(|t: &Token| *t == Token::PascalIdent("Self".into()))
        .map(|_| TypeRef::SelfType);

    // Parameterized type: Name{T U ...} — no space before {
    let parameterized = pascal()
        .then(
            recursive(|tr| {
                let inner_self = filter(|t: &Token| *t == Token::PascalIdent("Self".into()))
                    .map(|_| TypeRef::SelfType);
                let inner_param = pascal()
                    .then(
                        tr.clone()
                            .repeated()
                            .at_least(1)
                            .delimited_by(tok(Token::LBrace), tok(Token::RBrace))
                            .or_not()
                    )
                    .map(|(name, params)| {
                        if let Some(ps) = params {
                            TypeRef::Parameterized(name, ps)
                        } else if name == "Self" {
                            TypeRef::SelfType
                        } else {
                            TypeRef::Named(name)
                        }
                    });
                choice((inner_self, inner_param))
            })
            .repeated()
            .at_least(1)
            .delimited_by(tok(Token::LBrace), tok(Token::RBrace))
        )
        .map(|(name, params)| TypeRef::Parameterized(name, params));

    let named = pascal().map(|name| {
        if name == "Self" {
            TypeRef::SelfType
        } else {
            TypeRef::Named(name)
        }
    });

    choice((self_type, parameterized, named))
}

/// Type reference parser for struct fields — supports `:Type` borrowed types.
pub(crate) fn field_type_ref() -> impl Parser<Token, TypeRef, Error = Simple<Token>> + Clone {
    let borrowed = tok(Token::Colon)
        .ignore_then(type_ref())
        .map(|inner| TypeRef::Borrowed(Box::new(inner)));

    choice((borrowed, type_ref()))
}

/// Parameter parser.
/// `:@Self` — borrow self
/// `~@Self` — mut borrow self
/// `:@Type` — borrow type
/// `~@Type` — mut borrow type
/// `@Type` — owned param (the type is a struct name)
/// `@Name Type` — named param with explicit type
pub(crate) fn param() -> impl Parser<Token, Param, Error = Simple<Token>> + Clone {
    // `:@Self`
    let borrow_self = tok(Token::Colon)
        .ignore_then(tok(Token::At))
        .ignore_then(filter(|t: &Token| *t == Token::PascalIdent("Self".into())))
        .map(|_| Param::BorrowSelf);

    // `~@Self`
    let mut_borrow_self = tok(Token::Tilde)
        .ignore_then(tok(Token::At))
        .ignore_then(filter(|t: &Token| *t == Token::PascalIdent("Self".into())))
        .map(|_| Param::MutBorrowSelf);

    // `:@Type` (non-Self borrow)
    let borrow_type = tok(Token::Colon)
        .ignore_then(tok(Token::At))
        .ignore_then(pascal())
        .map(|name| {
            if name == "Self" {
                Param::BorrowSelf
            } else {
                Param::Borrow(name)
            }
        });

    // `~@Type` (non-Self mut borrow)
    let mut_borrow_type = tok(Token::Tilde)
        .ignore_then(tok(Token::At))
        .ignore_then(pascal())
        .map(|name| {
            if name == "Self" {
                Param::MutBorrowSelf
            } else {
                Param::MutBorrow(name)
            }
        });

    // `@Self` — owned/consumed self (move semantics, no borrow prefix)
    let owned_self = tok(Token::At)
        .ignore_then(filter(|t: &Token| *t == Token::PascalIdent("Self".into())))
        .map(|_| Param::OwnedSelf);

    // `@Name Type` — named param: @Other Point
    let named_param = tok(Token::At)
        .ignore_then(pascal())
        .then(type_ref())
        .map(|(name, tr)| Param::Named(name, tr));

    // `@Type` — owned param (just a type name)
    let owned_param = tok(Token::At)
        .ignore_then(pascal())
        .map(Param::Owned);

    choice((
        borrow_self,
        mut_borrow_self,
        borrow_type,
        mut_borrow_type,
        owned_self,
        named_param,
        owned_param,
    ))
}
