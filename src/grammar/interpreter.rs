//! PEG interpreter — executes grammar rules against token streams.
//!
//! Ordered choice with backtracking. First arm that matches wins.
//! ParseNodes are written directly to the World — no AST layer.
//! Newline skip: only Tok elements are position-exact; other elements
//! skip newlines before matching.

use crate::lexer::{self, Token};
use crate::grammar::config::GrammarConfig;
use super::{ParseArm, PatElem, ResultSpec, ResultArg, PegValue, Bindings, RuleTable};
use aski_core::{World, IdGen, ParseNode, ParseChild, CtxKind, ParseStatus};

/// Spanned token — re-uses the lexer's Spanned type.
pub type SpannedToken = lexer::Spanned;

/// Grammar parser — holds rule table, config, and the World being built.
pub struct GrammarParser {
    pub rules: RuleTable,
    pub config: GrammarConfig,
    pub world: World,
    ids: IdGen,
    root_id: i64,
    /// Context stack — tracks CtxKind of nodes being constructed.
    /// Grammar rules can reference @World.Context and @World.ParentContext.
    ctx_stack: Vec<CtxKind>,
}

impl GrammarParser {
    pub fn new(rules: RuleTable, config: GrammarConfig) -> Self {
        let mut ids = IdGen::new();
        let mut world = World::default();
        let ctx_stack = vec![CtxKind::Root];
        let root_id = ids.next();
        world.parse_nodes.push(ParseNode {
            id: root_id,
            constructor: "Root".to_string(),
            ctx: CtxKind::Root,
            parent_id: -1,
            status: ParseStatus::Committed,
            text: String::new(),
            token_start: 0,
            token_end: 0,
        });
        GrammarParser { rules, config, world, ids, root_id, ctx_stack }
    }

    /// Parse source text and return the populated World.
    /// User-defined grammar rules encountered during parsing are added to
    /// the live rule table, making them available for subsequent code.
    pub fn parse_to_world(&mut self, source: &str) -> Result<World, String> {
        let tokens = crate::lexer::lex(source).map_err(|errs| {
            errs.into_iter().map(|e| format!("{}", e)).collect::<Vec<_>>().join(", ")
        })?;

        // Try <header> first
        let mut pos = skip_newlines(&tokens, 0);
        if pos < tokens.len() && tokens[pos].token == Token::LParen {
            match self.try_rule("header", &tokens, pos) {
                Ok((PegValue::NodeId(hid), new_pos)) => {
                    // Extract module header info and populate world.modules/exports
                    let hnode = self.world.parse_nodes.iter().find(|n| n.id == hid).cloned();
                    if let Some(ref hnode) = hnode {
                        if hnode.constructor == "ModuleHeader" {
                            let name = hnode.text.clone();
                            self.world.modules.push(aski_core::Module {
                                name: name.clone(),
                                path: String::new(),
                            });
                            // Exports are children with constructor ExportName
                            let children: Vec<_> = self.world.parse_children.iter()
                                .filter(|c| c.parent_id == hid)
                                .map(|c| c.child_id)
                                .collect();
                            for cid in children {
                                if let Some(cn) = self.world.parse_nodes.iter().find(|n| n.id == cid) {
                                    if cn.constructor == "ExportName" {
                                        self.world.exports.push(aski_core::Export {
                                            module_name: name.clone(),
                                            export_name: cn.text.clone(),
                                        });
                                    }
                                }
                            }
                        }
                    }
                    // Add header as child of root
                    let ord = self.next_child_ordinal(self.root_id);
                    self.add_child(self.root_id, ord, hid);
                    pos = new_pos;
                }
                _ => {} // no header, that's fine
            }
        }

        // Parse items
        self.parse_items(&tokens, pos)?;

        // Run derivation rules
        aski_core::run_rules(&mut self.world);

        Ok(std::mem::take(&mut self.world))
    }

    /// Parse items from token stream.
    /// When a GrammarRule item is encountered, it's converted to a ParseRule
    /// and added to the live rule table for subsequent parsing.
    fn parse_items(&mut self, tokens: &[SpannedToken], mut pos: usize) -> Result<(), String> {
        pos = skip_newlines(tokens, pos);
        while pos < tokens.len() {
            match self.try_rule("item", tokens, pos) {
                Ok((value, new_pos)) => {
                    if let PegValue::NodeId(item_id) = value {
                        // Check constructor for live grammar rule injection + skip logic
                        let constructor = self.world.parse_nodes.iter()
                            .find(|n| n.id == item_id)
                            .map(|n| n.constructor.clone())
                            .unwrap_or_default();
                        let item_text = self.world.parse_nodes.iter()
                            .find(|n| n.id == item_id)
                            .map(|n| n.text.clone())
                            .unwrap_or_default();

                        // Live grammar rule injection
                        if constructor == "GrammarRule" {
                            if let Some(parse_rule) = super::grammar_node_to_parse_rule(&self.world, item_id) {
                                self.rules.insert(parse_rule.name.clone(), parse_rule);
                            }
                        }

                        // Skip leaked artifacts: zero/single-variant domains, duplicate names
                        let skip = match constructor.as_str() {
                            "Domain" => {
                                let variant_count = self.world.parse_children.iter()
                                    .filter(|c| c.parent_id == item_id)
                                    .count();
                                variant_count <= 1
                                    || self.has_existing_item(&item_text, item_id)
                            }
                            "Struct" => {
                                self.has_existing_item(&item_text, item_id)
                            }
                            _ => false,
                        };

                        if !skip {
                            let ord = self.next_child_ordinal(self.root_id);
                            self.add_child(self.root_id, ord, item_id);
                        }
                    }
                    pos = skip_newlines(tokens, new_pos);
                }
                Err(e) => {
                    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open("/tmp/bootstrap_skip.log") {
                        use std::io::Write;
                        let tok = if pos < tokens.len() { format!("{:?}", tokens[pos].token) } else { "EOF".into() };
                        let _ = writeln!(f, "SKIP[{}] {}: {}", pos, tok, e);
                    }
                    pos += 1;
                    loop {
                        pos = skip_newlines(tokens, pos);
                        if pos >= tokens.len() { break; }
                        match &tokens[pos].token {
                            crate::lexer::Token::PascalIdent(_) |
                            crate::lexer::Token::CamelIdent(_) => break,
                            _ => { pos += 1; }
                        }
                    }
                }
            }
        }
        Ok(())
    }

    /// Check if an item with this name already exists as a child of root.
    fn has_existing_item(&self, name: &str, exclude_id: i64) -> bool {
        let root_children: Vec<i64> = self.world.parse_children.iter()
            .filter(|c| c.parent_id == self.root_id)
            .map(|c| c.child_id)
            .collect();
        for cid in root_children {
            if cid == exclude_id { continue; }
            if let Some(n) = self.world.parse_nodes.iter().find(|n| n.id == cid) {
                if (n.constructor == "Domain" || n.constructor == "Struct") && n.text == name {
                    return true;
                }
            }
        }
        false
    }

    // ── Core PEG engine ──────────────────────────────────────

    /// Try a grammar rule: ordered choice over arms.
    /// Detect Cons/Nil list pattern and return the element rule name if found.
    /// Pattern: arm[0] has Cons(elem, self) result, arm[1] has Nil result.
    fn detect_cons_element(name: &str, rule: &super::ParseRule) -> Option<String> {
        if rule.arms.len() != 2 { return None; }
        let cons = &rule.arms[0];
        let nil = &rule.arms[1];
        if cons.result.constructor != "Cons" || nil.result.constructor != "Nil" { return None; }
        // Find the element Rule in the Cons arm's pattern (not the recursive self-call)
        for elem in &cons.pattern {
            if let PatElem::Rule(r) = elem {
                if r != name { return Some(r.clone()); }
            }
        }
        None
    }

    pub fn try_rule(&mut self, name: &str, tokens: &[SpannedToken], pos: usize) -> Result<(PegValue, usize), String> {
        let pos = skip_newlines(tokens, pos);
        // Grammar rules take priority — try them first if defined
        if self.rules.contains_key(name) {
            let rule = self.rules[name].clone();

            // @World-driven iterative Cons/Nil: pure 2-arm rules (Cons + Nil).
            // Calls the ELEMENT rule in a loop. Reversed to match Cons ordering.
            // @World-driven: only whitelist safe Cons/Nil rules for iteration
            let safe_to_iterate = false;
            if safe_to_iterate {
            if let Some(elem_rule) = Self::detect_cons_element(name, &rule) {
                let mut ids: Vec<i64> = Vec::new();
                let mut cur = pos;
                loop {
                    let cur_skip = skip_newlines(tokens, cur);
                    if cur_skip >= tokens.len() { break; }
                    // Stop at block-closing tokens (same check as stmts handler)
                    match &tokens[cur_skip].token {
                        Token::RBracket | Token::RParen | Token::RBrace => break,
                        _ if token_variant_name(&tokens[cur_skip].token) == "IterClose" => break,
                        _ if token_variant_name(&tokens[cur_skip].token) == "CompositionClose" => break,
                        _ if token_variant_name(&tokens[cur_skip].token) == "TraitBoundClose" => break,
                        _ => {}
                    }
                    let save = self.save_state();
                    let ctx_depth = self.ctx_stack.len();
                    if (name == "implMembers" && cur_skip >= 540) || (name == "stmts" && pos >= 600) {
                        let tok = if cur_skip < tokens.len() { format!("{:?}", &tokens[cur_skip].token) } else { "EOF".into() };
                    }
                    match self.try_rule(&elem_rule, tokens, cur_skip) {
                        Ok((val, new_pos)) => {
                            if (name == "implMembers" && cur_skip >= 540) || (name == "stmts" && pos >= 600) {
                            }
                            if new_pos <= cur_skip {
                                self.restore_state(&save);
                                while self.ctx_stack.len() > ctx_depth { self.ctx_stack.pop(); }
                                break;
                            }
                            match val {
                                PegValue::NodeId(id) => ids.push(id),
                                PegValue::NodeList(ref nids) => ids.extend(nids),
                                PegValue::Text(_) | PegValue::Int(_) | PegValue::Float(_) => {
                                    let leaf = self.make_leaf("TextLeaf", &peg_value_text(&val), cur_skip as i64, new_pos as i64);
                                    ids.push(leaf);
                                }
                                _ => {}
                            }
                            cur = new_pos;
                        }
                        Err(e) => {
                            if (name == "implMembers" && cur_skip >= 540) || (name == "stmts" && pos >= 600) {
                            }
                            self.restore_state(&save);
                            while self.ctx_stack.len() > ctx_depth { self.ctx_stack.pop(); }
                            break;
                        }
                    }
                }
                // Forward order + skip trailing newlines (match recursive handler behavior)
                return Ok((PegValue::NodeList(ids), skip_newlines(tokens, cur)));
            }
            } // safe_to_iterate

            // Block-aware parsing: for rules that create scope (stmts, body, implMembers, etc.),
            // use the ParsingWorld to track context instead of deep recursion.
            // @World-driven iterative stmts: the grammar uses Body context,
            // the interpreter iterates at constant call depth.
            // Reversed to match Cons ordering (downstream expects last-first).
            if false && name == "stmts" {
                let mut stmt_ids: Vec<i64> = Vec::new();
                let mut cur = pos;
                self.ctx_stack.push(CtxKind::Body);
                loop {
                    let cur_skip = skip_newlines(tokens, cur);
                    if cur_skip >= tokens.len() { break; }
                    // @World.IsBlockDelimiter — stop at block-closing tokens
                    match &tokens[cur_skip].token {
                        Token::RBracket | Token::RParen => break,
                        _ if token_variant_name(&tokens[cur_skip].token) == "IterClose" => break,
                        _ if token_variant_name(&tokens[cur_skip].token) == "CompositionClose" => break,
                        _ => {}
                    }
                    let save = self.save_state();
                    match self.try_rule("stmt", tokens, cur_skip) {
                        Ok((val, new_pos)) => {
                            if new_pos <= cur_skip {
                                self.restore_state(&save);
                                break;
                            }
                            match val {
                                PegValue::NodeId(id) => stmt_ids.push(id),
                                PegValue::NodeList(ref nids) => stmt_ids.extend(nids),
                                _ => {}
                            }
                            cur = new_pos;
                        }
                        Err(_) => {
                            self.restore_state(&save);
                            break;
                        }
                    }
                }
                self.ctx_stack.pop();
                // Reversed: codegen emits stmts last-to-first (Cons convention)
                stmt_ids.reverse();
                return Ok((PegValue::NodeList(stmt_ids), skip_newlines(tokens, cur)));
            }

            let mut last_err = String::new();
            for arm in &rule.arms {
                // Save world state for backtracking
                let node_count = self.world.parse_nodes.len();
                let child_count = self.world.parse_children.len();
                let type_count = self.world.types.len();
                let variant_count = self.world.variants.len();
                let field_count = self.world.fields.len();
                let ffi_count = self.world.ffi_entries.len();
                let rule_count = self.world.rules.len();
                let arm_count_saved = self.world.arms.len();
                let pat_elem_count = self.world.pat_elems.len();
                let id_saved = self.ids.next;
                match self.try_arm(arm, tokens, pos) {
                    Ok(result) => return Ok(result),
                    Err(e) => {
                        if (pos >= 540 && pos <= 570 && name == "item") || (pos >= 588 && pos <= 610 && (name == "implMember" || name == "body")) {
                            let arm_result = &arm.result.constructor;
                        }
                        // Backtrack: remove nodes added by failed arm
                        self.world.parse_nodes.truncate(node_count);
                        self.world.parse_children.truncate(child_count);
                        self.world.types.truncate(type_count);
                        self.world.variants.truncate(variant_count);
                        self.world.fields.truncate(field_count);
                        self.world.ffi_entries.truncate(ffi_count);
                        self.world.rules.truncate(rule_count);
                        self.world.arms.truncate(arm_count_saved);
                        self.world.pat_elems.truncate(pat_elem_count);
                        self.ids.next = id_saved;
                        last_err = e;
                    }
                }
            }
            return Err(format!("rule <{}> failed at pos {}: {}", name, pos, last_err));
        }

        Err(format!("unknown grammar rule: <{}>", name))
    }

