//! Kernel codegen — generates the World struct, queries, and derivation from kernel.aski.
//!
//! Unlike the standard codegen (which emits Rust application code from aski sources),
//! this mode generates the *compiler infrastructure itself*: the relational World
//! that the compiler and codegen query.
//!
//! Input: kernel.aski parsed into the old IR World
//! Output: Rust source defining World struct, query methods, and derive()

use crate::ir;

/// Generate the kernel World module from parsed kernel.aski.
///
/// The IR World must already be populated with the parsed kernel.aski contents.
/// Returns Rust source code that defines:
/// - Enums (NodeKind, ParamKind, etc.) with Display impls
/// - Structs (Node, Variant, etc.) with named fields
/// - `World` struct with `Vec<T>` for each relation
/// - Query methods on `World`
/// - `World::derive()` implementing fixed-point derivation rules
pub fn generate_kernel(world: &ir::World) -> Result<String, String> {
    let mut out = String::new();

    // Collect all struct definitions from kernel.aski
    let nodes = ir::query_all_top_level_nodes(world)?;
    let mut domains: Vec<(i64, String)> = Vec::new();
    let mut structs: Vec<(i64, String)> = Vec::new();

    for (id, kind, name) in &nodes {
        match kind.as_str() {
            "domain" => domains.push((*id, name.clone())),
            "struct" => structs.push((*id, name.clone())),
            _ => {}
        }
    }

    // ── Enums ──
    for (id, name) in &domains {
        let variants = ir::query_domain_variants(world, name)?;
        out.push_str("#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]\n");
        out.push_str(&format!("pub enum {} {{\n", name));
        for (_ord, vname, _wraps) in &variants {
            out.push_str(&format!("    {},\n", vname));
        }
        out.push_str("}\n\n");

        // Display impl
        out.push_str(&format!("impl std::fmt::Display for {} {{\n", name));
        out.push_str("    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {\n");
        out.push_str("        write!(f, \"{:?}\", self)\n");
        out.push_str("    }\n");
        out.push_str("}\n\n");

        // FromStr for matching string literals to enum variants
        out.push_str(&format!("impl {} {{\n", name));
        out.push_str("    pub fn from_str(s: &str) -> Option<Self> {\n");
        out.push_str("        match s {\n");
        for (_ord, vname, _wraps) in &variants {
            // snake_case version for matching (e.g. "tail_method" -> TailMethod)
            let snake = pascal_to_snake(vname);
            out.push_str(&format!("            \"{}\" => Some(Self::{}),\n", snake, vname));
            // Also match the PascalCase name
            if snake != vname.to_lowercase() {
                out.push_str(&format!("            \"{}\" => Some(Self::{}),\n", vname, vname));
            }
        }
        out.push_str("            _ => None,\n");
        out.push_str("        }\n");
        out.push_str("    }\n\n");
        // to_str
        out.push_str("    pub fn to_str(&self) -> &'static str {\n");
        out.push_str("        match self {\n");
        for (_ord, vname, _wraps) in &variants {
            let snake = pascal_to_snake(vname);
            out.push_str(&format!("            Self::{} => \"{}\",\n", vname, snake));
        }
        out.push_str("        }\n");
        out.push_str("    }\n");
        out.push_str("}\n\n");

        let _ = id;
    }

    // ── Structs (exclude World — it gets special treatment) ──
    for (id, name) in &structs {
        if name == "World" { continue; }
        let fields = ir::query_struct_fields(world, name)?;
        out.push_str("#[derive(Debug, Clone, PartialEq)]\n");
        out.push_str(&format!("pub struct {} {{\n", name));
        for (_ord, fname, ftype) in &fields {
            let rust_type = aski_type_to_rust(ftype);
            out.push_str(&format!("    pub {}: {},\n", to_snake(fname), rust_type));
        }
        out.push_str("}\n\n");

        let _ = id;
    }

    // ── World struct — read from kernel.aski's World definition ──
    let world_fields = ir::query_struct_fields(world, "World")?;
    // Collect (snake_field_name, element_type, original_name) for each Vec field
    let mut world_vec_fields: Vec<(String, String, String)> = Vec::new();

    out.push_str("/// Kernel World — holds all relations as Vec<T>.\n");
    out.push_str("#[derive(Debug, Clone, Default)]\n");
    out.push_str("pub struct World {\n");
    for (_ord, fname, ftype) in &world_fields {
        let snake_field = to_snake(fname);
        let rust_type = aski_type_to_rust(ftype);
        out.push_str(&format!("    pub {}: {},\n", snake_field, rust_type));
        // Track Vec fields for query generation
        if ftype.starts_with("Vec(") || ftype.contains("Vec") {
            // Extract element type from "Vec(Node)" or similar
            let elem = ftype.strip_prefix("Vec(").and_then(|s| s.strip_suffix(')'))
                .unwrap_or(ftype);
            world_vec_fields.push((snake_field.clone(), elem.to_string(), fname.clone()));
        }
    }
    out.push_str("}\n\n");

    // ── Query methods — for each Vec field, generate lookups by element fields ──
    out.push_str("impl World {\n");
    out.push_str("    pub fn new() -> Self { Self::default() }\n\n");

    for (vec_field, elem_type, _orig_name) in &world_vec_fields {
        // Find the struct definition for the element type
        let elem_fields = ir::query_struct_fields(world, elem_type)?;
        if elem_fields.is_empty() { continue; }

        let type_snake = to_snake(elem_type);
        for (_ord, fname, ftype) in &elem_fields {
            let snake_field = to_snake(fname);
            let rust_type = aski_type_to_rust(ftype);
            let param_type = query_param_type(&rust_type);
            let filter_expr = query_filter_expr(&snake_field, &rust_type);

            out.push_str(&format!(
                "    pub fn {}_by_{}(&self, val: {}) -> Vec<&{}> {{\n",
                type_snake, snake_field, param_type, elem_type
            ));
            out.push_str(&format!(
                "        self.{}.iter().filter(|r| {}).collect()\n",
                vec_field, filter_expr
            ));
            out.push_str("    }\n\n");
        }
    }

    // ── derive() and all derivation rules — generated from AST ──
    generate_derive_from_ast(&mut out, world)?;

    out.push_str("}\n"); // close impl World

    Ok(out)
}

