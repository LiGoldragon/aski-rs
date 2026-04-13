//! SemaWorld — legacy in-memory format.
//!
//! Being replaced by sema.rs (pure ordinals, no strings, rkyv).
//! Kept temporarily while lower/codegen/raise migrate.

/// Legacy in-memory format with string tables.
#[derive(Debug, Default, Clone)]
pub struct SemaWorld {
    pub types: Vec<SemaType>,
    pub variants: Vec<SemaVariant>,
    pub fields: Vec<SemaField>,
    pub trait_decls: Vec<SemaTraitDecl>,
    pub trait_impls: Vec<SemaTraitImpl>,
    pub methods: Vec<SemaMethod>,
    pub ffi_entries: Vec<SemaFfi>,
    pub constants: Vec<SemaConst>,

    // Process body — for .main files (fn main)
    pub process_body: Option<SemaBody>,

    // Module system — filesystem-regenerative
    pub modules: Vec<SemaModule>,

    // Name tables (ordinal → string, for codegen + deparse)
    pub type_names: Vec<String>,
    pub variant_names: Vec<String>,
    pub field_names: Vec<String>,
    pub trait_names: Vec<String>,
    pub method_names: Vec<String>,
    pub module_names: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SemaTypeForm { Domain, Struct, Alias }

#[derive(Debug, Clone)]
pub struct SemaType {
    pub name: i64,
    pub form: SemaTypeForm,
}

#[derive(Debug, Clone)]
pub struct SemaVariant {
    pub type_id: i64,
    pub name: i64,
    pub ordinal: i64,
    pub wraps: i64,  // -1 = unit, else index into type_names
}

#[derive(Debug, Clone)]
pub struct SemaField {
    pub type_id: i64,
    pub name: i64,
    pub field_type: String,
    pub ordinal: i64,
}

#[derive(Debug, Clone)]
pub struct SemaTraitDecl {
    pub name: i64,
    pub method_sigs: Vec<SemaMethodSig>,
}

#[derive(Debug, Clone)]
pub struct SemaMethodSig {
    pub name: i64,
    pub params: Vec<SemaParam>,
    pub return_type: String,
}

#[derive(Debug, Clone)]
pub struct SemaTraitImpl {
    pub trait_id: i64,
    pub type_id: i64,
    pub methods: Vec<SemaMethod>,
}

#[derive(Debug, Clone)]
pub struct SemaMethod {
    pub name: i64,
    pub params: Vec<SemaParam>,
    pub return_type: String,
    pub body: SemaBody,
}

#[derive(Debug, Clone)]
pub struct SemaParam {
    pub name: String,
    pub typ: String,
    pub borrow: ParamBorrow,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParamBorrow { Owned, Immutable, Mutable }

#[derive(Debug, Clone)]
pub struct SemaFfi {
    pub library: String,
    pub name: i64,
    pub params: Vec<SemaParam>,
    pub return_type: String,
}

#[derive(Debug, Clone)]
pub struct SemaConst {
    pub name: String,
    pub typ: String,
    pub value: SemaExpr,
}

// ── Expression tree ──────────────────────────────────────────────
// Fully-lowered expressions. Codegen reads only these — no parse
// tree references. SemaWorld is self-contained. rkyv handles
// Box<T> via ArchivedBox and Vec<T> via ArchivedVec.

#[derive(Debug, Clone)]
pub enum SemaExpr {
    IntLit(i64),
    FloatLit(String),
    StringLit(String),
    SelfRef,
    InstanceRef(String),
    QualifiedVariant { domain: i64, variant: i64 },
    BareName(String),
    TypePath(String),
    BinOp { op: String, lhs: Box<SemaExpr>, rhs: Box<SemaExpr> },
    FieldAccess { object: Box<SemaExpr>, field: String },
    MethodCall { object: Box<SemaExpr>, method: String, args: Vec<SemaExpr> },
    Group(Box<SemaExpr>),
    Return(Box<SemaExpr>),
    InlineEval(Vec<SemaStatement>),
    MatchExpr { target: Option<Box<SemaExpr>>, arms: Vec<SemaMatchArm> },
    StructConstruct { type_name: String, fields: Vec<(String, SemaExpr)> },
}

#[derive(Debug, Clone)]
pub enum SemaStatement {
    Expr(SemaExpr),
    Allocation { name: String, typ: Option<String>, init: Option<SemaExpr> },
    MutAllocation { name: String, typ: Option<String>, init: Option<SemaExpr> },
    Mutation { target: String, method: String, args: Vec<SemaExpr> },
    Iteration { source: SemaExpr, body: Vec<SemaStatement> },
}

#[derive(Debug, Clone)]
pub enum SemaBody {
    Empty,
    Block(Vec<SemaStatement>),
    MatchBody { target: Option<SemaExpr>, arms: Vec<SemaMatchArm> },
}

#[derive(Debug, Clone)]
pub struct SemaMatchArm {
    pub patterns: Vec<SemaPattern>,
    pub result: SemaExpr,
}

#[derive(Debug, Clone)]
pub enum SemaPattern {
    Variant(i64),
    Or(Vec<SemaPattern>),
}

// ── Module system ────────────────────────────────────────────────
// Stores everything needed to completely regenerate the filesystem
// from which the data came.

#[derive(Debug, Clone)]
pub struct SemaModule {
    pub name: i64,
    pub file_path: String,
    pub is_main: bool,
    pub exports: Vec<String>,
    pub imports: Vec<SemaImport>,
    pub declaration_order: Vec<SemaDeclarationRef>,
}

#[derive(Debug, Clone)]
pub struct SemaImport {
    pub module_name: String,
    pub names: Vec<String>,
}

/// Points to a declaration in the world by kind + index.
#[derive(Debug, Clone)]
pub enum SemaDeclarationRef {
    Type(usize),
    TraitDecl(usize),
    TraitImpl(usize),
    Const(usize),
    Ffi(usize),
}

// ── Serialization ────────────────────────────────────────────────
// SemaWorld → Sema binary (rkyv) and back.





// ── Name interning ───────────────────────────────────────────────

impl SemaWorld {
    pub fn new() -> Self { Self::default() }

