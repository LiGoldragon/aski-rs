use chumsky::prelude::*;

use crate::ast::*;
use crate::lexer::Token;
use super::tokens::*;
use super::patterns::matching_body;
use super::statements::{body, const_body};

/// Method signature for trait declarations: `name(params) ReturnType`
pub(crate) fn method_sig() -> impl Parser<Token, MethodSig, Error = Simple<Token>> + Clone {
    camel()
        .then(
            param()
                .separated_by(skip_newlines())
                .allow_trailing()
                .delimited_by(tok(Token::LParen), tok(Token::RParen)),
        )
        .then(type_ref().or_not())
        .map_with_span(|((name, params), output), span| {
            MethodSig {
                name,
                params,
                output,
                span,
            }
        })
}

/// Method body: either computed `[...]` or matching `(| ... |)`
pub(crate) fn method_body() -> impl Parser<Token, Body, Error = Simple<Token>> + Clone {
    choice((matching_body(), body()))
}

/// Method definition for impl blocks:
/// `name(params) ReturnType [ body ]` — computed
/// `name(params) ReturnType (| arms |)` — matching
pub(crate) fn method_def() -> impl Parser<Token, MethodDef, Error = Simple<Token>> + Clone {
    camel()
        .then(
            param()
                .separated_by(skip_newlines())
                .allow_trailing()
                .delimited_by(tok(Token::LParen), tok(Token::RParen)),
        )
        .then(type_ref().or_not())
        .then(method_body())
        .map_with_span(|(((name, params), output), body), span| {
            MethodDef {
                name,
                params,
                output,
                body,
                span,
            }
        })
}

// === Top-level item parsers ===

/// Parse a single variant inside a domain declaration.
pub(crate) fn variant_parser() -> impl Parser<Token, Variant, Error = Simple<Token>> + Clone {
    let field = pascal()
        .then(field_type_ref())
        .map_with_span(|(name, tr), span| Field {
            name,
            type_ref: tr,
            span,
        });

    // Struct variant: Name { fields }
    let struct_variant = pascal()
        .then(
            skip_newlines()
                .ignore_then(field)
                .separated_by(skip_newlines())
                .at_least(1)
                .then_ignore(skip_newlines())
                .delimited_by(tok(Token::LBrace), tok(Token::RBrace)),
        )
        .map_with_span(|(name, fields), span| Variant {
            name,
            wraps: None,
            fields: Some(fields),
            sub_variants: None,
            span,
        });

    // Inline domain variant: Name (A B C) — multiple PascalCase names
    let inline_domain = pascal()
        .then(
            skip_newlines()
                .ignore_then(
                    pascal()
                        .map_with_span(|name, span| Variant {
                            name,
                            wraps: None,
                            fields: None,
                            sub_variants: None,
                            span,
                        })
                )
                .separated_by(skip_newlines())
                .at_least(2)  // 2+ names = inline domain (1 name = newtype wrap)
                .delimited_by(tok(Token::LParen), tok(Token::RParen)),
        )
        .map_with_span(|(name, sub_vars), span| Variant {
            name,
            wraps: None,
            fields: None,
            sub_variants: Some(sub_vars),
            span,
        });

    // Newtype wrap: Name (Type) — single type in parens
    let newtype_wrap = pascal()
        .then(
            type_ref()
                .delimited_by(tok(Token::LParen), tok(Token::RParen)),
        )
        .map_with_span(|(name, tr), span| Variant {
            name,
            wraps: Some(tr),
            fields: None,
            sub_variants: None,
            span,
        });

    // Unit variant: just Name
    let unit_variant = pascal()
        .map_with_span(|name, span| Variant {
            name,
            wraps: None,
            fields: None,
            sub_variants: None,
            span,
        });

    choice((
        struct_variant,
        inline_domain,
        newtype_wrap,
        unit_variant,
    ))
}

/// Domain declaration: `Name (Variant1 Variant2 ...)`
pub(crate) fn domain_decl() -> impl Parser<Token, Item, Error = Simple<Token>> + Clone {
    pascal()
        .then(
            skip_newlines()
                .ignore_then(variant_parser())
                .separated_by(skip_newlines())
                .at_least(1)
                .allow_trailing()
                .delimited_by(tok(Token::LParen), tok(Token::RParen)),
        )
        .map_with_span(|(name, variants), span| {
            Item::Domain(DomainDecl {
                name,
                variants,
                span,
            })
        })
}

/// Struct declaration: `Name { Field Type ... }`
pub(crate) fn struct_decl() -> impl Parser<Token, Item, Error = Simple<Token>> + Clone {
    let field = pascal()
        .then(field_type_ref())
        .map_with_span(|(name, tr), span| Field {
            name,
            type_ref: tr,
            span,
        });

    pascal()
        .then(
            skip_newlines()
                .ignore_then(field)
                .separated_by(skip_newlines())
                .allow_trailing()
                .then_ignore(skip_newlines())
                .delimited_by(tok(Token::LBrace), tok(Token::RBrace)),
        )
        .map_with_span(|(name, fields), span| {
            Item::Struct(StructDecl {
                name,
                fields,
                span,
            })
        })
}

