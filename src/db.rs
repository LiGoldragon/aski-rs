use cozo::{DbInstance, NamedRows, ScriptMutability};
use serde_json::json;

use crate::ast::*;

/// Run a mutable CozoDB script (schema creation, puts).
fn run_mut(db: &DbInstance, script: &str) -> Result<NamedRows, String> {
    db.run_script(script, Default::default(), ScriptMutability::Mutable)
        .map_err(|e| e.to_string())
}

/// Run an immutable CozoDB query.
fn run_query(db: &DbInstance, script: &str) -> Result<NamedRows, String> {
    db.run_script(script, Default::default(), ScriptMutability::Immutable)
        .map_err(|e| e.to_string())
}

/// Initialize a CozoDB instance with the aski AST schema.
pub fn create_db() -> Result<DbInstance, String> {
    let db = DbInstance::new("mem", "", "").map_err(|e| e.to_string())?;
    init_schema(&db)?;
    Ok(db)
}

/// Create all relations in the database.
pub fn init_schema(db: &DbInstance) -> Result<(), String> {
    let schema_commands = vec![
        // Every node in the AST
        r#":create node {
            id: Int =>
            kind: String,
            name: String,
            parent: Int?,
            span_start: Int,
            span_end: Int
        }"#,
        // Domain variants
        r#":create variant {
            domain_id: Int, ordinal: Int =>
            name: String,
            wraps: String?
        }"#,
        // Struct fields (sub-types)
        r#":create field {
            struct_id: Int, ordinal: Int =>
            name: String,
            type_ref: String
        }"#,
        // Function/method parameters (Param-based)
        r#":create param {
            node_id: Int, ordinal: Int =>
            kind: String,
            name: String?,
            type_ref: String?
        }"#,
        // Function/method return type
        r#":create returns {
            node_id: Int =>
            type_ref: String
        }"#,
        // Trait implementations
        r#":create trait_impl {
            trait_name: String, type_name: String =>
            impl_node_id: Int
        }"#,
        // Constants
        r#":create constant {
            node_id: Int =>
            name: String,
            type_ref: String,
            has_value: Bool
        }"#,
        // Expression tree
        r#":create expr {
            id: Int =>
            parent_id: Int?,
            kind: String,
            ordinal: Int,
            value: String?
        }"#,
        // Match expression arms
        r#":create match_arm {
            match_id: Int, ordinal: Int =>
            patterns: [String],
            body_expr_id: Int?,
            arm_kind: String
        }"#,
    ];

    for cmd in schema_commands {
        run_mut(db, cmd).map_err(|e| format!("schema error: {e}"))?;
    }

    Ok(())
}

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
    }
}

/// Insert a parsed AST into the database.
pub fn insert_ast(
    db: &DbInstance,
    items: &[Spanned<Item>],
) -> Result<(), String> {
    let mut ids = IdGen::new();
    for item in items {
        insert_item(db, &mut ids, &item.node, None)?;
    }
    Ok(())
}

/// Insert AST items with an ID offset to avoid collisions in multi-file compilation.
/// Returns the number of IDs used.
pub fn insert_ast_with_offset(
    db: &DbInstance,
    items: &[Spanned<Item>],
    offset: i64,
) -> Result<i64, String> {
    let mut ids = IdGen { next: offset + 1 };
    for item in items {
        insert_item(db, &mut ids, &item.node, None)?;
    }
    Ok(ids.next - offset - 1)
}

fn insert_item(
    db: &DbInstance,
    ids: &mut IdGen,
    item: &Item,
    parent: Option<i64>,
) -> Result<i64, String> {
    match item {
        Item::Domain(d) => insert_domain(db, ids, d, parent),
        Item::Struct(s) => insert_struct(db, ids, s, parent),
        Item::Trait(t) => insert_trait(db, ids, t, parent),
        Item::TraitImpl(ti) => insert_trait_impl(db, ids, ti, parent),
        Item::InherentImpl(ii) => insert_inherent_impl(db, ids, ii, parent),
        Item::Const(c) => insert_const(db, ids, c, parent),
        Item::Main(m) => insert_main(db, ids, m, parent),
        Item::TypeAlias(ta) => {
            let id = ids.next();
            let span = &ta.span;
            insert_node(db, id, "type_alias", &ta.name, parent, span)?;
            // Store the aliased type in the returns relation
            let aliased = crate::db::type_ref_to_string(&ta.target);
            let script = format!(
                "?[node_id, type_ref] <- [[{}, '{}']]\n:put returns {{node_id => type_ref}}",
                id, aliased
            );
            run_mut(db, &script).map_err(|e| format!("insert type alias returns: {e}"))?;
            Ok(id)
        }
    }
}

fn insert_node(
    db: &DbInstance,
    id: i64,
    kind: &str,
    name: &str,
    parent: Option<i64>,
    span: &Span,
) -> Result<(), String> {
    let parent_val = parent.map(|p| json!(p)).unwrap_or(json!(null));
    let script = format!(
        "?[id, kind, name, parent, span_start, span_end] <- [[{}, '{}', '{}', {}, {}, {}]]\n:put node {{id => kind, name, parent, span_start, span_end}}",
        id,
        kind.replace('\'', "\\'"),
        name.replace('\'', "\\'"),
        parent_val,
        span.start,
        span.end
    );
    run_mut(db, &script).map_err(|e| format!("insert node: {e}"))?;
    Ok(())
}

