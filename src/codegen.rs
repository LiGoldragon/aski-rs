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

use crate::ir::{self, World};

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
pub fn generate_rust_from_db(db: &World) -> Result<String, String> {
    generate_rust_from_db_with_config(db, &CodegenConfig::default())
}

pub fn generate_rust_from_db_with_config(db: &World, config: &CodegenConfig) -> Result<String, String> {
    let variant_map = build_variant_map_from_db(db)?;
    let struct_fields = build_struct_field_map(db)?;
    // Auto-boxing: find fields that need Box due to recursive types
    let recursive_fields = ir::query_recursive_fields(db).unwrap_or_default();
    let needs_box: std::collections::HashSet<(String, String)> = recursive_fields.iter()
        .map(|(owner, field, _)| (owner.clone(), field.clone()))
        .collect();
    let mut out = String::new();

    let nodes = ir::query_all_top_level_nodes(db)?;

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
            "domain" | "struct" | "const" | "type_alias" => 0,  // type definitions first
            "trait" => 1,                         // trait declarations
            "impl" => 2,                          // implementations
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
            "type_alias" => gen_type_alias_from_db(&mut out, db, *node_id, name)?,
            "trait" => gen_trait_from_db(&mut out, db, *node_id, name)?,
            "impl" => gen_trait_impl_from_db(&mut out, db, *node_id, name, &variant_map, &struct_fields)?,
            "main" => gen_main_from_db(&mut out, db, *node_id, &variant_map, &struct_fields)?,
            _ => {}
        }
    }

    Ok(out)
}

/// Build a variant_name -> domain_name map from the DB.
fn build_variant_map_from_db(db: &World) -> Result<HashMap<String, String>, String> {
    let mut map = HashMap::new();
    let domains = ir::query_nodes_by_kind(db, "domain")?;
    for (_id, domain_name) in &domains {
        let variants = ir::query_domain_variants(db, domain_name)?;
        for (_ord, vname, _wraps) in variants {
            map.insert(vname, domain_name.clone());
        }
    }
    map.insert("True".to_string(), "bool".to_string());
    map.insert("False".to_string(), "bool".to_string());
    Ok(map)
}

/// Build struct_name -> Vec<(field_name, field_type)> map.
fn build_struct_field_map(db: &World) -> Result<HashMap<String, Vec<(String, String)>>, String> {
    let mut map = HashMap::new();
    let structs = ir::query_nodes_by_kind(db, "struct")?;
    for (_id, name) in &structs {
        let fields = ir::query_struct_fields(db, name)?;
        let field_list: Vec<(String, String)> = fields.into_iter()
            .map(|(_ord, fname, ftype)| (fname, ftype))
            .collect();
        map.insert(name.clone(), field_list);
    }
    Ok(map)
}

