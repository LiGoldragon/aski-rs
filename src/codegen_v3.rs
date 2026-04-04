//! Codegen v3 — reads kernel relations only. No hand-built maps.
//! Every query goes through aski_core. If a query is complex,
//! the kernel needs a new derived relation.

use aski_core::{self, World};

pub fn generate(world: &World) -> Result<String, String> {
    let mut out = String::new();

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
        _ if t.starts_with("Vec{") && t.ends_with('}') =>
            return format!("Vec<{}>", rust_type(&t[4..t.len()-1])),
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
    if let Some((domain, _)) = aski_core::query_variant_domain(world, name) {
        format!("{domain}::{name}")
    } else {
        name.to_string()
    }
}

// ── Domain ──

fn emit_domain(out: &mut String, world: &World, name: &str) -> Result<(), String> {
    let variants = aski_core::query_domain_variants(world, name);
    out.push_str(&format!("#[derive(Debug, Clone, Copy, PartialEq, Eq)]\npub enum {name} {{\n"));
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

fn emit_struct(out: &mut String, world: &World, name: &str) -> Result<(), String> {
    let fields = aski_core::query_struct_fields(world, name);
    let recursive = aski_core::query_recursive_fields(world);
    out.push_str(&format!("#[derive(Debug, Clone, PartialEq)]\npub struct {name} {{\n"));
    for (_, fname, ftype) in &fields {
        let sname = snake(fname);
        let rt = rust_type(ftype);
        if recursive.contains(&(name.to_string(), fname.clone())) {
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
        out.push_str(&format!("impl {} for {target_type} {{\n", rust_trait(&trait_name)));

        let methods = aski_core::query_child_nodes(world, *child_id);
        for (mid, mkind, mname) in &methods {
            if mkind != "method" && mkind != "tail_method" { continue; }
            let params = aski_core::query_params(world, *mid);
            let ret = aski_core::query_return_type(world, *mid);
            let ret_str = ret.map(|r| format!(" -> {}", rust_type(&r))).unwrap_or_default();
            out.push_str(&format!("    fn {}({}){ret_str} {{\n", snake(mname), emit_params(&params)));

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
        let pat = patterns.first().map(|p| qualify(world, p)).unwrap_or("_".into());
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
                        out.push_str(&format!("{indent}let {svar} = {val};\n"));
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
            let args: Vec<String> = children.iter().skip(1)
                .map(|(cid, _, _, _)| emit_expr(world, *cid))
                .collect::<Result<_, _>>()?;
            let s = snake(&method);
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
                    let pat = pats.first().map(|p| qualify(world, p)).unwrap_or("_".into());
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
