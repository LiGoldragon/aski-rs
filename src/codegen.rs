//! Rust code generation from aski v0.9.
//!
//! Primary path: source -> parse -> AST -> insert into CozoDB -> query CozoDB -> generate Rust
//! No free functions — all behavior is methods on types.
//! Trait names are camelCase in aski, PascalCase in Rust — codegen converts.
//!
//! ## `usize` in generated code
//!
//! Aski's type system is fixed-size only: U8, U16, U32, U64, I64, F64.
//! No platform-dependent types exist in the language, the binary format,
//! or the rkyv serialization layer.
//!
//! However, Rust's Vec uses `usize` for indexing (pointer arithmetic).
//! The codegen emits `as usize` in two places:
//!   - Vec `.get()` arguments: `vec.get(pos as usize)`
//!   - Vec `.len()` comparisons: `(pos as usize) >= vec.len()`
//!
//! This is an **ephemeral runtime detail** — the `usize` value exists only
//! as a CPU register during pointer arithmetic. It is never stored in a
//! field, never serialized by rkyv, never hashed, and never visible to
//! the aski programmer. It is equivalent to a CPU instruction, not a type.

use std::collections::HashMap;

use cozo::DbInstance;

use crate::db;

/// Convert a camelCase aski trait name to PascalCase Rust trait name.
fn aski_trait_to_rust(name: &str) -> String {
    let mut chars = name.chars();
    match chars.next() {
        Some(c) => c.to_uppercase().to_string() + chars.as_str(),
        None => String::new(),
    }
}

/// Controls what derives and features the generated Rust includes.
#[derive(Clone, Debug)]
pub struct CodegenConfig {
    /// Include rkyv derives on types.
    pub rkyv: bool,
}

impl Default for CodegenConfig {
    fn default() -> Self {
        Self { rkyv: true }
    }
}

impl CodegenConfig {
    pub fn enum_derives(&self) -> &str {
        if self.rkyv {
            "#[derive(Debug, Clone, Copy, PartialEq, Eq, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]\n"
        } else {
            "#[derive(Debug, Clone, Copy, PartialEq, Eq)]\n"
        }
    }

    pub fn struct_derives(&self) -> &str {
        if self.rkyv {
            "#[derive(Debug, Clone, PartialEq, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]\n"
        } else {
            "#[derive(Debug, Clone, PartialEq, Eq)]\n"
        }
    }
}

/// Generate complete, compilable Rust source by querying CozoDB.
pub fn generate_rust_from_db(db: &DbInstance) -> Result<String, String> {
    generate_rust_from_db_with_config(db, &CodegenConfig::default())
}

pub fn generate_rust_from_db_with_config(db: &DbInstance, config: &CodegenConfig) -> Result<String, String> {
    let variant_map = build_variant_map_from_db(db)?;
    let struct_fields = build_struct_field_map(db)?;
    // Auto-boxing: find fields that need Box due to recursive types
    let recursive_fields = db::query_recursive_fields(db).unwrap_or_default();
    let needs_box: std::collections::HashSet<(String, String)> = recursive_fields.iter()
        .map(|(owner, field, _)| (owner.clone(), field.clone()))
        .collect();
    let mut out = String::new();

    let nodes = db::query_all_top_level_nodes(db)?;

    // Check for operator trait impls and add imports
    // Trait names are camelCase in aski — map to PascalCase for Rust std::ops
    let operator_traits = ["add", "sub", "mul", "div", "rem", "neg", "not",
                           "bitAnd", "bitOr", "bitXor", "shl", "shr",
                           "addAssign", "subAssign", "mulAssign", "divAssign",
                           "index", "indexMut"];
    let mut needed_imports: Vec<String> = Vec::new();
    for (_id, kind, name) in &nodes {
        if kind == "impl" && operator_traits.contains(&name.as_str()) {
            needed_imports.push(aski_trait_to_rust(name));
        }
    }
    if !needed_imports.is_empty() {
        for imp in &needed_imports {
            out.push_str(&format!("use std::ops::{imp};\n"));
        }
        out.push('\n');
    }

    // Generate in dependency order: types first, then impls, then main
    let kind_order = |kind: &str| -> u8 {
        match kind {
            "domain" | "struct" | "const" => 0,  // type definitions first
            "trait" => 1,                         // trait declarations
            "impl" | "inherent_impl" => 2,        // implementations
            "main" => 3,                          // entry point last
            _ => 4,
        }
    };
    let mut sorted_nodes = nodes.clone();
    sorted_nodes.sort_by_key(|(_, kind, _)| kind_order(kind.as_str()));

    for (node_id, kind, name) in &sorted_nodes {
        match kind.as_str() {
            "domain" => gen_domain_from_db(&mut out, db, name, config, &needs_box)?,
            "struct" => gen_struct_from_db(&mut out, db, name, config, &needs_box)?,
            "const" => gen_const_from_db(&mut out, db, *node_id)?,
            "trait" => gen_trait_from_db(&mut out, db, *node_id, name)?,
            "impl" => gen_trait_impl_from_db(&mut out, db, *node_id, name, &variant_map, &struct_fields)?,
            "inherent_impl" => gen_inherent_impl_from_db(&mut out, db, *node_id, name, &variant_map, &struct_fields)?,
            "main" => gen_main_from_db(&mut out, db, *node_id, &variant_map, &struct_fields)?,
            _ => {}
        }
    }

    Ok(out)
}

/// Build a variant_name -> domain_name map from the DB.
fn build_variant_map_from_db(db: &DbInstance) -> Result<HashMap<String, String>, String> {
    let mut map = HashMap::new();
    let domains = db::query_nodes_by_kind(db, "domain")?;
    for (_id, domain_name) in &domains {
        let variants = db::query_domain_variants(db, domain_name)?;
        for (_ord, vname, _wraps) in variants {
            map.insert(vname, domain_name.clone());
        }
    }
    map.insert("True".to_string(), "bool".to_string());
    map.insert("False".to_string(), "bool".to_string());
    Ok(map)
}

/// Build struct_name -> Vec<(field_name, field_type)> map.
fn build_struct_field_map(db: &DbInstance) -> Result<HashMap<String, Vec<(String, String)>>, String> {
    let mut map = HashMap::new();
    let structs = db::query_nodes_by_kind(db, "struct")?;
    for (_id, name) in &structs {
        let fields = db::query_struct_fields(db, name)?;
        let field_list: Vec<(String, String)> = fields.into_iter()
            .map(|(_ord, fname, ftype)| (fname, ftype))
            .collect();
        map.insert(name.clone(), field_list);
    }
    Ok(map)
}

fn gen_domain_from_db(
    out: &mut String,
    db: &DbInstance,
    name: &str,
    config: &CodegenConfig,
    needs_box: &std::collections::HashSet<(String, String)>,
) -> Result<(), String> {
    let variants = db::query_domain_variants(db, name)?;

    // Check which variants have inline sub-domains (stored as child domain nodes)
    let all_domains = db::query_nodes_by_kind(db, "domain").unwrap_or_default();
    let sub_domain_names: std::collections::HashSet<String> = all_domains.iter()
        .map(|(_, n)| n.clone())
        .collect();

    // Build a map of variant names that have struct fields (stored as child struct nodes).
    // These are struct variants: `Variant { Field Type ... }`
    let mut struct_variant_fields: HashMap<String, Vec<(i32, String, String)>> = HashMap::new();
    for (_ord, vname, _wraps) in &variants {
        // Try to get struct fields for this variant name
        if let Ok(fields) = db::query_struct_fields(db, vname) {
            if !fields.is_empty() && !sub_domain_names.contains(vname) {
                struct_variant_fields.insert(vname.clone(), fields);
            }
        }
    }

    // Generate child sub-domains first (inline domain variants)
    for (_ord, vname, _wraps) in &variants {
        if sub_domain_names.contains(vname) && vname != name {
            gen_domain_from_db(out, db, vname, config, needs_box)?;
        }
    }

    // Check if enum has any data-carrying variants — if so, can't derive Copy
    let has_data = variants.iter().any(|(_, vname, wraps)| {
        wraps.is_some() || struct_variant_fields.contains_key(vname) ||
        (sub_domain_names.contains(vname) && vname != name)
    });
    if has_data {
        // Data-carrying enum — use Clone only (not Copy)
        if config.rkyv {
            out.push_str("#[derive(Debug, Clone, PartialEq, Eq, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]\n");
        } else {
            out.push_str("#[derive(Debug, Clone, PartialEq, Eq)]\n");
        }
    } else {
        out.push_str(config.enum_derives());
    }
    out.push_str(&format!("pub enum {name} {{\n"));
    for (_ord, vname, wraps) in &variants {
        if sub_domain_names.contains(vname) && vname != name {
            // Inline domain variant — wraps the generated sub-enum
            out.push_str(&format!("    {vname}({vname}),\n"));
        } else if let Some(ref fields) = struct_variant_fields.get(vname) {
            // Struct variant: Variant { field: Type, ... }
            out.push_str(&format!("    {vname} {{\n"));
            for (_ford, fname, ftype) in *fields {
                let rust_type = aski_type_to_rust(ftype);
                let field_name = to_snake_case(fname);
                out.push_str(&format!("        {field_name}: {rust_type},\n"));
            }
            out.push_str("    },\n");
        } else if let Some(w) = wraps {
            let rust_type = aski_type_to_rust(w);
            if needs_box.contains(&(name.to_string(), vname.clone())) || w == name {
                out.push_str(&format!("    {vname}(Box<{rust_type}>),\n"));
            } else {
                out.push_str(&format!("    {vname}({rust_type}),\n"));
            }
        } else {
            out.push_str(&format!("    {vname},\n"));
        }
    }
    out.push_str("}\n\n");

    // Auto-generate Display impl for domains
    out.push_str(&format!("impl std::fmt::Display for {name} {{\n"));
    out.push_str(&format!("    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {{\n"));
    out.push_str(&format!("        write!(f, \"{{:?}}\", self)\n"));
    out.push_str(&format!("    }}\n"));
    out.push_str(&format!("}}\n\n"));

    Ok(())
}

