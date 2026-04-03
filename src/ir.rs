#![allow(non_snake_case)]

use std::collections::HashSet;

use crate::ast::*;

// ═══════════════════════════════════════════════════════════════
// Re-exports from aski-core — the shared Kernel schema
// ═══════════════════════════════════════════════════════════════

pub use aski_core::{
    World, IdGen, ParsedPattern, parse_pattern_string, run_rules,
};

// ═══════════════════════════════════════════════════════════════
// Type ref formatting (depends on aski-rs AST types)
// ═══════════════════════════════════════════════════════════════

/// Format a TypeRef as a string for storage.
pub fn type_ref_to_string(tr: &TypeRef) -> String {
    match tr {
        TypeRef::Named(n) => n.clone(),
        TypeRef::Parameterized(n, params) => {
            let ps: Vec<String> = params.iter().map(type_ref_to_string).collect();
            format!("{}({})", n, ps.join(" "))
        }
        TypeRef::SelfType => "Self".to_string(),
        TypeRef::Borrowed(inner) => format!("&{}", type_ref_to_string(inner)),
        TypeRef::Bound(tb) => tb.bounds.join("&"),
    }
}

// ═══════════════════════════════════════════════════════════════
// Pattern utilities (depends on aski-rs AST types)
// ═══════════════════════════════════════════════════════════════

/// Serialize a pattern to a string for storage.
fn pattern_to_string(pat: &Pattern) -> String {
    match pat {
        Pattern::Variant(name) => name.clone(),
        Pattern::BoolLit(b) => if *b { "True".to_string() } else { "False".to_string() },
        Pattern::Wildcard => "_".to_string(),
        Pattern::Or(pats) => pats.iter().map(pattern_to_string).collect::<Vec<_>>().join("|"),
        Pattern::Destructure { head, tail } => {
            let h = head.iter().map(pattern_to_string).collect::<Vec<_>>().join(" ");
            match tail {
                Some(t) => format!("{} | @{}", h, t),
                None => h,
            }
        }
        Pattern::DataCarrying(name, inner) => format!("{}({})", name, pattern_to_string(inner)),
        Pattern::StringLit(s) => format!("\"{}\"", s),
        Pattern::InstanceBind(name) => format!("@{}", name),
    }
}

// ═══════════════════════════════════════════════════════════════
// Grammar rule serialization
// ═══════════════════════════════════════════════════════════════

/// Serialize grammar pattern elements to a JSON string for storage.
fn serialize_grammar_pattern(elements: &[GrammarElement]) -> String {
    let strs: Vec<String> = elements.iter().map(|e| match e {
        GrammarElement::Terminal(name) => format!("T:{name}"),
        GrammarElement::NonTerminal(name) => format!("NT:{name}"),
        GrammarElement::Binding(name) => format!("B:{name}"),
        GrammarElement::Rest(name) => format!("R:{name}"),
    }).collect();
    serde_json::to_string(&strs).unwrap_or_else(|_| "[]".to_string())
}

/// Serialize grammar result expressions to a JSON string for storage.
/// For now, stores a simplified representation of the result expressions.
fn serialize_grammar_result(exprs: &[Spanned<Expr>]) -> String {
    let strs: Vec<String> = exprs.iter().map(|e| format!("{:?}", e.node)).collect();
    serde_json::to_string(&strs).unwrap_or_else(|_| "[]".to_string())
}

// ═══════════════════════════════════════════════════════════════
// Insert functions (depend on aski-rs AST types)
// ═══════════════════════════════════════════════════════════════

/// Insert a parsed AST into the World.
pub fn insert_ast(
    world: &mut World,
    items: &[Spanned<Item>],
) -> Result<(), String> {
    let mut ids = IdGen::new();
    for item in items {
        insert_item(world, &mut ids, &item.node, None, None)?;
    }
    Ok(())
}

/// Insert AST items with an ID offset to avoid collisions in multi-file compilation.
/// Returns the number of IDs used.
pub fn insert_ast_with_offset(
    world: &mut World,
    items: &[Spanned<Item>],
    offset: i64,
    scope_id: Option<i64>,
) -> Result<i64, String> {
    let mut ids = IdGen { next: offset + 1 };
    for item in items {
        insert_item(world, &mut ids, &item.node, None, scope_id)?;
    }
    Ok(ids.next - offset - 1)
}

