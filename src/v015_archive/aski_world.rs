//! AskiWorld — the parsing state machine.
//!
//! Loaded from synth dialects. Reads tokens. Creates parse nodes.
//! The dialect stack + name registries drive all parsing decisions.
//! Lowered to SemaWorld after parsing.

use std::collections::HashMap;
use crate::synth::types::Dialect;

/// A parse node in the AskiWorld tree.
#[derive(Debug, Clone)]
pub struct ParseNode {
    pub id: i64,
    pub constructor: String,
    pub key: String,
    pub dialect: String,
    pub parent_id: i64,
    pub token_start: i64,
    pub token_end: i64,
}

/// Parent → child relationship with ordering.
#[derive(Debug, Clone)]
pub struct ParseChild {
    pub parent_id: i64,
    pub ordinal: i64,
    pub child_id: i64,
}

/// What kind of type was registered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TypeForm { Domain, Struct, Alias }

/// A registered type name.
#[derive(Debug, Clone)]
pub struct TypeReg { pub name: String, pub form: TypeForm }

/// A registered variant name + its parent domain.
#[derive(Debug, Clone)]
pub struct VariantReg { pub name: String, pub parent: String }

/// Record of a parsed file — enough to recreate the source filesystem.
#[derive(Debug, Clone)]
pub struct FileRecord {
    pub path: String,
    pub module_name: String,
    pub exports: Vec<String>,
    pub imports: Vec<(String, Vec<String>)>,
    pub root_node_id: i64,
}

/// AskiWorld — everything the parser needs.
pub struct AskiWorld {
    // Dialect tables (loaded from .synth files)
    pub dialects: HashMap<String, Dialect>,
    pub dialect_stack: Vec<String>,

    // Parse tree
    pub parse_nodes: Vec<ParseNode>,
    pub parse_children: Vec<ParseChild>,
    pub next_id: i64,

    // Name registries (populated during parsing, queried by engine)
    pub known_types: Vec<TypeReg>,
    pub known_variants: Vec<VariantReg>,
    pub known_ffi: Vec<String>,
    pub known_methods: Vec<String>,
    pub known_traits: Vec<String>,

    // Module tracking (enough to recreate the source filesystem)
    pub current_file: String,
    pub module_name: String,
    pub exports: Vec<String>,
    pub imports: Vec<(String, Vec<String>)>, // (module_name, imported_names)
    pub files: Vec<FileRecord>, // all parsed files
}

/// Snapshot for backtracking (ordered choice + cardinality).
pub struct Snapshot {
    nodes: usize,
    children: usize,
    types: usize,
    variants: usize,
    ffi: usize,
    methods: usize,
    traits: usize,
    next_id: i64,
    dialect_depth: usize,
}

impl AskiWorld {
    pub fn new(dialects: HashMap<String, Dialect>) -> Self {
        let mut world = AskiWorld {
            dialects,
            dialect_stack: Vec::new(),
            parse_nodes: Vec::new(),
            parse_children: Vec::new(),
            next_id: 0,
            known_types: Vec::new(),
            known_variants: Vec::new(),
            known_ffi: Vec::new(),
            known_methods: Vec::new(),
            known_traits: Vec::new(),
            current_file: String::new(),
            module_name: String::new(),
            exports: Vec::new(),
            imports: Vec::new(),
            files: Vec::new(),
        };
        // Root node
        let root_id = world.alloc_id();
        world.parse_nodes.push(ParseNode {
            id: root_id,
            constructor: "Root".into(),
            key: String::new(),
            dialect: "aski".into(),
            parent_id: -1,
            token_start: 0,
            token_end: 0,
        });
        world
    }

    pub fn root_id(&self) -> i64 { 0 }

    pub fn alloc_id(&mut self) -> i64 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    // ── Dialect stack ──

    pub fn push_dialect(&mut self, name: &str) {
        self.dialect_stack.push(name.to_string());
    }

    pub fn pop_dialect(&mut self) {
        self.dialect_stack.pop();
    }

    pub fn current_dialect(&self) -> Option<&Dialect> {
        let name = self.dialect_stack.last()?;
        self.dialects.get(name)
    }

    pub fn current_dialect_name(&self) -> &str {
        self.dialect_stack.last().map(|s| s.as_str()).unwrap_or("aski")
    }

    // ── Snapshot / Restore ──