/// Trait declaration: `name (supertrait1 supertrait2 [method signatures])`
/// Trait names are camelCase — they are verbs (behavior).
/// Supertraits are camelCase identifiers before the methods block.
pub(crate) fn trait_decl() -> impl Parser<Token, Item, Error = Simple<Token>> + Clone {
    let methods_block = skip_newlines()
        .ignore_then(method_sig())
        .separated_by(skip_newlines())
        .allow_trailing()
        .then_ignore(skip_newlines())
        .delimited_by(tok(Token::LBracket), tok(Token::RBracket));

    camel()
        .then(
            skip_newlines()
                .ignore_then(
                    // Optional supertraits before the methods block
                    camel().repeated()
                )
                .then(skip_newlines().ignore_then(methods_block))
                .then_ignore(skip_newlines())
                .delimited_by(tok(Token::LParen), tok(Token::RParen)),
        )
        .map_with_span(|(name, (supertraits, methods)), span| {
            Item::Trait(TraitDecl {
                name,
                supertraits,
                methods,
                span,
            })
        })
}

/// An impl member: either an associated type `PascalName Type` (consumed/discarded)
/// or a method definition (returned). Associated types are parsed and discarded
/// so they don't block parsing. TODO: proper AST storage for associated types.
fn impl_member() -> impl Parser<Token, Option<MethodDef>, Error = Simple<Token>> + Clone {
    // Associated type: PascalName PascalName — two PascalCase identifiers.
    // Method defs start with camelCase, so there's no ambiguity.
    // We parse and discard associated types for now.
    let assoc_type = pascal()
        .then(type_ref())
        .map(|_| None);

    let method = method_def().map(Some);

    choice((assoc_type, method))
}

/// Trait impl: `TraitName [TypeName [methods...]]`
/// Or inherent impl: `TypeName [methods...]`
pub(crate) fn impl_block() -> impl Parser<Token, Item, Error = Simple<Token>> + Clone {
    // Inner impl block for one type: `TypeName [method1 method2 ...]`
    // Members can be method defs or associated type defs (which are discarded).
    let type_impl = pascal()
        .then(
            skip_newlines()
                .ignore_then(impl_member())
                .separated_by(skip_newlines())
                .allow_trailing()
                .then_ignore(skip_newlines())
                .delimited_by(tok(Token::LBracket), tok(Token::RBracket)),
        )
        .map_with_span(|(target, members), span| {
            let methods: Vec<MethodDef> = members.into_iter().flatten().collect();
            TypeImpl {
                target,
                methods,
                span,
            }
        });

    // Trait impl: traitName [TypeImpl TypeImpl ...]
    let trait_impl = camel()
        .then(
            skip_newlines()
                .ignore_then(type_impl)
                .separated_by(skip_newlines())
                .allow_trailing()
                .then_ignore(skip_newlines())
                .delimited_by(tok(Token::LBracket), tok(Token::RBracket)),
        )
        .map_with_span(|(name, impls), span| {
            Item::TraitImpl(TraitImplDecl {
                trait_name: name,
                impls,
                span,
            })
        });

    let inherent_impl = pascal()
        .then(
            skip_newlines()
                .ignore_then(method_def())
                .separated_by(skip_newlines())
                .allow_trailing()
                .then_ignore(skip_newlines())
                .delimited_by(tok(Token::LBracket), tok(Token::RBracket)),
        )
        .map_with_span(|(name, methods), span| {
            Item::InherentImpl(InherentImplDecl {
                type_name: name,
                methods,
                span,
            })
        });

    choice((trait_impl, inherent_impl))
}

/// Constant declaration: `!Name Type {value}`
pub(crate) fn const_decl() -> impl Parser<Token, Item, Error = Simple<Token>> + Clone {
    tok(Token::Bang)
        .ignore_then(pascal())
        .then(type_ref())
        .then(const_body().or_not())
        .map_with_span(|((name, tr), val), span| {
            Item::Const(ConstDecl {
                name,
                type_ref: tr,
                value: val,
                span,
            })
        })
}

/// Main entry point: `Main [ body ]`
pub(crate) fn main_decl() -> impl Parser<Token, Item, Error = Simple<Token>> + Clone {
    filter(|t: &Token| *t == Token::PascalIdent("Main".into()))
        .ignore_then(body())
        .map_with_span(|body, span| {
            Item::Main(MainDecl { body, span })
        })
}

/// Type alias: `ChartResult Result{NatalChart ChartError}`
/// PascalCase name followed by a type reference, no delimiters.
pub(crate) fn type_alias() -> impl Parser<Token, Item, Error = Simple<Token>> + Clone {
    pascal()
        .then(type_ref())
        .map_with_span(|(name, target), span| {
            Item::TypeAlias(TypeAliasDecl { name, target, span })
        })
}

/// Top-level item parser.
pub(crate) fn item() -> impl Parser<Token, Spanned<Item>, Error = Simple<Token>> {
    choice((
        const_decl(),
        main_decl(),
        // Domain must come before struct because both start with PascalCase
        domain_decl(),
        struct_decl(),
        trait_decl(),
        impl_block(),
        // Type alias last — it's PascalCase PascalCase which overlaps with many things
        type_alias(),
    ))
    .map_with_span(|item, span| Spanned::new(item, span))
}