/// Check if an aski type name is Copy-eligible.
/// Copy-eligible: primitive numeric/bool types and domain enums with no data-carrying variants.
fn is_copy_eligible(type_name: &str, db: &DbInstance) -> bool {
    match type_name {
        "U8" | "U16" | "U32" | "U64" | "U128"
        | "I8" | "I16" | "I32" | "I64" | "I128"
        | "F32" | "F64" | "Bool" => true,
        _ => {
            // Check if it's a domain (enum) with no data-carrying variants
            if let Ok(variants) = db::query_domain_variants(db, type_name) {
                if !variants.is_empty() {
                    // It is a domain — check that no variant carries data
                    return variants.iter().all(|(_, _, wraps)| wraps.is_none());
                }
            }
            false
        }
    }
}

fn gen_struct_from_db(
    out: &mut String,
    db: &DbInstance,
    name: &str,
    config: &CodegenConfig,
    needs_box: &std::collections::HashSet<(String, String)>,
) -> Result<(), String> {
    let fields = db::query_struct_fields(db, name)?;

    // Check if all fields are Copy-eligible — if so, derive Copy
    let all_copy = !fields.is_empty() && fields.iter().all(|(_, _, ftype)| {
        // Boxed fields are not Copy
        if needs_box.contains(&(name.to_string(), ftype.clone())) {
            return false;
        }
        is_copy_eligible(ftype, db)
    });

    if all_copy {
        if config.rkyv {
            out.push_str("#[derive(Debug, Copy, Clone, PartialEq, Eq, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]\n");
        } else {
            out.push_str("#[derive(Debug, Copy, Clone, PartialEq)]\n");
        }
    } else {
        out.push_str(config.struct_derives());
    }
    out.push_str(&format!("pub struct {name} {{\n"));
    for (_ord, fname, ftype) in &fields {
        let rust_type = aski_type_to_rust(ftype);
        let field_name = to_snake_case(fname);
        // Auto-box if this field creates a recursive type
        if needs_box.contains(&(name.to_string(), fname.clone())) {
            out.push_str(&format!("    pub {field_name}: Box<{rust_type}>,\n"));
        } else {
            out.push_str(&format!("    pub {field_name}: {rust_type},\n"));
        }
    }
    out.push_str("}\n\n");
    Ok(())
}

fn gen_const_from_db(out: &mut String, db: &DbInstance, node_id: i64) -> Result<(), String> {
    let (name, type_ref, has_value) = db::query_constant(db, node_id)?
        .ok_or_else(|| format!("constant node {} not found", node_id))?;
    let rust_type = aski_type_to_rust(&type_ref);
    let screaming = to_screaming_snake(&name);

    if has_value {
        let children = db::query_child_exprs(db, node_id)?;
        if let Some((child_id, _kind, _ord, _val)) = children.first() {
            let empty_variants = HashMap::new();
            let empty_fields = HashMap::new();
            let ctx = ExprCtx::new_default(&empty_variants, &empty_fields);
            let val = emit_expr_from_db(db, *child_id, &ctx)?;
            out.push_str(&format!("pub const {screaming}: {rust_type} = {val};\n\n"));
        } else {
            out.push_str(&format!("pub const {screaming}: {rust_type} = todo!();\n\n"));
        }
    } else {
        out.push_str(&format!("pub const {screaming}: {rust_type} = todo!();\n\n"));
    }
    Ok(())
}


fn gen_trait_from_db(
    out: &mut String,
    db: &DbInstance,
    node_id: i64,
    name: &str,
) -> Result<(), String> {
    let rust_name = aski_trait_to_rust(name);
    out.push_str(&format!("pub trait {rust_name} {{\n"));

    let methods = db::query_child_nodes(db, node_id)?;
    for (method_id, _kind, method_name) in &methods {
        let params = db::query_params(db, *method_id)?;
        let return_type = db::query_return_type(db, *method_id)?;

        let rust_params = gen_trait_method_params(&params);
        let ret = return_type.as_ref()
            .map(|r| format!(" -> {}", aski_type_to_rust(r)))
            .unwrap_or_default();
        let snake = to_snake_case(method_name);
        out.push_str(&format!("    fn {snake}({rust_params}){ret};\n"));
    }

    out.push_str("}\n\n");
    Ok(())
}

fn gen_trait_impl_from_db(
    out: &mut String,
    db: &DbInstance,
    node_id: i64,
    trait_name: &str,
    variant_map: &HashMap<String, String>,
    struct_fields: &HashMap<String, Vec<(String, String)>>,
) -> Result<(), String> {
    let impl_bodies = db::query_child_nodes(db, node_id)?;

    // Trait names are camelCase in aski — convert to PascalCase for Rust
    let operator_traits_with_output = ["add", "sub", "mul", "div", "rem"];
    let operator_traits_move_self = ["add", "sub", "mul", "div", "rem", "neg", "not"];
    let rust_trait_name = aski_trait_to_rust(trait_name);

    for (impl_body_id, _kind, type_name) in &impl_bodies {
        out.push_str(&format!("impl {rust_trait_name} for {type_name} {{\n"));

        // For operator traits with Output: infer from method return type
        if operator_traits_with_output.contains(&trait_name) {
            let methods = db::query_child_nodes(db, *impl_body_id)?;
            if let Some((method_id, _, _)) = methods.first() {
                if let Some(ret) = db::query_return_type(db, *method_id)? {
                    out.push_str(&format!("    type Output = {};\n", aski_type_to_rust(&ret)));
                }
            }
        }

        let is_operator = operator_traits_move_self.contains(&trait_name);
        let methods = db::query_child_nodes(db, *impl_body_id)?;
        for (method_id, _kind, method_name) in &methods {
            if is_operator {
                gen_operator_method_from_db(out, db, *method_id, method_name, type_name, 1, variant_map, struct_fields)?;
            } else {
                gen_method_impl_from_db(out, db, *method_id, method_name, type_name, 1, false, variant_map, struct_fields)?;
            }
        }

        out.push_str("}\n\n");
    }
    Ok(())
}

fn gen_inherent_impl_from_db(
    out: &mut String,
    db: &DbInstance,
    node_id: i64,
    type_name: &str,
    variant_map: &HashMap<String, String>,
    struct_fields: &HashMap<String, Vec<(String, String)>>,
) -> Result<(), String> {
    out.push_str(&format!("impl {type_name} {{\n"));

    let methods = db::query_child_nodes(db, node_id)?;
    for (method_id, _kind, method_name) in &methods {
        gen_method_impl_from_db(out, db, *method_id, method_name, type_name, 1, true, variant_map, struct_fields)?;
    }

    out.push_str("}\n\n");
    Ok(())
}

