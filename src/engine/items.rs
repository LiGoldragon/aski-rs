//! Item parsers: domains, structs, traits, impls, const, main, type alias, grammar rules, methods.

use crate::ast::*;
use crate::lexer::Token;
use super::state::ParseState;
use super::types::{parse_type_ref, parse_field_type_ref, parse_param};
use super::expr::parse_expr;
use super::stmt::{parse_body, parse_const_body};

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

// ── Variant parser ──────────────────────────────────────────────────

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

// ── Item parsers ────────────────────────────────────────────────────

pub(crate) fn parse_domain_decl(st: &mut ParseState) -> Result<Item, String> {
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

pub(crate) fn parse_struct_decl(st: &mut ParseState) -> Result<Item, String> {
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

pub(crate) fn parse_const_decl(st: &mut ParseState) -> Result<Item, String> {
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

pub(crate) fn parse_main_decl(st: &mut ParseState) -> Result<Item, String> {
    let start = st.save();
    // "Main" already matched by caller
    let body = parse_body(st)?;
    let span = st.span_from(start);
    Ok(Item::Main(MainDecl { body, span }))
}

pub(crate) fn parse_trait_decl(st: &mut ParseState) -> Result<Item, String> {
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

pub(crate) fn parse_impl_block(st: &mut ParseState) -> Result<Item, String> {
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

pub(crate) fn parse_type_alias(st: &mut ParseState) -> Result<Item, String> {
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
    if st.peek() == Some(&Token::LessThan) {
        st.advance();
        let (name, _) = st.eat_pascal().ok_or("expected Pascal in non-terminal")?;
        st.expect(&Token::GreaterThan)?;
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

pub(crate) fn parse_grammar_rule(st: &mut ParseState) -> Result<Item, String> {
    let start = st.save();
    st.expect(&Token::LessThan)?;
    let (name, _) = st.eat_pascal().ok_or("expected rule name")?;
    st.expect(&Token::GreaterThan)?;
    st.skip_newlines();
    st.expect(&Token::LBrace)?;
    let mut arms = Vec::new();
    loop {
        st.skip_newlines();
        if st.peek() == Some(&Token::RBrace) {
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
