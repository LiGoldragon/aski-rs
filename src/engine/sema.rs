//! Sema — the pure binary world.
//!
//! All typed ordinals. No strings. Domain ordinals ARE the bytes.
//! rkyv-serialized to .sema files (the canonical artifact).
//!
//! Name resolution comes from AskiWorld (the text world) via
//! the ResolveName trait. Codegen emits the name registries
//! as Rust enums with Display impls.

use rkyv::{Archive, Serialize, Deserialize};

// ── Typed ordinals ───────────────────────────────────────────────
// Each name domain gets its own newtype. Can't mix them up.
// The inner u32 is the ordinal — index into AskiWorld's registry.

macro_rules! ordinal_type {
    ($name:ident) => {
        #[derive(Archive, Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
        pub struct $name(pub u32);

        impl $name {
            pub fn index(self) -> usize { self.0 as usize }
        }
    };
}

ordinal_type!(TypeName);
ordinal_type!(VariantName);
ordinal_type!(FieldName);
ordinal_type!(TraitName);
ordinal_type!(MethodName);
ordinal_type!(ModuleName);
ordinal_type!(StringLiteral);

// ── Operator enum ────────────────────────────────────────────────
// Fixed domain — not generated, all variants known.

#[derive(Archive, Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
pub enum Operator {
    Add, Sub, Mul, Mod,
    Eq, NotEq, Lt, Gt, LtEq, GtEq,
    And, Or,
}

impl Operator {
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "+" => Some(Self::Add), "-" => Some(Self::Sub),
            "*" => Some(Self::Mul), "%" => Some(Self::Mod),
            "==" => Some(Self::Eq), "!=" => Some(Self::NotEq),
            "<" => Some(Self::Lt), ">" => Some(Self::Gt),
            "<=" => Some(Self::LtEq), ">=" => Some(Self::GtEq),
            "&&" => Some(Self::And), "||" => Some(Self::Or),
            _ => None,
        }
    }

    pub fn as_rust(&self) -> &'static str {
        match self {
            Self::Add => "+", Self::Sub => "-",
            Self::Mul => "*", Self::Mod => "%",
            Self::Eq => "==", Self::NotEq => "!=",
            Self::Lt => "<", Self::Gt => ">",
            Self::LtEq => "<=", Self::GtEq => ">=",
            Self::And => "&&", Self::Or => "||",
        }
    }
}

// ── Sema — the binary world ─────────────────────────────────────

#[derive(Archive, Serialize, Deserialize, Debug, Default, Clone)]
pub struct Sema {
    pub types: Vec<SemaType>,
    pub variants: Vec<SemaVariant>,
    pub fields: Vec<SemaField>,
    pub trait_decls: Vec<SemaTraitDecl>,
    pub trait_impls: Vec<SemaTraitImpl>,
    pub ffi_entries: Vec<SemaFfi>,
    pub constants: Vec<SemaConst>,
    pub process_body: Option<BodyRef>,
    pub modules: Vec<SemaModule>,
    pub arena: ExprArena,
}

#[derive(Archive, Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
pub enum SemaTypeForm { Domain, Struct, Alias }

#[derive(Archive, Serialize, Deserialize, Debug, Clone)]
pub struct SemaType {
    pub name: TypeName,
    pub form: SemaTypeForm,
}

#[derive(Archive, Serialize, Deserialize, Debug, Clone)]
pub struct SemaVariant {
    pub type_id: TypeName,
    pub name: VariantName,
    pub ordinal: u32,
    pub wraps: Option<TypeName>,
}

#[derive(Archive, Serialize, Deserialize, Debug, Clone)]
pub struct SemaField {
    pub type_id: TypeName,
    pub name: FieldName,
    pub field_type: TypeName,
    pub ordinal: u32,
}

#[derive(Archive, Serialize, Deserialize, Debug, Clone)]
pub struct SemaTraitDecl {
    pub name: TraitName,
    pub method_sigs: Vec<SemaMethodSig>,
}

#[derive(Archive, Serialize, Deserialize, Debug, Clone)]
pub struct SemaMethodSig {
    pub name: MethodName,
    pub params: Vec<SemaParam>,
    pub return_type: Option<TypeName>,
}

#[derive(Archive, Serialize, Deserialize, Debug, Clone)]
pub struct SemaTraitImpl {
    pub trait_id: TraitName,
    pub type_id: TypeName,
    pub methods: Vec<SemaMethod>,
}

