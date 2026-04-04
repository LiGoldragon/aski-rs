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

    // ── Structs ──
    for (id, name) in &structs {
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

    // ── World struct ──
    out.push_str("/// Kernel World — holds all relations as Vec<T>.\n");
    out.push_str("#[derive(Debug, Clone, Default)]\n");
    out.push_str("pub struct World {\n");
    for (_id, name) in &structs {
        let field = to_snake_plural(name);
        out.push_str(&format!("    pub {}: Vec<{}>,\n", field, name));
    }
    out.push_str("}\n\n");

    // ── Query methods ──
    out.push_str("impl World {\n");
    out.push_str("    pub fn new() -> Self { Self::default() }\n\n");

    for (_id, name) in &structs {
        let fields = ir::query_struct_fields(world, name)?;
        let plural = to_snake_plural(name);

        // For each field, generate a query-by-field method
        for (_ord, fname, ftype) in &fields {
            let snake_field = to_snake(fname);
            let rust_type = aski_type_to_rust(&ftype);
            let param_type = query_param_type(&rust_type);
            let filter_expr = query_filter_expr(&snake_field, &rust_type);

            out.push_str(&format!(
                "    pub fn {}_by_{}(&self, val: {}) -> Vec<&{}> {{\n",
                to_snake(name), snake_field, param_type, name
            ));
            out.push_str(&format!(
                "        self.{}.iter().filter(|r| {}).collect()\n",
                plural, filter_expr
            ));
            out.push_str("    }\n\n");
        }
    }

    // ── derive() — fixed-point derivation rules ──
    out.push_str("    /// Run all derivation rules to fixed point.\n");
    out.push_str("    pub fn derive(&mut self) {\n");
    out.push_str("        self.derive_variant_of();\n");
    out.push_str("        self.derive_binding_info();\n");
    out.push_str("        self.derive_type_kind();\n");
    out.push_str("        self.derive_method_on_type();\n");
    out.push_str("        self.derive_contained_type();\n");
    out.push_str("        // Recursive: run until stable\n");
    out.push_str("        self.derive_qualified_names_fixpoint();\n");
    out.push_str("        self.derive_can_see_fixpoint();\n");
    out.push_str("        self.derive_recursive_type_fixpoint();\n");
    out.push_str("    }\n\n");

    // ── Non-recursive derivation rules ──
    generate_derive_variant_of(&mut out);
    generate_derive_binding_info(&mut out);
    generate_derive_type_kind(&mut out);
    generate_derive_method_on_type(&mut out);
    generate_derive_contained_type(&mut out);

    // ── Recursive (fixed-point) derivation rules ──
    generate_derive_qualified_names(&mut out);
    generate_derive_can_see(&mut out);
    generate_derive_recursive_type(&mut out);

    out.push_str("}\n"); // close impl World

    Ok(out)
}

// ═══════════════════════════════════════════════════════════════
// Derivation rule generators
// ═══════════════════════════════════════════════════════════════

fn generate_derive_variant_of(out: &mut String) {
    out.push_str("    fn derive_variant_of(&mut self) {\n");
    out.push_str("        let mut results = Vec::new();\n");
    out.push_str("        for node in &self.nodes {\n");
    out.push_str("            if node.kind == NodeKind::Domain {\n");
    out.push_str("                for var in &self.variants {\n");
    out.push_str("                    if var.domain_id == node.id {\n");
    out.push_str("                        results.push(VariantOf {\n");
    out.push_str("                            variant_name: var.name.clone(),\n");
    out.push_str("                            domain_name: node.name.clone(),\n");
    out.push_str("                            domain_node_id: node.id,\n");
    out.push_str("                        });\n");
    out.push_str("                    }\n");
    out.push_str("                }\n");
    out.push_str("            }\n");
    out.push_str("        }\n");
    out.push_str("        self.variant_ofs = results;\n");
    out.push_str("    }\n\n");
}