    /// Try a single arm: match pattern, build result.
    fn try_arm(&mut self, arm: &ParseArm, tokens: &[SpannedToken], pos: usize) -> Result<(PegValue, usize), String> {
        let mut bindings = Bindings::new();
        let mut cur = pos;

        for elem in &arm.pattern {
            // Skip newlines before all elements EXCEPT Tok.
            if !matches!(elem, PatElem::Tok(_)) {
                cur = skip_newlines(tokens, cur);
            }
            match elem {
                PatElem::Tok(name) => {
                    let tok = tokens.get(cur)
                        .ok_or_else(|| format!("expected token {}, got EOF", name))?;
                    if token_variant_name(&tok.token) != name.as_str() {
                        return Err(format!("expected {}, got {:?}", name, tok.token));
                    }
                    cur += 1;
                }
                PatElem::Lit(value) => {
                    let tok = tokens.get(cur)
                        .ok_or_else(|| format!("expected '{}', got EOF", value))?;
                    match &tok.token {
                        Token::PascalIdent(s) if s == value => { cur += 1; }
                        Token::CamelIdent(s) if s == value => { cur += 1; }
                        _ => return Err(format!("expected '{}', got {:?}", value, tok.token)),
                    }
                }
                PatElem::Rule(rule_name) => {
                    let (value, new_pos) = self.try_rule(rule_name, tokens, cur)?;
                    bindings.insert(rule_name.clone(), value);
                    cur = new_pos;
                }
                PatElem::Bind(bind_name) => {
                    let tok = tokens.get(cur)
                        .ok_or_else(|| format!("expected identifier for @{}, got EOF", bind_name))?;
                    let value = match &tok.token {
                        Token::PascalIdent(s) => PegValue::Text(s.clone()),
                        Token::CamelIdent(s) => PegValue::Text(s.clone()),
                        _ => return Err(format!("cannot bind @{} to {:?} (expected identifier)", bind_name, tok.token)),
                    };
                    bindings.insert(bind_name.clone(), value);
                    cur += 1;
                }
                PatElem::BindType(bind_name) => {
                    let tok = tokens.get(cur)
                        .ok_or_else(|| format!("expected PascalCase type for @{}, got EOF", bind_name))?;
                    let value = match &tok.token {
                        Token::PascalIdent(s) => PegValue::Text(s.clone()),
                        _ => return Err(format!("cannot bind @{} to {:?} (expected PascalCase type)", bind_name, tok.token)),
                    };
                    bindings.insert(bind_name.clone(), value);
                    cur += 1;
                }
                PatElem::BindLit(bind_name) => {
                    let tok = tokens.get(cur)
                        .ok_or_else(|| format!("expected literal for @{}, got EOF", bind_name))?;
                    let value = match &tok.token {
                        Token::Integer(n) => PegValue::Int(*n),
                        Token::Float(s) => PegValue::Float(s.parse().unwrap_or(0.0)),
                        Token::StringLit(s) => PegValue::Text(s.clone()),
                        _ => return Err(format!("cannot bind @{} to {:?} (expected literal)", bind_name, tok.token)),
                    };
                    bindings.insert(bind_name.clone(), value);
                    cur += 1;
                }
            }
        }

        // Evaluate guard (if present) — after pattern matches, before building result
        if let Some(ref guard) = arm.guard {
            if !self.evaluate_guard(guard, &bindings)? {
                return Err(format!("guard failed: {}", guard.condition));
            }
        }

        let start = if pos < tokens.len() { tokens[pos].span.start as i64 } else { 0 };
        let end = if cur > 0 && cur <= tokens.len() { tokens[cur.saturating_sub(1)].span.end as i64 } else { start };
        let value = self.build_result(&arm.result, &bindings, start, end)?;
        Ok((value, cur))
    }

    /// Evaluate a grammar guard against the ParsingWorld.
    /// Guard condition references @World.xxx queries and @BoundVars from pattern.
    fn evaluate_guard(&self, guard: &super::ParseGuard, bindings: &Bindings) -> Result<bool, String> {
        let cond = &guard.condition;

        // @World.Context(Kind) — check current context matches
        if cond.starts_with("@World.Context(") {
            if let Some(kind_str) = cond.strip_prefix("@World.Context(").and_then(|s| s.strip_suffix(')')) {
                let current = self.ctx_stack.last().unwrap_or(&CtxKind::Root);
                return Ok(&format!("{:?}", current) == kind_str);
            }
        }

        // @World.ParentContext(Kind)
        if cond.starts_with("@World.ParentContext(") {
            if let Some(kind_str) = cond.strip_prefix("@World.ParentContext(").and_then(|s| s.strip_suffix(')')) {
                let parent = if self.ctx_stack.len() >= 2 {
                    &self.ctx_stack[self.ctx_stack.len() - 2]
                } else {
                    &CtxKind::Root
                };
                return Ok(&format!("{:?}", parent) == kind_str);
            }
        }

        // @World.IsFfi(@Name) — resolve @Name from bindings, check FFI registry
        if cond.starts_with("@World.IsFfi(") {
            if let Some(var) = cond.strip_prefix("@World.IsFfi(@").and_then(|s| s.strip_suffix(')')) {
                let name = self.resolve_guard_var(var, bindings);
                return Ok(self.world.ffi_entries.iter().any(|e| e.aski_name == name));
            }
        }

        // @World.IsLowerFirst(@Name)
        if cond.starts_with("@World.IsLowerFirst(") {
            if let Some(var) = cond.strip_prefix("@World.IsLowerFirst(@").and_then(|s| s.strip_suffix(')')) {
                let name = self.resolve_guard_var(var, bindings);
                return Ok(!name.is_empty() && name.chars().next().map(|c| c.is_lowercase()).unwrap_or(false));
            }
        }

        // @World.TypeExists(@Name)
        if cond.starts_with("@World.TypeExists(") {
            if let Some(var) = cond.strip_prefix("@World.TypeExists(@").and_then(|s| s.strip_suffix(')')) {
                let name = self.resolve_guard_var(var, bindings);
                return Ok(self.world.types.iter().any(|t| t.name == name));
            }
        }

        // @World.IsVariant(@Name)
        if cond.starts_with("@World.IsVariant(") {
            if let Some(var) = cond.strip_prefix("@World.IsVariant(@").and_then(|s| s.strip_suffix(')')) {
                let name = self.resolve_guard_var(var, bindings);
                return Ok(self.world.variant_ofs.iter().any(|v| v.variant_name == name));
            }
        }

        // @World.IsBlockDelimiter() — true if next significant token closes a block
        // NOTE: this needs token access; guard evaluation doesn't have tokens.
        // Instead, this is handled by the interpreter's iterative stmts handler.
        // In grammar, use it as a marker: the interpreter recognizes it.
        if cond == "@World.IsBlockDelimiter()" {
            // Can't evaluate without token access — return true to signal "check externally"
            // This guard is used by the interpreter to decide when to stop iterating
            return Ok(true);
        }

        // Negation: !@World.xxx
        if cond.starts_with("!") {
            let inner = &cond[1..];
            let inner_guard = super::ParseGuard { condition: inner.to_string() };
            return self.evaluate_guard(&inner_guard, bindings).map(|v| !v);
        }

        Err(format!("unknown guard condition: {}", cond))
    }

    /// Resolve a guard variable from bindings to its text value.
    fn resolve_guard_var(&self, var_name: &str, bindings: &Bindings) -> String {
        bindings.get(var_name)
            .map(|v| peg_value_text(v))
            .unwrap_or_default()
    }

    // ── Result construction ─────────────────────────────────

    fn build_result(
        &mut self,
        spec: &ResultSpec,
        bindings: &Bindings,
        start: i64,
        end: i64,
    ) -> Result<PegValue, String> {
        let resolved: Vec<PegValue> = spec.args.iter().map(|arg| {
            match arg {
                ResultArg::Bound(name) => {
                    // Implicit World bindings — query the parse context
                    if name == "World.Context" {
                        let ctx = self.ctx_stack.last().unwrap_or(&CtxKind::Root);
                        Ok(PegValue::Text(format!("{:?}", ctx)))
                    } else if name == "World.ParentContext" {
                        let ctx = if self.ctx_stack.len() >= 2 {
                            &self.ctx_stack[self.ctx_stack.len() - 2]
                        } else {
                            &CtxKind::Root
                        };
                        Ok(PegValue::Text(format!("{:?}", ctx)))
                    } else if name.starts_with("World.") {
                        // Future: @World.TypeExists, @World.IsFfi, etc.
                        Err(format!("unknown World binding: @{}", name))
                    } else {
                        bindings.get(name)
                            .cloned()
                            .ok_or_else(|| format!("unbound: @{}", name))
                    }
                },
                ResultArg::RuleResult(name) => bindings.get(name)
                    .cloned()
                    .ok_or_else(|| format!("rule result not found: <{}>", name)),
                ResultArg::Nested(nested) => self.build_result(nested, bindings, start, end),
                ResultArg::Literal(s) => Ok(PegValue::Text(s.clone())),
            }
        }).collect::<Result<Vec<_>, _>>()?;

        let ctx_depth = self.ctx_stack.len();
        let result = self.dispatch_constructor(&spec.constructor, &resolved, start, end);
        // Restore context stack
        while self.ctx_stack.len() > ctx_depth {
            self.ctx_stack.pop();
        }
        result
    }

