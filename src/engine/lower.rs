//! Lower trait — AskiWorld → SemaWorld.
//! Walks parse nodes, builds typed relations and expression trees.
//! After lowering, SemaWorld is self-contained — no parse tree references.

use super::aski_world::AskiWorld;
use super::sema_world::*;

pub trait Lower {
    fn lower(&self) -> SemaWorld;
}

/// Expression lowering — parse nodes → SemaExpr.
trait LowerExpr {
    fn lower_expr(&self, node_id: i64) -> SemaExpr;
    fn lower_body(&self, node_id: i64) -> SemaBody;
    fn lower_statement(&self, node_id: i64) -> SemaStatement;
    fn lower_match_arm(&self, arm_id: i64) -> SemaMatchArm;
    fn lower_params(&self, parent_id: i64) -> Vec<SemaParam>;
}

impl Lower for AskiWorld {
    fn lower(&self) -> SemaWorld {
        let mut sema = SemaWorld::new();
        let root_children = self.children_of(self.root_id());

        // Track declaration order for module system
        let mut declaration_order = Vec::new();

        for node in root_children {
            let constructor = node.constructor.clone();
            let key = node.key.clone();

            match constructor.as_str() {
                "(" => self.lower_paren(&mut sema, node.id, &key, &mut declaration_order),
                "[" => self.lower_bracket(&mut sema, node.id, &key, &mut declaration_order),
                "{" => self.lower_brace(&mut sema, node.id, &key, &mut declaration_order),
                "{|" => self.lower_brace_pipe(&mut sema, node.id, &key, &mut declaration_order),
                "(|" => self.lower_paren_pipe(&mut sema, node.id, &key, &mut declaration_order),
                "[|" => self.lower_bracket_pipe(&mut sema, node.id, &key, &mut declaration_order),
                "ProcessBody" => self.lower_process_body(&mut sema, node.id),
                _ => {}
            }
        }

        // Build module record from the module header node
        // Module is {module/ ...} — a { node with camelCase key (not a struct)
        let root_children = self.children_of(self.root_id());
        let module_header = root_children.iter().find(|n| {
            n.constructor == "{" && !self.is_struct(&n.key)
                && n.key.starts_with(|c: char| c.is_lowercase())
        });

        if let Some(header) = module_header {
            let module_name = header.key.clone();
            let mod_id = sema.intern_module(&module_name);

            // Exports are the children of the module header node
            let header_children = self.children_of(header.id);
            let exports: Vec<String> = header_children.iter()
                .map(|c| c.key.clone())
                .filter(|k| !k.is_empty())
                .collect();

            // Imports are bracket children of the module header: [:Module/ items]
            let imports: Vec<SemaImport> = header_children.iter()
                .filter(|c| c.constructor == "[")
                .map(|c| {
                    let import_children = self.children_of(c.id);
                    let names: Vec<String> = import_children.iter()
                        .map(|ic| ic.key.clone())
                        .filter(|k| !k.is_empty())
                        .collect();
                    SemaImport {
                        module_name: c.key.clone(),
                        names,
                    }
                })
                .collect();

            sema.modules.push(SemaModule {
                name: mod_id,
                file_path: self.current_file.clone(),
                is_main: false,
                exports,
                imports,
                declaration_order,
            });
        }

        sema
    }
}