// ════════════════════════════════════════���══════════════════════
// AST-driven derive codegen
// ═══════════════════════════════════════════════════════════════

use aski_core::{ExprKind, NodeKind};

/// A loop variable binding: var name + element type name
struct Binding {
    var_name: String,
    type_name: String,
}

/// Decomposed pipeline node
enum PipelineNode {
    Map { source: SourceChain, constructor_id: i64 },
    FlatMap { source: SourceChain, inner: Box<PipelineNode> },
    Chain { left: Box<PipelineNode>, right: Box<PipelineNode> },
}

/// Source with filter chain
struct SourceChain {
    collection: String,
    filter_ids: Vec<i64>,
}

fn generate_derive_from_ast(out: &mut String, world: &ir::World) -> Result<(), String> {
    let derive_impl = world.trait_impls.iter()
        .find(|ti| ti.trait_name == "derive" && ti.type_name == "World")
        .ok_or("no derive trait impl on World in kernel.aski")?;

    let impl_body_id = derive_impl.impl_node_id;
    let mut methods: Vec<_> = world.nodes.iter()
        .filter(|n| n.parent == impl_body_id &&
                (n.kind == NodeKind::Method || n.kind == NodeKind::TailMethod))
        .collect();
    methods.sort_by_key(|n| n.id);

    // ── derive() dispatcher ──
    let derive_method = methods.iter().find(|m| m.name == "derive")
        .ok_or("no derive() method in derive impl")?;
    out.push_str("    /// Run all derivation rules to fixed point.\n");
    out.push_str("    pub fn derive(&mut self) {\n");
    let body_exprs = get_child_exprs(world, derive_method.id);
    for expr in &body_exprs {
        if expr.kind == ExprKind::MethodCall {
            let name = &expr.value;
            let rust_name = pascal_to_snake(name);
            let is_fp = methods.iter().any(|m| m.name == *name && m.kind == NodeKind::TailMethod);
            if is_fp {
                out.push_str(&format!("        self.{}_fixpoint();\n", rust_name));
            } else {
                out.push_str(&format!("        self.{}();\n", rust_name));
            }
        }
    }
    out.push_str("    }\n\n");

    // ── Each derive method ──
    for method in &methods {
        if method.name == "derive" { continue; }
        let rust_name = pascal_to_snake(&method.name);
        let result = if method.kind == NodeKind::TailMethod {
            gen_fixpoint_method(out, world, method, &rust_name)
        } else {
            gen_simple_method(out, world, method, &rust_name)
        };
        result.map_err(|e| format!("in derive method '{}': {}", method.name, e))?;
    }

    Ok(())
}