    /// Dispatch a constructor — PEG-internal cases handled here,
    /// everything else goes to commit_node.
    fn dispatch_constructor(
        &mut self,
        name: &str,
        args: &[PegValue],
        start: i64,
        end: i64,
    ) -> Result<PegValue, String> {
        match name {
            // ── List constructors ────────────────────────────────
            "Cons" => {
                if args.len() != 2 {
                    return Err(format!("Cons requires 2 args, got {}", args.len()));
                }
                let head = &args[0];
                let tail = args[1].as_node_list()?;
                match head {
                    PegValue::NodeId(id) => {
                        let mut list = vec![*id];
                        list.extend(tail);
                        Ok(PegValue::NodeList(list))
                    }
                    PegValue::Text(_) | PegValue::Int(_) | PegValue::Float(_) | PegValue::TaggedList(_) => {
                        // For non-node values (Text for supertraits, TaggedList for fold ops),
                        // wrap in a temporary node so it can participate in NodeList
                        let id = self.make_leaf("ConsElem", &peg_value_text(head), start, end);
                        // Store the original PegValue kind in the node's constructor for recovery
                        match head {
                            PegValue::TaggedList(_) => {
                                // TaggedList items keep their identity — we store in a "TaggedOp" node
                                // and recover in FoldLeft/FoldPost
                                if let Some(n) = self.world.parse_nodes.iter_mut().find(|n| n.id == id) {
                                    n.constructor = "TaggedOp".to_string();
                                }
                                // Store tagged list components as children
                                if let PegValue::TaggedList(parts) = head {
                                    for (i, part) in parts.iter().enumerate() {
                                        let child_id = match part {
                                            PegValue::Text(s) => self.make_leaf("OpText", s, start, end),
                                            PegValue::NodeId(nid) => *nid,
                                            PegValue::NodeList(ids) => {
                                                let list_id = self.make_leaf("OpArgList", "", start, end);
                                                for (j, aid) in ids.iter().enumerate() {
                                                    self.add_child(list_id, j as i64, *aid);
                                                }
                                                list_id
                                            }
                                            _ => self.make_leaf("OpPart", &peg_value_text(part), start, end),
                                        };
                                        self.add_child(id, i as i64, child_id);
                                    }
                                }
                            }
                            _ => {}
                        }
                        let mut list = vec![id];
                        list.extend(tail);
                        Ok(PegValue::NodeList(list))
                    }
                    PegValue::NodeList(ids) => {
                        // NodeList as head — this is a list-of-lists scenario, wrap
                        let id = self.make_leaf("ListGroup", "", start, end);
                        for (i, cid) in ids.iter().enumerate() {
                            self.add_child(id, i as i64, *cid);
                        }
                        let mut list = vec![id];
                        list.extend(tail);
                        Ok(PegValue::NodeList(list))
                    }
                    PegValue::None => {
                        // Skip None heads
                        Ok(PegValue::NodeList(tail))
                    }
                }
            }
            "Nil" => Ok(PegValue::NodeList(vec![])),
            "Singleton" => {
                if args.len() != 1 {
                    return Err(format!("Singleton requires 1 arg, got {}", args.len()));
                }
                match &args[0] {
                    PegValue::NodeId(id) => Ok(PegValue::NodeList(vec![*id])),
                    PegValue::Text(s) => {
                        let id = self.make_leaf("ConsElem", s, start, end);
                        Ok(PegValue::NodeList(vec![id]))
                    }
                    PegValue::TaggedList(_) => {
                        // Same as Cons for tagged lists
                        let id = self.make_leaf("TaggedOp", "", start, end);
                        if let PegValue::TaggedList(parts) = &args[0] {
                            for (i, part) in parts.iter().enumerate() {
                                let child_id = match part {
                                    PegValue::Text(s) => self.make_leaf("OpText", s, start, end),
                                    PegValue::NodeId(nid) => *nid,
                                    PegValue::NodeList(ids) => {
                                        let list_id = self.make_leaf("OpArgList", "", start, end);
                                        for (j, aid) in ids.iter().enumerate() {
                                            self.add_child(list_id, j as i64, *aid);
                                        }
                                        list_id
                                    }
                                    _ => self.make_leaf("OpPart", &peg_value_text(part), start, end),
                                };
                                self.add_child(id, i as i64, child_id);
                            }
                        }
                        Ok(PegValue::NodeList(vec![id]))
                    }
                    other => {
                        let id = self.make_leaf("ConsElem", &peg_value_text(other), start, end);
                        Ok(PegValue::NodeList(vec![id]))
                    }
                }
            }

            // ── Passthrough ─────────────────────────────────────
            "Passthrough" => Ok(args[0].clone()),

            // ── FoldLeft: base + list of Op pairs → left-assoc chain ──
            "FoldLeft" => {
                let ops = args[1].as_node_list()?;
                if ops.is_empty() { return Ok(args[0].clone()); }
                let mut lhs_id = match &args[0] {
                    PegValue::NodeId(id) => *id,
                    _ => return Err("FoldLeft: base must be NodeId".into()),
                };
                for op_node_id in ops {
                    // Each op is a TaggedOp node with children [OpText, rhs]
                    let op_children = aski_core::query_parse_children(&self.world, op_node_id);
                    if op_children.len() >= 2 {
                        let op_text = op_children[0].text.clone();
                        let rhs_id = op_children[1].id;
                        let fold_id = self.ids.next();
                        self.world.parse_nodes.push(ParseNode {
                            id: fold_id,
                            constructor: "BinOp".to_string(),
                            ctx: CtxKind::Expr,
                            parent_id: 0,
                            status: ParseStatus::Committed,
                            text: op_text,
                            token_start: start,
                            token_end: end,
                        });
                        self.add_child(fold_id, 0, lhs_id);
                        self.add_child(fold_id, 1, rhs_id);
                        lhs_id = fold_id;
                    } else if op_children.len() == 1 {
                        // Single child — might be a passthrough op
                        let rhs_id = op_children[0].id;
                        let fold_id = self.ids.next();
                        self.world.parse_nodes.push(ParseNode {
                            id: fold_id,
                            constructor: "BinOp".to_string(),
                            ctx: CtxKind::Expr,
                            parent_id: 0,
                            status: ParseStatus::Committed,
                            text: String::new(),
                            token_start: start,
                            token_end: end,
                        });
                        self.add_child(fold_id, 0, lhs_id);
                        self.add_child(fold_id, 1, rhs_id);
                        lhs_id = fold_id;
                    }
                }
                Ok(PegValue::NodeId(lhs_id))
            }
            "Op" => {
                // (op_string, rhs_expr) pair for FoldLeft — produce TaggedList
                Ok(PegValue::TaggedList(vec![args[0].clone(), args[1].clone()]))
            }

            // ── FoldPost: base + postfix operations ─────────────
            "FoldPost" => {
                let ops = args[1].as_node_list()?;
                if ops.is_empty() { return Ok(args[0].clone()); }
                let mut expr_id = match &args[0] {
                    PegValue::NodeId(id) => *id,
                    _ => return Err("FoldPost: base must be NodeId".into()),
                };
                for op_node_id in ops {
                    let op_node = self.world.parse_nodes.iter().find(|n| n.id == op_node_id).cloned();
                    if let Some(op_node) = op_node {
                        if op_node.constructor == "TaggedOp" {
                            let op_children: Vec<_> = aski_core::query_parse_children(&self.world, op_node_id)
                                .into_iter().cloned().collect();
                            if op_children.is_empty() { continue; }
                            let tag = &op_children[0].text;
                            match tag.as_str() {
                                "method" => {
                                    let method_name = if op_children.len() > 1 { op_children[1].text.clone() } else { String::new() };
                                    let fold_id = self.ids.next();
                                    self.world.parse_nodes.push(ParseNode {
                                        id: fold_id,
                                        constructor: "MethodCall".to_string(),
                                        ctx: CtxKind::Expr,
                                        parent_id: 0,
                                        status: ParseStatus::Committed,
                                        text: method_name,
                                        token_start: start,
                                        token_end: end,
                                    });
                                    self.add_child(fold_id, 0, expr_id);
                                    // args are in the third child (OpArgList)
                                    if op_children.len() > 2 {
                                        let arg_ids: Vec<i64> = aski_core::query_parse_children(&self.world, op_children[2].id)
                                            .iter().map(|a| a.id).collect();
                                        for (i, aid) in arg_ids.iter().enumerate() {
                                            self.add_child(fold_id, (i + 1) as i64, *aid);
                                        }
                                    }
                                    expr_id = fold_id;
                                }
                                "access" => {
                                    let field_name = if op_children.len() > 1 { op_children[1].text.clone() } else { String::new() };
                                    let fold_id = self.ids.next();
                                    self.world.parse_nodes.push(ParseNode {
                                        id: fold_id,
                                        constructor: "Access".to_string(),
                                        ctx: CtxKind::Expr,
                                        parent_id: 0,
                                        status: ParseStatus::Committed,
                                        text: field_name,
                                        token_start: start,
                                        token_end: end,
                                    });
                                    self.add_child(fold_id, 0, expr_id);
                                    expr_id = fold_id;
                                }
                                "error_prop" => {
                                    let fold_id = self.ids.next();
                                    self.world.parse_nodes.push(ParseNode {
                                        id: fold_id,
                                        constructor: "ErrorProp".to_string(),
                                        ctx: CtxKind::Expr,
                                        parent_id: 0,
                                        status: ParseStatus::Committed,
                                        text: String::new(),
                                        token_start: start,
                                        token_end: end,
                                    });
                                    self.add_child(fold_id, 0, expr_id);
                                    expr_id = fold_id;
                                }
                                "range_excl" => {
                                    let end_id = if op_children.len() > 1 { op_children[1].id } else { continue };
                                    let fold_id = self.ids.next();
                                    self.world.parse_nodes.push(ParseNode {
                                        id: fold_id,
                                        constructor: "RangeExclusive".to_string(),
                                        ctx: CtxKind::Expr,
                                        parent_id: 0,
                                        status: ParseStatus::Committed,
                                        text: String::new(),
                                        token_start: start,
                                        token_end: end,
                                    });
                                    self.add_child(fold_id, 0, expr_id);
                                    self.add_child(fold_id, 1, end_id);
                                    expr_id = fold_id;
                                }
                                "range_incl" => {
                                    let end_id = if op_children.len() > 1 { op_children[1].id } else { continue };
                                    let fold_id = self.ids.next();
                                    self.world.parse_nodes.push(ParseNode {
                                        id: fold_id,
                                        constructor: "RangeInclusive".to_string(),
                                        ctx: CtxKind::Expr,
                                        parent_id: 0,
                                        status: ParseStatus::Committed,
                                        text: String::new(),
                                        token_start: start,
                                        token_end: end,
                                    });
                                    self.add_child(fold_id, 0, expr_id);
                                    self.add_child(fold_id, 1, end_id);
                                    expr_id = fold_id;
                                }
                                _ => {} // unknown tag, skip
                            }
                        } else {
                            // Non-tagged op — treat as direct child
                        }
                    }
                }
                Ok(PegValue::NodeId(expr_id))
            }
            // Postfix op constructors — produce TaggedLists for FoldPost
            "MethodOp" => {
                if args.len() >= 2 {
                    Ok(PegValue::TaggedList(vec![PegValue::Text("method".into()), args[0].clone(), args[1].clone()]))
                } else {
                    // No-args method call (from guard: FFI or camelCase name)
                    Ok(PegValue::TaggedList(vec![PegValue::Text("method".into()), args[0].clone()]))
                }
            }
            "AccessOp" => Ok(PegValue::TaggedList(vec![PegValue::Text("access".into()), args[0].clone()])),
            "ErrorPropOp" => Ok(PegValue::TaggedList(vec![PegValue::Text("error_prop".into())])),
            "RangeExclOp" => Ok(PegValue::TaggedList(vec![PegValue::Text("range_excl".into()), args[0].clone()])),
            "RangeInclOp" => Ok(PegValue::TaggedList(vec![PegValue::Text("range_incl".into()), args[0].clone()])),

            // ── Domain → Type/Variant relations ─────────────────
            "Domain" => {
                let name = args[0].as_text()?;
                let variant_ids = args[1].as_node_list()?;
                let id = self.ids.next();
                self.world.parse_nodes.push(ParseNode {
                    id,
                    constructor: "Domain".to_string(),
                    ctx: CtxKind::Item,
                    parent_id: 0,
                    status: ParseStatus::Committed,
                    text: name.clone(),
                    token_start: start,
                    token_end: end,
                });
                // Type relation
                self.world.types.push(aski_core::Type {
                    id, name: name.clone(), form: aski_core::TypeForm::Domain, parent: 0,
                });
                // Variants
                for (i, vid) in variant_ids.iter().enumerate() {
                    self.add_child(id, i as i64, *vid);
                    if let Some(vnode) = self.world.parse_nodes.iter().find(|n| n.id == *vid) {
                        let vname = vnode.text.clone();
                        let vconstructor = vnode.constructor.clone();
                        // Check for wrapped type (ParenVariant with single child that's a type)
                        let wraps = if vconstructor == "ParenVariant" || vconstructor == "Variant" {
                            let vchildren: Vec<_> = self.world.parse_children.iter()
                                .filter(|c| c.parent_id == *vid)
                                .collect();
                            if vchildren.len() == 1 {
                                // Single child — check if it's a type wrapping
                                let child_node = self.world.parse_nodes.iter()
                                    .find(|n| n.id == vchildren[0].child_id);
                                if let Some(cn) = child_node {
                                    if cn.constructor == "UnitVariant" || cn.constructor == "Variant" {
                                        // Single bare name → type wrap
                                        cn.text.clone()
                                    } else {
                                        String::new()
                                    }
                                } else { String::new() }
                            } else { String::new() }
                        } else { String::new() };
                        self.world.variants.push(aski_core::Variant {
                            type_id: id, ordinal: i as i64, name: vname.clone(), contains_type: wraps,
                        });
                        // Handle struct variants (fields)
                        if vconstructor == "StructVariant" {
                            let struct_id = self.ids.next();
                            self.world.types.push(aski_core::Type {
                                id: struct_id, name: vname.clone(),
                                form: aski_core::TypeForm::Struct, parent: id,
                            });
                            let field_children: Vec<_> = self.world.parse_children.iter()
                                .filter(|c| c.parent_id == *vid)
                                .map(|c| c.child_id)
                                .collect();
                            for (ford, fid) in field_children.iter().enumerate() {
                                if let Some(fnode) = self.world.parse_nodes.iter().find(|n| n.id == *fid) {
                                    if fnode.constructor == "Field" || fnode.constructor == "BorrowedField" {
                                        let fname = fnode.text.clone();
                                        // Get type from first child
                                        let ftype_children: Vec<_> = self.world.parse_children.iter()
                                            .filter(|c| c.parent_id == *fid)
                                            .map(|c| c.child_id)
                                            .collect();
                                        let ftype = ftype_children.first()
                                            .and_then(|tid| self.world.parse_nodes.iter().find(|n| n.id == *tid))
                                            .map(|n| n.text.clone())
                                            .unwrap_or_default();
                                        self.world.fields.push(aski_core::Field {
                                            type_id: struct_id, ordinal: ford as i64,
                                            name: fname, field_type: ftype,
                                        });
                                    }
                                }
                            }
                        }
                        // Handle inline sub-variants (ParenVariant with multiple children)
                        if vconstructor == "ParenVariant" {
                            let sub_children: Vec<_> = self.world.parse_children.iter()
                                .filter(|c| c.parent_id == *vid)
                                .map(|c| c.child_id)
                                .collect();
                            if sub_children.len() > 1 {
                                let sub_type_id = self.ids.next();
                                self.world.types.push(aski_core::Type {
                                    id: sub_type_id, name: vname.clone(),
                                    form: aski_core::TypeForm::Domain, parent: id,
                                });
                                for (sord, scid) in sub_children.iter().enumerate() {
                                    if let Some(sn) = self.world.parse_nodes.iter().find(|n| n.id == *scid) {
                                        let sw = String::new(); // sub-variants don't wrap
                                        self.world.variants.push(aski_core::Variant {
                                            type_id: sub_type_id, ordinal: sord as i64,
                                            name: sn.text.clone(), contains_type: sw,
                                        });
                                    }
                                }
                            }
                        }
                    }
                }
                Ok(PegValue::NodeId(id))
            }

            // ── Struct → Type/Field relations ───────────────────
            "Struct" => {
                let name = args[0].as_text()?;
                let field_ids = args[1].as_node_list()?;
                let id = self.ids.next();
                self.world.parse_nodes.push(ParseNode {
                    id,
                    constructor: "Struct".to_string(),
                    ctx: CtxKind::Item,
                    parent_id: 0,
                    status: ParseStatus::Committed,
                    text: name.clone(),
                    token_start: start,
                    token_end: end,
                });
                self.world.types.push(aski_core::Type {
                    id, name: name.clone(), form: aski_core::TypeForm::Struct, parent: 0,
                });
                for (i, fid) in field_ids.iter().enumerate() {
                    self.add_child(id, i as i64, *fid);
                    if let Some(fnode) = self.world.parse_nodes.iter().find(|n| n.id == *fid) {
                        let fname = fnode.text.clone();
                        let ftype_children: Vec<i64> = self.world.parse_children.iter()
                            .filter(|c| c.parent_id == *fid)
                            .map(|c| c.child_id)
                            .collect();
                        let ftype = ftype_children.first()
                            .and_then(|tid| self.world.parse_nodes.iter().find(|n| n.id == *tid))
                            .map(|n| n.text.clone())
                            .unwrap_or_default();
                        let ftype_str = if fnode.constructor == "BorrowedField" {
                            format!("&{}", ftype)
                        } else {
                            ftype
                        };
                        self.world.fields.push(aski_core::Field {
                            type_id: id, ordinal: i as i64,
                            name: fname, field_type: ftype_str,
                        });
                    }
                }
                Ok(PegValue::NodeId(id))
            }

            // ── Foreign function block → FfiEntry relations ─────
            "ForeignBlock" => {
                let library = args[0].as_text()?;
                let func_ids = args[1].as_node_list()?;
                let id = self.ids.next();
                self.world.parse_nodes.push(ParseNode {
                    id,
                    constructor: "ForeignBlock".to_string(),
                    ctx: CtxKind::Ffi,
                    parent_id: 0,
                    status: ParseStatus::Committed,
                    text: library.clone(),
                    token_start: start,
                    token_end: end,
                });
                for (i, fid) in func_ids.iter().enumerate() {
                    self.add_child(id, i as i64, *fid);
                    if let Some(fnode) = self.world.parse_nodes.iter().find(|n| n.id == *fid) {
                        if fnode.constructor == "ForeignFunc" {
                            let func_name = fnode.text.clone();
                            // Children: params..., ReturnType, ExternName
                            let fc: Vec<_> = aski_core::query_parse_children(&self.world, *fid)
                                .into_iter().cloned().collect();
                            let ret_type = fc.iter()
                                .find(|c| c.constructor == "ReturnType")
                                .map(|c| c.text.clone())
                                .unwrap_or_default();
                            let extern_name = fc.iter()
                                .find(|c| c.constructor == "ExternName")
                                .map(|c| c.text.clone())
                                .unwrap_or_else(|| func_name.clone());
                            let rust_span = match library.as_str() {
                                "Cast" => aski_core::RustSpan::Cast,
                                "Std" => aski_core::RustSpan::MethodCall,
                                "VecOps" => aski_core::RustSpan::BlockExpr,
                                _ => aski_core::RustSpan::FreeCall,
                            };
                            self.world.ffi_entries.push(aski_core::FfiEntry {
                                library: library.clone(),
                                aski_name: func_name,
                                rust_name: extern_name,
                                span: rust_span,
                                return_type: ret_type,
                            });
                        }
                    }
                }
                Ok(PegValue::NodeId(id))
            }
            "ForeignFunc" => {
                let name = args[0].as_text()?;
                let param_ids = args[1].as_node_list()?;
                let return_type_text = self.resolve_type_text(&args[2]);
                let extern_name = args[3].as_text()?;
                let id = self.ids.next();
                self.world.parse_nodes.push(ParseNode {
                    id,
                    constructor: "ForeignFunc".to_string(),
                    ctx: CtxKind::Ffi,
                    parent_id: 0,
                    status: ParseStatus::Committed,
                    text: name,
                    token_start: start,
                    token_end: end,
                });
                for (i, pid) in param_ids.iter().enumerate() {
                    self.add_child(id, i as i64, *pid);
                }
                let ret_id = self.make_leaf("ReturnType", &return_type_text, start, end);
                self.add_child(id, param_ids.len() as i64, ret_id);
                let ext_id = self.make_leaf("ExternName", &extern_name, start, end);
                self.add_child(id, (param_ids.len() + 1) as i64, ext_id);
                Ok(PegValue::NodeId(id))
            }

            // ── Grammar rule self-definition ────────────────────
            "GrammarRuleItem" | "GrammarRule" => {
                let name = args[0].as_text()?;
                let arm_ids = if args.len() > 1 { args[1].as_node_list().unwrap_or_default() } else { vec![] };
                let id = self.ids.next();
                self.world.parse_nodes.push(ParseNode {
                    id,
                    constructor: "GrammarRule".to_string(),
                    ctx: CtxKind::Item,
                    parent_id: 0,
                    status: ParseStatus::Committed,
                    text: name.clone(),
                    token_start: start,
                    token_end: end,
                });
                // Store in Rule/Arm/PatElem relations
                let rule_id = self.ids.next();
                self.world.rules.push(aski_core::Rule {
                    id: rule_id, name: name.clone(), dialect: String::new(),
                });
                for (i, aid) in arm_ids.iter().enumerate() {
                    self.add_child(id, i as i64, *aid);
                }
                Ok(PegValue::NodeId(id))
            }
            "RuleArm" => {
                let id = self.ids.next();
                self.world.parse_nodes.push(ParseNode {
                    id,
                    constructor: "RuleArm".to_string(),
                    ctx: CtxKind::Item,
                    parent_id: 0,
                    status: ParseStatus::Committed,
                    text: String::new(),
                    token_start: start,
                    token_end: end,
                });
                // First arg is pattern list, second is result
                for (i, arg) in args.iter().enumerate() {
                    match arg {
                        PegValue::NodeId(cid) => {
                            self.add_child(id, i as i64, *cid);
                        }
                        PegValue::NodeList(ids) => {
                            let group_id = self.make_leaf(
                                if i == 0 { "PatternGroup" } else { "ResultGroup" },
                                "", start, end,
                            );
                            for (j, cid) in ids.iter().enumerate() {
                                self.add_child(group_id, j as i64, *cid);
                            }
                            self.add_child(id, i as i64, group_id);
                        }
                        _ => {}
                    }
                }
                Ok(PegValue::NodeId(id))
            }
            "NonTerminal" => Ok(PegValue::Text(format!("<{}>", args[0].as_text()?))),
            "Terminal" => Ok(PegValue::Text(args[0].as_text()?)),
            "Binding" | "RestBind" => {
                let prefix = if name == "RestBind" { "/@" } else { "@" };
                Ok(PegValue::Text(format!("{}{}", prefix, args[0].as_text()?)))
            }
            "Bound" | "RuleRef" => Ok(PegValue::Text(args[0].as_text()?)),
            "Constructor" | "Nested" => {
                let ctor_name = args[0].as_text()?;
                let id = self.make_leaf(&ctor_name, "", start, end);
                if args.len() > 1 {
                    for (i, arg) in args[1..].iter().enumerate() {
                        match arg {
                            PegValue::NodeId(cid) => {
                                self.add_child(id, i as i64, *cid);
                            }
                            _ => {
                                let cid = self.make_leaf("Arg", &peg_value_text(arg), start, end);
                                self.add_child(id, i as i64, cid);
                            }
                        }
                    }
                }
                Ok(PegValue::NodeId(id))
            }

            // ── Module header ───────────────────────────────────
            "ModuleHeader" => {
                let name = args[0].as_text()?;
                let export_ids = args[1].as_node_list()?;
                let import_ids = if args.len() > 2 { args[2].as_node_list().unwrap_or_default() } else { vec![] };
                let id = self.ids.next();
                self.world.parse_nodes.push(ParseNode {
                    id,
                    constructor: "ModuleHeader".to_string(),
                    ctx: CtxKind::Module,
                    parent_id: 0,
                    status: ParseStatus::Committed,
                    text: name,
                    token_start: start,
                    token_end: end,
                });
                let mut ord = 0i64;
                // Exports: ConsElem nodes → rename to ExportName for clarity
                for eid in export_ids {
                    if let Some(n) = self.world.parse_nodes.iter_mut().find(|n| n.id == eid) {
                        if n.constructor == "ConsElem" {
                            n.constructor = "ExportName".to_string();
                        }
                    }
                    self.add_child(id, ord, eid);
                    ord += 1;
                }
                for iid in import_ids {
                    self.add_child(id, ord, iid);
                    ord += 1;
                }
                Ok(PegValue::NodeId(id))
            }
            "ExportName" => {
                let name = args[0].as_text()?;
                let id = self.make_leaf("ExportName", &name, start, end);
                Ok(PegValue::NodeId(id))
            }
            "NamedImport" => {
                let module = args[0].as_text()?;
                let item_ids = args[1].as_node_list()?;
                let id = self.ids.next();
                self.world.parse_nodes.push(ParseNode {
                    id,
                    constructor: "NamedImport".to_string(),
                    ctx: CtxKind::Module,
                    parent_id: 0,
                    status: ParseStatus::Committed,
                    text: module,
                    token_start: start,
                    token_end: end,
                });
                for (i, iid) in item_ids.iter().enumerate() {
                    // Rename ConsElem children to ImportItem for clarity
                    if let Some(n) = self.world.parse_nodes.iter_mut().find(|n| n.id == *iid) {
                        if n.constructor == "ConsElem" {
                            n.constructor = "ImportItem".to_string();
                        }
                    }
                    self.add_child(id, i as i64, *iid);
                }
                Ok(PegValue::NodeId(id))
            }
            "WildcardImport" => {
                let module = args[0].as_text()?;
                let id = self.make_leaf("WildcardImport", &module, start, end);
                Ok(PegValue::NodeId(id))
            }

            // ── Variant constructors ────────────────────────────
            "UnitVariant" => {
                let name = args[0].as_text()?;
                let id = self.make_node("UnitVariant", &name, CtxKind::Item, start, end);
                Ok(PegValue::NodeId(id))
            }
            "ParenVariant" => {
                let name = args[0].as_text()?;
                let inner_ids = args[1].as_node_list()?;
                let id = self.ids.next();
                self.world.parse_nodes.push(ParseNode {
                    id,
                    constructor: "ParenVariant".to_string(),
                    ctx: CtxKind::Item,
                    parent_id: 0,
                    status: ParseStatus::Committed,
                    text: name,
                    token_start: start,
                    token_end: end,
                });
                for (i, cid) in inner_ids.iter().enumerate() {
                    self.add_child(id, i as i64, *cid);
                }
                Ok(PegValue::NodeId(id))
            }
            "StructVariant" => {
                let name = args[0].as_text()?;
                let field_ids = args[1].as_node_list()?;
                let id = self.ids.next();
                self.world.parse_nodes.push(ParseNode {
                    id,
                    constructor: "StructVariant".to_string(),
                    ctx: CtxKind::Item,
                    parent_id: 0,
                    status: ParseStatus::Committed,
                    text: name,
                    token_start: start,
                    token_end: end,
                });
                for (i, fid) in field_ids.iter().enumerate() {
                    self.add_child(id, i as i64, *fid);
                }
                Ok(PegValue::NodeId(id))
            }

            // ── Field constructors ──────────────────────────────
            "Field" => {
                let name = args[0].as_text()?;
                let type_text = self.resolve_type_text(&args[1]);
                let id = self.make_node("Field", &name, CtxKind::Item, start, end);
                let tid = self.make_leaf("TypeRef", &type_text, start, end);
                self.add_child(id, 0, tid);
                Ok(PegValue::NodeId(id))
            }
            "BorrowedField" => {
                let name = args[0].as_text()?;
                let type_text = self.resolve_type_text(&args[1]);
                let id = self.make_node("BorrowedField", &name, CtxKind::Item, start, end);
                let tid = self.make_leaf("TypeRef", &format!("&{}", type_text), start, end);
                self.add_child(id, 0, tid);
                Ok(PegValue::NodeId(id))
            }

            // ── Param constructors ──────────────────────────────
            // BorrowSelf, MutBorrowSelf, OwnedSelf → handled by commit_node
            // Borrow, MutBorrow → commit_node (remapped to BorrowParam, MutBorrowParam)
            "Named" => {
                let name = args[0].as_text()?;
                let type_text = self.resolve_type_text(&args[1]);
                let id = self.make_node("NamedParam", &name, CtxKind::Param, start, end);
                let tid = self.make_leaf("TypeRef", &type_text, start, end);
                self.add_child(id, 0, tid);
                Ok(PegValue::NodeId(id))
            }
            // Owned → commit_node (remapped to OwnedParam)

            // ── TypeRef constructors ────────────────────────────
            // SelfType, NamedType → commit_node
            "BorrowedType" => {
                let inner_text = self.resolve_type_text(&args[0]);
                Ok(PegValue::NodeId(self.make_node("BorrowedType", &format!("&{}", inner_text), CtxKind::TypeRef, start, end)))
            }
            "ParameterizedType" => {
                let name = args[0].as_text()?;
                let param_ids = args[1].as_node_list()?;
                let params: Vec<String> = param_ids.iter()
                    .filter_map(|pid| self.world.parse_nodes.iter().find(|n| n.id == *pid))
                    .map(|n| n.text.clone())
                    .collect();
                let text = format!("{}({})", name, params.join(" "));
                let id = self.make_node("ParameterizedType", &text, CtxKind::TypeRef, start, end);
                for (i, pid) in param_ids.iter().enumerate() {
                    self.add_child(id, i as i64, *pid);
                }
                Ok(PegValue::NodeId(id))
            }
            "BoundType" => {
                let bound_text = self.resolve_type_text(&args[0]);
                Ok(PegValue::NodeId(self.make_node("BoundType", &bound_text, CtxKind::TypeRef, start, end)))
            }

            // ── TraitBound constructors ─────────────────────────
            "CompoundBound" => {
                let first = args[0].as_text()?;
                let rest_text = self.resolve_type_text(&args[1]);
                let text = format!("{}&{}", first, rest_text);
                Ok(PegValue::NodeId(self.make_node("CompoundBound", &text, CtxKind::TypeRef, start, end)))
            }
            "SingleBound" => {
                let name = args[0].as_text()?;
                Ok(PegValue::NodeId(self.make_node("SingleBound", &name, CtxKind::TypeRef, start, end)))
            }

            // ── Item constructors ───────────────────────────────
            // TraitDecl, TraitImpl, TypeImpl → commit_node
            "Const" => {
                let name = args[0].as_text()?;
                let type_text = self.resolve_type_text(&args[1]);
                let id = self.make_node("Const", &name, CtxKind::Item, start, end);
                let tid = self.make_leaf("TypeRef", &type_text, start, end);
                self.add_child(id, 0, tid);
                if args.len() > 2 {
                    let body_id = self.resolve_child_id(&args[2]);
                    self.add_child(id, 1, body_id);
                }
                Ok(PegValue::NodeId(id))
            }
            // Main → commit_node
            "TypeAlias" => {
                let name = args[0].as_text()?;
                let type_text = self.resolve_type_text(&args[1]);
                let id = self.make_node("TypeAlias", &name, CtxKind::Item, start, end);
                let tid = self.make_leaf("TypeRef", &type_text, start, end);
                self.add_child(id, 0, tid);
                Ok(PegValue::NodeId(id))
            }

            // ── Supertrait ──────────────────────────────────────
            "Supertrait" => {
                let name = args[0].as_text()?;
                let id = self.make_leaf("Supertrait", &name, start, end);
                Ok(PegValue::NodeId(id))
            }
            "Methods" => Ok(args[0].clone()),

            // ── Method constructors ─────────────────────────────
            // MethodSig/MethodDef: grammar now uses Typed/Untyped variants
            // No arg-count dispatch needed — constructor name tells us
            "MethodSig" | "MethodSigTyped" => {
                let name = args[0].as_text()?;
                let param_ids = args[1].as_node_list()?;
                let ret_text = self.resolve_type_text(&args[2]);
                let id = self.make_node("MethodSig", &name, CtxKind::Item, start, end);
                for (i, pid) in param_ids.iter().enumerate() {
                    self.add_child(id, i as i64, *pid);
                }
                let ret_id = self.make_leaf("ReturnType", &ret_text, start, end);
                self.add_child(id, param_ids.len() as i64, ret_id);
                Ok(PegValue::NodeId(id))
            }
            "MethodSigUntyped" => {
                let name = args[0].as_text()?;
                let param_ids = args[1].as_node_list()?;
                let id = self.make_node("MethodSig", &name, CtxKind::Item, start, end);
                for (i, pid) in param_ids.iter().enumerate() {
                    self.add_child(id, i as i64, *pid);
                }
                Ok(PegValue::NodeId(id))
            }
            "MethodDef" => {
                // Old grammar: MethodDef(@Name <params> [<typeRef>] <body>) — arg count varies
                let name = args[0].as_text()?;
                let param_ids = args[1].as_node_list()?;
                let (output_arg, body_arg) = if args.len() == 4 {
                    (Some(&args[2]), &args[3])
                } else {
                    (None, &args[2])
                };
                let constructor = match body_arg {
                    PegValue::NodeId(bid) => {
                        if let Some(bn) = self.world.parse_nodes.iter().find(|n| n.id == *bid) {
                            if bn.constructor == "TailBlock" { "TailMethodDef" } else { "MethodDef" }
                        } else { "MethodDef" }
                    }
                    _ => "MethodDef",
                };
                let id = self.make_node(constructor, &name, CtxKind::Body, start, end);
                let mut ord = 0i64;
                for pid in &param_ids { self.add_child(id, ord, *pid); ord += 1; }
                if let Some(out) = output_arg {
                    let ret_text = self.resolve_type_text(out);
                    let ret_id = self.make_leaf("ReturnType", &ret_text, start, end);
                    self.add_child(id, ord, ret_id); ord += 1;
                }
                let body_id = self.resolve_child_id(body_arg);
                self.add_child(id, ord, body_id);
                Ok(PegValue::NodeId(id))
            }
            "MethodDefTyped" => {
                let name = args[0].as_text()?;
                let param_ids = args[1].as_node_list()?;
                let ret_text = self.resolve_type_text(&args[2]);
                let body_id = self.resolve_child_id(&args[3]);
                let constructor = if let Some(bn) = self.world.parse_nodes.iter().find(|n| n.id == body_id) {
                    if bn.constructor == "TailBlock" { "TailMethodDef" } else { "MethodDef" }
                } else { "MethodDef" };
                let id = self.make_node(constructor, &name, CtxKind::Body, start, end);
                let mut ord = 0i64;
                for pid in &param_ids { self.add_child(id, ord, *pid); ord += 1; }
                let ret_id = self.make_leaf("ReturnType", &ret_text, start, end);
                self.add_child(id, ord, ret_id); ord += 1;
                self.add_child(id, ord, body_id);
                Ok(PegValue::NodeId(id))
            }
            "MethodDefUntyped" => {
                let name = args[0].as_text()?;
                let param_ids = args[1].as_node_list()?;
                let body_id = self.resolve_child_id(&args[2]);
                let constructor = if let Some(bn) = self.world.parse_nodes.iter().find(|n| n.id == body_id) {
                    if bn.constructor == "TailBlock" { "TailMethodDef" } else { "MethodDef" }
                } else { "MethodDef" };
                let id = self.make_node(constructor, &name, CtxKind::Body, start, end);
                let mut ord = 0i64;
                for pid in &param_ids { self.add_child(id, ord, *pid); ord += 1; }
                self.add_child(id, ord, body_id);
                Ok(PegValue::NodeId(id))
            }
            "AssociatedType" => {
                let name = args[0].as_text()?;
                let type_text = self.resolve_type_text(&args[1]);
                let id = self.make_node("AssociatedType", &name, CtxKind::Item, start, end);
                let tid = self.make_leaf("TypeRef", &type_text, start, end);
                self.add_child(id, 0, tid);
                Ok(PegValue::NodeId(id))
            }

            // ── Body constructors ───────────────────────────────
            // Block, TailBlock, MatchBody, CommitArm, BacktrackArm, DestructureArm → commit_node

            // ── Pattern constructors ────────────────────────────
            // Wildcard, BoolTrue, BoolFalse → handled by commit_node
            // VariantPat, InstanceBind → commit_node
            "DataCarrying" => {
                let name = args[0].as_text()?;
                let inner_id = self.resolve_child_id(&args[1]);
                let inner_text = self.world.parse_nodes.iter().find(|n| n.id == inner_id).map(|n| n.text.clone()).unwrap_or_default();
                let id = self.make_node("DataCarrying", &format!("{}({})", name, inner_text), CtxKind::Pattern, start, end);
                self.add_child(id, 0, inner_id);
                Ok(PegValue::NodeId(id))
            }
            // LiteralPat → commit_node (text resolved by resolve_constructor)
            "OrPattern" => {
                let pat_ids = args[0].as_node_list()?;
                if pat_ids.len() == 1 {
                    // Single pattern — passthrough
                    return Ok(PegValue::NodeId(pat_ids[0]));
                }
                // Check for destructure: last element is InstanceBind
                let last_node = pat_ids.last()
                    .and_then(|id| self.world.parse_nodes.iter().find(|n| n.id == *id));
                if let Some(ln) = last_node {
                    if ln.constructor == "InstanceBind" {
                        // Destructure pattern
                        let texts: Vec<String> = pat_ids.iter()
                            .filter_map(|id| self.world.parse_nodes.iter().find(|n| n.id == *id))
                            .map(|n| n.text.clone())
                            .collect();
                        let id = self.make_node("OrPattern", &texts.join("|"), CtxKind::Pattern, start, end);
                        for (i, pid) in pat_ids.iter().enumerate() {
                            self.add_child(id, i as i64, *pid);
                        }
                        return Ok(PegValue::NodeId(id));
                    }
                }
                let texts: Vec<String> = pat_ids.iter()
                    .filter_map(|id| self.world.parse_nodes.iter().find(|n| n.id == *id))
                    .map(|n| n.text.clone())
                    .collect();
                let id = self.make_node("OrPattern", &texts.join("|"), CtxKind::Pattern, start, end);
                for (i, pid) in pat_ids.iter().enumerate() {
                    self.add_child(id, i as i64, *pid);
                }
                Ok(PegValue::NodeId(id))
            }
            "Elem" => Ok(PegValue::Text(args[0].as_text()?)),
            "DestructBind" => Ok(PegValue::Text(format!("@{}", args[0].as_text()?))),
            "DestructWildcard" => Ok(PegValue::Text("_".to_string())),
            "Rest" => {
                let name = args[0].as_text()?;
                let id = self.make_leaf("Rest", &name, start, end);
                Ok(PegValue::NodeId(id))
            }

            // ── Match expression ────────────────────────────────
            "MatchExpr" => {
                let target_id = self.resolve_child_id(&args[0]);
                let arm_ids = args[1].as_node_list()?;
                let id = self.make_node("Match", "", CtxKind::Expr, start, end);
                self.add_child(id, 0, target_id);
                for (i, aid) in arm_ids.iter().enumerate() {
                    self.add_child(id, (i + 1) as i64, *aid);
                }
                Ok(PegValue::NodeId(id))
            }

            // ── Expression constructors ─────────────────────────
            // ExprStub → handled by commit_node (as "ExprStub")
            // ConstRef → commit_node
            // Return, Yield → commit_node
            // InstanceRef, InlineEval, Group, StdOut, BareName, QualifiedVariant,
            // BareTrue, BareFalse, FnCall → commit_node
            "TypePath" => {
                // Name/Variant → Access(BareName(Name), Variant)
                let type_name = args[0].as_text()?;
                let variant = args[1].as_text()?;
                let base_id = self.make_leaf("BareName", &type_name, start, end);
                let id = self.make_leaf("Access", &variant, start, end);
                self.add_child(id, 0, base_id);
                Ok(PegValue::NodeId(id))
            }
            // ErrorProp, Literal, StructConstruct → commit_node
            "StructField" => {
                let name = args[0].as_text()?;
                let val_id = self.resolve_child_id(&args[1]);
                let id = self.make_leaf("StructField", &name, start, end);
                self.add_child(id, 0, val_id);
                Ok(PegValue::NodeId(id))
            }

            // ── Statement constructors ──────────────────────────
            "ExprStmt" => Ok(args[0].clone()),
            "MutableNew" => {
                let name = args[0].as_text()?;
                let type_text = self.resolve_type_text(&args[1]);
                let init_ids = args[2].as_node_list()?;
                let text = format!("{}:{}", name, type_text);
                let id = self.make_leaf("MutableNew", &text, start, end);
                for (i, aid) in init_ids.iter().enumerate() {
                    self.add_child(id, i as i64, *aid);
                }
                Ok(PegValue::NodeId(id))
            }
            "SubTypeNew" => {
                let name = args[0].as_text()?;
                let type_text = self.resolve_type_text(&args[1]);
                let init_ids = args[2].as_node_list()?;
                let text = format!("{}:{}", name, type_text);
                let id = self.make_leaf("SubTypeNew", &text, start, end);
                for (i, aid) in init_ids.iter().enumerate() {
                    self.add_child(id, i as i64, *aid);
                }
                Ok(PegValue::NodeId(id))
            }
            "SameTypeNew" => {
                let name = args[0].as_text()?;
                let init_ids = args[1].as_node_list()?;
                let id = self.make_leaf("SameTypeNew", &name, start, end);
                for (i, aid) in init_ids.iter().enumerate() {
                    self.add_child(id, i as i64, *aid);
                }
                Ok(PegValue::NodeId(id))
            }
            "DeferredNew" => {
                let name = args[0].as_text()?;
                let init_ids = args[1].as_node_list()?;
                let id = self.make_leaf("DeferredNew", &name, start, end);
                for (i, aid) in init_ids.iter().enumerate() {
                    self.add_child(id, i as i64, *aid);
                }
                Ok(PegValue::NodeId(id))
            }
            "SubTypeDecl" => {
                let name = args[0].as_text()?;
                let type_text = self.resolve_type_text(&args[1]);
                let text = format!("{}:{}", name, type_text);
                Ok(PegValue::NodeId(self.make_leaf("SubTypeDecl", &text, start, end)))
            }
            "MutableSet" => {
                let name = args[0].as_text()?;
                let val_id = self.resolve_child_id(&args[1]);
                let id = self.make_leaf("MutableSet", &name, start, end);
                self.add_child(id, 0, val_id);
                Ok(PegValue::NodeId(id))
            }
            "Set" | "Extend" => Ok(args[0].clone()),
            "Chain" => {
                // name.rest → Access(rest, name)
                let name = args[0].as_text()?;
                let rest_id = self.resolve_child_id(&args[1]);
                let id = self.make_leaf("Access", &name, start, end);
                self.add_child(id, 0, rest_id);
                Ok(PegValue::NodeId(id))
            }
            "MethodCall" => {
                let name = args[0].as_text()?;
                let call_arg_ids = if args.len() > 1 { args[1].as_node_list().unwrap_or_default() } else { vec![] };
                let base_id = self.make_leaf("BareName", "_chain", start, end);
                let id = self.make_leaf("MethodCall", &name, start, end);
                self.add_child(id, 0, base_id);
                for (i, aid) in call_arg_ids.iter().enumerate() {
                    self.add_child(id, (i + 1) as i64, *aid);
                }
                Ok(PegValue::NodeId(id))
            }

            // ── Default: commit_node handles all other constructors generically ──
            _ => self.commit_node(name, args, start, end),
        }
    }