fn generate_derive_binding_info(out: &mut String) {
    out.push_str("    fn derive_binding_info(&mut self) {\n");
    out.push_str("        let mut results = Vec::new();\n");
    out.push_str("        for expr in &self.exprs {\n");
    out.push_str("            if expr.kind == ExprKind::SubTypeNew {\n");
    out.push_str("                if expr.value.contains(':') {\n");
    out.push_str("                    if let Some(colon) = expr.value.find(':') {\n");
    out.push_str("                        results.push(BindingInfo {\n");
    out.push_str("                            expr_id: expr.id,\n");
    out.push_str("                            var_name: expr.value[..colon].to_string(),\n");
    out.push_str("                            type_name: expr.value[colon+1..].to_string(),\n");
    out.push_str("                        });\n");
    out.push_str("                    }\n");
    out.push_str("                }\n");
    out.push_str("            } else if expr.kind == ExprKind::SameTypeNew {\n");
    out.push_str("                if !expr.value.contains(':') {\n");
    out.push_str("                    results.push(BindingInfo {\n");
    out.push_str("                        expr_id: expr.id,\n");
    out.push_str("                        var_name: expr.value.clone(),\n");
    out.push_str("                        type_name: expr.value.clone(),\n");
    out.push_str("                    });\n");
    out.push_str("                }\n");
    out.push_str("            }\n");
    out.push_str("        }\n");
    out.push_str("        self.binding_infos = results;\n");
    out.push_str("    }\n\n");
}

fn generate_derive_type_kind(out: &mut String) {
    out.push_str("    fn derive_type_kind(&mut self) {\n");
    out.push_str("        let mut results = Vec::new();\n");
    out.push_str("        for node in &self.nodes {\n");
    out.push_str("            match node.kind {\n");
    out.push_str("                NodeKind::Domain => results.push(TypeKind {\n");
    out.push_str("                    type_name: node.name.clone(),\n");
    out.push_str("                    category: TypeCategory::Domain,\n");
    out.push_str("                }),\n");
    out.push_str("                NodeKind::Struct => results.push(TypeKind {\n");
    out.push_str("                    type_name: node.name.clone(),\n");
    out.push_str("                    category: TypeCategory::Struct,\n");
    out.push_str("                }),\n");
    out.push_str("                _ => {}\n");
    out.push_str("            }\n");
    out.push_str("        }\n");
    out.push_str("        self.type_kinds = results;\n");
    out.push_str("    }\n\n");
}

fn generate_derive_method_on_type(out: &mut String) {
    out.push_str("    fn derive_method_on_type(&mut self) {\n");
    out.push_str("        let mut results = Vec::new();\n");
    out.push_str("        for ti in &self.trait_impls {\n");
    out.push_str("            for node in &self.nodes {\n");
    out.push_str("                if (node.kind == NodeKind::Method || node.kind == NodeKind::TailMethod)\n");
    out.push_str("                    && node.parent == ti.impl_node_id\n");
    out.push_str("                {\n");
    out.push_str("                    results.push(MethodOnType {\n");
    out.push_str("                        type_name: ti.type_name.clone(),\n");
    out.push_str("                        method_name: node.name.clone(),\n");
    out.push_str("                        method_node_id: node.id,\n");
    out.push_str("                    });\n");
    out.push_str("                }\n");
    out.push_str("            }\n");
    out.push_str("        }\n");
    out.push_str("        self.method_on_types = results;\n");
    out.push_str("    }\n\n");
}

