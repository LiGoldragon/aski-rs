//! Raise trait — SemaWorld → AskiWorld.
//! Builds parse nodes from typed relations, reversing the lower step.
//! Used for roundtrip verification: parse → lower → raise → deparse.

use std::collections::HashMap;
use crate::synth::types::Dialect;
use super::aski_world::AskiWorld;
use super::sema_world::*;

pub trait Raise {
    fn raise(sema: &SemaWorld, dialects: HashMap<String, Dialect>) -> AskiWorld;
}

/// Expression raising — SemaExpr → parse nodes.
trait RaiseExpr {
    fn raise_expr(&mut self, parent_id: i64, expr: &SemaExpr);
    fn raise_body(&mut self, parent_id: i64, body: &SemaBody);
    fn raise_statement(&mut self, parent_id: i64, stmt: &SemaStatement);
}

impl Raise for AskiWorld {
    fn raise(sema: &SemaWorld, dialects: HashMap<String, Dialect>) -> AskiWorld {
        let mut world = AskiWorld::new(dialects);
        let root = world.root_id();

        // Rebuild module header
        for module in &sema.modules {
            let mod_name = &sema.module_names[module.name as usize];
            let node_id = world.make_node("{", mod_name, 0, 0);
            world.add_child(root, node_id);
            for export in &module.exports {
                let exp_id = world.make_node("Export", export, 0, 0);
                world.add_child(node_id, exp_id);
            }
        }

        // Rebuild types (domains + structs)
        for sema_type in &sema.types {
            let name = &sema.type_names[sema_type.name as usize];
            match sema_type.form {
                SemaTypeForm::Domain => {
                    let node_id = world.make_node("(", name, 0, 0);
                    world.add_child(root, node_id);
                    world.register_domain(name);

                    let variants: Vec<_> = sema.variants.iter()
                        .filter(|v| v.type_id == sema_type.name)
                        .collect();
                    for var in variants {
                        let var_name = &sema.variant_names[var.name as usize];
                        if var.wraps >= 0 {
                            let var_id = world.make_node("(", var_name, 0, 0);
                            world.add_child(node_id, var_id);
                            let inner = &sema.type_names[var.wraps as usize];
                            let type_id = world.make_node("Type", inner, 0, 0);
                            world.add_child(var_id, type_id);
                        } else {
                            let var_id = world.make_node("Variant", var_name, 0, 0);
                            world.add_child(node_id, var_id);
                        }
                        world.register_variant(var_name, name);
                    }
                }
                SemaTypeForm::Struct => {
                    let node_id = world.make_node("{", name, 0, 0);
                    world.add_child(root, node_id);
                    world.register_struct(name);

                    let fields: Vec<_> = sema.fields.iter()
                        .filter(|f| f.type_id == sema_type.name)
                        .collect();
                    for field in fields {
                        let field_name = &sema.field_names[field.name as usize];
                        let field_id = world.make_node("Field", field_name, 0, 0);
                        world.add_child(node_id, field_id);
                        let type_id = world.make_node("Type", &field.field_type, 0, 0);
                        world.add_child(node_id, type_id);
                    }
                }
                SemaTypeForm::Alias => {}
            }
        }

        // Rebuild trait declarations
        for decl in &sema.trait_decls {
            let name = &sema.trait_names[decl.name as usize];
            let node_id = world.make_node("(", name, 0, 0);
            world.add_child(root, node_id);
            world.register_trait(name);

            let bracket_id = world.make_node("[", "", 0, 0);
            world.add_child(node_id, bracket_id);
            for sig in &decl.method_sigs {
                let method_name = &sema.method_names[sig.name as usize];
                let sig_id = world.make_node("(", method_name, 0, 0);
                world.add_child(bracket_id, sig_id);
                world.raise_params(sig_id, &sig.params);
                if !sig.return_type.is_empty() {
                    let ret_id = world.make_node("ReturnType", &sig.return_type, 0, 0);
                    world.add_child(sig_id, ret_id);
                }
            }
        }

        // Rebuild trait implementations
        for imp in &sema.trait_impls {
            let trait_name = &sema.trait_names[imp.trait_id as usize];
            let type_name = &sema.type_names[imp.type_id as usize];
            let node_id = world.make_node("[", trait_name, 0, 0);
            world.add_child(root, node_id);

            let type_ref_id = world.make_node("Type", type_name, 0, 0);
            world.add_child(node_id, type_ref_id);

            let bracket_id = world.make_node("[", "", 0, 0);
            world.add_child(node_id, bracket_id);
            for method in &imp.methods {
                let method_name = &sema.method_names[method.name as usize];
                let method_id = world.make_node("(", method_name, 0, 0);
                world.add_child(bracket_id, method_id);
                world.raise_params(method_id, &method.params);
                if !method.return_type.is_empty() {
                    let ret_id = world.make_node("ReturnType", &method.return_type, 0, 0);
                    world.add_child(method_id, ret_id);
                }
                world.raise_body(method_id, &method.body);
            }
        }

        // Rebuild constants
        for constant in &sema.constants {
            let node_id = world.make_node("{|", &constant.name, 0, 0);
            world.add_child(root, node_id);
            let type_id = world.make_node("Type", &constant.typ, 0, 0);
            world.add_child(node_id, type_id);
            world.raise_expr(node_id, &constant.value);
        }

        world
    }
}