/// Insert a module header into the World, creating a Scope node and
/// populating Export and Import relations.
/// Returns the scope_id.
pub fn insert_module_header(
    world: &mut World,
    ids: &mut IdGen,
    header: &ModuleHeader,
) -> i64 {
    let scope_id = ids.next();

    // Create scope node
    world.Scope.push((scope_id, "module".to_string(), header.name.clone(), None));

    // Store exports
    for export in &header.exports {
        world.Export.push((scope_id, export.clone()));
    }

    // Store imports
    for import in &header.imports {
        match &import.items {
            ImportItems::Named(items) => {
                for item in items {
                    world.Import.push((scope_id, import.module.clone(), item.clone()));
                }
            }
            ImportItems::Wildcard => {
                world.Import.push((scope_id, import.module.clone(), "_".to_string()));
            }
        }
    }

    scope_id
}

fn insert_item(
    world: &mut World,
    ids: &mut IdGen,
    item: &Item,
    parent: Option<i64>,
    scope_id: Option<i64>,
) -> Result<i64, String> {
    match item {
        Item::Domain(d) => insert_domain(world, ids, d, parent, scope_id),
        Item::Struct(s) => insert_struct(world, ids, s, parent, scope_id),
        Item::Trait(t) => insert_trait(world, ids, t, parent, scope_id),
        Item::TraitImpl(ti) => insert_trait_impl(world, ids, ti, parent, scope_id),
        Item::Const(c) => insert_const(world, ids, c, parent, scope_id),
        Item::Main(m) => insert_main(world, ids, m, parent, scope_id),
        Item::TypeAlias(ta) => {
            let id = ids.next();
            let span = &ta.span;
            insert_node(world, id, "type_alias", &ta.name, parent, span, scope_id);
            let aliased = type_ref_to_string(&ta.target);
            world.Returns.push((id, aliased));
            Ok(id)
        }
        Item::GrammarRule(gr) => {
            let id = ids.next();
            insert_node(world, id, "grammar_rule", &gr.name, parent, &gr.span, scope_id);
            world.GrammarRule.push((id, gr.name.clone()));
            for (i, arm) in gr.arms.iter().enumerate() {
                let pattern_json = serialize_grammar_pattern(&arm.pattern);
                let result_json = serialize_grammar_result(&arm.result);
                world.GrammarArm.push((id, i as i64, pattern_json, result_json));
            }
            Ok(id)
        }
    }
}

fn insert_node(
    world: &mut World,
    id: i64,
    kind: &str,
    name: &str,
    parent: Option<i64>,
    span: &Span,
    scope_id: Option<i64>,
) {
    world.Node.push((id, kind.to_string(), name.to_string(), parent, span.start, span.end, scope_id));
}

fn insert_domain(
    world: &mut World,
    ids: &mut IdGen,
    domain: &DomainDecl,
    parent: Option<i64>,
    scope_id: Option<i64>,
) -> Result<i64, String> {
    let id = ids.next();
    insert_node(world, id, "domain", &domain.name, parent, &domain.span, scope_id);

    for (ordinal, variant) in domain.variants.iter().enumerate() {
        let wraps = variant.wraps.as_ref().map(type_ref_to_string);
        world.Variant.push((id, ordinal as i64, variant.name.clone(), wraps));

        // Struct variant fields
        if let Some(ref fields) = variant.fields {
            let variant_id = ids.next();
            insert_node(world, variant_id, "struct", &variant.name, Some(id), &variant.span, scope_id);
            for (ford, field) in fields.iter().enumerate() {
                let tr = type_ref_to_string(&field.type_ref);
                world.Field.push((variant_id, ford as i64, field.name.clone(), tr));
            }
        }

        // Inline domain variant — sub-variants form a nested domain
        if let Some(ref sub_vars) = variant.sub_variants {
            let sub_domain_id = ids.next();
            insert_node(world, sub_domain_id, "domain", &variant.name, Some(id), &variant.span, scope_id);
            for (sord, sub_var) in sub_vars.iter().enumerate() {
                let sw = sub_var.wraps.as_ref().map(type_ref_to_string);
                world.Variant.push((sub_domain_id, sord as i64, sub_var.name.clone(), sw));
            }
        }
    }

    Ok(id)
}

