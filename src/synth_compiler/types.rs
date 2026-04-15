//! Stage 1 types — all typed structures the synth compiler produces.
//!
//! Every name domain is a Vec<String> indexed by a typed ordinal.
//! The ordinal types wrap u32 — the discriminant of a generated enum.
//! In Stage 2, these become actual enum variants. Here in the bootstrap,
//! we use u32 newtypes because we can't generate Rust enums at compile time.

use rkyv::Archive;

// ── Name ordinals ──────────────────────────────────────────────

macro_rules! ordinal {
    ($name:ident) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Archive, rkyv::Serialize, rkyv::Deserialize)]
        #[repr(transparent)]
        pub struct $name(pub u32);

        impl $name {
            pub const NONE: Self = Self(u32::MAX);
            pub fn index(self) -> usize { self.0 as usize }
        }
    };
}

ordinal!(TypeName);
ordinal!(VariantName);
ordinal!(FieldName);
ordinal!(TraitName);
ordinal!(MethodName);
ordinal!(ModuleName);
ordinal!(StringLiteral);
ordinal!(BindingName);

// ── NameRegistry ───────────────────────────────────────────────

/// Global interning table for all names across all modules.
/// Each domain is a Vec<String> indexed by the corresponding ordinal.
#[derive(Debug, Clone, Archive, rkyv::Serialize, rkyv::Deserialize)]
pub struct NameRegistry {
    pub type_names: Vec<String>,
    pub variant_names: Vec<String>,
    pub field_names: Vec<String>,
    pub trait_names: Vec<String>,
    pub method_names: Vec<String>,
    pub module_names: Vec<String>,
    pub literal_strings: Vec<String>,
    pub binding_names: Vec<String>,
}

impl NameRegistry {
    pub fn new() -> Self {
        NameRegistry {
            type_names: Vec::new(),
            variant_names: Vec::new(),
            field_names: Vec::new(),
            trait_names: Vec::new(),
            method_names: Vec::new(),
            module_names: Vec::new(),
            literal_strings: Vec::new(),
            binding_names: Vec::new(),
        }
    }

    pub fn intern_type(&mut self, name: &str) -> TypeName {
        intern(&mut self.type_names, name, TypeName)
    }

    pub fn intern_variant(&mut self, name: &str) -> VariantName {
        intern(&mut self.variant_names, name, VariantName)
    }

    pub fn intern_field(&mut self, name: &str) -> FieldName {
        intern(&mut self.field_names, name, FieldName)
    }

    pub fn intern_trait(&mut self, name: &str) -> TraitName {
        intern(&mut self.trait_names, name, TraitName)
    }

    pub fn intern_method(&mut self, name: &str) -> MethodName {
        intern(&mut self.method_names, name, MethodName)
    }

    pub fn intern_module(&mut self, name: &str) -> ModuleName {
        intern(&mut self.module_names, name, ModuleName)
    }

    pub fn intern_string(&mut self, s: &str) -> StringLiteral {
        intern(&mut self.literal_strings, s, StringLiteral)
    }

    pub fn intern_binding(&mut self, name: &str) -> BindingName {
        intern(&mut self.binding_names, name, BindingName)
    }

    pub fn resolve_type(&self, id: TypeName) -> &str {
        &self.type_names[id.index()]
    }

    pub fn resolve_variant(&self, id: VariantName) -> &str {
        &self.variant_names[id.index()]
    }

    pub fn resolve_field(&self, id: FieldName) -> &str {
        &self.field_names[id.index()]
    }

    pub fn resolve_trait(&self, id: TraitName) -> &str {
        &self.trait_names[id.index()]
    }

    pub fn resolve_method(&self, id: MethodName) -> &str {
        &self.method_names[id.index()]
    }

    pub fn resolve_module(&self, id: ModuleName) -> &str {
        &self.module_names[id.index()]
    }
}

