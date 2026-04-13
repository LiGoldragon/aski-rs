//! Lower trait — AskiWorld → Sema.
//! Walks parse nodes, builds typed relations with ordinals.
//! Names interned into NameInterner (aski-side, not in Sema).
//! Expressions allocated into the flat ExprArena.

use super::aski_world::AskiWorld;
use super::sema::*;

/// Result of lowering: pure binary Sema + name interner for resolution.
pub struct LowerResult {
    pub sema: Sema,
    pub names: NameInterner,
    pub exports: Vec<String>,  // aski-level: module export names
}

pub trait Lower {
    fn lower(&self) -> LowerResult;
}

/// Expression lowering — parse nodes → ExprArena refs.
trait LowerExpr {
    fn lower_expr(&self, arena: &mut ExprArena, names: &mut NameInterner, node_id: i64) -> ExprRef;
    fn lower_body(&self, arena: &mut ExprArena, names: &mut NameInterner, node_id: i64) -> BodyRef;
    fn lower_statement(&self, arena: &mut ExprArena, names: &mut NameInterner, node_id: i64) -> StmtRef;
    fn lower_match_arm(&self, arena: &mut ExprArena, names: &mut NameInterner, arm_id: i64) -> u32;
    fn lower_params(&self, arena: &mut ExprArena, names: &mut NameInterner, parent_id: i64) -> Vec<SemaParam>;
}

impl Lower for AskiWorld {
    fn lower(&self) -> LowerResult {
        let mut sema = Sema::default();
        let mut names = NameInterner::default();
        let root_children = self.children_of(self.root_id());

        let mut declaration_order = Vec::new();

        for node in root_children {
            let constructor = node.constructor.clone();
            let key = node.key.clone();

            match constructor.as_str() {
                "(" => self.lower_paren(&mut sema, &mut names, node.id, &key, &mut declaration_order),
                "[" => self.lower_bracket(&mut sema, &mut names, node.id, &key, &mut declaration_order),
                "{" => self.lower_brace(&mut sema, &mut names, node.id, &key, &mut declaration_order),
                "{|" => self.lower_brace_pipe(&mut sema, &mut names, node.id, &key, &mut declaration_order),
                "(|" => self.lower_paren_pipe(&mut sema, &mut names, node.id, &key, &mut declaration_order),
                "[|" => self.lower_bracket_pipe(&mut sema, &mut names, node.id, &key, &mut declaration_order),
                "ProcessBody" => {
                    let body_ref = self.lower_process_body(&mut sema.arena, &mut names, node.id);
                    sema.process_body = Some(body_ref);
                }
                _ => {}
            }
        }

        // Build module record from module header node
        let root_children = self.children_of(self.root_id());
        let module_header = root_children.iter().find(|n| {
            n.constructor == "{" && !self.is_struct(&n.key)
                && n.key.starts_with(|c: char| c.is_lowercase())
        });

        if let Some(header) = module_header {
            let mod_id = names.intern_module(&header.key);
            sema.modules.push(SemaModule {
                name: mod_id,
                is_main: false,
                declaration_order,
            });
        }

        // Capture exports from module header children
        let exports: Vec<String> = if let Some(header) = module_header {
            self.children_of(header.id).iter()
                .map(|c| c.key.clone())
                .filter(|k| !k.is_empty())
                .collect()
        } else {
            Vec::new()
        };

        LowerResult { sema, names, exports }
    }
}