    /// Generic node creation — replaces 80+ specific constructor arms.
    /// Creates a ParseNode, adds children, updates semantic relations.
    fn commit_node(
        &mut self,
        constructor: &str,
        args: &[PegValue],
        start: i64,
        end: i64,
    ) -> Result<PegValue, String> {
        // 1. Resolve text and possibly remap constructor name
        let (actual_constructor, text) = self.resolve_constructor(constructor, args)?;

        let constructor = &actual_constructor;

        // 2. Create ParseNode
        let ctx = self.infer_context(constructor);
        let id = self.make_node(constructor, &text, ctx, start, end);

        // 3. Add children in forward order (flatten NodeLists)
        let mut ordinal = 0i64;
        for arg in args {
            match arg {
                PegValue::NodeId(cid) => {
                    self.add_child(id, ordinal, *cid);
                    ordinal += 1;
                }
                PegValue::NodeList(cids) => {
                    for cid in cids {
                        self.add_child(id, ordinal, *cid);
                        ordinal += 1;
                    }
                }
                _ => {} // Text/Int/Float are captured in the node's text field
            }
        }

        // 4. Update semantic relations
        self.update_relations(constructor, id, &text);

        Ok(PegValue::NodeId(id))
    }

    /// Resolve constructor name and text from grammar constructor + args.
    /// Handles name remapping (Borrow→BorrowParam) and text transformation (@prefix, quotes).
    fn resolve_constructor(&self, constructor: &str, args: &[PegValue]) -> Result<(String, String), String> {
        let first_text = args.iter()
            .find_map(|a| match a {
                PegValue::Text(s) => Some(s.clone()),
                PegValue::Int(n) => Some(n.to_string()),
                PegValue::Float(f) => Some(format_float(*f)),
                _ => None,
            })
            .unwrap_or_default();

        match constructor {
            // Name remapping: grammar name → ParseNode constructor name
            "Borrow" => Ok(("BorrowParam".into(), first_text)),
            "MutBorrow" => Ok(("MutBorrowParam".into(), first_text)),
            "Owned" => Ok(("OwnedParam".into(), first_text)),
            "BareTrue" => Ok(("BareName".into(), "True".into())),
            "BareFalse" => Ok(("BareName".into(), "False".into())),

            // Text transformation
            "InstanceBind" => Ok(("InstanceBind".into(), format!("@{}", first_text))),
            "SelfType" => Ok(("SelfType".into(), "Self".into())),

            // Defaults with specific text
            "Wildcard" => Ok(("Wildcard".into(), "_".into())),
            "BoolTrue" => Ok(("BoolTrue".into(), "True".into())),
            "BoolFalse" => Ok(("BoolFalse".into(), "False".into())),
            "Stub" | "ExprStub" => Ok(("Stub".into(), String::new())),

            // Literal: dispatch on arg type
            "Literal" => {
                if let Some(arg) = args.first() {
                    match arg {
                        PegValue::Int(n) => Ok(("IntLit".into(), n.to_string())),
                        PegValue::Float(f) => Ok(("FloatLit".into(), format_float(*f))),
                        PegValue::Text(s) => Ok(("StringLit".into(), s.clone())),
                        _ => Ok(("StringLit".into(), first_text)),
                    }
                } else {
                    Ok(("StringLit".into(), String::new()))
                }
            }
            "LiteralPat" => {
                if let Some(arg) = args.first() {
                    match arg {
                        PegValue::Int(n) => Ok(("LiteralPat".into(), n.to_string())),
                        PegValue::Text(s) => Ok(("LiteralPat".into(), format!("\"{}\"", s))),
                        _ => Ok(("LiteralPat".into(), first_text)),
                    }
                } else {
                    Ok(("LiteralPat".into(), String::new()))
                }
            }

            // BorrowedType: prepend &
            "BorrowedType" => Ok(("BorrowedType".into(), format!("&{}", first_text))),

            // ParameterizedType: Base{Param}
            "ParameterizedType" => {
                let base = args.get(0).map(|a| peg_value_text(a)).unwrap_or_default();
                let param = args.get(1).map(|a| peg_value_text(a)).unwrap_or_default();
                Ok(("ParameterizedType".into(), format!("{}({})", base, param)))
            }

            // FnCall: Type/Method → combine into path
            "FnCall" => {
                let type_name = args.get(0).map(|a| peg_value_text(a)).unwrap_or_default();
                let method = args.get(1).map(|a| peg_value_text(a)).unwrap_or_default();
                Ok(("FnCall".into(), format!("{}/{}", type_name, method)))
            }

            // QualifiedVariant: look up domain and qualify
            "QualifiedVariant" => {
                let variant_name = first_text.clone();
                let qualified = self.world.variant_ofs.iter()
                    .find(|v| v.variant_name == variant_name)
                    .map(|v| format!("{}::{}", v.type_name, variant_name))
                    .unwrap_or(variant_name);
                Ok(("BareName".into(), qualified))
            }

            // Match arms: text is the pattern text from first arg
            "CommitArm" | "BacktrackArm" | "DestructureArm" => {
                let pat_text = args.first().map(|a| self.resolve_pattern_text(a)).unwrap_or_default();
                Ok((constructor.into(), pat_text))
            }

            // Main always has text "Main"
            "Main" => Ok(("Main".into(), "Main".into())),

            // Default: use constructor name as-is, first text arg as text
            _ => Ok((constructor.into(), first_text)),
        }
    }