fn insert_struct(
    world: &mut World,
    ids: &mut IdGen,
    s: &StructDecl,
    parent: Option<i64>,
    scope_id: Option<i64>,
) -> Result<i64, String> {
    let id = ids.next();
    insert_node(world, id, "struct", &s.name, parent, &s.span, scope_id);

    for (ordinal, field) in s.fields.iter().enumerate() {
        let tr = type_ref_to_string(&field.type_ref);
        world.Field.push((id, ordinal as i64, field.name.clone(), tr));
    }

    Ok(id)
}

/// Insert a Param into the param relation.
fn insert_param(
    world: &mut World,
    node_id: i64,
    ordinal: usize,
    param: &Param,
) {
    let (kind, name, type_ref): (&str, Option<String>, Option<String>) = match param {
        Param::Owned(t) => ("owned", None, Some(t.clone())),
        Param::Named(n, tr) => ("named", Some(n.clone()), Some(type_ref_to_string(tr))),
        Param::BorrowSelf => ("borrow_self", None, None),
        Param::MutBorrowSelf => ("mut_borrow_self", None, None),
        Param::OwnedSelf => ("owned_self", None, None),
        Param::Borrow(t) => ("borrow", None, Some(t.clone())),
        Param::MutBorrow(t) => ("mut_borrow", None, Some(t.clone())),
    };

    world.Param.push((node_id, ordinal as i64, kind.to_string(), name, type_ref));
}

fn insert_return_type(
    world: &mut World,
    node_id: i64,
    tr: &TypeRef,
) {
    let type_str = type_ref_to_string(tr);
    world.Returns.push((node_id, type_str));
}

fn insert_trait(
    world: &mut World,
    ids: &mut IdGen,
    t: &TraitDecl,
    parent: Option<i64>,
    scope_id: Option<i64>,
) -> Result<i64, String> {
    let id = ids.next();
    insert_node(world, id, "trait", &t.name, parent, &t.span, scope_id);

    for st in &t.supertraits {
        world.Supertrait.push((id, st.clone()));
    }

    for method in &t.methods {
        insert_method_sig(world, ids, method, id, scope_id)?;
    }

    for constant in &t.constants {
        insert_const(world, ids, constant, Some(id), scope_id)?;
    }

    Ok(id)
}

fn insert_method_sig(
    world: &mut World,
    ids: &mut IdGen,
    method: &MethodSig,
    parent_id: i64,
    scope_id: Option<i64>,
) -> Result<i64, String> {
    let id = ids.next();
    insert_node(world, id, "method_sig", &method.name, Some(parent_id), &method.span, scope_id);

    for (ordinal, param) in method.params.iter().enumerate() {
        insert_param(world, id, ordinal, param);
    }
    if let Some(ref output) = method.output {
        insert_return_type(world, id, output);
    }

    Ok(id)
}

fn insert_trait_impl(
    world: &mut World,
    ids: &mut IdGen,
    ti: &TraitImplDecl,
    parent: Option<i64>,
    scope_id: Option<i64>,
) -> Result<i64, String> {
    let id = ids.next();
    insert_node(world, id, "impl", &ti.trait_name, parent, &ti.span, scope_id);

    for type_impl in &ti.impls {
        let impl_id = ids.next();
        insert_node(world, impl_id, "impl_body", &type_impl.target, Some(id), &type_impl.span, scope_id);

        world.TraitImpl.push((ti.trait_name.clone(), type_impl.target.clone(), impl_id));

        // Store associated types as assoc_type child nodes
        for assoc in &type_impl.associated_types {
            let assoc_id = ids.next();
            insert_node(world, assoc_id, "assoc_type", &assoc.name, Some(impl_id), &assoc.span, scope_id);
            let type_str = type_ref_to_string(&assoc.concrete_type);
            world.Returns.push((assoc_id, type_str));
        }

        // Store associated constants as const child nodes
        for constant in &type_impl.associated_constants {
            insert_const(world, ids, constant, Some(impl_id), scope_id)?;
        }

        for method in &type_impl.methods {
            insert_method_def(world, ids, method, impl_id, scope_id)?;
        }
    }

    Ok(id)
}

