//! Codegen — Sema → Rust source.
//! Reads typed ordinals from Sema, resolves names via ResolveName trait.
//! Emits name enums (TypeName, VariantName, etc.) + domain types + traits.

use super::sema::*;

pub struct CodegenContext<'a> {
    pub sema: &'a Sema,
    pub names: &'a dyn ResolveName,
}

pub trait Codegen {
    fn codegen(&self) -> String;
}

impl<'a> Codegen for CodegenContext<'a> {
    fn codegen(&self) -> String {
        let mut out = String::new();

        // 1. Name enums
        self.emit_name_enum(&mut out, "TypeName", self.names.type_count(), |i| self.names.type_name(TypeName(i as u32)));
        self.emit_name_enum(&mut out, "VariantName", self.names.variant_count(), |i| self.names.variant_name(VariantName(i as u32)));
        if self.names.field_count() > 0 {
            self.emit_name_enum(&mut out, "FieldName", self.names.field_count(), |i| self.names.field_name(FieldName(i as u32)));
        }
        if self.names.trait_count() > 0 {
            self.emit_name_enum(&mut out, "TraitName", self.names.trait_count(), |i| self.names.trait_name(TraitName(i as u32)));
        }
        if self.names.method_count() > 0 {
            self.emit_name_enum(&mut out, "MethodName", self.names.method_count(), |i| self.names.method_name(MethodName(i as u32)));
        }

        // 2. Types
        for sema_type in &self.sema.types {
            match sema_type.form {
                SemaTypeForm::Domain => self.emit_domain(&mut out, sema_type),
                SemaTypeForm::Struct => self.emit_struct(&mut out, sema_type),
                SemaTypeForm::Alias => {}
            }
        }

        // 3. Trait declarations
        for decl in &self.sema.trait_decls {
            self.emit_trait_decl(&mut out, decl);
        }

        // 4. Trait implementations
        for imp in &self.sema.trait_impls {
            self.emit_trait_impl(&mut out, imp);
        }

        // 5. Constants
        for constant in &self.sema.constants {
            self.emit_const(&mut out, constant);
        }

        // 6. Process body (fn main)
        if let Some(body_ref) = &self.sema.process_body {
            out.push_str("fn main() {\n");
            self.emit_body(&mut out, *body_ref, 1);
            out.push_str("}\n");
        }

        out
    }
}

impl<'a> CodegenContext<'a> {
    // ── Name enum emission ───────────────────────────────────────

    fn emit_name_enum(&self, out: &mut String, enum_name: &str, count: usize, resolve: impl Fn(usize) -> &'a str) {
        // Skip Rust keywords and primitive types
        let skip = |name: &str| matches!(name, "Self" | "self" | "u8" | "u16" | "u32" | "u64" | "i8" | "i16" | "i32" | "i64" | "f32" | "f64" | "bool" | "String");

        let names: Vec<&str> = (0..count).map(|i| resolve(i)).filter(|n| !skip(n)).collect();
        if names.is_empty() { return; }

        out.push_str("#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]\n");
        out.push_str(&format!("pub enum {} {{\n", enum_name));
        for name in &names {
            out.push_str(&format!("    {},\n", pascal(name)));
        }
        out.push_str("}\n\n");

        out.push_str(&format!("impl std::fmt::Display for {} {{\n", enum_name));
        out.push_str("    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {\n");
        out.push_str("        match self {\n");
        for name in &names {
            out.push_str(&format!("            Self::{} => write!(f, \"{}\"),\n", pascal(name), name));
        }
        out.push_str("        }\n");
        out.push_str("    }\n");
        out.push_str("}\n\n");
    }

    // ── Domain (enum) ────────────────────────────────────────────