    /// Infer CtxKind from constructor name.
    fn infer_context(&self, constructor: &str) -> CtxKind {
        match constructor {
            "Domain" | "Struct" | "TraitDecl" | "TraitImpl" | "TypeImpl" |
            "Const" | "Main" | "TypeAlias" | "GrammarRule" | "RuleArm" |
            "MethodSig" | "MethodDef" | "TailMethodDef" | "AssociatedType" |
            "UnitVariant" | "ParenVariant" | "StructVariant" |
            "Field" | "BorrowedField" | "ForeignBlock" | "ForeignFunc" => CtxKind::Item,

            "Block" | "TailBlock" | "MatchBody" | "Stub" |
            "CommitArm" | "BacktrackArm" | "DestructureArm" => CtxKind::Body,

            "BorrowSelf" | "MutBorrowSelf" | "OwnedSelf" |
            "OwnedParam" | "NamedParam" | "BorrowParam" | "MutBorrowParam" => CtxKind::Param,

            "SelfType" | "NamedType" | "BorrowedType" | "ParameterizedType" |
            "BoundType" | "CompoundBound" | "SingleBound" => CtxKind::TypeRef,

            "Wildcard" | "BoolTrue" | "BoolFalse" | "VariantPat" |
            "InstanceBind" | "DataCarrying" | "LiteralPat" | "OrPattern" => CtxKind::Pattern,

            "ModuleHeader" | "ExportName" | "NamedImport" | "WildcardImport" => CtxKind::Module,

            _ => CtxKind::Expr,
        }
    }