fn insert_domain(
    db: &DbInstance,
    ids: &mut IdGen,
    domain: &DomainDecl,
    parent: Option<i64>,
) -> Result<i64, String> {
    let id = ids.next();
    insert_node(db, id, "domain", &domain.name, parent, &domain.span)?;

    for (ordinal, variant) in domain.variants.iter().enumerate() {
        let wraps = variant.wraps.as_ref().map(type_ref_to_string);
        let wraps_val = wraps
            .as_ref()
            .map(|w| format!("'{}'", w.replace('\'', "\\'")))
            .unwrap_or_else(|| "null".to_string());
        let script = format!(
            "?[domain_id, ordinal, name, wraps] <- [[{}, {}, '{}', {}]]\n:put variant {{domain_id, ordinal => name, wraps}}",
            id, ordinal, variant.name.replace('\'', "\\'"), wraps_val
        );
        run_mut(db, &script).map_err(|e| format!("insert variant: {e}"))?;

        // Struct variant fields
        if let Some(ref fields) = variant.fields {
            let variant_id = ids.next();
            insert_node(db, variant_id, "struct", &variant.name, Some(id), &variant.span)?;
            for (ford, field) in fields.iter().enumerate() {
                let tr = type_ref_to_string(&field.type_ref);
                let script = format!(
                    "?[struct_id, ordinal, name, type_ref] <- [[{}, {}, '{}', '{}']]\n:put field {{struct_id, ordinal => name, type_ref}}",
                    variant_id, ford, field.name.replace('\'', "\\'"), tr.replace('\'', "\\'")
                );
                run_mut(db, &script).map_err(|e| format!("insert variant field: {e}"))?;
            }
        }

        // Inline domain variant — sub-variants form a nested domain
        if let Some(ref sub_vars) = variant.sub_variants {
            let sub_domain_id = ids.next();
            insert_node(db, sub_domain_id, "domain", &variant.name, Some(id), &variant.span)?;
            for (sord, sub_var) in sub_vars.iter().enumerate() {
                let sw = sub_var.wraps.as_ref().map(type_ref_to_string);
                let sw_val = sw
                    .as_ref()
                    .map(|w| format!("'{}'", w.replace('\'', "\\'")))
                    .unwrap_or_else(|| "null".to_string());
                let script = format!(
                    "?[domain_id, ordinal, name, wraps] <- [[{}, {}, '{}', {}]]\n:put variant {{domain_id, ordinal => name, wraps}}",
                    sub_domain_id, sord, sub_var.name.replace('\'', "\\'"), sw_val
                );
                run_mut(db, &script).map_err(|e| format!("insert sub-variant: {e}"))?;
            }
        }
    }

    Ok(id)
}

fn insert_struct(
    db: &DbInstance,
    ids: &mut IdGen,
    s: &StructDecl,
    parent: Option<i64>,
) -> Result<i64, String> {
    let id = ids.next();
    insert_node(db, id, "struct", &s.name, parent, &s.span)?;

    for (ordinal, field) in s.fields.iter().enumerate() {
        let tr = type_ref_to_string(&field.type_ref);
        let script = format!(
            "?[struct_id, ordinal, name, type_ref] <- [[{}, {}, '{}', '{}']]\n:put field {{struct_id, ordinal => name, type_ref}}",
            id, ordinal, field.name.replace('\'', "\\'"), tr.replace('\'', "\\'")
        );
        run_mut(db, &script)
            .map_err(|e| format!("insert field: {e}"))?;
    }

    Ok(id)
}

/// Insert a Param into the param relation.
fn insert_param(
    db: &DbInstance,
    node_id: i64,
    ordinal: usize,
    param: &Param,
) -> Result<(), String> {
    let (kind, name, type_ref) = match param {
        Param::Owned(t) => ("owned", None, Some(t.as_str())),
        Param::Named(n, tr) => ("named", Some(n.as_str()), Some(type_ref_to_string(tr).leak() as &str)),
        Param::BorrowSelf => ("borrow_self", None, None),
        Param::MutBorrowSelf => ("mut_borrow_self", None, None),
        Param::OwnedSelf => ("owned_self", None, None),
        Param::Borrow(t) => ("borrow", None, Some(t.as_str())),
        Param::MutBorrow(t) => ("mut_borrow", None, Some(t.as_str())),
    };

    let name_val = name
        .map(|n| format!("'{}'", n.replace('\'', "\\'")))
        .unwrap_or_else(|| "null".to_string());
    let type_val = type_ref
        .map(|t| format!("'{}'", t.replace('\'', "\\'")))
        .unwrap_or_else(|| "null".to_string());

    let script = format!(
        "?[node_id, ordinal, kind, name, type_ref] <- [[{}, {}, '{}', {}, {}]]\n:put param {{node_id, ordinal => kind, name, type_ref}}",
        node_id, ordinal, kind, name_val, type_val
    );
    run_mut(db, &script).map_err(|e| format!("insert param: {e}"))?;
    Ok(())
}

fn insert_return_type(
    db: &DbInstance,
    node_id: i64,
    tr: &TypeRef,
) -> Result<(), String> {
    let type_str = type_ref_to_string(tr);
    let script = format!(
        "?[node_id, type_ref] <- [[{}, '{}']]\n:put returns {{node_id => type_ref}}",
        node_id, type_str.replace('\'', "\\'")
    );
    run_mut(db, &script).map_err(|e| format!("insert returns: {e}"))?;
    Ok(())
}

fn insert_trait(
    db: &DbInstance,
    ids: &mut IdGen,
    t: &TraitDecl,
    parent: Option<i64>,
) -> Result<i64, String> {
    let id = ids.next();
    insert_node(db, id, "trait", &t.name, parent, &t.span)?;

    for method in &t.methods {
        insert_method_sig(db, ids, method, id)?;
    }

    for constant in &t.constants {
        insert_const(db, ids, constant, Some(id))?;
    }

    Ok(id)
}