#[derive(Archive, Serialize, Deserialize, Debug, Clone)]
pub struct SemaMethod {
    pub name: MethodName,
    pub params: Vec<SemaParam>,
    pub return_type: Option<TypeName>,
    pub body: BodyRef,
}

#[derive(Archive, Serialize, Deserialize, Debug, Clone)]
pub struct SemaParam {
    pub name: MethodName, // param names interned in method_names
    pub typ: Option<TypeName>,
    pub borrow: ParamBorrow,
}

#[derive(Archive, Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParamBorrow { Owned, Immutable, Mutable }

#[derive(Archive, Serialize, Deserialize, Debug, Clone)]
pub struct SemaFfi {
    pub library: TypeName, // FFI library name interned in type_names
    pub name: MethodName,
    pub params: Vec<SemaParam>,
    pub return_type: Option<TypeName>,
}

#[derive(Archive, Serialize, Deserialize, Debug, Clone)]
pub struct SemaConst {
    pub name: TypeName, // const name interned in type_names
    pub typ: TypeName,
    pub value: ExprRef,
}

// ── Expression arena ─────────────────────────────────────────────
// Flat storage — no Box, no recursion. All sub-expressions referenced
// by ExprRef (ordinal into the arena). Trivially rkyv-serializable.
// This IS the sema way: everything is ordinals.

ordinal_type!(ExprRef);
ordinal_type!(StmtRef);
ordinal_type!(BodyRef);
ordinal_type!(BindingName);  // local binding names interned per-method

/// Flat expression arena. Lives in Sema. No strings.
#[derive(Archive, Serialize, Deserialize, Debug, Default, Clone)]
pub struct ExprArena {
    pub exprs: Vec<SemaExpr>,
    pub stmts: Vec<SemaStatement>,
    pub bodies: Vec<SemaBody>,
    pub match_arms: Vec<SemaMatchArm>,
}

#[derive(Archive, Serialize, Deserialize, Debug, Clone)]
pub enum SemaExpr {
    IntLit(i64),
    FloatLit(f64),
    StringLit(StringLiteral),
    SelfRef,
    InstanceRef(BindingName),
    QualifiedVariant { domain: TypeName, variant: VariantName },
    BareName(BindingName),
    TypePath { typ: TypeName, member: MethodName },
    BinOp { op: Operator, lhs: ExprRef, rhs: ExprRef },
    FieldAccess { object: ExprRef, field: FieldName },
    MethodCall { object: ExprRef, method: MethodName, args: Vec<ExprRef> },
    Group(ExprRef),
    Return(ExprRef),
    InlineEval(Vec<StmtRef>),
    MatchExpr { target: Option<ExprRef>, arms: Vec<u32> }, // indices into match_arms
    StructConstruct { type_name: TypeName, fields: Vec<(FieldName, ExprRef)> },
    TryUnwrap(ExprRef),  // expr? — error propagation
}

#[derive(Archive, Serialize, Deserialize, Debug, Clone)]
pub enum SemaStatement {
    Expr(ExprRef),
    Allocation { name: BindingName, typ: Option<TypeName>, init: Option<ExprRef> },
    MutAllocation { name: BindingName, typ: Option<TypeName>, init: Option<ExprRef> },
    Mutation { target: BindingName, method: MethodName, args: Vec<ExprRef> },
    Iteration { source: ExprRef, body: Vec<StmtRef> },
}

#[derive(Archive, Serialize, Deserialize, Debug, Clone)]
pub enum SemaBody {
    Empty,
    Block(Vec<StmtRef>),
    MatchBody { target: Option<ExprRef>, arms: Vec<u32> }, // indices into match_arms
}

#[derive(Archive, Serialize, Deserialize, Debug, Clone)]
pub struct SemaMatchArm {
    pub patterns: Vec<SemaPattern>,
    pub result: ExprRef,
}

#[derive(Archive, Serialize, Deserialize, Debug, Clone)]
pub enum SemaPattern {
    Variant(VariantName),
    Or(Vec<VariantName>),  // flattened — or-patterns are always variant lists
}

// ── Module system ────────────────────────────────────────────────

#[derive(Archive, Serialize, Deserialize, Debug, Clone)]
pub struct SemaModule {
    pub name: ModuleName,
    pub is_main: bool,
    pub declaration_order: Vec<SemaDeclarationRef>,
}