    /// Update Type/Variant/Field/FfiEntry relations from committed nodes.
    fn update_relations(&mut self, constructor: &str, node_id: i64, text: &str) {
        match constructor {
            "Domain" => {
                self.world.types.push(aski_core::Type {
                    id: node_id, name: text.to_string(),
                    form: aski_core::TypeForm::Domain, parent: 0,
                });
                // Variants from children
                let children: Vec<_> = self.world.parse_children.iter()
                    .filter(|c| c.parent_id == node_id)
                    .map(|c| (c.ordinal, c.child_id))
                    .collect();
                for (ord, cid) in children {
                    if let Some(cn) = self.world.parse_nodes.iter().find(|n| n.id == cid) {
                        self.world.variants.push(aski_core::Variant {
                            type_id: node_id, ordinal: ord,
                            name: cn.text.clone(), contains_type: String::new(),
                        });
                    }
                }
            }
            "Struct" => {
                self.world.types.push(aski_core::Type {
                    id: node_id, name: text.to_string(),
                    form: aski_core::TypeForm::Struct, parent: 0,
                });
                // Fields from children
                let children: Vec<_> = self.world.parse_children.iter()
                    .filter(|c| c.parent_id == node_id)
                    .map(|c| (c.ordinal, c.child_id))
                    .collect();
                for (ord, cid) in children {
                    if let Some(cn) = self.world.parse_nodes.iter().find(|n| n.id == cid) {
                        // Field type from the Field node's first child
                        let field_type = self.world.parse_children.iter()
                            .find(|c| c.parent_id == cid && c.ordinal == 0)
                            .and_then(|c| self.world.parse_nodes.iter().find(|n| n.id == c.child_id))
                            .map(|n| n.text.clone())
                            .unwrap_or_default();
                        self.world.fields.push(aski_core::Field {
                            type_id: node_id, ordinal: ord,
                            name: cn.text.clone(), field_type,
                        });
                    }
                }
            }
            "ForeignBlock" => {
                let children: Vec<_> = self.world.parse_children.iter()
                    .filter(|c| c.parent_id == node_id)
                    .map(|c| c.child_id)
                    .collect();
                for cid in children {
                    if let Some(cn) = self.world.parse_nodes.iter().find(|n| n.id == cid) {
                        if cn.constructor == "ForeignFunc" {
                            // Extract return type and extern name from ForeignFunc children
                            let fc: Vec<_> = self.world.parse_children.iter()
                                .filter(|c| c.parent_id == cid)
                                .map(|c| c.child_id)
                                .collect();
                            let ret_type = fc.first()
                                .and_then(|id| self.world.parse_nodes.iter().find(|n| n.id == *id))
                                .map(|n| n.text.clone())
                                .unwrap_or_default();
                            let ext_name = fc.get(1)
                                .and_then(|id| self.world.parse_nodes.iter().find(|n| n.id == *id))
                                .map(|n| n.text.clone())
                                .unwrap_or(cn.text.clone());
                            self.world.ffi_entries.push(aski_core::FfiEntry {
                                library: text.to_string(),
                                aski_name: cn.text.clone(),
                                rust_name: ext_name,
                                span: aski_core::RustSpan::MethodCall,
                                return_type: ret_type,
                            });
                        }
                    }
                }
            }
            _ => {}
        }
    }

