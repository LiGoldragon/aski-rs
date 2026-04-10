//! Codegen v3 — reads kernel relations only. No hand-built maps.
//! Every query goes through aski_core. If a query is complex,
//! the kernel needs a new derived relation.

use aski_core::{self, World};

pub struct CodegenConfig {
    pub rkyv: bool,
}

impl Default for CodegenConfig {
    fn default() -> Self { Self { rkyv: false } }
}

pub fn generate(world: &World) -> Result<String, String> {
    generate_with_config(world, &CodegenConfig::default())
}

pub fn generate_with_config(world: &World, config: &CodegenConfig) -> Result<String, String> {
    let mut out = String::new();

    // Emit operator trait imports from kernel
    let op_impls = aski_core::query_all_operator_impls(world);
    let imports: Vec<String> = op_impls.iter()
        .map(|(name, _, _)| rust_trait(name))
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();
    for imp in &imports {
        out.push_str(&format!("use std::ops::{imp};\n"));
    }
    if !imports.is_empty() { out.push('\n'); }

    let mut nodes = aski_core::query_all_top_level_nodes(world);
    nodes.sort_by_key(|(_, kind, _)| match kind.as_str() {
        "foreign_block" | "domain" | "struct" | "const" => 0u8,
        "trait" => 1,
        "impl" => 2,
        "main" => 3,
        _ => 4,
    });

    for (node_id, kind, name) in &nodes {
        match kind.as_str() {
            "domain" => emit_domain(&mut out, world, name, config)?,
            "struct" => emit_struct(&mut out, world, name, config)?,
            "const" => emit_const(&mut out, world, *node_id)?,
            "trait" => emit_trait(&mut out, world, *node_id, name)?,
            "impl" => emit_impl(&mut out, world, *node_id)?,
            "main" => emit_main(&mut out, world, *node_id)?,
            "foreign_block" => emit_foreign_block(&mut out, world, *node_id, name)?,
            _ => {}
        }
    }

    Ok(out)
}

// ── Naming ──

fn snake(s: &str) -> String {
    let mut out = String::new();
    for (i, ch) in s.chars().enumerate() {
        if ch.is_uppercase() && i > 0 { out.push('_'); }
        out.push(ch.to_ascii_lowercase());
    }
    out
}

fn screaming(s: &str) -> String { snake(s).to_uppercase() }

