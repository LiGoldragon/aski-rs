//! emit.rs — Rust code generator from ParseNode trees + Type/Variant/Field relations.
//!
//! Reads a World containing:
//! - ParseNode tree (the CST from representation.rs)
//! - Type/Variant/Field relations (schema data)
//! - FfiEntry relations (FFI declarations)
//! - Derived relations (VariantOf, RecursiveType, ContainedType)
//!
//! Emits valid Rust source code for an aski module.

use aski_core::{self, World, ParseNode, RustSpan, TypeForm};
use std::collections::HashSet;

// ═══════════════════════════════════════════════════════════════
// Configuration
// ═══════════════════════════════════════════════════════════════

pub struct CodegenConfig {
    pub rkyv: bool,
}

impl Default for CodegenConfig {
    fn default() -> Self { Self { rkyv: false } }
}

// ═══════════════════════════════════════════════════════════════
// Public API
// ═══════════════════════════════════════════════════════════════

pub fn generate(world: &World) -> Result<String, String> {
    generate_with_config(world, &CodegenConfig::default())
}

pub fn generate_with_config(world: &World, config: &CodegenConfig) -> Result<String, String> {
    let mut out = String::new();

    // Find the Root node
    let root = world.parse_nodes.iter()
        .find(|n| n.constructor == "Root")
        .ok_or("no Root node in World")?;
    let items = children(world, root.id);

    // Detect what imports we need
    let has_ffi = items.iter().any(|n| n.constructor == "ForeignBlock");
    if has_ffi {
        out.push_str("use crate::helpers::{StringExt, VecExt, ToI64, WithPush};\n");
    }

    // Collect operator trait imports from TraitImpl nodes
    let mut op_traits: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for item in &items {
        if item.constructor == "TraitImpl" {
            let trait_name = &item.text;
            let type_impls = children(world, item.id);
            for _ti in &type_impls {
                if is_operator_trait(trait_name) {
                    op_traits.insert(rust_trait(trait_name));
                }
            }
        }
    }
    for imp in &op_traits {
        out.push_str(&format!("use std::ops::{imp};\n"));
    }
    if !op_traits.is_empty() { out.push('\n'); }

    // Compute no-rkyv set (types that recursively contain themselves)
    let no_rkyv = compute_no_rkyv_set(world);
    let mut emitted_accessors: HashSet<String> = HashSet::new();
    let mut emitted_aliases: HashSet<String> = HashSet::new();

    // Sort items: domains+structs+consts+ffi first, then traits, then impls, then main
    let mut sorted: Vec<&ParseNode> = items.iter().copied().collect();
    sorted.sort_by_key(|n| match n.constructor.as_str() {
        "ForeignBlock" | "Domain" | "Struct" | "Const" | "TypeAlias" => 0u8,
        "TraitDecl" => 1,
        "TraitImpl" => 2,
        "Main" => 3,
        _ => 4,
    });

    for node in &sorted {
        match node.constructor.as_str() {
            "Domain" => emit_domain(&mut out, world, node, config)?,
            "Struct" => emit_struct(&mut out, world, node, config, &no_rkyv, &mut emitted_accessors)?,
            "Const" => emit_const(&mut out, world, node)?,
            "TypeAlias" => {
                if emitted_aliases.insert(node.text.clone()) {
                    emit_type_alias(&mut out, world, node)?;
                }
            }
            "TraitDecl" => emit_trait_decl(&mut out, world, node)?,
            "TraitImpl" => emit_trait_impl(&mut out, world, node)?,
            "Main" => emit_main(&mut out, world, node)?,
            "ForeignBlock" => emit_foreign_block(&mut out, world, node)?,
            "GrammarRule" => {} // grammar rules are for the parser, not emitted
            _ => {}
        }
    }

    Ok(out)
}

// ═══════════════════════════════════════════════════════════════
// Naming helpers
// ═══════════════════════════════════════════════════════════════

fn snake(s: &str) -> String {
    let mut out = String::new();
    for (i, ch) in s.chars().enumerate() {
        if ch.is_uppercase() && i > 0 { out.push('_'); }
        out.push(ch.to_ascii_lowercase());
    }
    match out.as_str() {
        "type" | "match" | "ref" | "mod" | "fn" | "struct" | "enum"
        | "impl" | "trait" | "where" | "self" | "super" | "crate"
        | "use" | "pub" | "in" | "for" | "loop" | "while" | "if"
        | "else" | "return" | "break" | "continue" | "as" | "const"
        | "static" | "mut" | "move" | "let" | "async" | "await"
        | "dyn" | "abstract" | "become" | "box" | "do" | "final"
        | "macro" | "override" | "priv" | "typeof" | "unsized"
        | "virtual" | "yield" | "try" => format!("r#{out}"),
        _ => out,
    }
}

fn screaming(s: &str) -> String { snake(s).to_uppercase() }

fn rust_type(t: &str) -> String {
    match t {
        "F32" => "f32", "F64" => "f64",
        "I8" => "i8", "I16" => "i16", "I32" => "i32", "I64" => "i64",
        "U8" => "u8", "U16" => "u16", "U32" => "u32", "U64" => "u64",
        "Bool" => "bool", "String" => "String",
        _ if t.starts_with("Vec{") && t.ends_with('}') =>
            return format!("Vec<{}>", rust_type(&t[4..t.len()-1])),
        _ if t.starts_with("Vec(") && t.ends_with(')') =>
            return format!("Vec<{}>", rust_type(&t[4..t.len()-1])),
        _ if t.starts_with("Option(") && t.ends_with(')') =>
            return format!("Option<{}>", rust_type(&t[7..t.len()-1])),
        _ if t.starts_with("Result(") && t.ends_with(')') => {
            let inner = &t[7..t.len()-1];
            let parts: Vec<&str> = inner.splitn(2, ' ').collect();
            if parts.len() == 2 {
                return format!("Result<{}, {}>", rust_type(parts[0]), rust_type(parts[1]));
            }
            return format!("Result<{}>", rust_type(inner));
        }
        _ => return t.to_string(),
    }.to_string()
}

fn rust_trait(t: &str) -> String {
    let mut out = String::new();
    let mut cap = true;
    for ch in t.chars() {
        if cap { out.push(ch.to_ascii_uppercase()); cap = false; }
        else { out.push(ch); }
    }
    out
}

fn is_operator_trait(name: &str) -> bool {
    matches!(name, "add" | "sub" | "mul" | "div" | "rem" | "neg"
        | "Add" | "Sub" | "Mul" | "Div" | "Rem" | "Neg")
}



fn rkyv_suffix(config: &CodegenConfig) -> &'static str {
    if config.rkyv { ", rkyv::Archive, rkyv::Serialize, rkyv::Deserialize" } else { "" }
}

fn qualify(world: &World, name: &str) -> String {
    match name {
        "True" => "true".to_string(),
        "False" => "false".to_string(),
        "_" => "_".to_string(),
        _ => {
            if let Some((domain, _)) = aski_core::query_variant_domain(world, name) {
                format!("{domain}::{name}")
            } else {
                name.to_string()
            }
        }
    }
}

fn qualify_pattern(world: &World, pat_text: &str) -> String {
    // Pattern text may contain "|" for or-patterns, "(..)" for destructure, etc.
    // Split on "|" and qualify each alternative
    if pat_text.contains('|') {
        return pat_text.split('|')
            .map(|p| qualify_single_pattern(world, p.trim()))
            .collect::<Vec<_>>()
            .join(" | ");
    }
    qualify_single_pattern(world, pat_text)
}

fn qualify_single_pattern(world: &World, pat: &str) -> String {
    // Handle data-carrying: "Name(binding)"
    if let Some(idx) = pat.find('(') {
        let name = &pat[..idx];
        let rest = &pat[idx..];
        match name {
            "True" => return format!("true{rest}"),
            "False" => return format!("false{rest}"),
            _ => {
                if let Some((domain, _)) = aski_core::query_variant_domain(world, name) {
                    return format!("{domain}::{name}{rest}");
                }
                return pat.to_string();
            }
        }
    }
    // Handle @binding
    if pat.starts_with('@') {
        return snake(&pat[1..]).to_string();
    }
    // Simple variant name
    match pat {
        "True" => "true".to_string(),
        "False" => "false".to_string(),
        "_" => "_".to_string(),
        name if name.starts_with('"') => name.to_string(),
        name => {
            if let Some((domain, _)) = aski_core::query_variant_domain(world, name) {
                // Check if data-carrying — need (..)
                let variants = aski_core::query_domain_variants(world, &domain);
                let has_data = variants.iter()
                    .any(|(_, vname, wraps)| vname == name && wraps.is_some());
                if has_data {
                    format!("{domain}::{name}(..)")
                } else {
                    format!("{domain}::{name}")
                }
            } else {
                name.to_string()
            }
        }
    }
}

