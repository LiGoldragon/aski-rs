//! AST construction — maps grammar rule constructor names to AST nodes.
//!
//! Each constructor name used in grammar/*.aski result specs maps to
//! a function that builds a concrete AST type from resolved arguments.

use crate::ast::*;
use super::{Value, ForeignFunction};

/// Build an AST value from a constructor name and resolved arguments.
pub fn construct(name: &str, args: &[Value], span: Span) -> Result<Value, String> {
    match name {
        // ── List constructors ────────────────────────────────
        "Cons" => {
            if args.len() != 2 {
                return Err(format!("Cons requires 2 args, got {}", args.len()));
            }
            let head = args[0].clone();
            match &args[1] {
                Value::List(tail) => {
                    let mut list = vec![head];
                    list.extend(tail.iter().cloned());
                    Ok(Value::List(list))
                }
                _ => Err("Cons: second argument must be a list".to_string()),
            }
        }
        "Nil" => Ok(Value::List(vec![])),
        "Singleton" => {
            if args.len() != 1 {
                return Err(format!("Singleton requires 1 arg, got {}", args.len()));
            }
            Ok(Value::List(vec![args[0].clone()]))
        }

        // ── Param constructors ───────────────────────────────
        "BorrowSelf" => Ok(Value::Param(Param::BorrowSelf)),
        "MutBorrowSelf" => Ok(Value::Param(Param::MutBorrowSelf)),
        "OwnedSelf" => Ok(Value::Param(Param::OwnedSelf)),
        "Borrow" => Ok(Value::Param(Param::Borrow(args[0].as_str()?))),
        "MutBorrow" => Ok(Value::Param(Param::MutBorrow(args[0].as_str()?))),
        "Named" => Ok(Value::Param(Param::Named(
            args[0].as_str()?,
            args[1].as_type_ref()?,
        ))),
        "Owned" => Ok(Value::Param(Param::Owned(args[0].as_str()?))),

        // ── TypeRef constructors ─────────────────────────────
        "SelfType" => Ok(Value::TypeRef(TypeRef::SelfType)),
        "NamedType" => Ok(Value::TypeRef(TypeRef::Named(args[0].as_str()?))),
        "BorrowedType" => {
            let inner = args[0].as_type_ref()?;
            Ok(Value::TypeRef(TypeRef::Borrowed(Box::new(inner))))
        }
        "ParameterizedType" => {
            let name = args[0].as_str()?;
            let params = args[1].clone().into_list()?
                .into_iter()
                .map(|v| v.as_type_ref())
                .collect::<Result<Vec<_>, _>>()?;
            Ok(Value::TypeRef(TypeRef::Parameterized(name, params)))
        }
        "BoundType" => Ok(Value::TypeRef(TypeRef::Bound(args[0].as_trait_bound()?))),

        // ── TraitBound constructors ──────────────────────────
        "CompoundBound" => {
            let first = args[0].as_str()?;
            let rest = args[1].as_trait_bound()?;
            let mut bounds = vec![first];
            bounds.extend(rest.bounds);
            let name = bounds.join("&");
            Ok(Value::TraitBound(TraitBound {
                name,
                bounds,
                span,
            }))
        }
        "SingleBound" => {
            let name = args[0].as_str()?;
            Ok(Value::TraitBound(TraitBound {
                name: name.clone(),
                bounds: vec![name],
                span,
            }))
        }

        // ── Field constructors ───────────────────────────────
        "Field" => {
            let name = args[0].as_str()?;
            let type_ref = args[1].as_type_ref()?;
            Ok(Value::Field(Field { name, type_ref, span }))
        }
        "BorrowedField" => {
            let name = args[0].as_str()?;
            let inner_type = args[1].as_type_ref()?;
            Ok(Value::Field(Field {
                name,
                type_ref: TypeRef::Borrowed(Box::new(inner_type)),
                span,
            }))
        }

        // ── Variant constructors ─────────────────────────────
        "UnitVariant" => {
            let name = args[0].as_str()?;
            Ok(Value::Variant(Variant {
                name,
                wraps: None,
                fields: None,
                sub_variants: None,
                span,
            }))
        }
        "ParenVariant" => {
            // If inner list has 1 element that looks like a type → wrapped variant
            // If 2+ elements → inline domain with sub-variants
            let name = args[0].as_str()?;
            let inner = args[1].clone().into_list()?;
            if inner.len() == 1 {
                // Single element — try as type ref first, fall back to sub-variant
                match &inner[0] {
                    Value::Variant(v) if v.wraps.is_none() && v.fields.is_none() && v.sub_variants.is_none() => {
                        // Bare name — could be type wrap or sub-variant
                        // If it's a single bare name with no sub-structure, treat as type wrap
                        Ok(Value::Variant(Variant {
                            name,
                            wraps: Some(TypeRef::Named(v.name.clone())),
                            fields: None,
                            sub_variants: None,
                            span,
                        }))
                    }
                    _ => {
                        let sub_variants: Vec<Variant> = inner.into_iter()
                            .map(|v| v.as_variant())
                            .collect::<Result<Vec<_>, _>>()?;
                        Ok(Value::Variant(Variant {
                            name,
                            wraps: None,
                            fields: None,
                            sub_variants: Some(sub_variants),
                            span,
                        }))
                    }
                }
            } else {
                let sub_variants: Vec<Variant> = inner.into_iter()
                    .map(|v| v.as_variant())
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(Value::Variant(Variant {
                    name,
                    wraps: None,
                    fields: None,
                    sub_variants: Some(sub_variants),
                    span,
                }))
            }
        }
        "StructVariant" => {
            let name = args[0].as_str()?;
            let fields = args[1].clone().into_list()?
                .into_iter()
                .map(|v| v.as_field())
                .collect::<Result<Vec<_>, _>>()?;
            Ok(Value::Variant(Variant {
                name,
                wraps: None,
                fields: Some(fields),
                sub_variants: None,
                span,
            }))
        }

        // ── Item constructors ────────────────────────────────
        "Domain" => {
            let name = args[0].as_str()?;
            let variants = args[1].clone().into_list()?
                .into_iter()
                .map(|v| v.as_variant())
                .collect::<Result<Vec<_>, _>>()?;
            Ok(Value::Item(Spanned::new(
                Item::Domain(DomainDecl { name, variants, span: span.clone() }),
                span,
            )))
        }
        "Struct" => {
            let name = args[0].as_str()?;
            let fields = args[1].clone().into_list()?
                .into_iter()
                .map(|v| v.as_field())
                .collect::<Result<Vec<_>, _>>()?;
            Ok(Value::Item(Spanned::new(
                Item::Struct(StructDecl { name, fields, span: span.clone() }),
                span,
            )))
        }
        "TraitDecl" => {
            let name = args[0].as_str()?;
            let supertraits_val = if args.len() > 1 { args[1].clone() } else { Value::List(vec![]) };
            let members_val = if args.len() > 2 { args[2].clone() } else { Value::List(vec![]) };

            let supertraits: Vec<String> = match supertraits_val {
                Value::List(items) => items.into_iter().map(|v| v.as_str()).collect::<Result<Vec<_>, _>>()?,
                _ => vec![],
            };
            let members = match members_val {
                Value::List(items) => items,
                _ => vec![],
            };

            let mut method_sigs = Vec::new();
            let mut constants = Vec::new();
            for m in members {
                match m {
                    Value::MethodSig(s) => method_sigs.push(s),
                    Value::ConstDecl(c) => constants.push(c),
                    _ => {}
                }
            }

            Ok(Value::Item(Spanned::new(
                Item::Trait(TraitDecl { name, supertraits, methods: method_sigs, constants, span: span.clone() }),
                span,
            )))
        }
        "TraitImpl" => {
            let name = args[0].as_str()?;
            let impls = args[1].clone().into_list()?
                .into_iter()
                .map(|v| v.as_type_impl())
                .collect::<Result<Vec<_>, _>>()?;
            Ok(Value::Item(Spanned::new(
                Item::TraitImpl(TraitImplDecl { trait_name: name, impls, span: span.clone() }),
                span,
            )))
        }
        "Const" => {
            let name = args[0].as_str()?;
            let type_ref = args[1].as_type_ref()?;
            let body = if args.len() > 2 { Some(args[2].as_body()?) } else { None };
            Ok(Value::Item(Spanned::new(
                Item::Const(ConstDecl { name, type_ref, value: body, span: span.clone() }),
                span,
            )))
        }
        "Main" => {
            let body = args[0].as_body()?;
            Ok(Value::Item(Spanned::new(
                Item::Main(MainDecl { body, span: span.clone() }),
                span,
            )))
        }
        "TypeAlias" => {
            let name = args[0].as_str()?;
            let target = args[1].as_type_ref()?;
            Ok(Value::Item(Spanned::new(
                Item::TypeAlias(TypeAliasDecl { name, target, span: span.clone() }),
                span,
            )))
        }
        "GrammarRuleItem" => {
            let name = args[0].as_str()?;
            // Grammar rules are parsed by bootstrap, stored as Item::GrammarRule
            Ok(Value::Item(Spanned::new(
                Item::GrammarRule(GrammarRule {
                    name,
                    arms: vec![],
                    span: span.clone(),
                }),
                span,
            )))
        }

        // ── Module header constructors ───────────────────────
        "ModuleHeader" => {
            let name = args[0].as_str()?;
            let exports = args[1].clone().into_list()?
                .into_iter()
                .map(|v| v.as_str())
                .collect::<Result<Vec<_>, _>>()?;
            let imports = if args.len() > 2 {
                args[2].clone().into_list()?
                    .into_iter()
                    .map(|v| v.as_import_entry())
                    .collect::<Result<Vec<_>, _>>()?
            } else {
                vec![]
            };
            let constraints = if args.len() > 3 {
                args[3].clone().into_list()?
                    .into_iter()
                    .map(|v| v.as_str())
                    .collect::<Result<Vec<_>, _>>()?
            } else {
                vec![]
            };

            Ok(Value::ModuleHeader(ModuleHeader {
                name,
                exports,
                imports,
                constraints,
                span,
            }))
        }

        // ── Import constructors ──────────────────────────────
        "NamedImport" => {
            let module = args[0].as_str()?;
            let items = args[1].clone().into_list()?
                .into_iter()
                .map(|v| v.as_str())
                .collect::<Result<Vec<_>, _>>()?;
            Ok(Value::ImportEntry(ImportEntry {
                module,
                items: ImportItems::Named(items),
                span,
            }))
        }
        "WildcardImport" => {
            let module = args[0].as_str()?;
            Ok(Value::ImportEntry(ImportEntry {
                module,
                items: ImportItems::Wildcard,
                span,
            }))
        }

        // ── Foreign function constructors ────────────────────
        "ForeignFunc" => {
            let name = args[0].as_str()?;
            let params_list = args[1].clone().into_list()?
                .into_iter()
                .map(|v| v.as_param())
                .collect::<Result<Vec<_>, _>>()?;
            let return_type = args[2].as_type_ref()?;
            let extern_name = args[3].as_str()?;
            Ok(Value::ForeignFunction(ForeignFunction {
                name,
                params: params_list,
                return_type,
                extern_name,
                span,
            }))
        }
        "ForeignBlock" => {
            let library = args[0].as_str()?;
            let functions = args[1].clone().into_list()?
                .into_iter()
                .map(|v| v.as_foreign_function())
                .collect::<Result<Vec<_>, _>>()?;
            Ok(Value::Item(Spanned::new(
                Item::ForeignBlock(ForeignBlockDecl { library, functions, span: span.clone() }),
                span,
            )))
        }

        // ── Passthrough: result is a sub-rule's value directly ──
        "Passthrough" => {
            if args.len() == 1 {
                Ok(args[0].clone())
            } else {
                // Passthrough with extra args — first is the result, rest are context
                Ok(args[0].clone())
            }
        }

        // ── Pass-through: already-constructed values ─────────
        // When a constructor name matches a Value variant, wrap it
        _ => Err(format!("unknown constructor: {}", name)),
    }
}