    fn emit_domain(&self, out: &mut String, sema_type: &SemaType) {
        let name = self.names.type_name(sema_type.name);
        let variants: Vec<_> = self.sema.variants.iter()
            .filter(|v| v.type_id == sema_type.name)
            .collect();

        let has_data = variants.iter().any(|v| v.wraps.is_some());
        let has_float = variants.iter().any(|v| {
            v.wraps.map(|w| {
                let inner = self.names.type_name(w);
                matches!(inner, "F32" | "F64")
            }).unwrap_or(false)
        });
        let first_is_unit = variants.first().map(|v| v.wraps.is_none()).unwrap_or(true);

        let mut derives = vec!["Debug", "Clone"];
        if !has_data { derives.push("Copy"); }
        if !has_float { derives.push("PartialEq"); derives.push("Eq"); }
        else { derives.push("PartialEq"); }
        if first_is_unit { derives.insert(0, "Default"); }
        out.push_str(&format!("#[derive({})]\n", derives.join(", ")));

        out.push_str(&format!("pub enum {} {{\n", name));
        for (i, var) in variants.iter().enumerate() {
            let var_name = self.names.variant_name(var.name);
            if i == 0 && first_is_unit { out.push_str("    #[default]\n"); }
            if let Some(wraps) = var.wraps {
                let inner = self.names.type_name(wraps);
                out.push_str(&format!("    {}({}),\n", var_name, rust_type(inner)));
            } else {
                out.push_str(&format!("    {},\n", var_name));
            }
        }
        out.push_str("}\n\n");

        out.push_str(&format!("impl std::fmt::Display for {} {{\n", name));
        out.push_str("    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {\n");
        out.push_str("        write!(f, \"{:?}\", self)\n");
        out.push_str("    }\n");
        out.push_str("}\n\n");
    }

    // ── Struct ───────────────────────────────────────────────────

    fn emit_struct(&self, out: &mut String, sema_type: &SemaType) {
        let name = self.names.type_name(sema_type.name);
        let fields: Vec<_> = self.sema.fields.iter()
            .filter(|f| f.type_id == sema_type.name)
            .collect();

        out.push_str("#[derive(Debug, Clone, PartialEq, Eq)]\n");
        out.push_str(&format!("pub struct {} {{\n", name));
        for field in &fields {
            let field_name = self.names.field_name(field.name);
            let field_type = self.names.type_name(field.field_type);
            out.push_str(&format!("    pub {}: {},\n", snake(field_name), rust_type(field_type)));
        }
        out.push_str("}\n\n");
    }

    // ── Trait declaration ────────────────────────────────────────

    fn emit_trait_decl(&self, out: &mut String, decl: &SemaTraitDecl) {
        let name = pascal(self.names.trait_name(decl.name));
        out.push_str(&format!("pub trait {} {{\n", name));
        for sig in &decl.method_sigs {
            self.emit_method_sig(out, sig.name, &sig.params, sig.return_type, 1);
            out.push_str(";\n");
        }
        out.push_str("}\n\n");
    }

    // ── Trait implementation ─────────────────────────────────────

    fn emit_trait_impl(&self, out: &mut String, imp: &SemaTraitImpl) {
        let trait_name = pascal(self.names.trait_name(imp.trait_id));
        let type_name = self.names.type_name(imp.type_id);

        out.push_str(&format!("impl {} for {} {{\n", trait_name, type_name));
        for method in &imp.methods {
            self.emit_method_sig(out, method.name, &method.params, method.return_type, 1);
            out.push_str(" {\n");
            self.emit_body(out, method.body, 2);
            out.push_str("    }\n");
        }
        out.push_str("}\n\n");
    }

    // ── Constant ─────────────────────────────────────────────────

    fn emit_const(&self, out: &mut String, constant: &SemaConst) {
        let name = self.names.type_name(constant.name);
        let typ = self.names.type_name(constant.typ);
        out.push_str(&format!("pub const {}: {} = ", screaming_snake(name), rust_type(typ)));
        self.emit_expr(out, constant.value);
        out.push_str(";\n\n");
    }

    // ── Method signature ─────────────────────────────────────────

    fn emit_method_sig(&self, out: &mut String, method: MethodName, params: &[SemaParam], ret: Option<TypeName>, indent: usize) {
        let pad = "    ".repeat(indent);
        let method_str = snake(self.names.method_name(method));
        let ret_str = match ret {
            Some(t) => rust_type(self.names.type_name(t)).to_string(),
            None => "()".into(),
        };

        out.push_str(&format!("{}fn {}(", pad, method_str));
        let mut first = true;
        for param in params {
            if !first { out.push_str(", "); }
            first = false;
            let param_name = self.names.method_name(param.name);
            match (&param.borrow, param_name) {
                (ParamBorrow::Immutable, "self") => out.push_str("&self"),
                (ParamBorrow::Mutable, "self") => out.push_str("&mut self"),
                (ParamBorrow::Owned, "self") => out.push_str("self"),
                (borrow, name) => {
                    let typ = param.typ.map(|t| self.names.type_name(t)).unwrap_or("Self");
                    match borrow {
                        ParamBorrow::Immutable => out.push_str(&format!("{}: &{}", snake(name), rust_type(typ))),
                        ParamBorrow::Mutable => out.push_str(&format!("{}: &mut {}", snake(name), rust_type(typ))),
                        ParamBorrow::Owned => out.push_str(&format!("{}: {}", snake(name), rust_type(typ))),
                    }
                }
            }
        }
        out.push_str(&format!(") -> {}", ret_str));
    }