fn is_copy_eligible(type_name: &str, world: &World) -> bool {
    match type_name {
        "U8"|"U16"|"U32"|"U64"|"I8"|"I16"|"I32"|"I64"|"F32"|"F64"|"Bool" => true,
        _ => {
            let variants = aski_core::query_domain_variants(world, type_name);
            !variants.is_empty() && variants.iter().all(|(_, _, w)| w.is_none())
        }
    }
}

fn is_recursive_field(world: &World, struct_name: &str, field_name: &str) -> bool {
    let fields = aski_core::query_struct_fields(world, struct_name);
    for (_, fname, ftype) in &fields {
        if fname != field_name { continue; }
        let inner = ftype.strip_prefix("Vec(").and_then(|s| s.strip_suffix(')'))
            .unwrap_or(ftype);
        if world.recursive_types.iter().any(|r| r.parent_type == struct_name && r.child_type == inner) {
            return true;
        }
    }
    false
}

/// Strip Vec wrapper from a type string (handles both Vec{T} and Vec(T) formats).
fn strip_vec_wrapper(s: &str) -> &str {
    s.strip_prefix("Vec{").and_then(|s| s.strip_suffix('}'))
        .or_else(|| s.strip_prefix("Vec(").and_then(|s| s.strip_suffix(')')))
        .unwrap_or(s)
}

fn compute_no_rkyv_set(world: &World) -> HashSet<String> {
    let mut no_rkyv = HashSet::new();

    // Direct self-reference: struct field type (possibly inside Vec) == struct name
    for t in &world.types {
        if t.form != TypeForm::Struct { continue; }
        let fields = aski_core::query_struct_fields(world, &t.name);
        for (_, _, ftype) in &fields {
            let inner = strip_vec_wrapper(ftype);
            if inner == t.name {
                no_rkyv.insert(t.name.clone());
                break;
            }
        }
    }

    // Kernel-derived transitive recursion
    for rt in &world.recursive_types {
        if rt.parent_type == rt.child_type {
            no_rkyv.insert(rt.parent_type.clone());
        }
    }

    // Propagate: any struct containing a no_rkyv type (transitively)
    loop {
        let mut changed = false;
        for t in &world.types {
            if t.form != TypeForm::Struct || no_rkyv.contains(&t.name) { continue; }
            let fields = aski_core::query_struct_fields(world, &t.name);
            for (_, _, ftype) in &fields {
                let inner = strip_vec_wrapper(ftype);
                if no_rkyv.contains(inner) {
                    no_rkyv.insert(t.name.clone());
                    changed = true;
                    break;
                }
            }
        }
        if !changed { break; }
    }
    no_rkyv
}

// ═══════════════════════════════════════════════════════════════
// ParseNode tree traversal
// ═══════════════════════════════════════════════════════════════

fn children<'a>(world: &'a World, parent_id: i64) -> Vec<&'a ParseNode> {
    aski_core::query_parse_children(world, parent_id)
}

fn find_node<'a>(world: &'a World, id: i64) -> Option<&'a ParseNode> {
    world.parse_nodes.iter().find(|n| n.id == id)
}

fn lookup_ffi<'a>(world: &'a World, aski_name: &str) -> Option<&'a aski_core::FfiEntry> {
    world.ffi_entries.iter().find(|e| e.aski_name == aski_name)
}

/// Walk up from a node to find the containing TypeImpl's target type name.
fn infer_self_type(world: &World, node_id: i64) -> String {
    let mut current = node_id;
    loop {
        let node = match find_node(world, current) {
            Some(n) => n,
            None => break,
        };
        if node.constructor == "TypeImpl" {
            return node.text.clone();
        }
        if node.parent_id < 0 { break; }
        current = node.parent_id;
    }
    "Self".to_string()
}

// ═══════════════════════════════════════════════════════════════
// Domain emission
// ═══════════════════════════════════════════════════════════════

fn emit_domain(out: &mut String, world: &World, node: &ParseNode, config: &CodegenConfig) -> Result<(), String> {
    let name = &node.text;
    let variants = aski_core::query_domain_variants(world, name);
    let has_data = variants.iter().any(|(_, _, wraps)| wraps.is_some());
    let rkyv = rkyv_suffix(config);
    let derives = if has_data {
        format!("#[derive(Default, Debug, Clone, PartialEq, Eq{rkyv})]")
    } else {
        format!("#[derive(Default, Debug, Clone, Copy, PartialEq, Eq{rkyv})]")
    };
    out.push_str(&format!("{derives}\npub enum {name} {{\n"));
    for (i, (_, vname, wraps)) in variants.iter().enumerate() {
        let default_attr = if i == 0 { "#[default]\n    " } else { "" };
        match wraps {
            Some(w) => out.push_str(&format!("    {default_attr}{vname}({}),\n", rust_type(w))),
            None => out.push_str(&format!("    {default_attr}{vname},\n")),
        }
    }
    out.push_str("}\n\n");

    // Display
    out.push_str(&format!("impl std::fmt::Display for {name} {{\n"));
    out.push_str("    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {\n");
    out.push_str("        write!(f, \"{:?}\", self)\n    }\n}\n\n");

    // from_str / to_str only for non-data-carrying
    if !has_data {
        out.push_str(&format!("impl {name} {{\n"));
        out.push_str("    pub fn from_str(s: &str) -> Option<Self> {\n");
        out.push_str("        match s {\n");
        for (_, vname, _) in &variants {
            let sn = snake(vname);
            out.push_str(&format!("            \"{sn}\" => Some(Self::{vname}),\n"));
            if sn != vname.to_lowercase() {
                out.push_str(&format!("            \"{vname}\" => Some(Self::{vname}),\n"));
            }
        }
        out.push_str("            _ => None,\n");
        out.push_str("        }\n    }\n\n");
        out.push_str("    pub fn to_str(&self) -> &'static str {\n");
        out.push_str("        match self {\n");
        for (_, vname, _) in &variants {
            out.push_str(&format!("            Self::{vname} => \"{}\",\n", snake(vname)));
        }
        out.push_str("        }\n    }\n");
        out.push_str("}\n\n");
    }
    Ok(())
}

// ═══════════════════════════════════════════════════════════════
// Struct emission
// ═══════════════════════════════════════════════════════════════

fn emit_struct(out: &mut String, world: &World, node: &ParseNode, config: &CodegenConfig,
               no_rkyv: &HashSet<String>, emitted_accessors: &mut HashSet<String>) -> Result<(), String> {
    let name = &node.text;
    let fields = aski_core::query_struct_fields(world, name);
    let all_copy = !fields.is_empty() && fields.iter().all(|(_, fname, ftype)| {
        !is_recursive_field(world, name, fname) && is_copy_eligible(ftype, world)
    });
    let rkyv = if no_rkyv.contains(name.as_str()) { "" } else { rkyv_suffix(config) };
    let has_float = fields.iter().any(|(_, _, ftype)| matches!(ftype.as_str(), "F32" | "F64"));
    let eq_str = if has_float { "" } else { ", Eq" };
    let derives = if all_copy {
        format!("#[derive(Debug, Copy, Clone, PartialEq{eq_str}{rkyv})]")
    } else {
        format!("#[derive(Debug, Clone, PartialEq{eq_str}{rkyv})]")
    };
    out.push_str(&format!("{derives}\npub struct {name} {{\n"));
    for (_, fname, ftype) in &fields {
        let sname = snake(fname);
        let rt = rust_type(ftype);
        if is_recursive_field(world, name, fname) {
            out.push_str(&format!("    pub {sname}: Box<{rt}>,\n"));
        } else {
            out.push_str(&format!("    pub {sname}: {rt},\n"));
        }
    }
    out.push_str("}\n\n");

    // Vec fields → accessor methods
    let vec_fields: Vec<(String, String)> = fields.iter()
        .filter_map(|(_, fname, ftype)| {
            ftype.strip_prefix("Vec(").and_then(|s| s.strip_suffix(')'))
                .map(|elem| (snake(fname), elem.to_string()))
        })
        .collect();
    if !vec_fields.is_empty() {
        out.push_str(&format!("impl Default for {name} {{ fn default() -> Self {{ Self {{"));
        for (_, fname, _) in &fields {
            out.push_str(&format!(" {}: Default::default(),", snake(fname)));
        }
        out.push_str(" } } }\n\n");
        out.push_str(&format!("impl {name} {{\n"));
        out.push_str("    pub fn new() -> Self { Self::default() }\n\n");
        for (vec_field, elem_type) in &vec_fields {
            let elem_fields = aski_core::query_struct_fields(world, elem_type);
            if elem_fields.is_empty() { continue; }
            let type_snake = snake(elem_type);
            for (_, ef_name, ef_type) in &elem_fields {
                let ef_snake = snake(ef_name);
                let method_key = format!("{name}::{type_snake}_by_{ef_snake}");
                if emitted_accessors.contains(&method_key) { continue; }
                emitted_accessors.insert(method_key);
                let rt = rust_type(ef_type);
                let param_t = match rt.as_str() {
                    "String" => "&str",
                    other => other,
                };
                out.push_str(&format!(
                    "    pub fn {type_snake}_by_{ef_snake}(&self, val: {param_t}) -> Vec<&{elem_type}> {{\n"
                ));
                out.push_str(&format!(
                    "        self.{vec_field}.iter().filter(|r| r.{ef_snake} == val).collect()\n"
                ));
                out.push_str("    }\n\n");
            }
        }
        out.push_str("}\n\n");
    }
    Ok(())
}