/// Generate an operator trait method (uses `self` not `&self`, owned params).
fn gen_operator_method_from_db(
    out: &mut String,
    db: &DbInstance,
    method_id: i64,
    method_name: &str,
    self_type: &str,
    base_indent: usize,
    variant_map: &HashMap<String, String>,
    struct_fields: &HashMap<String, Vec<(String, String)>>,
) -> Result<(), String> {
    let indent = "    ".repeat(base_indent);
    let params = db::query_params(db, method_id)?;
    let return_type = db::query_return_type(db, method_id)?;

    // Generate params with self (not &self) and owned types (not borrowed)
    let mut parts = Vec::new();
    for (kind, name, type_ref) in &params {
        match kind.as_str() {
            "borrow_self" | "mut_borrow_self" | "owned_self" => parts.push("self".to_string()),
            "owned" | "borrow" => {
                let t = type_ref.as_deref().unwrap_or("()");
                let var = to_snake_case(t);
                parts.push(format!("{var}: {t}"));
            }
            "named" => {
                let n = name.as_deref().unwrap_or("arg");
                let t = type_ref.as_deref().unwrap_or("()");
                let var = to_snake_case(n);
                parts.push(format!("{var}: {}", aski_type_to_rust(t)));
            }
            _ => {}
        }
    }
    let rust_params = parts.join(", ");

    let ret = return_type.as_ref()
        .map(|r| format!(" -> {}", aski_type_to_rust(r)))
        .unwrap_or_default();
    let snake = to_snake_case(method_name);

    out.push_str(&format!("{indent}fn {snake}({rust_params}){ret} {{\n"));

    let param_bindings = build_method_param_bindings(&params, self_type);
    let ctx = ExprCtx {
        indent: base_indent + 1,
        variant_map,
        struct_fields,
        bindings: param_bindings,
        binding_types: HashMap::new(),
        self_type: Some(self_type.to_string()),
    };
    gen_body_from_db(out, db, method_id, &ctx)?;
    out.push_str(&format!("{indent}}}\n"));
    Ok(())
}

fn gen_method_impl_from_db(
    out: &mut String,
    db: &DbInstance,
    method_id: i64,
    method_name: &str,
    self_type: &str,
    base_indent: usize,
    is_inherent: bool,
    variant_map: &HashMap<String, String>,
    struct_fields: &HashMap<String, Vec<(String, String)>>,
) -> Result<(), String> {
    let indent = "    ".repeat(base_indent);
    let params = db::query_params(db, method_id)?;
    let return_type = db::query_return_type(db, method_id)?;

    let rust_params = gen_method_impl_params(&params);
    let ret = return_type.as_ref()
        .map(|r| format!(" -> {}", aski_type_to_rust(r)))
        .unwrap_or_default();
    let snake = to_snake_case(method_name);
    let vis = if is_inherent { "pub " } else { "" };

    out.push_str(&format!("{indent}{vis}fn {snake}({rust_params}){ret} {{\n"));

    let param_bindings = build_method_param_bindings(&params, self_type);
    let ctx = ExprCtx {
        indent: base_indent + 1,
        variant_map,
        struct_fields,
        bindings: param_bindings,
        binding_types: HashMap::new(),
        self_type: Some(self_type.to_string()),
    };
    gen_body_from_db(out, db, method_id, &ctx)?;
    out.push_str(&format!("{indent}}}\n"));
    Ok(())
}

fn gen_main_from_db(
    out: &mut String,
    db: &DbInstance,
    node_id: i64,
    variant_map: &HashMap<String, String>,
    struct_fields: &HashMap<String, Vec<(String, String)>>,
) -> Result<(), String> {
    out.push_str("fn main() {\n");

    let ctx = ExprCtx {
        indent: 1,
        variant_map,
        struct_fields,
        bindings: HashMap::new(),
        binding_types: HashMap::new(),
        self_type: None,
    };
    gen_body_from_db(out, db, node_id, &ctx)?;
    out.push_str("}\n");
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════
// Parameter generation
// ═══════════════════════════════════════════════════════════════════

/// Generate param list for trait method signatures.
fn gen_trait_method_params(params: &[(String, Option<String>, Option<String>)]) -> String {
    let mut parts = Vec::new();
    for (kind, name, type_ref) in params {
        match kind.as_str() {
            "borrow_self" => parts.push("&self".to_string()),
            "mut_borrow_self" => parts.push("&mut self".to_string()),
            "owned_self" => parts.push("self".to_string()),
            "owned" => {
                let t = type_ref.as_deref().unwrap_or("()");
                let var = to_snake_case(t);
                let rust_t = aski_type_to_rust(t);
                parts.push(format!("{var}: &{rust_t}"));
            }
            "named" => {
                let n = name.as_deref().unwrap_or("arg");
                let t = type_ref.as_deref().unwrap_or("()");
                let var = to_snake_case(n);
                let rust_t = aski_type_to_rust(t);
                parts.push(format!("{var}: &{rust_t}"));
            }
            _ => {}
        }
    }
    parts.join(", ")
}

/// Check if a Rust type is Copy (passed by value, not reference).
fn is_copy_type(rust_type: &str) -> bool {
    matches!(rust_type, "u8" | "u16" | "u32" | "u64" | "i8" | "i16" | "i32" | "i64" | "f32" | "f64" | "bool" | "char")
}

/// Generate param list for method implementations.
fn gen_method_impl_params(params: &[(String, Option<String>, Option<String>)]) -> String {
    let mut parts = Vec::new();
    for (kind, name, type_ref) in params {
        match kind.as_str() {
            "borrow_self" => parts.push("&self".to_string()),
            "mut_borrow_self" => parts.push("&mut self".to_string()),
            "owned_self" => parts.push("self".to_string()),
            "owned" => {
                let t = type_ref.as_deref().unwrap_or("()");
                let var = to_snake_case(t);
                let rust_t = aski_type_to_rust(t);
                parts.push(format!("{var}: &{rust_t}"));
            }
            "named" => {
                let n = name.as_deref().unwrap_or("arg");
                let t = type_ref.as_deref().unwrap_or("()");
                let var = to_snake_case(n);
                let rust_t = aski_type_to_rust(t);
                parts.push(format!("{var}: &{rust_t}"));
            }
            "borrow" => {
                let t = type_ref.as_deref().unwrap_or("()");
                let var = to_snake_case(t);
                parts.push(format!("{}: &{}", var, t));
            }
            _ => {}
        }
    }
    parts.join(", ")
}

/// Build binding map for method params (includes Self -> self).
fn build_method_param_bindings(params: &[(String, Option<String>, Option<String>)], _self_type: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for (kind, name, type_ref) in params {
        match kind.as_str() {
            "borrow_self" | "mut_borrow_self" | "owned_self" => {
                map.insert("Self".to_string(), "self".to_string());
            }
            "owned" => {
                let t = type_ref.as_deref().unwrap_or("Unknown");
                map.insert(t.to_string(), to_snake_case(t));
            }
            "named" => {
                let n = name.as_deref().unwrap_or("arg");
                let t = type_ref.as_deref().unwrap_or("Unknown");
                map.insert(n.to_string(), to_snake_case(n));
                // Also allow resolving by type
                map.insert(t.to_string(), to_snake_case(n));
            }
            "borrow" => {
                let t = type_ref.as_deref().unwrap_or("Unknown");
                map.insert(t.to_string(), to_snake_case(t));
            }
            _ => {}
        }
    }
    map
}

// ═══════════════════════════════════════════════════════════════════
// Body and expression generation
// ═══════════════════════════════════════════════════════════════════

/// Expression context for code generation.
#[derive(Clone)]
struct ExprCtx<'a> {
    indent: usize,
    variant_map: &'a HashMap<String, String>,
    struct_fields: &'a HashMap<String, Vec<(String, String)>>,
    /// @Name -> rust variable name
    bindings: HashMap<String, String>,
    /// @Name -> aski type name (for format decisions)
    binding_types: HashMap<String, String>,
    self_type: Option<String>,
}

impl<'a> ExprCtx<'a> {
    fn new_default(
        variant_map: &'a HashMap<String, String>,
        struct_fields: &'a HashMap<String, Vec<(String, String)>>,
    ) -> Self {
        Self {
            indent: 1,
            variant_map,
            struct_fields,
            bindings: HashMap::new(),
            binding_types: HashMap::new(),
            self_type: None,
        }
    }

