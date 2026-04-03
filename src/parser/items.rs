use chumsky::prelude::*;

use crate::ast::*;
use crate::lexer::Token;
use super::tokens::*;
use super::patterns::matching_body;
use super::statements::{body, const_body, tail_body};

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

/// Method body: computed `[...]`, matching `(| ... |)`, or tail-recursive `[| ... |]`
pub(crate) fn method_body() -> impl Parser<Token, Body, Error = Simple<Token>> + Clone {
    choice((matching_body(), tail_body(), body()))
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

/// A trait member: either a method signature or an associated constant.
#[derive(Debug, Clone)]
enum TraitMember {
    Method(MethodSig),
    Const(ConstDecl),
}

/// Trait declaration: `name (supertrait1 supertrait2 [method signatures / constants])`
/// Trait names are camelCase — they are verbs (behavior).
/// Supertraits are camelCase identifiers before the methods block.
pub(crate) fn trait_decl() -> impl Parser<Token, Item, Error = Simple<Token>> + Clone {
    let trait_const = tok(Token::Bang)
        .ignore_then(pascal())
        .then(type_ref())
        .then(const_body().or_not())
        .map_with_span(|((name, tr), val), span| {
            TraitMember::Const(ConstDecl {
                name,
                type_ref: tr,
                value: val,
                span,
            })
        });

    let trait_method = method_sig().map(TraitMember::Method);

    let members_block = skip_newlines()
        .ignore_then(choice((trait_const, trait_method)))
        .separated_by(skip_newlines())
        .allow_trailing()
        .then_ignore(skip_newlines())
        .delimited_by(tok(Token::LBracket), tok(Token::RBracket));

    camel()
        .then(
            skip_newlines()
                .ignore_then(
                    // Optional supertraits before the members block
                    camel().repeated()
                )
                .then(skip_newlines().ignore_then(members_block))
                .then_ignore(skip_newlines())
                .delimited_by(tok(Token::LParen), tok(Token::RParen)),
        )
        .map_with_span(|(name, (supertraits, members)), span| {
            let mut methods = Vec::new();
            let mut constants = Vec::new();
            for member in members {
                match member {
                    TraitMember::Method(m) => methods.push(m),
                    TraitMember::Const(c) => constants.push(c),
                }
            }
            Item::Trait(TraitDecl {
                name,
                supertraits,
                methods,
                constants,
                span,
            })
        })
}

/// An impl member: associated type, associated constant, or method definition.
#[derive(Debug, Clone)]
enum ImplMember {
    AssocType(AssociatedTypeDef),
    AssocConst(ConstDecl),
    Method(MethodDef),
}

fn impl_member() -> impl Parser<Token, ImplMember, Error = Simple<Token>> + Clone {
    // Associated constant: `!Name Type {value}` — starts with Bang, no ambiguity.
    let assoc_const = tok(Token::Bang)
        .ignore_then(pascal())
        .then(type_ref())
        .then(const_body().or_not())
        .map_with_span(|((name, tr), val), span| ImplMember::AssocConst(ConstDecl {
            name,
            type_ref: tr,
            value: val,
            span,
        }));

    // Associated type: PascalName Type — PascalCase name followed by a type ref.
    // Method defs start with camelCase, so there's no ambiguity.
    let assoc_type = pascal()
        .then(type_ref())
        .map_with_span(|(name, concrete_type), span| ImplMember::AssocType(AssociatedTypeDef {
            name,
            concrete_type,
            span,
        }));

    let method = method_def().map(ImplMember::Method);

    choice((assoc_const, assoc_type, method))
}

/// Trait impl: `traitName [TypeName [methods...]]`
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
            let mut methods = Vec::new();
            let mut associated_types = Vec::new();
            let mut associated_constants = Vec::new();
            for member in members {
                match member {
                    ImplMember::Method(m) => methods.push(m),
                    ImplMember::AssocType(a) => associated_types.push(a),
                    ImplMember::AssocConst(c) => associated_constants.push(c),
                }
            }
            TypeImpl {
                target,
                methods,
                associated_types,
                associated_constants,
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

    trait_impl
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

// ═══════════════════════════════════════════════════════════════
// Grammar rule parsing
// ═══════════════════════════════════════════════════════════════

/// Grammar element inside an arm pattern.
/// - `<Name>` → NonTerminal
/// - `@Name` → Binding
/// - PascalIdent → Terminal
fn grammar_element() -> impl Parser<Token, GrammarElement, Error = Simple<Token>> + Clone {
    let non_terminal = tok(Token::Lt)
        .ignore_then(pascal())
        .then_ignore(tok(Token::Gt))
        .map(GrammarElement::NonTerminal);

    let binding = tok(Token::At)
        .ignore_then(pascal())
        .map(GrammarElement::Binding);

    let terminal = pascal().map(GrammarElement::Terminal);

    choice((non_terminal, binding, terminal))
}

/// Grammar arm: `[ elements ] result_expr` or `[ elements | @Rest ] result_expr`
fn grammar_arm() -> impl Parser<Token, GrammarArm, Error = Simple<Token>> + Clone {
    let rest_binding = tok(Token::Pipe)
        .ignore_then(skip_newlines())
        .ignore_then(tok(Token::At))
        .ignore_then(pascal())
        .map(GrammarElement::Rest);

    let pattern = skip_newlines()
        .ignore_then(grammar_element())
        .separated_by(skip_newlines())
        .at_least(1)
        .then(skip_newlines().ignore_then(rest_binding).or_not())
        .then_ignore(skip_newlines())
        .delimited_by(tok(Token::LBracket), tok(Token::RBracket))
        .map(|(mut elems, rest)| {
            if let Some(r) = rest {
                elems.push(r);
            }
            elems
        });

    let result_expr = skip_newlines()
        .ignore_then(super::expressions::expr_parser());

    pattern
        .then(result_expr)
        .map_with_span(|(pattern, result), span| {
            GrammarArm { pattern, result: vec![result], span }
        })
}

/// Grammar rule: `<Name> [ arms ]`
pub(crate) fn grammar_rule() -> impl Parser<Token, Item, Error = Simple<Token>> + Clone {
    tok(Token::Lt)
        .ignore_then(pascal())
        .then_ignore(tok(Token::Gt))
        .then_ignore(skip_newlines())
        .then(
            skip_newlines()
                .ignore_then(grammar_arm())
                .separated_by(skip_newlines())
                .at_least(1)
                .allow_trailing()
                .then_ignore(skip_newlines())
                .delimited_by(tok(Token::LBracket), tok(Token::RBracket)),
        )
        .map_with_span(|(name, arms), span| {
            Item::GrammarRule(GrammarRule { name, arms, span })
        })
}

/// Top-level item parser.
pub(crate) fn item() -> impl Parser<Token, Spanned<Item>, Error = Simple<Token>> {
    choice((
        const_decl(),
        main_decl(),
        // Grammar rule before domain — starts with `<` which is unambiguous
        grammar_rule(),
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