impl AskiWorld {
    fn lower_paren(&self, sema: &mut Sema, names: &mut NameInterner, node_id: i64, key: &str, decl_order: &mut Vec<SemaDeclarationRef>) {
        if self.is_domain(key) {
            let type_id = names.intern_type(key);
            let idx = sema.types.len();
            sema.types.push(SemaType { name: type_id, form: SemaTypeForm::Domain });
            decl_order.push(SemaDeclarationRef::Type(idx as u32));

            for (i, child) in self.children_of(node_id).iter().enumerate() {
                let var_id = names.intern_variant(&child.key);
                let wraps = match child.constructor.as_str() {
                    "(" => {
                        let var_children = self.children_of(child.id);
                        var_children.first().map(|tc| names.intern_type(&tc.key))
                    }
                    "{" => {
                        let struct_name = format!("{}{}", key, child.key);
                        let struct_type_id = names.intern_type(&struct_name);
                        sema.types.push(SemaType { name: struct_type_id, form: SemaTypeForm::Struct });
                        self.lower_struct_fields(sema, names, child.id, struct_type_id);
                        Some(struct_type_id)
                    }
                    _ => None,
                };
                sema.variants.push(SemaVariant {
                    type_id,
                    name: var_id,
                    ordinal: i as u32,
                    wraps,
                });
            }
        } else if self.is_trait(key) {
            let trait_id = names.intern_trait(key);
            let children = self.children_of(node_id);
            let mut sigs = Vec::new();

            for child in &children {
                if child.constructor == "[" {
                    for sig_node in self.children_of(child.id) {
                        if sig_node.constructor == "(" {
                            let method_id = names.intern_method(&sig_node.key);
                            let params = self.lower_params(&mut sema.arena, names, sig_node.id);
                            let sig_children = self.children_of(sig_node.id);
                            let ret_type = sig_children.iter()
                                .find(|c| c.constructor == "ReturnType")
                                .map(|c| names.intern_type(&c.key));
                            sigs.push(SemaMethodSig {
                                name: method_id,
                                params,
                                return_type: ret_type,
                            });
                        }
                    }
                }
            }

            let idx = sema.trait_decls.len();
            sema.trait_decls.push(SemaTraitDecl { name: trait_id, method_sigs: sigs });
            decl_order.push(SemaDeclarationRef::TraitDecl(idx as u32));
        }
        // Not a domain or trait — module header handled in lower()
    }

    fn lower_bracket(&self, sema: &mut Sema, names: &mut NameInterner, node_id: i64, key: &str, decl_order: &mut Vec<SemaDeclarationRef>) {
        if self.is_trait(key) {
            let trait_id = names.intern_trait(key);
            let children = self.children_of(node_id);
            let mut i = 0;
            while i < children.len() {
                if children[i].constructor == "Type" || children[i].key.chars().next().map(|c| c.is_uppercase()).unwrap_or(false) {
                    let type_name = &children[i].key;
                    let type_id = names.intern_type(type_name);
                    let mut methods = Vec::new();

                    if i + 1 < children.len() && children[i + 1].constructor == "[" {
                        let bracket_id = children[i + 1].id;
                        for method_node in self.children_of(bracket_id) {
                            if method_node.constructor == "(" {
                                let method_id = names.intern_method(&method_node.key);
                                let method_children = self.children_of(method_node.id);
                                let params = self.lower_params(&mut sema.arena, names, method_node.id);
                                let ret_type = method_children.iter()
                                    .find(|c| c.constructor == "ReturnType")
                                    .map(|c| names.intern_type(&c.key));
                                let body = method_children.iter()
                                    .find(|c| matches!(c.constructor.as_str(), "MatchBody" | "Block" | "TailBlock"))
                                    .map(|c| self.lower_body(&mut sema.arena, names, c.id))
                                    .unwrap_or_else(|| sema.arena.alloc_body(SemaBody::Empty));
                                methods.push(SemaMethod {
                                    name: method_id,
                                    params,
                                    return_type: ret_type,
                                    body,
                                });
                            }
                        }
                        i += 2;
                    } else {
                        i += 1;
                    }

                    let idx = sema.trait_impls.len();
                    sema.trait_impls.push(SemaTraitImpl {
                        trait_id,
                        type_id,
                        methods,
                    });
                    decl_order.push(SemaDeclarationRef::TraitImpl(idx as u32));
                } else {
                    i += 1;
                }
            }
        }
    }

    fn lower_brace(&self, sema: &mut Sema, names: &mut NameInterner, node_id: i64, key: &str, decl_order: &mut Vec<SemaDeclarationRef>) {
        if self.is_struct(key) {
            let type_id = names.intern_type(key);
            let idx = sema.types.len();
            sema.types.push(SemaType { name: type_id, form: SemaTypeForm::Struct });
            self.lower_struct_fields(sema, names, node_id, type_id);
            decl_order.push(SemaDeclarationRef::Type(idx as u32));
        }
    }

    fn lower_brace_pipe(&self, sema: &mut Sema, names: &mut NameInterner, node_id: i64, key: &str, decl_order: &mut Vec<SemaDeclarationRef>) {
        let children = self.children_of(node_id);
        let typ = children.iter()
            .find(|c| c.constructor == "Type" || c.key.chars().next().map(|ch| ch.is_uppercase()).unwrap_or(false))
            .map(|c| names.intern_type(&c.key))
            .unwrap_or(TypeName(0));
        let value = children.iter()
            .find(|c| matches!(c.constructor.as_str(), "IntLit" | "FloatLit" | "StringLit"))
            .map(|c| self.lower_expr(&mut sema.arena, names, c.id))
            .unwrap_or_else(|| sema.arena.alloc_expr(SemaExpr::IntLit(0)));

        let idx = sema.constants.len();
        sema.constants.push(SemaConst {
            name: names.intern_type(key),
            typ,
            value,
        });
        decl_order.push(SemaDeclarationRef::Const(idx as u32));
    }