    fn resolve_instance(&self, name: &str) -> String {
        if name == "Self" {
            return "self".to_string();
        }
        if let Some(var) = self.bindings.get(name) {
            return var.clone();
        }
        // Fall back to snake_case of the name
        to_snake_case(name)
    }

    fn qualify_variant(&self, name: &str) -> String {
        if let Some(domain) = self.variant_map.get(name) {
            if domain == "bool" {
                match name {
                    "True" => "true".to_string(),
                    "False" => "false".to_string(),
                    _ => name.to_string(),
                }
            } else {
                format!("{domain}::{name}")
            }
        } else {
            name.to_string()
        }
    }
}

/// Generate function/method body from DB.
fn gen_body_from_db(
    out: &mut String,
    db: &DbInstance,
    owner_id: i64,
    ctx: &ExprCtx,
) -> Result<(), String> {
    // Check if this is a matching method body (has match_arm rows)
    let arms = db::query_match_arms(db, owner_id)?;
    if !arms.is_empty() {
        return gen_matching_body_from_db(out, db, owner_id, ctx, &arms);
    }

    let children = db::query_child_exprs(db, owner_id)?;

    if children.is_empty() {
        let indent = "    ".repeat(ctx.indent);
        out.push_str(&format!("{indent}todo!()\n"));
        return Ok(());
    }

    if children.len() == 1 && children[0].1 == "stub" {
        let indent = "    ".repeat(ctx.indent);
        out.push_str(&format!("{indent}todo!()\n"));
        return Ok(());
    }

    let indent = "    ".repeat(ctx.indent);
    let mut ctx = ctx.clone();

    for (i, (child_id, kind, _ordinal, value)) in children.iter().enumerate() {
        let is_last = i == children.len() - 1;
        gen_statement_from_db(out, db, *child_id, kind, value.as_deref(), &mut ctx, &indent, is_last)?;
    }
    Ok(())
}

/// Generate Rust match block from a matching method body.
/// The method params are the match targets, the match_arm rows are the arms.
fn gen_matching_body_from_db(
    out: &mut String,
    db: &DbInstance,
    _method_id: i64,
    ctx: &ExprCtx,
    arms: &[(i64, Vec<String>, Option<i64>)],
) -> Result<(), String> {
    let indent = "    ".repeat(ctx.indent);
    let arm_indent = "    ".repeat(ctx.indent + 1);

    // Determine if multi-value matching by checking pattern count on first arm
    let is_multi = arms.first().map(|(_, pats, _)| pats.len() > 1).unwrap_or(false);

    if is_multi {
        // Multi-value matching: build param names from the method's non-self params
        // The param names are snake_cased in gen_method_impl_params, we need them here
        // For now, collect from the arm pattern count and use positional references
        let param_count = arms.first().map(|(_, pats, _)| pats.len()).unwrap_or(1);

        // First param is always self. Additional params come from the method signature.
        // We need to build a tuple match target.
        // The params are available in ctx.bindings — look for non-Self entries.
        let mut match_targets = vec!["self".to_string()];
        // Collect non-self param names from bindings
        let mut extra_params: Vec<String> = ctx.bindings.iter()
            .filter(|(k, _)| *k != "Self")
            .map(|(_, v)| v.clone())
            .collect();
        extra_params.sort(); // deterministic order
        match_targets.extend(extra_params.iter().take(param_count - 1).cloned());

        out.push_str(&format!("{indent}match ({}) {{\n", match_targets.join(", ")));

        for (_ordinal, patterns, body_expr_id) in arms {
            let tuple_pats: Vec<String> = patterns.iter()
                .map(|p| emit_pattern_from_db(p, ctx, db))
                .collect();
            let body = if let Some(bid) = body_expr_id {
                emit_expr_from_db(db, *bid, ctx)?
            } else {
                "todo!()".to_string()
            };
            out.push_str(&format!("{arm_indent}({}) => {body},\n", tuple_pats.join(", ")));
        }
    } else {
        // Single-value matching: match on self
        out.push_str(&format!("{indent}match self {{\n"));

        for (_ordinal, patterns, body_expr_id) in arms {
            if let Some(pat_str) = patterns.first() {
                let pat = emit_pattern_from_db(pat_str, ctx, db);

                // Add pattern bindings to context for the arm body
                let mut arm_ctx = ctx.clone();
                let parsed_pat = db::parse_pattern_string(pat_str);
                if let db::ParsedPattern::DataCarrying(_, ref inner) = parsed_pat {
                    if inner.starts_with('@') {
                        let bind_name = &inner[1..];
                        arm_ctx.bindings.insert(
                            bind_name.to_string(),
                            to_snake_case(bind_name),
                        );
                    }
                }

                let body = if let Some(bid) = body_expr_id {
                    emit_expr_from_db(db, *bid, &arm_ctx)?
                } else {
                    "todo!()".to_string()
                };
                out.push_str(&format!("{arm_indent}{pat} => {body},\n"));
            }
        }
    }

    out.push_str(&format!("{indent}}}\n"));
    Ok(())
}