fn insert_method_sig(
    db: &DbInstance,
    ids: &mut IdGen,
    method: &MethodSig,
    parent_id: i64,
) -> Result<i64, String> {
    let id = ids.next();
    insert_node(db, id, "method_sig", &method.name, Some(parent_id), &method.span)?;

    for (ordinal, param) in method.params.iter().enumerate() {
        insert_param(db, id, ordinal, param)?;
    }
    if let Some(ref output) = method.output {
        insert_return_type(db, id, output)?;
    }

    Ok(id)
}

fn insert_trait_impl(
    db: &DbInstance,
    ids: &mut IdGen,
    ti: &TraitImplDecl,
    parent: Option<i64>,
) -> Result<i64, String> {
    let id = ids.next();
    insert_node(db, id, "impl", &ti.trait_name, parent, &ti.span)?;

    for type_impl in &ti.impls {
        let impl_id = ids.next();
        insert_node(db, impl_id, "impl_body", &type_impl.target, Some(id), &type_impl.span)?;

        let script = format!(
            "?[trait_name, type_name, impl_node_id] <- [['{}', '{}', {}]]\n:put trait_impl {{trait_name, type_name => impl_node_id}}",
            ti.trait_name.replace('\'', "\\'"),
            type_impl.target.replace('\'', "\\'"),
            impl_id
        );
        run_mut(db, &script)
            .map_err(|e| format!("insert trait_impl: {e}"))?;

        // Store associated types as assoc_type child nodes
        for assoc in &type_impl.associated_types {
            let assoc_id = ids.next();
            insert_node(db, assoc_id, "assoc_type", &assoc.name, Some(impl_id), &assoc.span)?;
            let type_str = type_ref_to_string(&assoc.concrete_type);
            let ret_script = format!(
                "?[node_id, type_ref] <- [[{}, '{}']]\n:put returns {{node_id => type_ref}}",
                assoc_id, type_str.replace('\'', "\\'")
            );
            run_mut(db, &ret_script).map_err(|e| format!("insert assoc_type returns: {e}"))?;
        }

        // Store associated constants as const child nodes
        for constant in &type_impl.associated_constants {
            insert_const(db, ids, constant, Some(impl_id))?;
        }

        for method in &type_impl.methods {
            insert_method_def(db, ids, method, impl_id)?;
        }
    }

    Ok(id)
}

fn insert_method_def(
    db: &DbInstance,
    ids: &mut IdGen,
    method: &MethodDef,
    parent_id: i64,
) -> Result<i64, String> {
    let id = ids.next();
    let kind = if matches!(method.body, Body::TailBlock(_)) { "tail_method" } else { "method" };
    insert_node(db, id, kind, &method.name, Some(parent_id), &method.span)?;

    for (ordinal, param) in method.params.iter().enumerate() {
        insert_param(db, id, ordinal, param)?;
    }
    if let Some(ref output) = method.output {
        insert_return_type(db, id, output)?;
    }

    insert_body(db, ids, &method.body, id)?;

    Ok(id)
}

fn insert_inherent_impl(
    db: &DbInstance,
    ids: &mut IdGen,
    ii: &InherentImplDecl,
    parent: Option<i64>,
) -> Result<i64, String> {
    let id = ids.next();
    insert_node(db, id, "inherent_impl", &ii.type_name, parent, &ii.span)?;

    for method in &ii.methods {
        insert_method_def(db, ids, method, id)?;
    }

    Ok(id)
}

fn insert_const(
    db: &DbInstance,
    ids: &mut IdGen,
    c: &ConstDecl,
    parent: Option<i64>,
) -> Result<i64, String> {
    let id = ids.next();
    insert_node(db, id, "const", &c.name, parent, &c.span)?;

    let tr = type_ref_to_string(&c.type_ref);
    let has_value = c.value.is_some();

    let script = format!(
        "?[node_id, name, type_ref, has_value] <- [[{}, '{}', '{}', {}]]\n:put constant {{node_id => name, type_ref, has_value}}",
        id,
        c.name.replace('\'', "\\'"),
        tr.replace('\'', "\\'"),
        has_value
    );
    run_mut(db, &script)
        .map_err(|e| format!("insert constant: {e}"))?;

    if let Some(body) = &c.value {
        insert_body(db, ids, body, id)?;
    }

    Ok(id)
}

fn insert_main(
    db: &DbInstance,
    ids: &mut IdGen,
    m: &MainDecl,
    parent: Option<i64>,
) -> Result<i64, String> {
    let id = ids.next();
    insert_node(db, id, "main", "Main", parent, &m.span)?;

    insert_body(db, ids, &m.body, id)?;

    Ok(id)
}