    fn lower_paren_pipe(&self, sema: &mut Sema, names: &mut NameInterner, node_id: i64, key: &str, decl_order: &mut Vec<SemaDeclarationRef>) {
        let children = self.children_of(node_id);
        let first_idx = sema.ffi_entries.len();
        for child in &children {
            if child.constructor == "(" {
                let method_id = names.intern_method(&child.key);
                let params = self.lower_params(&mut sema.arena, names, child.id);
                let method_children = self.children_of(child.id);
                let ret_type = method_children.iter()
                    .find(|c| c.constructor == "ReturnType" || c.constructor == "Type")
                    .map(|c| names.intern_type(&c.key));
                sema.ffi_entries.push(SemaFfi {
                    library: names.intern_type(key),
                    name: method_id,
                    params,
                    return_type: ret_type,
                });
            }
        }
        if sema.ffi_entries.len() > first_idx {
            decl_order.push(SemaDeclarationRef::Ffi(first_idx as u32));
        }
    }

    fn lower_bracket_pipe(&self, _sema: &mut Sema, _names: &mut NameInterner, _node_id: i64, _key: &str, _decl_order: &mut Vec<SemaDeclarationRef>) {
        // Process block [| |] — inline executable in .aski files
    }

    fn lower_process_body(&self, arena: &mut ExprArena, names: &mut NameInterner, node_id: i64) -> BodyRef {
        let children = self.children_of(node_id);
        let stmts: Vec<StmtRef> = children.iter()
            .map(|c| self.lower_statement(arena, names, c.id))
            .collect();
        arena.alloc_body(SemaBody::Block(stmts))
    }

    fn lower_struct_fields(&self, sema: &mut Sema, names: &mut NameInterner, node_id: i64, type_id: TypeName) {
        let children = self.children_of(node_id);
        let mut ordinal = 0u32;
        let mut i = 0;
        while i < children.len() {
            if children[i].constructor == "Field" {
                let field_id = names.intern_field(&children[i].key);
                let field_type = if i + 1 < children.len() {
                    names.intern_type(&children[i + 1].key)
                } else {
                    TypeName(0)
                };
                sema.fields.push(SemaField {
                    type_id,
                    name: field_id,
                    field_type,
                    ordinal,
                });
                ordinal += 1;
                i += 2;
            } else {
                i += 1;
            }
        }
    }
}

// ── Expression lowering ──────────────────────────────────────────

