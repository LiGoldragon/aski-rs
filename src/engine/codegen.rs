//! Codegen — SemaWorld → Rust source.
//! Reads only typed relations and expression trees from SemaWorld.
//! No parse tree references — SemaWorld is self-contained.

use super::sema_world::*;

pub trait Codegen {
    fn codegen(&self) -> String;
}

impl Codegen for SemaWorld {
    fn codegen(&self) -> String {
        let mut out = String::new();

        for sema_type in &self.types {
            match sema_type.form {
                SemaTypeForm::Domain => self.emit_domain(&mut out, sema_type),
                SemaTypeForm::Struct => self.emit_struct(&mut out, sema_type),
                SemaTypeForm::Alias => {}
            }
        }

        for decl in &self.trait_decls {
            self.emit_trait_decl(&mut out, decl);
        }

        for imp in &self.trait_impls {
            self.emit_trait_impl(&mut out, imp);
        }

        for constant in &self.constants {
            self.emit_const(&mut out, constant);
        }

        if let Some(body) = &self.process_body {
            out.push_str("fn main() {\n");
            self.emit_body(&mut out, body, 1);
            out.push_str("}\n");
        }

        out
    }
}

/// Emit trait — all code generation methods on SemaWorld.
trait Emit {
    fn emit_domain(&self, out: &mut String, sema_type: &SemaType);
    fn emit_struct(&self, out: &mut String, sema_type: &SemaType);
    fn emit_trait_decl(&self, out: &mut String, decl: &SemaTraitDecl);
    fn emit_trait_impl(&self, out: &mut String, imp: &SemaTraitImpl);
    fn emit_const(&self, out: &mut String, constant: &SemaConst);
    fn emit_method_sig(&self, out: &mut String, method_name: &str, params: &[SemaParam], ret: &str, indent: usize);
    fn emit_body(&self, out: &mut String, body: &SemaBody, indent: usize);
    fn emit_stmts(&self, out: &mut String, stmts: &[SemaStatement], indent: usize);
    fn emit_stmt(&self, out: &mut String, stmt: &SemaStatement, indent: usize);
    fn emit_match_body(&self, out: &mut String, target: &Option<SemaExpr>, arms: &[SemaMatchArm], indent: usize);
    fn emit_match_arm(&self, out: &mut String, arm: &SemaMatchArm, indent: usize);
    fn emit_expr(&self, out: &mut String, expr: &SemaExpr);
}