fn insert_method_def(
    world: &mut World,
    ids: &mut IdGen,
    method: &MethodDef,
    parent_id: i64,
    scope_id: Option<i64>,
) -> Result<i64, String> {
    let id = ids.next();
    let kind = if matches!(method.body, Body::TailBlock(_)) { "tail_method" } else { "method" };
    insert_node(world, id, kind, &method.name, Some(parent_id), &method.span, scope_id);

    for (ordinal, param) in method.params.iter().enumerate() {
        insert_param(world, id, ordinal, param);
    }
    if let Some(ref output) = method.output {
        insert_return_type(world, id, output);
    }

    insert_body(world, ids, &method.body, id)?;

    Ok(id)
}

fn insert_const(
    world: &mut World,
    ids: &mut IdGen,
    c: &ConstDecl,
    parent: Option<i64>,
    scope_id: Option<i64>,
) -> Result<i64, String> {
    let id = ids.next();
    insert_node(world, id, "const", &c.name, parent, &c.span, scope_id);

    let tr = type_ref_to_string(&c.type_ref);
    let has_value = c.value.is_some();

    world.Constant.push((id, c.name.clone(), tr, has_value));

    if let Some(body) = &c.value {
        insert_body(world, ids, body, id)?;
    }

    Ok(id)
}

fn insert_main(
    world: &mut World,
    ids: &mut IdGen,
    m: &MainDecl,
    parent: Option<i64>,
    scope_id: Option<i64>,
) -> Result<i64, String> {
    let id = ids.next();
    insert_node(world, id, "main", "Main", parent, &m.span, scope_id);

    insert_body(world, ids, &m.body, id)?;

    Ok(id)
}

/// Insert a function/method body into the expr relation.
fn insert_body(
    world: &mut World,
    ids: &mut IdGen,
    body: &Body,
    owner_id: i64,
) -> Result<(), String> {
    match body {
        Body::Stub => {
            let id = ids.next();
            world.Expr.push((id, Some(owner_id), "stub".to_string(), 0, None));
        }
        Body::Block(stmts) | Body::TailBlock(stmts) => {
            for (ordinal, stmt) in stmts.iter().enumerate() {
                insert_expr(world, ids, &stmt.node, owner_id, ordinal as i64)?;
            }
        }
        Body::MatchBody(arms) => {
            // Store matching method arms as match_arm rows
            for (arm_ord, arm) in arms.iter().enumerate() {
                let patterns: Vec<String> = if arm.kind == ArmKind::Destructure {
                    if let Some((ref elements, ref rest_name)) = arm.destructure {
                        let elem_strs: Vec<String> = elements.iter().map(|e| match e {
                            DestructureElement::ExactToken(name) => name.clone(),
                            DestructureElement::Binding(name) => format!("@{}", name),
                            DestructureElement::Wildcard => "_".to_string(),
                        }).collect();
                        vec![format!("destr:{}|{}", elem_strs.join(","), rest_name)]
                    } else {
                        vec![]
                    }
                } else {
                    arm.patterns.iter().map(pattern_to_string).collect()
                };
                let patterns_json = serde_json::to_string(&patterns).unwrap();

                let body_expr_id = if arm.body.len() == 1 {
                    let bid = insert_expr(world, ids, &arm.body[0].node, owner_id, arm_ord as i64)?;
                    Some(bid)
                } else if arm.body.is_empty() {
                    None
                } else {
                    let wrapper_id = ids.next();
                    put_expr(world, wrapper_id, owner_id, "inline_eval", arm_ord as i64, None);
                    for (i, e) in arm.body.iter().enumerate() {
                        insert_expr(world, ids, &e.node, wrapper_id, i as i64)?;
                    }
                    Some(wrapper_id)
                };

                let kind_str = match arm.kind {
                    ArmKind::Commit => "commit",
                    ArmKind::Backtrack => "backtrack",
                    ArmKind::Destructure => "destructure",
                };
                world.MatchArm.push((
                    owner_id,
                    arm_ord as i64,
                    patterns_json,
                    body_expr_id,
                    kind_str.to_string(),
                ));
            }
        }
    }
    Ok(())
}