fn rust_type(t: &str) -> String {
    match t {
        "F32" => "f32", "F64" => "f64",
        "I8" => "i8", "I16" => "i16", "I32" => "i32", "I64" => "i64",
        "U8" => "u8", "U16" => "u16", "U32" => "u32", "U64" => "u64",
        "Bool" => "bool", "String" => "String",
        // IR stores parameterized types as Vec(T), aski syntax uses Vec{T}
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

fn is_primitive(t: &str) -> bool {
    matches!(t, "F32"|"F64"|"I8"|"I16"|"I32"|"I64"|"U8"|"U16"|"U32"|"U64"|"Bool"|"String")
}

fn rkyv_suffix(config: &CodegenConfig) -> &'static str {
    if config.rkyv { ", rkyv::Archive, rkyv::Serialize, rkyv::Deserialize" } else { "" }
}

// ── Qualify a name: variant → Domain::Variant, other → as-is ──

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

/// Qualify a variant name for use in a match pattern.
/// Data-carrying variants need (..) wildcard.
fn qualify_pattern(world: &World, name: &str) -> String {
    match name {
        "True" => "true".to_string(),
        "False" => "false".to_string(),
        "_" => "_".to_string(),
        _ => {
            if let Some((domain, domain_id)) = aski_core::query_variant_domain(world, name) {
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

// ── Domain ──

fn has_data_carrying(world: &World, domain_name: &str) -> bool {
    aski_core::query_domain_variants(world, domain_name)
        .iter().any(|(_, _, wraps)| wraps.is_some())
}

fn emit_domain(out: &mut String, world: &World, name: &str, config: &CodegenConfig) -> Result<(), String> {
    let variants = aski_core::query_domain_variants(world, name);
    let has_data = variants.iter().any(|(_, _, wraps)| wraps.is_some());
    let rkyv = rkyv_suffix(config);
    let derives = if has_data {
        format!("#[derive(Debug, Clone, PartialEq, Eq{rkyv})]")
    } else {
        format!("#[derive(Debug, Clone, Copy, PartialEq, Eq{rkyv})]")
    };
    out.push_str(&format!("{derives}\npub enum {name} {{\n"));
    for (_, vname, wraps) in &variants {
        match wraps {
            Some(w) => out.push_str(&format!("    {vname}({}),\n", rust_type(w))),
            None => out.push_str(&format!("    {vname},\n")),
        }
    }
    out.push_str("}\n\n");
    // Display impl
    out.push_str(&format!("impl std::fmt::Display for {name} {{\n"));
    out.push_str("    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {\n");
    out.push_str("        write!(f, \"{:?}\", self)\n    }\n}\n\n");
    // from_str / to_str — only for enums without data-carrying variants
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

// ── Struct ──

fn is_copy_eligible(type_name: &str, world: &World) -> bool {
    match type_name {
        "U8"|"U16"|"U32"|"U64"|"I8"|"I16"|"I32"|"I64"|"F32"|"F64"|"Bool" => true,
        _ => {
            let variants = aski_core::query_domain_variants(world, type_name);
            !variants.is_empty() && variants.iter().all(|(_, _, w)| w.is_none())
        }
    }
}

fn emit_struct(out: &mut String, world: &World, name: &str, config: &CodegenConfig) -> Result<(), String> {
    let fields = aski_core::query_struct_fields(world, name);
    let all_copy = !fields.is_empty() && fields.iter().all(|(_, fname, ftype)| {
        !aski_core::is_recursive_field(world, name, fname) && is_copy_eligible(ftype, world)
    });
    let rkyv = rkyv_suffix(config);
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
        if aski_core::is_recursive_field(world, name, fname) {
            out.push_str(&format!("    pub {sname}: Box<{rt}>,\n"));
        } else {
            out.push_str(&format!("    pub {sname}: {rt},\n"));
        }
    }
    out.push_str("}\n\n");

    // Check if this struct has Vec fields → generate accessor methods
    let vec_fields: Vec<(String, String)> = fields.iter()
        .filter_map(|(_, fname, ftype)| {
            ftype.strip_prefix("Vec(").and_then(|s| s.strip_suffix(')'))
                .map(|elem| (snake(fname), elem.to_string()))
        })
        .collect();
    if !vec_fields.is_empty() {
        // Add Default derive + new() for container structs
        // (re-emit with Default — the struct is already emitted, so add impl)
        out.push_str(&format!("impl Default for {name} {{ fn default() -> Self {{ Self {{"));
        for (_, fname, _) in &fields {
            out.push_str(&format!(" {}: Default::default(),", snake(fname)));
        }
        out.push_str(" } } }\n\n");
        out.push_str(&format!("impl {name} {{\n"));
        out.push_str("    pub fn new() -> Self { Self::default() }\n\n");
        // For each Vec field, generate accessor methods keyed by element struct fields
        for (vec_field, elem_type) in &vec_fields {
            let elem_fields = aski_core::query_struct_fields(world, elem_type);
            if elem_fields.is_empty() { continue; }
            let type_snake = snake(elem_type);
            for (_, ef_name, ef_type) in &elem_fields {
                let ef_snake = snake(ef_name);
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

// ── Constant ──

fn emit_const(out: &mut String, world: &World, node_id: i64) -> Result<(), String> {
    let (name, type_ref, has_value) = aski_core::query_constant(world, node_id)
        .ok_or("constant not found")?;
    if has_value {
        let children = aski_core::query_child_exprs(world, node_id);
        if let Some((cid, _, _, _)) = children.first() {
            let val = emit_expr(world, *cid)?;
            out.push_str(&format!("pub const {}: {} = {val};\n\n", screaming(&name), rust_type(&type_ref)));
        }
    }
    Ok(())
}

// ── Trait ──

fn emit_trait(out: &mut String, world: &World, node_id: i64, name: &str) -> Result<(), String> {
    let supers = aski_core::query_supertraits(world, node_id);
    let children = aski_core::query_child_nodes(world, node_id);
    let super_str = if supers.is_empty() { String::new() }
        else { format!(": {}", supers.iter().map(|s| rust_trait(s)).collect::<Vec<_>>().join(" + ")) };
    out.push_str(&format!("pub trait {}{super_str} {{\n", rust_trait(name)));
    for (cid, kind, cname) in &children {
        if kind == "method_sig" {
            let params = aski_core::query_params(world, *cid);
            let ret = aski_core::query_return_type(world, *cid);
            let ret_str = ret.map(|r| format!(" -> {}", rust_type(&r))).unwrap_or_default();
            out.push_str(&format!("    fn {}({}){ret_str};\n", snake(cname), emit_params(&params)));
        }
    }
    out.push_str("}\n\n");
    Ok(())
}

// ── Impl ──

fn emit_impl(out: &mut String, world: &World, node_id: i64) -> Result<(), String> {
    let children = aski_core::query_child_nodes(world, node_id);
    let trait_name = world.nodes.iter()
        .find(|n| n.id == node_id)
        .map(|n| n.name.clone())
        .ok_or("impl node not found")?;

    for (child_id, kind, target_type) in &children {
        if kind != "impl_body" { continue; }

        // Check if this is an operator trait impl via kernel
        let is_operator = aski_core::query_operator_impl(world, &trait_name, target_type).is_some();

        // Derive trait → special collection pipeline emission
        if trait_name == "derive" {
            out.push_str(&format!("impl {target_type} {{\n"));
            emit_derive_impl(out, world, *child_id, target_type)?;
            out.push_str("}\n\n");
            continue;
        }

        out.push_str(&format!("impl {} for {target_type} {{\n", rust_trait(&trait_name)));

        // Operator traits need `type Output`
        if is_operator {
            let ret = aski_core::query_child_nodes(world, *child_id).iter()
                .find(|(_, k, _)| k == "method" || k == "tail_method")
                .and_then(|(mid, _, _)| aski_core::query_return_type(world, *mid));
            let output = ret.unwrap_or_else(|| target_type.clone());
            out.push_str(&format!("    type Output = {output};\n"));
        }

        let methods = aski_core::query_child_nodes(world, *child_id);
        for (mid, mkind, mname) in &methods {
            if mkind != "method" && mkind != "tail_method" { continue; }
            let params = aski_core::query_params(world, *mid);
            let ret = aski_core::query_return_type(world, *mid);
            let ret_str = ret.map(|r| format!(" -> {}", rust_type(&r))).unwrap_or_default();

            // Operator traits use owned self, not &self
            let param_str = if is_operator {
                emit_params_operator(&params)
            } else {
                emit_params(&params)
            };
            out.push_str(&format!("    fn {}({}){ret_str} {{\n", snake(mname), param_str));

            let arms = aski_core::query_match_arms(world, *mid);
            if !arms.is_empty() {
                emit_match_body(out, world, &arms)?;
            } else {
                let exprs = aski_core::query_child_exprs(world, *mid);
                emit_block(out, world, &exprs, "        ")?;
            }

            out.push_str("    }\n");
        }
        out.push_str("}\n\n");
    }
    Ok(())
}

// ── Main ──

fn emit_main(out: &mut String, world: &World, node_id: i64) -> Result<(), String> {
    out.push_str("pub fn main() {\n");
    let exprs = aski_core::query_child_exprs(world, node_id);
    emit_block(out, world, &exprs, "    ")?;
    out.push_str("}\n");
    Ok(())
}

// ── Foreign block ──

fn emit_foreign_block(out: &mut String, world: &World, node_id: i64, library: &str) -> Result<(), String> {
    let children = aski_core::query_child_nodes(world, node_id);
    let crate_name = snake(library);
    for (cid, _, cname) in &children {
        let ret = aski_core::query_return_type(world, *cid).unwrap_or_else(|| "()".into());
        let params = aski_core::query_params(world, *cid);
        let exprs = aski_core::query_child_exprs(world, *cid);
        let extern_name = exprs.iter()
            .find(|(_, k, _, _)| k == "extern_name")
            .and_then(|(_, _, _, v)| v.clone())
            .unwrap_or_else(|| cname.clone());
        let (param_strs, arg_names) = emit_ffi_params(&params);
        out.push_str(&format!("pub fn {}({}) -> {} {{\n",
            snake(cname), param_strs, rust_type(&ret)));
        out.push_str(&format!("    {crate_name}::{}({})\n", extern_name, arg_names));
        out.push_str("}\n\n");
    }
    Ok(())
}

// ── Params ──

fn emit_params(params: &[(String, Option<String>, Option<String>)]) -> String {
    params.iter().filter_map(|(kind, name, typ)| match kind.as_str() {
        "borrow_self" => Some("&self".into()),
        "mut_borrow_self" => Some("&mut self".into()),
        "owned_self" => Some("self".into()),
        "named" | "borrow" => {
            let t = typ.as_deref()?;
            let n = name.as_deref().unwrap_or(t);
            Some(format!("{}: &{}", snake(n), rust_type(t)))
        }
        _ => None,
    }).collect::<Vec<_>>().join(", ")
}

fn emit_params_operator(params: &[(String, Option<String>, Option<String>)]) -> String {
    params.iter().filter_map(|(kind, name, typ)| match kind.as_str() {
        "borrow_self" | "mut_borrow_self" | "owned_self" => Some("self".into()),
        "named" | "borrow" | "owned" => {
            let t = typ.as_deref()?;
            let n = name.as_deref().unwrap_or(t);
            Some(format!("{}: {}", snake(n), rust_type(t)))
        }
        _ => None,
    }).collect::<Vec<_>>().join(", ")
}

fn emit_ffi_params(params: &[(String, Option<String>, Option<String>)]) -> (String, String) {
    let mut p = Vec::new();
    let mut a = Vec::new();
    for (kind, name, typ) in params {
        if let Some(t) = typ {
            let n = snake(name.as_deref().unwrap_or(kind));
            p.push(format!("{n}: {}", rust_type(t)));
            a.push(n);
        }
    }
    (p.join(", "), a.join(", "))
}

// ── Match body ──

fn emit_match_body(out: &mut String, world: &World, arms: &[(i64, Vec<String>, Option<i64>, String)]) -> Result<(), String> {
    out.push_str("        match self {\n");
    for (_, patterns, body_id, _) in arms {
        let pat = patterns.first().map(|p| qualify_pattern(world, p)).unwrap_or("_".into());
        if let Some(bid) = body_id {
            let body = emit_expr(world, *bid)?;
            out.push_str(&format!("            {pat} => {body},\n"));
        }
    }
    out.push_str("        }\n");
    Ok(())
}

// ── Block ──

fn emit_block(out: &mut String, world: &World, exprs: &[(i64, String, i64, Option<String>)], indent: &str) -> Result<(), String> {
    for (eid, kind, _, _) in exprs {
        match kind.as_str() {
            "same_type_new" | "sub_type_new" => {
                let (var_name, type_name) = aski_core::query_binding_info(world, *eid)
                    .ok_or_else(|| format!("no BindingInfo for expr {eid}"))?;
                let children = aski_core::query_child_exprs(world, *eid);
                let svar = snake(&var_name);
                let type_kind = aski_core::query_type_kind(world, &type_name);

                match type_kind.as_deref() {
                    Some("struct") => {
                        let val = emit_struct_or_expr(world, &type_name, &children)?;
                        out.push_str(&format!("{indent}let {svar}: {type_name} = {val};\n"));
                    }
                    _ if is_primitive(&type_name) => {
                        if let Some((cid, _, _, _)) = children.first() {
                            let val = emit_expr(world, *cid)?;
                            out.push_str(&format!("{indent}let {svar}: {} = {val};\n", rust_type(&type_name)));
                        }
                    }
                    _ => {
                        if let Some((cid, _, _, _)) = children.first() {
                            let val = emit_expr(world, *cid)?;
                            out.push_str(&format!("{indent}let {svar}: {} = {val};\n", rust_type(&type_name)));
                        }
                    }
                }
            }
            "mutable_new" => {
                let children = aski_core::query_child_exprs(world, *eid);
                if let Some((var_name, type_name)) = aski_core::query_mutable_binding(world, *eid) {
                    let svar = snake(&var_name);
                    if let Some((cid, _, _, _)) = children.first() {
                        let val = emit_expr(world, *cid)?;
                        out.push_str(&format!("{indent}let mut {svar}: {} = {val};\n", rust_type(&type_name)));
                    }
                } else if let Some((var_name, type_name)) = aski_core::query_binding_info(world, *eid) {
                    let svar = snake(&var_name);
                    if let Some((cid, _, _, _)) = children.first() {
                        let val = emit_expr(world, *cid)?;
                        out.push_str(&format!("{indent}let mut {svar}: {} = {val};\n", rust_type(&type_name)));
                    }
                }
            }
            "mutable_set" => {
                let children = aski_core::query_child_exprs(world, *eid);
                let var_name = aski_core::query_expr_by_id(world, *eid)
                    .and_then(|(_, v)| v);
                if let Some(name) = var_name {
                    let svar = snake(&name);
                    if let Some((cid, ckind, _, cval)) = children.first() {
                        if svar == "self" && ckind == "method_call" {
                            // ~@Self.Field.set(value) → self.field = value;
                            let mc_children = aski_core::query_child_exprs(world, *cid);
                            if mc_children.len() > 1 {
                                // First child is the Access chain (Self.Field), second is the value
                                let field_chain = emit_expr(world, mc_children[0].0)?;
                                let val = emit_expr(world, mc_children[1].0)?;
                                out.push_str(&format!("{indent}{field_chain} = {val};\n"));
                            } else {
                                let val = emit_expr(world, *cid)?;
                                out.push_str(&format!("{indent}{svar} = {val};\n"));
                            }
                        } else {
                            let val = emit_expr(world, *cid)?;
                            out.push_str(&format!("{indent}{svar} = {val};\n"));
                        }
                    }
                }
            }
            "std_out" => {
                let children = aski_core::query_child_exprs(world, *eid);
                if let Some((cid, _, _, _)) = children.first() {
                    let val = emit_expr(world, *cid)?;
                    out.push_str(&format!("{indent}println!(\"{{}}\", {val});\n"));
                }
            }
            "yield" => {
                // >collection.method — generates for-loop iteration
                let children = aski_core::query_child_exprs(world, *eid);
                if let Some((cid, ckind, _, cval)) = children.first() {
                    if ckind == "method_call" {
                        // MethodCall on a collection: for item in collection { body }
                        let method_name = cval.as_deref().unwrap_or("");
                        let mc_children = aski_core::query_child_exprs(world, *cid);
                        if let Some((base_id, base_kind, _, base_val)) = mc_children.first() {
                            let collection = emit_expr(world, *base_id)?;
                            // The method body argument (second child of MethodCall)
                            if mc_children.len() > 1 {
                                let (body_id, body_kind, _, body_val) = &mc_children[1];
                                let loop_var = snake(method_name);
                                out.push_str(&format!("{indent}for {loop_var} in {collection}.iter() {{\n"));
                                let body_exprs = aski_core::query_child_exprs(world, *body_id);
                                emit_block(out, world, &body_exprs, &format!("{indent}    "))?;
                                out.push_str(&format!("{indent}}}\n"));
                            } else {
                                let val = emit_expr(world, *cid)?;
                                out.push_str(&format!("{indent}{val};\n"));
                            }
                        }
                    } else {
                        let val = emit_expr(world, *cid)?;
                        out.push_str(&format!("{indent}{val};\n"));
                    }
                }
            }
            "return" => {
                let children = aski_core::query_child_exprs(world, *eid);
                if let Some((cid, _, _, _)) = children.first() {
                    let val = emit_expr(world, *cid)?;
                    out.push_str(&format!("{indent}{val}\n"));
                }
            }
            "error_prop" => {
                let children = aski_core::query_child_exprs(world, *eid);
                if let Some((cid, _, _, _)) = children.first() {
                    let val = emit_expr(world, *cid)?;
                    out.push_str(&format!("{indent}{val}?;\n"));
                }
            }
            _ => {
                let val = emit_expr(world, *eid)?;
                if !val.is_empty() {
                    out.push_str(&format!("{indent}{val};\n"));
                }
            }
        }
    }
    Ok(())
}

// ── Expression — uniform, context-free ──

fn emit_expr(world: &World, expr_id: i64) -> Result<String, String> {
    let (kind, value) = aski_core::query_expr_by_id(world, expr_id)
        .ok_or_else(|| format!("expr {expr_id} not found"))?;

    match kind.as_str() {
        "int_lit" => Ok(value.unwrap_or("0".into())),
        "float_lit" => Ok(value.unwrap_or("0.0".into())),
        "string_lit" => Ok(format!("\"{}\"", value.unwrap_or_default())),
        "const_ref" => Ok(screaming(&value.unwrap_or_default())),
        "instance_ref" => {
            let name = value.unwrap_or_default();
            if name == "Self" { Ok("self".into()) }
            else { Ok(snake(&name)) }
        }
        "bare_name" => Ok(qualify(world, &value.unwrap_or_default())),

        "bin_op" => {
            let children = aski_core::query_child_exprs(world, expr_id);
            let op = value.unwrap_or("+".into());
            if children.len() >= 2 {
                let l = emit_expr(world, children[0].0)?;
                let r = emit_expr(world, children[1].0)?;
                Ok(format!("({l} {op} {r})"))
            } else { Ok("todo!()".into()) }
        }

        "inline_eval" => {
            let children = aski_core::query_child_exprs(world, expr_id);
            if children.len() == 1 {
                emit_expr(world, children[0].0)
            } else {
                children.last().map(|(id, _, _, _)| emit_expr(world, *id))
                    .unwrap_or(Ok(String::new()))
            }
        }

        "access" => {
            let name = value.unwrap_or_default();
            let children = aski_core::query_child_exprs(world, expr_id);
            if let Some((cid, _, _, _)) = children.first() {
                let base = emit_expr(world, *cid)?;
                let s = snake(&name);
                // Kernel primitives on access
                if s == "len" { return Ok(format!("({base}.len() as u32)")); }
                if s == "clone" { return Ok(format!("{base}.clone()")); }
                if s == "to_string" { return Ok(format!("{base}.to_string()")); }
                if s == "is_empty" { return Ok(format!("{base}.is_empty()")); }
                if s == "unwrap" { return Ok(format!("{base}.unwrap()")); }
                if name.chars().next().map(|c| c.is_lowercase()).unwrap_or(false) {
                    Ok(format!("{base}.{s}()"))
                } else {
                    Ok(format!("{base}.{s}"))
                }
            } else {
                Ok(format!("self.{}", snake(&name)))
            }
        }

        "method_call" => {
            let method = value.unwrap_or_default();
            let children = aski_core::query_child_exprs(world, expr_id);
            let base = if children.is_empty() { "self".into() }
                       else { emit_expr(world, children[0].0)? };
            let s = snake(&method);

            // Kernel primitive casts
            if method == "toF32" { return Ok(format!("({base} as f32)")); }
            if method == "toU32" { return Ok(format!("({base} as u32)")); }
            if method == "toI64" { return Ok(format!("({base} as i64)")); }
            if method == "toSnake" { return Ok(format!("to_snake(&{base})")); }
            if method == "toRustType" { return Ok(format!("to_rust_type(&{base})")); }
            if method == "stripVec" { return Ok(format!("strip_vec(&{base})")); }
            if method == "toParamType" { return Ok(format!("to_param_type(&{base})")); }
            if method == "needsPascalAlias" { return Ok(format!("{base}.needs_pascal_alias()")); }
            if method == "allFieldsCopy" { return Ok(format!("{base}.all_fields_copy()")); }

            // .with(Field(value)) → struct update syntax
            if method == "with" {
                let mut field_inits = Vec::new();
                for (cid, ckind, _, cval) in children.iter().skip(1) {
                    if ckind == "struct_construct" {
                        let fname = snake(cval.as_deref().unwrap_or(""));
                        let inner = aski_core::query_child_exprs(world, *cid);
                        if let Some((vid, _, _, _)) = inner.first() {
                            let gc = aski_core::query_child_exprs(world, *vid);
                            if let Some((gid, _, _, _)) = gc.first() {
                                field_inits.push(format!("{fname}: {}", emit_expr(world, *gid)?));
                            } else {
                                field_inits.push(format!("{fname}: {}", emit_expr(world, *vid)?));
                            }
                        }
                    } else {
                        let v = emit_expr(world, *cid)?;
                        if let Some(name) = cval.as_deref() {
                            field_inits.push(format!("{}: {v}", snake(name)));
                        } else {
                            field_inits.push(v);
                        }
                    }
                }
                // Infer type from the receiver — find the struct type
                // For now use a generic approach: receiver type name
                if field_inits.is_empty() {
                    return Ok(format!("{base}.clone()"));
                }
                // Try to determine the type name from the base expression
                let type_name = infer_self_type(world, expr_id);
                return Ok(format!("{type_name} {{ {}, ..{base}.clone() }}", field_inits.join(", ")));
            }

            // Vec::get(index) → .get(index as usize).unwrap().clone()
            if s == "get" && children.len() == 2 {
                let arg = emit_expr(world, children[1].0)?;
                return Ok(format!("{base}.get({arg} as usize).unwrap().clone()"));
            }

            // Vec::len() → .len() as u32 (aski uses U32 for lengths)
            if s == "len" && children.len() == 1 {
                return Ok(format!("({base}.len() as u32)"));
            }

            let args: Vec<String> = children.iter().skip(1)
                .map(|(cid, _, _, _)| emit_expr(world, *cid).map(|v| format!("&{v}")))
                .collect::<Result<_, _>>()?;
            if args.is_empty() { Ok(format!("{base}.{s}()")) }
            else { Ok(format!("{base}.{s}({})", args.join(", "))) }
        }

        "struct_construct" => {
            let name = value.unwrap_or_default();
            let children = aski_core::query_child_exprs(world, expr_id);

            if aski_core::query_variant_domain(world, &name).is_some() {
                if children.is_empty() { return Ok(qualify(world, &name)); }
                if children.len() == 1 {
                    let inner = emit_expr(world, children[0].0)?;
                    return Ok(format!("{}({inner})", qualify(world, &name)));
                }
            }

            let mut fields = Vec::new();
            for (cid, _, _, cval) in &children {
                let fname = snake(cval.as_deref().unwrap_or(""));
                let inner = aski_core::query_child_exprs(world, *cid);
                if let Some((vid, _, _, _)) = inner.first() {
                    fields.push(format!("{fname}: {}", emit_expr(world, *vid)?));
                }
            }
            Ok(format!("{name} {{ {} }}", fields.join(", ")))
        }

        "group" => {
            let children = aski_core::query_child_exprs(world, expr_id);
            children.first().map(|(cid, _, _, _)| {
                emit_expr(world, *cid).map(|v| format!("({v})"))
            }).unwrap_or(Ok("()".into()))
        }

        "return" => {
            let children = aski_core::query_child_exprs(world, expr_id);
            children.first().map(|(cid, _, _, _)| emit_expr(world, *cid))
                .unwrap_or(Ok("()".into()))
        }

        "fn_call" => {
            let name = value.unwrap_or_default();
            let children = aski_core::query_child_exprs(world, expr_id);
            let args: Vec<String> = children.iter()
                .map(|(cid, _, _, _)| emit_expr(world, *cid).map(|v| format!("&{v}")))
                .collect::<Result<_, _>>()?;
            Ok(format!("{}({})", snake(&name), args.join(", ")))
        }

        "match" => {
            let children = aski_core::query_child_exprs(world, expr_id);
            let arms = aski_core::query_match_arms(world, expr_id);
            if let Some((tid, _, _, _)) = children.first() {
                let target = emit_expr(world, *tid)?;
                let arm_strs: Vec<String> = arms.iter().filter_map(|(_, pats, bid, _)| {
                    let pat = pats.first().map(|p| qualify_pattern(world, p)).unwrap_or("_".into());
                    bid.map(|b| emit_expr(world, b).ok().map(|body| format!("{pat} => {body}")))
                        .flatten()
                }).collect();
                Ok(format!("match {target} {{ {} }}", arm_strs.join(", ")))
            } else { Ok("todo!()".into()) }
        }

        "yield" => {
            let children = aski_core::query_child_exprs(world, expr_id);
            children.first().map(|(cid, _, _, _)| emit_expr(world, *cid))
                .unwrap_or(Ok("()".into()))
        }

        "struct_field" => {
            // Named struct field: value is the field name, child is the value expr
            let fname = snake(&value.unwrap_or_default());
            let children = aski_core::query_child_exprs(world, expr_id);
            if let Some((cid, _, _, _)) = children.first() {
                let val = emit_expr(world, *cid)?;
                Ok(format!("{fname}: {val}"))
            } else {
                Ok(format!("{fname}: todo!()"))
            }
        }

        "stub" => Ok("todo!()".into()),
        other => Ok(format!("/* unhandled: {other} */")),
    }
}

// ── Struct construction from binding children ──

/// Infer the self type for struct update syntax by walking up the expression tree
/// to find the containing method's parent impl type.
fn infer_self_type(world: &World, expr_id: i64) -> String {
    // Walk up from expr → find the method node → find the impl → get target type
    if let Some(expr) = world.exprs.iter().find(|e| e.id == expr_id) {
        // Walk up parent chain to find the method node
        let mut current_id = expr.parent_id;
        while current_id != 0 {
            if let Some(parent_expr) = world.exprs.iter().find(|e| e.id == current_id) {
                current_id = parent_expr.parent_id;
            } else {
                // Not an expr — check if it's a node
                if let Some(node) = world.nodes.iter().find(|n| n.id == current_id) {
                    if node.kind == aski_core::NodeKind::Method || node.kind == aski_core::NodeKind::TailMethod {
                        // Found the method — get the impl body parent
                        if let Some(impl_body) = world.nodes.iter().find(|n| n.id == node.parent) {
                            // The impl body's parent trait_impl has the type name
                            for ti in &world.trait_impls {
                                if ti.impl_node_id == impl_body.id {
                                    return ti.type_name.clone();
                                }
                            }
                        }
                    }
                }
                break;
            }
        }
    }
    "Self".to_string()
}

fn emit_struct_or_expr(world: &World, type_name: &str, children: &[(i64, String, i64, Option<String>)]) -> Result<String, String> {
    let mut fields = Vec::new();
    for (cid, ckind, _, cval) in children {
        if ckind == "struct_construct" || ckind == "struct_field" {
            let fname = snake(cval.as_deref().unwrap_or(""));
            let inner = aski_core::query_child_exprs(world, *cid);
            if let Some((vid, vkind, _, _)) = inner.first() {
                // If the child is itself a struct_field, get its value child
                if vkind == "struct_field" {
                    let gc = aski_core::query_child_exprs(world, *vid);
                    if let Some((gid, _, _, _)) = gc.first() {
                        fields.push(format!("{fname}: {}", emit_expr(world, *gid)?));
                    }
                } else {
                    fields.push(format!("{fname}: {}", emit_expr(world, *vid)?));
                }
            }
        } else {
            return emit_expr(world, *cid);
        }
    }
    if fields.is_empty() { Ok(format!("{type_name} {{}}")) }
    else { Ok(format!("{type_name} {{ {} }}", fields.join(", "))) }
}

// ═══════════════════════════════════════════════════════════════
// Derive rule emission — collection pipelines → nested for-loops
// ═══════════════════════════════════════════════════════════════

use aski_core::{ExprKind, NodeKind};

struct DeriveBinding {
    var_name: String,
    type_name: String,
}

enum Pipeline {
    Map { source: Source, constructor_id: i64 },
    FlatMap { source: Source, inner: Box<Pipeline> },
    Chain { left: Box<Pipeline>, right: Box<Pipeline> },
}

struct Source {
    collection: String,
    filter_ids: Vec<i64>,
}

/// Emit a complete derive trait impl for a container type (e.g. World).
fn emit_derive_impl(out: &mut String, world: &World, impl_body_id: i64, target_type: &str) -> Result<(), String> {
    let mut methods: Vec<_> = world.nodes.iter()
        .filter(|n| n.parent == impl_body_id &&
                (n.kind == NodeKind::Method || n.kind == NodeKind::TailMethod))
        .collect();
    methods.sort_by_key(|n| n.id);

    // derive() dispatcher
    let derive_method = methods.iter().find(|m| m.name == "derive")
        .ok_or("no derive() method in derive impl")?;
    out.push_str("    pub fn derive(&mut self) {\n");
    let body_exprs = child_exprs_sorted(world, derive_method.id);
    for expr in &body_exprs {
        // Each statement is MutableSet("Self", MethodCall(name)) or MethodCall directly
        let method_name = if expr.kind == ExprKind::MethodCall {
            Some(expr.value.clone())
        } else if expr.kind == ExprKind::MutableSet {
            // Child of MutableSet is the chain result — a MethodCall or Access
            child_exprs_sorted(world, expr.id).first()
                .filter(|c| c.kind == ExprKind::MethodCall)
                .map(|c| c.value.clone())
        } else { None };
        if let Some(name) = method_name {
            let is_fp = methods.iter().any(|m| m.name == name && m.kind == NodeKind::TailMethod);
            if is_fp {
                out.push_str(&format!("        self.{}_fixpoint();\n", snake(&name)));
            } else {
                out.push_str(&format!("        self.{}();\n", snake(&name)));
            }
        }
    }
    out.push_str("    }\n\n");

    // Each derive method
    for method in &methods {
        if method.name == "derive" { continue; }
        let rn = snake(&method.name);
        if method.kind == NodeKind::TailMethod {
            emit_fixpoint_method(out, world, method, &rn)?;
        } else {
            emit_simple_derive_method(out, world, method, &rn)?;
        }
    }
    Ok(())
}

fn emit_simple_derive_method(out: &mut String, world: &World, method: &aski_core::Node, rn: &str) -> Result<(), String> {
    out.push_str(&format!("    fn {}(&mut self) {{\n", rn));
    out.push_str("        let mut results = Vec::new();\n");
    let body = child_exprs_sorted(world, method.id);
    let stmt = body.first().ok_or(format!("empty body in {}", method.name))?;
    let field_name = extract_set_field_from_world(world, stmt.id);
    let pipeline_id = find_pipeline_root(world, stmt.id)?;
    let pipeline = decompose_pipeline(world, pipeline_id)?;
    let mut bindings = Vec::new();
    emit_pipeline_loops(out, world, &pipeline, "results", 2, &mut bindings)?;
    out.push_str(&format!("        self.{} = results;\n", field_name));
    out.push_str("    }\n\n");
    Ok(())
}

fn emit_fixpoint_method(out: &mut String, world: &World, method: &aski_core::Node, rn: &str) -> Result<(), String> {
    out.push_str(&format!("    fn {}_fixpoint(&mut self) {{\n", rn));
    let body = child_exprs_sorted(world, method.id);
    let set_stmt = &body[0];
    let field_name = extract_set_field_from_world(world, set_stmt.id);
    // Initial set
    out.push_str("        {\n            let mut results = Vec::new();\n");
    let ip_id = find_pipeline_root(world, set_stmt.id)?;
    let ip = decompose_pipeline(world, ip_id)?;
    let mut bindings = Vec::new();
    emit_pipeline_loops(out, world, &ip, "results", 3, &mut bindings)?;
    out.push_str(&format!("            self.{} = results;\n        }}\n", field_name));
    // Fixpoint loop
    out.push_str("        loop {\n            let mut new_items = Vec::new();\n");
    let ext = &body[1];
    let ep_id = find_pipeline_root(world, ext.id)?;
    let ep = decompose_pipeline(world, ep_id)?;
    let mut bindings = Vec::new();
    emit_pipeline_loops(out, world, &ep, "new_items", 3, &mut bindings)?;
    out.push_str(&format!("            new_items.retain(|item| !self.{}.contains(item));\n", field_name));
    out.push_str("            if new_items.is_empty() { break; }\n");
    out.push_str(&format!("            self.{}.extend(new_items);\n        }}\n    }}\n\n", field_name));
    Ok(())
}

/// Find the pipeline expression root by unwrapping Access/MutableSet chains.
/// MutableSet stores Access(pipeline, field_name) as its child.
fn find_pipeline_root(world: &World, expr_id: i64) -> Result<i64, String> {
    let ch = child_exprs_sorted(world, expr_id);
    if ch.is_empty() { return Err("no children for pipeline root".into()); }
    let child = ch[0];
    if child.kind == ExprKind::Access {
        // Access(inner, field) — the inner is the pipeline
        let inner = child_exprs_sorted(world, child.id);
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
    let expr = find_expr_by_id(world, id)?;
    let ch = child_exprs_sorted(world, id);
    if expr.kind == ExprKind::MethodCall {
        match expr.value.as_str() {
            "map" => Ok(Pipeline::Map { source: decompose_source(world, ch[0].id)?, constructor_id: ch[1].id }),
            "flatMap" => Ok(Pipeline::FlatMap { source: decompose_source(world, ch[0].id)?, inner: Box::new(decompose_pipeline(world, ch[1].id)?) }),
            "chain" => Ok(Pipeline::Chain { left: Box::new(decompose_pipeline(world, ch[0].id)?), right: Box::new(decompose_pipeline(world, ch[1].id)?) }),
            other => Err(format!("unexpected pipeline op: {}", other))
        }
    } else { Err(format!("expected MethodCall in pipeline, got {:?}", expr.kind)) }
}

fn decompose_source(world: &World, id: i64) -> Result<Source, String> {
    let expr = find_expr_by_id(world, id)?;
    let ch = child_exprs_sorted(world, id);
    if expr.kind == ExprKind::MethodCall && expr.value == "filter" {
        let mut s = decompose_source(world, ch[0].id)?;
        s.filter_ids.push(ch[1].id);
        Ok(s)
    } else if expr.kind == ExprKind::Access {
        Ok(Source { collection: snake(&expr.value), filter_ids: vec![] })
    } else { Err(format!("unexpected source: {:?}", expr.kind)) }
}

fn emit_pipeline_loops(out: &mut String, world: &World, node: &Pipeline, rv: &str, indent: usize, bindings: &mut Vec<DeriveBinding>) -> Result<(), String> {
    let ind = "    ".repeat(indent);
    match node {
        Pipeline::Map { source, constructor_id } => {
            let lv = determine_loop_var(world, source, Some(*constructor_id), bindings);
            let et = elem_type_for(world, &source.collection);
            out.push_str(&format!("{ind}for {lv} in &self.{} {{\n", source.collection));
            bindings.push(DeriveBinding { var_name: lv.clone(), type_name: et });
            let fc = source.filter_ids.len();
            for (i, &fid) in source.filter_ids.iter().enumerate() {
                let c = translate_derive_expr(world, fid, bindings, None)?;
                out.push_str(&format!("{}if {} {{\n", "    ".repeat(indent+1+i), c));
            }
            let v = translate_derive_expr(world, *constructor_id, bindings, None)?;
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
    let expr = find_expr_by_id(world, id)?;
    let ch = child_exprs_sorted(world, id);
    match expr.kind {
        ExprKind::BinOp => {
            let op = &expr.value;
            let lt = derive_expr_type(world, ch[0].id, bindings);
            let l = translate_derive_expr(world, ch[0].id, bindings, None)?;
            let r = translate_derive_expr(world, ch[1].id, bindings, lt.as_deref())?;
            if op == "!=" && r == "\"\"" { return Ok(format!("!{l}.is_empty()")); }
            if op == "==" && r == "false" { return Ok(format!("!{l}")); }
            if is_cmp(op) { Ok(format!("{l} {op} {r}")) } else { Ok(format!("({l} {op} {r})")) }
        }
        ExprKind::Access => {
            let f = &expr.value;
            let b = translate_derive_expr(world, ch[0].id, bindings, None)?;
            match f.as_str() {
                "beforeColon" => Ok(format!("{b}[..{b}.find(':').unwrap()].to_string()")),
                "afterColon" => Ok(format!("{b}[{b}.find(':').unwrap()+1..].to_string()")),
                _ => Ok(format!("{b}.{}", snake(f)))
            }
        }
        ExprKind::InstanceRef => {
            if expr.value == "Self" { Ok("self".into()) } else { Ok(snake(&expr.value)) }
        }
        ExprKind::BareName => {
            match expr.value.as_str() {
                "True" => Ok("true".into()),
                "False" => Ok("false".into()),
                name => {
                    // Use type hint for disambiguation when a variant name exists in multiple domains
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
        ExprKind::IntLit => Ok(expr.value.clone()),
        ExprKind::StringLit => {
            if expr.value.contains("$@") {
                translate_interpolated(&expr.value, bindings)
            } else { Ok(format!("\"{}\"", expr.value)) }
        }
        ExprKind::MethodCall => {
            let b = translate_derive_expr(world, ch[0].id, bindings, None)?;
            let method = &expr.value;
            if method == "contains" {
                let a = translate_derive_expr(world, ch[1].id, bindings, None)?;
                return Ok(format!("{b}.contains({a})"));
            }
            let args: Vec<String> = ch[1..].iter()
                .map(|c| translate_derive_expr(world, c.id, bindings, None))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(format!("{b}.{}({})", snake(method), args.join(", ")))
        }
        ExprKind::StructConstruct => {
            let tn = &expr.value;
            let mut fs = Vec::new();
            for fe in &ch {
                if fe.kind != ExprKind::StructField { continue; }
                let rf = snake(&fe.value);
                let vc = child_exprs_sorted(world, fe.id);
                let ft = get_struct_field_type_for(world, tn, &fe.value);
                let v = translate_derive_expr(world, vc[0].id, bindings, ft.as_deref())?;
                let vs = if ft.as_deref() == Some("String") { format!("{v}.clone()") } else { v };
                fs.push(format!("{rf}: {vs}"));
            }
            Ok(format!("{tn} {{ {} }}", fs.join(", ")))
        }
        _ => Err(format!("unhandled derive expr: {:?}", expr.kind))
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
    let Some(e) = world.exprs.iter().find(|e| e.id == id) else { return vec![] };
    let mut r = Vec::new();
    if e.kind == ExprKind::InstanceRef { r.push(e.value.clone()); }
    for c in child_exprs_sorted(world, id) { r.extend(scan_refs(world, c.id)); }
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
    let e = world.exprs.iter().find(|e| e.id == id)?;
    if e.kind == ExprKind::Access {
        let ch = child_exprs_sorted(world, id);
        let base = ch.first()?;
        if base.kind == ExprKind::InstanceRef && base.value != "Self" {
            let b = bindings.iter().find(|b| b.var_name == snake(&base.value))?;
            let fields = aski_core::query_struct_fields(world, &b.type_name);
            for (_, fname, ftype) in &fields {
                if fname == &e.value { return Some(ftype.clone()); }
            }
        }
    }
    None
}

fn get_struct_field_type_for(world: &World, sn: &str, fn_: &str) -> Option<String> {
    let fields = aski_core::query_struct_fields(world, sn);
    for (_, fname, ftype) in &fields {
        if fname == fn_ { return Some(ftype.clone()); }
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

/// Extract the target field name from a MutableSet statement.
/// The MutableSet stores the binding name (e.g., "Self") in its value.
/// The actual field is in the Access chain child: Access(inner, "FieldName").
fn extract_set_field_from_world(world: &World, stmt_id: i64) -> String {
    let ch = child_exprs_sorted(world, stmt_id);
    if let Some(child) = ch.first() {
        if child.kind == ExprKind::Access {
            return snake(&child.value);
        }
    }
    // Fallback: parse from value string (old codegen_kernel behavior)
    if let Some(expr) = world.exprs.iter().find(|e| e.id == stmt_id) {
        let parts: Vec<&str> = expr.value.split('.').collect();
        if parts.len() >= 2 { return snake(parts[1]); }
    }
    "unknown".to_string()
}

fn child_exprs_sorted<'a>(world: &'a World, parent_id: i64) -> Vec<&'a aski_core::Expr> {
    let mut ch: Vec<_> = world.exprs.iter()
        .filter(|e| e.parent_id == parent_id && parent_id != 0)
        .collect();
    ch.sort_by_key(|e| e.ordinal);
    ch
}

fn find_expr_by_id(world: &World, id: i64) -> Result<&aski_core::Expr, String> {
    world.exprs.iter().find(|e| e.id == id)
        .ok_or_else(|| format!("expr {} not found", id))
}