fn gen_simple_method(out: &mut String, world: &ir::World, method: &aski_core::Node, rust_name: &str) -> Result<(), String> {
    out.push_str(&format!("    fn {}(&mut self) {{\n", rust_name));
    out.push_str("        let mut results = Vec::new();\n");

    let body_exprs = get_child_exprs(world, method.id);
    let stmt = body_exprs.first().ok_or(format!("empty body in {}", method.name))?;
    if stmt.kind != ExprKind::MutableSet {
        return Err(format!("expected MutableSet in {}, got {:?}", method.name, stmt.kind));
    }
    let field_name = extract_set_field(&stmt.value);
    let pipeline_children = get_child_exprs(world, stmt.id);
    let pipeline = decompose_pipeline(world, pipeline_children[0].id)?;
    let mut bindings = Vec::new();
    emit_pipeline(out, world, &pipeline, "results", 2, &mut bindings)?;
    out.push_str(&format!("        self.{} = results;\n", field_name));
    out.push_str("    }\n\n");
    Ok(())
}

fn gen_fixpoint_method(out: &mut String, world: &ir::World, method: &aski_core::Node, rust_name: &str) -> Result<(), String> {
    out.push_str(&format!("    fn {}_fixpoint(&mut self) {{\n", rust_name));

    let body_exprs = get_child_exprs(world, method.id);
    // Statement 0: MutableSet for initial set
    let set_stmt = &body_exprs[0];
    let field_name = extract_set_field(&set_stmt.value);

    // Initial set
    out.push_str("        {\n");
    out.push_str("            let mut results = Vec::new();\n");
    let init_children = get_child_exprs(world, set_stmt.id);
    let init_pipeline = decompose_pipeline(world, init_children[0].id)?;
    let mut bindings = Vec::new();
    emit_pipeline(out, world, &init_pipeline, "results", 3, &mut bindings)?;
    out.push_str(&format!("            self.{} = results;\n", field_name));
    out.push_str("        }\n");

    // Fixpoint loop
    out.push_str("        loop {\n");
    // Statement 1: SubTypeNew or MutableNew for extension pipeline
    let ext_stmt = &body_exprs[1];
    let ext_children = get_child_exprs(world, ext_stmt.id);
    out.push_str("            let mut new_items = Vec::new();\n");
    let ext_pipeline = decompose_pipeline(world, ext_children[0].id)?;
    let mut bindings = Vec::new();
    emit_pipeline(out, world, &ext_pipeline, "new_items", 3, &mut bindings)?;
    out.push_str(&format!("            new_items.retain(|item| !self.{}.contains(item));\n", field_name));
    out.push_str("            if new_items.is_empty() { break; }\n");
    out.push_str(&format!("            self.{}.extend(new_items);\n", field_name));
    out.push_str("        }\n");
    out.push_str("    }\n\n");
    Ok(())
}

// ── Pipeline decomposition ──

fn decompose_pipeline(world: &ir::World, expr_id: i64) -> Result<PipelineNode, String> {
    let expr = find_expr(world, expr_id)?;
    let children = get_child_exprs(world, expr_id);

    if expr.kind == ExprKind::MethodCall {
        match expr.value.as_str() {
            "map" => {
                let source = decompose_source(world, children[0].id)?;
                Ok(PipelineNode::Map { source, constructor_id: children[1].id })
            }
            "flatMap" => {
                let source = decompose_source(world, children[0].id)?;
                let inner = decompose_pipeline(world, children[1].id)?;
                Ok(PipelineNode::FlatMap { source, inner: Box::new(inner) })
            }
            "chain" => {
                let left = decompose_pipeline(world, children[0].id)?;
                let right = decompose_pipeline(world, children[1].id)?;
                Ok(PipelineNode::Chain { left: Box::new(left), right: Box::new(right) })
            }
            other => Err(format!("unexpected pipeline op: {}", other))
        }
    } else {
        Err(format!("expected MethodCall in pipeline, got {:?}", expr.kind))
    }
}