// ═══════════════════════════════════════════════════════════════
// Constant emission
// ═══════════════════════════════════════════════════════════════

fn emit_const(out: &mut String, world: &World, node: &ParseNode) -> Result<(), String> {
    let name = &node.text;
    let ch = children(world, node.id);
    // First child is TypeRef, second (if present) is the body
    let type_ref = ch.iter().find(|c| c.constructor == "TypeRef")
        .map(|c| c.text.clone())
        .unwrap_or("()".to_string());
    let body_node = ch.iter().find(|c| c.constructor == "Block" || c.constructor == "TailBlock");
    if let Some(body) = body_node {
        let body_children = children(world, body.id);
        if let Some(val_node) = body_children.first() {
            let val = emit_expr(world, val_node)?;
            out.push_str(&format!("pub const {}: {} = {val};\n\n", screaming(name), rust_type(&type_ref)));
        }
    }
    Ok(())
}

// ═══════════════════════════════════════════════════════════════
// Type alias emission
// ═══════════════════════════════════════════════════════════════

fn emit_type_alias(out: &mut String, world: &World, node: &ParseNode) -> Result<(), String> {
    let name = &node.text;
    // Skip parameter-name aliases (lowercase) — only emit actual PascalCase type aliases
    if name.is_empty() || !name.chars().next().unwrap_or('a').is_uppercase() {
        return Ok(());
    }
    let ch = children(world, node.id);
    if let Some(target) = ch.first() {
        out.push_str(&format!("pub type {name} = {};\n\n", rust_type(&target.text)));
    }
    Ok(())
}

// ═══════════════════════════════════════════════════════════════
// Trait declaration emission
// ═══════════════════════════════════════════════════════════════

fn emit_trait_decl(out: &mut String, world: &World, node: &ParseNode) -> Result<(), String> {
    let name = &node.text;
    let ch = children(world, node.id);

    // Supertraits
    let supers: Vec<&str> = ch.iter()
        .filter(|c| c.constructor == "Supertrait")
        .map(|c| c.text.as_str())
        .collect();
    let super_str = if supers.is_empty() { String::new() }
        else { format!(": {}", supers.iter().map(|s| rust_trait(s)).collect::<Vec<_>>().join(" + ")) };

    out.push_str(&format!("pub trait {}{super_str} {{\n", rust_trait(name)));
    for child in &ch {
        if child.constructor == "MethodSig" {
            emit_method_sig(out, world, child)?;
        }
    }
    out.push_str("}\n\n");
    Ok(())
}

fn emit_method_sig(out: &mut String, world: &World, node: &ParseNode) -> Result<(), String> {
    let name = &node.text;
    let ch = children(world, node.id);
    let params = emit_params(world, &ch);
    let ret = ch.iter().find(|c| c.constructor == "ReturnType")
        .map(|c| format!(" -> {}", rust_type(&c.text)))
        .unwrap_or_default();
    out.push_str(&format!("    fn {}({}){ret};\n", snake(name), params));
    Ok(())
}

// ═══════════════════════════════════════════════════════════════
// Trait impl emission
// ═══════════════════════════════════════════════════════════════

fn emit_trait_impl(out: &mut String, world: &World, node: &ParseNode) -> Result<(), String> {
    let trait_name = &node.text;
    let type_impls = children(world, node.id);

    for type_impl in &type_impls {
        if type_impl.constructor != "TypeImpl" { continue; }
        let target_type = &type_impl.text;
        let is_operator = is_operator_trait(trait_name);
        let is_derive = trait_name == "derive";

        if is_derive {
            out.push_str(&format!("impl {target_type} {{\n"));
            emit_derive_impl(out, world, type_impl, target_type)?;
            out.push_str("}\n\n");
            continue;
        }

        if is_operator {
            out.push_str(&format!("impl {} for {target_type} {{\n", rust_trait(trait_name)));
            // Find the method to get return type for Output
            let methods = children(world, type_impl.id);
            let ret = methods.iter()
                .find(|m| m.constructor == "MethodDef" || m.constructor == "TailMethodDef")
                .and_then(|m| {
                    children(world, m.id).iter()
                        .find(|c| c.constructor == "ReturnType")
                        .map(|c| c.text.clone())
                });
            let output = ret.unwrap_or_else(|| target_type.clone());
            out.push_str(&format!("    type Output = {output};\n"));
        } else {
            out.push_str(&format!("impl {} for {target_type} {{\n", rust_trait(trait_name)));
        }

        let methods = children(world, type_impl.id);
        for method in &methods {
            if method.constructor != "MethodDef" && method.constructor != "TailMethodDef" { continue; }
            emit_method(out, world, method, is_operator)?;
        }
        out.push_str("}\n\n");
    }
    Ok(())
}

fn emit_method(out: &mut String, world: &World, node: &ParseNode, is_operator: bool) -> Result<(), String> {
    let name = &node.text;
    let ch = children(world, node.id);

    let params = if is_operator {
        emit_params_operator(world, &ch)
    } else {
        emit_params(world, &ch)
    };
    let ret = ch.iter().find(|c| c.constructor == "ReturnType")
        .map(|c| format!(" -> {}", rust_type(&c.text)))
        .unwrap_or_default();

    out.push_str(&format!("    fn {}({}){ret} {{\n", snake(name), params));

    // Find the body node (Block, TailBlock, MatchBody, or Stub)
    let body = ch.iter().find(|c| matches!(c.constructor.as_str(),
        "Block" | "TailBlock" | "MatchBody" | "Stub"));

    if let Some(body_node) = body {
        match body_node.constructor.as_str() {
            "MatchBody" => emit_match_body(out, world, body_node)?,
            "Block" => {
                let stmts = children(world, body_node.id);
                emit_block(out, world, &stmts, "        ")?;
            }
            "TailBlock" => {
                let stmts = children(world, body_node.id);
                emit_block(out, world, &stmts, "        ")?;
            }
            "Stub" => out.push_str("        todo!()\n"),
            _ => {}
        }
    }
    out.push_str("    }\n");
    Ok(())
}

// ═══════════════════════════════════════════════════════════════
// Main emission
// ═══════════════════════════════════════════════════════════════

fn emit_main(out: &mut String, world: &World, node: &ParseNode) -> Result<(), String> {
    out.push_str("pub fn main() {\n");
    let ch = children(world, node.id);
    if let Some(body) = ch.first() {
        let stmts = children(world, body.id);
        emit_block(out, world, &stmts, "    ")?;
    }
    out.push_str("}\n");
    Ok(())
}

// ═══════════════════════════════════════════════════════════════
// Foreign block emission
// ═══════════════════════════════════════════════════════════════

fn emit_foreign_block(out: &mut String, world: &World, node: &ParseNode) -> Result<(), String> {
    let library = &node.text;
    let crate_name = snake(library);
    let funcs = children(world, node.id);
    for func in &funcs {
        if func.constructor != "ForeignFunc" { continue; }
        let ffi = world.ffi_entries.iter().find(|e| e.aski_name == func.text && e.library == *library);
        let ret = ffi.map(|f| f.return_type.clone()).unwrap_or("()".to_string());
        let func_children = children(world, func.id);
        let param_nodes: Vec<&ParseNode> = func_children.iter()
            .filter(|c| is_param_node(c))
            .copied()
            .collect();
        let (param_strs, arg_names) = emit_ffi_params(world, &param_nodes);
        let rust_name = ffi.map(|f| f.rust_name.clone()).unwrap_or_else(|| func.text.clone());
        out.push_str(&format!("pub fn {}({}) -> {} {{\n",
            snake(&func.text), param_strs, rust_type(&ret)));
        out.push_str(&format!("    {crate_name}::{}({})\n", rust_name, arg_names));
        out.push_str("}\n\n");
    }
    Ok(())
}