#[derive(Archive, Serialize, Deserialize, Debug, Clone)]
pub enum SemaDeclarationRef {
    Type(u32),
    TraitDecl(u32),
    TraitImpl(u32),
    Const(u32),
    Ffi(u32),
}

impl ExprArena {
    pub fn alloc_expr(&mut self, expr: SemaExpr) -> ExprRef {
        let idx = self.exprs.len() as u32;
        self.exprs.push(expr);
        ExprRef(idx)
    }

    pub fn alloc_stmt(&mut self, stmt: SemaStatement) -> StmtRef {
        let idx = self.stmts.len() as u32;
        self.stmts.push(stmt);
        StmtRef(idx)
    }

    pub fn alloc_body(&mut self, body: SemaBody) -> BodyRef {
        let idx = self.bodies.len() as u32;
        self.bodies.push(body);
        BodyRef(idx)
    }

    pub fn alloc_match_arm(&mut self, arm: SemaMatchArm) -> u32 {
        let idx = self.match_arms.len() as u32;
        self.match_arms.push(arm);
        idx
    }

    pub fn expr(&self, r: ExprRef) -> &SemaExpr { &self.exprs[r.index()] }
    pub fn stmt(&self, r: StmtRef) -> &SemaStatement { &self.stmts[r.index()] }
    pub fn body(&self, r: BodyRef) -> &SemaBody { &self.bodies[r.index()] }
    pub fn match_arm(&self, idx: u32) -> &SemaMatchArm { &self.match_arms[idx as usize] }
}

// ── Name resolution ──────────────────────────────────────────────
// Names live in AskiWorld (the text world). Sema has only ordinals.
// Codegen and deparse use this trait to resolve ordinals → strings.

pub trait ResolveName {
    fn type_name(&self, id: TypeName) -> &str;
    fn variant_name(&self, id: VariantName) -> &str;
    fn field_name(&self, id: FieldName) -> &str;
    fn trait_name(&self, id: TraitName) -> &str;
    fn method_name(&self, id: MethodName) -> &str;
    fn module_name(&self, id: ModuleName) -> &str;
    fn literal_string(&self, id: StringLiteral) -> &str;

    fn binding_name(&self, id: BindingName) -> &str;

    fn type_count(&self) -> usize;
    fn variant_count(&self) -> usize;
    fn field_count(&self) -> usize;
    fn trait_count(&self) -> usize;
    fn method_count(&self) -> usize;
}

// ── Interning ────────────────────────────────────────────────────
// During lower, names are interned into AskiWorld's registries.
// These helper methods live on a NameInterner that wraps the tables.

pub struct NameInterner {
    pub type_names: Vec<String>,
    pub variant_names: Vec<String>,
    pub field_names: Vec<String>,
    pub trait_names: Vec<String>,
    pub method_names: Vec<String>,
    pub module_names: Vec<String>,
    pub literal_strings: Vec<String>,
    pub binding_names: Vec<String>,
}