/// Dedup-intern a string into a Vec, returning the ordinal.
fn intern<F, T>(table: &mut Vec<String>, name: &str, wrap: F) -> T
where
    F: Fn(u32) -> T,
{
    if let Some(pos) = table.iter().position(|s| s == name) {
        wrap(pos as u32)
    } else {
        let idx = table.len() as u32;
        table.push(name.to_string());
        wrap(idx)
    }
}

// ── NameRef ────────────────────────────────────────────────────

/// A reference to any kind of name (discriminated union).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Archive, rkyv::Serialize, rkyv::Deserialize)]
pub enum NameRef {
    Type(TypeName),
    Variant(VariantName),
    Field(FieldName),
    Trait(TraitName),
    Method(MethodName),
    Module(ModuleName),
    Binding(BindingName),
    Literal(StringLiteral),
    Operator(Operator),
    None,
}

/// Binary operators.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Archive, rkyv::Serialize, rkyv::Deserialize)]
pub enum Operator {
    Add,
    Sub,
    Mul,
    Mod,
    Eq,
    NotEq,
    Lt,
    Gt,
    LtEq,
    GtEq,
    And,
    Or,
}

// ── NodeKind ───────────────────────────────────────────────────

/// Every construct that creates a node in the data-tree.
/// Generated from the synth dialect tree (~75 variants).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Archive, rkyv::Serialize, rkyv::Deserialize)]
pub enum NodeKind {
    // Root (aski.synth)
    Root,
    Module,
    Domain,
    TraitDecl,
    TraitImpl,
    Struct,
    Const,
    Ffi,
    Process,

    // Domain (domain.synth)
    Variant,
    DataVariant,
    StructVariant,

    // Struct (struct.synth)
    TypedField,
    SelfTypedField,

    // Trait (trait-decl, trait-impl, type-impl)
    SignatureBlock,
    Signature,
    TypeImpl,
    Method,

    // Params (param.synth)
    BorrowParam,
    MutBorrowParam,
    OwnedParam,
    NamedParam,

    // Body (body.synth)
    Block,
    TailBlock,
    MatchBody,

    // Statement (statement.synth)
    EarlyReturn,
    LoopBlock,
    Iteration,
    MutationStmt,
    AllocationStmt,
    ExprStatement,

    // Allocation (allocation.synth)
    TypedAlloc,
    InitAlloc,
    BareAlloc,

    // Mutation (mutation.synth)
    MethodMut,
    TypeMut,
    InitMut,

    // Loop (loop.synth)
    ConditionalLoop,
    InfiniteLoop,

    // Match (match.synth)
    MatchArm,

    // Pattern (pattern.synth)
    VariantBindPattern,
    VariantPattern,
    LiteralPattern,
    WildcardPattern,
    OrPattern,

    // Expressions — binary (expr-*.synth)
    BinOr,
    BinAnd,
    BinEq,
    BinNotEq,
    BinLt,
    BinGt,
    BinLtEq,
    BinGtEq,
    BinAdd,
    BinSub,
    BinMul,
    BinMod,

    // Expressions — postfix (expr-postfix.synth)
    FieldAccess,
    MethodCall,
    TryUnwrap,

    // Expressions — atoms (expr-atom.synth)
    InstanceRef,
    QualifiedVariant,
    BareName,
    IntLit,
    FloatLit,
    StringLit,
    Group,
    InlineEval,
    MatchExpr,
    LoopExpr,
    StructConstruct,

    // Meta nodes
    ReturnType,
    TypeRef,
    Export,
    Import,
    MatchTarget,
    PatternBind,
}

// ── DialectKind ────────────────────────────────────────────────

/// The 28 synth dialects, indexed by enum variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DialectKind {
    Aski,
    Module,
    Domain,
    Struct,
    TraitDecl,
    TraitImpl,
    TypeImpl,
    Method,
    Signature,
    Param,
    Body,
    Statement,
    Loop,
    Allocation,
    Mutation,
    Match,
    Pattern,
    Ffi,
    Main,
    Process,
    Expr,
    ExprOr,
    ExprAnd,
    ExprCompare,
    ExprAdd,
    ExprMul,
    ExprPostfix,
    ExprAtom,
}