fn generate_derive_contained_type(out: &mut String) {
    out.push_str("    fn derive_contained_type(&mut self) {\n");
    out.push_str("        let mut results = Vec::new();\n");
    out.push_str("        // From struct fields\n");
    out.push_str("        for node in &self.nodes {\n");
    out.push_str("            if node.kind == NodeKind::Struct {\n");
    out.push_str("                for field in &self.fields {\n");
    out.push_str("                    if field.struct_id == node.id {\n");
    out.push_str("                        results.push(ContainedType {\n");
    out.push_str("                            parent_type: node.name.clone(),\n");
    out.push_str("                            child_type: field.type_ref.clone(),\n");
    out.push_str("                        });\n");
    out.push_str("                    }\n");
    out.push_str("                }\n");
    out.push_str("            }\n");
    out.push_str("            if node.kind == NodeKind::Domain {\n");
    out.push_str("                for var in &self.variants {\n");
    out.push_str("                    if var.domain_id == node.id && !var.wraps_type.is_empty() {\n");
    out.push_str("                        results.push(ContainedType {\n");
    out.push_str("                            parent_type: node.name.clone(),\n");
    out.push_str("                            child_type: var.wraps_type.clone(),\n");
    out.push_str("                        });\n");
    out.push_str("                    }\n");
    out.push_str("                }\n");
    out.push_str("            }\n");
    out.push_str("        }\n");
    out.push_str("        self.contained_types = results;\n");
    out.push_str("    }\n\n");
}

fn generate_derive_qualified_names(out: &mut String) {
    out.push_str("    fn derive_qualified_names_fixpoint(&mut self) {\n");
    out.push_str("        use std::collections::HashMap;\n");
    out.push_str("        let mut qn: HashMap<i64, String> = HashMap::new();\n");
    out.push_str("        // Top-level nodes with scope\n");
    out.push_str("        for node in &self.nodes {\n");
    out.push_str("            if node.parent == 0 && node.scope_id != 0 {\n");
    out.push_str("                if let Some(scope) = self.scopes.iter().find(|s| s.id == node.scope_id) {\n");
    out.push_str("                    qn.insert(node.id, format!(\"{}::{}\", scope.name, node.name));\n");
    out.push_str("                }\n");
    out.push_str("            }\n");
    out.push_str("        }\n");
    out.push_str("        // Top-level nodes without scope\n");
    out.push_str("        for node in &self.nodes {\n");
    out.push_str("            if node.parent == 0 && node.scope_id == 0 {\n");
    out.push_str("                qn.insert(node.id, node.name.clone());\n");
    out.push_str("            }\n");
    out.push_str("        }\n");
    out.push_str("        // Fixed-point: walk parent chain\n");
    out.push_str("        loop {\n");
    out.push_str("            let mut changed = false;\n");
    out.push_str("            for node in &self.nodes {\n");
    out.push_str("                if node.parent != 0 && !qn.contains_key(&node.id) {\n");
    out.push_str("                    if let Some(parent_qn) = qn.get(&node.parent) {\n");
    out.push_str("                        qn.insert(node.id, format!(\"{}::{}\", parent_qn, node.name));\n");
    out.push_str("                        changed = true;\n");
    out.push_str("                    }\n");
    out.push_str("                }\n");
    out.push_str("            }\n");
    out.push_str("            if !changed { break; }\n");
    out.push_str("        }\n");
    out.push_str("        self.qualified_names = qn.into_iter()\n");
    out.push_str("            .map(|(id, path)| QualifiedName { node_id: id, full_path: path })\n");
    out.push_str("            .collect();\n");
    out.push_str("    }\n\n");
}

