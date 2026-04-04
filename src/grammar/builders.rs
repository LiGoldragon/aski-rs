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
            // <traitBody> produces: Cons(supertrait, Cons(..., Methods(sigs))) or Methods(sigs) or Nil
            let body = if args.len() > 1 { args[1].clone() } else { Value::List(vec![]) };
            let (supertraits, method_sigs) = extract_trait_body(body)?;
            Ok(Value::Item(Spanned::new(
                Item::Trait(TraitDecl { name, supertraits, methods: method_sigs, constants: vec![], span: span.clone() }),
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
            let body = if args.len() > 2 {
                match &args[2] {
                    Value::Body(_) => Some(args[2].as_body()?),
                    Value::Expr(e) => Some(Body::Block(vec![e.clone()])),
                    _ => None,
                }
            } else { None };
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
        "Passthrough" => Ok(args[0].clone()),

        // ── Expression constructors ───────────────────────────

        // FoldLeft: base expr + list of Op(op_str, rhs_expr) pairs → left-assoc BinOp chain
        "FoldLeft" => {
            let ops = args[1].clone().into_list()?;
            if ops.is_empty() { return Ok(args[0].clone()); }
            let mut lhs = args[0].as_expr()?;
            for op_pair in ops {
                let pair = match op_pair { Value::List(v) => v, _ => return Err("FoldLeft: op must be list".into()) };
                let op_str = pair[0].as_str()?;
                let rhs = pair[1].as_expr()?;
                let binop = str_to_binop(&op_str)?;
                let sp = lhs.span.start..rhs.span.end;
                lhs = Spanned::new(Expr::BinOp(Box::new(lhs), binop, Box::new(rhs)), sp);
            }
            Ok(Value::Expr(lhs))
        }
        // Op: (op_string, rhs_expr) pair for FoldLeft
        "Op" => Ok(Value::List(vec![args[0].clone(), args[1].clone()])),

        // FoldPost: base expr + list of postfix operations → left-assoc chain
        "FoldPost" => {
            let ops = args[1].clone().into_list()?;
            if ops.is_empty() { return Ok(args[0].clone()); }
            let mut expr = args[0].as_expr()?;
            for op in ops {
                let parts = match op { Value::List(v) => v, _ => return Err("FoldPost: op must be list".into()) };
                let tag = parts[0].as_str()?;
                match tag.as_str() {
                    "method" => {
                        let name = parts[1].as_str()?;
                        let call_args = parts[2].clone().into_list()?
                            .into_iter().map(|v| v.as_expr()).collect::<Result<Vec<_>, _>>()?;
                        let end = call_args.last().map(|a| a.span.end).unwrap_or(expr.span.end);
                        let sp = expr.span.start..end;
                        expr = Spanned::new(Expr::MethodCall(Box::new(expr), name, call_args), sp);
                    }
                    "access" => {
                        let name = parts[1].as_str()?;
                        let sp = expr.span.clone();
                        expr = Spanned::new(Expr::Access(Box::new(expr), name), sp);
                    }
                    "error_prop" => {
                        let sp = expr.span.clone();
                        expr = Spanned::new(Expr::ErrorProp(Box::new(expr)), sp);
                    }
                    "range_excl" => {
                        let end_expr = parts[1].as_expr()?;
                        let sp = expr.span.start..end_expr.span.end;
                        expr = Spanned::new(Expr::Range {
                            start: Box::new(expr), end: Box::new(end_expr), inclusive: false,
                        }, sp);
                    }
                    "range_incl" => {
                        let end_expr = parts[1].as_expr()?;
                        let sp = expr.span.start..end_expr.span.end;
                        expr = Spanned::new(Expr::Range {
                            start: Box::new(expr), end: Box::new(end_expr), inclusive: true,
                        }, sp);
                    }
                    _ => return Err(format!("FoldPost: unknown op tag: {}", tag)),
                }
            }
            Ok(Value::Expr(expr))
        }
        // Postfix op constructors — produce tagged lists for FoldPost
        "MethodOp" => Ok(Value::List(vec![Value::Str("method".into()), args[0].clone(), args[1].clone()])),
        "AccessOp" => Ok(Value::List(vec![Value::Str("access".into()), args[0].clone()])),
        "ErrorPropOp" => Ok(Value::List(vec![Value::Str("error_prop".into())])),
        "RangeExclOp" => Ok(Value::List(vec![Value::Str("range_excl".into()), args[0].clone()])),
        "RangeInclOp" => Ok(Value::List(vec![Value::Str("range_incl".into()), args[0].clone()])),

        // Atom constructors
        "ExprStub" => Ok(Value::Expr(Spanned::new(Expr::Stub, span))),
        "ConstRef" => Ok(Value::Expr(Spanned::new(Expr::ConstRef(args[0].as_str()?), span))),
        "Return" => {
            let inner = args[0].as_expr()?;
            Ok(Value::Expr(Spanned::new(Expr::Return(Box::new(inner)), span)))
        }
        "Yield" => {
            let inner = args[0].as_expr()?;
            Ok(Value::Expr(Spanned::new(Expr::Yield(Box::new(inner)), span)))
        }
        "InstanceRef" => Ok(Value::Expr(Spanned::new(Expr::InstanceRef(args[0].as_str()?), span))),
        "InlineEval" => {
            let stmts = args[0].clone().into_list()?
                .into_iter().map(|v| v.as_expr()).collect::<Result<Vec<_>, _>>()?;
            Ok(Value::Expr(Spanned::new(Expr::InlineEval(stmts), span)))
        }
        "Group" => {
            let inner = args[0].as_expr()?;
            Ok(Value::Expr(Spanned::new(Expr::Group(Box::new(inner)), span)))
        }
        "StdOut" => {
            let inner = args[0].as_expr()?;
            Ok(Value::Expr(Spanned::new(Expr::StdOut(Box::new(inner)), span)))
        }
        "BareName" => Ok(Value::Expr(Spanned::new(Expr::BareName(args[0].as_str()?), span))),
        "FnCall" => {
            let type_name = args[0].as_str()?;
            let method_name = args[1].as_str()?;
            let call_args = args[2].clone().into_list()?
                .into_iter().map(|v| v.as_expr()).collect::<Result<Vec<_>, _>>()?;
            Ok(Value::Expr(Spanned::new(Expr::FnCall(
                format!("{}/{}", type_name, method_name), call_args), span)))
        }
        "TypePath" => {
            // Name/Variant → Access(BareName(Name), Variant)
            let type_name = args[0].as_str()?;
            let variant = args[1].as_str()?;
            let base = Spanned::new(Expr::BareName(type_name), span.clone());
            Ok(Value::Expr(Spanned::new(Expr::Access(Box::new(base), variant), span)))
        }
        "BareTrue" => Ok(Value::Expr(Spanned::new(Expr::BareName("True".into()), span))),
        "BareFalse" => Ok(Value::Expr(Spanned::new(Expr::BareName("False".into()), span))),
        "ErrorProp" => {
            let inner = args[0].as_expr()?;
            Ok(Value::Expr(Spanned::new(Expr::ErrorProp(Box::new(inner)), span)))
        }
        "Literal" => {
            match &args[0] {
                Value::Int(n) => Ok(Value::Expr(Spanned::new(Expr::IntLit(*n), span))),
                Value::Float(f) => Ok(Value::Expr(Spanned::new(Expr::FloatLit(*f), span))),
                Value::Str(s) => Ok(Value::Expr(Spanned::new(Expr::StringLit(s.clone()), span))),
                _ => Err("Literal: unsupported value type".into()),
            }
        }
        "StructConstruct" => {
            let name = args[0].as_str()?;
            let field_vals = args[1].clone().into_list()?;
            let mut fields = Vec::new();
            for fv in field_vals {
                match fv {
                    // StructField produces a (name, expr) pair
                    Value::List(pair) if pair.len() == 2 => {
                        let fname = pair[0].as_str()?;
                        let fexpr = pair[1].as_expr()?;
                        fields.push((fname, fexpr));
                    }
                    // Bare expression (positional arg)
                    _ => {
                        let e = fv.as_expr()?;
                        fields.push((String::new(), e));
                    }
                }
            }
            Ok(Value::Expr(Spanned::new(Expr::StructConstruct(name, fields), span)))
        }
        "StructField" => {
            // Produce a (name, expr) pair for StructConstruct
            let name = args[0].as_str()?;
            let val = args[1].as_expr()?;
            Ok(Value::List(vec![Value::Str(name), Value::Expr(val)]))
        }

        // ── Statement constructors ────────────────────────────

        "ExprStmt" => Ok(args[0].clone()),
        "MutableNew" => {
            let name = args[0].as_str()?;
            let type_ref = args[1].as_type_ref()?;
            let init_args = args[2].clone().into_list()?
                .into_iter().map(|v| v.as_expr()).collect::<Result<Vec<_>, _>>()?;
            Ok(Value::Expr(Spanned::new(Expr::MutableNew(name, type_ref, init_args), span)))
        }
        "SubTypeNew" => {
            let name = args[0].as_str()?;
            let type_ref = args[1].as_type_ref()?;
            let init_args = args[2].clone().into_list()?
                .into_iter().map(|v| v.as_expr()).collect::<Result<Vec<_>, _>>()?;
            Ok(Value::Expr(Spanned::new(Expr::SubTypeNew(name, type_ref, init_args), span)))
        }
        "SameTypeNew" => {
            let name = args[0].as_str()?;
            let init_args = args[1].clone().into_list()?
                .into_iter().map(|v| v.as_expr()).collect::<Result<Vec<_>, _>>()?;
            Ok(Value::Expr(Spanned::new(Expr::SameTypeNew(name, init_args), span)))
        }
        "DeferredNew" => {
            let name = args[0].as_str()?;
            let init_args = args[1].clone().into_list()?
                .into_iter().map(|v| v.as_expr()).collect::<Result<Vec<_>, _>>()?;
            Ok(Value::Expr(Spanned::new(Expr::DeferredNew(name, init_args), span)))
        }
        "SubTypeDecl" => {
            let name = args[0].as_str()?;
            let type_ref = args[1].as_type_ref()?;
            Ok(Value::Expr(Spanned::new(Expr::SubTypeDecl(name, type_ref), span)))
        }
        "MutableSet" => {
            // name + set chain value
            let name = args[0].as_str()?;
            let chain_val = args[1].as_expr()?;
            Ok(Value::Expr(Spanned::new(Expr::MutableSet(name, Box::new(chain_val)), span)))
        }

        // Set chain constructors
        "Set" => Ok(args[0].clone()),
        "Extend" => Ok(args[0].clone()),
        "Chain" => {
            // name.rest → Access(rest, name)
            let name = args[0].as_str()?;
            let rest = args[1].as_expr()?;
            Ok(Value::Expr(Spanned::new(Expr::Access(Box::new(rest), name), span)))
        }

        // ── Body constructors ─────────────────────────────────

        "Block" => {
            let stmts = args[0].clone().into_list()?
                .into_iter().map(|v| v.as_expr()).collect::<Result<Vec<_>, _>>()?;
            Ok(Value::Body(Body::Block(stmts)))
        }
        "TailBlock" => {
            let stmts = args[0].clone().into_list()?
                .into_iter().map(|v| v.as_expr()).collect::<Result<Vec<_>, _>>()?;
            Ok(Value::Body(Body::TailBlock(stmts)))
        }
        "MatchBody" => {
            let arms = args[0].clone().into_list()?
                .into_iter().map(|v| v.as_match_method_arm()).collect::<Result<Vec<_>, _>>()?;
            Ok(Value::Body(Body::MatchBody(arms)))
        }
        "Stub" => Ok(Value::Body(Body::Stub)),

        // ── Match arm constructors ────────────────────────────

        "CommitArm" => {
            let pat = args[0].as_pattern()?;
            let body_expr = args[1].as_expr()?;
            Ok(Value::MatchMethodArm(MatchMethodArm {
                kind: ArmKind::Commit,
                patterns: vec![pat],
                body: vec![body_expr],
                destructure: None,
                span,
            }))
        }
        "BacktrackArm" => {
            let pat = args[0].as_pattern()?;
            let body_expr = args[1].as_expr()?;
            Ok(Value::MatchMethodArm(MatchMethodArm {
                kind: ArmKind::Backtrack,
                patterns: vec![pat],
                body: vec![body_expr],
                destructure: None,
                span,
            }))
        }

        // ── Pattern constructors ──────────────────────────────

        "Wildcard" => Ok(Value::Pattern(Pattern::Wildcard)),
        "BoolTrue" => Ok(Value::Pattern(Pattern::BoolLit(true))),
        "BoolFalse" => Ok(Value::Pattern(Pattern::BoolLit(false))),
        "VariantPat" => Ok(Value::Pattern(Pattern::Variant(args[0].as_str()?))),
        "InstanceBind" => Ok(Value::Pattern(Pattern::InstanceBind(args[0].as_str()?))),
        "DataCarrying" => {
            let name = args[0].as_str()?;
            let inner = args[1].as_pattern()?;
            Ok(Value::Pattern(Pattern::DataCarrying(name, Box::new(inner))))
        }
        "LiteralPat" => {
            match &args[0] {
                Value::Str(s) => Ok(Value::Pattern(Pattern::StringLit(s.clone()))),
                Value::Int(n) => Ok(Value::Pattern(Pattern::Variant(n.to_string()))),
                _ => Err("LiteralPat: unsupported".into()),
            }
        }

        // ── Match expression constructors ──────────────────���──

        "MatchExpr" => {
            let target = args[0].as_expr()?;
            let arms = args[1].clone().into_list()?
                .into_iter().map(|v| v.as_match_method_arm()).collect::<Result<Vec<_>, _>>()?;
            let match_arms: Vec<MatchArm> = arms.into_iter().map(|mma| {
                MatchArm { patterns: mma.patterns, body: mma.body, span: mma.span }
            }).collect();
            Ok(Value::Expr(Spanned::new(Expr::MatchExpr(MatchExprData {
                targets: vec![target],
                arms: match_arms,
            }), span)))
        }

        // ── Supertrait ────────────────────────────────────────

        "Supertrait" => Ok(Value::Str(args[0].as_str()?)),

        // ── Methods list wrapper ──────────────────────────────

        "Methods" => Ok(args[0].clone()),

        // ── Method signatures ─────────────────────────────────

        "MethodSig" => {
            let name = args[0].as_str()?;
            let params = args[1].clone().into_list()?
                .into_iter().map(|v| v.as_param()).collect::<Result<Vec<_>, _>>()?;
            let output = if args.len() > 2 {
                Some(args[2].as_type_ref()?)
            } else {
                None
            };
            Ok(Value::MethodSig(MethodSig { name, params, output, span }))
        }

        // ── Type implementation ──────────────────────────────

        "TypeImpl" => {
            let target = args[0].as_str()?;
            let members = if args.len() > 1 { args[1].clone().into_list()? } else { vec![] };
            let mut methods = Vec::new();
            let mut associated_types = Vec::new();
            let mut associated_constants = Vec::new();
            for m in members {
                match m {
                    Value::MethodDef(d) => methods.push(d),
                    Value::AssociatedTypeDef(t) => associated_types.push(t),
                    Value::ConstDecl(c) => associated_constants.push(c),
                    _ => {}
                }
            }
            Ok(Value::TypeImpl(TypeImpl { target, methods, associated_types, associated_constants, span }))
        }

        // ── Method definition ────────────────────────────────

        "MethodDef" => {
            let name = args[0].as_str()?;
            let params = args[1].clone().into_list()?
                .into_iter().map(|v| v.as_param()).collect::<Result<Vec<_>, _>>()?;
            let (output, body) = if args.len() == 4 {
                (Some(args[2].as_type_ref()?), args[3].as_body()?)
            } else {
                (None, args[2].as_body()?)
            };
            Ok(Value::MethodDef(MethodDef { name, params, output, body, span }))
        }

        // ── Associated type ──────────────────────────────────

        "AssociatedType" => {
            let name = args[0].as_str()?;
            let concrete_type = args[1].as_type_ref()?;
            Ok(Value::AssociatedTypeDef(AssociatedTypeDef { name, concrete_type, span }))
        }

        // ── Destructure arm ──────────────────────────────────

        "DestructureArm" => {
            let destructure_val = args[0].clone();
            let body_expr = args[1].as_expr()?;
            // Destructure is a Cons/Rest list of elements
            let (elems, rest_name) = parse_destructure_list(destructure_val)?;
            Ok(Value::MatchMethodArm(MatchMethodArm {
                kind: ArmKind::Destructure,
                patterns: vec![],
                body: vec![body_expr],
                destructure: Some((elems, rest_name)),
                span,
            }))
        }

        "Elem" => Ok(Value::Str(args[0].as_str()?)),
        "Rest" => Ok(Value::Str(args[0].as_str()?)),

        // ── Set chain method call ────────────────────────────

        "MethodCall" => {
            let name = args[0].as_str()?;
            let call_args = if args.len() > 1 {
                args[1].clone().into_list()?
                    .into_iter().map(|v| v.as_expr()).collect::<Result<Vec<_>, _>>()?
            } else {
                vec![]
            };
            Ok(Value::Expr(Spanned::new(
                Expr::MethodCall(
                    Box::new(Spanned::new(Expr::BareName("_chain".into()), span.clone())),
                    name,
                    call_args,
                ),
                span,
            )))
        }

        // ── Grammar rule self-definition ─────────────────────

        "GrammarRule" => {
            let name = args[0].as_str()?;
            Ok(Value::Item(Spanned::new(
                Item::GrammarRule(GrammarRule {
                    name,
                    arms: vec![],
                    span: span.clone(),
                }),
                span,
            )))
        }
        "RuleArm" => Ok(Value::List(args.to_vec())),
        "NonTerminal" => Ok(Value::Str(args[0].as_str()?)),
        "Terminal" => Ok(Value::Str(args[0].as_str()?)),
        "Binding" => Ok(Value::Str(args[0].as_str()?)),
        "RestBind" => Ok(Value::Str(args[0].as_str()?)),
        "Bound" => Ok(Value::Str(args[0].as_str()?)),
        "RuleRef" => Ok(Value::Str(args[0].as_str()?)),
        "Constructor" => {
            let name = args[0].as_str()?;
            let ctor_args = if args.len() > 1 { args[1..].to_vec() } else { vec![] };
            Ok(Value::List(vec![Value::Str(name), Value::List(ctor_args)]))
        }
        "Nested" => {
            let name = args[0].as_str()?;
            let nested_args = if args.len() > 1 { args[1..].to_vec() } else { vec![] };
            Ok(Value::List(vec![Value::Str(name), Value::List(nested_args)]))
        }

        // ── Pass-through: already-constructed values ─────────
        _ => Err(format!("unknown constructor: {}", name)),
    }
}

fn str_to_binop(s: &str) -> Result<crate::ast::BinOp, String> {
    use crate::ast::BinOp;
    match s {
        "+" => Ok(BinOp::Addition),
        "-" => Ok(BinOp::Subtraction),
        "*" => Ok(BinOp::Multiplication),
        "/" => Ok(BinOp::Division),
        "%" => Ok(BinOp::Remainder),
        "==" => Ok(BinOp::Equal),
        "!=" => Ok(BinOp::NotEqual),
        "<" => Ok(BinOp::LessThan),
        ">" => Ok(BinOp::GreaterThan),
        "<=" => Ok(BinOp::LessThanOrEqual),
        ">=" => Ok(BinOp::GreaterThanOrEqual),
        "&&" => Ok(BinOp::LogicalAnd),
        "||" => Ok(BinOp::LogicalOr),
        _ => Err(format!("unknown operator: {}", s)),
    }
}

/// Walk <traitBody> flat list into (supertraits, method_sigs).
/// Cons flattens, so the result is [Str("super1"), Str("super2"), MethodSig, MethodSig, ...].
fn extract_trait_body(val: Value) -> Result<(Vec<String>, Vec<MethodSig>), String> {
    match val {
        Value::List(items) => {
            let mut supertraits = Vec::new();
            let mut method_sigs = Vec::new();
            for item in items {
                match item {
                    Value::Str(s) => supertraits.push(s),
                    Value::MethodSig(s) => method_sigs.push(s),
                    _ => {}
                }
            }
            Ok((supertraits, method_sigs))
        }
        Value::None => Ok((vec![], vec![])),
        _ => Ok((vec![], vec![])),
    }
}

/// Walk a Cons/Rest list from <destructure> grammar rule into DestructureElements + rest name.
fn parse_destructure_list(val: Value) -> Result<(Vec<DestructureElement>, String), String> {
    let mut elems = Vec::new();
    let mut current = val;
    loop {
        match current {
            Value::List(ref items) if items.len() == 2 => {
                // Cons(Elem(@Name), rest)
                let head = items[0].as_str()?;
                elems.push(DestructureElement::Binding(head));
                current = items[1].clone();
            }
            Value::Str(rest_name) => {
                // Rest(@Name) — the tail binding
                return Ok((elems, rest_name));
            }
            _ => return Err(format!("unexpected destructure element: {:?}", current)),
        }
    }
}