/// Insert a function/method body into the expr relation.
fn insert_body(
    db: &DbInstance,
    ids: &mut IdGen,
    body: &Body,
    owner_id: i64,
) -> Result<(), String> {
    match body {
        Body::Stub => {
            let id = ids.next();
            let script = format!(
                "?[id, parent_id, kind, ordinal, value] <- [[{}, {}, 'stub', 0, null]]\n:put expr {{id => parent_id, kind, ordinal, value}}",
                id, owner_id
            );
            run_mut(db, &script).map_err(|e| format!("insert stub expr: {e}"))?;
        }
        Body::Block(stmts) | Body::TailBlock(stmts) => {
            for (ordinal, stmt) in stmts.iter().enumerate() {
                insert_expr(db, ids, &stmt.node, owner_id, ordinal as i64)?;
            }
        }
        Body::MatchBody(arms) => {
            // Store matching method arms as match_arm rows
            for (arm_ord, arm) in arms.iter().enumerate() {
                let patterns: Vec<String> = if arm.kind == ArmKind::Destructure {
                    // For destructure arms, encode elements + tail in patterns list
                    // Format: "destr:Element1,Element2,...|RestName"
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
                    let bid = insert_expr(db, ids, &arm.body[0].node, owner_id, arm_ord as i64)?;
                    Some(bid)
                } else if arm.body.is_empty() {
                    None
                } else {
                    let wrapper_id = ids.next();
                    put_expr(db, wrapper_id, owner_id, "inline_eval", arm_ord as i64, None)?;
                    for (i, e) in arm.body.iter().enumerate() {
                        insert_expr(db, ids, &e.node, wrapper_id, i as i64)?;
                    }
                    Some(wrapper_id)
                };

                let body_val = body_expr_id.map(|b| b.to_string()).unwrap_or_else(|| "null".to_string());
                let kind_str = match arm.kind {
                    ArmKind::Commit => "commit",
                    ArmKind::Backtrack => "backtrack",
                    ArmKind::Destructure => "destructure",
                };
                let script = format!(
                    "?[match_id, ordinal, patterns, body_expr_id, arm_kind] <- [[{}, {}, {}, {}, '{}']]\n:put match_arm {{match_id, ordinal => patterns, body_expr_id, arm_kind}}",
                    owner_id, arm_ord, patterns_json, body_val, kind_str
                );
                run_mut(db, &script).map_err(|e| format!("insert match method arm: {e}"))?;
            }
        }
    }
    Ok(())
}