fn gen_statement_from_db(
    out: &mut String,
    db: &DbInstance,
    expr_id: i64,
    kind: &str,
    value: Option<&str>,
    ctx: &mut ExprCtx,
    indent: &str,
    is_last: bool,
) -> Result<(), String> {
    match kind {
        "sub_type_decl" => {
            // Format: "Name:Type" — just register the binding, don't emit code yet
            if let Some(val) = value {
                if let Some(colon_idx) = val.find(':') {
                    let name = &val[..colon_idx];
                    let type_name = &val[colon_idx + 1..];
                    let var_name = to_snake_case(name);
                    ctx.bindings.insert(name.to_string(), var_name);
                    ctx.binding_types.insert(name.to_string(), type_name.to_string());
                }
            }
        }
        "same_type_new" | "deferred_new" => {
            let name = value.unwrap_or("");
            let var_name = to_snake_case(name);
            let children = db::query_child_exprs(db, expr_id)?;
            if children.is_empty() {
                out.push_str(&format!("{indent}let {var_name} = {name} {{}};\n"));
            } else if children.len() == 1 {
                let (child_id, child_kind, _ord, child_val) = &children[0];
                if child_kind == "struct_construct" {
                    // Single field pair: Value(5.0) → Radius { value: 5.0 }
                    // The struct_construct's name is the field name, wrap in binding type
                    let field_name = to_snake_case(child_val.as_deref().unwrap_or(""));
                    let grandchildren = db::query_child_exprs(db, *child_id)?;
                    if let Some((gc_id, _gk, _go, _gv)) = grandchildren.first() {
                        let gc_inner = db::query_child_exprs(db, *gc_id)?;
                        if let Some((val_id, _vk, _vo, _vv)) = gc_inner.first() {
                            let v = emit_expr_from_db(db, *val_id, ctx)?;
                            out.push_str(&format!("{indent}let {var_name} = {name} {{ {field_name}: {v} }};\n"));
                        } else {
                            let v = emit_expr_from_db(db, *gc_id, ctx)?;
                            out.push_str(&format!("{indent}let {var_name} = {name} {{ {field_name}: {v} }};\n"));
                        }
                    } else {
                        out.push_str(&format!("{indent}let {var_name} = {name} {{}};\n"));
                    }
                } else {
                    // Single value — primitive or expression
                    let val = emit_expr_from_db(db, *child_id, ctx)?;
                    out.push_str(&format!("{indent}let {var_name} = {val};\n"));
                }
            } else {
                // Multiple children — struct field pairs from .new(Field(val) ...)
                // Wrap in the type name since .new() knows the type
                let mut field_inits = Vec::new();
                for (child_id, child_kind, _ord, child_val) in &children {
                    if child_kind == "struct_field" {
                        let fname = to_snake_case(child_val.as_deref().unwrap_or(""));
                        let inner = db::query_child_exprs(db, *child_id)?;
                        if let Some((val_id, _k, _o, _v)) = inner.first() {
                            let v = emit_expr_from_db(db, *val_id, ctx)?;
                            field_inits.push(format!("{fname}: {v}"));
                        }
                    } else if child_kind == "struct_construct" {
                        // A field initializer like Value(5.0) parsed as struct_construct
                        let inner_name = child_val.as_deref().unwrap_or("");
                        let fname = to_snake_case(inner_name);
                        let grandchildren = db::query_child_exprs(db, *child_id)?;
                        if let Some((gc_id, _gk, _go, _gv)) = grandchildren.first() {
                            let gc_children = db::query_child_exprs(db, *gc_id)?;
                            if let Some((val_id, _vk, _vo, _vv)) = gc_children.first() {
                                let v = emit_expr_from_db(db, *val_id, ctx)?;
                                field_inits.push(format!("{fname}: {v}"));
                            }
                        }
                    } else {
                        let v = emit_expr_from_db(db, *child_id, ctx)?;
                        field_inits.push(v);
                    }
                }
                out.push_str(&format!("{indent}let {var_name} = {name} {{ {} }};\n", field_inits.join(", ")));
            }
            ctx.bindings.insert(name.to_string(), var_name);
        }
        "sub_type_new" => {
            // Format: "Name:Type"
            if let Some(val) = value {
                if let Some(colon_idx) = val.find(':') {
                    let name = &val[..colon_idx];
                    let type_name = &val[colon_idx + 1..];
                    let var_name = to_snake_case(name);
                    let rust_type = aski_type_to_rust(type_name);
                    let children = db::query_child_exprs(db, expr_id)?;
                    let is_primitive = matches!(type_name, "U8"|"U16"|"U32"|"U64"|"U128"|"I8"|"I16"|"I32"|"I64"|"I128"|"F32"|"F64"|"Bool"|"String");
                    if is_primitive || children.len() == 1 {
                        // Primitive or single expression — just emit with type annotation
                        if let Some((child_id, _kind, _ord, _val)) = children.first() {
                            let child_val = emit_expr_from_db(db, *child_id, ctx)?;
                            out.push_str(&format!("{indent}let {var_name}: {rust_type} = {child_val};\n"));
                        }
                    } else if children.is_empty() {
                        out.push_str(&format!("{indent}let {var_name} = {type_name} {{}};\n"));
                    } else {
                        // Multiple children — struct field pairs
                        let mut field_inits = Vec::new();
                        for (child_id, child_kind, _ord, child_val) in &children {
                            if child_kind == "struct_construct" {
                                let field_name = to_snake_case(child_val.as_deref().unwrap_or(""));
                                let grandchildren = db::query_child_exprs(db, *child_id)?;
                                if let Some((gc_id, _gk, _go, _gv)) = grandchildren.first() {
                                    let gc_inner = db::query_child_exprs(db, *gc_id)?;
                                    if let Some((val_id, _vk, _vo, _vv)) = gc_inner.first() {
                                        let v = emit_expr_from_db(db, *val_id, ctx)?;
                                        field_inits.push(format!("{field_name}: {v}"));
                                    } else {
                                        let v = emit_expr_from_db(db, *gc_id, ctx)?;
                                        field_inits.push(format!("{field_name}: {v}"));
                                    }
                                }
                            } else {
                                let v = emit_expr_from_db(db, *child_id, ctx)?;
                                field_inits.push(v);
                            }
                        }
                        out.push_str(&format!("{indent}let {var_name} = {type_name} {{ {} }};\n", field_inits.join(", ")));
                    }
                    ctx.bindings.insert(name.to_string(), var_name);
                }
            }
        }
        "mutable_new" => {
            // Format: "Name:Type"
            if let Some(val) = value {
                if let Some(colon_idx) = val.find(':') {
                    let name = &val[..colon_idx];
                    let type_name = &val[colon_idx + 1..];
                    let var_name = to_snake_case(name);
                    let rust_type = aski_type_to_rust(type_name);
                    let children = db::query_child_exprs(db, expr_id)?;
                    if let Some((child_id, _kind, _ord, _val)) = children.first() {
                        let child_val = emit_expr_from_db(db, *child_id, ctx)?;
                        out.push_str(&format!("{indent}let mut {var_name}: {rust_type} = {child_val};\n"));
                        ctx.bindings.insert(name.to_string(), var_name);
                    }
                }
            }
        }
        "mutable_set" => {
            let name = value.unwrap_or("");
            let var_name = ctx.bindings.get(name).cloned().unwrap_or_else(|| to_snake_case(name));
            let children = db::query_child_exprs(db, expr_id)?;
            if let Some((child_id, _kind, _ord, _val)) = children.first() {
                let val = emit_expr_from_db(db, *child_id, ctx)?;
                out.push_str(&format!("{indent}{var_name} = {val};\n"));
            }
        }
        "error_prop" => {
            let children = db::query_child_exprs(db, expr_id)?;
            if let Some((child_id, _kind, _ord, _val)) = children.first() {
                let val = emit_expr_from_db(db, *child_id, ctx)?;
                out.push_str(&format!("{indent}{val}?;\n"));
            }
        }
        "return" => {
            let children = db::query_child_exprs(db, expr_id)?;
            if let Some((child_id, child_kind, _ord, _val)) = children.first() {
                let actual_id = if child_kind == "group" {
                    let inner = db::query_child_exprs(db, *child_id)?;
                    inner.first().map(|(id, _, _, _)| *id).unwrap_or(*child_id)
                } else {
                    *child_id
                };
                let val = emit_expr_from_db(db, actual_id, ctx)?;
                if is_last {
                    out.push_str(&format!("{indent}{val}\n"));
                } else {
                    out.push_str(&format!("{indent}return {val};\n"));
                }
            }
        }
        "stdout" => {
            let children = db::query_child_exprs(db, expr_id)?;
            if let Some((child_id, child_kind, _ord, child_val)) = children.first() {
                if child_kind == "string_lit" {
                    let s = child_val.as_deref().unwrap_or("");
                    // Check for interpolation: [...] inside string
                    if s.contains('[') && s.contains(']') {
                        let (fmt, args) = emit_interpolated_string(s, ctx);
                        if args.is_empty() {
                            out.push_str(&format!("{indent}println!(\"{fmt}\");\n"));
                        } else {
                            out.push_str(&format!("{indent}println!(\"{fmt}\", {});\n", args.join(", ")));
                        }
                    } else {
                        out.push_str(&format!("{indent}println!(\"{s}\");\n"));
                    }
                } else {
                    let val = emit_expr_from_db(db, *child_id, ctx)?;
                    out.push_str(&format!("{indent}println!(\"{{}}\", {val});\n"));
                }
            }
        }
        _ => {
            let val = emit_expr_from_db(db, expr_id, ctx)?;
            if is_last {
                out.push_str(&format!("{indent}{val}\n"));
            } else {
                out.push_str(&format!("{indent}{val};\n"));
            }
        }
    }
    Ok(())
}

