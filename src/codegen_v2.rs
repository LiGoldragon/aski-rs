//! Codegen v2 — generates Rust by reading resolved World relations.
//! No context threading, no special-casing. struct_construct emits the same
//! Rust whether it appears in Main, a match arm, or a method body.

use crate::ir::{self, World};
use std::collections::HashMap;

pub fn generate(world: &World) -> Result<String, String> {
    let mut out = String::new();
    let nodes = ir::query_all_top_level_nodes(world)?;

    // Build variant→domain map for qualifying variant names
    let mut variant_map: HashMap<String, String> = HashMap::new();
    for (id, kind, name) in &nodes {
        if kind == "domain" {
            for (_, vname, _) in ir::query_domain_variants(world, name)? {
                variant_map.insert(vname.clone(), name.clone());
            }
        }
    }

    // Build struct field map for struct construction
    let mut struct_fields: HashMap<String, Vec<(String, String)>> = HashMap::new();
    for (_id, kind, name) in &nodes {
        if kind == "struct" {
            let fields: Vec<(String, String)> = ir::query_struct_fields(world, name)?
                .into_iter()
                .map(|(_, fname, ftype)| (fname, ftype))
                .collect();
            struct_fields.insert(name.clone(), fields);
        }
    }

    // Sort by kind: types first, traits, impls, main last
    let mut sorted = nodes.clone();
    sorted.sort_by_key(|(_, kind, _)| match kind.as_str() {
        "foreign_block" | "domain" | "struct" | "const" => 0u8,
        "trait" => 1,
        "impl" => 2,
        "main" => 3,
        _ => 4,
    });

    let ctx = EmitCtx { variant_map: &variant_map, struct_fields: &struct_fields };

    for (node_id, kind, name) in &sorted {
        match kind.as_str() {
            "domain" => emit_domain(&mut out, world, name)?,
            "struct" => emit_struct(&mut out, world, name)?,
            "const" => emit_const(&mut out, world, *node_id)?,
            "trait" => emit_trait(&mut out, world, *node_id, name)?,
            "impl" => emit_impl(&mut out, world, *node_id, name, &ctx)?,
            "main" => emit_main(&mut out, world, *node_id, &ctx)?,
            "foreign_block" => emit_foreign_block(&mut out, world, *node_id, name)?,
            _ => {}
        }
    }

    Ok(out)
}

struct EmitCtx<'a> {
    variant_map: &'a HashMap<String, String>,
    struct_fields: &'a HashMap<String, Vec<(String, String)>>,
}

fn to_snake(s: &str) -> String {
    let mut out = String::new();
    for (i, ch) in s.chars().enumerate() {
        if ch.is_uppercase() && i > 0 {
            out.push('_');
        }
        out.push(ch.to_ascii_lowercase());
    }
    out
}

fn to_screaming(s: &str) -> String {
    to_snake(s).to_uppercase()
}

fn type_to_rust(t: &str) -> String {
    match t {
        "F32" => "f32".into(),
        "F64" => "f64".into(),
        "I8" => "i8".into(), "I16" => "i16".into(),
        "I32" => "i32".into(), "I64" => "i64".into(),
        "U8" => "u8".into(), "U16" => "u16".into(),
        "U32" => "u32".into(), "U64" => "u64".into(),
        "Bool" => "bool".into(),
        "String" => "String".into(),
        other => {
            if other.starts_with("Vec{") && other.ends_with('}') {
                let inner = &other[4..other.len()-1];
                format!("Vec<{}>", type_to_rust(inner))
            } else {
                other.to_string()
            }
        }
    }
}

fn trait_to_rust(t: &str) -> String {
    let mut out = String::new();
    let mut capitalize_next = true;
    for ch in t.chars() {
        if capitalize_next {
            out.push(ch.to_ascii_uppercase());
            capitalize_next = false;
        } else {
            out.push(ch);
        }
    }
    out
}

// ── Domain ──

fn emit_domain(out: &mut String, world: &World, name: &str) -> Result<(), String> {
    let variants = ir::query_domain_variants(world, name)?;

    out.push_str(&format!("#[derive(Debug, Clone, Copy, PartialEq, Eq)]\npub enum {name} {{\n"));
    for (_, vname, wraps) in &variants {
        if let Some(w) = wraps {
            out.push_str(&format!("    {vname}({}),\n", type_to_rust(w)));
        } else {
            out.push_str(&format!("    {vname},\n"));
        }
    }
    out.push_str("}\n\n");

    // Display impl
    out.push_str(&format!("impl std::fmt::Display for {name} {{\n"));
    out.push_str("    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {\n");
    out.push_str("        write!(f, \"{:?}\", self)\n");
    out.push_str("    }\n}\n\n");

    Ok(())
}