    // ── Body ─────────────────────────────────────────────────────

    fn emit_body(&self, out: &mut String, body_ref: BodyRef, indent: usize) {
        let body = self.sema.arena.body(body_ref);
        match body.clone() {
            SemaBody::Empty => {
                out.push_str(&format!("{}todo!()\n", "    ".repeat(indent)));
            }
            SemaBody::Block(stmts) => {
                for stmt_ref in &stmts {
                    self.emit_stmt(out, *stmt_ref, indent);
                }
            }
            SemaBody::MatchBody { target, arms } => {
                let pad = "    ".repeat(indent);
                out.push_str(&format!("{}match ", pad));
                if let Some(target_ref) = target {
                    self.emit_expr(out, target_ref);
                } else {
                    out.push_str("self");
                }
                out.push_str(" {\n");
                for arm_idx in &arms {
                    self.emit_match_arm(out, *arm_idx, indent + 1);
                }
                out.push_str(&format!("{}}}\n", pad));
            }
        }
    }

    // ── Statement ────────────────────────────────────────────────

    fn emit_stmt(&self, out: &mut String, stmt_ref: StmtRef, indent: usize) {
        let pad = "    ".repeat(indent);
        let stmt = self.sema.arena.stmt(stmt_ref).clone();
        match stmt {
            SemaStatement::Expr(expr_ref) => {
                out.push_str(&pad);
                self.emit_expr(out, expr_ref);
                out.push('\n');
            }
            SemaStatement::Allocation { name, typ, init } => {
                out.push_str(&format!("{}let {}", pad, snake(self.sema.arena.binding(name))));
                if let Some(t) = typ {
                    out.push_str(&format!(": {}", rust_type(self.names.type_name(t))));
                }
                if let Some(init_ref) = init {
                    out.push_str(" = ");
                    self.emit_expr(out, init_ref);
                }
                out.push_str(";\n");
            }
            SemaStatement::MutAllocation { name, typ, init } => {
                out.push_str(&format!("{}let mut {}", pad, snake(self.sema.arena.binding(name))));
                if let Some(t) = typ {
                    out.push_str(&format!(": {}", rust_type(self.names.type_name(t))));
                }
                if let Some(init_ref) = init {
                    out.push_str(" = ");
                    self.emit_expr(out, init_ref);
                }
                out.push_str(";\n");
            }
            SemaStatement::Mutation { target, method, args } => {
                out.push_str(&format!("{}{}.{}(", pad, snake(self.sema.arena.binding(target)), snake(self.names.method_name(method))));
                for (i, arg_ref) in args.iter().enumerate() {
                    if i > 0 { out.push_str(", "); }
                    self.emit_expr(out, *arg_ref);
                }
                out.push_str(");\n");
            }
            SemaStatement::Iteration { source, body } => {
                out.push_str(&format!("{}for item in ", pad));
                self.emit_expr(out, source);
                out.push_str(" {\n");
                for stmt_ref in &body {
                    self.emit_stmt(out, *stmt_ref, indent + 1);
                }
                out.push_str(&format!("{}}}\n", pad));
            }
        }
    }

    // ── Match arm ────────────────────────────────────────────────

    fn emit_match_arm(&self, out: &mut String, arm_idx: u32, indent: usize) {
        let pad = "    ".repeat(indent);
        let arm = self.sema.arena.match_arm(arm_idx).clone();
        out.push_str(&pad);

        for (i, pat) in arm.patterns.iter().enumerate() {
            if i > 0 { out.push_str(" | "); }
            self.emit_pattern(out, pat);
        }

        out.push_str(" => ");
        self.emit_expr(out, arm.result);
        out.push_str(",\n");
    }

    // ── Pattern ──────────────────────────────────────────────────

    fn emit_pattern(&self, out: &mut String, pat: &SemaPattern) {
        match pat {
            SemaPattern::Variant(v) => {
                out.push_str(&format!("Self::{}", self.names.variant_name(*v)));
            }
            SemaPattern::Or(variants) => {
                for (i, v) in variants.iter().enumerate() {
                    if i > 0 { out.push_str(" | "); }
                    out.push_str(&format!("Self::{}", self.names.variant_name(*v)));
                }
            }
        }
    }

    // ── Expression ───────────────────────────────────────────────