/// Emit a Rust expression string from a DB expr node.
fn emit_expr_from_db(
    db: &DbInstance,
    expr_id: i64,
    ctx: &ExprCtx,
) -> Result<String, String> {
    let (kind, value) = db::query_expr_by_id(db, expr_id)?
        .ok_or_else(|| format!("expr {} not found", expr_id))?;

    match kind.as_str() {
        "int_lit" => Ok(value.unwrap_or_else(|| "0".to_string())),
        "float_lit" => Ok(value.unwrap_or_else(|| "0.0".to_string())),
        "string_lit" => {
            let s = value.unwrap_or_default();
            Ok(format!("\"{s}\".to_string()"))
        }
        "const_ref" => {
            let name = value.unwrap_or_default();
            Ok(to_screaming_snake(&name))
        }
        "instance_ref" => {
            let name = value.unwrap_or_default();
            Ok(ctx.resolve_instance(&name))
        }
        "return" => {
            let children = db::query_child_exprs(db, expr_id)?;
            if let Some((child_id, _, _, _)) = children.first() {
                emit_expr_from_db(db, *child_id, ctx)
            } else {
                Ok("()".to_string())
            }
        }
        "binop" => {
            let op = value.unwrap_or_default();
            let children = db::query_child_exprs(db, expr_id)?;
            if children.len() >= 2 {
                let left = emit_expr_from_db(db, children[0].0, ctx)?;
                let right = emit_expr_from_db(db, children[1].0, ctx)?;
                Ok(format!("{left} {op} {right}"))
            } else {
                Ok("todo!()".to_string())
            }
        }
        "group" => {
            let children = db::query_child_exprs(db, expr_id)?;
            if let Some((child_id, _, _, _)) = children.first() {
                let inner = emit_expr_from_db(db, *child_id, ctx)?;
                Ok(format!("({inner})"))
            } else {
                Ok("()".to_string())
            }
        }
        "inline_eval" => {
            let children = db::query_child_exprs(db, expr_id)?;
            if children.len() == 1 {
                emit_expr_from_db(db, children[0].0, ctx)
            } else if let Some(last) = children.last() {
                emit_expr_from_db(db, last.0, ctx)
            } else {
                Ok("()".to_string())
            }
        }
        "access" => {
            let field = value.unwrap_or_default();
            let children = db::query_child_exprs(db, expr_id)?;
            if let Some((child_id, child_kind, _, _)) = children.first() {
                let base = emit_expr_from_db(db, *child_id, ctx)?;
                // Wrap inline_eval and binop bases in parens for method calls
                let base = if (child_kind == "inline_eval" || child_kind == "binop") && !base.starts_with('(') {
                    format!("({base})")
                } else {
                    base
                };
                let f = to_snake_case(&field);
                // camelCase = verb = method call, PascalCase = noun = field access
                let is_method = field.starts_with(|c: char| c.is_lowercase())
                    || is_known_method(&f, db)
                    || is_known_method(&field, db);
                if is_method {
                    // Vec .len() returns usize — cast to u32 for aski's fixed-size types.
                    // Ephemeral usize — see module doc for rationale.
                    if f == "len" {
                        Ok(format!("({base}.{f}() as u32)"))
                    } else {
                        Ok(format!("{base}.{f}()"))
                    }
                } else {
                    Ok(format!("{base}.{f}"))
                }
            } else {
                Ok("todo!()".to_string())
            }
        }
        "match" => {
            emit_match_from_db(db, expr_id, ctx)
        }
        "bare_name" => {
            let name = value.unwrap_or_default();
            Ok(ctx.qualify_variant(&name))
        }
        // v0.8: no free function calls — method calls handle everything
        // This arm is kept for backward compat with old DB data
        "fn_call" => {
            let name = value.unwrap_or_default();
            let snake = to_snake_case(&name);
            let children = db::query_child_exprs(db, expr_id)?;
            let mut args = Vec::new();
            for (child_id, _kind, _ord, _val) in &children {
                let arg = emit_expr_from_db(db, *child_id, ctx)?;
                args.push(format!("&{arg}"));
            }
            Ok(format!("{snake}({})", args.join(", ")))
        }
        // v0.8 binding forms used as expressions
        "same_type_new" | "deferred_new" | "sub_type_new" | "mutable_new" => {
            let children = db::query_child_exprs(db, expr_id)?;
            if let Some((child_id, _kind, _ord, _val)) = children.first() {
                emit_expr_from_db(db, *child_id, ctx)
            } else {
                Ok("todo!()".to_string())
            }
        }
        "error_prop" => {
            let children = db::query_child_exprs(db, expr_id)?;
            if let Some((child_id, _kind, _ord, _val)) = children.first() {
                let val = emit_expr_from_db(db, *child_id, ctx)?;
                Ok(format!("{val}?"))
            } else {
                Ok("todo!()".to_string())
            }
        }
        "struct_construct" => {
            let name = value.unwrap_or_default();
            let children = db::query_child_exprs(db, expr_id)?;

            // Check if this is a variant construction (name is in variant_map)
            // If so, generate Domain::Variant(value) instead of Struct { field: value }
            if let Some(domain) = ctx.variant_map.get(&name) {
                // Check if it has a single _0 field (variant wrap)
                if children.len() == 1 {
                    let fname = children[0].3.as_deref().unwrap_or("");
                    if fname == "_0" {
                        let inner = db::query_child_exprs(db, children[0].0)?;
                        if let Some((val_id, _k, _o, _v)) = inner.first() {
                            let val = emit_expr_from_db(db, *val_id, ctx)?;
                            let qualified = ctx.qualify_variant(&name);
                            return Ok(format!("{qualified}({val})"));
                        }
                    }
                }
            }

            let mut field_inits = Vec::new();
            for (child_id, _kind, _ord, field_name) in &children {
                let fname = to_snake_case(field_name.as_deref().unwrap_or(""));
                let inner = db::query_child_exprs(db, *child_id)?;
                if let Some((val_id, _k, _o, _v)) = inner.first() {
                    let val = emit_expr_from_db(db, *val_id, ctx)?;
                    field_inits.push(format!("{fname}: {val}"));
                }
            }
            Ok(format!("{name} {{ {} }}", field_inits.join(", ")))
        }
        "method_call" => {
            let method = value.unwrap_or_default();
            let children = db::query_child_exprs(db, expr_id)?;
            if children.is_empty() {
                return Ok("todo!()".to_string());
            }
            let base = emit_expr_from_db(db, children[0].0, ctx)?;
            // Wrap inline_eval and binop bases in parens
            let base = if (children[0].1 == "inline_eval" || children[0].1 == "binop") && !base.starts_with('(') {
                format!("({base})")
            } else {
                base
            };

            // .with auto-Protocol — struct update syntax
            if method == "with" {
                let mut field_inits = Vec::new();
                for (child_id, child_kind, _ord, child_val) in children.iter().skip(1) {
                    // Args may be struct_construct (FieldName(value)) or bare expressions
                    if child_kind == "struct_construct" {
                        let field_name = to_snake_case(child_val.as_deref().unwrap_or(""));
                        let grandchildren = db::query_child_exprs(db, *child_id)?;
                        if let Some((gc_id, _gk, _go, _gv)) = grandchildren.first() {
                            let gc_inner = db::query_child_exprs(db, *gc_id)?;
                            if let Some((val_id, _vk, _vo, _vv)) = gc_inner.first() {
                                let v = emit_expr_from_db(db, *val_id, ctx)?;
                                field_inits.push(format!("{field_name}: {v}"));
                            } else {
                                let v = emit_expr_from_db(db, *gc_id, ctx)?;
                                field_inits.push(format!("{field_name}: {v}"));
                            }
                        }
                    } else {
                        // Try to extract field name from the expression
                        // This handles the case where the arg is parsed as a method call
                        // like Position([expr]) → bare_name "Position" + inline_eval
                        let v = emit_expr_from_db(db, *child_id, ctx)?;
                        if let Some(name) = child_val.as_deref() {
                            field_inits.push(format!("{}: {v}", to_snake_case(name)));
                        } else {
                            field_inits.push(v);
                        }
                    }
                }
                let type_name = ctx.self_type.as_deref().unwrap_or("Self");
                if field_inits.is_empty() {
                    return Ok(format!("{base}.clone()"));
                }
                return Ok(format!("{type_name} {{ {}, ..{base}.clone() }}", field_inits.join(", ")));
            }

            let snake = to_snake_case(&method);

            // Vec indexing: .get(u32) → .get(u32 as usize)
            // Ephemeral usize — see module doc for rationale.
            if snake == "get" && children.len() == 2 {
                let arg = emit_expr_from_db(db, children[1].0, ctx)?;
                return Ok(format!("{base}.get({arg} as usize).unwrap().clone()"));
            }

            let mut args = Vec::new();
            for (child_id, _kind, _ord, _val) in children.iter().skip(1) {
                let arg = emit_expr_from_db(db, *child_id, ctx)?;
                // Pass by reference for method args — Rust auto-derefs as needed
                args.push(format!("&{arg}"));
            }
            Ok(format!("{base}.{snake}({})", args.join(", ")))
        }
        "comprehension" => {
            let children = db::query_child_exprs(db, expr_id)?;
            // child 0 = source, child 1 = output (optional), child 2 = guard (optional)
            let source = if let Some((child_id, _, _, _)) = children.first() {
                emit_expr_from_db(db, *child_id, ctx)?
            } else {
                return Ok("todo!()".to_string());
            };
            let output_expr = children.iter().find(|(_, _, ord, _)| *ord == 1);
            let guard_expr = children.iter().find(|(_, _, ord, _)| *ord == 2);
            match (output_expr, guard_expr) {
                (None, None) => {
                    // Source-only comprehension: identity
                    Ok(format!("{source}.clone()"))
                }
                (Some((out_id, _, _, _)), None) => {
                    // Map only
                    let output = emit_expr_from_db(db, *out_id, ctx)?;
                    Ok(format!("{source}.iter().map(|item| {output}).collect::<Vec<_>>()"))
                }
                (None, Some((guard_id, _, _, _))) => {
                    // Filter only
                    let guard = emit_expr_from_db(db, *guard_id, ctx)?;
                    Ok(format!("{source}.iter().filter(|item| {guard}).cloned().collect::<Vec<_>>()"))
                }
                (Some((out_id, _, _, _)), Some((guard_id, _, _, _))) => {
                    // Filter + map
                    let output = emit_expr_from_db(db, *out_id, ctx)?;
                    let guard = emit_expr_from_db(db, *guard_id, ctx)?;
                    Ok(format!("{source}.iter().filter(|item| {guard}).map(|item| {output}).collect::<Vec<_>>()"))
                }
            }
        }
        "stub" => Ok("todo!()".to_string()),
        _ => Ok("todo!()".to_string()),
    }
}