// ── Struct ──

fn emit_struct(out: &mut String, world: &World, name: &str) -> Result<(), String> {
    let fields = ir::query_struct_fields(world, name)?;

    out.push_str(&format!("#[derive(Debug, Clone, PartialEq)]\npub struct {name} {{\n"));
    for (_, fname, ftype) in &fields {
        out.push_str(&format!("    pub {}: {},\n", to_snake(fname), type_to_rust(ftype)));
    }
    out.push_str("}\n\n");

    Ok(())
}

// ── Constant ──

fn emit_const(out: &mut String, world: &World, node_id: i64) -> Result<(), String> {
    let (name, type_ref, has_value) = ir::query_constant(world, node_id)?
        .ok_or("constant not found")?;
    let rust_type = type_to_rust(&type_ref);
    let screaming = to_screaming(&name);

    if has_value {
        let children = ir::query_child_exprs(world, node_id)?;
        if let Some((child_id, _, _, _)) = children.first() {
            let val = emit_expr(world, *child_id, &EmitCtx { variant_map: &HashMap::new(), struct_fields: &HashMap::new() })?;
            out.push_str(&format!("pub const {screaming}: {rust_type} = {val};\n\n"));
        }
    }

    Ok(())
}

// ── Trait declaration ──

fn emit_trait(out: &mut String, world: &World, node_id: i64, name: &str) -> Result<(), String> {
    let rust_name = trait_to_rust(name);
    let children = ir::query_child_nodes(world, node_id)?;

    out.push_str(&format!("pub trait {rust_name} {{\n"));
    for (child_id, kind, child_name) in &children {
        if kind == "method_sig" {
            let params = ir::query_params(world, *child_id)?;
            let ret = ir::query_return_type(world, *child_id)?;
            let rust_params = emit_params(&params);
            let ret_str = ret.map(|r| format!(" -> {}", type_to_rust(&r))).unwrap_or_default();
            out.push_str(&format!("    fn {}({rust_params}){ret_str};\n", to_snake(child_name)));
        }
    }
    out.push_str("}\n\n");

    Ok(())
}

// ── Trait impl ──

fn emit_impl(out: &mut String, world: &World, node_id: i64, trait_name: &str, ctx: &EmitCtx) -> Result<(), String> {
    let children = ir::query_child_nodes(world, node_id)?;
    let rust_trait = trait_to_rust(trait_name);

    for (child_id, kind, target_type) in &children {
        if kind != "impl_body" { continue; }
        out.push_str(&format!("impl {rust_trait} for {target_type} {{\n"));

        let methods = ir::query_child_nodes(world, *child_id)?;
        for (method_id, mkind, method_name) in &methods {
            if mkind != "method" { continue; }
            let params = ir::query_params(world, *method_id)?;
            let ret = ir::query_return_type(world, *method_id)?;
            let rust_params = emit_params(&params);
            let ret_str = ret.map(|r| format!(" -> {}", type_to_rust(&r))).unwrap_or_default();

            out.push_str(&format!("    fn {}({rust_params}){ret_str} {{\n", to_snake(method_name)));

            // Check for match arms (matching body)
            let arms = ir::query_match_arms(world, *method_id)?;
            if !arms.is_empty() {
                emit_match_body(out, world, &arms, ctx)?;
            } else {
                // Computed body — emit expressions
                let exprs = ir::query_child_exprs(world, *method_id)?;
                emit_block(out, world, &exprs, ctx, "        ")?;
            }

            out.push_str("    }\n");
        }

        out.push_str("}\n\n");
    }

    Ok(())
}

// ── Main ──

fn emit_main(out: &mut String, world: &World, node_id: i64, ctx: &EmitCtx) -> Result<(), String> {
    out.push_str("pub fn main() {\n");
    let exprs = ir::query_child_exprs(world, node_id)?;
    emit_block(out, world, &exprs, ctx, "    ")?;
    out.push_str("}\n");
    Ok(())
}

// ── Foreign block ──

