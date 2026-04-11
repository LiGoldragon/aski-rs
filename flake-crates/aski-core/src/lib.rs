#![allow(non_snake_case)]
//! aski-core — Kernel schema shared between aski-rs and aski-cc.
//!
//! Everything here is generated from kernel.aski by askic.
//! The World struct, relation types, queries, and derivation rules
//! are all generated — no hand-written code.

// World, enums, structs, queries, derive() — all generated from kernel.aski
include!(concat!(env!("OUT_DIR"), "/kernel.rs"));

// ═══════════════════════════════════════════════════════════════
// ID Generator
// ═══════════════════════════════════════════════════════════════

pub struct IdGen {
    pub next: i64,
}

impl IdGen {
    pub fn new() -> Self {
        Self { next: 1 }
    }

    pub fn next(&mut self) -> i64 {
        let id = self.next;
        self.next += 1;
        id
    }
}

impl Default for IdGen {
    fn default() -> Self {
        Self::new()
    }
}

// ═══════════════════════════════════════════════════════════════
// World lifecycle
// ═══════════════════════════════════════════════════════════════

pub fn run_rules(world: &mut World) {
    world.derive();
}

// ═══════════════════════════════════════════════════════════════
// Parse tree queries
// ═══════════════════════════════════════════════════════════════

/// Children of a parse node, ordered by ordinal.
pub fn query_parse_children(world: &World, parent_id: i64) -> Vec<&ParseNode> {
    let mut child_ids: Vec<_> = world.parse_children.iter()
        .filter(|c| c.parent_id == parent_id)
        .collect();
    child_ids.sort_by_key(|c| c.ordinal);
    child_ids.iter()
        .filter_map(|c| world.parse_nodes.iter().find(|n| n.id == c.child_id))
        .collect()
}

/// Find a parse node by id.
pub fn find_parse_node(world: &World, id: i64) -> Option<&ParseNode> {
    world.parse_nodes.iter().find(|n| n.id == id)
}

/// Ancestor context chain (current → root).
pub fn query_ancestor_contexts(world: &World, node_id: i64) -> Vec<CtxKind> {
    let mut contexts = Vec::new();
    let mut current = node_id;
    loop {
        let node = world.parse_nodes.iter().find(|n| n.id == current);
        match node {
            Some(n) => {
                contexts.push(n.ctx.clone());
                if n.parent_id < 0 { break; }
                current = n.parent_id;
            }
            None => break,
        }
    }
    contexts
}

/// Find nearest ancestor with a specific context.
pub fn query_in_context(world: &World, node_id: i64, ctx: CtxKind) -> bool {
    let mut current = node_id;
    loop {
        let node = world.parse_nodes.iter().find(|n| n.id == current);
        match node {
            Some(n) => {
                if n.ctx == ctx { return true; }
                if n.parent_id < 0 { return false; }
                current = n.parent_id;
            }
            None => return false,
        }
    }
}

// ═══════════════════════════════════════════════════════════════
// Schema queries (Type/Variant/Field)
// ═══════════════════════════════════════════════════════════════

/// All types of a given form (Domain or Struct).
pub fn query_types_by_form(world: &World, form: TypeForm) -> Vec<&Type> {
    world.types.iter().filter(|t| t.form == form).collect()
}

/// Type by name.
pub fn query_type_by_name<'a>(world: &'a World, name: &str) -> Option<&'a Type> {
    world.types.iter().find(|t| t.name == name)
}

/// Domain variants ordered by ordinal.
pub fn query_domain_variants(world: &World, domain_name: &str) -> Vec<(i32, String, Option<String>)> {
    let type_id = world.types.iter()
        .find(|t| t.form == TypeForm::Domain && t.name == domain_name)
        .map(|t| t.id);
    let Some(tid) = type_id else { return Vec::new() };
    let mut variants: Vec<_> = world.variants.iter()
        .filter(|v| v.type_id == tid)
        .map(|v| (v.ordinal as i32, v.name.clone(),
                   if v.contains_type.is_empty() { None } else { Some(v.contains_type.clone()) }))
        .collect();
    variants.sort_by_key(|(ord, _, _)| *ord);
    variants
}

/// Struct fields ordered by ordinal.
pub fn query_struct_fields(world: &World, struct_name: &str) -> Vec<(i32, String, String)> {
    let type_id = world.types.iter()
        .find(|t| t.form == TypeForm::Struct && t.name == struct_name)
        .map(|t| t.id);
    let Some(tid) = type_id else { return Vec::new() };
    let mut fields: Vec<_> = world.fields.iter()
        .filter(|f| f.type_id == tid)
        .map(|f| (f.ordinal as i32, f.name.clone(), f.field_type.clone()))
        .collect();
    fields.sort_by_key(|(ord, _, _)| *ord);
    fields
}