    // ── Helper methods ──────────────────────────────────────

    fn make_node(&mut self, constructor: &str, text: &str, ctx: CtxKind, start: i64, end: i64) -> i64 {
        let id = self.ids.next();
        self.ctx_stack.push(ctx.clone());
        self.world.parse_nodes.push(ParseNode {
            id,
            constructor: constructor.to_string(),
            ctx,
            parent_id: 0,
            status: ParseStatus::Committed,
            text: text.to_string(),
            token_start: start,
            token_end: end,
        });
        id
    }



    fn make_leaf(&mut self, constructor: &str, text: &str, start: i64, end: i64) -> i64 {
        self.make_node(constructor, text, CtxKind::Expr, start, end)
    }

    /// Save world state for backtracking.
    fn save_state(&self) -> (usize, usize, usize, usize, usize, usize, usize, usize, usize, i64) {
        (
            self.world.parse_nodes.len(), self.world.parse_children.len(),
            self.world.types.len(), self.world.variants.len(),
            self.world.fields.len(), self.world.ffi_entries.len(),
            self.world.rules.len(), self.world.arms.len(),
            self.world.pat_elems.len(), self.ids.next,
        )
    }

    /// Restore world state from a save point.
    fn restore_state(&mut self, save: &(usize, usize, usize, usize, usize, usize, usize, usize, usize, i64)) {
        self.world.parse_nodes.truncate(save.0);
        self.world.parse_children.truncate(save.1);
        self.world.types.truncate(save.2);
        self.world.variants.truncate(save.3);
        self.world.fields.truncate(save.4);
        self.world.ffi_entries.truncate(save.5);
        self.world.rules.truncate(save.6);
        self.world.arms.truncate(save.7);
        self.world.pat_elems.truncate(save.8);
        self.ids.next = save.9;
    }

    fn next_child_ordinal(&self, parent_id: i64) -> i64 {
        self.world.parse_children.iter()
            .filter(|c| c.parent_id == parent_id)
            .count() as i64
    }

    /// Add a child to a parent node — creates ParseChild entry AND sets the child's parent_id.
    fn add_child(&mut self, parent_id: i64, ordinal: i64, child_id: i64) {
        self.world.parse_children.push(ParseChild { parent_id, ordinal, child_id });
        if let Some(node) = self.world.parse_nodes.iter_mut().find(|n| n.id == child_id) {
            node.parent_id = parent_id;
        }
    }

    /// Resolve a PegValue to its type text representation.
    fn resolve_type_text(&self, val: &PegValue) -> String {
        match val {
            PegValue::Text(s) => s.clone(),
            PegValue::NodeId(id) => {
                self.world.parse_nodes.iter()
                    .find(|n| n.id == *id)
                    .map(|n| n.text.clone())
                    .unwrap_or_default()
            }
            _ => String::new(),
        }
    }

    /// Resolve a PegValue to a child_id for ParseChild insertion.
    fn resolve_child_id(&self, val: &PegValue) -> i64 {
        match val {
            PegValue::NodeId(id) => *id,
            _ => 0, // shouldn't happen in well-formed grammar
        }
    }

    /// Resolve a PegValue (pattern) to its text representation.
    fn resolve_pattern_text(&self, val: &PegValue) -> String {
        match val {
            PegValue::Text(s) => s.clone(),
            PegValue::NodeId(id) => {
                self.world.parse_nodes.iter()
                    .find(|n| n.id == *id)
                    .map(|n| n.text.clone())
                    .unwrap_or_default()
            }
            PegValue::NodeList(ids) => {
                ids.iter()
                    .filter_map(|id| self.world.parse_nodes.iter().find(|n| n.id == *id))
                    .map(|n| n.text.clone())
                    .collect::<Vec<_>>()
                    .join("|")
            }
            _ => String::new(),
        }
    }
}

// ── Helper functions ─────────────────────────────────────────

/// Public access to skip_newlines for header-only parsing.
pub fn skip_newlines_pub(tokens: &[SpannedToken], pos: usize) -> usize {
    skip_newlines(tokens, pos)
}

fn skip_newlines(tokens: &[SpannedToken], mut pos: usize) -> usize {
    while pos < tokens.len() && matches!(tokens[pos].token, Token::Newline | Token::Comment) {
        pos += 1;
    }
    pos
}