fn gen_domain_from_db(
    out: &mut String,
    db: &World,
    name: &str,
    config: &CodegenConfig,
    needs_box: &std::collections::HashSet<(String, String)>,
) -> Result<(), String> {
    let variants = ir::query_domain_variants(db, name)?;

    // Check which variants have inline sub-domains (stored as child domain nodes)
    let all_domains = ir::query_nodes_by_kind(db, "domain").unwrap_or_default();
    let sub_domain_names: std::collections::HashSet<String> = all_domains.iter()
        .map(|(_, n)| n.clone())
        .collect();

    // Build a map of variant names that have struct fields (stored as child struct nodes).
    // These are struct variants: `Variant { Field Type ... }`
    let mut struct_variant_fields: HashMap<String, Vec<(i32, String, String)>> = HashMap::new();
    for (_ord, vname, _wraps) in &variants {
        // Try to get struct fields for this variant name
        if let Ok(fields) = ir::query_struct_fields(db, vname) {
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
fn is_copy_eligible(type_name: &str, db: &World) -> bool {
    match type_name {
        "U8" | "U16" | "U32" | "U64" | "U128"
        | "I8" | "I16" | "I32" | "I64" | "I128"
        | "F32" | "F64" | "Bool" => true,
        _ => {
            // Check if it's a domain (enum) with no data-carrying variants
            if let Ok(variants) = ir::query_domain_variants(db, type_name) {
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
    db: &World,
    name: &str,
    config: &CodegenConfig,
    needs_box: &std::collections::HashSet<(String, String)>,
) -> Result<(), String> {
    let fields = ir::query_struct_fields(db, name)?;

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

fn gen_type_alias_from_db(out: &mut String, db: &World, node_id: i64, name: &str) -> Result<(), String> {
    let type_ref = ir::query_return_type(db, node_id)?
        .ok_or_else(|| format!("type alias {} has no aliased type", name))?;
    let rust_type = aski_type_to_rust(&type_ref);
    out.push_str(&format!("type {name} = {rust_type};\n\n"));
    Ok(())
}

fn gen_const_from_db(out: &mut String, db: &World, node_id: i64) -> Result<(), String> {
    let (name, type_ref, has_value) = ir::query_constant(db, node_id)?
        .ok_or_else(|| format!("constant node {} not found", node_id))?;
    let rust_type = aski_type_to_rust(&type_ref);
    let screaming = to_screaming_snake(&name);

    if has_value {
        let children = ir::query_child_exprs(db, node_id)?;
        if let Some((child_id, _kind, _ord, _val)) = children.first() {
            let empty_variants = HashMap::new();
            let empty_fields = HashMap::new();
            let ctx = ExprCtx::new_default(&empty_variants, &empty_fields);
            let val = emit_expr_from_db(db, *child_id, &ctx)?;
            out.push_str(&format!("pub const {screaming}: {rust_type} = {val};\n\n"));
        } else {
            return Err(format!("constant {} has value flag but no value expression", name));
        }
    } else {
        return Err(format!("constant {} has no value", name));
    }
    Ok(())
}


fn gen_trait_from_db(
    out: &mut String,
    db: &World,
    node_id: i64,
    name: &str,
) -> Result<(), String> {
    let rust_name = aski_trait_to_rust(name);
    let supers = ir::query_supertraits(db, node_id)?;
    if supers.is_empty() {
        out.push_str(&format!("pub trait {rust_name} {{\n"));
    } else {
        let super_list: Vec<String> = supers.iter().map(|s| aski_trait_to_rust(s)).collect();
        out.push_str(&format!("pub trait {rust_name}: {} {{\n", super_list.join(" + ")));
    }

    let children = ir::query_child_nodes(db, node_id)?;
    for (child_id, kind, child_name) in &children {
        if kind == "method_sig" {
            let params = ir::query_params(db, *child_id)?;
            let return_type = ir::query_return_type(db, *child_id)?;

            let rust_params = gen_trait_method_params(&params);
            let ret = return_type.as_ref()
                .map(|r| format!(" -> {}", aski_type_to_rust(r)))
                .unwrap_or_default();
            let snake = to_snake_case(child_name);
            out.push_str(&format!("    fn {snake}({rust_params}){ret};\n"));
        } else if kind == "const" {
            if let Some((cname, type_ref, has_value)) = ir::query_constant(db, *child_id)? {
                let rust_type = aski_type_to_rust(&type_ref);
                let screaming = to_screaming_snake(&cname);
                if has_value {
                    let expr_children = ir::query_child_exprs(db, *child_id)?;
                    if let Some((expr_id, _, _, _)) = expr_children.first() {
                        let empty_variants = HashMap::new();
                        let empty_fields = HashMap::new();
                        let ctx = ExprCtx::new_default(&empty_variants, &empty_fields);
                        let val = emit_expr_from_db(db, *expr_id, &ctx)?;
                        out.push_str(&format!("    const {screaming}: {rust_type} = {val};\n"));
                    } else {
                        out.push_str(&format!("    const {screaming}: {rust_type};\n"));
                    }
                } else {
                    out.push_str(&format!("    const {screaming}: {rust_type};\n"));
                }
            }
        }
    }

    out.push_str("}\n\n");
    Ok(())
}

fn gen_trait_impl_from_db(
    out: &mut String,
    db: &World,
    node_id: i64,
    trait_name: &str,
    variant_map: &HashMap<String, String>,
    struct_fields: &HashMap<String, Vec<(String, String)>>,
) -> Result<(), String> {
    let impl_bodies = ir::query_child_nodes(db, node_id)?;

    // Trait names are camelCase in aski — convert to PascalCase for Rust
    let operator_traits_with_output = ["add", "sub", "mul", "div", "rem"];
    let operator_traits_move_self = ["add", "sub", "mul", "div", "rem", "neg", "not"];
    let rust_trait_name = aski_trait_to_rust(trait_name);

    for (impl_body_id, _kind, type_name) in &impl_bodies {
        out.push_str(&format!("impl {rust_trait_name} for {type_name} {{\n"));

        // Emit associated types and constants from DB (assoc_type / const child nodes)
        let all_children = ir::query_child_nodes(db, *impl_body_id)?;
        let mut has_explicit_output = false;
        for (assoc_id, kind, assoc_name) in &all_children {
            if kind == "assoc_type" {
                if let Some(type_ref) = ir::query_return_type(db, *assoc_id)? {
                    out.push_str(&format!("    type {} = {};\n", assoc_name, aski_type_to_rust(&type_ref)));
                    if assoc_name == "Output" {
                        has_explicit_output = true;
                    }
                }
            } else if kind == "const" {
                if let Some((cname, type_ref, has_value)) = ir::query_constant(db, *assoc_id)? {
                    let rust_type = aski_type_to_rust(&type_ref);
                    let screaming = to_screaming_snake(&cname);
                    if has_value {
                        let expr_children = ir::query_child_exprs(db, *assoc_id)?;
                        if let Some((expr_id, _, _, _)) = expr_children.first() {
                            let empty_variants = HashMap::new();
                            let empty_fields = HashMap::new();
                            let ctx = ExprCtx::new_default(&empty_variants, &empty_fields);
                            let val = emit_expr_from_db(db, *expr_id, &ctx)?;
                            out.push_str(&format!("    const {screaming}: {rust_type} = {val};\n"));
                        } else {
                            return Err(format!("impl constant {} has value flag but no value expression", cname));
                        }
                    } else {
                        return Err(format!("impl constant {} has no value", cname));
                    }
                }
            }
        }

        // For operator traits with Output: infer from method return type (if not explicit)
        if !has_explicit_output && operator_traits_with_output.contains(&trait_name) {
            let methods: Vec<_> = all_children.iter().filter(|(_, k, _)| k == "method" || k == "tail_method").collect();
            if let Some((method_id, _, _)) = methods.first() {
                if let Some(ret) = ir::query_return_type(db, *method_id)? {
                    out.push_str(&format!("    type Output = {};\n", aski_type_to_rust(&ret)));
                }
            }
        }

        let is_operator = operator_traits_move_self.contains(&trait_name);
        let methods: Vec<_> = all_children.iter().filter(|(_, k, _)| k == "method" || k == "tail_method").cloned().collect();
        for (method_id, _kind, method_name) in &methods {
            if is_operator {
                gen_operator_method_from_db(out, db, *method_id, method_name, type_name, 1, variant_map, struct_fields)?;
            } else {
                gen_method_impl_from_db(out, db, *method_id, method_name, type_name, 1, variant_map, struct_fields)?;
            }
        }

        out.push_str("}\n\n");
    }
    Ok(())
}

/// Generate an operator trait method (uses `self` not `&self`, owned params).
fn gen_operator_method_from_db(
    out: &mut String,
    db: &World,
    method_id: i64,
    method_name: &str,
    self_type: &str,
    base_indent: usize,
    variant_map: &HashMap<String, String>,
    struct_fields: &HashMap<String, Vec<(String, String)>>,
) -> Result<(), String> {
    let indent = "    ".repeat(base_indent);
    let params = ir::query_params(db, method_id)?;
    let return_type = ir::query_return_type(db, method_id)?;

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
        current_method: Some(method_name.to_string()),
    };
    gen_body_from_db(out, db, method_id, &ctx)?;
    out.push_str(&format!("{indent}}}\n"));
    Ok(())
}

fn gen_method_impl_from_db(
    out: &mut String,
    db: &World,
    method_id: i64,
    method_name: &str,
    self_type: &str,
    base_indent: usize,
    variant_map: &HashMap<String, String>,
    struct_fields: &HashMap<String, Vec<(String, String)>>,
) -> Result<(), String> {
    let indent = "    ".repeat(base_indent);
    let params = ir::query_params(db, method_id)?;
    let return_type = ir::query_return_type(db, method_id)?;

    let rust_params = gen_method_impl_params(&params);
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
        current_method: Some(method_name.to_string()),
    };
    gen_body_from_db(out, db, method_id, &ctx)?;
    out.push_str(&format!("{indent}}}\n"));
    Ok(())
}

fn gen_main_from_db(
    out: &mut String,
    db: &World,
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
        current_method: None,
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
                let var = to_snake_case(n);
                // Params are &Type in generated Rust. For primitive types,
                // auto-deref so arithmetic works: *count instead of count
                let rust_t = aski_type_to_rust(t);
                let resolved = if is_copy_type(&rust_t) {
                    format!("*{var}")
                } else {
                    var.clone()
                };
                map.insert(n.to_string(), resolved.clone());
                map.insert(t.to_string(), resolved);
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
    /// The name of the method currently being generated (for tail-call detection).
    current_method: Option<String>,
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
            current_method: None,
        }
    }

    fn resolve_instance(&self, name: &str) -> String {
        // Check bindings first — allows tail-call optimization to remap Self → _self
        if let Some(var) = self.bindings.get(name) {
            return var.clone();
        }
        if name == "Self" {
            return "self".to_string();
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
    db: &World,
    owner_id: i64,
    ctx: &ExprCtx,
) -> Result<(), String> {
    // Check if this is a matching method body (has match_arm rows)
    let arms = ir::query_match_arms(db, owner_id)?;
    if !arms.is_empty() {
        return gen_matching_body_from_db(out, db, owner_id, ctx, &arms);
    }

    let children = ir::query_child_exprs(db, owner_id)?;

    if children.is_empty() {
        return Err(format!("method body (node {}) has no expressions", owner_id));
    }

    if children.len() == 1 && children[0].1 == "stub" {
        // ___ stub: placeholder for FFI or incomplete methods.
        // Emits todo!() until FFI macro design is complete.
        let indent = "    ".repeat(ctx.indent);
        out.push_str(&format!("{indent}todo!()\n"));
        return Ok(());
    }

    // Tail-call optimization: triggered by [| ... |] body (tail_method node kind).
    // Pattern: binding + return of method_call on the binding, where that method
    // is a matching method with a recursive arm back to current_method.
    let is_tail_method = ir::query_node_kind(db, owner_id)?
        .map(|k| k == "tail_method")
        .unwrap_or(false);

    if is_tail_method && children.len() == 2 && ctx.current_method.is_some() {
        let current = ctx.current_method.as_ref().unwrap().clone();
        let (bind_id, bind_kind, _, bind_val) = &children[0];
        let (ret_id, ret_kind, _, _) = &children[1];

        let is_binding = matches!(bind_kind.as_str(), "same_type_new" | "sub_type_new" | "deferred_new");
        let is_return = ret_kind == "return";

        if is_binding && is_return {
            // Check if the return's inner expression is a method_call on the binding
            let ret_children = ir::query_child_exprs(db, *ret_id)?;
            let ret_inner_id = if let Some((rid, rkind, _, _)) = ret_children.first() {
                if rkind == "group" {
                    ir::query_child_exprs(db, *rid)?.first().map(|(id, _, _, _)| *id).unwrap_or(*rid)
                } else {
                    *rid
                }
            } else {
                0
            };

            if ret_inner_id != 0 {
                if let Ok(Some((ref rk, ref rv))) = ir::query_expr_by_id(db, ret_inner_id) {
                    if rk == "method_call" {
                        if let Some(continuation_method) = rv.as_ref() {
                            // Find method nodes with this name
                            let method_nodes: Vec<(i64, String)> = db.Node.iter()
                                .filter(|(_, kind, name, _, _, _, _)| kind == "method" && name == continuation_method)
                                .map(|(id, _, _, _, _, _, _)| (*id, continuation_method.clone()))
                                .collect();
                            {
                                for (cont_method_id, _) in &method_nodes {
                                    let cont_arms = ir::query_match_arms(db, *cont_method_id).unwrap_or_default();
                                    if !cont_arms.is_empty() {
                                        let has_recursive_arm = cont_arms.iter().any(|(_, _, body_id, _)| {
                                            if let Some(bid) = body_id {
                                                contains_method_call_to(db, *bid, &current)
                                            } else {
                                                false
                                            }
                                        });

                                        if has_recursive_arm {
                                            return gen_fused_tail_call_loop(
                                                out, db, ctx,
                                                *bind_id, bind_kind, bind_val.as_deref(),
                                                ret_inner_id, continuation_method,
                                                *cont_method_id, &cont_arms,
                                            );
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    let indent = "    ".repeat(ctx.indent);
    let mut ctx = ctx.clone();

    for (i, (child_id, kind, _ordinal, value)) in children.iter().enumerate() {
        let is_last = i == children.len() - 1;
        gen_statement_from_db(out, db, *child_id, kind, value.as_deref(), &mut ctx, &indent, is_last)?;
    }
    Ok(())
}

/// Generate a fused tail-call loop from a computed body + matching continuation method.
///
/// Pattern: `let result = self.method_a(); return result.method_b(args)`
/// where method_b has a matching body with one arm recursing back to the current method.
///
/// Generates:
/// ```ignore
/// let mut _self = self;
/// let mut _param = *param;
/// loop {
///     let result: ResultType = _self.method_a();
///     match result {
///         Arm1(binding) => { _self = binding; _param = new_val; }
///         Arm2(_) => { return value; }
///     }
/// }
/// ```
#[allow(clippy::too_many_arguments)]
fn gen_fused_tail_call_loop(
    out: &mut String,
    db: &World,
    ctx: &ExprCtx,
    bind_id: i64,
    bind_kind: &str,
    bind_val: Option<&str>,
    _ret_method_call_id: i64,
    _continuation_method: &str,
    _cont_method_id: i64,
    cont_arms: &[(i64, Vec<String>, Option<i64>, String)],
) -> Result<(), String> {
    let indent = "    ".repeat(ctx.indent);
    let inner_indent = "    ".repeat(ctx.indent + 1);
    let arm_indent = "    ".repeat(ctx.indent + 2);
    let current_method = ctx.current_method.as_ref().unwrap();

    // Collect method params (non-self) for mutable loop variables
    // These are from the CURRENT method's params in ctx.bindings
    // Deduplicate by rust_var since named params insert both name and type keys
    let mut loop_vars: Vec<(String, String)> = Vec::new(); // (original_name, rust_var)
    let mut seen_vars: std::collections::HashSet<String> = std::collections::HashSet::new();
    for (name, rust_var) in &ctx.bindings {
        if name != "Self" {
            let clean_var = rust_var.trim_start_matches('*').to_string();
            if seen_vars.insert(clean_var) {
                loop_vars.push((name.clone(), rust_var.clone()));
            }
        }
    }

    // Emit mutable copies of self and params
    out.push_str(&format!("{indent}let mut _self = self;\n"));
    for (_name, rust_var) in &loop_vars {
        // If the var is *name (deref'd), we need the original var name
        let clean_var = rust_var.trim_start_matches('*');
        out.push_str(&format!("{indent}let mut _{clean_var} = {rust_var};\n"));
    }
    out.push_str(&format!("{indent}loop {{\n"));

    // Emit the binding statement with mutated self reference
    // Build a modified context where Self → _self and params → _param
    let mut loop_ctx = ctx.clone();
    loop_ctx.indent = ctx.indent + 1;
    loop_ctx.bindings.insert("Self".to_string(), "_self".to_string());
    for (_name, rust_var) in &loop_vars {
        let clean_var = rust_var.trim_start_matches('*');
        // Re-insert with underscore prefix
        for (k, v) in ctx.bindings.iter() {
            if v == rust_var {
                loop_ctx.bindings.insert(k.clone(), format!("_{clean_var}"));
            }
        }
    }
    loop_ctx.current_method = None; // prevent nested detection

    // Emit the binding (first statement) — register the binding name
    let binding_name = match bind_kind {
        "same_type_new" | "deferred_new" => bind_val.unwrap_or("result").to_string(),
        "sub_type_new" => {
            if let Some(val) = bind_val {
                val.split(':').next().unwrap_or("result").to_string()
            } else {
                "result".to_string()
            }
        }
        _ => "result".to_string(),
    };
    let binding_var = to_snake_case(&binding_name);

    gen_statement_from_db(out, db, bind_id, bind_kind, bind_val, &mut loop_ctx, &inner_indent, false)?;

    // Now emit the match from the continuation method's arms
    out.push_str(&format!("{inner_indent}match {binding_var} {{\n"));

    for (_ordinal, patterns, body_expr_id, _arm_kind) in cont_arms {
        if let Some(pat_str) = patterns.first() {
            let pat = emit_pattern_from_db(pat_str, &loop_ctx, db);

            // Add pattern bindings to context
            let mut arm_ctx = loop_ctx.clone();
            arm_ctx.indent = ctx.indent + 2;
            let parsed_pat = ir::parse_pattern_string(pat_str);
            if let ir::ParsedPattern::DataCarrying(_, ref inner) = parsed_pat {
                if inner.starts_with('@') {
                    let bind_name = &inner[1..];
                    arm_ctx.bindings.insert(
                        bind_name.to_string(),
                        to_snake_case(bind_name),
                    );
                }
            }

            if let Some(bid) = body_expr_id {
                // Check if this arm calls current_method (recursive arm)
                if contains_method_call_to(db, *bid, current_method) {
                    // This is the recursive arm — extract the receiver and args
                    // and generate reassignment instead of recursion
                    if let Some((_receiver_id, arg_ids)) = is_method_call_to(db, *bid, current_method) {
                        // The receiver becomes the new _self
                        let new_self = emit_expr_from_db(db, _receiver_id, &arm_ctx)?;
                        out.push_str(&format!("{arm_indent}{pat} => {{ _self = {new_self}; "));

                        // Reassign loop vars from method call args
                        for (i, arg_id) in arg_ids.iter().enumerate() {
                            if i < loop_vars.len() {
                                let clean_var = loop_vars[i].1.trim_start_matches('*');
                                let new_val = emit_expr_from_db(db, *arg_id, &arm_ctx)?;
                                // Strip the & prefix that method_call args normally get
                                out.push_str(&format!("_{clean_var} = {new_val}; "));
                            }
                        }
                        out.push_str("}\n");
                    } else {
                        // Fallback — can't extract the recursive call cleanly
                        let body = emit_expr_from_db(db, *bid, &arm_ctx)?;
                        out.push_str(&format!("{arm_indent}{pat} => {{ return {body}; }}\n"));
                    }
                } else {
                    // Non-recursive arm — emit return
                    let body = emit_expr_from_db(db, *bid, &arm_ctx)?;
                    out.push_str(&format!("{arm_indent}{pat} => {{ return {body}; }}\n"));
                }
            } else {
                return Err(format!("tail-call match arm for pattern {} has no body expression", pat));
            }
        }
    }

    out.push_str(&format!("{inner_indent}}}\n"));
    out.push_str(&format!("{indent}}}\n"));

    Ok(())
}

/// Generate Rust match block from a matching method body.
/// The method params are the match targets, the match_arm rows are the arms.
fn gen_matching_body_from_db(
    out: &mut String,
    db: &World,
    _method_id: i64,
    ctx: &ExprCtx,
    arms: &[(i64, Vec<String>, Option<i64>, String)],
) -> Result<(), String> {
    let indent = "    ".repeat(ctx.indent);
    let arm_indent = "    ".repeat(ctx.indent + 1);

    // Determine if multi-value matching by checking pattern count on first arm
    let is_multi = arms.first().map(|(_, pats, _, _)| pats.len() > 1).unwrap_or(false);

    if is_multi {
        // Multi-value matching: build param names from the method's non-self params
        // The param names are snake_cased in gen_method_impl_params, we need them here
        // For now, collect from the arm pattern count and use positional references
        let param_count = arms.first().map(|(_, pats, _, _)| pats.len()).unwrap_or(1);

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

        for (_ordinal, patterns, body_expr_id, _arm_kind) in arms {
            let tuple_pats: Vec<String> = patterns.iter()
                .map(|p| emit_pattern_from_db(p, ctx, db))
                .collect();
            let body = if let Some(bid) = body_expr_id {
                emit_expr_from_db(db, *bid, ctx)?
            } else {
                return Err("multi-value match arm has no body expression".to_string())
            };
            out.push_str(&format!("{arm_indent}({}) => {body},\n", tuple_pats.join(", ")));
        }

        out.push_str(&format!("{indent}}}\n"));
    } else {
        // Check if any arms are backtrack or destructure type
        let has_backtrack = arms.iter().any(|(_, _, _, kind)| kind == "backtrack");
        let has_destructure = arms.iter().any(|(_, _, _, kind)| kind == "destructure");

        if has_backtrack || has_destructure {
            // Backtrack/destructure mode: sequential try
            for (_ordinal, patterns, body_expr_id, arm_kind) in arms {
                if arm_kind == "backtrack" {
                    // Try this arm — if body returns Some/Ok, return it
                    if let Some(bid) = body_expr_id {
                        let mut arm_ctx = ctx.clone();
                        // Add pattern bindings
                        if let Some(pat_str) = patterns.first() {
                            let parsed_pat = ir::parse_pattern_string(pat_str);
                            if let ir::ParsedPattern::DataCarrying(_, ref inner) = parsed_pat {
                                if inner.starts_with('@') {
                                    let bind_name = &inner[1..];
                                    arm_ctx.bindings.insert(bind_name.to_string(), to_snake_case(bind_name));
                                }
                            }
                        }
                        let body = emit_expr_from_db(db, *bid, &arm_ctx)?;
                        let pat_is_wildcard = patterns.first().map(|p| p == "_").unwrap_or(false);
                        if pat_is_wildcard {
                            out.push_str(&format!("{indent}if let result @ Some(_) = {body} {{ return result; }}\n"));
                        } else {
                            let pat = emit_pattern_from_db(patterns.first().unwrap(), ctx, db);
                            out.push_str(&format!("{indent}if matches!(self, {pat}) {{\n"));
                            out.push_str(&format!("{arm_indent}if let result @ Some(_) = {body} {{ return result; }}\n"));
                            out.push_str(&format!("{indent}}}\n"));
                        }
                    }
                } else if arm_kind == "destructure" {
                    // Destructure arm: split sequence, try body
                    // The destructure elements are stored in the patterns list
                    // Format: "destr:Element1,Element2,...|RestName"
                    if let Some(bid) = body_expr_id {
                        if let Some(destr_str) = patterns.first() {
                            if let Some(destr_data) = destr_str.strip_prefix("destr:") {
                                let (elements_str, rest_name) = if let Some(pipe_pos) = destr_data.find('|') {
                                    (&destr_data[..pipe_pos], &destr_data[pipe_pos + 1..])
                                } else {
                                    (destr_data, "rest")
                                };
                                let elements: Vec<&str> = elements_str.split(',').collect();
                                let rest_var = to_snake_case(rest_name);

                                // Generate: check that self has enough elements, then match head elements
                                // and bind the rest
                                let elem_count = elements.len();
                                out.push_str(&format!("{indent}if self.len() >= {elem_count} {{\n"));

                                // Match each head element
                                let mut conditions = Vec::new();
                                let mut bindings = Vec::new();
                                for (i, elem) in elements.iter().enumerate() {
                                    if elem.starts_with('@') {
                                        // Binding element — bind it
                                        let bind_name = &elem[1..];
                                        let var = to_snake_case(bind_name);
                                        bindings.push((var.clone(), i));
                                    } else if *elem != "_" {
                                        // Exact token match
                                        let qualified = ctx.qualify_variant(elem);
                                        conditions.push(format!("self[{i}] == {qualified}"));
                                    }
                                }

                                if !conditions.is_empty() {
                                    out.push_str(&format!("{arm_indent}if {} {{\n", conditions.join(" && ")));
                                    let inner_indent = "    ".repeat(ctx.indent + 2);
                                    for (var, i) in &bindings {
                                        out.push_str(&format!("{inner_indent}let {var} = self[{i}].clone();\n"));
                                    }
                                    out.push_str(&format!("{inner_indent}let {rest_var} = self[{elem_count}..].to_vec();\n"));
                                    let mut arm_ctx = ctx.clone();
                                    arm_ctx.bindings.insert(rest_name.to_string(), rest_var.clone());
                                    for (var, _) in &bindings {
                                        arm_ctx.bindings.insert(var.clone(), var.clone());
                                    }
                                    let body = emit_expr_from_db(db, *bid, &arm_ctx)?;
                                    out.push_str(&format!("{inner_indent}if let result @ Some(_) = {body} {{ return result; }}\n"));
                                    out.push_str(&format!("{arm_indent}}}\n"));
                                } else {
                                    for (var, i) in &bindings {
                                        out.push_str(&format!("{arm_indent}let {var} = self[{i}].clone();\n"));
                                    }
                                    out.push_str(&format!("{arm_indent}let {rest_var} = self[{elem_count}..].to_vec();\n"));
                                    let mut arm_ctx = ctx.clone();
                                    arm_ctx.bindings.insert(rest_name.to_string(), rest_var.clone());
                                    for (var, _) in &bindings {
                                        arm_ctx.bindings.insert(var.clone(), var.clone());
                                    }
                                    let body = emit_expr_from_db(db, *bid, &arm_ctx)?;
                                    out.push_str(&format!("{arm_indent}if let result @ Some(_) = {body} {{ return result; }}\n"));
                                }

                                out.push_str(&format!("{indent}}}\n"));
                            }
                        }
                    }
                } else {
                    // Commit arm — unconditional return (usually the final fallback)
                    if let Some(bid) = body_expr_id {
                        let mut arm_ctx = ctx.clone();
                        if let Some(pat_str) = patterns.first() {
                            let parsed_pat = ir::parse_pattern_string(pat_str);
                            if let ir::ParsedPattern::DataCarrying(_, ref inner) = parsed_pat {
                                if inner.starts_with('@') {
                                    let bind_name = &inner[1..];
                                    arm_ctx.bindings.insert(bind_name.to_string(), to_snake_case(bind_name));
                                }
                            }
                        }
                        let body = emit_expr_from_db(db, *bid, &arm_ctx)?;
                        let pat_is_wildcard = patterns.first().map(|p| p == "_").unwrap_or(true);
                        if pat_is_wildcard {
                            out.push_str(&format!("{indent}{body}\n"));
                        } else {
                            // Commit arm with pattern in a backtrack body — emit as match
                            let pat = emit_pattern_from_db(patterns.first().unwrap(), ctx, db);
                            out.push_str(&format!("{indent}match self {{\n"));
                            out.push_str(&format!("{arm_indent}{pat} => {body},\n"));
                            out.push_str(&format!("{arm_indent}_ => None,\n"));
                            out.push_str(&format!("{indent}}}\n"));
                        }
                    }
                }
            }
            // If the last arm wasn't a commit, add None fallback
            let last_kind = arms.last().map(|(_, _, _, k)| k.as_str()).unwrap_or("commit");
            if last_kind != "commit" {
                out.push_str(&format!("{indent}None\n"));
            }
        } else {
            // Single-value matching: match on self (all commit arms)
            out.push_str(&format!("{indent}match self {{\n"));

            for (_ordinal, patterns, body_expr_id, _arm_kind) in arms {
                if let Some(pat_str) = patterns.first() {
                    let pat = emit_pattern_from_db(pat_str, ctx, db);

                    // Add pattern bindings to context for the arm body
                    let mut arm_ctx = ctx.clone();
                    let parsed_pat = ir::parse_pattern_string(pat_str);
                    if let ir::ParsedPattern::DataCarrying(_, ref inner) = parsed_pat {
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
                        return Err(format!("match arm for pattern {} has no body expression", pat));
                    };
                    out.push_str(&format!("{arm_indent}{pat} => {body},\n"));
                }
            }

            out.push_str(&format!("{indent}}}\n"));
        }
    }

    Ok(())
}

fn gen_statement_from_db(
    out: &mut String,
    db: &World,
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
            let children = ir::query_child_exprs(db, expr_id)?;
            if children.is_empty() {
                out.push_str(&format!("{indent}let {var_name} = {name} {{}};\n"));
            } else if children.len() == 1 {
                let (child_id, child_kind, _ord, child_val) = &children[0];
                if child_kind == "struct_construct" {
                    // Single field pair: Value(5.0) → Radius { value: 5.0 }
                    // The struct_construct's name is the field name, wrap in binding type
                    let field_name = to_snake_case(child_val.as_deref().unwrap_or(""));
                    let grandchildren = ir::query_child_exprs(db, *child_id)?;
                    if let Some((gc_id, _gk, _go, _gv)) = grandchildren.first() {
                        let gc_inner = ir::query_child_exprs(db, *gc_id)?;
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
                        let inner = ir::query_child_exprs(db, *child_id)?;
                        if let Some((val_id, _k, _o, _v)) = inner.first() {
                            let v = emit_expr_from_db(db, *val_id, ctx)?;
                            field_inits.push(format!("{fname}: {v}"));
                        }
                    } else if child_kind == "struct_construct" {
                        // A field initializer like Value(5.0) parsed as struct_construct
                        let inner_name = child_val.as_deref().unwrap_or("");
                        let fname = to_snake_case(inner_name);
                        let grandchildren = ir::query_child_exprs(db, *child_id)?;
                        if let Some((gc_id, _gk, _go, _gv)) = grandchildren.first() {
                            let gc_children = ir::query_child_exprs(db, *gc_id)?;
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
                    let children = ir::query_child_exprs(db, expr_id)?;
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
                                let grandchildren = ir::query_child_exprs(db, *child_id)?;
                                if let Some((gc_id, _gk, _go, _gv)) = grandchildren.first() {
                                    let gc_inner = ir::query_child_exprs(db, *gc_id)?;
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
                    let children = ir::query_child_exprs(db, expr_id)?;
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
            let children = ir::query_child_exprs(db, expr_id)?;
            if let Some((child_id, _kind, _ord, _val)) = children.first() {
                let val = emit_expr_from_db(db, *child_id, ctx)?;
                out.push_str(&format!("{indent}{var_name} = {val};\n"));
            }
        }
        "error_prop" => {
            let children = ir::query_child_exprs(db, expr_id)?;
            if let Some((child_id, _kind, _ord, _val)) = children.first() {
                let val = emit_expr_from_db(db, *child_id, ctx)?;
                out.push_str(&format!("{indent}{val}?;\n"));
            }
        }
        "return" => {
            let children = ir::query_child_exprs(db, expr_id)?;
            if let Some((child_id, child_kind, _ord, _val)) = children.first() {
                let actual_id = if child_kind == "group" {
                    let inner = ir::query_child_exprs(db, *child_id)?;
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
            let children = ir::query_child_exprs(db, expr_id)?;
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
    db: &World,
    expr_id: i64,
    ctx: &ExprCtx,
) -> Result<String, String> {
    let (kind, value) = ir::query_expr_by_id(db, expr_id)?
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
            let children = ir::query_child_exprs(db, expr_id)?;
            if let Some((child_id, _, _, _)) = children.first() {
                emit_expr_from_db(db, *child_id, ctx)
            } else {
                Ok("()".to_string())
            }
        }
        "binop" => {
            let op = value.unwrap_or_default();
            let children = ir::query_child_exprs(db, expr_id)?;
            if children.len() >= 2 {
                let left = emit_expr_from_db(db, children[0].0, ctx)?;
                let right = emit_expr_from_db(db, children[1].0, ctx)?;
                Ok(format!("{left} {op} {right}"))
            } else {
                Err(format!("binary operator '{}' requires two operands, found {}", op, children.len()))
            }
        }
        "group" => {
            let children = ir::query_child_exprs(db, expr_id)?;
            if let Some((child_id, _, _, _)) = children.first() {
                let inner = emit_expr_from_db(db, *child_id, ctx)?;
                Ok(format!("({inner})"))
            } else {
                Ok("()".to_string())
            }
        }
        "inline_eval" => {
            let children = ir::query_child_exprs(db, expr_id)?;
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
            let children = ir::query_child_exprs(db, expr_id)?;
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
                    || is_known_method_codegen(&f, db)
                    || is_known_method_codegen(&field, db);
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
                Err(format!("field access '{}' has no receiver expression", field))
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
            let children = ir::query_child_exprs(db, expr_id)?;
            let mut args = Vec::new();
            for (child_id, _kind, _ord, _val) in &children {
                let arg = emit_expr_from_db(db, *child_id, ctx)?;
                args.push(format!("&{arg}"));
            }
            Ok(format!("{snake}({})", args.join(", ")))
        }
        // v0.8 binding forms used as expressions
        "same_type_new" | "deferred_new" | "sub_type_new" | "mutable_new" => {
            let children = ir::query_child_exprs(db, expr_id)?;
            if let Some((child_id, _kind, _ord, _val)) = children.first() {
                emit_expr_from_db(db, *child_id, ctx)
            } else {
                Err(format!("binding expression (kind '{}') has no child expression", kind))
            }
        }
        "error_prop" => {
            let children = ir::query_child_exprs(db, expr_id)?;
            if let Some((child_id, _kind, _ord, _val)) = children.first() {
                let val = emit_expr_from_db(db, *child_id, ctx)?;
                Ok(format!("{val}?"))
            } else {
                Err("error propagation (?) has no inner expression".to_string())
            }
        }
        "struct_construct" => {
            let name = value.unwrap_or_default();
            let children = ir::query_child_exprs(db, expr_id)?;

            // Check if this is a variant construction (name is in variant_map)
            // If so, generate Domain::Variant(value) instead of Struct { field: value }
            if let Some(domain) = ctx.variant_map.get(&name) {
                // Check if it has a single _0 field (variant wrap)
                if children.len() == 1 {
                    let fname = children[0].3.as_deref().unwrap_or("");
                    if fname == "_0" {
                        let inner = ir::query_child_exprs(db, children[0].0)?;
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
                let inner = ir::query_child_exprs(db, *child_id)?;
                if let Some((val_id, _k, _o, _v)) = inner.first() {
                    let val = emit_expr_from_db(db, *val_id, ctx)?;
                    field_inits.push(format!("{fname}: {val}"));
                }
            }
            Ok(format!("{name} {{ {} }}", field_inits.join(", ")))
        }
        "method_call" => {
            let method = value.unwrap_or_default();
            let children = ir::query_child_exprs(db, expr_id)?;
            if children.is_empty() {
                return Err(format!("method call '{}' has no receiver expression", method));
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
                        let grandchildren = ir::query_child_exprs(db, *child_id)?;
                        if let Some((gc_id, _gk, _go, _gv)) = grandchildren.first() {
                            let gc_inner = ir::query_child_exprs(db, *gc_id)?;
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

            // Collection trait methods: filter, map, each, find, count
            let collection_methods = ["filter", "map", "each", "find", "count"];
            if collection_methods.contains(&snake.as_str()) && children.len() >= 2 {
                // Find element binding: scan arg expression tree for instance_ref not in ctx.bindings
                let arg_id = children[1].0;
                let mut unknown_refs = Vec::new();
                collect_unknown_instance_refs(db, arg_id, ctx, &mut unknown_refs);
                let elem_var = if let Some(ref_name) = unknown_refs.first() {
                    to_snake_case(ref_name)
                } else {
                    "item".to_string()
                };
                let elem_binding_name = unknown_refs.first().cloned().unwrap_or_default();

                // Build a closure context with the element binding
                let mut closure_ctx = ctx.clone();
                if !elem_binding_name.is_empty() {
                    closure_ctx.bindings.insert(elem_binding_name, elem_var.clone());
                }

                let body_expr = emit_expr_from_db(db, arg_id, &closure_ctx)?;

                return match snake.as_str() {
                    "filter" => Ok(format!("{base}.iter().filter(|{elem_var}| {body_expr}).cloned().collect::<Vec<_>>()")),
                    "map" => Ok(format!("{base}.iter().map(|{elem_var}| {body_expr}).collect::<Vec<_>>()")),
                    "each" => Ok(format!("for {elem_var} in &{base} {{ {body_expr}; }}")),
                    "find" => Ok(format!("{base}.iter().find(|{elem_var}| {body_expr}).cloned()")),
                    "count" => Ok(format!("({base}.iter().filter(|{elem_var}| {body_expr}).count() as u32)")),
                    _ => unreachable!(),
                };
            }

            let mut args = Vec::new();
            for (child_id, _kind, _ord, _val) in children.iter().skip(1) {
                let arg = emit_expr_from_db(db, *child_id, ctx)?;
                // Pass by reference — wrap in parens if arg contains operators
                if arg.contains(' ') || arg.starts_with('*') {
                    args.push(format!("&({arg})"));
                } else {
                    args.push(format!("&{arg}"));
                }
            }
            Ok(format!("{base}.{snake}({})", args.join(", ")))
        }
        "range_exclusive" => {
            let children = ir::query_child_exprs(db, expr_id)?;
            if children.len() >= 2 {
                let start = emit_expr_from_db(db, children[0].0, ctx)?;
                let end = emit_expr_from_db(db, children[1].0, ctx)?;
                Ok(format!("{start}..{end}"))
            } else {
                Err(format!("exclusive range requires two bounds, found {}", children.len()))
            }
        }
        "range_inclusive" => {
            let children = ir::query_child_exprs(db, expr_id)?;
            if children.len() >= 2 {
                let start = emit_expr_from_db(db, children[0].0, ctx)?;
                let end = emit_expr_from_db(db, children[1].0, ctx)?;
                Ok(format!("{start}..={end}"))
            } else {
                Err(format!("inclusive range requires two bounds, found {}", children.len()))
            }
        }
        // ___ stub: emits todo!() for FFI and incomplete methods.
        // Stubs are the ONLY remaining source of todo!() in generated code.
        // Once FFI macros are designed, stubs will be rejected at compile time.
        "stub" => Ok("todo!()".to_string()),
        "yield" => {
            let children = ir::query_child_exprs(db, expr_id)?;
            if let Some((child_id, _, _, _)) = children.first() {
                let val = emit_expr_from_db(db, *child_id, ctx)?;
                Ok(format!("yield {val}"))
            } else {
                Err("yield requires an expression".to_string())
            }
        }
        other => Err(format!("unrecognized expression kind: {}", other)),
    }
}

fn emit_match_from_db(
    db: &World,
    match_id: i64,
    ctx: &ExprCtx,
) -> Result<String, String> {
    let children = ir::query_child_exprs(db, match_id)?;
    let arms = ir::query_match_arms(db, match_id)?;

    if children.is_empty() {
        return Err("match expression has no target expression".to_string());
    }

    let target = emit_expr_from_db(db, children[0].0, ctx)?;

    let mut result = format!("match {target} {{\n");
    let arm_indent = "    ".repeat(ctx.indent + 1);

    for (_ordinal, patterns, body_expr_id, _arm_kind) in &arms {
        if let Some(pat_str) = patterns.first() {
            let pat = emit_pattern_from_db(pat_str, ctx, db);
            let body = if let Some(bid) = body_expr_id {
                emit_expr_from_db(db, *bid, ctx)?
            } else {
                return Err(format!("match arm for pattern {} has no body expression", pat));
            };
            result.push_str(&format!("{arm_indent}{pat} => {body},\n"));
        }
    }

    let indent = "    ".repeat(ctx.indent);
    result.push_str(&format!("{indent}}}"));
    Ok(result)
}

fn emit_pattern_from_db(pat_str: &str, ctx: &ExprCtx, db: &World) -> String {
    let parsed = ir::parse_pattern_string(pat_str);
    match parsed {
        ir::ParsedPattern::Variant(name) => {
            let qualified = ctx.qualify_variant(&name);
            // Check if this variant carries data — if so, add (..) wildcard
            if let Some(domain_name) = ctx.variant_map.get(&name) {
                if domain_name != "bool" {
                    if let Ok(variants) = ir::query_domain_variants(db, domain_name) {
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
        ir::ParsedPattern::DataCarrying(name, inner) => {
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
        ir::ParsedPattern::Wildcard => "_".to_string(),
        ir::ParsedPattern::BoolLit(b) => b.to_string(),
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

/// Check if an expression node is a method_call to a specific method name.
/// Returns Some((receiver_expr_id, vec_of_arg_ids)) if it matches, None otherwise.
fn is_method_call_to(
    db: &World,
    expr_id: i64,
    method_name: &str,
) -> Option<(i64, Vec<i64>)> {
    if let Ok(Some((kind, value))) = ir::query_expr_by_id(db, expr_id) {
        if kind == "method_call" && value.as_deref() == Some(method_name) {
            if let Ok(children) = ir::query_child_exprs(db, expr_id) {
                if !children.is_empty() {
                    let receiver = children[0].0;
                    let args: Vec<i64> = children.iter().skip(1).map(|(id, _, _, _)| *id).collect();
                    return Some((receiver, args));
                }
            }
        }
    }
    None
}

/// Check if any expression in a subtree contains a method_call to a specific method name.
fn contains_method_call_to(
    db: &World,
    expr_id: i64,
    method_name: &str,
) -> bool {
    if is_method_call_to(db, expr_id, method_name).is_some() {
        return true;
    }
    if let Ok(children) = ir::query_child_exprs(db, expr_id) {
        for (child_id, _, _, _) in &children {
            if contains_method_call_to(db, *child_id, method_name) {
                return true;
            }
        }
    }
    false
}

/// Recursively collect instance_ref names from an expression tree that are NOT in ctx.bindings.
/// These are inferred element bindings for collection trait methods.
fn collect_unknown_instance_refs(
    db: &World,
    expr_id: i64,
    ctx: &ExprCtx,
    out: &mut Vec<String>,
) {
    if let Ok(Some((kind, value))) = ir::query_expr_by_id(db, expr_id) {
        if kind == "instance_ref" {
            if let Some(name) = value {
                if name != "Self" && !ctx.bindings.contains_key(&name) {
                    if !out.contains(&name) {
                        out.push(name);
                    }
                }
            }
        }
        // Recurse into children
        if let Ok(children) = ir::query_child_exprs(db, expr_id) {
            for (child_id, _, _, _) in &children {
                collect_unknown_instance_refs(db, *child_id, ctx, out);
            }
        }
    }
}

fn is_known_method_codegen(name: &str, world: &World) -> bool {
    ir::is_known_method(name, world)
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

    // Trait bound: "sort&display" → "impl Sort + Display"
    if aski.contains('&') && !aski.contains('(') {
        let bounds: Vec<String> = aski.split('&')
            .map(|b| aski_trait_to_rust(b.trim()))
            .collect();
        return format!("impl {}", bounds.join(" + "));
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
    use crate::ir;
    use crate::parser::parse_source;

    fn setup(src: &str) -> World {
        let mut world = ir::create_world();
        let items = parse_source(src).unwrap();
        ir::insert_ast(&mut world, &items).unwrap();
        ir::run_rules(&mut world);
        world
    }

    #[test]
    fn codegen_domain() {
        let db = setup("Element (Fire Earth Air Water)");
        let config = CodegenConfig { rkyv: false };
        let code = generate_rust_from_db_with_config(&db, &config).unwrap();
        assert!(code.contains("pub enum Element"));
        assert!(code.contains("Fire,"));
        assert!(code.contains("Water,"));
    }

    #[test]
    fn codegen_struct() {
        let db = setup("Point { X F64 Y F64 }");
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
    fn codegen_trait_impl_method() {
        let db = setup("Addition { Left U32 Right U32 }\ncompute [Addition [add(:@Self) U32 [ ^(@Self.Left + @Self.Right) ]]]");
        let config = CodegenConfig { rkyv: false };
        let code = generate_rust_from_db_with_config(&db, &config).unwrap();
        assert!(code.contains("pub struct Addition"));
        assert!(code.contains("impl Compute for Addition"));
        assert!(code.contains("fn add(&self)"));
    }

    #[test]
    fn codegen_same_type_binding() {
        let db = setup("Main [ @Radius.new(5.0) ]");
        let config = CodegenConfig { rkyv: false };
        let code = generate_rust_from_db_with_config(&db, &config).unwrap();
        assert!(code.contains("fn main()"));
        assert!(code.contains("let radius = 5"));
    }

    #[test]
    fn codegen_subtype_binding() {
        let db = setup("Main [ @Area F64.new(42.0) ]");
        let config = CodegenConfig { rkyv: false };
        let code = generate_rust_from_db_with_config(&db, &config).unwrap();
        assert!(code.contains("fn main()"));
        assert!(code.contains("let area: f64 = 42"));
    }

    #[test]
    fn codegen_auto_box_recursive_struct() {
        let db = setup("LinkedList { Value U32 Next LinkedList }");
        let config = CodegenConfig { rkyv: false };
        let code = generate_rust_from_db_with_config(&db, &config).unwrap();
        eprintln!("=== Generated ===\n{code}");
        assert!(code.contains("Box<LinkedList>"), "recursive field should be auto-boxed: {code}");
        assert!(code.contains("value: u32"), "non-recursive field should not be boxed: {code}");
    }

    #[test]
    fn codegen_inline_domain_variant() {
        let db = setup("Domain (One (A B C) Two)");
        let config = CodegenConfig { rkyv: false };
        let code = generate_rust_from_db_with_config(&db, &config).unwrap();
        eprintln!("=== Generated ===\n{code}");
        assert!(code.contains("pub enum One"), "inline domain should generate sub-enum: {code}");
        assert!(code.contains("A,"), "sub-enum should have variant A: {code}");
        assert!(code.contains("B,"), "sub-enum should have variant B: {code}");
        assert!(code.contains("C,"), "sub-enum should have variant C: {code}");
        assert!(code.contains("One(One)"), "parent variant should wrap sub-enum: {code}");
        assert!(code.contains("Two,"), "unit variant should remain: {code}");
    }

    #[test]
    fn codegen_operator_trait_impl() {
        let db = setup("Point { X F64 Y F64 }\nadd [Point [\n  add(:@Self @Rhs Point) Point [\n    ^Point(X([@Self.X + @Rhs.X]) Y([@Self.Y + @Rhs.Y]))\n  ]\n]]");
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
        let db = setup("Point { X F64 Y F64 }");
        let config = CodegenConfig { rkyv: false };
        let code = generate_rust_from_db_with_config(&db, &config).unwrap();
        eprintln!("=== Generated ===\n{code}");
        assert!(code.contains("pub struct Point"));
        assert!(code.contains("Copy"), "all-Copy-field struct should derive Copy: {code}");
    }

    #[test]
    fn codegen_copy_derive_not_for_string() {
        let db = setup("Person { Name String Age U32 }");
        let config = CodegenConfig { rkyv: false };
        let code = generate_rust_from_db_with_config(&db, &config).unwrap();
        eprintln!("=== Generated ===\n{code}");
        assert!(code.contains("pub struct Person"));
        assert!(!code.contains("Copy"), "struct with String field should not derive Copy: {code}");
    }

    #[test]
    fn codegen_struct_variant() {
        let db = setup("Shape (Circle (F64) Rectangle { Width F64 Height F64 })");
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