    pub fn intern_type(&mut self, name: &str) -> i64 {
        if let Some(i) = self.type_names.iter().position(|n| n == name) {
            i as i64
        } else {
            let i = self.type_names.len() as i64;
            self.type_names.push(name.into());
            i
        }
    }

    pub fn intern_variant(&mut self, name: &str) -> i64 {
        if let Some(i) = self.variant_names.iter().position(|n| n == name) {
            i as i64
        } else {
            let i = self.variant_names.len() as i64;
            self.variant_names.push(name.into());
            i
        }
    }

    pub fn intern_field(&mut self, name: &str) -> i64 {
        if let Some(i) = self.field_names.iter().position(|n| n == name) {
            i as i64
        } else {
            let i = self.field_names.len() as i64;
            self.field_names.push(name.into());
            i
        }
    }

    pub fn intern_trait(&mut self, name: &str) -> i64 {
        if let Some(i) = self.trait_names.iter().position(|n| n == name) {
            i as i64
        } else {
            let i = self.trait_names.len() as i64;
            self.trait_names.push(name.into());
            i
        }
    }

    pub fn intern_method(&mut self, name: &str) -> i64 {
        if let Some(i) = self.method_names.iter().position(|n| n == name) {
            i as i64
        } else {
            let i = self.method_names.len() as i64;
            self.method_names.push(name.into());
            i
        }
    }

    pub fn intern_module(&mut self, name: &str) -> i64 {
        if let Some(i) = self.module_names.iter().position(|n| n == name) {
            i as i64
        } else {
            let i = self.module_names.len() as i64;
            self.module_names.push(name.into());
            i
        }
    }
}