pub const DIALECT_COUNT: usize = 28;

impl DialectKind {
    /// Map a synth filename (without .synth) to a DialectKind.
    pub fn from_filename(name: &str) -> Option<Self> {
        match name {
            "aski" => Some(Self::Aski),
            "module" => Some(Self::Module),
            "domain" => Some(Self::Domain),
            "struct" => Some(Self::Struct),
            "trait-decl" => Some(Self::TraitDecl),
            "trait-impl" => Some(Self::TraitImpl),
            "type-impl" => Some(Self::TypeImpl),
            "method" => Some(Self::Method),
            "signature" => Some(Self::Signature),
            "param" => Some(Self::Param),
            "body" => Some(Self::Body),
            "statement" => Some(Self::Statement),
            "loop" => Some(Self::Loop),
            "allocation" => Some(Self::Allocation),
            "mutation" => Some(Self::Mutation),
            "match" => Some(Self::Match),
            "pattern" => Some(Self::Pattern),
            "ffi" => Some(Self::Ffi),
            "main" => Some(Self::Main),
            "process" => Some(Self::Process),
            "expr" => Some(Self::Expr),
            "expr-or" => Some(Self::ExprOr),
            "expr-and" => Some(Self::ExprAnd),
            "expr-compare" => Some(Self::ExprCompare),
            "expr-add" => Some(Self::ExprAdd),
            "expr-mul" => Some(Self::ExprMul),
            "expr-postfix" => Some(Self::ExprPostfix),
            "expr-atom" => Some(Self::ExprAtom),
            _ => None,
        }
    }

    /// Map a `<dialect-ref>` name from synth source to a DialectKind.
    pub fn from_dialect_ref(name: &str) -> Option<Self> {
        match name {
            "module" => Some(Self::Module),
            "domain" => Some(Self::Domain),
            "struct" => Some(Self::Struct),
            "trait-decl" => Some(Self::TraitDecl),
            "trait-impl" => Some(Self::TraitImpl),
            "type-impl" => Some(Self::TypeImpl),
            "method" => Some(Self::Method),
            "signature" => Some(Self::Signature),
            "param" => Some(Self::Param),
            "body" => Some(Self::Body),
            "statement" => Some(Self::Statement),
            "loop" => Some(Self::Loop),
            "allocation" => Some(Self::Allocation),
            "mutation" => Some(Self::Mutation),
            "match" => Some(Self::Match),
            "pattern" => Some(Self::Pattern),
            "ffi" => Some(Self::Ffi),
            "main" => Some(Self::Main),
            "process" => Some(Self::Process),
            "expr" => Some(Self::Expr),
            "expr-or" => Some(Self::ExprOr),
            "expr-and" => Some(Self::ExprAnd),
            "expr-compare" => Some(Self::ExprCompare),
            "expr-add" => Some(Self::ExprAdd),
            "expr-mul" => Some(Self::ExprMul),
            "expr-postfix" => Some(Self::ExprPostfix),
            "expr-atom" => Some(Self::ExprAtom),
            _ => None,
        }
    }
}

// ── DialectTable ───────────────────────────────────────────────

use crate::synth::types::Dialect;

/// All 28 dialects, indexed by DialectKind.
pub struct DialectTable {
    dialects: Vec<Option<Dialect>>,
}

impl DialectTable {
    pub fn new() -> Self {
        let mut dialects = Vec::with_capacity(DIALECT_COUNT);
        for _ in 0..DIALECT_COUNT {
            dialects.push(None);
        }
        DialectTable { dialects }
    }

    pub fn insert(&mut self, kind: DialectKind, dialect: Dialect) {
        self.dialects[kind as usize] = Some(dialect);
    }

    pub fn get(&self, kind: DialectKind) -> Option<&Dialect> {
        self.dialects[kind as usize].as_ref()
    }
}