// ═══════════════════════════════════════════════════════════════
// Param emission
// ═══════════════════════════════════════════════════════════════

fn is_param_node(node: &ParseNode) -> bool {
    matches!(node.constructor.as_str(),
        "BorrowSelf" | "MutBorrowSelf" | "OwnedSelf" | "OwnedParam"
        | "NamedParam" | "BorrowParam" | "MutBorrowParam")
}

fn emit_params(world: &World, nodes: &[&ParseNode]) -> String {
    nodes.iter()
        .filter(|n| is_param_node(n))
        .filter_map(|n| emit_single_param(world, n))
        .collect::<Vec<_>>()
        .join(", ")
}

fn emit_single_param(world: &World, node: &ParseNode) -> Option<String> {
    match node.constructor.as_str() {
        "BorrowSelf" => Some("&self".into()),
        "MutBorrowSelf" => Some("&mut self".into()),
        "OwnedSelf" => Some("self".into()),
        "OwnedParam" => {
            let rt = rust_type(&node.text);
            Some(format!("{}: {rt}", snake(&node.text)))
        }
        "NamedParam" => {
            let ch = children(world, node.id);
            let type_ref = ch.iter().find(|c| c.constructor == "TypeRef")?;
            let rt = rust_type(&type_ref.text);
            Some(format!("{}: {rt}", snake(&node.text)))
        }
        "BorrowParam" => {
            let rt = rust_type(&node.text);
            Some(format!("{}: {rt}", snake(&node.text)))
        }
        "MutBorrowParam" => {
            let rt = rust_type(&node.text);
            Some(format!("{}: {rt}", snake(&node.text)))
        }
        _ => None,
    }
}