impl AskiWorld {
    fn lower_paren(&self, sema: &mut SemaWorld, node_id: i64, key: &str, decl_order: &mut Vec<SemaDeclarationRef>) {
        if self.is_domain(key) {
            let type_id = sema.intern_type(key);
            let idx = sema.types.len();
            sema.types.push(SemaType { name: type_id, form: SemaTypeForm::Domain });
            decl_order.push(SemaDeclarationRef::Type(idx));

            for (i, child) in self.children_of(node_id).iter().enumerate() {
                let var_id = sema.intern_variant(&child.key);
                let wraps = match child.constructor.as_str() {
                    "(" => {
                        // Data-carrying variant: (@Variant/ :Type)
                        let var_children = self.children_of(child.id);
                        if let Some(type_child) = var_children.first() {
                            sema.intern_type(&type_child.key)
                        } else {
                            -1
                        }
                    }
                    "{" => {
                        // Struct variant: {@Variant/ fields}
                        // Lower the inner struct
                        let struct_name = format!("{}{}", key, child.key);
                        let struct_type_id = sema.intern_type(&struct_name);
                        sema.types.push(SemaType { name: struct_type_id, form: SemaTypeForm::Struct });
                        self.lower_struct_fields(sema, child.id, struct_type_id);
                        struct_type_id
                    }
                    _ => -1,
                };
                sema.variants.push(SemaVariant {
                    type_id,
                    name: var_id,
                    ordinal: i as i64,
                    wraps,
                });
            }
        } else if self.is_trait(key) {
            let trait_id = sema.intern_trait(key);
            let children = self.children_of(node_id);
            let mut sigs = Vec::new();

            for child in &children {
                if child.constructor == "[" {
                    for sig_node in self.children_of(child.id) {
                        if sig_node.constructor == "(" {
                            let method_id = sema.intern_method(&sig_node.key);
                            let params = self.lower_params(sig_node.id);
                            let sig_children = self.children_of(sig_node.id);
                            let ret_type = sig_children.iter()
                                .find(|c| c.constructor == "ReturnType")
                                .map(|c| c.key.clone())
                                .unwrap_or_default();
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
            decl_order.push(SemaDeclarationRef::TraitDecl(idx));
        }
        // Not a domain or trait — skip (module header is now {module/ ...})
    }

    fn lower_bracket(&self, sema: &mut SemaWorld, node_id: i64, key: &str, decl_order: &mut Vec<SemaDeclarationRef>) {
        if self.is_trait(key) {
            let trait_id = sema.intern_trait(key);
            let children = self.children_of(node_id);
            let mut i = 0;
            while i < children.len() {
                if children[i].constructor == "Type" || children[i].key.chars().next().map(|c| c.is_uppercase()).unwrap_or(false) {
                    let type_name = &children[i].key;
                    let type_id = sema.intern_type(type_name);
                    let mut methods = Vec::new();

                    if i + 1 < children.len() && children[i + 1].constructor == "[" {
                        let bracket_id = children[i + 1].id;
                        for method_node in self.children_of(bracket_id) {
                            if method_node.constructor == "(" {
                                let method_id = sema.intern_method(&method_node.key);
                                let method_children = self.children_of(method_node.id);
                                let params = self.lower_params(method_node.id);
                                let ret_type = method_children.iter()
                                    .find(|c| c.constructor == "ReturnType")
                                    .map(|c| c.key.clone())
                                    .unwrap_or_default();
                                let body = method_children.iter()
                                    .find(|c| matches!(c.constructor.as_str(), "MatchBody" | "Block" | "TailBlock"))
                                    .map(|c| self.lower_body(c.id))
                                    .unwrap_or(SemaBody::Empty);
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
                    decl_order.push(SemaDeclarationRef::TraitImpl(idx));
                } else {
                    i += 1;
                }
            }
        }
    }

    fn lower_brace(&self, sema: &mut SemaWorld, node_id: i64, key: &str, decl_order: &mut Vec<SemaDeclarationRef>) {
        if self.is_struct(key) {
            let type_id = sema.intern_type(key);
            let idx = sema.types.len();
            sema.types.push(SemaType { name: type_id, form: SemaTypeForm::Struct });
            self.lower_struct_fields(sema, node_id, type_id);
            decl_order.push(SemaDeclarationRef::Type(idx));
        }
    }

    fn lower_brace_pipe(&self, sema: &mut SemaWorld, node_id: i64, key: &str, decl_order: &mut Vec<SemaDeclarationRef>) {
        let children = self.children_of(node_id);
        let typ = children.iter()
            .find(|c| c.constructor == "Type" || c.key.chars().next().map(|ch| ch.is_uppercase()).unwrap_or(false))
            .map(|c| c.key.clone())
            .unwrap_or_default();
        let value = children.iter()
            .find(|c| matches!(c.constructor.as_str(), "IntLit" | "FloatLit" | "StringLit" | "BareName"))
            .map(|c| self.lower_expr(c.id))
            .unwrap_or(SemaExpr::IntLit(0));

        let idx = sema.constants.len();
        sema.constants.push(SemaConst {
            name: key.to_string(),
            typ,
            value,
        });
        decl_order.push(SemaDeclarationRef::Const(idx));
    }

    fn lower_paren_pipe(&self, sema: &mut SemaWorld, node_id: i64, key: &str, decl_order: &mut Vec<SemaDeclarationRef>) {
        let children = self.children_of(node_id);
        let first_idx = sema.ffi_entries.len();
        for child in &children {
            if child.constructor == "(" {
                let method_id = sema.intern_method(&child.key);
                let params = self.lower_params(child.id);
                let method_children = self.children_of(child.id);
                let ret_type = method_children.iter()
                    .find(|c| c.constructor == "ReturnType" || c.constructor == "Type")
                    .map(|c| c.key.clone())
                    .unwrap_or_default();
                sema.ffi_entries.push(SemaFfi {
                    library: key.to_string(),
                    name: method_id,
                    params,
                    return_type: ret_type,
                });
            }
        }
        if sema.ffi_entries.len() > first_idx {
            decl_order.push(SemaDeclarationRef::Ffi(first_idx));
        }
    }

    fn lower_bracket_pipe(&self, _sema: &mut SemaWorld, _node_id: i64, _key: &str, _decl_order: &mut Vec<SemaDeclarationRef>) {
        // Process block [| |] — inline executable in .aski files
        // TODO: lower statements into SemaBody
    }

    fn lower_process_body(&self, sema: &mut SemaWorld, node_id: i64) {
        let children = self.children_of(node_id);
        let stmts: Vec<SemaStatement> = children.iter()
            .map(|c| self.lower_statement(c.id))
            .collect();
        sema.process_body = Some(SemaBody::Block(stmts));
    }

    fn lower_struct_fields(&self, sema: &mut SemaWorld, node_id: i64, type_id: i64) {
        let children = self.children_of(node_id);
        let mut ordinal = 0;
        let mut i = 0;
        while i < children.len() {
            if children[i].constructor == "Field" {
                let field_id = sema.intern_field(&children[i].key);
                let field_type = if i + 1 < children.len() {
                    children[i + 1].key.clone()
                } else {
                    String::new()
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
    fn lower_expr(&self, node_id: i64) -> SemaExpr {
        let node = match self.find_node(node_id) {
            Some(n) => n,
            None => return SemaExpr::BareName("?".into()),
        };
        let constructor = node.constructor.clone();
        let key = node.key.clone();
        let children = self.children_of(node_id);

        match constructor.as_str() {
            "IntLit" => SemaExpr::IntLit(key.parse().unwrap_or(0)),
            "FloatLit" => SemaExpr::FloatLit(key),
            "StringLit" => SemaExpr::StringLit(key),
            "InstanceRef" => {
                if key == "Self" {
                    SemaExpr::SelfRef
                } else {
                    SemaExpr::InstanceRef(key)
                }
            }
            "QualifiedVariant" => {
                let domain = self.variant_of(&key).unwrap_or("").to_string();
                let domain_id = self.known_types.iter().position(|t| t.name == domain).unwrap_or(0) as i64;
                let variant_id = self.known_variants.iter().position(|v| v.name == key).unwrap_or(0) as i64;
                SemaExpr::QualifiedVariant { domain: domain_id, variant: variant_id }
            }
            "BareName" => SemaExpr::BareName(key),
            "TypePath" => SemaExpr::TypePath(key),
            "BinOp" => {
                let lhs = if !children.is_empty() {
                    self.lower_expr(children[0].id)
                } else {
                    SemaExpr::BareName("?".into())
                };
                let rhs = if children.len() > 1 {
                    self.lower_expr(children[1].id)
                } else {
                    SemaExpr::BareName("?".into())
                };
                SemaExpr::BinOp {
                    op: key,
                    lhs: Box::new(lhs),
                    rhs: Box::new(rhs),
                }
            }
            "FieldAccess" => {
                let object = if !children.is_empty() {
                    self.lower_expr(children[0].id)
                } else {
                    SemaExpr::SelfRef
                };
                SemaExpr::FieldAccess {
                    object: Box::new(object),
                    field: key,
                }
            }
            "MethodCall" => {
                let object = if !children.is_empty() {
                    self.lower_expr(children[0].id)
                } else {
                    SemaExpr::SelfRef
                };
                let args: Vec<SemaExpr> = children.iter().skip(1)
                    .map(|c| self.lower_expr(c.id))
                    .collect();
                SemaExpr::MethodCall {
                    object: Box::new(object),
                    method: key,
                    args,
                }
            }
            "Group" => {
                let inner = if !children.is_empty() {
                    self.lower_expr(children[0].id)
                } else {
                    SemaExpr::BareName("()".into())
                };
                SemaExpr::Group(Box::new(inner))
            }
            "Return" => {
                let inner = if !children.is_empty() {
                    self.lower_expr(children[0].id)
                } else {
                    SemaExpr::BareName("()".into())
                };
                SemaExpr::Return(Box::new(inner))
            }
            "InlineEval" => {
                let stmts: Vec<SemaStatement> = children.iter()
                    .map(|c| self.lower_statement(c.id))
                    .collect();
                SemaExpr::InlineEval(stmts)
            }
            "MatchBody" => {
                // Match expression used as expression value
                let target = children.iter()
                    .find(|c| c.constructor == "MatchTarget")
                    .and_then(|c| {
                        let target_children = self.children_of(c.id);
                        target_children.first().map(|tc| Box::new(self.lower_expr(tc.id)))
                    });
                let arms: Vec<SemaMatchArm> = children.iter()
                    .filter(|c| c.constructor == "CommitArm")
                    .map(|c| self.lower_match_arm(c.id))
                    .collect();
                SemaExpr::MatchExpr { target, arms }
            }
            _ => SemaExpr::BareName(key),
        }
    }

    fn lower_body(&self, node_id: i64) -> SemaBody {
        let node = match self.find_node(node_id) {
            Some(n) => n,
            None => return SemaBody::Empty,
        };
        let children = self.children_of(node_id);

        match node.constructor.as_str() {
            "Block" | "TailBlock" => {
                let stmts: Vec<SemaStatement> = children.iter()
                    .map(|c| self.lower_statement(c.id))
                    .collect();
                SemaBody::Block(stmts)
            }
            "MatchBody" => {
                let target = children.iter()
                    .find(|c| c.constructor == "MatchTarget")
                    .and_then(|c| {
                        let target_children = self.children_of(c.id);
                        target_children.first().map(|tc| self.lower_expr(tc.id))
                    });
                let arms: Vec<SemaMatchArm> = children.iter()
                    .filter(|c| c.constructor == "CommitArm")
                    .map(|c| self.lower_match_arm(c.id))
                    .collect();
                SemaBody::MatchBody { target, arms }
            }
            _ => SemaBody::Empty,
        }
    }

    fn lower_statement(&self, node_id: i64) -> SemaStatement {
        let node = match self.find_node(node_id) {
            Some(n) => n,
            None => return SemaStatement::Expr(SemaExpr::BareName("?".into())),
        };
        let children = self.children_of(node_id);

        match node.constructor.as_str() {
            "Return" => {
                let inner = if !children.is_empty() {
                    self.lower_expr(children[0].id)
                } else {
                    SemaExpr::BareName("()".into())
                };
                SemaStatement::Expr(SemaExpr::Return(Box::new(inner)))
            }
            "Alloc" => {
                let typ = children.iter()
                    .find(|c| c.constructor == "TypeRef")
                    .map(|c| c.key.clone());
                let init = children.iter()
                    .find(|c| c.constructor != "TypeRef")
                    .map(|c| self.lower_expr(c.id));
                SemaStatement::Allocation {
                    name: node.key.clone(),
                    typ,
                    init,
                }
            }
            "MutAlloc" => {
                let typ = children.iter()
                    .find(|c| c.constructor == "TypeRef")
                    .map(|c| c.key.clone());
                let init = children.iter()
                    .find(|c| c.constructor != "TypeRef")
                    .map(|c| self.lower_expr(c.id));
                SemaStatement::MutAllocation {
                    name: node.key.clone(),
                    typ,
                    init,
                }
            }
            "MutCall" => {
                let method = children.iter()
                    .find(|c| c.constructor == "MethodName")
                    .map(|c| c.key.clone())
                    .unwrap_or_default();
                let args: Vec<SemaExpr> = children.iter()
                    .filter(|c| c.constructor != "MethodName")
                    .map(|c| self.lower_expr(c.id))
                    .collect();
                SemaStatement::Mutation {
                    target: node.key.clone(),
                    method,
                    args,
                }
            }
            "Iteration" => {
                let source = if !children.is_empty() {
                    self.lower_expr(children[0].id)
                } else {
                    SemaExpr::BareName("?".into())
                };
                let body_stmts: Vec<SemaStatement> = children.iter().skip(1)
                    .filter(|c| c.constructor == "Block")
                    .flat_map(|c| self.children_of(c.id))
                    .map(|c| self.lower_statement(c.id))
                    .collect();
                SemaStatement::Iteration { source, body: body_stmts }
            }
            _ => SemaStatement::Expr(self.lower_expr(node_id)),
        }
    }

    fn lower_match_arm(&self, arm_id: i64) -> SemaMatchArm {
        let children = self.children_of(arm_id);

        let mut patterns = Vec::new();
        let mut result = SemaExpr::BareName("?".into());

        if !children.is_empty() && children[0].constructor == "Pattern" {
            let pattern_children = self.children_of(children[0].id);
            if pattern_children.len() > 1 {
                // Multiple patterns = or-pattern: (Fire | Air) → Or([Fire, Air])
                let or_pats: Vec<SemaPattern> = pattern_children.iter().map(|pat| {
                    let variant_idx = self.known_variants.iter()
                        .position(|v| v.name == pat.key)
                        .unwrap_or(0) as i64;
                    SemaPattern::Variant(variant_idx)
                }).collect();
                patterns.push(SemaPattern::Or(or_pats));
            } else if let Some(pat) = pattern_children.first() {
                let variant_idx = self.known_variants.iter()
                    .position(|v| v.name == pat.key)
                    .unwrap_or(0) as i64;
                patterns.push(SemaPattern::Variant(variant_idx));
            }
        }

        if children.len() > 1 {
            result = self.lower_expr(children[children.len() - 1].id);
        }

        SemaMatchArm { patterns, result }
    }

    fn lower_params(&self, parent_id: i64) -> Vec<SemaParam> {
        let children = self.children_of(parent_id);
        let mut params = Vec::new();

        for child in &children {
            match child.constructor.as_str() {
                "BorrowParam" => {
                    params.push(SemaParam {
                        name: child.key.to_lowercase(),
                        typ: if child.key == "Self" { "Self".into() } else { String::new() },
                        borrow: ParamBorrow::Immutable,
                    });
                }
                "MutBorrowParam" => {
                    params.push(SemaParam {
                        name: child.key.to_lowercase(),
                        typ: if child.key == "Self" { "Self".into() } else { String::new() },
                        borrow: ParamBorrow::Mutable,
                    });
                }
                "OwnedParam" => {
                    params.push(SemaParam {
                        name: child.key.to_lowercase(),
                        typ: if child.key == "Self" { "Self".into() } else { String::new() },
                        borrow: ParamBorrow::Owned,
                    });
                }
                "NamedParam" => {
                    let type_children = self.children_of(child.id);
                    let typ = type_children.iter()
                        .find(|c| c.constructor == "TypeRef")
                        .map(|c| c.key.clone())
                        .unwrap_or_default();
                    params.push(SemaParam {
                        name: child.key.clone(),
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