fn decompose_source(world: &ir::World, expr_id: i64) -> Result<SourceChain, String> {
    let expr = find_expr(world, expr_id)?;
    let children = get_child_exprs(world, expr_id);

    if expr.kind == ExprKind::MethodCall && expr.value == "filter" {
        let mut chain = decompose_source(world, children[0].id)?;
        chain.filter_ids.push(children[1].id);
        Ok(chain)
    } else if expr.kind == ExprKind::Access {
        Ok(SourceChain {
            collection: to_snake(&expr.value),
            filter_ids: vec![],
        })
    } else {
        Err(format!("unexpected source expr: {:?} {:?}", expr.kind, expr.value))
    }
}

// ── Pipeline emission ──

fn emit_pipeline(
    out: &mut String, world: &ir::World, node: &PipelineNode,
    results_var: &str, indent: usize, bindings: &mut Vec<Binding>,
) -> Result<(), String> {
    match node {
        PipelineNode::Map { source, constructor_id } => {
            let loop_var = determine_loop_var(world, source, Some(*constructor_id), bindings);
            let elem_type = element_type_for_collection(world, &source.collection);
            let ind = "    ".repeat(indent);
            out.push_str(&format!("{}for {} in &self.{} {{\n", ind, loop_var, source.collection));
            bindings.push(Binding { var_name: loop_var.clone(), type_name: elem_type });
            let filter_count = source.filter_ids.len();
            for (i, &fid) in source.filter_ids.iter().enumerate() {
                let cond = translate_predicate(world, fid, bindings)?;
                let fi = "    ".repeat(indent + 1 + i);
                out.push_str(&format!("{}if {} {{\n", fi, cond));
            }
            let ci = "    ".repeat(indent + 1 + filter_count);
            let value = translate_constructor(world, *constructor_id, bindings)?;
            out.push_str(&format!("{}{}.push({});\n", ci, results_var, value));
            for i in (0..filter_count).rev() {
                let fi = "    ".repeat(indent + 1 + i);
                out.push_str(&format!("{}}}\n", fi));
            }
            out.push_str(&format!("{}}}\n", ind));
            bindings.pop();
            Ok(())
        }
        PipelineNode::FlatMap { source, inner } => {
            let loop_var = determine_loop_var_for_flatmap(world, source, inner, bindings);
            let elem_type = element_type_for_collection(world, &source.collection);
            let ind = "    ".repeat(indent);
            out.push_str(&format!("{}for {} in &self.{} {{\n", ind, loop_var, source.collection));
            bindings.push(Binding { var_name: loop_var.clone(), type_name: elem_type });
            let filter_count = source.filter_ids.len();
            for (i, &fid) in source.filter_ids.iter().enumerate() {
                let cond = translate_predicate(world, fid, bindings)?;
                let fi = "    ".repeat(indent + 1 + i);
                out.push_str(&format!("{}if {} {{\n", fi, cond));
            }
            emit_pipeline(out, world, inner, results_var, indent + 1 + filter_count, bindings)?;
            for i in (0..filter_count).rev() {
                let fi = "    ".repeat(indent + 1 + i);
                out.push_str(&format!("{}}}\n", fi));
            }
            out.push_str(&format!("{}}}\n", ind));
            bindings.pop();
            Ok(())
        }
        PipelineNode::Chain { left, right } => {
            emit_pipeline(out, world, left, results_var, indent, bindings)?;
            emit_pipeline(out, world, right, results_var, indent, bindings)?;
            Ok(())
        }
    }
}

// ── Loop variable determination ──