fn emit_match_from_db(
    db: &DbInstance,
    match_id: i64,
    ctx: &ExprCtx,
) -> Result<String, String> {
    let children = db::query_child_exprs(db, match_id)?;
    let arms = db::query_match_arms(db, match_id)?;

    if children.is_empty() {
        return Ok("todo!()".to_string());
    }

    let target = emit_expr_from_db(db, children[0].0, ctx)?;

    let mut result = format!("match {target} {{\n");
    let arm_indent = "    ".repeat(ctx.indent + 1);

    for (_ordinal, patterns, body_expr_id) in &arms {
        if let Some(pat_str) = patterns.first() {
            let pat = emit_pattern_from_db(pat_str, ctx, db);
            let body = if let Some(bid) = body_expr_id {
                emit_expr_from_db(db, *bid, ctx)?
            } else {
                "todo!()".to_string()
            };
            result.push_str(&format!("{arm_indent}{pat} => {body},\n"));
        }
    }

    let indent = "    ".repeat(ctx.indent);
    result.push_str(&format!("{indent}}}"));
    Ok(result)
}

fn emit_pattern_from_db(pat_str: &str, ctx: &ExprCtx, db: &DbInstance) -> String {
    let parsed = db::parse_pattern_string(pat_str);
    match parsed {
        db::ParsedPattern::Variant(name) => {
            let qualified = ctx.qualify_variant(&name);
            // Check if this variant carries data — if so, add (..) wildcard
            if let Some(domain_name) = ctx.variant_map.get(&name) {
                if domain_name != "bool" {
                    if let Ok(variants) = db::query_domain_variants(db, domain_name) {
                        for (_ord, vname, wraps) in &variants {
                            if vname == &name && wraps.is_some() {
                                return format!("{qualified}(..)");
                            }
                        }
                    }
                }
            }
            qualified
        }
        db::ParsedPattern::DataCarrying(name, inner) => {
            let qualified = ctx.qualify_variant(&name);
            // Inner binding: @Name → ref variable, _ → wildcard
            if inner.starts_with('@') {
                let bind_name = to_snake_case(&inner[1..]);
                format!("{qualified}({bind_name})")
            } else if inner == "_" {
                format!("{qualified}(..)")
            } else {
                format!("{qualified}(..)")
            }
        }
        db::ParsedPattern::Wildcard => "_".to_string(),
        db::ParsedPattern::BoolLit(b) => b.to_string(),
    }
}

/// Parse an interpolated string like `"3 + 7 = [@Sum.to_string]"`
/// Returns (format_string, vec_of_args)
fn emit_interpolated_string(s: &str, ctx: &ExprCtx) -> (String, Vec<String>) {
    let mut fmt = String::new();
    let mut args = Vec::new();
    let mut i = 0;
    let chars: Vec<char> = s.chars().collect();

    while i < chars.len() {
        if chars[i] == '[' {
            // Find matching ]
            let start = i + 1;
            let mut depth = 1;
            let mut j = start;
            while j < chars.len() && depth > 0 {
                if chars[j] == '[' { depth += 1; }
                if chars[j] == ']' { depth -= 1; }
                j += 1;
            }
            let inner = &s[start..j - 1]; // content between [ and ]
            // Parse the interpolation expression: @Name.method
            let arg = emit_interpolation_expr(inner, ctx);
            // Check if the expression is a domain/enum type binding — use {:?}
            let binding_name = inner.trim_start_matches('@').split('.').next().unwrap_or("");
            let is_debug = is_debug_format_binding(binding_name, ctx);
            if is_debug {
                fmt.push_str("{:?}");
            } else {
                fmt.push_str("{}");
            }
            args.push(arg);
            i = j;
        } else {
            fmt.push(chars[i]);
            i += 1;
        }
    }

    (fmt, args)
}

/// Check if a binding name refers to a domain (enum) type that needs {:?} formatting.
fn is_debug_format_binding(name: &str, ctx: &ExprCtx) -> bool {
    // Look up the binding's declared type
    if let Some(type_name) = ctx.binding_types.get(name) {
        // Check if this type is a domain (has variants in variant_map)
        return ctx.variant_map.values().any(|domain| domain == type_name);
    }
    false
}

/// Emit a Rust expression from an interpolation string like `@Sum.to_string`
fn emit_interpolation_expr(s: &str, ctx: &ExprCtx) -> String {
    let s = s.trim();
    if let Some(stripped) = s.strip_prefix('@') {
        // @Name or @Name.method
        let parts: Vec<&str> = stripped.splitn(2, '.').collect();
        let name = parts[0];
        let var = ctx.resolve_instance(name);
        if parts.len() > 1 {
            let _method = parts[1]; // e.g., "to_string"
            // Just return the variable — to_string is implicit in format!
            var
        } else {
            var
        }
    } else {
        s.to_string()
    }
}

fn is_known_method(name: &str, db: &DbInstance) -> bool {
    if matches!(
        name,
        "sqrt" | "abs" | "len" | "clone" | "to_string" | "is_empty" | "unwrap"
    ) {
        return true;
    }
    // Check if the name is a method defined in the DB (trait or inherent)
    let script = format!(
        "?[id] := *node{{id, kind: 'method', name: '{}'}}", name.replace('\'', "\\'")
    );
    if let Ok(result) = db.run_script(&script, Default::default(), cozo::ScriptMutability::Immutable) {
        if !result.rows.is_empty() {
            return true;
        }
    }
    // Also check method_sig (trait declarations)
    let script2 = format!(
        "?[id] := *node{{id, kind: 'method_sig', name: '{}'}}", name.replace('\'', "\\'")
    );
    if let Ok(result) = db.run_script(&script2, Default::default(), cozo::ScriptMutability::Immutable) {
        if !result.rows.is_empty() {
            return true;
        }
    }
    false
}

// ═══════════════════════════════════════════════════════════════════
// Shared utilities
// ═══════════════════════════════════════════════════════════════════

/// Convert an aski type reference string to a Rust type string.
fn aski_type_to_rust(aski: &str) -> String {
    // Borrowed type: &Type → &RustType
    if let Some(inner) = aski.strip_prefix('&') {
        return format!("&{}", aski_type_to_rust(inner));
    }

    if let Some(idx) = aski.find('(') {
        let name = &aski[..idx];
        let params = &aski[idx + 1..aski.len() - 1];
        let rust_params: Vec<String> = params.split_whitespace().map(aski_type_to_rust).collect();
        return format!("{name}<{}>", rust_params.join(", "));
    }

    match aski {
        "Bool" => "bool".to_string(),
        "String" => "String".to_string(),
        "U8" => "u8".to_string(),
        "U16" => "u16".to_string(),
        "U32" => "u32".to_string(),
        "U64" => "u64".to_string(),
        "I8" => "i8".to_string(),
        "I16" => "i16".to_string(),
        "I32" => "i32".to_string(),
        "I64" => "i64".to_string(),
        "F32" => "f32".to_string(),
        "F64" => "f64".to_string(),
        other => other.to_string(),
    }
}

/// Convert PascalCase to snake_case.
fn to_snake_case(s: &str) -> String {
    let mut result = String::new();
    for (i, ch) in s.chars().enumerate() {
        if ch.is_uppercase() {
            if i > 0 {
                result.push('_');
            }
            result.push(ch.to_lowercase().next().unwrap());
        } else {
            result.push(ch);
        }
    }
    result
}

