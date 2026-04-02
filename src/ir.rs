#![allow(non_snake_case)]

use std::collections::HashSet;

use ascent::ascent;

use crate::ast::*;

// ═══════════════════════════════════════════════════════════════
// Ascent World — all AST relations + derived rules
// ═══════════════════════════════════════════════════════════════

ascent! {
    pub struct World;

    // Every AST node
    relation Node(i64, String, String, Option<i64>, usize, usize);
    // (id, kind, name, parent, span_start, span_end)

    // Domain variants
    relation Variant(i64, i64, String, Option<String>);
    // (domain_id, ordinal, name, wraps)

    // Struct fields
    relation Field(i64, i64, String, String);
    // (struct_id, ordinal, name, type_ref)

    // Method/function parameters
    relation Param(i64, i64, String, Option<String>, Option<String>);
    // (node_id, ordinal, kind, name, type_ref)

    // Return types
    relation Returns(i64, String);
    // (node_id, type_ref)

    // Trait implementations
    relation TraitImpl(String, String, i64);
    // (trait_name, type_name, impl_node_id)

    // Constants
    relation Constant(i64, String, String, bool);
    // (node_id, name, type_ref, has_value)

    // Expression tree
    relation Expr(i64, Option<i64>, String, i64, Option<String>);
    // (id, parent_id, kind, ordinal, value)

    // Match arms — patterns serialized as pipe-separated string
    relation MatchArm(i64, i64, String, Option<i64>, String);
    // (match_id, ordinal, patterns_json, body_expr_id, arm_kind)

    // Derived: field containment for recursive type detection
    relation ContainedType(String, String);
    // (parent_type, child_type) — immediate containment

    ContainedType(parent_type, field_type) <--
        Node(parent_id, kind, parent_type, _, _, _),
        if kind == "struct",
        Field(*parent_id, _, _, field_type);

    ContainedType(parent_type, field_type.clone()) <--
        Node(parent_id, kind, parent_type, _, _, _),
        if kind == "domain",
        Variant(*parent_id, _, _, wraps),
        if wraps.is_some(),
        let field_type = wraps.as_ref().unwrap();

    relation TransitiveContains(String, String);
    // Transitive closure — recursive types need auto-boxing
    TransitiveContains(x, y) <-- ContainedType(x, y);
    TransitiveContains(x, z) <-- ContainedType(x, y), TransitiveContains(y, z);
}

// ═══════════════════════════════════════════════════════════════
// ID Generator
// ═══════════════════════════════════════════════════════════════

/// Counter for generating unique node IDs.
pub struct IdGen {
    pub next: i64,
}

impl IdGen {
    pub fn new() -> Self {
        Self { next: 1 }
    }

    pub fn next(&mut self) -> i64 {
        let id = self.next;
        self.next += 1;
        id
    }
}

impl Default for IdGen {
    fn default() -> Self {
        Self::new()
    }
}

// ═══════════════════════════════════════════════════════════════
// Type ref formatting (copied from db.rs, no CozoDB dependency)
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
// Pattern utilities (copied from db.rs, no CozoDB dependency)
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

/// Parse a pattern string back from storage.
pub fn parse_pattern_string(s: &str) -> ParsedPattern {
    match s {
        "_" => ParsedPattern::Wildcard,
        "True" => ParsedPattern::BoolLit(true),
        "False" => ParsedPattern::BoolLit(false),
        other => {
            // Check for data-carrying pattern: "Name(@Bind)" or "Name(inner)"
            if let Some(paren_pos) = other.find('(') {
                if other.ends_with(')') {
                    let name = other[..paren_pos].to_string();
                    let inner = other[paren_pos + 1..other.len() - 1].to_string();
                    return ParsedPattern::DataCarrying(name, inner);
                }
            }
            ParsedPattern::Variant(other.to_string())
        }
    }
}

#[derive(Debug, Clone)]
pub enum ParsedPattern {
    Variant(String),
    Wildcard,
    BoolLit(bool),
    /// Data-carrying variant: Name(binding) — e.g., Parsed(@Toks)
    DataCarrying(String, String),
}

// ═══════════════════════════════════════════════════════════════
// Insert functions
// ═══════════════════════════════════════════════════════════════

/// Insert a parsed AST into the World.
pub fn insert_ast(
    world: &mut World,
    items: &[Spanned<Item>],
) -> Result<(), String> {
    let mut ids = IdGen::new();
    for item in items {
        insert_item(world, &mut ids, &item.node, None)?;
    }
    Ok(())
}