fn determine_loop_var(world: &ir::World, source: &SourceChain, constructor_id: Option<i64>, bindings: &[Binding]) -> String {
    // Scan filter predicates for new instance references
    for &fid in &source.filter_ids {
        if let Some(name) = first_new_ref(world, fid, bindings) {
            return name;
        }
    }
    // Scan constructor
    if let Some(cid) = constructor_id {
        if let Some(name) = first_new_ref(world, cid, bindings) {
            return name;
        }
    }
    // Fallback: singularize collection
    singularize(&source.collection)
}

fn determine_loop_var_for_flatmap(world: &ir::World, source: &SourceChain, _inner: &PipelineNode, bindings: &[Binding]) -> String {
    // Try filters first
    for &fid in &source.filter_ids {
        if let Some(name) = first_new_ref(world, fid, bindings) {
            return name;
        }
    }
    // Fallback: singularize collection
    singularize(&source.collection)
}

fn first_new_ref(world: &ir::World, expr_id: i64, bindings: &[Binding]) -> Option<String> {
    let refs = scan_instance_refs(world, expr_id);
    for r in refs {
        if r == "Self" { continue; }
        let snake = to_snake(&r);
        if !bindings.iter().any(|b| b.var_name == snake) {
            return Some(snake);
        }
    }
    None
}

fn scan_instance_refs(world: &ir::World, expr_id: i64) -> Vec<String> {
    let Some(expr) = world.exprs.iter().find(|e| e.id == expr_id) else { return vec![] };
    let mut result = Vec::new();
    if expr.kind == ExprKind::InstanceRef {
        result.push(expr.value.clone());
    }
    for child in get_child_exprs(world, expr_id) {
        result.extend(scan_instance_refs(world, child.id));
    }
    result
}

fn element_type_for_collection(world: &ir::World, collection: &str) -> String {
    let fields = ir::query_struct_fields(world, "World").unwrap_or_default();
    for (_ord, fname, ftype) in &fields {
        if to_snake(fname) == collection {
            if let Some(inner) = ftype.strip_prefix("Vec(").and_then(|s| s.strip_suffix(')')) {
                return inner.to_string();
            }
        }
    }
    collection.to_string()
}

fn singularize(s: &str) -> String {
    if s.ends_with("ies") {
        format!("{}y", &s[..s.len()-3])
    } else if s.ends_with("ses") || s.ends_with("shes") || s.ends_with("ches") {
        s[..s.len()-2].to_string()
    } else if s.ends_with('s') {
        s[..s.len()-1].to_string()
    } else {
        s.to_string()
    }
}

// ── Expression translation ──

fn translate_predicate(world: &ir::World, expr_id: i64, bindings: &[Binding]) -> Result<String, String> {
    translate_expr(world, expr_id, bindings, None)
}

fn translate_constructor(world: &ir::World, expr_id: i64, bindings: &[Binding]) -> Result<String, String> {
    translate_expr(world, expr_id, bindings, None)
}