/// Insert an expression into the expr relation. Returns the assigned expr id.
fn insert_expr(
    world: &mut World,
    ids: &mut IdGen,
    expr: &Expr,
    parent_id: i64,
    ordinal: i64,
) -> Result<i64, String> {
    let id = ids.next();

    match expr {
        Expr::IntLit(v) => {
            put_expr(world, id, parent_id, "int_lit", ordinal, Some(&v.to_string()));
        }
        Expr::FloatLit(v) => {
            put_expr(world, id, parent_id, "float_lit", ordinal, Some(&format_float(*v)));
        }
        Expr::StringLit(s) => {
            put_expr(world, id, parent_id, "string_lit", ordinal, Some(s));
        }
        Expr::ConstRef(name) => {
            put_expr(world, id, parent_id, "const_ref", ordinal, Some(name));
        }
        Expr::InstanceRef(name) => {
            put_expr(world, id, parent_id, "instance_ref", ordinal, Some(name));
        }
        Expr::Return(inner) => {
            put_expr(world, id, parent_id, "return", ordinal, None);
            insert_expr(world, ids, &inner.node, id, 0)?;
        }
        Expr::BinOp(left, op, right) => {
            let op_str = binop_to_string(op);
            put_expr(world, id, parent_id, "binop", ordinal, Some(op_str));
            insert_expr(world, ids, &left.node, id, 0)?;
            insert_expr(world, ids, &right.node, id, 1)?;
        }
        Expr::Group(inner) => {
            put_expr(world, id, parent_id, "group", ordinal, None);
            insert_expr(world, ids, &inner.node, id, 0)?;
        }
        Expr::InlineEval(exprs) => {
            put_expr(world, id, parent_id, "inline_eval", ordinal, None);
            for (i, e) in exprs.iter().enumerate() {
                insert_expr(world, ids, &e.node, id, i as i64)?;
            }
        }
        Expr::Access(base, field) => {
            put_expr(world, id, parent_id, "access", ordinal, Some(field));
            insert_expr(world, ids, &base.node, id, 0)?;
        }
        Expr::SameTypeNew(name, args) => {
            put_expr(world, id, parent_id, "same_type_new", ordinal, Some(name));
            for (i, arg) in args.iter().enumerate() {
                insert_expr(world, ids, &arg.node, id, i as i64)?;
            }
        }
        Expr::SubTypeNew(name, tr, args) => {
            let type_str = type_ref_to_string(tr);
            let val = format!("{}:{}", name, type_str);
            put_expr(world, id, parent_id, "sub_type_new", ordinal, Some(&val));
            for (i, arg) in args.iter().enumerate() {
                insert_expr(world, ids, &arg.node, id, i as i64)?;
            }
        }
        Expr::MutableNew(name, tr, args) => {
            let type_str = type_ref_to_string(tr);
            let val = format!("{}:{}", name, type_str);
            put_expr(world, id, parent_id, "mutable_new", ordinal, Some(&val));
            for (i, arg) in args.iter().enumerate() {
                insert_expr(world, ids, &arg.node, id, i as i64)?;
            }
        }
        Expr::MutableSet(name, value) => {
            put_expr(world, id, parent_id, "mutable_set", ordinal, Some(name));
            insert_expr(world, ids, &value.node, id, 0)?;
        }
        Expr::SubTypeDecl(name, tr) => {
            let type_str = type_ref_to_string(tr);
            let val = format!("{}:{}", name, type_str);
            put_expr(world, id, parent_id, "sub_type_decl", ordinal, Some(&val));
        }
        Expr::DeferredNew(name, args) => {
            put_expr(world, id, parent_id, "deferred_new", ordinal, Some(name));
            for (i, arg) in args.iter().enumerate() {
                insert_expr(world, ids, &arg.node, id, i as i64)?;
            }
        }
        Expr::MatchExpr(data) => {
            put_expr(world, id, parent_id, "match", ordinal, None);
            for (i, target) in data.targets.iter().enumerate() {
                insert_expr(world, ids, &target.node, id, i as i64)?;
            }
            let target_count = data.targets.len() as i64;
            for (arm_ord, arm) in data.arms.iter().enumerate() {
                let patterns: Vec<String> = arm.patterns.iter().map(pattern_to_string).collect();
                let patterns_json = serde_json::to_string(&patterns).unwrap();

                let body_expr_id = if arm.body.len() == 1 {
                    let bid = insert_expr(world, ids, &arm.body[0].node, id, target_count + arm_ord as i64)?;
                    Some(bid)
                } else if arm.body.is_empty() {
                    None
                } else {
                    let wrapper_id = ids.next();
                    put_expr(world, wrapper_id, id, "inline_eval", target_count + arm_ord as i64, None);
                    for (i, e) in arm.body.iter().enumerate() {
                        insert_expr(world, ids, &e.node, wrapper_id, i as i64)?;
                    }
                    Some(wrapper_id)
                };

                let kind_str = "commit"; // inline match expressions always commit
                world.MatchArm.push((
                    id,
                    arm_ord as i64,
                    patterns_json,
                    body_expr_id,
                    kind_str.to_string(),
                ));
            }
        }
        Expr::StdOut(inner) => {
            put_expr(world, id, parent_id, "stdout", ordinal, None);
            insert_expr(world, ids, &inner.node, id, 0)?;
        }
        Expr::Stub => {
            put_expr(world, id, parent_id, "stub", ordinal, None);
        }
        Expr::BareName(name) => {
            put_expr(world, id, parent_id, "bare_name", ordinal, Some(name));
        }
        Expr::ErrorProp(inner) => {
            put_expr(world, id, parent_id, "error_prop", ordinal, None);
            insert_expr(world, ids, &inner.node, id, 0)?;
        }
        Expr::StructConstruct(name, fields) => {
            put_expr(world, id, parent_id, "struct_construct", ordinal, Some(name));
            for (i, (fname, val)) in fields.iter().enumerate() {
                let field_id = ids.next();
                put_expr(world, field_id, id, "struct_field", i as i64, Some(fname));
                insert_expr(world, ids, &val.node, field_id, 0)?;
            }
        }
        Expr::MethodCall(base, method, args) => {
            put_expr(world, id, parent_id, "method_call", ordinal, Some(method));
            insert_expr(world, ids, &base.node, id, 0)?;
            for (i, arg) in args.iter().enumerate() {
                insert_expr(world, ids, &arg.node, id, (i + 1) as i64)?;
            }
        }
        Expr::Range { start, end, inclusive } => {
            let kind = if *inclusive { "range_inclusive" } else { "range_exclusive" };
            put_expr(world, id, parent_id, kind, ordinal, None);
            insert_expr(world, ids, &start.node, id, 0)?;
            insert_expr(world, ids, &end.node, id, 1)?;
        }
        Expr::Yield(inner) => {
            put_expr(world, id, parent_id, "yield", ordinal, None);
            insert_expr(world, ids, &inner.node, id, 0)?;
        }
    }

    Ok(id)
}