fn token_variant_name(token: &Token) -> &'static str {
    match token {
        Token::Plus => "Plus",
        Token::Minus => "Minus",
        Token::Star => "Star",
        Token::Slash => "Slash",
        Token::Percent => "Percent",
        Token::DoubleEquals => "DoubleEquals",
        Token::NotEqual => "NotEqual",
        Token::LessThan => "LessThan",
        Token::GreaterThan => "GreaterThan",
        Token::LessThanOrEqual => "LessThanOrEqual",
        Token::GreaterThanOrEqual => "GreaterThanOrEqual",
        Token::LogicalAnd => "LogicalAnd",
        Token::LogicalOr => "LogicalOr",
        Token::LParen => "LParen",
        Token::RParen => "RParen",
        Token::LBracket => "LBracket",
        Token::RBracket => "RBracket",
        Token::LBrace => "LBrace",
        Token::RBrace => "RBrace",
        Token::Dot => "Dot",
        Token::At => "At",
        Token::Dollar => "Dollar",
        Token::Caret => "Caret",
        Token::Ampersand => "Ampersand",
        Token::Tilde => "Tilde",
        Token::Question => "Question",
        Token::Bang => "Bang",
        Token::Hash => "Hash",
        Token::Pipe => "Pipe",
        Token::Tick => "Tick",
        Token::Colon => "Colon",
        Token::Comma => "Comma",
        Token::Underscore => "Underscore",
        Token::CompositionOpen => "CompositionOpen",
        Token::CompositionClose => "CompositionClose",
        Token::TraitBoundOpen => "TraitBoundOpen",
        Token::TraitBoundClose => "TraitBoundClose",
        Token::IterOpen => "IterOpen",
        Token::IterClose => "IterClose",
        Token::RangeInclusive => "RangeInclusive",
        Token::RangeExclusive => "RangeExclusive",
        Token::Stub => "Stub",
        Token::Newline => "Newline",
        Token::Equals => "Equals",
        _ => "Unknown",
    }
}

fn peg_value_text(val: &PegValue) -> String {
    match val {
        PegValue::Text(s) => s.clone(),
        PegValue::Int(n) => n.to_string(),
        PegValue::Float(f) => format_float(*f),
        PegValue::NodeId(id) => format!("node:{}", id),
        PegValue::NodeList(ids) => format!("nodes:{}", ids.len()),
        PegValue::TaggedList(parts) => parts.iter().map(peg_value_text).collect::<Vec<_>>().join(","),
        PegValue::None => String::new(),
    }
}

fn format_float(v: f64) -> String {
    let s = format!("{v}");
    if s.contains('.') { s } else { format!("{s}.0") }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grammar::bootstrap;
    use crate::grammar::config;

    fn make_test_parser() -> GrammarParser {
        let grammar_dir = config::find_grammar_dir().expect("grammar dir");
        let config = config::GrammarConfig::load_from_dir(&grammar_dir)
            .unwrap_or_else(|_| config::GrammarConfig::bootstrap());
        let rules = bootstrap::load_rules(&grammar_dir).unwrap_or_default();
        GrammarParser::new(rules, config)
    }

    #[test]
    fn grammar_parse_domain() {
        let mut parser = make_test_parser();
        let world = parser.parse_to_world("Element (Fire Earth Air Water)").unwrap();
        let root_children = aski_core::query_parse_children(&world, 1);
        let domain = root_children.iter().find(|n| n.constructor == "Domain").expect("should have Domain");
        assert_eq!(domain.text, "Element");
        let variants = aski_core::query_parse_children(&world, domain.id);
        assert_eq!(variants.len(), 4);
        assert_eq!(variants[0].text, "Fire");
        assert_eq!(variants[3].text, "Water");
    }

    #[test]
    fn grammar_parse_struct() {
        let mut parser = make_test_parser();
        let world = parser.parse_to_world("Point { Horizontal F64 Vertical F64 }").unwrap();
        let root_children = aski_core::query_parse_children(&world, 1);
        let s = root_children.iter().find(|n| n.constructor == "Struct").expect("should have Struct");
        assert_eq!(s.text, "Point");
        let fields = aski_core::query_parse_children(&world, s.id);
        assert_eq!(fields.len(), 2);
        assert_eq!(fields[0].text, "Horizontal");
    }

    #[test]
    fn context_kind_on_nodes() {
        let mut parser = make_test_parser();
        let world = parser.parse_to_world("compute [Addition [ add(:@Self) U32 [ ^(@Self.Left + @Self.Right) ] ]]").unwrap();
        let root_children = aski_core::query_parse_children(&world, 1);
        let ti = root_children.iter().find(|n| n.constructor == "TraitImpl").unwrap();
        assert_eq!(ti.ctx, CtxKind::Item);
        let type_impls = aski_core::query_parse_children(&world, ti.id);
        let type_impl = &type_impls[0];
        assert_eq!(type_impl.constructor, "TypeImpl");
        let methods = aski_core::query_parse_children(&world, type_impl.id);
        let method = &methods[0];
        assert_eq!(method.constructor, "MethodDef");
        assert_eq!(method.ctx, CtxKind::Body);
        // Ancestor context query
        assert!(aski_core::query_in_context(&world, method.id, CtxKind::Item));
    }

    #[test]
    fn grammar_parse_const() {
        let mut parser = make_test_parser();
        let world = parser.parse_to_world("!Pi F64 {3.14159265358979}").unwrap();
        let root_children = aski_core::query_parse_children(&world, 1);
        let c = root_children.iter().find(|n| n.constructor == "Const").expect("should have Const");
        assert_eq!(c.text, "Pi");
    }

    #[test]
    fn grammar_parse_main() {
        let mut parser = make_test_parser();
        let world = parser.parse_to_world("Main [ StdOut \"hello\" ]").unwrap();
        let root_children = aski_core::query_parse_children(&world, 1);
        let main = root_children.iter().find(|n| n.constructor == "Main").expect("should have Main");
        let body = aski_core::query_parse_children(&world, main.id);
        assert!(!body.is_empty());
    }

    #[test]
    fn grammar_parse_trait_decl() {
        let mut parser = make_test_parser();
        let world = parser.parse_to_world("classify ([element(:@Self) Element])").unwrap();
        let root_children = aski_core::query_parse_children(&world, 1);
        let t = root_children.iter().find(|n| n.constructor == "TraitDecl").expect("should have TraitDecl");
        assert_eq!(t.text, "classify");
    }

    #[test]
    fn grammar_parse_trait_impl() {
        let mut parser = make_test_parser();
        let world = parser.parse_to_world(
            "compute [Addition [ add(:@Self) U32 [ ^(@Self.Left + @Self.Right) ] ]]"
        ).unwrap();
        let root_children = aski_core::query_parse_children(&world, 1);
        let ti = root_children.iter().find(|n| n.constructor == "TraitImpl").expect("should have TraitImpl");
        assert_eq!(ti.text, "compute");
    }

    #[test]
    fn grammar_parse_type_alias() {
        let mut parser = make_test_parser();
        let world = parser.parse_to_world("SignList Vec{Sign}").unwrap();
        let root_children = aski_core::query_parse_children(&world, 1);
        let ta = root_children.iter().find(|n| n.constructor == "TypeAlias").expect("should have TypeAlias");
        assert_eq!(ta.text, "SignList");
    }

    #[test]
    fn grammar_parse_multiple_items() {
        let mut parser = make_test_parser();
        let src = r#"
Element (Fire Earth Air Water)
Point { Horizontal F64 Vertical F64 }
!Pi F64 {3.14}
"#;
        let world = parser.parse_to_world(src).unwrap();
        let root_children = aski_core::query_parse_children(&world, 1);
        assert_eq!(root_children.len(), 3);
        assert_eq!(root_children[0].constructor, "Domain");
        assert_eq!(root_children[1].constructor, "Struct");
        assert_eq!(root_children[2].constructor, "Const");
    }

    #[test]
    fn grammar_parse_simple_aski_file() {
        let mut parser = make_test_parser();
        let source = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/simple.aski")
        ).unwrap();
        let world = parser.parse_to_world(&source).unwrap();
        let root_children = aski_core::query_parse_children(&world, 1);
        assert!(root_children.len() >= 3);
    }

    #[test]
    fn grammar_parse_module_header() {
        let mut parser = make_test_parser();
        let src = "(Chart Sign Planet)\n[Ephemeris(julianDay longitude)]\nSign (Aries Taurus)";
        let world = parser.parse_to_world(src).unwrap();
        assert!(!world.modules.is_empty());
        assert_eq!(world.modules[0].name, "Chart");
        assert!(world.exports.iter().any(|e| e.export_name == "Sign"));
        assert!(world.exports.iter().any(|e| e.export_name == "Planet"));
    }

    #[test]
    fn grammar_parse_bootstrap_tokens_aski() {
        let mut parser = make_test_parser();
        let source = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("bootstrap/tokens.aski")
        ).unwrap();
        let world = parser.parse_to_world(&source).unwrap();
        assert!(!world.modules.is_empty(), "should have module header");
        let root_children = aski_core::query_parse_children(&world, 1);
        assert!(root_children.len() > 3, "should have struct + trait decls + impls");
    }

    #[test]
    fn grammar_type_path_variant() {
        let mut parser = make_test_parser();
        let world = parser.parse_to_world("Main [ ^Sign/Aries ]").unwrap();
        let root_children = aski_core::query_parse_children(&world, 1);
        let main = root_children.iter().find(|n| n.constructor == "Main").expect("Main");
        let body = aski_core::query_parse_children(&world, main.id);
        assert!(!body.is_empty());
    }

    #[test]
    fn grammar_guard_isvariant() {
        let mut parser = make_test_parser();

        // Check that the guard rule was loaded from grammar/test_guard.aski
        assert!(parser.rules.contains_key("testGuardedAtom"),
            "testGuardedAtom rule should be loaded from grammar/test_guard.aski");

        // Check the first arm has a guard
        let rule = &parser.rules["testGuardedAtom"];
        assert!(rule.arms[0].guard.is_some(),
            "first arm should have a guard");
        let guard_cond = &rule.arms[0].guard.as_ref().unwrap().condition;
        assert!(guard_cond.contains("IsVariant"),
            "guard should contain IsVariant, got: {}", guard_cond);

        // Populate World with a domain so IsVariant works
        // (normally done by parsing "Element (Fire Water)" but we do it directly)
        aski_core::run_rules(&mut parser.world);  // derive VariantOf from types+variants
        parser.world.types.push(aski_core::Type {
            id: 99, name: "Element".into(),
            form: aski_core::TypeForm::Domain, parent: 0,
        });
        parser.world.variants.push(aski_core::Variant {
            type_id: 99, ordinal: 0, name: "Fire".into(), contains_type: String::new(),
        });
        parser.world.variants.push(aski_core::Variant {
            type_id: 99, ordinal: 1, name: "Water".into(), contains_type: String::new(),
        });
        aski_core::run_rules(&mut parser.world);  // re-derive with new data

        // Test: "Fire" is a variant → guard passes → QualifiedVariant
        let tokens = crate::lexer::lex("Fire").unwrap();
        let result = parser.try_rule("testGuardedAtom", &tokens, 0);
        assert!(result.is_ok(), "should parse Fire: {:?}", result);
        let (val, _) = result.unwrap();
        if let PegValue::NodeId(id) = val {
            let node = parser.world.parse_nodes.iter().find(|n| n.id == id).unwrap();
            assert_eq!(node.constructor, "BareName",
                "Fire qualified variant should produce BareName node, got {}", node.constructor);
            assert!(node.text.contains("::"),
                "qualified variant text should contain '::', got {}", node.text);
        } else {
            panic!("expected NodeId, got {:?}", val);
        }

        // Test: "Foo" is NOT a variant → guard fails → BareName
        let tokens2 = crate::lexer::lex("Foo").unwrap();
        let result2 = parser.try_rule("testGuardedAtom", &tokens2, 0);
        assert!(result2.is_ok(), "should parse Foo: {:?}", result2);
        let (val2, _) = result2.unwrap();
        if let PegValue::NodeId(id) = val2 {
            let node = parser.world.parse_nodes.iter().find(|n| n.id == id).unwrap();
            assert_eq!(node.constructor, "BareName",
                "Foo is not a variant, should be BareName, got {}", node.constructor);
        } else {
            panic!("expected NodeId, got {:?}", val2);
        }
    }
}