/// Convert PascalCase to SCREAMING_SNAKE_CASE.
fn to_screaming_snake(s: &str) -> String {
    let mut result = String::new();
    for (i, ch) in s.chars().enumerate() {
        if ch.is_uppercase() && i > 0 {
            result.push('_');
        }
        result.push(ch.to_uppercase().next().unwrap());
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{create_db, insert_ast};
    use crate::parser::parse_source;

    #[test]
    fn codegen_domain() {
        let db = create_db().unwrap();
        let items = parse_source("Element (Fire Earth Air Water)").unwrap();
        insert_ast(&db, &items).unwrap();

        let config = CodegenConfig { rkyv: false };
        let code = generate_rust_from_db_with_config(&db, &config).unwrap();
        assert!(code.contains("pub enum Element"));
        assert!(code.contains("Fire,"));
        assert!(code.contains("Water,"));
    }

    #[test]
    fn codegen_struct() {
        let db = create_db().unwrap();
        let items = parse_source("Point { X F64 Y F64 }").unwrap();
        insert_ast(&db, &items).unwrap();

        let config = CodegenConfig { rkyv: false };
        let code = generate_rust_from_db_with_config(&db, &config).unwrap();
        assert!(code.contains("pub struct Point"));
        assert!(code.contains("x: f64"));
        assert!(code.contains("y: f64"));
    }

    #[test]
    fn type_conversion() {
        assert_eq!(aski_type_to_rust("U32"), "u32");
        assert_eq!(aski_type_to_rust("F64"), "f64");
        assert_eq!(aski_type_to_rust("Bool"), "bool");
        assert_eq!(aski_type_to_rust("String"), "String");
    }

    #[test]
    fn codegen_inherent_method() {
        let db = create_db().unwrap();
        let src = "Addition { Left U32 Right U32 }\nAddition [ add(:@Self) U32 [ ^(@Self.Left + @Self.Right) ] ]";
        let items = parse_source(src).unwrap();
        insert_ast(&db, &items).unwrap();

        let config = CodegenConfig { rkyv: false };
        let code = generate_rust_from_db_with_config(&db, &config).unwrap();
        assert!(code.contains("pub struct Addition"));
        assert!(code.contains("impl Addition"));
        assert!(code.contains("pub fn add(&self)"));
    }

    #[test]
    fn codegen_same_type_binding() {
        let db = create_db().unwrap();
        let src = "Main [ @Radius.new(5.0) ]";
        let items = parse_source(src).unwrap();
        insert_ast(&db, &items).unwrap();

        let config = CodegenConfig { rkyv: false };
        let code = generate_rust_from_db_with_config(&db, &config).unwrap();
        assert!(code.contains("fn main()"));
        assert!(code.contains("let radius = 5"));
    }

    #[test]
    fn codegen_subtype_binding() {
        let db = create_db().unwrap();
        let src = "Main [ @Area F64.new(42.0) ]";
        let items = parse_source(src).unwrap();
        insert_ast(&db, &items).unwrap();

        let config = CodegenConfig { rkyv: false };
        let code = generate_rust_from_db_with_config(&db, &config).unwrap();
        assert!(code.contains("fn main()"));
        assert!(code.contains("let area: f64 = 42"));
    }

    #[test]
    fn codegen_auto_box_recursive_struct() {
        let db = create_db().unwrap();
        // LinkedList contains itself — Left field is LinkedList
        let src = "LinkedList { Value U32 Next LinkedList }";
        let items = parse_source(src).unwrap();
        insert_ast(&db, &items).unwrap();

        let config = CodegenConfig { rkyv: false };
        let code = generate_rust_from_db_with_config(&db, &config).unwrap();
        eprintln!("=== Generated ===\n{code}");
        assert!(code.contains("Box<LinkedList>"), "recursive field should be auto-boxed: {code}");
        assert!(code.contains("value: u32"), "non-recursive field should not be boxed: {code}");
    }

    #[test]
    fn codegen_inline_domain_variant() {
        let db = create_db().unwrap();
        let src = "Domain (One (A B C) Two)";
        let items = parse_source(src).unwrap();
        insert_ast(&db, &items).unwrap();

        let config = CodegenConfig { rkyv: false };
        let code = generate_rust_from_db_with_config(&db, &config).unwrap();
        eprintln!("=== Generated ===\n{code}");
        // Sub-domain One should be its own enum
        assert!(code.contains("pub enum One"), "inline domain should generate sub-enum: {code}");
        assert!(code.contains("A,"), "sub-enum should have variant A: {code}");
        assert!(code.contains("B,"), "sub-enum should have variant B: {code}");
        assert!(code.contains("C,"), "sub-enum should have variant C: {code}");
        // Parent domain should wrap it
        assert!(code.contains("One(One)"), "parent variant should wrap sub-enum: {code}");
        assert!(code.contains("Two,"), "unit variant should remain: {code}");
    }

    #[test]
    fn codegen_operator_trait_impl() {
        let db = create_db().unwrap();
        let src = "Point { X F64 Y F64 }\nadd [Point [\n  add(:@Self @Rhs Point) Point [\n    ^Point(X([@Self.X + @Rhs.X]) Y([@Self.Y + @Rhs.Y]))\n  ]\n]]";
        let items = parse_source(src).unwrap();
        insert_ast(&db, &items).unwrap();

        let config = CodegenConfig { rkyv: false };
        let code = generate_rust_from_db_with_config(&db, &config).unwrap();
        eprintln!("=== Generated ===\n{code}");
        assert!(code.contains("use std::ops::Add;"), "should import Add: {code}");
        assert!(code.contains("impl Add for Point"), "should impl Add: {code}");
        assert!(code.contains("type Output = Point"), "should have Output type: {code}");
        assert!(code.contains("fn add(self"), "should use self not &self: {code}");
    }

    #[test]
    fn codegen_copy_derive_auto() {
        let db = create_db().unwrap();
        // All fields are Copy (f64) → should get Copy derive
        let src = "Point { X F64 Y F64 }";
        let items = parse_source(src).unwrap();
        insert_ast(&db, &items).unwrap();

        let config = CodegenConfig { rkyv: false };
        let code = generate_rust_from_db_with_config(&db, &config).unwrap();
        eprintln!("=== Generated ===\n{code}");
        assert!(code.contains("pub struct Point"));
        assert!(code.contains("Copy"), "all-Copy-field struct should derive Copy: {code}");
    }

    #[test]
    fn codegen_copy_derive_not_for_string() {
        let db = create_db().unwrap();
        // String is not Copy — should NOT get Copy derive
        let src = "Person { Name String Age U32 }";
        let items = parse_source(src).unwrap();
        insert_ast(&db, &items).unwrap();

        let config = CodegenConfig { rkyv: false };
        let code = generate_rust_from_db_with_config(&db, &config).unwrap();
        eprintln!("=== Generated ===\n{code}");
        assert!(code.contains("pub struct Person"));
        assert!(!code.contains("Copy"), "struct with String field should not derive Copy: {code}");
    }

    #[test]
    fn codegen_comprehension() {
        let db = create_db().unwrap();
        let src = "Main [ [| @AllSigns @Sign.element {@Sign.modality == Cardinal} |] ]";
        let items = parse_source(src).unwrap();
        insert_ast(&db, &items).unwrap();

        let config = CodegenConfig { rkyv: false };
        let code = generate_rust_from_db_with_config(&db, &config).unwrap();
        eprintln!("=== Generated ===\n{code}");
        assert!(code.contains("iter()"), "comprehension should generate iterator chain: {code}");
        assert!(code.contains("filter("), "comprehension with guard should use filter: {code}");
        assert!(code.contains("map("), "comprehension with output should use map: {code}");
        assert!(code.contains("collect"), "comprehension should collect: {code}");
    }

    #[test]
    fn codegen_struct_variant() {
        let db = create_db().unwrap();
        let src = "Shape (Circle (F64) Rectangle { Width F64 Height F64 })";
        let items = parse_source(src).unwrap();
        insert_ast(&db, &items).unwrap();

        let config = CodegenConfig { rkyv: false };
        let code = generate_rust_from_db_with_config(&db, &config).unwrap();
        eprintln!("=== Generated ===\n{code}");
        assert!(code.contains("pub enum Shape"), "should generate enum: {code}");
        assert!(code.contains("Circle(f64)"), "should have newtype variant: {code}");
        assert!(code.contains("Rectangle {"), "should have struct variant: {code}");
        assert!(code.contains("width: f64"), "struct variant should have width field: {code}");
        assert!(code.contains("height: f64"), "struct variant should have height field: {code}");
    }
}