impl Default for NameInterner {
    fn default() -> Self {
        Self {
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
}

macro_rules! intern_method {
    ($name:ident, $table:ident, $ordinal:ident) => {
        pub fn $name(&mut self, name: &str) -> $ordinal {
            if let Some(i) = self.$table.iter().position(|n| n == name) {
                $ordinal(i as u32)
            } else {
                let i = self.$table.len() as u32;
                self.$table.push(name.into());
                $ordinal(i)
            }
        }
    };
}

impl NameInterner {
    intern_method!(intern_type, type_names, TypeName);
    intern_method!(intern_variant, variant_names, VariantName);
    intern_method!(intern_field, field_names, FieldName);
    intern_method!(intern_trait, trait_names, TraitName);
    intern_method!(intern_method, method_names, MethodName);
    intern_method!(intern_module, module_names, ModuleName);
    intern_method!(intern_string, literal_strings, StringLiteral);
    intern_method!(intern_binding, binding_names, BindingName);

    pub fn binding(&self, id: BindingName) -> &str { &self.binding_names[id.index()] }
}

impl ResolveName for NameInterner {
    fn type_name(&self, id: TypeName) -> &str { &self.type_names[id.index()] }
    fn variant_name(&self, id: VariantName) -> &str { &self.variant_names[id.index()] }
    fn field_name(&self, id: FieldName) -> &str { &self.field_names[id.index()] }
    fn trait_name(&self, id: TraitName) -> &str { &self.trait_names[id.index()] }
    fn method_name(&self, id: MethodName) -> &str { &self.method_names[id.index()] }
    fn module_name(&self, id: ModuleName) -> &str { &self.module_names[id.index()] }
    fn literal_string(&self, id: StringLiteral) -> &str { &self.literal_strings[id.index()] }
    fn binding_name(&self, id: BindingName) -> &str { &self.binding_names[id.index()] }

    fn type_count(&self) -> usize { self.type_names.len() }
    fn variant_count(&self) -> usize { self.variant_names.len() }
    fn field_count(&self) -> usize { self.field_names.len() }
    fn trait_count(&self) -> usize { self.trait_names.len() }
    fn method_count(&self) -> usize { self.method_names.len() }
}

// ── Aski name table ──────────────────────────────────────────────
// Separate sema file: .aski-table.sema
// Maps ordinals → aski source names. One possible projection.
// Other tables (rust-table, display-table) could exist for other targets.

#[derive(Archive, Serialize, Deserialize, Debug, Default, Clone)]
pub struct AskiNameTable {
    pub type_names: Vec<String>,
    pub variant_names: Vec<String>,
    pub field_names: Vec<String>,
    pub trait_names: Vec<String>,
    pub method_names: Vec<String>,
    pub module_names: Vec<String>,
    pub literal_strings: Vec<String>,
    pub binding_names: Vec<String>,
    pub exports: Vec<String>,
}

impl ResolveName for AskiNameTable {
    fn type_name(&self, id: TypeName) -> &str { &self.type_names[id.index()] }
    fn variant_name(&self, id: VariantName) -> &str { &self.variant_names[id.index()] }
    fn field_name(&self, id: FieldName) -> &str { &self.field_names[id.index()] }
    fn trait_name(&self, id: TraitName) -> &str { &self.trait_names[id.index()] }
    fn method_name(&self, id: MethodName) -> &str { &self.method_names[id.index()] }
    fn module_name(&self, id: ModuleName) -> &str { &self.module_names[id.index()] }
    fn literal_string(&self, id: StringLiteral) -> &str { &self.literal_strings[id.index()] }
    fn binding_name(&self, id: BindingName) -> &str { &self.binding_names[id.index()] }

    fn type_count(&self) -> usize { self.type_names.len() }
    fn variant_count(&self) -> usize { self.variant_names.len() }
    fn field_count(&self) -> usize { self.field_names.len() }
    fn trait_count(&self) -> usize { self.trait_names.len() }
    fn method_count(&self) -> usize { self.method_names.len() }
}

impl AskiNameTable {
    pub fn to_bytes(&self) -> Vec<u8> {
        rkyv::to_bytes::<rkyv::rancor::Error>(self)
            .expect("name table serialization failed")
            .to_vec()
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, String> {
        let archived = unsafe { rkyv::access_unchecked::<ArchivedAskiNameTable>(bytes) };
        rkyv::deserialize::<AskiNameTable, rkyv::rancor::Error>(archived)
            .map_err(|e| format!("name table deserialization failed: {}", e))
    }
}

impl AskiNameTable {
    pub fn from_lower(interner: &NameInterner, exports: &[String]) -> Self {
        AskiNameTable {
            type_names: interner.type_names.clone(),
            variant_names: interner.variant_names.clone(),
            field_names: interner.field_names.clone(),
            trait_names: interner.trait_names.clone(),
            method_names: interner.method_names.clone(),
            module_names: interner.module_names.clone(),
            literal_strings: interner.literal_strings.clone(),
            binding_names: interner.binding_names.clone(),
            exports: exports.to_vec(),
        }
    }
}

// ── Serialization ────────────────────────────────────────────────

pub trait SemaSerialize {
    fn to_sema_bytes(&self) -> Vec<u8>;
}

impl SemaSerialize for Sema {
    fn to_sema_bytes(&self) -> Vec<u8> {
        rkyv::to_bytes::<rkyv::rancor::Error>(self)
            .expect("sema serialization failed")
            .to_vec()
    }
}

impl Sema {
    pub fn from_sema_bytes(bytes: &[u8]) -> Result<Sema, String> {
        let archived = unsafe { rkyv::access_unchecked::<ArchivedSema>(bytes) };
        rkyv::deserialize::<Sema, rkyv::rancor::Error>(archived)
            .map_err(|e| format!("sema deserialization failed: {}", e))
    }
}