/// Helper to put a single expr row.
fn put_expr(
    world: &mut World,
    id: i64,
    parent_id: i64,
    kind: &str,
    ordinal: i64,
    value: Option<&str>,
) {
    world.Expr.push((id, Some(parent_id), kind.to_string(), ordinal, value.map(|s| s.to_string())));
}

fn binop_to_string(op: &BinOp) -> &'static str {
    match op {
        BinOp::Add => "+",
        BinOp::Sub => "-",
        BinOp::Mul => "*",
        BinOp::Div => "/",
        BinOp::Rem => "%",
        BinOp::Eq => "==",
        BinOp::Neq => "!=",
        BinOp::Lt => "<",
        BinOp::Gt => ">",
        BinOp::Lte => "<=",
        BinOp::Gte => ">=",
        BinOp::And => "&&",
        BinOp::Or => "||",
    }
}

fn format_float(v: f64) -> String {
    let s = format!("{v}");
    if s.contains('.') { s } else { format!("{s}.0") }
}

// ═══════════════════════════════════════════════════════════════
// Create + run
// ═══════════════════════════════════════════════════════════════

/// Create a new World, analogous to the old create_db().
pub fn create_world() -> World {
    World::default()
}

// ═══════════════════════════════════════════════════════════════
// Query functions — Result-wrapping shims over aski-core
//
// Codegen uses `?` on these, so we keep the Result<T, String>
// signatures. The underlying aski-core functions never fail.
// ═══════════════════════════════════════════════════════════════

pub fn query_all_top_level_nodes(world: &World) -> Result<Vec<(i64, String, String)>, String> {
    Ok(aski_core::query_all_top_level_nodes(world))
}

pub fn query_child_nodes(world: &World, parent_id: i64) -> Result<Vec<(i64, String, String)>, String> {
    Ok(aski_core::query_child_nodes(world, parent_id))
}

pub fn query_domain_variants(world: &World, domain_name: &str) -> Result<Vec<(i32, String, Option<String>)>, String> {
    Ok(aski_core::query_domain_variants(world, domain_name))
}