fn emit_foreign_block(out: &mut String, world: &World, node_id: i64, library: &str) -> Result<(), String> {
    let children = ir::query_child_nodes(world, node_id)?;
    let crate_name = to_snake(library);

    for (child_id, _, child_name) in &children {
        let ret = ir::query_return_type(world, *child_id)?
            .unwrap_or_else(|| "()".to_string());
        let rust_ret = type_to_rust(&ret);
        let params = ir::query_params(world, *child_id)?;

        let exprs = ir::query_child_exprs(world, *child_id)?;
        let extern_name = exprs.iter()
            .find(|(_, kind, _, _)| kind == "extern_name")
            .and_then(|(_, _, _, val)| val.clone())
            .unwrap_or_else(|| child_name.clone());

        let mut param_strs = Vec::new();
        let mut arg_names = Vec::new();
        for (kind, name_opt, type_opt) in &params {
            if let Some(pt) = type_opt {
                let rust_type = type_to_rust(pt);
                let pname = to_snake(name_opt.as_deref().unwrap_or(kind));
                param_strs.push(format!("{pname}: {rust_type}"));
                arg_names.push(pname);
            }
        }

        out.push_str(&format!("pub fn {}({}) -> {rust_ret} {{\n",
            to_snake(child_name), param_strs.join(", ")));
        out.push_str(&format!("    {crate_name}::{}({})\n",
            extern_name, arg_names.join(", ")));
        out.push_str("}\n\n");
    }

    Ok(())
}

// ── Params ──

fn emit_params(params: &[(String, Option<String>, Option<String>)]) -> String {
    let mut parts = Vec::new();
    for (kind, name_opt, type_opt) in params {
        match kind.as_str() {
            "borrow_self" => parts.push("&self".to_string()),
            "mut_borrow_self" => parts.push("&mut self".to_string()),
            "owned_self" => parts.push("self".to_string()),
            "named" => {
                if let (Some(n), Some(t)) = (name_opt, type_opt) {
                    parts.push(format!("{}: &{}", to_snake(n), type_to_rust(t)));
                }
            }
            "borrow" => {
                if let Some(t) = type_opt {
                    let pname = name_opt.as_deref().unwrap_or(t);
                    parts.push(format!("{}: &{}", to_snake(pname), type_to_rust(t)));
                }
            }
            _ => {}
        }
    }
    parts.join(", ")
}

// ── Match body ──

fn emit_match_body(out: &mut String, world: &World, arms: &[(i64, Vec<String>, Option<i64>, String)], ctx: &EmitCtx) -> Result<(), String> {
    out.push_str("        match self {\n");
    for (_ord, patterns, body_id, _kind) in arms {
        let pat = patterns.first().map(|p| emit_pattern(p, ctx)).unwrap_or("_".into());
        if let Some(bid) = body_id {
            let body = emit_expr(world, *bid, ctx)?;
            out.push_str(&format!("            {pat} => {body},\n"));
        }
    }
    out.push_str("        }\n");
    Ok(())
}

fn emit_pattern(pat_str: &str, ctx: &EmitCtx) -> String {
    if pat_str == "_" {
        return "_".to_string();
    }
    // Qualify variant names: Fire → Element::Fire
    if let Some(domain) = ctx.variant_map.get(pat_str) {
        return format!("{domain}::{pat_str}");
    }
    pat_str.to_string()
}

// ── Block (sequence of expressions) ──

fn emit_block(out: &mut String, world: &World, exprs: &[(i64, String, i64, Option<String>)], ctx: &EmitCtx, indent: &str) -> Result<(), String> {
    for (expr_id, kind, _, _) in exprs {
        match kind.as_str() {
            "same_type_new" | "sub_type_new" => {
                let children = ir::query_child_exprs(world, *expr_id)?;
                let (var_name, type_name) = get_binding_info(world, *expr_id)?;
                let snake_var = to_snake(&var_name);
                let rust_type = type_to_rust(&type_name);

                if is_primitive_type(&type_name) {
                    // Primitive: let x: f64 = expr;
                    if let Some((child_id, _, _, _)) = children.first() {
                        let val = emit_expr(world, *child_id, ctx)?;
                        out.push_str(&format!("{indent}let {snake_var}: {rust_type} = {val};\n"));
                    }
                } else if ctx.struct_fields.contains_key(&type_name) {
                    // Struct construction
                    let val = emit_struct_construct(world, &type_name, &children, ctx)?;
                    out.push_str(&format!("{indent}let {snake_var} = {val};\n"));
                } else {
                    // Domain variant or other
                    if let Some((child_id, _, _, _)) = children.first() {
                        let val = emit_expr(world, *child_id, ctx)?;
                        out.push_str(&format!("{indent}let {snake_var}: {rust_type} = {val};\n"));
                    }
                }
            }
            "std_out" => {
                let children = ir::query_child_exprs(world, *expr_id)?;
                if let Some((child_id, _, _, _)) = children.first() {
                    let val = emit_expr(world, *child_id, ctx)?;
                    out.push_str(&format!("{indent}println!(\"{{}}\", {val});\n"));
                }
            }
            "return" => {
                let children = ir::query_child_exprs(world, *expr_id)?;
                if let Some((child_id, _, _, _)) = children.first() {
                    let val = emit_expr(world, *child_id, ctx)?;
                    out.push_str(&format!("{indent}{val}\n"));
                }
            }
            _ => {
                // Try to emit as expression statement
                let val = emit_expr(world, *expr_id, ctx)?;
                if !val.is_empty() {
                    out.push_str(&format!("{indent}{val};\n"));
                }
            }
        }
    }
    Ok(())
}