/// Insert AST items with an ID offset to avoid collisions in multi-file compilation.
/// Returns the number of IDs used.
pub fn insert_ast_with_offset(
    world: &mut World,
    items: &[Spanned<Item>],
    offset: i64,
) -> Result<i64, String> {
    let mut ids = IdGen { next: offset + 1 };
    for item in items {
        insert_item(world, &mut ids, &item.node, None)?;
    }
    Ok(ids.next - offset - 1)
}

fn insert_item(
    world: &mut World,
    ids: &mut IdGen,
    item: &Item,
    parent: Option<i64>,
) -> Result<i64, String> {
    match item {
        Item::Domain(d) => insert_domain(world, ids, d, parent),
        Item::Struct(s) => insert_struct(world, ids, s, parent),
        Item::Trait(t) => insert_trait(world, ids, t, parent),
        Item::TraitImpl(ti) => insert_trait_impl(world, ids, ti, parent),
        Item::InherentImpl(ii) => insert_inherent_impl(world, ids, ii, parent),
        Item::Const(c) => insert_const(world, ids, c, parent),
        Item::Main(m) => insert_main(world, ids, m, parent),
        Item::TypeAlias(ta) => {
            let id = ids.next();
            let span = &ta.span;
            insert_node(world, id, "type_alias", &ta.name, parent, span);
            let aliased = type_ref_to_string(&ta.target);
            world.Returns.push((id, aliased));
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
) {
    world.Node.push((id, kind.to_string(), name.to_string(), parent, span.start, span.end));
}

fn insert_domain(
    world: &mut World,
    ids: &mut IdGen,
    domain: &DomainDecl,
    parent: Option<i64>,
) -> Result<i64, String> {
    let id = ids.next();
    insert_node(world, id, "domain", &domain.name, parent, &domain.span);

    for (ordinal, variant) in domain.variants.iter().enumerate() {
        let wraps = variant.wraps.as_ref().map(type_ref_to_string);
        world.Variant.push((id, ordinal as i64, variant.name.clone(), wraps));

        // Struct variant fields
        if let Some(ref fields) = variant.fields {
            let variant_id = ids.next();
            insert_node(world, variant_id, "struct", &variant.name, Some(id), &variant.span);
            for (ford, field) in fields.iter().enumerate() {
                let tr = type_ref_to_string(&field.type_ref);
                world.Field.push((variant_id, ford as i64, field.name.clone(), tr));
            }
        }

        // Inline domain variant — sub-variants form a nested domain
        if let Some(ref sub_vars) = variant.sub_variants {
            let sub_domain_id = ids.next();
            insert_node(world, sub_domain_id, "domain", &variant.name, Some(id), &variant.span);
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
) -> Result<i64, String> {
    let id = ids.next();
    insert_node(world, id, "struct", &s.name, parent, &s.span);

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
) -> Result<i64, String> {
    let id = ids.next();
    insert_node(world, id, "trait", &t.name, parent, &t.span);

    for method in &t.methods {
        insert_method_sig(world, ids, method, id)?;
    }

    for constant in &t.constants {
        insert_const(world, ids, constant, Some(id))?;
    }

    Ok(id)
}

fn insert_method_sig(
    world: &mut World,
    ids: &mut IdGen,
    method: &MethodSig,
    parent_id: i64,
) -> Result<i64, String> {
    let id = ids.next();
    insert_node(world, id, "method_sig", &method.name, Some(parent_id), &method.span);

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
) -> Result<i64, String> {
    let id = ids.next();
    insert_node(world, id, "impl", &ti.trait_name, parent, &ti.span);

    for type_impl in &ti.impls {
        let impl_id = ids.next();
        insert_node(world, impl_id, "impl_body", &type_impl.target, Some(id), &type_impl.span);

        world.TraitImpl.push((ti.trait_name.clone(), type_impl.target.clone(), impl_id));

        // Store associated types as assoc_type child nodes
        for assoc in &type_impl.associated_types {
            let assoc_id = ids.next();
            insert_node(world, assoc_id, "assoc_type", &assoc.name, Some(impl_id), &assoc.span);
            let type_str = type_ref_to_string(&assoc.concrete_type);
            world.Returns.push((assoc_id, type_str));
        }

        // Store associated constants as const child nodes
        for constant in &type_impl.associated_constants {
            insert_const(world, ids, constant, Some(impl_id))?;
        }

        for method in &type_impl.methods {
            insert_method_def(world, ids, method, impl_id)?;
        }
    }

    Ok(id)
}

fn insert_method_def(
    world: &mut World,
    ids: &mut IdGen,
    method: &MethodDef,
    parent_id: i64,
) -> Result<i64, String> {
    let id = ids.next();
    let kind = if matches!(method.body, Body::TailBlock(_)) { "tail_method" } else { "method" };
    insert_node(world, id, kind, &method.name, Some(parent_id), &method.span);

    for (ordinal, param) in method.params.iter().enumerate() {
        insert_param(world, id, ordinal, param);
    }
    if let Some(ref output) = method.output {
        insert_return_type(world, id, output);
    }

    insert_body(world, ids, &method.body, id)?;

    Ok(id)
}

fn insert_inherent_impl(
    world: &mut World,
    ids: &mut IdGen,
    ii: &InherentImplDecl,
    parent: Option<i64>,
) -> Result<i64, String> {
    let id = ids.next();
    insert_node(world, id, "inherent_impl", &ii.type_name, parent, &ii.span);

    for method in &ii.methods {
        insert_method_def(world, ids, method, id)?;
    }

    Ok(id)
}

fn insert_const(
    world: &mut World,
    ids: &mut IdGen,
    c: &ConstDecl,
    parent: Option<i64>,
) -> Result<i64, String> {
    let id = ids.next();
    insert_node(world, id, "const", &c.name, parent, &c.span);

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
) -> Result<i64, String> {
    let id = ids.next();
    insert_node(world, id, "main", "Main", parent, &m.span);

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
        Expr::Comprehension { source, output, guard } => {
            put_expr(world, id, parent_id, "comprehension", ordinal, None);
            insert_expr(world, ids, &source.node, id, 0)?;
            if let Some(out) = output {
                insert_expr(world, ids, &out.node, id, 1)?;
            }
            if let Some(g) = guard {
                insert_expr(world, ids, &g.node, id, 2)?;
            }
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

/// Run derived rules (must be called after all inserts, before queries).
pub fn run_rules(world: &mut World) {
    world.run();
}

// ═══════════════════════════════════════════════════════════════
// Query functions — same shape as db.rs for codegen compatibility
// ═══════════════════════════════════════════════════════════════

/// Query all top-level nodes ordered by id (preserving source order).
pub fn query_all_top_level_nodes(world: &World) -> Result<Vec<(i64, String, String)>, String> {
    let mut nodes: Vec<(i64, String, String)> = world.Node.iter()
        .filter(|(_, _, _, parent, _, _)| parent.is_none())
        .map(|(id, kind, name, _, _, _)| (*id, kind.clone(), name.clone()))
        .collect();
    nodes.sort_by_key(|(id, _, _)| *id);
    Ok(nodes)
}

/// Query child nodes of a parent, ordered by id.
pub fn query_child_nodes(world: &World, parent_id: i64) -> Result<Vec<(i64, String, String)>, String> {
    let mut nodes: Vec<(i64, String, String)> = world.Node.iter()
        .filter(|(_, _, _, parent, _, _)| *parent == Some(parent_id))
        .map(|(id, kind, name, _, _, _)| (*id, kind.clone(), name.clone()))
        .collect();
    nodes.sort_by_key(|(id, _, _)| *id);
    Ok(nodes)
}

/// Query domain variants.
pub fn query_domain_variants(world: &World, domain_name: &str) -> Result<Vec<(i32, String, Option<String>)>, String> {
    // First find the domain node id
    let domain_ids: Vec<i64> = world.Node.iter()
        .filter(|(_, kind, name, _, _, _)| kind == "domain" && name == domain_name)
        .map(|(id, _, _, _, _, _)| *id)
        .collect();

    let mut variants: Vec<(i32, String, Option<String>)> = Vec::new();
    for domain_id in &domain_ids {
        for (did, ordinal, name, wraps) in &world.Variant {
            if did == domain_id {
                variants.push((*ordinal as i32, name.clone(), wraps.clone()));
            }
        }
    }
    variants.sort_by_key(|(ord, _, _)| *ord);
    Ok(variants)
}

/// Query struct fields.
pub fn query_struct_fields(world: &World, struct_name: &str) -> Result<Vec<(i32, String, String)>, String> {
    let struct_ids: Vec<i64> = world.Node.iter()
        .filter(|(_, kind, name, _, _, _)| kind == "struct" && name == struct_name)
        .map(|(id, _, _, _, _, _)| *id)
        .collect();

    let mut fields: Vec<(i32, String, String)> = Vec::new();
    for struct_id in &struct_ids {
        for (sid, ordinal, name, type_ref) in &world.Field {
            if sid == struct_id {
                fields.push((*ordinal as i32, name.clone(), type_ref.clone()));
            }
        }
    }
    fields.sort_by_key(|(ord, _, _)| *ord);
    Ok(fields)
}

/// Query parameters for a function/method node, ordered by ordinal.
pub fn query_params(world: &World, node_id: i64) -> Result<Vec<(String, Option<String>, Option<String>)>, String> {
    let mut params: Vec<(i64, String, Option<String>, Option<String>)> = world.Param.iter()
        .filter(|(nid, _, _, _, _)| *nid == node_id)
        .map(|(_, ordinal, kind, name, type_ref)| (*ordinal, kind.clone(), name.clone(), type_ref.clone()))
        .collect();
    params.sort_by_key(|(ord, _, _, _)| *ord);
    Ok(params.into_iter().map(|(_, kind, name, type_ref)| (kind, name, type_ref)).collect())
}

/// Query return type for a function/method node.
pub fn query_return_type(world: &World, node_id: i64) -> Result<Option<String>, String> {
    Ok(world.Returns.iter()
        .find(|(nid, _)| *nid == node_id)
        .map(|(_, type_ref)| type_ref.clone()))
}

/// Query constant by node_id.
pub fn query_constant(world: &World, node_id: i64) -> Result<Option<(String, String, bool)>, String> {
    Ok(world.Constant.iter()
        .find(|(nid, _, _, _)| *nid == node_id)
        .map(|(_, name, type_ref, has_value)| (name.clone(), type_ref.clone(), *has_value)))
}

/// Query all child expressions of a parent, ordered by ordinal.
pub fn query_child_exprs(world: &World, parent_id: i64) -> Result<Vec<(i64, String, i64, Option<String>)>, String> {
    let mut children: Vec<(i64, String, i64, Option<String>)> = world.Expr.iter()
        .filter(|(_, pid, _, _, _)| *pid == Some(parent_id))
        .map(|(id, _, kind, ordinal, value)| (*id, kind.clone(), *ordinal, value.clone()))
        .collect();
    children.sort_by_key(|(_, _, ordinal, _)| *ordinal);
    Ok(children)
}

/// Query match arms for a match expression, ordered by ordinal.
pub fn query_match_arms(world: &World, match_id: i64) -> Result<Vec<(i64, Vec<String>, Option<i64>, String)>, String> {
    let mut arms: Vec<(i64, String, Option<i64>, String)> = world.MatchArm.iter()
        .filter(|(mid, _, _, _, _)| *mid == match_id)
        .map(|(_, ordinal, patterns_json, body_expr_id, arm_kind)| {
            (*ordinal, patterns_json.clone(), *body_expr_id, arm_kind.clone())
        })
        .collect();
    arms.sort_by_key(|(ord, _, _, _)| *ord);

    Ok(arms.into_iter().map(|(ordinal, patterns_json, body_expr_id, arm_kind)| {
        let patterns: Vec<String> = serde_json::from_str(&patterns_json).unwrap_or_default();
        (ordinal, patterns, body_expr_id, arm_kind)
    }).collect())
}

/// Query a single expression by id.
pub fn query_expr_by_id(world: &World, expr_id: i64) -> Result<Option<(String, Option<String>)>, String> {
    Ok(world.Expr.iter()
        .find(|(id, _, _, _, _)| *id == expr_id)
        .map(|(_, _, kind, _, value)| (kind.clone(), value.clone())))
}

/// Query all nodes of a given kind.
pub fn query_nodes_by_kind(world: &World, kind: &str) -> Result<Vec<(i64, String)>, String> {
    let mut nodes: Vec<(i64, String)> = world.Node.iter()
        .filter(|(_, k, _, _, _, _)| k == kind)
        .map(|(id, _, name, _, _, _)| (*id, name.clone()))
        .collect();
    nodes.sort_by_key(|(id, _)| *id);
    Ok(nodes)
}

/// Query the kind of a node by its ID.
pub fn query_node_kind(world: &World, node_id: i64) -> Result<Option<String>, String> {
    Ok(world.Node.iter()
        .find(|(id, _, _, _, _, _)| *id == node_id)
        .map(|(_, kind, _, _, _, _)| kind.clone()))
}

/// Check if a name is a known method in the World.
pub fn is_known_method(name: &str, world: &World) -> bool {
    if matches!(
        name,
        "sqrt" | "abs" | "len" | "clone" | "to_string" | "is_empty" | "unwrap"
    ) {
        return true;
    }
    // Check if the name is a method defined in the world
    world.Node.iter().any(|(_, kind, n, _, _, _)| {
        (kind == "method" || kind == "tail_method" || kind == "method_sig") && n == name
    })
}

/// Query fields that need auto-boxing — where a type contains itself (directly or transitively).
/// Returns (containing_type, field_name, recursive_type) triples.
pub fn query_recursive_fields(world: &World) -> Result<Vec<(String, String, String)>, String> {
    // After run(), TransitiveContains is computed.
    let mut fields = Vec::new();

    // Find struct fields where the field type transitively contains the owner, or IS the owner
    for (owner_id, kind, owner_name, _, _, _) in &world.Node {
        if kind != "struct" {
            continue;
        }
        for (sid, _, field_name, field_type) in &world.Field {
            if sid != owner_id {
                continue;
            }
            // Check: field_type == owner_name (direct self-reference)
            // OR: field_type transitively contains owner_name
            // OR: owner_name transitively contains field_type (mutual recursion via field_type)
            let _is_recursive = field_type == owner_name
                || world.TransitiveContains.iter().any(|(a, b)| a == field_type && b == owner_name)
                || world.TransitiveContains.iter().any(|(a, b)| a == owner_name && b == field_type && field_type == owner_name);

            // Match the CozoDB version more precisely:
            // recursive_field[owner, field_name, field_type] :=
            //   *node{id: owner_id, kind: 'struct', name: owner},
            //   *field{struct_id: owner_id, name: field_name, type_ref: field_type},
            //   transitive[owner, field_type], field_type == owner
            // recursive_field[owner, field_name, field_type] :=
            //   *node{id: owner_id, kind: 'struct', name: owner},
            //   *field{struct_id: owner_id, name: field_name, type_ref: field_type},
            //   transitive[field_type, owner]
            let matches_rule1 = field_type == owner_name
                && world.TransitiveContains.iter().any(|(a, b)| a == owner_name && b == field_type);
            let matches_rule2 = world.TransitiveContains.iter().any(|(a, b)| a == field_type && b == owner_name);

            if matches_rule1 || matches_rule2 {
                fields.push((owner_name.clone(), field_name.clone(), field_type.clone()));
            }
        }
    }

    Ok(fields)
}

/// Validate that no method return type references a body-scoped type.
pub fn validate_return_type_scope(world: &World) -> Result<Vec<(String, String)>, String> {
    // Body-scoped types have non-null parent and kind in ['struct', 'domain']
    let body_scoped: HashSet<String> = world.Node.iter()
        .filter(|(_, kind, _, parent, _, _)| {
            parent.is_some() && (kind == "struct" || kind == "domain")
        })
        .map(|(_, _, name, _, _, _)| name.clone())
        .collect();

    let mut violations = Vec::new();
    for (node_id, type_name) in &world.Returns {
        if body_scoped.contains(type_name) {
            if let Some((_, _, method_name, _, _, _)) = world.Node.iter().find(|(id, _, _, _, _, _)| id == node_id) {
                violations.push((method_name.clone(), type_name.clone()));
            }
        }
    }
    Ok(violations)
}

/// Validate that no struct-domain has String fields.
pub fn validate_no_string_fields(world: &World) -> Result<Vec<(String, String)>, String> {
    let mut violations = Vec::new();
    for (struct_id, kind, struct_name, _, _, _) in &world.Node {
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
    fn ir_roundtrip_inherent_method() {
        let world = setup_world("Addition [ add(:@Self) U32 [ ^(@Self.Left + @Self.Right) ] ]");
        let nodes = query_nodes_by_kind(&world, "inherent_impl").unwrap();
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].1, "Addition");
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
        let world = setup_world("Point { X F64 Y F64 }\nPoint [ translate(:@Self) Point [ ^@Self ] ]");
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
}