pub fn query_struct_fields(world: &World, struct_name: &str) -> Result<Vec<(i32, String, String)>, String> {
    Ok(aski_core::query_struct_fields(world, struct_name))
}

pub fn query_params(world: &World, node_id: i64) -> Result<Vec<(String, Option<String>, Option<String>)>, String> {
    Ok(aski_core::query_params(world, node_id))
}

pub fn query_return_type(world: &World, node_id: i64) -> Result<Option<String>, String> {
    Ok(aski_core::query_return_type(world, node_id))
}

pub fn query_constant(world: &World, node_id: i64) -> Result<Option<(String, String, bool)>, String> {
    Ok(aski_core::query_constant(world, node_id))
}

pub fn query_child_exprs(world: &World, parent_id: i64) -> Result<Vec<(i64, String, i64, Option<String>)>, String> {
    Ok(aski_core::query_child_exprs(world, parent_id))
}

pub fn query_match_arms(world: &World, match_id: i64) -> Result<Vec<(i64, Vec<String>, Option<i64>, String)>, String> {
    Ok(aski_core::query_match_arms(world, match_id))
}

pub fn query_expr_by_id(world: &World, expr_id: i64) -> Result<Option<(String, Option<String>)>, String> {
    Ok(aski_core::query_expr_by_id(world, expr_id))
}

pub fn query_nodes_by_kind(world: &World, kind: &str) -> Result<Vec<(i64, String)>, String> {
    Ok(aski_core::query_nodes_by_kind(world, kind))
}

pub fn query_node_kind(world: &World, node_id: i64) -> Result<Option<String>, String> {
    Ok(aski_core::query_node_kind(world, node_id))
}

pub fn query_supertraits(world: &World, trait_id: i64) -> Result<Vec<String>, String> {
    Ok(aski_core::query_supertraits(world, trait_id))
}

/// Kernel primitives — the fixed set of operations aski requires from Rust.
/// Like Shen's KLambda. Adding a primitive = updating this list + codegen.
pub const KERNEL_PRIMITIVES: &[&str] = &[
    "sin", "cos", "sqrt", "abs",
    "truncate", "toF32", "toU32", "toI64",
    "fromOrdinal",
    "len", "clone", "to_string", "is_empty", "unwrap",
];

/// Check if a name is a known method in the World.
/// Extends aski-core's version with kernel primitives.
pub fn is_known_method(name: &str, world: &World) -> bool {
    if KERNEL_PRIMITIVES.contains(&name) {
        return true;
    }
    aski_core::is_known_method(name, world)
}

/// Query fields that need auto-boxing — where a type contains itself (directly or transitively).
/// Returns (containing_type, field_name, recursive_type) triples.
/// This version returns 3-tuples (aski-core returns 2-tuples) for codegen compatibility.
pub fn query_recursive_fields(world: &World) -> Result<Vec<(String, String, String)>, String> {
    let mut fields = Vec::new();

    for (owner_id, kind, owner_name, _, _, _, _) in &world.Node {
        if kind != "struct" {
            continue;
        }
        for (sid, _, field_name, field_type) in &world.Field {
            if sid != owner_id {
                continue;
            }
            let matches_rule1 = field_type == owner_name
                && world.RecursiveType.iter().any(|(a, b)| a == owner_name && b == field_type);
            let matches_rule2 = world.RecursiveType.iter().any(|(a, b)| a == field_type && b == owner_name);

            if matches_rule1 || matches_rule2 {
                fields.push((owner_name.clone(), field_name.clone(), field_type.clone()));
            }
        }
    }

    Ok(fields)
}

/// Validate that no method return type references a body-scoped type.
pub fn validate_return_type_scope(world: &World) -> Result<Vec<(String, String)>, String> {
    let body_scoped: HashSet<String> = world.Node.iter()
        .filter(|(_, kind, _, parent, _, _, _)| {
            parent.is_some() && (kind == "struct" || kind == "domain")
        })
        .map(|(_, _, name, _, _, _, _)| name.clone())
        .collect();

    let mut violations = Vec::new();
    for (node_id, type_name) in &world.Returns {
        if body_scoped.contains(type_name) {
            if let Some((_, _, method_name, _, _, _, _)) = world.Node.iter().find(|(id, _, _, _, _, _, _)| id == node_id) {
                violations.push((method_name.clone(), type_name.clone()));
            }
        }
    }
    Ok(violations)
}