fn translate_expr(world: &ir::World, expr_id: i64, bindings: &[Binding], type_hint: Option<&str>) -> Result<String, String> {
    let expr = find_expr(world, expr_id)?;
    let children = get_child_exprs(world, expr_id);

    match expr.kind {
        ExprKind::BinOp => {
            let op = &expr.value;
            if is_comparison(op) {
                let left_type = determine_expr_type(world, children[0].id, bindings);
                let left = translate_expr(world, children[0].id, bindings, None)?;
                let right = translate_expr(world, children[1].id, bindings, left_type.as_deref())?;
                // != "" → !x.is_empty()
                if op == "!=" && right == "\"\"" {
                    return Ok(format!("!{}.is_empty()", left));
                }
                // == false → !x
                if op == "==" && right == "false" {
                    return Ok(format!("!{}", left));
                }
                Ok(format!("{} {} {}", left, op, right))
            } else {
                let left = translate_expr(world, children[0].id, bindings, None)?;
                let right = translate_expr(world, children[1].id, bindings, None)?;
                Ok(format!("({} {} {})", left, op, right))
            }
        }
        ExprKind::Access => {
            let field = &expr.value;
            let base = translate_expr(world, children[0].id, bindings, None)?;
            match field.as_str() {
                "beforeColon" => Ok(format!("{}[..{}.find(':').unwrap()].to_string()", base, base)),
                "afterColon" => Ok(format!("{}[{}.find(':').unwrap()+1..].to_string()", base, base)),
                _ => Ok(format!("{}.{}", base, to_snake(field)))
            }
        }
        ExprKind::InstanceRef => {
            if expr.value == "Self" {
                Ok("self".to_string())
            } else {
                Ok(to_snake(&expr.value))
            }
        }
        ExprKind::BareName => {
            match expr.value.as_str() {
                "True" => Ok("true".to_string()),
                "False" => Ok("false".to_string()),
                name => {
                    // Try type hint first for disambiguation
                    if let Some(hint) = type_hint {
                        let variants = ir::query_domain_variants(world, hint).unwrap_or_default();
                        if variants.iter().any(|(_, vname, _)| vname == name) {
                            return Ok(format!("{}::{}", hint, name));
                        }
                    }
                    // Search all domains
                    if let Some(domain) = find_variant_domain(world, name) {
                        Ok(format!("{}::{}", domain, name))
                    } else {
                        Ok(name.to_string())
                    }
                }
            }
        }
        ExprKind::IntLit => Ok(expr.value.clone()),
        ExprKind::StringLit => {
            if expr.value.contains("$@") {
                translate_interpolated_string(&expr.value, bindings)
            } else {
                Ok(format!("\"{}\"", expr.value))
            }
        }
        ExprKind::MethodCall => {
            let method = &expr.value;
            let base = translate_expr(world, children[0].id, bindings, None)?;
            match method.as_str() {
                "contains" => {
                    let arg = translate_expr(world, children[1].id, bindings, None)?;
                    Ok(format!("{}.contains({})", base, arg))
                }
                _ => {
                    let args: Vec<String> = children[1..].iter()
                        .map(|c| translate_expr(world, c.id, bindings, None))
                        .collect::<Result<Vec<_>, _>>()?;
                    Ok(format!("{}.{}({})", base, to_snake(method), args.join(", ")))
                }
            }
        }
        ExprKind::StructConstruct => {
            let type_name = &expr.value;
            let mut field_strs = Vec::new();
            for field_expr in &children {
                if field_expr.kind != ExprKind::StructField { continue; }
                let fname = &field_expr.value;
                let rust_field = to_snake(fname);
                let val_children = get_child_exprs(world, field_expr.id);
                let field_type = get_struct_field_type(world, type_name, fname);
                let value = translate_expr(world, val_children[0].id, bindings, field_type.as_deref())?;
                let needs_clone = field_type.as_deref() == Some("String");
                let val_str = if needs_clone {
                    format!("{}.clone()", value)
                } else {
                    value
                };
                field_strs.push(format!("{}: {}", rust_field, val_str));
            }
            Ok(format!("{} {{ {} }}", type_name, field_strs.join(", ")))
        }
        _ => Err(format!("unhandled expr kind: {:?} (id={}, parent={}, value={:?})", expr.kind, expr.id, expr.parent_id, expr.value))
    }
}

fn is_comparison(op: &str) -> bool {
    matches!(op, "==" | "!=" | "<" | ">" | "<=" | ">=")
}

fn determine_expr_type(world: &ir::World, expr_id: i64, bindings: &[Binding]) -> Option<String> {
    let expr = world.exprs.iter().find(|e| e.id == expr_id)?;
    if expr.kind == ExprKind::Access {
        let children = get_child_exprs(world, expr_id);
        let base = children.first()?;
        if base.kind == ExprKind::InstanceRef && base.value != "Self" {
            let binding = bindings.iter().find(|b| b.var_name == to_snake(&base.value))?;
            let fields = ir::query_struct_fields(world, &binding.type_name).unwrap_or_default();
            for (_ord, fname, ftype) in &fields {
                if fname == &expr.value {
                    return Some(ftype.clone());
                }
            }
        }
    }
    None
}

fn get_struct_field_type(world: &ir::World, struct_name: &str, field_name: &str) -> Option<String> {
    let fields = ir::query_struct_fields(world, struct_name).unwrap_or_default();
    for (_ord, fname, ftype) in &fields {
        if fname == field_name {
            return Some(ftype.clone());
        }
    }
    None
}