/// Which domain owns this variant name?
pub fn query_variant_domain(world: &World, variant_name: &str) -> Option<(String, i64)> {
    world.variant_ofs.iter()
        .find(|v| v.variant_name == variant_name)
        .map(|v| (v.type_name.clone(), v.type_id))
}

/// Pattern utilities
#[derive(Debug, Clone)]
pub enum ParsedPattern {
    Variant(String),
    Wildcard,
    BoolLit(bool),
    DataCarrying(String, String),
}

pub fn parse_pattern_string(s: &str) -> ParsedPattern {
    match s {
        "_" => ParsedPattern::Wildcard,
        "True" | "true" => ParsedPattern::BoolLit(true),
        "False" | "false" => ParsedPattern::BoolLit(false),
        other => {
            if let Some(paren_pos) = other.find('(') {
                if other.ends_with(')') {
                    let name = other[..paren_pos].to_string();
                    let inner = other[paren_pos + 1..other.len() - 1].to_string();
                    return ParsedPattern::DataCarrying(name, inner);
                }
            }
            ParsedPattern::Variant(other.to_string())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_tree_children_and_context() {
        let mut world = World::default();
        world.parse_nodes.push(ParseNode {
            id: 1, constructor: "TraitImpl".into(), ctx: CtxKind::Item,
            parent_id: -1, status: ParseStatus::Committed,
            text: "myTrait".into(), token_start: 0, token_end: 10,
        });
        world.parse_nodes.push(ParseNode {
            id: 2, constructor: "MethodDef".into(), ctx: CtxKind::Body,
            parent_id: 1, status: ParseStatus::Committed,
            text: "doStuff".into(), token_start: 11, token_end: 20,
        });
        world.parse_nodes.push(ParseNode {
            id: 3, constructor: "BareName".into(), ctx: CtxKind::Expr,
            parent_id: 2, status: ParseStatus::Committed,
            text: "foo".into(), token_start: 21, token_end: 24,
        });
        world.parse_children.push(ParseChild { parent_id: 1, ordinal: 0, child_id: 2 });
        world.parse_children.push(ParseChild { parent_id: 2, ordinal: 0, child_id: 3 });

        let children = query_parse_children(&world, 1);
        assert_eq!(children.len(), 1);
        assert_eq!(children[0].constructor, "MethodDef");

        let contexts = query_ancestor_contexts(&world, 3);
        assert_eq!(contexts, vec![CtxKind::Expr, CtxKind::Body, CtxKind::Item]);

        assert!(query_in_context(&world, 3, CtxKind::Item));
        assert!(!query_in_context(&world, 3, CtxKind::Ffi));
    }

    #[test]
    fn domain_and_variant_queries() {
        let mut world = World::default();
        world.types.push(Type { id: 1, name: "Element".into(), form: TypeForm::Domain, parent: 0 });
        world.variants.push(Variant { type_id: 1, ordinal: 0, name: "Fire".into(), contains_type: String::new() });
        world.variants.push(Variant { type_id: 1, ordinal: 1, name: "Water".into(), contains_type: String::new() });
        run_rules(&mut world);

        let variants = query_domain_variants(&world, "Element");
        assert_eq!(variants.len(), 2);
        assert_eq!(variants[0].1, "Fire");

        let owner = query_variant_domain(&world, "Fire");
        assert!(owner.is_some());
        assert_eq!(owner.unwrap().0, "Element");
    }

    #[test]
    fn struct_field_queries() {
        let mut world = World::default();
        world.types.push(Type { id: 1, name: "Point".into(), form: TypeForm::Struct, parent: 0 });
        world.fields.push(Field { type_id: 1, ordinal: 1, name: "Y".into(), field_type: "F64".into() });
        world.fields.push(Field { type_id: 1, ordinal: 0, name: "X".into(), field_type: "F64".into() });
        let fields = query_struct_fields(&world, "Point");
        assert_eq!(fields[0].1, "X");
        assert_eq!(fields[1].1, "Y");
    }

    #[test]
    fn recursive_type_detection() {
        let mut world = World::default();
        world.types.push(Type { id: 1, name: "Tree".into(), form: TypeForm::Struct, parent: 0 });
        world.types.push(Type { id: 2, name: "Branch".into(), form: TypeForm::Struct, parent: 0 });
        world.fields.push(Field { type_id: 1, ordinal: 0, name: "children".into(), field_type: "Branch".into() });
        world.fields.push(Field { type_id: 2, ordinal: 0, name: "subtree".into(), field_type: "Tree".into() });
        run_rules(&mut world);
        assert!(world.recursive_types.iter().any(|r| r.parent_type == "Tree" && r.child_type == "Tree"));
        assert!(world.recursive_types.iter().any(|r| r.parent_type == "Branch" && r.child_type == "Branch"));
    }
}
