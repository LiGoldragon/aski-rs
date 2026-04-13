//! Raise trait — Sema → AskiWorld.
//! Builds parse nodes from typed relations, reversing the lower step.
//! Needs ResolveName for ordinal → string resolution.

use std::collections::HashMap;
use crate::synth::types::Dialect;
use super::aski_world::AskiWorld;
use super::sema::*;

pub trait Raise {
    fn raise(sema: &Sema, names: &dyn ResolveName, dialects: HashMap<String, Dialect>) -> AskiWorld;
}

impl Raise for AskiWorld {
    fn raise(sema: &Sema, names: &dyn ResolveName, dialects: HashMap<String, Dialect>) -> AskiWorld {
        let mut world = AskiWorld::new(dialects);
        let root = world.root_id();

        // Rebuild module header
        for module in &sema.modules {
            let mod_name = names.module_name(module.name);
            let node_id = world.make_node("{", mod_name, 0, 0);
            world.add_child(root, node_id);
        }

        // Rebuild types
        for sema_type in &sema.types {
            let name = names.type_name(sema_type.name);
            match sema_type.form {
                SemaTypeForm::Domain => {
                    let node_id = world.make_node("(", name, 0, 0);
                    world.add_child(root, node_id);
                    world.register_domain(name);

                    for var in sema.variants.iter().filter(|v| v.type_id == sema_type.name) {
                        let var_name = names.variant_name(var.name);
                        if let Some(wraps) = var.wraps {
                            let var_id = world.make_node("(", var_name, 0, 0);
                            world.add_child(node_id, var_id);
                            let inner = names.type_name(wraps);
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

                    for field in sema.fields.iter().filter(|f| f.type_id == sema_type.name) {
                        let field_name = names.field_name(field.name);
                        let fid = world.make_node("Field", field_name, 0, 0);
                        world.add_child(node_id, fid);
                        let tid = world.make_node("Type", names.type_name(field.field_type), 0, 0);
                        world.add_child(node_id, tid);
                    }
                }
                SemaTypeForm::Alias => {}
            }
        }

        // Rebuild trait declarations
        for decl in &sema.trait_decls {
            let name = names.trait_name(decl.name);
            let node_id = world.make_node("(", name, 0, 0);
            world.add_child(root, node_id);
            world.register_trait(name);

            let bracket_id = world.make_node("[", "", 0, 0);
            world.add_child(node_id, bracket_id);
            for sig in &decl.method_sigs {
                let mname = names.method_name(sig.name);
                let sig_id = world.make_node("(", mname, 0, 0);
                world.add_child(bracket_id, sig_id);
                raise_params(&mut world, sig_id, &sig.params, names);
                if let Some(ret) = sig.return_type {
                    let ret_id = world.make_node("ReturnType", names.type_name(ret), 0, 0);
                    world.add_child(sig_id, ret_id);
                }
            }
        }

        // Rebuild trait implementations
        for imp in &sema.trait_impls {
            let trait_name = names.trait_name(imp.trait_id);
            let type_name = names.type_name(imp.type_id);
            let node_id = world.make_node("[", trait_name, 0, 0);
            world.add_child(root, node_id);

            let type_ref_id = world.make_node("Type", type_name, 0, 0);
            world.add_child(node_id, type_ref_id);

            let bracket_id = world.make_node("[", "", 0, 0);
            world.add_child(node_id, bracket_id);
            for method in &imp.methods {
                let mname = names.method_name(method.name);
                let method_id = world.make_node("(", mname, 0, 0);
                world.add_child(bracket_id, method_id);
                raise_params(&mut world, method_id, &method.params, names);
                if let Some(ret) = method.return_type {
                    let ret_id = world.make_node("ReturnType", names.type_name(ret), 0, 0);
                    world.add_child(method_id, ret_id);
                }
                raise_body(&mut world, &sema.arena, method_id, method.body, names);
            }
        }

        // Rebuild constants
        for constant in &sema.constants {
            let node_id = world.make_node("{|", names.type_name(constant.name), 0, 0);
            world.add_child(root, node_id);
            let type_id = world.make_node("Type", names.type_name(constant.typ), 0, 0);
            world.add_child(node_id, type_id);
            raise_expr(&mut world, &sema.arena, node_id, constant.value, names);
        }

        world
    }
}

fn raise_params(world: &mut AskiWorld, parent_id: i64, params: &[SemaParam], names: &dyn ResolveName) {
    for param in params {
        let pname = names.method_name(param.name);
        let constructor = match (&param.borrow, pname) {
            (ParamBorrow::Immutable, "self") => "BorrowParam",
            (ParamBorrow::Mutable, "self") => "MutBorrowParam",
            (ParamBorrow::Owned, "self") => "OwnedParam",
            (ParamBorrow::Immutable, _) => "BorrowParam",
            (ParamBorrow::Mutable, _) => "MutBorrowParam",
            (ParamBorrow::Owned, _) => "NamedParam",
        };
        let display = if pname == "self" { "Self" } else { pname };
        let id = world.make_node(constructor, display, 0, 0);
        world.add_child(parent_id, id);
        if constructor == "NamedParam" {
            if let Some(t) = param.typ {
                let tid = world.make_node("TypeRef", names.type_name(t), 0, 0);
                world.add_child(id, tid);
            }
        }
    }
}

fn raise_body(world: &mut AskiWorld, arena: &ExprArena, parent_id: i64, body_ref: BodyRef, names: &dyn ResolveName) {
    let body = arena.body(body_ref).clone();
    match body {
        SemaBody::Empty => {}
        SemaBody::Block(stmts) => {
            let id = world.make_node("Block", "", 0, 0);
            for stmt_ref in &stmts {
                raise_stmt(world, arena, id, *stmt_ref, names);
            }
            world.add_child(parent_id, id);
        }
        SemaBody::MatchBody { target, arms } => {
            let id = world.make_node("MatchBody", "", 0, 0);
            if let Some(t) = target {
                let tid = world.make_node("MatchTarget", "", 0, 0);
                raise_expr(world, arena, tid, t, names);
                world.add_child(id, tid);
            }
            for arm_idx in &arms {
                let arm = arena.match_arm(*arm_idx).clone();
                let arm_id = world.make_node("CommitArm", "", 0, 0);
                let pat_id = world.make_node("Pattern", "", 0, 0);
                world.add_child(arm_id, pat_id);
                for pat in &arm.patterns {
                    raise_pattern(world, pat_id, pat, names);
                }
                raise_expr(world, arena, arm_id, arm.result, names);
                world.add_child(id, arm_id);
            }
            world.add_child(parent_id, id);
        }
    }
}

fn raise_stmt(world: &mut AskiWorld, arena: &ExprArena, parent_id: i64, stmt_ref: StmtRef, names: &dyn ResolveName) {
    let stmt = arena.stmt(stmt_ref).clone();
    match stmt {
        SemaStatement::Expr(expr_ref) => {
            raise_expr(world, arena, parent_id, expr_ref, names);
        }
        SemaStatement::Allocation { name, .. } => {
            let id = world.make_node("Alloc", names.binding_name(name), 0, 0);
            world.add_child(parent_id, id);
        }
        SemaStatement::MutAllocation { name, .. } => {
            let id = world.make_node("MutAlloc", names.binding_name(name), 0, 0);
            world.add_child(parent_id, id);
        }
        SemaStatement::Mutation { target, method, .. } => {
            let id = world.make_node("MutCall", names.binding_name(target), 0, 0);
            let mid = world.make_node("MethodName", names.method_name(method), 0, 0);
            world.add_child(id, mid);
            world.add_child(parent_id, id);
        }
        SemaStatement::Iteration { source, body } => {
            let id = world.make_node("Iteration", "", 0, 0);
            raise_expr(world, arena, id, source, names);
            let body_id = world.make_node("Block", "", 0, 0);
            for s in &body { raise_stmt(world, arena, body_id, *s, names); }
            world.add_child(id, body_id);
            world.add_child(parent_id, id);
        }
    }
}

fn raise_expr(world: &mut AskiWorld, arena: &ExprArena, parent_id: i64, expr_ref: ExprRef, names: &dyn ResolveName) {
    let expr = arena.expr(expr_ref).clone();
    let id = match expr {
        SemaExpr::IntLit(n) => world.make_node("IntLit", &n.to_string(), 0, 0),
        SemaExpr::FloatLit(f) => world.make_node("FloatLit", &f.to_string(), 0, 0),
        SemaExpr::StringLit(s) => world.make_node("StringLit", names.literal_string(s), 0, 0),
        SemaExpr::SelfRef => world.make_node("InstanceRef", "Self", 0, 0),
        SemaExpr::InstanceRef(b) => world.make_node("InstanceRef", names.binding_name(b), 0, 0),
        SemaExpr::QualifiedVariant { variant, .. } => {
            world.make_node("QualifiedVariant", names.variant_name(variant), 0, 0)
        }
        SemaExpr::BareName(b) => world.make_node("BareName", names.binding_name(b), 0, 0),
        SemaExpr::TypePath { typ, member } => {
            world.make_node("TypePath", &format!("{}:{}", names.type_name(typ), names.method_name(member)), 0, 0)
        }
        SemaExpr::BinOp { op, lhs, rhs } => {
            let id = world.make_node("BinOp", op.as_rust(), 0, 0);
            raise_expr(world, arena, id, lhs, names);
            raise_expr(world, arena, id, rhs, names);
            world.add_child(parent_id, id);
            return;
        }
        SemaExpr::FieldAccess { object, field } => {
            let id = world.make_node("FieldAccess", names.field_name(field), 0, 0);
            raise_expr(world, arena, id, object, names);
            world.add_child(parent_id, id);
            return;
        }
        SemaExpr::MethodCall { object, method, args } => {
            let id = world.make_node("MethodCall", names.method_name(method), 0, 0);
            raise_expr(world, arena, id, object, names);
            for a in &args { raise_expr(world, arena, id, *a, names); }
            world.add_child(parent_id, id);
            return;
        }
        SemaExpr::Group(inner) => {
            let id = world.make_node("Group", "", 0, 0);
            raise_expr(world, arena, id, inner, names);
            world.add_child(parent_id, id);
            return;
        }
        SemaExpr::Return(inner) => {
            let id = world.make_node("Return", "", 0, 0);
            raise_expr(world, arena, id, inner, names);
            world.add_child(parent_id, id);
            return;
        }
        SemaExpr::InlineEval(stmts) => {
            let id = world.make_node("InlineEval", "", 0, 0);
            for s in &stmts { raise_stmt(world, arena, id, *s, names); }
            world.add_child(parent_id, id);
            return;
        }
        SemaExpr::MatchExpr { target, arms } => {
            let id = world.make_node("MatchBody", "", 0, 0);
            if let Some(t) = target {
                let tid = world.make_node("MatchTarget", "", 0, 0);
                raise_expr(world, arena, tid, t, names);
                world.add_child(id, tid);
            }
            for arm_idx in &arms {
                let arm = arena.match_arm(*arm_idx).clone();
                let arm_id = world.make_node("CommitArm", "", 0, 0);
                let pat_id = world.make_node("Pattern", "", 0, 0);
                world.add_child(arm_id, pat_id);
                for pat in &arm.patterns { raise_pattern(world, pat_id, pat, names); }
                raise_expr(world, arena, arm_id, arm.result, names);
                world.add_child(id, arm_id);
            }
            world.add_child(parent_id, id);
            return;
        }
        SemaExpr::StructConstruct { type_name, fields } => {
            let id = world.make_node("StructConstruct", names.type_name(type_name), 0, 0);
            for (f, v) in &fields {
                let fid = world.make_node("FieldInit", names.field_name(*f), 0, 0);
                raise_expr(world, arena, fid, *v, names);
                world.add_child(id, fid);
            }
            world.add_child(parent_id, id);
            return;
        }
    };
    world.add_child(parent_id, id);
}

fn raise_pattern(world: &mut AskiWorld, parent_id: i64, pat: &SemaPattern, names: &dyn ResolveName) {
    match pat {
        SemaPattern::Variant(v) => {
            let id = world.make_node("QualifiedVariant", names.variant_name(*v), 0, 0);
            world.add_child(parent_id, id);
        }
        SemaPattern::Or(variants) => {
            for v in variants {
                let id = world.make_node("QualifiedVariant", names.variant_name(*v), 0, 0);
                world.add_child(parent_id, id);
            }
        }
    }
}