fn emit_params_operator(world: &World, nodes: &[&ParseNode]) -> String {
    nodes.iter()
        .filter(|n| is_param_node(n))
        .filter_map(|n| match n.constructor.as_str() {
            "BorrowSelf" | "MutBorrowSelf" | "OwnedSelf" => Some("self".into()),
            "NamedParam" => {
                let ch = children(world, n.id);
                let type_ref = ch.iter().find(|c| c.constructor == "TypeRef")?;
                Some(format!("{}: {}", snake(&n.text), rust_type(&type_ref.text)))
            }
            "OwnedParam" | "BorrowParam" => {
                Some(format!("{}: {}", snake(&n.text), rust_type(&n.text)))
            }
            _ => None,
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn emit_ffi_params(world: &World, nodes: &[&ParseNode]) -> (String, String) {
    let mut params = Vec::new();
    let mut args = Vec::new();
    for node in nodes {
        match node.constructor.as_str() {
            "NamedParam" => {
                let ch = children(world, node.id);
                if let Some(type_ref) = ch.iter().find(|c| c.constructor == "TypeRef") {
                    let n = snake(&node.text);
                    params.push(format!("{n}: {}", rust_type(&type_ref.text)));
                    args.push(n);
                }
            }
            "OwnedParam" | "BorrowParam" => {
                let n = snake(&node.text);
                params.push(format!("{n}: {}", rust_type(&node.text)));
                args.push(n);
            }
            _ => {}
        }
    }
    (params.join(", "), args.join(", "))
}

// ═══════════════════════════════════════════════════════════════
// Match body emission (method-level match on self)
// ═══════════════════════════════════════════════════════════════

fn emit_match_body(out: &mut String, world: &World, node: &ParseNode) -> Result<(), String> {
    let arms = children(world, node.id);
    out.push_str("        match self {\n");
    for arm in &arms {
        let pat = qualify_pattern(world, &arm.text);
        let body = children(world, arm.id);
        if let Some(expr_node) = body.first() {
            let body_str = emit_expr(world, expr_node)?;
            out.push_str(&format!("            {pat} => {body_str},\n"));
        }
    }
    out.push_str("        }\n");
    Ok(())
}

// ═══════════════════════════════════════════════════════════════
// Block emission (sequence of statements)
// ═══════════════════════════════════════════════════════════════

fn emit_block(out: &mut String, world: &World, stmts: &[&ParseNode], indent: &str) -> Result<(), String> {
    for (i, node) in stmts.iter().enumerate() {
        let _is_last = i == stmts.len() - 1;
        match node.constructor.as_str() {
            "SameTypeNew" | "SubTypeNew" => {
                let (var_name, type_name) = split_binding(&node.text);
                let ch = children(world, node.id);
                let svar = snake(&var_name);

                let type_kind = world.type_kinds.iter()
                    .find(|tk| tk.type_name == type_name);
                let is_struct = type_kind.map(|tk| tk.category == TypeForm::Struct).unwrap_or(false);

                if is_struct {
                    let val = emit_struct_or_expr(world, &type_name, &ch)?;
                    out.push_str(&format!("{indent}let {svar}: {type_name} = {val};\n"));
                } else if let Some(val_node) = ch.first() {
                    let val = emit_expr(world, val_node)?;
                    let val = if type_name == "String" && val_node.constructor == "StringLit" {
                        format!("{val}.to_string()")
                    } else { val };
                    out.push_str(&format!("{indent}let {svar}: {} = {val};\n", rust_type(&type_name)));
                }
            }
            "MutableNew" => {
                let (var_name, type_name) = split_binding(&node.text);
                let ch = children(world, node.id);
                let svar = snake(&var_name);
                if let Some(val_node) = ch.first() {
                    let val = emit_expr(world, val_node)?;
                    let val = if val == "\"\"" && type_name == "String" {
                        "String::new()".to_string()
                    } else { val };
                    out.push_str(&format!("{indent}let mut {svar}: {} = {val};\n", rust_type(&type_name)));
                }
            }
            "MutableSet" => {
                let svar = snake(&node.text);
                let ch = children(world, node.id);
                if let Some(val_node) = ch.first() {
                    if svar == "self" && val_node.constructor == "MethodCall" {
                        // ~@Self.Field.set(value) → self.field = value;
                        let mc_children = children(world, val_node.id);
                        if mc_children.len() > 1 {
                            let field_chain = emit_expr(world, mc_children[0])?;
                            let val = emit_expr(world, mc_children[1])?;
                            out.push_str(&format!("{indent}{field_chain} = {val};\n"));
                        } else {
                            let val = emit_expr(world, val_node)?;
                            out.push_str(&format!("{indent}{svar} = {val};\n"));
                        }
                    } else {
                        let val = emit_expr(world, val_node)?;
                        out.push_str(&format!("{indent}{svar} = {val};\n"));
                    }
                }
            }
            "SubTypeDecl" => {
                // Type declaration only, no value — used for forward references
                let (var_name, type_name) = split_binding(&node.text);
                out.push_str(&format!("{indent}let {}: {};\n", snake(&var_name), rust_type(&type_name)));
            }
            "StdOut" => {
                let ch = children(world, node.id);
                if let Some(val_node) = ch.first() {
                    let val = emit_expr(world, val_node)?;
                    out.push_str(&format!("{indent}println!(\"{{}}\", {val});\n"));
                }
            }
            "Yield" => {
                let ch = children(world, node.id);
                if let Some(val_node) = ch.first() {
                    if val_node.constructor == "MethodCall" {
                        let mc_children = children(world, val_node.id);
                        if let Some(base_node) = mc_children.first() {
                            let collection = emit_expr(world, base_node)?;
                            let method_name = &val_node.text;
                            if mc_children.len() > 1 {
                                let loop_var = snake(method_name);
                                out.push_str(&format!("{indent}for {loop_var} in {collection}.iter() {{\n"));
                                let body_children = children(world, mc_children[1].id);
                                if body_children.is_empty() {
                                    let body = emit_expr(world, mc_children[1])?;
                                    out.push_str(&format!("{indent}    {body};\n"));
                                } else {
                                    let refs: Vec<&ParseNode> = body_children.iter().copied().collect();
                                    emit_block(out, world, &refs, &format!("{indent}    "))?;
                                }
                                out.push_str(&format!("{indent}}}\n"));
                            } else {
                                let val = emit_expr(world, val_node)?;
                                out.push_str(&format!("{indent}{val};\n"));
                            }
                        }
                    } else {
                        let val = emit_expr(world, val_node)?;
                        out.push_str(&format!("{indent}{val};\n"));
                    }
                }
            }
            "Return" => {
                let ch = children(world, node.id);
                if let Some(val_node) = ch.first() {
                    let val = emit_expr(world, val_node)?;
                    out.push_str(&format!("{indent}{val}\n"));
                }
            }
            "ErrorProp" => {
                let ch = children(world, node.id);
                if let Some(val_node) = ch.first() {
                    let val = emit_expr(world, val_node)?;
                    out.push_str(&format!("{indent}{val}?;\n"));
                }
            }
            _ => {
                let val = emit_expr(world, node)?;
                if !val.is_empty() {
                    out.push_str(&format!("{indent}{val};\n"));
                }
            }
        }
    }
    Ok(())
}

/// Split "varName:TypeName" → (varName, TypeName).
/// If no colon, the text is just the name (SameTypeNew).
fn split_binding(text: &str) -> (String, String) {
    if let Some(idx) = text.find(':') {
        (text[..idx].to_string(), text[idx+1..].to_string())
    } else {
        (text.to_string(), text.to_string())
    }
}

// ═══════════════════════════════════════════════════════════════
// Expression emission
// ═══════════════════════════════════════════════════════════════

fn emit_expr(world: &World, node: &ParseNode) -> Result<String, String> {
    match node.constructor.as_str() {
        "IntLit" => Ok(node.text.clone()),
        "FloatLit" => Ok(node.text.clone()),
        "StringLit" => {
            if node.text.is_empty() {
                Ok("String::new()".into())
            } else {
                Ok(format!("\"{}\"", node.text))
            }
        }
        "ConstRef" => Ok(screaming(&node.text)),
        "InstanceRef" => {
            if node.text == "Self" { Ok("self".into()) }
            else { Ok(snake(&node.text)) }
        }
        "BareName" => {
            let name = &node.text;
            // Handle Type::Variant paths
            if name.contains("::") {
                return Ok(name.clone());
            }
            Ok(qualify(world, name))
        }
        "Stub" => Ok("todo!()".into()),

        "BinOp" => {
            let ch = children(world, node.id);
            let op = &node.text;
            if ch.len() >= 2 {
                let l = emit_expr(world, ch[0])?;
                let r = emit_expr(world, ch[1])?;
                if op == "+" {
                    let rk = &ch[1].constructor;
                    let needs_ref = matches!(rk.as_str(),
                        "Access" | "MethodCall" | "InstanceRef" | "SameTypeNew" |
                        "SubTypeNew" | "InlineEval" | "BinOp" | "BareName");
                    if needs_ref {
                        Ok(format!("({l} + &{r})"))
                    } else {
                        Ok(format!("({l} + {r})"))
                    }
                } else {
                    Ok(format!("({l} {op} {r})"))
                }
            } else { Ok("todo!()".into()) }
        }

        "Access" => {
            let name = &node.text;
            let ch = children(world, node.id);
            if let Some(base_node) = ch.first() {
                let base = emit_expr(world, base_node)?;
                let s = snake(name);
                // FFI lookup
                if let Some(ffi) = lookup_ffi(world, name) {
                    return match ffi.span {
                        RustSpan::Cast => Ok(format!("({base} as {})", rust_type(&ffi.return_type))),
                        RustSpan::MethodCall if ffi.rust_name == "len" => Ok(format!("({base}.len() as u32)")),
                        RustSpan::MethodCall => Ok(format!("{base}.{}()", ffi.rust_name)),
                        RustSpan::FreeCall => Ok(format!("{}(&{base})", ffi.rust_name)),
                        RustSpan::BlockExpr => Ok(format!("{base}.{s}()")),
                        RustSpan::IndexAccess => Ok(format!("{base}.{}()", ffi.rust_name)),
                    };
                }
                if s == "len" { return Ok(format!("({base}.len() as u32)")); }
                if name.chars().next().map(|c| c.is_lowercase()).unwrap_or(false) {
                    Ok(format!("{base}.{s}()"))
                } else if base.chars().next().map(|c| c.is_uppercase()).unwrap_or(false) && !base.contains('.') {
                    Ok(format!("{base}::{name}"))
                } else {
                    Ok(format!("{base}.{s}"))
                }
            } else {
                Ok(format!("self.{}", snake(name)))
            }
        }

        "MethodCall" => {
            let method = &node.text;
            let ch = children(world, node.id);
            let base = if ch.is_empty() { "self".into() }
                       else { emit_expr(world, ch[0])? };
            let s = snake(method);

            // FFI lookup
            if let Some(ffi) = lookup_ffi(world, method) {
                return match ffi.span {
                    RustSpan::Cast => Ok(format!("({base} as {})", rust_type(&ffi.return_type))),
                    RustSpan::MethodCall => Ok(format!("{base}.{}()", ffi.rust_name)),
                    RustSpan::FreeCall => Ok(format!("{}(&{base})", ffi.rust_name)),
                    RustSpan::BlockExpr => {
                        let arg = if ch.len() > 1 { emit_expr(world, ch[1])? } else { "todo!()".into() };
                        Ok(format!("{{ let mut v = {base}; v.push({arg}); v }}"))
                    }
                    RustSpan::IndexAccess => Ok(format!("{base}.{}()", ffi.rust_name)),
                };
            }

            // Legacy casts
            if method == "toF32" { return Ok(format!("({base} as f32)")); }
            if method == "toU32" { return Ok(format!("({base} as u32)")); }
            if method == "toI64" { return Ok(format!("({base} as i64)")); }

            // .with(Field(value)) → struct update syntax
            if method == "with" {
                let mut field_inits = Vec::new();
                for fc in ch.iter().skip(1) {
                    if fc.constructor == "StructConstruct" {
                        let fname = snake(&fc.text);
                        let inner = children(world, fc.id);
                        if let Some(val_node) = inner.first() {
                            let gc = children(world, val_node.id);
                            if let Some(gn) = gc.first() {
                                field_inits.push(format!("{fname}: {}", emit_expr(world, gn)?));
                            } else {
                                field_inits.push(format!("{fname}: {}", emit_expr(world, val_node)?));
                            }
                        }
                    } else {
                        let v = emit_expr(world, fc)?;
                        field_inits.push(v);
                    }
                }
                if field_inits.is_empty() {
                    return Ok(format!("{base}.clone()"));
                }
                let type_name = infer_self_type(world, node.id);
                return Ok(format!("{type_name} {{ {}, ..{base}.clone() }}", field_inits.join(", ")));
            }

            // Vec::get
            if s == "get" && ch.len() == 2 {
                let arg = emit_expr(world, ch[1])?;
                return Ok(format!("{base}.get({arg} as usize).unwrap().clone()"));
            }
            // Vec::len
            if s == "len" && ch.len() == 1 {
                return Ok(format!("({base}.len() as u32)"));
            }

            // Auto-borrow for accessor methods
            let is_accessor = s.contains("_by_") && (base == "self" || base.starts_with("self."));
            let args: Vec<String> = ch.iter().skip(1)
                .map(|c| {
                    let v = emit_expr(world, c)?;
                    if is_accessor && matches!(c.constructor.as_str(), "InstanceRef" | "Access") {
                        let is_numeric = v.parse::<i64>().is_ok() || v.contains(".id") || v.contains(".type_id") || v.contains(".ordinal");
                        if is_numeric { Ok(v) } else { Ok(format!("&{v}")) }
                    } else {
                        Ok(v)
                    }
                })
                .collect::<Result<Vec<_>, String>>()?;
            if args.is_empty() { Ok(format!("{base}.{s}()")) }
            else { Ok(format!("{base}.{s}({})", args.join(", "))) }
        }

        "FnCall" => {
            let name = &node.text;
            let ch = children(world, node.id);
            let args: Vec<String> = ch.iter()
                .map(|c| emit_expr(world, c))
                .collect::<Result<_, _>>()?;
            Ok(format!("{}({})", snake(name), args.join(", ")))
        }

        "StructConstruct" => {
            let name = &node.text;
            let ch = children(world, node.id);

            // Is this a domain variant construction?
            if aski_core::query_variant_domain(world, name).is_some() {
                if ch.is_empty() { return Ok(qualify(world, name)); }
                if ch.len() == 1 {
                    let inner = emit_expr(world, ch[0])?;
                    return Ok(format!("{}({inner})", qualify(world, name)));
                }
            }

            // Regular struct construction
            let mut fields_out = Vec::new();
            let struct_fields = aski_core::query_struct_fields(world, name);
            for fc in &ch {
                if fc.constructor != "StructField" { continue; }
                let fname = snake(&fc.text);
                let inner = children(world, fc.id);
                let field_type = struct_fields.iter()
                    .find(|(_, fn_, _)| snake(fn_) == fname)
                    .map(|(_, _, ft)| ft.as_str());
                let is_vec = field_type.map(|ft| ft.starts_with("Vec")).unwrap_or(false);
                let is_string = field_type == Some("String");

                if inner.is_empty() {
                    if is_vec { fields_out.push(format!("{fname}: Vec::new()")); }
                    else if is_string { fields_out.push(format!("{fname}: String::new()")); }
                    else { fields_out.push(format!("{fname}: Default::default()")); }
                } else if let Some(val_node) = inner.first() {
                    let val = emit_expr(world, val_node)?;
                    let val = if is_vec {
                        if val == "String::new()" || val.starts_with("Vec::") || val.starts_with("vec![")
                            || val.contains(".with_push(") {
                            val
                        } else if matches!(val_node.constructor.as_str(), "Access" | "MethodCall") {
                            format!("{val}.clone()")
                        } else if val_node.constructor == "InstanceRef" {
                            format!("vec![{val}]")
                        } else {
                            format!("vec![{val}]")
                        }
                    } else if is_string {
                        if val == "\"\"" { "String::new()".to_string() }
                        else if matches!(val_node.constructor.as_str(), "InstanceRef" | "Access") {
                            format!("{val}.clone()")
                        } else { val }
                    } else { val };
                    fields_out.push(format!("{fname}: {val}"));
                }
            }
            Ok(format!("{name} {{ {} }}", fields_out.join(", ")))
        }

        "StructField" => {
            let fname = snake(&node.text);
            let ch = children(world, node.id);
            if let Some(val_node) = ch.first() {
                let val = emit_expr(world, val_node)?;
                Ok(format!("{fname}: {val}"))
            } else {
                Ok(format!("{fname}: todo!()"))
            }
        }

        "Group" => {
            let ch = children(world, node.id);
            ch.first().map(|c| emit_expr(world, c).map(|v| format!("({v})")))
                .unwrap_or(Ok("()".into()))
        }

        "Return" => {
            let ch = children(world, node.id);
            ch.first().map(|c| emit_expr(world, c))
                .unwrap_or(Ok("()".into()))
        }

        "InlineEval" => {
            let ch = children(world, node.id);
            if ch.len() == 1 {
                emit_expr(world, ch[0])
            } else if ch.len() > 1 {
                let mut parts = Vec::new();
                for (i, c) in ch.iter().enumerate() {
                    if i == ch.len() - 1 {
                        parts.push(emit_expr(world, c)?);
                    } else {
                        match c.constructor.as_str() {
                            "SameTypeNew" | "SubTypeNew" => {
                                let (var_name, type_name) = split_binding(&c.text);
                                let svar = snake(&var_name);
                                let inner = children(world, c.id);
                                if let Some(val_node) = inner.first() {
                                    let val = emit_expr(world, val_node)?;
                                    parts.push(format!("let mut {svar}: {} = {val}", rust_type(&type_name)));
                                }
                            }
                            "MutableNew" => {
                                let (var_name, type_name) = split_binding(&c.text);
                                let svar = snake(&var_name);
                                let inner = children(world, c.id);
                                if let Some(val_node) = inner.first() {
                                    let val = emit_expr(world, val_node)?;
                                    parts.push(format!("let mut {svar}: {} = {val}", rust_type(&type_name)));
                                }
                            }
                            "MutableSet" => {
                                let svar = snake(&c.text);
                                let inner = children(world, c.id);
                                if let Some(val_node) = inner.first() {
                                    let val = emit_expr(world, val_node)?;
                                    parts.push(format!("{svar} = {val}"));
                                }
                            }
                            _ => {
                                let val = emit_expr(world, c)?;
                                if !val.is_empty() { parts.push(val); }
                            }
                        }
                    }
                }
                if parts.len() == 1 {
                    Ok(parts.into_iter().next().unwrap())
                } else {
                    let last = parts.pop().unwrap_or_default();
                    let stmts = parts.join("; ");
                    Ok(format!("{{ {stmts}; {last} }}"))
                }
            } else {
                Ok(String::new())
            }
        }

        "Match" => {
            let ch = children(world, node.id);
            // Children: targets first, then CommitArm nodes
            let targets: Vec<&ParseNode> = ch.iter()
                .take_while(|c| c.constructor != "CommitArm" && c.constructor != "BacktrackArm")
                .copied()
                .collect();
            let arms: Vec<&ParseNode> = ch.iter()
                .filter(|c| c.constructor == "CommitArm" || c.constructor == "BacktrackArm" || c.constructor == "DestructureArm")
                .copied()
                .collect();

            if let Some(target_node) = targets.first() {
                let target = emit_expr(world, target_node)?;
                let arm_strs: Vec<String> = arms.iter().filter_map(|arm| {
                    let pat = qualify_pattern(world, &arm.text);
                    let body = children(world, arm.id);
                    body.first().map(|b| {
                        emit_expr(world, b).ok().map(|body_str| {
                            let body_str = if body_str.starts_with('"') && body_str.ends_with('"') {
                                format!("{body_str}.to_string()")
                            } else { body_str };
                            format!("{pat} => {body_str}")
                        })
                    }).flatten()
                }).collect();
                let has_str_pat = arm_strs.iter().any(|a| a.starts_with('"'));
                let target_expr = if has_str_pat { format!("{target}.as_str()") } else { target };
                Ok(format!("match {target_expr} {{ {} }}", arm_strs.join(", ")))
            } else { Ok("todo!()".into()) }
        }

        "Yield" => {
            let ch = children(world, node.id);
            ch.first().map(|c| emit_expr(world, c))
                .unwrap_or(Ok("()".into()))
        }

        "ErrorProp" => {
            let ch = children(world, node.id);
            ch.first().map(|c| emit_expr(world, c).map(|v| format!("{v}?")))
                .unwrap_or(Ok("()".into()))
        }

        "StdOut" => {
            let ch = children(world, node.id);
            ch.first().map(|c| emit_expr(world, c).map(|v| format!("println!(\"{{}}\", {v})")))
                .unwrap_or(Ok("()".into()))
        }

        "SameTypeNew" | "SubTypeNew" => {
            // When used as an expression (not a statement)
            let (_var_name, type_name) = split_binding(&node.text);
            let ch = children(world, node.id);
            if let Some(val_node) = ch.first() {
                emit_expr(world, val_node)
            } else {
                Ok(format!("{}::default()", rust_type(&type_name)))
            }
        }

        "DeferredNew" => {
            let ch = children(world, node.id);
            if let Some(val_node) = ch.first() {
                emit_expr(world, val_node)
            } else {
                Ok("Default::default()".into())
            }
        }

        "RangeInclusive" => {
            let ch = children(world, node.id);
            if ch.len() >= 2 {
                let start = emit_expr(world, ch[0])?;
                let end = emit_expr(world, ch[1])?;
                Ok(format!("{start}..={end}"))
            } else { Ok("todo!()".into()) }
        }

        "RangeExclusive" => {
            let ch = children(world, node.id);
            if ch.len() >= 2 {
                let start = emit_expr(world, ch[0])?;
                let end = emit_expr(world, ch[1])?;
                Ok(format!("{start}..{end}"))
            } else { Ok("todo!()".into()) }
        }

        other => Ok(format!("/* unhandled: {other} */")),
    }
}

fn emit_struct_or_expr(world: &World, type_name: &str, ch: &[&ParseNode]) -> Result<String, String> {
    let mut fields = Vec::new();
    for c in ch {
        if c.constructor == "StructField" {
            let fname = snake(&c.text);
            let inner = children(world, c.id);
            if let Some(val_node) = inner.first() {
                if val_node.constructor == "StructField" {
                    let gc = children(world, val_node.id);
                    if let Some(gn) = gc.first() {
                        fields.push(format!("{fname}: {}", emit_expr(world, gn)?));
                    }
                } else {
                    fields.push(format!("{fname}: {}", emit_expr(world, val_node)?));
                }
            }
        } else {
            return emit_expr(world, c);
        }
    }
    if fields.is_empty() { Ok(format!("{type_name} {{}}")) }
    else { Ok(format!("{type_name} {{ {} }}", fields.join(", "))) }
}

// ═══════════════════════════════════════════════════════════════
// Derive rule emission — collection pipelines → nested for-loops
// ═══════════════════════════════════════════════════════════════

struct DeriveBinding {
    var_name: String,
    type_name: String,
}

enum Pipeline {
    Map { source: Source, constructor_node: i64 },
    FlatMap { source: Source, inner: Box<Pipeline> },
    Chain { left: Box<Pipeline>, right: Box<Pipeline> },
}

struct Source {
    collection: String,
    filter_ids: Vec<i64>,
}

fn emit_derive_impl(out: &mut String, world: &World, type_impl: &ParseNode, _target_type: &str) -> Result<(), String> {
    let methods = children(world, type_impl.id);
    let mut method_list: Vec<&ParseNode> = methods.iter()
        .filter(|m| m.constructor == "MethodDef" || m.constructor == "TailMethodDef")
        .copied()
        .collect();
    method_list.sort_by_key(|m| m.id);

    // derive() dispatcher
    let derive_method = method_list.iter()
        .find(|m| m.text == "derive")
        .ok_or("no derive() method in derive impl")?;

    out.push_str("    pub fn derive(&mut self) {\n");
    let body = derive_body_node(world, derive_method);
    let body_stmts = body.map(|b| children(world, b.id)).unwrap_or_default();

    for stmt in &body_stmts {
        let method_name = extract_derive_method_call(world, stmt);
        if let Some(name) = method_name {
            let is_fp = method_list.iter().any(|m| m.text == name && m.constructor == "TailMethodDef");
            if is_fp {
                out.push_str(&format!("        self.{}_fixpoint();\n", snake(&name)));
            } else {
                out.push_str(&format!("        self.{}();\n", snake(&name)));
            }
        }
    }
    out.push_str("    }\n\n");

    // Each derive method
    for method in &method_list {
        if method.text == "derive" { continue; }
        let rn = snake(&method.text);
        if method.constructor == "TailMethodDef" {
            emit_fixpoint_method(out, world, method, &rn)?;
        } else {
            emit_simple_derive_method(out, world, method, &rn)?;
        }
    }
    Ok(())
}

fn derive_body_node<'a>(world: &'a World, method: &ParseNode) -> Option<&'a ParseNode> {
    let ch = children(world, method.id);
    ch.into_iter().find(|c| matches!(c.constructor.as_str(), "Block" | "TailBlock"))
}

fn extract_derive_method_call(world: &World, node: &ParseNode) -> Option<String> {
    if node.constructor == "MethodCall" {
        return Some(node.text.clone());
    }
    if node.constructor == "MutableSet" {
        let ch = children(world, node.id);
        if let Some(first) = ch.first() {
            if first.constructor == "MethodCall" {
                return Some(first.text.clone());
            }
            // Access(pipeline) → walk into Access to find MethodCall
            if first.constructor == "Access" {
                let inner = children(world, first.id);
                if let Some(mc) = inner.first() {
                    if mc.constructor == "MethodCall" {
                        return Some(mc.text.clone());
                    }
                }
            }
        }
    }
    None
}

fn emit_simple_derive_method(out: &mut String, world: &World, method: &ParseNode, rn: &str) -> Result<(), String> {
    out.push_str(&format!("    fn {}(&mut self) {{\n", rn));
    out.push_str("        let mut results = Vec::new();\n");
    let body = derive_body_node(world, method);
    let body_stmts = body.map(|b| children(world, b.id)).unwrap_or_default();
    let stmt = body_stmts.first().ok_or(format!("empty body in {}", method.text))?;
    let field_name = extract_set_field(world, stmt);
    let pipeline_id = find_pipeline_root(world, stmt)?;
    let pipeline = decompose_pipeline(world, pipeline_id)?;
    let mut bindings = Vec::new();
    emit_pipeline_loops(out, world, &pipeline, "results", 2, &mut bindings)?;
    out.push_str(&format!("        self.{} = results;\n", field_name));
    out.push_str("    }\n\n");
    Ok(())
}

fn emit_fixpoint_method(out: &mut String, world: &World, method: &ParseNode, rn: &str) -> Result<(), String> {
    out.push_str(&format!("    fn {}_fixpoint(&mut self) {{\n", rn));
    let body = derive_body_node(world, method);
    let body_stmts = body.map(|b| children(world, b.id)).unwrap_or_default();
    let set_stmt = body_stmts.first().ok_or("empty fixpoint body")?;
    let field_name = extract_set_field(world, set_stmt);

    // Initial set
    out.push_str("        {\n            let mut results = Vec::new();\n");
    let ip_id = find_pipeline_root(world, set_stmt)?;
    let ip = decompose_pipeline(world, ip_id)?;
    let mut bindings = Vec::new();
    emit_pipeline_loops(out, world, &ip, "results", 3, &mut bindings)?;
    out.push_str(&format!("            self.{} = results;\n        }}\n", field_name));

    // Fixpoint loop
    if body_stmts.len() > 1 {
        let ext = body_stmts[1];
        out.push_str("        loop {\n            let mut new_items = Vec::new();\n");
        let ep_id = find_pipeline_root(world, ext)?;
        let ep = decompose_pipeline(world, ep_id)?;
        let mut bindings = Vec::new();
        emit_pipeline_loops(out, world, &ep, "new_items", 3, &mut bindings)?;
        out.push_str(&format!("            new_items.retain(|item| !self.{}.contains(item));\n", field_name));
        out.push_str("            if new_items.is_empty() { break; }\n");
        out.push_str(&format!("            self.{}.extend(new_items);\n        }}\n", field_name));
    }
    out.push_str("    }\n\n");
    Ok(())
}

fn extract_set_field(world: &World, stmt: &ParseNode) -> String {
    if stmt.constructor == "MutableSet" {
        let ch = children(world, stmt.id);
        if let Some(first) = ch.first() {
            if first.constructor == "Access" {
                return snake(&first.text);
            }
        }
    }
    "unknown".to_string()
}

fn find_pipeline_root(world: &World, stmt: &ParseNode) -> Result<i64, String> {
    let ch = children(world, stmt.id);
    let child = ch.first().ok_or("no children for pipeline root")?;
    if child.constructor == "Access" {
        let inner = children(world, child.id);
        if let Some(first) = inner.first() {
            Ok(first.id)
        } else {
            Ok(child.id)
        }
    } else {
        Ok(child.id)
    }
}

fn decompose_pipeline(world: &World, id: i64) -> Result<Pipeline, String> {
    let node = find_node(world, id).ok_or(format!("pipeline node {} not found", id))?;
    let ch = children(world, id);
    if node.constructor == "MethodCall" {
        match node.text.as_str() {
            "map" => {
                if ch.len() < 2 { return Err("map needs 2 children".into()); }
                Ok(Pipeline::Map { source: decompose_source(world, ch[0].id)?, constructor_node: ch[1].id })
            }
            "flatMap" => {
                if ch.len() < 2 { return Err("flatMap needs 2 children".into()); }
                Ok(Pipeline::FlatMap { source: decompose_source(world, ch[0].id)?, inner: Box::new(decompose_pipeline(world, ch[1].id)?) })
            }
            "chain" => {
                if ch.len() < 2 { return Err("chain needs 2 children".into()); }
                Ok(Pipeline::Chain { left: Box::new(decompose_pipeline(world, ch[0].id)?), right: Box::new(decompose_pipeline(world, ch[1].id)?) })
            }
            other => Err(format!("unexpected pipeline op: {}", other))
        }
    } else { Err(format!("expected MethodCall in pipeline, got {}", node.constructor)) }
}

fn decompose_source(world: &World, id: i64) -> Result<Source, String> {
    let node = find_node(world, id).ok_or(format!("source node {} not found", id))?;
    let ch = children(world, id);
    if node.constructor == "MethodCall" && node.text == "filter" {
        if ch.is_empty() { return Err("filter needs children".into()); }
        let mut s = decompose_source(world, ch[0].id)?;
        if ch.len() > 1 { s.filter_ids.push(ch[1].id); }
        Ok(s)
    } else if node.constructor == "Access" {
        Ok(Source { collection: snake(&node.text), filter_ids: vec![] })
    } else { Err(format!("unexpected source: {}", node.constructor)) }
}

fn emit_pipeline_loops(out: &mut String, world: &World, pipeline: &Pipeline, rv: &str,
                       indent: usize, bindings: &mut Vec<DeriveBinding>) -> Result<(), String> {
    let ind = "    ".repeat(indent);
    match pipeline {
        Pipeline::Map { source, constructor_node } => {
            let lv = determine_loop_var(world, source, Some(*constructor_node), bindings);
            let et = elem_type_for(world, &source.collection);
            out.push_str(&format!("{ind}for {lv} in &self.{} {{\n", source.collection));
            bindings.push(DeriveBinding { var_name: lv.clone(), type_name: et });
            let fc = source.filter_ids.len();
            for (i, &fid) in source.filter_ids.iter().enumerate() {
                let c = translate_derive_expr(world, fid, bindings, None)?;
                out.push_str(&format!("{}if {} {{\n", "    ".repeat(indent+1+i), c));
            }
            let v = translate_derive_expr(world, *constructor_node, bindings, None)?;
            out.push_str(&format!("{}{rv}.push({v});\n", "    ".repeat(indent+1+fc)));
            for i in (0..fc).rev() { out.push_str(&format!("{}}}\n", "    ".repeat(indent+1+i))); }
            out.push_str(&format!("{ind}}}\n"));
            bindings.pop();
            Ok(())
        }
        Pipeline::FlatMap { source, inner } => {
            let lv = determine_loop_var(world, source, None, bindings);
            let et = elem_type_for(world, &source.collection);
            out.push_str(&format!("{ind}for {lv} in &self.{} {{\n", source.collection));
            bindings.push(DeriveBinding { var_name: lv.clone(), type_name: et });
            let fc = source.filter_ids.len();
            for (i, &fid) in source.filter_ids.iter().enumerate() {
                let c = translate_derive_expr(world, fid, bindings, None)?;
                out.push_str(&format!("{}if {} {{\n", "    ".repeat(indent+1+i), c));
            }
            emit_pipeline_loops(out, world, inner, rv, indent+1+fc, bindings)?;
            for i in (0..fc).rev() { out.push_str(&format!("{}}}\n", "    ".repeat(indent+1+i))); }
            out.push_str(&format!("{ind}}}\n"));
            bindings.pop();
            Ok(())
        }
        Pipeline::Chain { left, right } => {
            emit_pipeline_loops(out, world, left, rv, indent, bindings)?;
            emit_pipeline_loops(out, world, right, rv, indent, bindings)
        }
    }
}

fn translate_derive_expr(world: &World, id: i64, bindings: &[DeriveBinding], type_hint: Option<&str>) -> Result<String, String> {
    let node = find_node(world, id).ok_or(format!("derive expr {} not found", id))?;
    let ch = children(world, id);

    match node.constructor.as_str() {
        "BinOp" => {
            let op = &node.text;
            let lt = derive_expr_type(world, ch[0].id, bindings);
            let l = translate_derive_expr(world, ch[0].id, bindings, None)?;
            let r = translate_derive_expr(world, ch[1].id, bindings, lt.as_deref())?;
            if op == "!=" && r == "\"\"" { return Ok(format!("!{l}.is_empty()")); }
            if op == "==" && r == "false" { return Ok(format!("!{l}")); }
            if is_cmp(op) { Ok(format!("{l} {op} {r}")) } else { Ok(format!("({l} {op} {r})")) }
        }
        "Access" => {
            let f = &node.text;
            let b = translate_derive_expr(world, ch[0].id, bindings, None)?;
            match f.as_str() {
                "beforeColon" => Ok(format!("{b}[..{b}.find(':').unwrap()].to_string()")),
                "afterColon" => Ok(format!("{b}[{b}.find(':').unwrap()+1..].to_string()")),
                _ => Ok(format!("{b}.{}", snake(f)))
            }
        }
        "InstanceRef" => {
            if node.text == "Self" { Ok("self".into()) } else { Ok(snake(&node.text)) }
        }
        "BareName" => {
            match node.text.as_str() {
                "True" => Ok("true".into()),
                "False" => Ok("false".into()),
                name => {
                    if let Some(hint) = type_hint {
                        let variants = aski_core::query_domain_variants(world, hint);
                        if variants.iter().any(|(_, vname, _)| vname == name) {
                            return Ok(format!("{hint}::{name}"));
                        }
                    }
                    if let Some((domain, _)) = aski_core::query_variant_domain(world, name) {
                        Ok(format!("{domain}::{name}"))
                    } else { Ok(name.to_string()) }
                }
            }
        }
        "IntLit" => Ok(node.text.clone()),
        "StringLit" => {
            if node.text.contains("$@") {
                translate_interpolated(&node.text, bindings)
            } else { Ok(format!("\"{}\"", node.text)) }
        }
        "MethodCall" => {
            let b = translate_derive_expr(world, ch[0].id, bindings, None)?;
            let method = &node.text;
            if method == "contains" && ch.len() > 1 {
                let a = translate_derive_expr(world, ch[1].id, bindings, None)?;
                return Ok(format!("{b}.contains({a})"));
            }
            let args: Vec<String> = ch[1..].iter()
                .map(|c| translate_derive_expr(world, c.id, bindings, None))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(format!("{b}.{}({})", snake(method), args.join(", ")))
        }
        "StructConstruct" => {
            let tn = &node.text;
            let mut fs = Vec::new();
            for fe in &ch {
                if fe.constructor != "StructField" { continue; }
                let rf = snake(&fe.text);
                let vc = children(world, fe.id);
                let ft = get_struct_field_type(world, tn, &fe.text);
                if vc.is_empty() { continue; }
                let v = translate_derive_expr(world, vc[0].id, bindings, ft.as_deref())?;
                let vs = if ft.as_deref() == Some("String") { format!("{v}.clone()") } else { v };
                fs.push(format!("{rf}: {vs}"));
            }
            Ok(format!("{tn} {{ {} }}", fs.join(", ")))
        }
        _ => Err(format!("unhandled derive expr: {}", node.constructor))
    }
}

fn is_cmp(op: &str) -> bool { matches!(op, "==" | "!=" | "<" | ">" | "<=" | ">=") }

fn determine_loop_var(world: &World, source: &Source, cid: Option<i64>, bindings: &[DeriveBinding]) -> String {
    for &fid in &source.filter_ids {
        if let Some(n) = first_new_ref(world, fid, bindings) { return n; }
    }
    if let Some(c) = cid {
        if let Some(n) = first_new_ref(world, c, bindings) { return n; }
    }
    singularize(&source.collection)
}

fn first_new_ref(world: &World, id: i64, bindings: &[DeriveBinding]) -> Option<String> {
    for r in scan_refs(world, id) {
        if r == "Self" { continue; }
        let s = snake(&r);
        if !bindings.iter().any(|b| b.var_name == s) { return Some(s); }
    }
    None
}

fn scan_refs(world: &World, id: i64) -> Vec<String> {
    let Some(node) = find_node(world, id) else { return vec![] };
    let mut r = Vec::new();
    if node.constructor == "InstanceRef" { r.push(node.text.clone()); }
    for c in children(world, id) { r.extend(scan_refs(world, c.id)); }
    r
}

fn elem_type_for(world: &World, collection: &str) -> String {
    let fields = aski_core::query_struct_fields(world, "World");
    for (_, fname, ftype) in &fields {
        if snake(fname) == collection {
            if let Some(inner) = ftype.strip_prefix("Vec(").and_then(|s| s.strip_suffix(')')) {
                return inner.to_string();
            }
        }
    }
    collection.to_string()
}

fn singularize(s: &str) -> String {
    if s.ends_with("ies") { format!("{}y", &s[..s.len()-3]) }
    else if s.ends_with("ses") || s.ends_with("shes") || s.ends_with("ches") { s[..s.len()-2].to_string() }
    else if s.ends_with('s') { s[..s.len()-1].to_string() }
    else { s.to_string() }
}

fn derive_expr_type(world: &World, id: i64, bindings: &[DeriveBinding]) -> Option<String> {
    let node = find_node(world, id)?;
    if node.constructor == "Access" {
        let ch = children(world, id);
        let base = ch.first()?;
        if base.constructor == "InstanceRef" && base.text != "Self" {
            let b = bindings.iter().find(|b| b.var_name == snake(&base.text))?;
            let fields = aski_core::query_struct_fields(world, &b.type_name);
            for (_, fname, ftype) in &fields {
                if *fname == node.text { return Some(ftype.clone()); }
            }
        }
    }
    None
}

fn get_struct_field_type(world: &World, struct_name: &str, field_name: &str) -> Option<String> {
    let fields = aski_core::query_struct_fields(world, struct_name);
    for (_, fname, ftype) in &fields {
        if fname == field_name { return Some(ftype.clone()); }
    }
    None
}

fn translate_interpolated(s: &str, _bindings: &[DeriveBinding]) -> Result<String, String> {
    let mut fmt = String::new();
    let mut args = Vec::new();
    let mut i = 0;
    let chars: Vec<char> = s.chars().collect();
    while i < chars.len() {
        if i + 1 < chars.len() && chars[i] == '$' && chars[i+1] == '@' {
            fmt.push_str("{}");
            i += 2;
            let rs = i;
            while i < chars.len() && chars[i].is_alphanumeric() { i += 1; }
            let rn = &s[rs..i];
            if i < chars.len() && chars[i] == '.' { i += 1; }
            let fs = i;
            while i < chars.len() && chars[i].is_alphanumeric() { i += 1; }
            let fn_ = &s[fs..i];
            args.push(format!("{}.{}", snake(rn), snake(fn_)));
        } else { fmt.push(chars[i]); i += 1; }
    }
    Ok(format!("format!(\"{fmt}\", {})", args.join(", ")))
}