// ── Expression emission — THE UNIFORM PATH ──
// This is the key difference from v1: every expression emits the same
// way regardless of where it appears in the tree.

fn emit_expr(world: &World, expr_id: i64, ctx: &EmitCtx) -> Result<String, String> {
    let (kind, value) = ir::query_expr_by_id(world, expr_id)?
        .ok_or_else(|| format!("expr {} not found", expr_id))?;

    match kind.as_str() {
        "int_lit" => Ok(value.unwrap_or("0".into())),
        "float_lit" => Ok(value.unwrap_or("0.0".into())),
        "string_lit" => Ok(format!("\"{}\"", value.unwrap_or_default())),
        "const_ref" => Ok(to_screaming(&value.unwrap_or_default())),
        "instance_ref" => Ok(to_snake(&value.unwrap_or_default())),

        "bare_name" => {
            let name = value.unwrap_or_default();
            // Qualify variant names
            if let Some(domain) = ctx.variant_map.get(&name) {
                Ok(format!("{domain}::{name}"))
            } else {
                Ok(name)
            }
        }

        "bin_op" => {
            let children = ir::query_child_exprs(world, expr_id)?;
            let op = value.unwrap_or("+".into());
            if children.len() >= 2 {
                let left = emit_expr(world, children[0].0, ctx)?;
                let right = emit_expr(world, children[1].0, ctx)?;
                Ok(format!("({left} {op} {right})"))
            } else {
                Ok("todo!()".into())
            }
        }

        "inline_eval" => {
            let children = ir::query_child_exprs(world, expr_id)?;
            if children.len() == 1 {
                emit_expr(world, children[0].0, ctx)
            } else {
                // Multi-expression: wrap in block
                let mut parts = Vec::new();
                for (cid, _, _, _) in &children {
                    parts.push(emit_expr(world, *cid, ctx)?);
                }
                Ok(parts.last().cloned().unwrap_or_default())
            }
        }

        "access" => {
            let name = value.unwrap_or_default();
            let children = ir::query_child_exprs(world, expr_id)?;
            if let Some((child_id, _, _, _)) = children.first() {
                let base = emit_expr(world, *child_id, ctx)?;
                let snake = to_snake(&name);
                // Check if this is a method call (no args) or field access
                // If name is camelCase (starts lowercase), it's a method
                if name.chars().next().map(|c| c.is_lowercase()).unwrap_or(false) {
                    Ok(format!("{base}.{snake}()"))
                } else {
                    Ok(format!("{base}.{snake}"))
                }
            } else {
                Ok(format!("self.{}", to_snake(&name)))
            }
        }

        "method_call" => {
            let method = value.unwrap_or_default();
            let children = ir::query_child_exprs(world, expr_id)?;
            if children.is_empty() {
                return Ok(format!("self.{}()", to_snake(&method)));
            }
            let base = emit_expr(world, children[0].0, ctx)?;
            let mut args = Vec::new();
            for (cid, _, _, _) in children.iter().skip(1) {
                args.push(emit_expr(world, *cid, ctx)?);
            }
            let snake = to_snake(&method);
            if args.is_empty() {
                Ok(format!("{base}.{snake}()"))
            } else {
                Ok(format!("{base}.{snake}({})", args.join(", ")))
            }
        }

        // THE KEY: struct_construct emits the same everywhere
        "struct_construct" => {
            let name = value.unwrap_or_default();
            let children = ir::query_child_exprs(world, expr_id)?;

            // Check if it's a variant
            if let Some(domain) = ctx.variant_map.get(&name) {
                if children.is_empty() {
                    return Ok(format!("{domain}::{name}"));
                }
                if children.len() == 1 {
                    let inner = emit_expr(world, children[0].0, ctx)?;
                    return Ok(format!("{domain}::{name}({inner})"));
                }
            }

            // Struct literal: Name { field: val, ... }
            let mut field_inits = Vec::new();
            for (child_id, _kind, _ord, child_val) in &children {
                let fname = to_snake(child_val.as_deref().unwrap_or(""));
                let inner = ir::query_child_exprs(world, *child_id)?;
                if let Some((val_id, _, _, _)) = inner.first() {
                    let val = emit_expr(world, *val_id, ctx)?;
                    field_inits.push(format!("{fname}: {val}"));
                }
            }
            Ok(format!("{name} {{ {} }}", field_inits.join(", ")))
        }

        "group" => {
            let children = ir::query_child_exprs(world, expr_id)?;
            if let Some((child_id, _, _, _)) = children.first() {
                let val = emit_expr(world, *child_id, ctx)?;
                Ok(format!("({val})"))
            } else {
                Ok("()".into())
            }
        }

        "return" => {
            let children = ir::query_child_exprs(world, expr_id)?;
            if let Some((child_id, _, _, _)) = children.first() {
                emit_expr(world, *child_id, ctx)
            } else {
                Ok("()".into())
            }
        }

        "fn_call" => {
            let name = value.unwrap_or_default();
            let children = ir::query_child_exprs(world, expr_id)?;
            let mut args = Vec::new();
            for (cid, _, _, _) in &children {
                let arg = emit_expr(world, *cid, ctx)?;
                args.push(format!("&{arg}"));
            }
            Ok(format!("{}({})", to_snake(&name), args.join(", ")))
        }

        "match" => {
            let children = ir::query_child_exprs(world, expr_id)?;
            let arms = ir::query_match_arms(world, expr_id)?;
            if let Some((target_id, _, _, _)) = children.first() {
                let target = emit_expr(world, *target_id, ctx)?;
                let mut arm_strs = Vec::new();
                for (_ord, patterns, body_id, _kind) in &arms {
                    let pat = patterns.first().map(|p| emit_pattern(p, ctx)).unwrap_or("_".into());
                    if let Some(bid) = body_id {
                        let body = emit_expr(world, *bid, ctx)?;
                        arm_strs.push(format!("{pat} => {body}"));
                    }
                }
                Ok(format!("match {target} {{ {} }}", arm_strs.join(", ")))
            } else {
                Ok("todo!()".into())
            }
        }

        "stub" => Ok("todo!()".into()),

        other => Ok(format!("/* unhandled: {other} */")),
    }
}