impl AskiWorld {
    fn raise_params(&mut self, parent_id: i64, params: &[SemaParam]) {
        for param in params {
            let constructor = match (&param.borrow, param.name.as_str()) {
                (ParamBorrow::Immutable, "self") => "BorrowParam",
                (ParamBorrow::Mutable, "self") => "MutBorrowParam",
                (ParamBorrow::Owned, "self") => "OwnedParam",
                (ParamBorrow::Immutable, _) => "BorrowParam",
                (ParamBorrow::Mutable, _) => "MutBorrowParam",
                (ParamBorrow::Owned, _) => "NamedParam",
            };
            let display_name = if param.name == "self" { "Self" } else { &param.name };
            let id = self.make_node(constructor, display_name, 0, 0);
            self.add_child(parent_id, id);
            if constructor == "NamedParam" && !param.typ.is_empty() {
                let type_id = self.make_node("TypeRef", &param.typ, 0, 0);
                self.add_child(id, type_id);
            }
        }
    }
}

impl RaiseExpr for AskiWorld {
    fn raise_expr(&mut self, parent_id: i64, expr: &SemaExpr) {
        match expr {
            SemaExpr::IntLit(n) => {
                let id = self.make_node("IntLit", &n.to_string(), 0, 0);
                self.add_child(parent_id, id);
            }
            SemaExpr::FloatLit(s) => {
                let id = self.make_node("FloatLit", s, 0, 0);
                self.add_child(parent_id, id);
            }
            SemaExpr::StringLit(s) => {
                let id = self.make_node("StringLit", s, 0, 0);
                self.add_child(parent_id, id);
            }
            SemaExpr::SelfRef => {
                let id = self.make_node("InstanceRef", "Self", 0, 0);
                self.add_child(parent_id, id);
            }
            SemaExpr::InstanceRef(name) => {
                let id = self.make_node("InstanceRef", name, 0, 0);
                self.add_child(parent_id, id);
            }
            SemaExpr::QualifiedVariant { variant, .. } => {
                let name = self.known_variants.get(*variant as usize)
                    .map(|v| v.name.clone())
                    .unwrap_or_else(|| "?".into());
                let id = self.make_node("QualifiedVariant", &name, 0, 0);
                self.add_child(parent_id, id);
            }
            SemaExpr::BareName(name) => {
                let id = self.make_node("BareName", name, 0, 0);
                self.add_child(parent_id, id);
            }
            SemaExpr::TypePath(path) => {
                let id = self.make_node("TypePath", path, 0, 0);
                self.add_child(parent_id, id);
            }
            SemaExpr::BinOp { op, lhs, rhs } => {
                let id = self.make_node("BinOp", op, 0, 0);
                self.raise_expr(id, lhs);
                self.raise_expr(id, rhs);
                self.add_child(parent_id, id);
            }
            SemaExpr::FieldAccess { object, field } => {
                let id = self.make_node("FieldAccess", field, 0, 0);
                self.raise_expr(id, object);
                self.add_child(parent_id, id);
            }
            SemaExpr::MethodCall { object, method, args } => {
                let id = self.make_node("MethodCall", method, 0, 0);
                self.raise_expr(id, object);
                for arg in args {
                    self.raise_expr(id, arg);
                }
                self.add_child(parent_id, id);
            }
            SemaExpr::Group(inner) => {
                let id = self.make_node("Group", "", 0, 0);
                self.raise_expr(id, inner);
                self.add_child(parent_id, id);
            }
            SemaExpr::Return(inner) => {
                let id = self.make_node("Return", "", 0, 0);
                self.raise_expr(id, inner);
                self.add_child(parent_id, id);
            }
            SemaExpr::InlineEval(stmts) => {
                let id = self.make_node("InlineEval", "", 0, 0);
                for stmt in stmts {
                    self.raise_statement(id, stmt);
                }
                self.add_child(parent_id, id);
            }
            SemaExpr::MatchExpr { target, arms } => {
                let id = self.make_node("MatchBody", "", 0, 0);
                if let Some(t) = target {
                    let target_id = self.make_node("MatchTarget", "", 0, 0);
                    self.raise_expr(target_id, t);
                    self.add_child(id, target_id);
                }
                for arm in arms {
                    let arm_id = self.make_node("CommitArm", "", 0, 0);
                    let pat_id = self.make_node("Pattern", "", 0, 0);
                    self.add_child(arm_id, pat_id);
                    for pat in &arm.patterns {
                        self.raise_pattern(pat_id, pat);
                    }
                    self.raise_expr(arm_id, &arm.result);
                    self.add_child(id, arm_id);
                }
                self.add_child(parent_id, id);
            }
            SemaExpr::StructConstruct { type_name, fields } => {
                let id = self.make_node("StructConstruct", type_name, 0, 0);
                for (name, val) in fields {
                    let field_id = self.make_node("FieldInit", name, 0, 0);
                    self.raise_expr(field_id, val);
                    self.add_child(id, field_id);
                }
                self.add_child(parent_id, id);
            }
        }
    }