/// Validate that no struct-domain has String fields.
pub fn validate_no_string_fields(world: &World) -> Result<Vec<(String, String)>, String> {
    let mut violations = Vec::new();
    for (struct_id, kind, struct_name, _, _, _, _) in &world.Node {
        if kind != "struct" {
            continue;
        }
        for (sid, _, field_name, type_ref) in &world.Field {
            if sid == struct_id && type_ref == "String" {
                violations.push((struct_name.clone(), field_name.clone()));
            }
        }
    }
    Ok(violations)
}

// ═══════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_source;

    fn setup_world(src: &str) -> World {
        let mut world = create_world();
        let items = parse_source(src).unwrap();
        insert_ast(&mut world, &items).unwrap();
        run_rules(&mut world);
        world
    }

    #[test]
    fn ir_roundtrip_domain() {
        let world = setup_world("Element (Fire Earth Air Water)");
        let variants = query_domain_variants(&world, "Element").unwrap();
        assert_eq!(variants.len(), 4);
        assert_eq!(variants[0].1, "Fire");
        assert_eq!(variants[3].1, "Water");
    }

    #[test]
    fn ir_roundtrip_struct() {
        let world = setup_world("Point { X F64 Y F64 }");
        let fields = query_struct_fields(&world, "Point").unwrap();
        assert_eq!(fields.len(), 2);
        assert_eq!(fields[0].1, "X");
        assert_eq!(fields[0].2, "F64");
    }

    #[test]
    fn ir_roundtrip_trait_impl_method() {
        let world = setup_world("compute [Addition [add(:@Self) U32 [ ^(@Self.Left + @Self.Right) ]]]");
        let nodes = query_nodes_by_kind(&world, "impl").unwrap();
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].1, "compute");
    }

    #[test]
    fn ir_roundtrip_main() {
        let world = setup_world("Main [ StdOut \"hello\" ]");
        let nodes = query_nodes_by_kind(&world, "main").unwrap();
        assert_eq!(nodes.len(), 1);
    }

    #[test]
    fn ir_roundtrip_const() {
        let world = setup_world("!Pi F64 {3.14159265358979}");
        let consts = query_nodes_by_kind(&world, "const").unwrap();
        assert_eq!(consts.len(), 1);
    }

    #[test]
    fn validate_module_scope_return_type_passes() {
        let world = setup_world("Point { X F64 Y F64 }\ntranslate [Point [translate(:@Self) Point [ ^@Self ]]]");
        let violations = validate_return_type_scope(&world).unwrap();
        assert!(violations.is_empty(), "expected no violations, got {:?}", violations);
    }

    #[test]
    fn validate_no_string_fields_passes_for_domain_types() {
        let world = setup_world("Point { X F64 Y F64 }\nSign (Aries Taurus)");
        let violations = validate_no_string_fields(&world).unwrap();
        assert!(violations.is_empty());
    }

    #[test]
    fn validate_string_field_rejected() {
        let world = setup_world("BadStruct { Name String Value F64 }");
        let violations = validate_no_string_fields(&world).unwrap();
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].0, "BadStruct");
        assert_eq!(violations[0].1, "Name");
    }

    #[test]
    fn ir_grammar_rule_stored() {
        let world = setup_world("<Truncate> [\n  [@Value] @Value.truncate\n]");
        // Check that grammar_rule node exists
        let grammar_nodes = query_nodes_by_kind(&world, "grammar_rule").unwrap();
        assert_eq!(grammar_nodes.len(), 1);
        assert_eq!(grammar_nodes[0].1, "Truncate");
        // Check that GrammarRule relation is populated
        assert_eq!(world.GrammarRule.len(), 1);
        assert_eq!(world.GrammarRule[0].1, "Truncate");
        // Check that GrammarArm relation is populated
        assert_eq!(world.GrammarArm.len(), 1);
        assert_eq!(world.GrammarArm[0].1, 0); // ordinal 0
    }

    #[test]
    fn ir_grammar_rule_multi_arm() {
        let world = setup_world("<Convert> [\n  [Truncate @Value] @Value.truncate\n  [ToFloat @Value] @Value.toF32\n]");
        assert_eq!(world.GrammarRule.len(), 1);
        assert_eq!(world.GrammarArm.len(), 2);
    }
}