impl Emit for SemaWorld {
    fn emit_domain(&self, out: &mut String, sema_type: &SemaType) {
        let name = &self.type_names[sema_type.name as usize];
        let variants: Vec<_> = self.variants.iter()
            .filter(|v| v.type_id == sema_type.name)
            .collect();

        let has_data = variants.iter().any(|v| v.wraps >= 0);
        let has_float = variants.iter().any(|v| {
            if v.wraps >= 0 {
                let inner = &self.type_names[v.wraps as usize];
                matches!(inner.as_str(), "F32" | "F64")
            } else {
                false
            }
        });
        let first_is_unit = variants.first().map(|v| v.wraps < 0).unwrap_or(true);

        // Derive traits based on contained types
        let mut derives = vec!["Debug", "Clone"];
        if !has_data { derives.push("Copy"); }
        if !has_float { derives.push("PartialEq"); derives.push("Eq"); }
        else { derives.push("PartialEq"); }
        if first_is_unit { derives.insert(0, "Default"); }
        out.push_str(&format!("#[derive({})]\n", derives.join(", ")));

        out.push_str(&format!("pub enum {} {{\n", name));
        for (i, var) in variants.iter().enumerate() {
            let var_name = &self.variant_names[var.name as usize];
            if i == 0 && first_is_unit { out.push_str("    #[default]\n"); }
            if var.wraps >= 0 {
                let inner = &self.type_names[var.wraps as usize];
                out.push_str(&format!("    {}({}),\n", var_name, self.rust_type(inner)));
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

    fn emit_struct(&self, out: &mut String, sema_type: &SemaType) {
        let name = &self.type_names[sema_type.name as usize];
        let fields: Vec<_> = self.fields.iter()
            .filter(|f| f.type_id == sema_type.name)
            .collect();

        out.push_str("#[derive(Debug, Clone, PartialEq, Eq)]\n");
        out.push_str(&format!("pub struct {} {{\n", name));
        for field in &fields {
            let field_name = &self.field_names[field.name as usize];
            let field_type = self.rust_type(&field.field_type);
            out.push_str(&format!("    pub {}: {},\n", self.snake(field_name), field_type));
        }
        out.push_str("}\n\n");
    }

    fn emit_trait_decl(&self, out: &mut String, decl: &SemaTraitDecl) {
        let name = &self.trait_names[decl.name as usize];
        let trait_name = self.pascal(name);
        out.push_str(&format!("pub trait {} {{\n", trait_name));
        for sig in &decl.method_sigs {
            let method_name = &self.method_names[sig.name as usize];
            self.emit_method_sig(out, method_name, &sig.params, &sig.return_type, 1);
            out.push_str(";\n");
        }
        out.push_str("}\n\n");
    }

    fn emit_trait_impl(&self, out: &mut String, imp: &SemaTraitImpl) {
        let trait_name = &self.trait_names[imp.trait_id as usize];
        let type_name = &self.type_names[imp.type_id as usize];
        let trait_pascal = self.pascal(trait_name);

        out.push_str(&format!("impl {} for {} {{\n", trait_pascal, type_name));
        for method in &imp.methods {
            let method_name = &self.method_names[method.name as usize];
            self.emit_method_sig(out, method_name, &method.params, &method.return_type, 1);
            out.push_str(" {\n");
            self.emit_body(out, &method.body, 2);
            out.push_str("    }\n");
        }
        out.push_str("}\n\n");
    }

    fn emit_const(&self, out: &mut String, constant: &SemaConst) {
        let typ = self.rust_type(&constant.typ);
        out.push_str(&format!("pub const {}: {} = ", self.screaming_snake(&constant.name), typ));
        self.emit_expr(out, &constant.value);
        out.push_str(";\n\n");
    }

    fn emit_method_sig(&self, out: &mut String, method_name: &str, params: &[SemaParam], ret: &str, indent: usize) {
        let pad = "    ".repeat(indent);
        let ret_type = if ret.is_empty() { "()" } else { &self.rust_type(ret) };

        out.push_str(&format!("{}fn {}(", pad, self.snake(method_name)));

        let mut first = true;
        for param in params {
            if !first { out.push_str(", "); }
            first = false;
            match (&param.borrow, param.name.as_str()) {
                (ParamBorrow::Immutable, "self") => out.push_str("&self"),
                (ParamBorrow::Mutable, "self") => out.push_str("&mut self"),
                (ParamBorrow::Owned, "self") => out.push_str("self"),
                (ParamBorrow::Immutable, name) => {
                    let typ = if param.typ.is_empty() { "Self" } else { &param.typ };
                    out.push_str(&format!("{}: &{}", self.snake(name), self.rust_type(typ)));
                }
                (ParamBorrow::Mutable, name) => {
                    let typ = if param.typ.is_empty() { "Self" } else { &param.typ };
                    out.push_str(&format!("{}: &mut {}", self.snake(name), self.rust_type(typ)));
                }
                (ParamBorrow::Owned, name) => {
                    let typ = if param.typ.is_empty() { "Self" } else { &param.typ };
                    out.push_str(&format!("{}: {}", self.snake(name), self.rust_type(typ)));
                }
            }
        }

        out.push_str(&format!(") -> {}", self.rust_type(ret_type)));
    }

    fn emit_body(&self, out: &mut String, body: &SemaBody, indent: usize) {
        match body {
            SemaBody::Empty => {
                let pad = "    ".repeat(indent);
                out.push_str(&format!("{}todo!()\n", pad));
            }
            SemaBody::Block(stmts) => {
                self.emit_stmts(out, stmts, indent);
            }
            SemaBody::MatchBody { target, arms } => {
                self.emit_match_body(out, target, arms, indent);
            }
        }
    }

    fn emit_stmts(&self, out: &mut String, stmts: &[SemaStatement], indent: usize) {
        for stmt in stmts {
            self.emit_stmt(out, stmt, indent);
        }
    }

    fn emit_stmt(&self, out: &mut String, stmt: &SemaStatement, indent: usize) {
        let pad = "    ".repeat(indent);
        match stmt {
            SemaStatement::Expr(expr) => {
                out.push_str(&pad);
                self.emit_expr(out, expr);
                out.push('\n');
            }
            SemaStatement::Allocation { name, typ, init } => {
                out.push_str(&format!("{}let {}", pad, self.snake(name)));
                if let Some(t) = typ {
                    out.push_str(&format!(": {}", self.rust_type(t)));
                }
                if let Some(init_expr) = init {
                    out.push_str(" = ");
                    self.emit_expr(out, init_expr);
                }
                out.push_str(";\n");
            }
            SemaStatement::MutAllocation { name, typ, init } => {
                out.push_str(&format!("{}let mut {}", pad, self.snake(name)));
                if let Some(t) = typ {
                    out.push_str(&format!(": {}", self.rust_type(t)));
                }
                if let Some(init_expr) = init {
                    out.push_str(" = ");
                    self.emit_expr(out, init_expr);
                }
                out.push_str(";\n");
            }
            SemaStatement::Mutation { target, method, args } => {
                out.push_str(&format!("{}{}.{}(", pad, self.snake(target), self.snake(method)));
                for (i, arg) in args.iter().enumerate() {
                    if i > 0 { out.push_str(", "); }
                    self.emit_expr(out, arg);
                }
                out.push_str(");\n");
            }
            SemaStatement::Iteration { source, body } => {
                out.push_str(&format!("{}for item in ", pad));
                self.emit_expr(out, source);
                out.push_str(" {\n");
                self.emit_stmts(out, body, indent + 1);
                out.push_str(&format!("{}}}\n", pad));
            }
        }
    }

    fn emit_match_body(&self, out: &mut String, target: &Option<SemaExpr>, arms: &[SemaMatchArm], indent: usize) {
        let pad = "    ".repeat(indent);
        out.push_str(&format!("{}match ", pad));
        if let Some(target_expr) = target {
            self.emit_expr(out, target_expr);
        } else {
            out.push_str("self");
        }
        out.push_str(" {\n");
        for arm in arms {
            self.emit_match_arm(out, arm, indent + 1);
        }
        out.push_str(&format!("{}}}\n", pad));
    }

    fn emit_match_arm(&self, out: &mut String, arm: &SemaMatchArm, indent: usize) {
        let pad = "    ".repeat(indent);
        out.push_str(&pad);

        for (i, pat) in arm.patterns.iter().enumerate() {
            if i > 0 { out.push_str(" | "); }
            self.emit_pattern(out, pat);
        }

        out.push_str(" => ");
        self.emit_expr(out, &arm.result);
        out.push_str(",\n");
    }

    fn emit_expr(&self, out: &mut String, expr: &SemaExpr) {
        match expr {
            SemaExpr::IntLit(n) => out.push_str(&n.to_string()),
            SemaExpr::FloatLit(s) => out.push_str(s),
            SemaExpr::StringLit(s) => out.push_str(&format!("\"{}\"", s)),
            SemaExpr::SelfRef => out.push_str("self"),
            SemaExpr::InstanceRef(name) => out.push_str(&self.snake(name)),
            SemaExpr::QualifiedVariant { domain, variant } => {
                let d = self.type_names.get(*domain as usize).map(|s| s.as_str()).unwrap_or("?");
                let v = self.variant_names.get(*variant as usize).map(|s| s.as_str()).unwrap_or("?");
                out.push_str(&format!("{}::{}", d, v));
            }
            SemaExpr::BareName(name) => out.push_str(name),
            SemaExpr::TypePath(path) => out.push_str(&path.replace(':', "::")),
            SemaExpr::BinOp { op, lhs, rhs } => {
                self.emit_expr(out, lhs);
                out.push_str(&format!(" {} ", op));
                self.emit_expr(out, rhs);
            }
            SemaExpr::FieldAccess { object, field } => {
                self.emit_expr(out, object);
                out.push('.');
                out.push_str(&self.snake(field));
            }
            SemaExpr::MethodCall { object, method, args } => {
                self.emit_expr(out, object);
                out.push('.');
                out.push_str(&self.snake(method));
                out.push('(');
                for (i, arg) in args.iter().enumerate() {
                    if i > 0 { out.push_str(", "); }
                    self.emit_expr(out, arg);
                }
                out.push(')');
            }
            SemaExpr::Group(inner) => {
                out.push('(');
                self.emit_expr(out, inner);
                out.push(')');
            }
            SemaExpr::Return(inner) => {
                // Unwrap redundant Group inside Return: ^(expr) → expr
                match inner.as_ref() {
                    SemaExpr::Group(g) => self.emit_expr(out, g),
                    _ => self.emit_expr(out, inner),
                }
            }
            SemaExpr::InlineEval(stmts) => {
                out.push_str("{\n");
                self.emit_stmts(out, stmts, 0);
                out.push('}');
            }
            SemaExpr::MatchExpr { target, arms } => {
                let target_ref = target.as_ref().map(|t| t.as_ref().clone());
                self.emit_match_body(out, &target_ref, arms, 0);
            }
            SemaExpr::StructConstruct { type_name, fields } => {
                out.push_str(&format!("{} {{ ", type_name));
                for (i, (name, val)) in fields.iter().enumerate() {
                    if i > 0 { out.push_str(", "); }
                    out.push_str(&format!("{}: ", self.snake(name)));
                    self.emit_expr(out, val);
                }
                out.push_str(" }");
            }
        }
    }
}

/// PatternEmit — separated because Emit trait can't be extended with more methods.
trait PatternEmit {
    fn emit_pattern(&self, out: &mut String, pat: &SemaPattern);
}

impl PatternEmit for SemaWorld {
    fn emit_pattern(&self, out: &mut String, pat: &SemaPattern) {
        match pat {
            SemaPattern::Variant(idx) => {
                let name = self.variant_names.get(*idx as usize).map(|s| s.as_str()).unwrap_or("_");
                out.push_str(&format!("Self::{}", name));
            }
            SemaPattern::Or(pats) => {
                for (i, p) in pats.iter().enumerate() {
                    if i > 0 { out.push_str(" | "); }
                    self.emit_pattern(out, p);
                }
            }
        }
    }
}

/// Helpers on SemaWorld — name transformations.
trait NameTransform {
    fn snake(&self, name: &str) -> String;
    fn pascal(&self, name: &str) -> String;
    fn screaming_snake(&self, name: &str) -> String;
    fn rust_type(&self, aski_type: &str) -> String;
}

impl NameTransform for SemaWorld {
    fn snake(&self, name: &str) -> String {
        let mut result = String::new();
        for (i, c) in name.chars().enumerate() {
            if c.is_uppercase() && i > 0 { result.push('_'); }
            result.push(c.to_lowercase().next().unwrap_or(c));
        }
        result
    }

    fn pascal(&self, name: &str) -> String {
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

    fn screaming_snake(&self, name: &str) -> String {
        let mut result = String::new();
        for (i, c) in name.chars().enumerate() {
            if c.is_uppercase() && i > 0 { result.push('_'); }
            result.push(c.to_uppercase().next().unwrap_or(c));
        }
        result
    }

    fn rust_type(&self, aski_type: &str) -> String {
        match aski_type {
            "U8" => "u8".into(), "U16" => "u16".into(),
            "U32" => "u32".into(), "U64" => "u64".into(),
            "I8" => "i8".into(), "I16" => "i16".into(),
            "I32" => "i32".into(), "I64" => "i64".into(),
            "F32" => "f32".into(), "F64" => "f64".into(),
            "String" => "String".into(), "Bool" => "bool".into(),
            "" | "()" => "()".into(),
            other => other.to_string(),
        }
    }
}