    pub fn snapshot(&self) -> Snapshot {
        Snapshot {
            nodes: self.parse_nodes.len(),
            children: self.parse_children.len(),
            types: self.known_types.len(),
            variants: self.known_variants.len(),
            ffi: self.known_ffi.len(),
            methods: self.known_methods.len(),
            traits: self.known_traits.len(),
            next_id: self.next_id,
            dialect_depth: self.dialect_stack.len(),
        }
    }

    pub fn restore(&mut self, s: &Snapshot) {
        self.parse_nodes.truncate(s.nodes);
        self.parse_children.truncate(s.children);
        self.known_types.truncate(s.types);
        self.known_variants.truncate(s.variants);
        self.known_ffi.truncate(s.ffi);
        self.known_methods.truncate(s.methods);
        self.known_traits.truncate(s.traits);
        self.next_id = s.next_id;
        self.dialect_stack.truncate(s.dialect_depth);
    }

    // ── Name queries ──

    pub fn is_type(&self, name: &str) -> bool {
        self.known_types.iter().any(|t| t.name == name)
    }

    pub fn is_struct(&self, name: &str) -> bool {
        self.known_types.iter().any(|t| t.name == name && t.form == TypeForm::Struct)
    }

    pub fn is_domain(&self, name: &str) -> bool {
        self.known_types.iter().any(|t| t.name == name && t.form == TypeForm::Domain)
    }

    pub fn is_variant(&self, name: &str) -> bool {
        self.known_variants.iter().any(|v| v.name == name)
    }

    pub fn is_ffi(&self, name: &str) -> bool {
        self.known_ffi.iter().any(|n| n == name)
    }

    pub fn is_method(&self, name: &str) -> bool {
        self.known_methods.iter().any(|n| n == name)
    }

    pub fn is_trait(&self, name: &str) -> bool {
        self.known_traits.iter().any(|n| n == name)
    }

    pub fn variant_of(&self, name: &str) -> Option<&str> {
        self.known_variants.iter()
            .find(|v| v.name == name)
            .map(|v| v.parent.as_str())
    }

    // ── Node operations ──

    pub fn make_node(&mut self, constructor: &str, key: &str, start: i64, end: i64) -> i64 {
        let id = self.alloc_id();
        let dialect = self.current_dialect_name().to_string();
        self.parse_nodes.push(ParseNode {
            id,
            constructor: constructor.into(),
            key: key.into(),
            dialect,
            parent_id: -1,
            token_start: start,
            token_end: end,
        });
        id
    }

    pub fn add_child(&mut self, parent_id: i64, child_id: i64) {
        let ordinal = self.parse_children.iter()
            .filter(|c| c.parent_id == parent_id)
            .count() as i64;
        self.parse_children.push(ParseChild { parent_id, ordinal, child_id });
        if let Some(node) = self.parse_nodes.iter_mut().find(|n| n.id == child_id) {
            node.parent_id = parent_id;
        }
    }

    pub fn children_of(&self, parent_id: i64) -> Vec<&ParseNode> {
        let mut children: Vec<_> = self.parse_children.iter()
            .filter(|c| c.parent_id == parent_id)
            .collect();
        children.sort_by_key(|c| c.ordinal);
        children.iter()
            .filter_map(|c| self.parse_nodes.iter().find(|n| n.id == c.child_id))
            .collect()
    }

    pub fn find_node(&self, id: i64) -> Option<&ParseNode> {
        self.parse_nodes.iter().find(|n| n.id == id)
    }

    // ── Name registration ──

    pub fn register_domain(&mut self, name: &str) {
        if !self.is_type(name) {
            self.known_types.push(TypeReg { name: name.into(), form: TypeForm::Domain });
        }
    }

    pub fn register_struct(&mut self, name: &str) {
        if !self.is_type(name) {
            self.known_types.push(TypeReg { name: name.into(), form: TypeForm::Struct });
        }
    }

    pub fn register_variant(&mut self, name: &str, parent: &str) {
        self.known_variants.push(VariantReg { name: name.into(), parent: parent.into() });
    }

    pub fn register_ffi(&mut self, name: &str) {
        if !self.is_ffi(name) { self.known_ffi.push(name.into()); }
    }

    pub fn register_method(&mut self, name: &str) {
        if !self.is_method(name) { self.known_methods.push(name.into()); }
    }

    pub fn register_trait(&mut self, name: &str) {
        if !self.is_trait(name) { self.known_traits.push(name.into()); }
    }
}