// ── Span ───────────────────────────────────────────────────────

/// Source position — every node knows its module and byte range.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Archive, rkyv::Serialize, rkyv::Deserialize)]
pub struct Span {
    pub module: ModuleName,
    pub start: u32,
    pub end: u32,
}

impl Span {
    pub fn new(module: ModuleName, start: u32, end: u32) -> Self {
        Span { module, start, end }
    }

    pub fn empty(module: ModuleName) -> Self {
        Span { module, start: 0, end: 0 }
    }
}

// ── Node ───────────────────────────────────────────────────────

/// Literal values that don't fit in NameRef.
#[derive(Debug, Clone, Archive, rkyv::Serialize, rkyv::Deserialize)]
pub enum NodeValue {
    None,
    Int(i64),
    Float(f64),
}

/// The fundamental unit of the data-tree.
/// Owned children (tree structure), no IDs, no parent pointers.
#[derive(Debug, Clone, Archive, rkyv::Serialize, rkyv::Deserialize)]
pub struct Node {
    pub kind: NodeKind,
    pub name: NameRef,
    pub children: Vec<Node>,
    pub span: Span,
    pub value: NodeValue,
}

impl Node {
    pub fn new(kind: NodeKind, name: NameRef, span: Span) -> Self {
        Node { kind, name, children: Vec::new(), span, value: NodeValue::None }
    }

    pub fn with_children(kind: NodeKind, name: NameRef, span: Span, children: Vec<Node>) -> Self {
        Node { kind, name, children, span, value: NodeValue::None }
    }

    pub fn with_value(kind: NodeKind, name: NameRef, span: Span, value: NodeValue) -> Self {
        Node { kind, name, children: Vec::new(), span, value }
    }
}

// ── ScopeGraph ─────────────────────────────────────────────────

/// What a module declares about a name.
#[derive(Debug, Clone, Archive, rkyv::Serialize, rkyv::Deserialize)]
pub enum DeclaredName {
    Type {
        name: TypeName,
        form: TypeForm,
    },
    Variant {
        name: VariantName,
        parent: TypeName,
        ordinal: u32,
        wraps: Option<TypeName>,
    },
    Field {
        name: FieldName,
        owner: TypeName,
        field_type: TypeName,
        ordinal: u32,
    },
    Trait {
        name: TraitName,
        methods: Vec<MethodName>,
    },
    Method {
        name: MethodName,
    },
    Const {
        name: TypeName,
        typ: TypeName,
    },
    Ffi {
        library: TypeName,
        functions: Vec<MethodName>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Archive, rkyv::Serialize, rkyv::Deserialize)]
pub enum TypeForm {
    Domain,
    Struct,
    Alias,
}

/// An import from another module.
#[derive(Debug, Clone, Archive, rkyv::Serialize, rkyv::Deserialize)]
pub struct ModuleImport {
    pub source: ModuleName,
    pub names: Vec<NameRef>,
}

/// One entry per .aski file.
#[derive(Debug, Clone, Archive, rkyv::Serialize, rkyv::Deserialize)]
pub struct ModuleScope {
    pub name: ModuleName,
    pub path: String,
    pub is_main: bool,
    pub declared: Vec<DeclaredName>,
    pub exports: Vec<NameRef>,
    pub imports: Vec<ModuleImport>,
}

/// The complete scope graph.
#[derive(Debug, Clone, Archive, rkyv::Serialize, rkyv::Deserialize)]
pub struct ScopeGraph {
    pub modules: Vec<ModuleScope>,
}

impl ScopeGraph {
    pub fn new() -> Self {
        ScopeGraph { modules: Vec::new() }
    }
}

// ── SynthOutput ────────────────────────────────────────────────

/// The complete output of Stage 1.
#[derive(Archive, rkyv::Serialize, rkyv::Deserialize)]
pub struct SynthOutput {
    pub names: NameRegistry,
    pub scope: ScopeGraph,
}