/// Insert an expression into the expr relation. Returns the assigned expr id.
fn insert_expr(
    db: &DbInstance,
    ids: &mut IdGen,
    expr: &Expr,
    parent_id: i64,
    ordinal: i64,
) -> Result<i64, String> {
    let id = ids.next();

    match expr {
        Expr::IntLit(v) => {
            put_expr(db, id, parent_id, "int_lit", ordinal, Some(&v.to_string()))?;
        }
        Expr::FloatLit(v) => {
            put_expr(db, id, parent_id, "float_lit", ordinal, Some(&format_float(*v)))?;
        }
        Expr::StringLit(s) => {
            put_expr(db, id, parent_id, "string_lit", ordinal, Some(s))?;
        }
        Expr::ConstRef(name) => {
            put_expr(db, id, parent_id, "const_ref", ordinal, Some(name))?;
        }
        Expr::InstanceRef(name) => {
            put_expr(db, id, parent_id, "instance_ref", ordinal, Some(name))?;
        }
        Expr::Return(inner) => {
            put_expr(db, id, parent_id, "return", ordinal, None)?;
            insert_expr(db, ids, &inner.node, id, 0)?;
        }
        Expr::BinOp(left, op, right) => {
            let op_str = binop_to_string(op);
            put_expr(db, id, parent_id, "binop", ordinal, Some(op_str))?;
            insert_expr(db, ids, &left.node, id, 0)?;
            insert_expr(db, ids, &right.node, id, 1)?;
        }
        Expr::Group(inner) => {
            put_expr(db, id, parent_id, "group", ordinal, None)?;
            insert_expr(db, ids, &inner.node, id, 0)?;
        }
        Expr::InlineEval(exprs) => {
            put_expr(db, id, parent_id, "inline_eval", ordinal, None)?;
            for (i, e) in exprs.iter().enumerate() {
                insert_expr(db, ids, &e.node, id, i as i64)?;
            }
        }
        Expr::Access(base, field) => {
            put_expr(db, id, parent_id, "access", ordinal, Some(field))?;
            insert_expr(db, ids, &base.node, id, 0)?;
        }
        Expr::SameTypeNew(name, args) => {
            put_expr(db, id, parent_id, "same_type_new", ordinal, Some(name))?;
            for (i, arg) in args.iter().enumerate() {
                insert_expr(db, ids, &arg.node, id, i as i64)?;
            }
        }
        Expr::SubTypeNew(name, tr, args) => {
            let type_str = type_ref_to_string(tr);
            let val = format!("{}:{}", name, type_str);
            put_expr(db, id, parent_id, "sub_type_new", ordinal, Some(&val))?;
            for (i, arg) in args.iter().enumerate() {
                insert_expr(db, ids, &arg.node, id, i as i64)?;
            }
        }
        Expr::MutableNew(name, tr, args) => {
            let type_str = type_ref_to_string(tr);
            let val = format!("{}:{}", name, type_str);
            put_expr(db, id, parent_id, "mutable_new", ordinal, Some(&val))?;
            for (i, arg) in args.iter().enumerate() {
                insert_expr(db, ids, &arg.node, id, i as i64)?;
            }
        }
        Expr::MutableSet(name, value) => {
            put_expr(db, id, parent_id, "mutable_set", ordinal, Some(name))?;
            insert_expr(db, ids, &value.node, id, 0)?;
        }
        Expr::SubTypeDecl(name, tr) => {
            let type_str = type_ref_to_string(tr);
            let val = format!("{}:{}", name, type_str);
            put_expr(db, id, parent_id, "sub_type_decl", ordinal, Some(&val))?;
        }
        Expr::DeferredNew(name, args) => {
            put_expr(db, id, parent_id, "deferred_new", ordinal, Some(name))?;
            for (i, arg) in args.iter().enumerate() {
                insert_expr(db, ids, &arg.node, id, i as i64)?;
            }
        }
        Expr::MatchExpr(data) => {
            put_expr(db, id, parent_id, "match", ordinal, None)?;
            for (i, target) in data.targets.iter().enumerate() {
                insert_expr(db, ids, &target.node, id, i as i64)?;
            }
            let target_count = data.targets.len() as i64;
            for (arm_ord, arm) in data.arms.iter().enumerate() {
                let patterns: Vec<String> = arm.patterns.iter().map(pattern_to_string).collect();
                let patterns_json = serde_json::to_string(&patterns).unwrap();

                let body_expr_id = if arm.body.len() == 1 {
                    let bid = insert_expr(db, ids, &arm.body[0].node, id, target_count + arm_ord as i64)?;
                    Some(bid)
                } else if arm.body.is_empty() {
                    None
                } else {
                    let wrapper_id = ids.next();
                    put_expr(db, wrapper_id, id, "inline_eval", target_count + arm_ord as i64, None)?;
                    for (i, e) in arm.body.iter().enumerate() {
                        insert_expr(db, ids, &e.node, wrapper_id, i as i64)?;
                    }
                    Some(wrapper_id)
                };

                let body_val = body_expr_id.map(|b| b.to_string()).unwrap_or_else(|| "null".to_string());
                let kind_str = "commit"; // inline match expressions always commit
                let script = format!(
                    "?[match_id, ordinal, patterns, body_expr_id, arm_kind] <- [[{}, {}, {}, {}, '{}']]\n:put match_arm {{match_id, ordinal => patterns, body_expr_id, arm_kind}}",
                    id, arm_ord, patterns_json, body_val, kind_str
                );
                run_mut(db, &script).map_err(|e| format!("insert match_arm: {e}"))?;
            }
        }
        Expr::StdOut(inner) => {
            put_expr(db, id, parent_id, "stdout", ordinal, None)?;
            insert_expr(db, ids, &inner.node, id, 0)?;
        }
        Expr::Stub => {
            put_expr(db, id, parent_id, "stub", ordinal, None)?;
        }
        Expr::BareName(name) => {
            put_expr(db, id, parent_id, "bare_name", ordinal, Some(name))?;
        }
        Expr::ErrorProp(inner) => {
            put_expr(db, id, parent_id, "error_prop", ordinal, None)?;
            insert_expr(db, ids, &inner.node, id, 0)?;
        }
        Expr::Comprehension { source, output, guard } => {
            put_expr(db, id, parent_id, "comprehension", ordinal, None)?;
            insert_expr(db, ids, &source.node, id, 0)?;
            if let Some(out) = output {
                insert_expr(db, ids, &out.node, id, 1)?;
            }
            if let Some(g) = guard {
                insert_expr(db, ids, &g.node, id, 2)?;
            }
        }
        Expr::StructConstruct(name, fields) => {
            put_expr(db, id, parent_id, "struct_construct", ordinal, Some(name))?;
            for (i, (fname, val)) in fields.iter().enumerate() {
                // Store field as a wrapper node with value = field name
                let field_id = ids.next();
                put_expr(db, field_id, id, "struct_field", i as i64, Some(fname))?;
                insert_expr(db, ids, &val.node, field_id, 0)?;
            }
        }
        Expr::MethodCall(base, method, args) => {
            put_expr(db, id, parent_id, "method_call", ordinal, Some(method))?;
            insert_expr(db, ids, &base.node, id, 0)?;
            for (i, arg) in args.iter().enumerate() {
                insert_expr(db, ids, &arg.node, id, (i + 1) as i64)?;
            }
        }
        Expr::Range { start, end, inclusive } => {
            let kind = if *inclusive { "range_inclusive" } else { "range_exclusive" };
            put_expr(db, id, parent_id, kind, ordinal, None)?;
            insert_expr(db, ids, &start.node, id, 0)?;
            insert_expr(db, ids, &end.node, id, 1)?;
        }
        Expr::Yield(inner) => {
            put_expr(db, id, parent_id, "yield", ordinal, None)?;
            insert_expr(db, ids, &inner.node, id, 0)?;
        }
    }

    Ok(id)
}