    fn emit_expr(&self, out: &mut String, expr_ref: ExprRef) {
        let expr = self.sema.arena.expr(expr_ref).clone();
        match expr {
            SemaExpr::IntLit(n) => out.push_str(&n.to_string()),
            SemaExpr::FloatLit(f) => out.push_str(&format!("{}", f)),
            SemaExpr::StringLit(s) => out.push_str(&format!("\"{}\"", self.names.literal_string(s))),
            SemaExpr::SelfRef => out.push_str("self"),
            SemaExpr::InstanceRef(b) => out.push_str(&snake(self.sema.arena.binding(b))),
            SemaExpr::QualifiedVariant { domain, variant } => {
                out.push_str(&format!("{}::{}", self.names.type_name(domain), self.names.variant_name(variant)));
            }
            SemaExpr::BareName(b) => out.push_str(self.sema.arena.binding(b)),
            SemaExpr::TypePath { typ, member } => {
                out.push_str(&format!("{}::{}", self.names.type_name(typ), self.names.method_name(member)));
            }
            SemaExpr::BinOp { op, lhs, rhs } => {
                self.emit_expr(out, lhs);
                out.push_str(&format!(" {} ", op.as_rust()));
                self.emit_expr(out, rhs);
            }
            SemaExpr::FieldAccess { object, field } => {
                self.emit_expr(out, object);
                out.push('.');
                out.push_str(&snake(self.names.field_name(field)));
            }
            SemaExpr::MethodCall { object, method, args } => {
                self.emit_expr(out, object);
                out.push('.');
                out.push_str(&snake(self.names.method_name(method)));
                out.push('(');
                for (i, arg_ref) in args.iter().enumerate() {
                    if i > 0 { out.push_str(", "); }
                    self.emit_expr(out, *arg_ref);
                }
                out.push(')');
            }
            SemaExpr::Group(inner) => {
                out.push('(');
                self.emit_expr(out, inner);
                out.push(')');
            }
            SemaExpr::Return(inner) => {
                // Unwrap redundant Group inside Return
                let inner_expr = self.sema.arena.expr(inner);
                match inner_expr {
                    SemaExpr::Group(g) => self.emit_expr(out, *g),
                    _ => self.emit_expr(out, inner),
                }
            }
            SemaExpr::InlineEval(stmts) => {
                out.push_str("{\n");
                for stmt_ref in &stmts {
                    self.emit_stmt(out, *stmt_ref, 0);
                }
                out.push('}');
            }
            SemaExpr::MatchExpr { target, arms } => {
                out.push_str("match ");
                if let Some(t) = target {
                    self.emit_expr(out, t);
                } else {
                    out.push_str("self");
                }
                out.push_str(" {\n");
                for arm_idx in &arms {
                    self.emit_match_arm(out, *arm_idx, 0);
                }
                out.push('}');
            }
            SemaExpr::StructConstruct { type_name, fields } => {
                out.push_str(&format!("{} {{ ", self.names.type_name(type_name)));
                for (i, (field, val)) in fields.iter().enumerate() {
                    if i > 0 { out.push_str(", "); }
                    out.push_str(&format!("{}: ", snake(self.names.field_name(*field))));
                    self.emit_expr(out, *val);
                }
                out.push_str(" }");
            }
        }
    }
}

// ── Free helper functions (name transforms) ──────────────────────

fn snake(name: &str) -> String {
    let mut result = String::new();
    for (i, c) in name.chars().enumerate() {
        if c.is_uppercase() && i > 0 { result.push('_'); }
        result.push(c.to_lowercase().next().unwrap_or(c));
    }
    result
}

fn pascal(name: &str) -> String {
    let mut result = String::new();
    let mut capitalize = true;
    for c in name.chars() {
        if capitalize {
            result.push(c.to_uppercase().next().unwrap_or(c));
            capitalize = false;
        } else {
            result.push(c);
        }
    }
    result
}

fn screaming_snake(name: &str) -> String {
    let mut result = String::new();
    for (i, c) in name.chars().enumerate() {
        if c.is_uppercase() && i > 0 { result.push('_'); }
        result.push(c.to_uppercase().next().unwrap_or(c));
    }
    result
}

fn rust_type(aski_type: &str) -> &str {
    match aski_type {
        "U8" => "u8", "U16" => "u16", "U32" => "u32", "U64" => "u64",
        "I8" => "i8", "I16" => "i16", "I32" => "i32", "I64" => "i64",
        "F32" => "f32", "F64" => "f64",
        "String" => "String", "Bool" => "bool",
        "" | "()" => "()",
        other => other,
    }
}