    fn raise_body(&mut self, parent_id: i64, body: &SemaBody) {
        match body {
            SemaBody::Empty => {}
            SemaBody::Block(stmts) => {
                let id = self.make_node("Block", "", 0, 0);
                for stmt in stmts {
                    self.raise_statement(id, stmt);
                }
                self.add_child(parent_id, id);
            }
            SemaBody::MatchBody { target, arms } => {
                let id = self.make_node("MatchBody", "", 0, 0);
                if let Some(t) = target {
                    let target_id = self.make_node("MatchTarget", "", 0, 0);
                    self.raise_expr(target_id, t);
                    self.add_child(id, target_id);
                }
                for arm in arms {
                    let arm_id = self.make_node("CommitArm", "", 0, 0);
                    let pat_id = self.make_node("Pattern", "", 0, 0);
                    self.add_child(arm_id, pat_id);
                    for pat in &arm.patterns {
                        self.raise_pattern(pat_id, pat);
                    }
                    self.raise_expr(arm_id, &arm.result);
                    self.add_child(id, arm_id);
                }
                self.add_child(parent_id, id);
            }
        }
    }

    fn raise_statement(&mut self, parent_id: i64, stmt: &SemaStatement) {
        match stmt {
            SemaStatement::Expr(expr) => {
                self.raise_expr(parent_id, expr);
            }
            SemaStatement::Allocation { name, typ, init } => {
                let id = self.make_node("Alloc", name, 0, 0);
                if let Some(t) = typ {
                    let type_id = self.make_node("TypeRef", t, 0, 0);
                    self.add_child(id, type_id);
                }
                if let Some(init_expr) = init {
                    self.raise_expr(id, init_expr);
                }
                self.add_child(parent_id, id);
            }
            SemaStatement::MutAllocation { name, typ, init } => {
                let id = self.make_node("MutAlloc", name, 0, 0);
                if let Some(t) = typ {
                    let type_id = self.make_node("TypeRef", t, 0, 0);
                    self.add_child(id, type_id);
                }
                if let Some(init_expr) = init {
                    self.raise_expr(id, init_expr);
                }
                self.add_child(parent_id, id);
            }
            SemaStatement::Mutation { target, method, args } => {
                let id = self.make_node("MutCall", target, 0, 0);
                let method_id = self.make_node("MethodName", method, 0, 0);
                self.add_child(id, method_id);
                for arg in args {
                    self.raise_expr(id, arg);
                }
                self.add_child(parent_id, id);
            }
            SemaStatement::Iteration { source, body } => {
                let id = self.make_node("Iteration", "", 0, 0);
                self.raise_expr(id, source);
                let body_id = self.make_node("Block", "", 0, 0);
                for stmt in body {
                    self.raise_statement(body_id, stmt);
                }
                self.add_child(id, body_id);
                self.add_child(parent_id, id);
            }
        }
    }
}

impl AskiWorld {
    fn raise_pattern(&mut self, parent_id: i64, pat: &SemaPattern) {
        match pat {
            SemaPattern::Variant(idx) => {
                let name = self.known_variants.get(*idx as usize)
                    .map(|v| v.name.clone())
                    .unwrap_or_else(|| "?".into());
                let id = self.make_node("QualifiedVariant", &name, 0, 0);
                self.add_child(parent_id, id);
            }
            SemaPattern::Or(pats) => {
                for p in pats {
                    self.raise_pattern(parent_id, p);
                }
            }
        }
    }
}
