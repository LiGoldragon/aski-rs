//! Codegen v3 — reads kernel relations only. No hand-built maps.
//! Every query goes through aski_core. If a query is complex,
//! the kernel needs a new derived relation.

use aski_core::{self, World};

pub fn generate(world: &World) -> Result<String, String> {
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
            "domain" => emit_domain(&mut out, world, name)?,
            "struct" => emit_struct(&mut out, world, name)?,
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

fn emit_domain(out: &mut String, world: &World, name: &str) -> Result<(), String> {
    let variants = aski_core::query_domain_variants(world, name);
    let has_data = variants.iter().any(|(_, _, wraps)| wraps.is_some());
    let derives = if has_data {
        "#[derive(Debug, Clone, PartialEq, Eq)]"
    } else {
        "#[derive(Debug, Clone, Copy, PartialEq, Eq)]"
    };
    out.push_str(&format!("{derives}\npub enum {name} {{\n"));
    for (_, vname, wraps) in &variants {
        match wraps {
            Some(w) => out.push_str(&format!("    {vname}({}),\n", rust_type(w))),
            None => out.push_str(&format!("    {vname},\n")),
        }
    }
    out.push_str("}\n\n");
    out.push_str(&format!("impl std::fmt::Display for {name} {{\n"));
    out.push_str("    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {\n");
    out.push_str("        write!(f, \"{:?}\", self)\n    }\n}\n\n");
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

fn emit_struct(out: &mut String, world: &World, name: &str) -> Result<(), String> {
    let fields = aski_core::query_struct_fields(world, name);
    let all_copy = !fields.is_empty() && fields.iter().all(|(_, fname, ftype)| {
        !aski_core::is_recursive_field(world, name, fname) && is_copy_eligible(ftype, world)
    });
    let derives = if all_copy {
        "#[derive(Debug, Copy, Clone, PartialEq)]"
    } else {
        "#[derive(Debug, Clone, PartialEq, Eq)]"
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
            "mutable_new" | "mutable_set" => {
                let children = aski_core::query_child_exprs(world, *eid);
                if let Some((var_name, type_name)) = aski_core::query_binding_info(world, *eid) {
                    let svar = snake(&var_name);
                    if let Some((cid, _, _, _)) = children.first() {
                        let val = emit_expr(world, *cid)?;
                        if kind == "mutable_new" {
                            out.push_str(&format!("{indent}let mut {svar}: {} = {val};\n", rust_type(&type_name)));
                        } else {
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
                .map(|(cid, _, _, _)| emit_expr(world, *cid))
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
        if ckind == "struct_construct" {
            let fname = snake(cval.as_deref().unwrap_or(""));
            let inner = aski_core::query_child_exprs(world, *cid);
            if let Some((vid, _, _, _)) = inner.first() {
                fields.push(format!("{fname}: {}", emit_expr(world, *vid)?));
            }
        } else {
            return emit_expr(world, *cid);
        }
    }
    if fields.is_empty() { Ok(format!("{type_name} {{}}")) }
    else { Ok(format!("{type_name} {{ {} }}", fields.join(", "))) }
}