impl LowerExpr for AskiWorld {
    fn lower_expr(&self, arena: &mut ExprArena, names: &mut NameInterner, node_id: i64) -> ExprRef {
        let node = match self.find_node(node_id) {
            Some(n) => n,
            None => return arena.alloc_expr(SemaExpr::BareName(BindingName(0))),
        };
        let constructor = node.constructor.clone();
        let key = node.key.clone();
        let children = self.children_of(node_id);

        let expr = match constructor.as_str() {
            "IntLit" => SemaExpr::IntLit(key.parse().unwrap_or(0)),
            "FloatLit" => SemaExpr::FloatLit(key.parse().unwrap_or(0.0)),
            "StringLit" => SemaExpr::StringLit(names.intern_string(&key)),
            "InstanceRef" => {
                if key == "Self" {
                    SemaExpr::SelfRef
                } else {
                    SemaExpr::InstanceRef(names.intern_binding(&key))
                }
            }
            "QualifiedVariant" => {
                let domain_name = self.variant_of(&key).unwrap_or("").to_string();
                let domain = names.intern_type(&domain_name);
                let variant = names.intern_variant(&key);
                SemaExpr::QualifiedVariant { domain, variant }
            }
            "BareName" => SemaExpr::BareName(names.intern_binding(&key)),
            "TypePath" => {
                // key is "Type:member"
                let parts: Vec<&str> = key.splitn(2, ':').collect();
                if parts.len() == 2 {
                    SemaExpr::TypePath {
                        typ: names.intern_type(parts[0]),
                        member: names.intern_method(parts[1]),
                    }
                } else {
                    SemaExpr::BareName(names.intern_binding(&key))
                }
            }
            "BinOp" => {
                let op = Operator::from_str(&key).unwrap_or(Operator::Add);
                let lhs = if !children.is_empty() {
                    self.lower_expr(arena, names, children[0].id)
                } else {
                    arena.alloc_expr(SemaExpr::IntLit(0))
                };
                let rhs = if children.len() > 1 {
                    self.lower_expr(arena, names, children[1].id)
                } else {
                    arena.alloc_expr(SemaExpr::IntLit(0))
                };
                SemaExpr::BinOp { op, lhs, rhs }
            }
            "FieldAccess" => {
                let object = if !children.is_empty() {
                    self.lower_expr(arena, names, children[0].id)
                } else {
                    arena.alloc_expr(SemaExpr::SelfRef)
                };
                SemaExpr::FieldAccess {
                    object,
                    field: names.intern_field(&key),
                }
            }
            "MethodCall" => {
                let object = if !children.is_empty() {
                    self.lower_expr(arena, names, children[0].id)
                } else {
                    arena.alloc_expr(SemaExpr::SelfRef)
                };
                let args: Vec<ExprRef> = children.iter().skip(1)
                    .map(|c| self.lower_expr(arena, names, c.id))
                    .collect();
                SemaExpr::MethodCall {
                    object,
                    method: names.intern_method(&key),
                    args,
                }
            }
            "Group" => {
                let inner = if !children.is_empty() {
                    self.lower_expr(arena, names, children[0].id)
                } else {
                    arena.alloc_expr(SemaExpr::IntLit(0))
                };
                SemaExpr::Group(inner)
            }
            "Return" => {
                let inner = if !children.is_empty() {
                    self.lower_expr(arena, names, children[0].id)
                } else {
                    arena.alloc_expr(SemaExpr::IntLit(0))
                };
                SemaExpr::Return(inner)
            }
            "InlineEval" => {
                let stmts: Vec<StmtRef> = children.iter()
                    .map(|c| self.lower_statement(arena, names, c.id))
                    .collect();
                SemaExpr::InlineEval(stmts)
            }
            "MatchBody" => {
                let target = children.iter()
                    .find(|c| c.constructor == "MatchTarget")
                    .and_then(|c| {
                        let tc = self.children_of(c.id);
                        tc.first().map(|t| self.lower_expr(arena, names, t.id))
                    });
                let arms: Vec<u32> = children.iter()
                    .filter(|c| c.constructor == "CommitArm")
                    .map(|c| self.lower_match_arm(arena, names, c.id))
                    .collect();
                SemaExpr::MatchExpr { target, arms }
            }
            "TryUnwrap" => {
                let inner = if !children.is_empty() {
                    self.lower_expr(arena, names, children[0].id)
                } else {
                    arena.alloc_expr(SemaExpr::IntLit(0))
                };
                SemaExpr::TryUnwrap(inner)
            }
            _ => SemaExpr::BareName(names.intern_binding(&key)),
        };
        arena.alloc_expr(expr)
    }

    fn lower_body(&self, arena: &mut ExprArena, names: &mut NameInterner, node_id: i64) -> BodyRef {
        let node = match self.find_node(node_id) {
            Some(n) => n,
            None => return arena.alloc_body(SemaBody::Empty),
        };
        let children = self.children_of(node_id);

        let body = match node.constructor.as_str() {
            "Block" | "TailBlock" => {
                let stmts: Vec<StmtRef> = children.iter()
                    .map(|c| self.lower_statement(arena, names, c.id))
                    .collect();
                SemaBody::Block(stmts)
            }
            "MatchBody" => {
                let target = children.iter()
                    .find(|c| c.constructor == "MatchTarget")
                    .and_then(|c| {
                        let tc = self.children_of(c.id);
                        tc.first().map(|t| self.lower_expr(arena, names, t.id))
                    });
                let arms: Vec<u32> = children.iter()
                    .filter(|c| c.constructor == "CommitArm")
                    .map(|c| self.lower_match_arm(arena, names, c.id))
                    .collect();
                SemaBody::MatchBody { target, arms }
            }
            _ => SemaBody::Empty,
        };
        arena.alloc_body(body)
    }