// ── Struct construction from binding children ──

fn emit_struct_construct(world: &World, type_name: &str, children: &[(i64, String, i64, Option<String>)], ctx: &EmitCtx) -> Result<String, String> {
    // Children are struct_construct nodes with field names
    let mut field_inits = Vec::new();
    for (child_id, child_kind, _, child_val) in children {
        if child_kind == "struct_construct" {
            let fname = to_snake(child_val.as_deref().unwrap_or(""));
            let inner = ir::query_child_exprs(world, *child_id)?;
            if let Some((val_id, _, _, _)) = inner.first() {
                let val = emit_expr(world, *val_id, ctx)?;
                field_inits.push(format!("{fname}: {val}"));
            }
        } else {
            // Single expression — maybe a method call result
            let val = emit_expr(world, *child_id, ctx)?;
            return Ok(format!("{val}"));
        }
    }
    if field_inits.is_empty() {
        Ok(format!("{type_name} {{}}"))
    } else {
        Ok(format!("{type_name} {{ {} }}", field_inits.join(", ")))
    }
}

// ── Helpers ──

fn get_binding_info(world: &World, expr_id: i64) -> Result<(String, String), String> {
    let (kind, value) = ir::query_expr_by_id(world, expr_id)?
        .ok_or_else(|| format!("binding expr {} not found", expr_id))?;
    let raw = value.unwrap_or_default();

    // Value is "VarName:TypeName" for sub_type_new, or just "TypeName" for same_type_new
    if let Some(colon) = raw.find(':') {
        let var_name = &raw[..colon];
        let type_name = &raw[colon+1..];
        Ok((var_name.to_string(), type_name.to_string()))
    } else {
        // same_type_new: name IS the type
        Ok((raw.clone(), raw))
    }
}

fn is_primitive_type(t: &str) -> bool {
    matches!(t, "F32" | "F64" | "I8" | "I16" | "I32" | "I64" |
               "U8" | "U16" | "U32" | "U64" | "Bool" | "String")
}