fn find_variant_domain(world: &ir::World, variant_name: &str) -> Option<String> {
    for variant in &world.variants {
        if variant.name == variant_name {
            for node in &world.nodes {
                if node.id == variant.domain_id && node.kind == NodeKind::Domain {
                    return Some(node.name.clone());
                }
            }
        }
    }
    None
}

fn translate_interpolated_string(s: &str, bindings: &[Binding]) -> Result<String, String> {
    // Parse "$@Ref.Field" patterns → format!() args
    let mut fmt_str = String::new();
    let mut args = Vec::new();
    let mut i = 0;
    let chars: Vec<char> = s.chars().collect();
    while i < chars.len() {
        if i + 1 < chars.len() && chars[i] == '$' && chars[i+1] == '@' {
            fmt_str.push_str("{}");
            i += 2; // skip $@
            // Read Ref
            let ref_start = i;
            while i < chars.len() && chars[i].is_alphanumeric() { i += 1; }
            let ref_name = &s[ref_start..i];
            // Expect .
            if i < chars.len() && chars[i] == '.' { i += 1; }
            // Read Field
            let field_start = i;
            while i < chars.len() && chars[i].is_alphanumeric() { i += 1; }
            let field_name = &s[field_start..i];
            let var = to_snake(ref_name);
            let field = to_snake(field_name);
            args.push(format!("{}.{}", var, field));
        } else {
            fmt_str.push(chars[i]);
            i += 1;
        }
    }
    let _ = bindings;
    Ok(format!("format!(\"{}\", {})", fmt_str, args.join(", ")))
}

// ── IR helpers ──

fn find_expr<'a>(world: &'a ir::World, expr_id: i64) -> Result<&'a aski_core::Expr, String> {
    world.exprs.iter().find(|e| e.id == expr_id)
        .ok_or_else(|| format!("expr {} not found", expr_id))
}

fn get_child_exprs(world: &ir::World, parent_id: i64) -> Vec<&aski_core::Expr> {
    let mut kids: Vec<_> = world.exprs.iter()
        .filter(|e| e.parent_id == parent_id && parent_id != 0)
        .collect();
    kids.sort_by_key(|e| e.ordinal);
    kids
}

fn extract_set_field(mutable_set_value: &str) -> String {
    // "Self.VariantOfs" → "variant_ofs"
    let parts: Vec<&str> = mutable_set_value.split('.').collect();
    if parts.len() >= 2 {
        to_snake(parts[1])
    } else {
        to_snake(mutable_set_value)
    }
}

// ═══════════════════════════════════════════════════════════════
// Helpers
// ═══════════════════════════════════════════════════════════════

fn pascal_to_snake(s: &str) -> String {
    let mut result = String::new();
    for (i, c) in s.chars().enumerate() {
        if c.is_uppercase() && i > 0 {
            result.push('_');
        }
        result.push(c.to_lowercase().next().unwrap());
    }
    result
}

fn to_snake(s: &str) -> String {
    pascal_to_snake(s)
}


fn aski_type_to_rust(t: &str) -> String {
    // Handle parameterized types: "Vec(Node)" → "Vec<Node>"
    if let Some(inner) = t.strip_prefix("Vec(").and_then(|s| s.strip_suffix(')')) {
        return format!("Vec<{}>", aski_type_to_rust(inner));
    }
    match t {
        "I64" => "i64".to_string(),
        "F64" => "f64".to_string(),
        "String" => "String".to_string(),
        "Bool" => "bool".to_string(),
        other => other.to_string(), // domain/struct references stay as-is
    }
}

fn query_param_type(rust_type: &str) -> String {
    match rust_type {
        "i64" => "i64".to_string(),
        "f64" => "f64".to_string(),
        "bool" => "bool".to_string(),
        "String" => "&str".to_string(),
        other => other.to_string(), // enum types pass by value (Copy)
    }
}

fn query_filter_expr(field: &str, rust_type: &str) -> String {
    match rust_type {
        "String" => format!("r.{} == val", field),
        _ => format!("r.{} == val", field),
    }
}