    fn lower_statement(&self, arena: &mut ExprArena, names: &mut NameInterner, node_id: i64) -> StmtRef {
        let node = match self.find_node(node_id) {
            Some(n) => n,
            None => {
                let e = arena.alloc_expr(SemaExpr::IntLit(0));
                return arena.alloc_stmt(SemaStatement::Expr(e));
            }
        };
        let children = self.children_of(node_id);

        let stmt = match node.constructor.as_str() {
            "Return" => {
                let inner = if !children.is_empty() {
                    self.lower_expr(arena, names, children[0].id)
                } else {
                    arena.alloc_expr(SemaExpr::IntLit(0))
                };
                let ret = arena.alloc_expr(SemaExpr::Return(inner));
                SemaStatement::Expr(ret)
            }
            "Alloc" => {
                let binding = names.intern_binding(&node.key);
                let typ = children.iter()
                    .find(|c| c.constructor == "TypeRef")
                    .map(|c| names.intern_type(&c.key));
                let init = children.iter()
                    .find(|c| c.constructor != "TypeRef")
                    .map(|c| self.lower_expr(arena, names, c.id));
                SemaStatement::Allocation { name: binding, typ, init }
            }
            "MutAlloc" => {
                let binding = names.intern_binding(&node.key);
                let typ = children.iter()
                    .find(|c| c.constructor == "TypeRef")
                    .map(|c| names.intern_type(&c.key));
                let init = children.iter()
                    .find(|c| c.constructor != "TypeRef")
                    .map(|c| self.lower_expr(arena, names, c.id));
                SemaStatement::MutAllocation { name: binding, typ, init }
            }
            "MutCall" => {
                let binding = names.intern_binding(&node.key);
                let method = children.iter()
                    .find(|c| c.constructor == "MethodName")
                    .map(|c| names.intern_method(&c.key))
                    .unwrap_or(MethodName(0));
                let args: Vec<ExprRef> = children.iter()
                    .filter(|c| c.constructor != "MethodName")
                    .map(|c| self.lower_expr(arena, names, c.id))
                    .collect();
                SemaStatement::Mutation { target: binding, method, args }
            }
            "Iteration" => {
                let source = if !children.is_empty() {
                    self.lower_expr(arena, names, children[0].id)
                } else {
                    arena.alloc_expr(SemaExpr::IntLit(0))
                };
                let body_stmts: Vec<StmtRef> = children.iter().skip(1)
                    .filter(|c| c.constructor == "Block")
                    .flat_map(|c| self.children_of(c.id))
                    .map(|c| self.lower_statement(arena, names, c.id))
                    .collect();
                SemaStatement::Iteration { source, body: body_stmts }
            }
            _ => {
                let expr = self.lower_expr(arena, names, node_id);
                SemaStatement::Expr(expr)
            }
        };
        arena.alloc_stmt(stmt)
    }

    fn lower_match_arm(&self, arena: &mut ExprArena, names: &mut NameInterner, arm_id: i64) -> u32 {
        let children = self.children_of(arm_id);

        let mut patterns = Vec::new();
        let mut result = arena.alloc_expr(SemaExpr::IntLit(0));

        if !children.is_empty() && children[0].constructor == "Pattern" {
            let pattern_children = self.children_of(children[0].id);
            if pattern_children.len() > 1 {
                let or_pats: Vec<VariantName> = pattern_children.iter()
                    .map(|p| names.intern_variant(&p.key))
                    .collect();
                patterns.push(SemaPattern::Or(or_pats));
            } else if let Some(pat) = pattern_children.first() {
                patterns.push(SemaPattern::Variant(names.intern_variant(&pat.key)));
            }
        }

        if children.len() > 1 {
            result = self.lower_expr(arena, names, children[children.len() - 1].id);
        }

        arena.alloc_match_arm(SemaMatchArm { patterns, result })
    }

    fn lower_params(&self, _arena: &mut ExprArena, names: &mut NameInterner, parent_id: i64) -> Vec<SemaParam> {
        let children = self.children_of(parent_id);
        let mut params = Vec::new();

        for child in &children {
            match child.constructor.as_str() {
                "BorrowParam" => {
                    let name = names.intern_method(&child.key.to_lowercase());
                    params.push(SemaParam {
                        name,
                        typ: if child.key == "Self" { Some(names.intern_type("Self")) } else { None },
                        borrow: ParamBorrow::Immutable,
                    });
                }
                "MutBorrowParam" => {
                    let name = names.intern_method(&child.key.to_lowercase());
                    params.push(SemaParam {
                        name,
                        typ: if child.key == "Self" { Some(names.intern_type("Self")) } else { None },
                        borrow: ParamBorrow::Mutable,
                    });
                }
                "OwnedParam" => {
                    let name = names.intern_method(&child.key.to_lowercase());
                    params.push(SemaParam {
                        name,
                        typ: if child.key == "Self" { Some(names.intern_type("Self")) } else { None },
                        borrow: ParamBorrow::Owned,
                    });
                }
                "NamedParam" => {
                    let name = names.intern_method(&child.key);
                    let type_children = self.children_of(child.id);
                    let typ = type_children.iter()
                        .find(|c| c.constructor == "TypeRef")
                        .map(|c| names.intern_type(&c.key));
                    params.push(SemaParam {
                        name,
                        typ,
                        borrow: ParamBorrow::Owned,
                    });
                }
                _ => {}
            }
        }

        params
    }
}