/// Helper to put a single expr row.
fn put_expr(
    db: &DbInstance,
    id: i64,
    parent_id: i64,
    kind: &str,
    ordinal: i64,
    value: Option<&str>,
) -> Result<(), String> {
    let value_str = value
        .map(|v| format!("'{}'", v.replace('\\', "\\\\").replace('\'', "\\'")))
        .unwrap_or_else(|| "null".to_string());
    let script = format!(
        "?[id, parent_id, kind, ordinal, value] <- [[{}, {}, '{}', {}, {}]]\n:put expr {{id => parent_id, kind, ordinal, value}}",
        id, parent_id, kind, ordinal, value_str
    );
    run_mut(db, &script).map_err(|e| format!("insert expr: {e}"))?;
    Ok(())
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

/// Parse a pattern string back from DB storage.
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

// ═══════════════════════════════════════════════════════════════════
// Query functions
// ═══════════════════════════════════════════════════════════════════

/// Query all child expressions of a parent, ordered by ordinal.
pub fn query_child_exprs(db: &DbInstance, parent_id: i64) -> Result<Vec<(i64, String, i64, Option<String>)>, String> {
    let script = format!(
        "?[id, kind, ordinal, value] := *expr{{id, parent_id: {}, kind, ordinal, value}} :order ordinal",
        parent_id
    );
    let result = run_query(db, &script).map_err(|e| format!("query child exprs: {e}"))?;

    let mut children = Vec::new();
    for row in result.rows {
        let id = row[0].get_int().unwrap_or(0);
        let kind = row[1].get_str().unwrap_or("").to_string();
        let ordinal = row[2].get_int().unwrap_or(0);
        let value = row[3].get_str().map(|s| s.to_string());
        children.push((id, kind, ordinal, value));
    }
    Ok(children)
}

/// Query match arms for a match expression, ordered by ordinal.
pub fn query_match_arms(db: &DbInstance, match_id: i64) -> Result<Vec<(i64, Vec<String>, Option<i64>, String)>, String> {
    let script = format!(
        "?[ordinal, patterns, body_expr_id, arm_kind] := *match_arm{{match_id: {}, ordinal, patterns, body_expr_id, arm_kind}} :order ordinal",
        match_id
    );
    let result = run_query(db, &script).map_err(|e| format!("query match_arms: {e}"))?;

    let mut arms = Vec::new();
    for row in result.rows {
        let ordinal = row[0].get_int().unwrap_or(0);
        let patterns: Vec<String> = row[1]
            .get_slice()
            .map(|arr| arr.iter().filter_map(|v| v.get_str().map(|s| s.to_string())).collect())
            .unwrap_or_default();
        let body_expr_id = row[2].get_int();
        let arm_kind = row[3].get_str().unwrap_or("commit").to_string();
        arms.push((ordinal, patterns, body_expr_id, arm_kind));
    }
    Ok(arms)
}

/// Query a single expression by id.
pub fn query_expr_by_id(db: &DbInstance, expr_id: i64) -> Result<Option<(String, Option<String>)>, String> {
    let script = format!(
        "?[kind, value] := *expr{{id: {}, kind, value}}",
        expr_id
    );
    let result = run_query(db, &script).map_err(|e| format!("query expr: {e}"))?;
    if let Some(row) = result.rows.first() {
        let kind = row[0].get_str().unwrap_or("").to_string();
        let value = row[1].get_str().map(|s| s.to_string());
        Ok(Some((kind, value)))
    } else {
        Ok(None)
    }
}

/// Query all top-level nodes ordered by id (preserving source order).
pub fn query_all_top_level_nodes(db: &DbInstance) -> Result<Vec<(i64, String, String)>, String> {
    let script = "?[id, kind, name] := *node{id, kind, name, parent: null} :order id";
    let result = run_query(db, script).map_err(|e| format!("query top-level: {e}"))?;

    let mut nodes = Vec::new();
    for row in result.rows {
        let id = row[0].get_int().unwrap_or(0);
        let kind = row[1].get_str().unwrap_or("").to_string();
        let name = row[2].get_str().unwrap_or("").to_string();
        nodes.push((id, kind, name));
    }
    Ok(nodes)
}

/// Query child nodes of a parent, ordered by id.
pub fn query_child_nodes(db: &DbInstance, parent_id: i64) -> Result<Vec<(i64, String, String)>, String> {
    let script = format!(
        "?[id, kind, name] := *node{{id, kind, name, parent: {}}} :order id",
        parent_id
    );
    let result = run_query(db, &script).map_err(|e| format!("query child nodes: {e}"))?;

    let mut nodes = Vec::new();
    for row in result.rows {
        let id = row[0].get_int().unwrap_or(0);
        let kind = row[1].get_str().unwrap_or("").to_string();
        let name = row[2].get_str().unwrap_or("").to_string();
        nodes.push((id, kind, name));
    }
    Ok(nodes)
}

/// Query parameters for a function/method node, ordered by ordinal.
pub fn query_params(db: &DbInstance, node_id: i64) -> Result<Vec<(String, Option<String>, Option<String>)>, String> {
    let script = format!(
        "?[kind, name, type_ref, ordinal] := *param{{node_id: {}, ordinal, kind, name, type_ref}} :order ordinal",
        node_id
    );
    let result = run_query(db, &script).map_err(|e| format!("query params: {e}"))?;

    let mut params = Vec::new();
    for row in result.rows {
        let kind = row[0].get_str().unwrap_or("").to_string();
        let name = row[1].get_str().map(|s| s.to_string());
        let type_ref = row[2].get_str().map(|s| s.to_string());
        params.push((kind, name, type_ref));
    }
    Ok(params)
}

/// Query return type for a function/method node.
pub fn query_return_type(db: &DbInstance, node_id: i64) -> Result<Option<String>, String> {
    let script = format!(
        "?[type_ref] := *returns{{node_id: {}, type_ref}}",
        node_id
    );
    let result = run_query(db, &script).map_err(|e| format!("query returns: {e}"))?;
    if let Some(row) = result.rows.first() {
        Ok(Some(row[0].get_str().unwrap_or("").to_string()))
    } else {
        Ok(None)
    }
}

/// Query constant by node_id.
pub fn query_constant(db: &DbInstance, node_id: i64) -> Result<Option<(String, String, bool)>, String> {
    let script = format!(
        "?[name, type_ref, has_value] := *constant{{node_id: {}, name, type_ref, has_value}}",
        node_id
    );
    let result = run_query(db, &script).map_err(|e| format!("query constant: {e}"))?;

    if let Some(row) = result.rows.first() {
        let name = row[0].get_str().unwrap_or("").to_string();
        let type_ref = row[1].get_str().unwrap_or("").to_string();
        let has_value = row[2].get_bool().unwrap_or(false);
        Ok(Some((name, type_ref, has_value)))
    } else {
        Ok(None)
    }
}

/// Query domain variants.
pub fn query_domain_variants(db: &DbInstance, domain_name: &str) -> Result<Vec<(i32, String, Option<String>)>, String> {
    let script = format!(
        "?[ordinal, name, wraps] := *node{{id, kind: 'domain', name: '{}'}}, *variant{{domain_id: id, ordinal, name, wraps}}",
        domain_name.replace('\'', "\\'")
    );
    let result = run_query(db, &script).map_err(|e| format!("query error: {e}"))?;

    let mut variants = Vec::new();
    for row in result.rows {
        let ordinal = row[0].get_int().unwrap_or(0) as i32;
        let name = row[1].get_str().unwrap_or("").to_string();
        let wraps = row[2].get_str().map(|s| s.to_string());
        variants.push((ordinal, name, wraps));
    }
    Ok(variants)
}

/// Query struct fields.
pub fn query_struct_fields(db: &DbInstance, struct_name: &str) -> Result<Vec<(i32, String, String)>, String> {
    let script = format!(
        "?[ordinal, name, type_ref] := *node{{id, kind: 'struct', name: '{}'}}, *field{{struct_id: id, ordinal, name, type_ref}}",
        struct_name.replace('\'', "\\'")
    );
    let result = run_query(db, &script).map_err(|e| format!("query error: {e}"))?;

    let mut fields = Vec::new();
    for row in result.rows {
        let ordinal = row[0].get_int().unwrap_or(0) as i32;
        let name = row[1].get_str().unwrap_or("").to_string();
        let type_ref = row[2].get_str().unwrap_or("").to_string();
        fields.push((ordinal, name, type_ref));
    }
    Ok(fields)
}

/// Query the kind of a node by its ID.
pub fn query_node_kind(db: &DbInstance, node_id: i64) -> Result<Option<String>, String> {
    let script = format!(
        "?[kind] := *node{{id: {}, kind}}",
        node_id
    );
    let result = run_query(db, &script).map_err(|e| format!("query node kind: {e}"))?;
    Ok(result.rows.first().and_then(|row| row[0].get_str().map(|s| s.to_string())))
}

/// Query all nodes of a given kind.
pub fn query_nodes_by_kind(db: &DbInstance, kind: &str) -> Result<Vec<(i64, String)>, String> {
    let script = format!(
        "?[id, name] := *node{{id, kind: '{}', name}}",
        kind.replace('\'', "\\'")
    );
    let result = run_query(db, &script).map_err(|e| format!("query error: {e}"))?;

    let mut nodes = Vec::new();
    for row in result.rows {
        let id = row[0].get_int().unwrap_or(0);
        let name = row[1].get_str().unwrap_or("").to_string();
        nodes.push((id, name));
    }
    Ok(nodes)
}

/// Query fields that need auto-boxing — where a type contains itself (directly or transitively).
/// Returns (containing_type, field_name, recursive_type) triples.
pub fn query_recursive_fields(db: &DbInstance) -> Result<Vec<(String, String, String)>, String> {
    // Datalog: find all types that transitively contain themselves via fields or variants.
    // A field references a type by name. If that type is the same as (or transitively contains)
    // the parent type, the field needs Box in codegen.
    let script = r#"
        contains[parent_type, field_type] :=
            *node{id: parent_id, kind: 'struct', name: parent_type},
            *field{struct_id: parent_id, type_ref: field_type}

        contains[parent_type, field_type] :=
            *node{id: parent_id, kind: 'domain', name: parent_type},
            *variant{domain_id: parent_id, wraps: field_type},
            not is_null(field_type)

        transitive[ancestor, descendant] := contains[ancestor, descendant]
        transitive[ancestor, descendant] := contains[ancestor, mid], transitive[mid, descendant]

        recursive_field[owner, field_name, field_type] :=
            *node{id: owner_id, kind: 'struct', name: owner},
            *field{struct_id: owner_id, name: field_name, type_ref: field_type},
            transitive[owner, field_type],
            field_type == owner

        recursive_field[owner, field_name, field_type] :=
            *node{id: owner_id, kind: 'struct', name: owner},
            *field{struct_id: owner_id, name: field_name, type_ref: field_type},
            transitive[field_type, owner]

        ?[owner, field_name, field_type] := recursive_field[owner, field_name, field_type]
    "#;
    let result = run_query(db, script).map_err(|e| format!("query recursive fields: {e}"))?;

    let mut fields = Vec::new();
    for row in result.rows {
        let owner = row[0].get_str().unwrap_or("").to_string();
        let field_name = row[1].get_str().unwrap_or("").to_string();
        let field_type = row[2].get_str().unwrap_or("").to_string();
        fields.push((owner, field_name, field_type));
    }
    Ok(fields)
}

/// Validate that no method return type references a body-scoped type.
/// A body-scoped type has a non-null parent in the node relation.
/// Returns a list of (method_name, type_name) violations.
pub fn validate_return_type_scope(db: &DbInstance) -> Result<Vec<(String, String)>, String> {
    // Find all return types that reference a type name which exists as a
    // node with a non-null parent (meaning it's defined inside a body,
    // not at module scope).
    // CozoDB: null parent means module-scope. Non-null parent means body-scoped.
    // We find types with a parent that matches any existing node id.
    let script = r#"
        body_scoped[type_name] :=
            *node{kind, name: type_name, parent},
            not is_null(parent),
            kind in ['struct', 'domain']
        ?[method_name, type_name] :=
            *returns{node_id, type_ref: type_name},
            *node{id: node_id, name: method_name},
            body_scoped[type_name]
    "#;
    let result = run_query(db, script).map_err(|e| format!("validate return scope: {e}"))?;

    let mut violations = Vec::new();
    for row in result.rows {
        let method_name = row[0].get_str().unwrap_or("").to_string();
        let type_name = row[1].get_str().unwrap_or("").to_string();
        violations.push((method_name, type_name));
    }
    Ok(violations)
}

/// Validate that no struct-domain has String fields.
/// Strings are boundary-only (StdOut, StdIn, FFI) — not interior data.
/// Returns (struct_name, field_name) violations.
pub fn validate_no_string_fields(db: &DbInstance) -> Result<Vec<(String, String)>, String> {
    let script = r#"
        ?[struct_name, field_name] :=
            *node{id, kind: 'struct', name: struct_name},
            *field{struct_id: id, name: field_name, type_ref: 'String'}
    "#;
    let result = run_query(db, script).map_err(|e| format!("validate no strings: {e}"))?;
    let mut violations = Vec::new();
    for row in result.rows {
        let struct_name = row[0].get_str().unwrap_or("").to_string();
        let field_name = row[1].get_str().unwrap_or("").to_string();
        violations.push((struct_name, field_name));
    }
    Ok(violations)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_source;

    #[test]
    fn db_roundtrip_domain() {
        let db = create_db().unwrap();
        let items = parse_source("Element (Fire Earth Air Water)").unwrap();
        insert_ast(&db, &items).unwrap();

        let variants = query_domain_variants(&db, "Element").unwrap();
        assert_eq!(variants.len(), 4);
        assert_eq!(variants[0].1, "Fire");
        assert_eq!(variants[3].1, "Water");
    }

    #[test]
    fn db_roundtrip_struct() {
        let db = create_db().unwrap();
        let items = parse_source("Point { X F64 Y F64 }").unwrap();
        insert_ast(&db, &items).unwrap();

        let fields = query_struct_fields(&db, "Point").unwrap();
        assert_eq!(fields.len(), 2);
        assert_eq!(fields[0].1, "X");
        assert_eq!(fields[0].2, "F64");
    }

    #[test]
    fn db_roundtrip_inherent_method() {
        let db = create_db().unwrap();
        let items = parse_source("Addition [ add(:@Self) U32 [ ^(@Self.Left + @Self.Right) ] ]").unwrap();
        insert_ast(&db, &items).unwrap();

        let nodes = query_nodes_by_kind(&db, "inherent_impl").unwrap();
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].1, "Addition");
    }

    #[test]
    fn db_roundtrip_main() {
        let db = create_db().unwrap();
        let items = parse_source("Main [ StdOut \"hello\" ]").unwrap();
        insert_ast(&db, &items).unwrap();

        let nodes = query_nodes_by_kind(&db, "main").unwrap();
        assert_eq!(nodes.len(), 1);
    }

    #[test]
    fn db_roundtrip_const() {
        let db = create_db().unwrap();
        let items = parse_source("!Pi F64 {3.14159265358979}").unwrap();
        insert_ast(&db, &items).unwrap();

        let consts = query_nodes_by_kind(&db, "const").unwrap();
        assert_eq!(consts.len(), 1);
    }

    #[test]
    fn validate_module_scope_return_type_passes() {
        // Point is module-scoped, returning it is fine
        let db = create_db().unwrap();
        let src = "Point { X F64 Y F64 }\nPoint [ translate(:@Self) Point [ ^@Self ] ]";
        let items = parse_source(src).unwrap();
        insert_ast(&db, &items).unwrap();

        let violations = validate_return_type_scope(&db).unwrap();
        assert!(violations.is_empty(), "expected no violations, got {:?}", violations);
    }

    #[test]
    fn validate_body_scoped_return_type_rejected() {
        // Simulate a body-scoped type by manually inserting a node with a parent
        let db = create_db().unwrap();

        // Create a method node
        run_mut(&db, "?[id, kind, name, parent, span_start, span_end] <- [[1, 'inherent_impl', 'MyType', null, 0, 10]]\n:put node {id => kind, name, parent, span_start, span_end}").unwrap();
        run_mut(&db, "?[id, kind, name, parent, span_start, span_end] <- [[2, 'method', 'doWork', 1, 0, 10]]\n:put node {id => kind, name, parent, span_start, span_end}").unwrap();

        // Method returns 'Inner' type
        run_mut(&db, "?[node_id, type_ref] <- [[2, 'Inner']]\n:put returns {node_id => type_ref}").unwrap();

        // 'Inner' is body-scoped (parent = method node 2)
        run_mut(&db, "?[id, kind, name, parent, span_start, span_end] <- [[3, 'struct', 'Inner', 2, 0, 10]]\n:put node {id => kind, name, parent, span_start, span_end}").unwrap();

        let violations = validate_return_type_scope(&db).unwrap();
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].0, "doWork");
        assert_eq!(violations[0].1, "Inner");
    }

    #[test]
    fn validate_no_string_fields_passes_for_domain_types() {
        let db = create_db().unwrap();
        let src = "Point { X F64 Y F64 }\nSign (Aries Taurus)";
        let items = parse_source(src).unwrap();
        insert_ast(&db, &items).unwrap();

        let violations = validate_no_string_fields(&db).unwrap();
        assert!(violations.is_empty());
    }

    #[test]
    fn validate_string_field_rejected() {
        let db = create_db().unwrap();
        let src = "BadStruct { Name String Value F64 }";
        let items = parse_source(src).unwrap();
        insert_ast(&db, &items).unwrap();

        let violations = validate_no_string_fields(&db).unwrap();
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].0, "BadStruct");
        assert_eq!(violations[0].1, "Name");
    }
}