fn generate_derive_can_see(out: &mut String) {
    out.push_str("    fn derive_can_see_fixpoint(&mut self) {\n");
    out.push_str("        use std::collections::HashSet;\n");
    out.push_str("        let mut seen: HashSet<(i64, i64)> = HashSet::new();\n");
    out.push_str("        // Self-visibility\n");
    out.push_str("        for node in &self.nodes {\n");
    out.push_str("            seen.insert((node.id, node.id));\n");
    out.push_str("        }\n");
    out.push_str("        // Siblings (same parent)\n");
    out.push_str("        for a in &self.nodes {\n");
    out.push_str("            for b in &self.nodes {\n");
    out.push_str("                if a.parent == b.parent && a.id != b.id {\n");
    out.push_str("                    seen.insert((a.id, b.id));\n");
    out.push_str("                }\n");
    out.push_str("            }\n");
    out.push_str("        }\n");
    out.push_str("        // Imports\n");
    out.push_str("        for node in &self.nodes {\n");
    out.push_str("            if node.scope_id != 0 {\n");
    out.push_str("                for imp in &self.imports {\n");
    out.push_str("                    if imp.scope_id == node.scope_id {\n");
    out.push_str("                        for target in &self.nodes {\n");
    out.push_str("                            if target.name == imp.imported_name {\n");
    out.push_str("                                seen.insert((node.id, target.id));\n");
    out.push_str("                            }\n");
    out.push_str("                        }\n");
    out.push_str("                    }\n");
    out.push_str("                }\n");
    out.push_str("            }\n");
    out.push_str("        }\n");
    out.push_str("        // Fixed-point: inherited visibility from parent\n");
    out.push_str("        loop {\n");
    out.push_str("            let mut changed = false;\n");
    out.push_str("            let snapshot: Vec<(i64, i64)> = seen.iter().copied().collect();\n");
    out.push_str("            for node in &self.nodes {\n");
    out.push_str("                if node.parent != 0 {\n");
    out.push_str("                    for &(observer, visible) in &snapshot {\n");
    out.push_str("                        if observer == node.parent {\n");
    out.push_str("                            if seen.insert((node.id, visible)) {\n");
    out.push_str("                                changed = true;\n");
    out.push_str("                            }\n");
    out.push_str("                        }\n");
    out.push_str("                    }\n");
    out.push_str("                }\n");
    out.push_str("            }\n");
    out.push_str("            if !changed { break; }\n");
    out.push_str("        }\n");
    out.push_str("        self.can_sees = seen.into_iter()\n");
    out.push_str("            .map(|(o, v)| CanSee { observer_id: o, visible_id: v })\n");
    out.push_str("            .collect();\n");
    out.push_str("    }\n\n");
}

fn generate_derive_recursive_type(out: &mut String) {
    out.push_str("    fn derive_recursive_type_fixpoint(&mut self) {\n");
    out.push_str("        use std::collections::HashSet;\n");
    out.push_str("        let mut reachable: HashSet<(String, String)> = HashSet::new();\n");
    out.push_str("        // Base: direct containment\n");
    out.push_str("        for ct in &self.contained_types {\n");
    out.push_str("            reachable.insert((ct.parent_type.clone(), ct.child_type.clone()));\n");
    out.push_str("        }\n");
    out.push_str("        // Transitive closure\n");
    out.push_str("        loop {\n");
    out.push_str("            let mut changed = false;\n");
    out.push_str("            let snapshot: Vec<(String, String)> = reachable.iter().cloned().collect();\n");
    out.push_str("            for ct in &self.contained_types {\n");
    out.push_str("                for (_, z) in snapshot.iter().filter(|(x, _)| *x == ct.child_type) {\n");
    out.push_str("                    if reachable.insert((ct.parent_type.clone(), z.clone())) {\n");
    out.push_str("                        changed = true;\n");
    out.push_str("                    }\n");
    out.push_str("                }\n");
    out.push_str("            }\n");
    out.push_str("            if !changed { break; }\n");
    out.push_str("        }\n");
    out.push_str("        self.recursive_types = reachable.into_iter()\n");
    out.push_str("            .map(|(p, c)| RecursiveType { parent_type: p, child_type: c })\n");
    out.push_str("            .collect();\n");
    out.push_str("    }\n\n");
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

fn to_snake_plural(s: &str) -> String {
    let snake = pascal_to_snake(s);
    // Simple pluralization
    if snake.ends_with('s') || snake.ends_with("sh") || snake.ends_with("ch") {
        format!("{}es", snake)
    } else if snake.ends_with('y') && !snake.ends_with("ey") {
        format!("{}ies", &snake[..snake.len()-1])
    } else {
        format!("{}s", snake)
    }
}

fn aski_type_to_rust(t: &str) -> String {
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
