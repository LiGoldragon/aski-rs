/// Span in source text.
pub type Span = std::ops::Range<usize>;

/// A spanned AST node.
#[derive(Debug, Clone, PartialEq)]
pub struct Spanned<T> {
    pub node: T,
    pub span: Span,
}

impl<T> Spanned<T> {
    pub fn new(node: T, span: Span) -> Self {
        Self { node, span }
    }
}

/// Top-level item in an aski v0.9 source file.
#[derive(Debug, Clone, PartialEq)]
pub enum Item {
    /// Domain declaration: `Name (Variant1 Variant2 ...)`
    Domain(DomainDecl),
    /// Struct declaration: `Name { Field Type ... }`
    Struct(StructDecl),
    /// Trait declaration: `traitName ([method signatures])`
    Trait(TraitDecl),
    /// Trait implementation: `traitName [TypeName [methods]]`
    TraitImpl(TraitImplDecl),
    /// Constant: `!Name Type {value}`
    Const(ConstDecl),
    /// Main entry point: `Main [ body ]`
    Main(MainDecl),
    /// Type alias: `ChartResult Result{NatalChart ChartError}`
    TypeAlias(TypeAliasDecl),
}

/// Domain (enum) declaration.
#[derive(Debug, Clone, PartialEq)]
pub struct DomainDecl {
    pub name: String,
    pub variants: Vec<Variant>,
    pub span: Span,
}

/// A variant in a domain.
/// Three forms:
///   `Variant`              — unit (no data)
///   `Variant (Type)`       — newtype wrap (solar)
///   `Variant { fields }`   — struct variant (saturnian)
///   `Variant (A B C)`      — inline domain (variant carries an enum)
#[derive(Debug, Clone, PartialEq)]
pub struct Variant {
    pub name: String,
    /// Optional wrapped type: `Some (T)` wraps T.
    pub wraps: Option<TypeRef>,
    /// Optional struct fields: `Variant { Field Type ... }`
    pub fields: Option<Vec<Field>>,
    /// Optional inline domain: `Variant (A B C)` — variant carries an enum
    pub sub_variants: Option<Vec<Variant>>,
    pub span: Span,
}

/// Struct declaration.
#[derive(Debug, Clone, PartialEq)]
pub struct StructDecl {
    pub name: String,
    pub fields: Vec<Field>,
    pub span: Span,
}

/// A field (sub-type) in a struct.
#[derive(Debug, Clone, PartialEq)]
pub struct Field {
    pub name: String,
    pub type_ref: TypeRef,
    pub span: Span,
}

/// Trait declaration.
#[derive(Debug, Clone, PartialEq)]
pub struct TraitDecl {
    pub name: String,
    /// Supertrait names (camelCase): `print (display [methods])`
    pub supertraits: Vec<String>,
    pub methods: Vec<MethodSig>,
    /// Associated constants: `!Max U32` or `!Max U32 {360}`
    pub constants: Vec<ConstDecl>,
    pub span: Span,
}

/// Trait implementation block.
#[derive(Debug, Clone, PartialEq)]
pub struct TraitImplDecl {
    pub trait_name: String,
    pub impls: Vec<TypeImpl>,
    pub span: Span,
}

/// A single type's implementation of a trait.
#[derive(Debug, Clone, PartialEq)]
pub struct TypeImpl {
    pub target: String,
    pub methods: Vec<MethodDef>,
    pub associated_types: Vec<AssociatedTypeDef>,
    /// Associated constants: `!Max U32 {360}`
    pub associated_constants: Vec<ConstDecl>,
    pub span: Span,
}

/// Associated type definition in a trait impl: `Output Point`
#[derive(Debug, Clone, PartialEq)]
pub struct AssociatedTypeDef {
    pub name: String,
    pub concrete_type: TypeRef,
    pub span: Span,
}

/// Method signature (in trait declarations).
#[derive(Debug, Clone, PartialEq)]
pub struct MethodSig {
    pub name: String,
    pub params: Vec<Param>,
    pub output: Option<TypeRef>,
    pub span: Span,
}

/// Method definition (in impl blocks).
/// Body can be computed `[...]` or matching `(| ... |)`.
#[derive(Debug, Clone, PartialEq)]
pub struct MethodDef {
    pub name: String,
    pub params: Vec<Param>,
    pub output: Option<TypeRef>,
    pub body: Body,
    pub span: Span,
}

/// Constant declaration.
#[derive(Debug, Clone, PartialEq)]
pub struct ConstDecl {
    pub name: String,
    pub type_ref: TypeRef,
    pub value: Option<Body>,
    pub span: Span,
}

/// Main entry point.
#[derive(Debug, Clone, PartialEq)]
pub struct MainDecl {
    pub body: Body,
    pub span: Span,
}

/// A parameter in a method signature.
/// `@Type` (same-type), `@Name Type` (sub-type), `:@Self`, `~@Self`, `@Self` (owned)
#[derive(Debug, Clone, PartialEq)]
pub enum Param {
    /// `@Type` — same-type: name matches existing type, moved
    Owned(String),
    /// `@Name Type` — sub-type: creates wrapper, moved
    Named(String, TypeRef),
    /// `:@Self` — immutable borrow of self
    BorrowSelf,
    /// `~@Self` — mutable borrow of self
    MutBorrowSelf,
    /// `@Self` — owned/consumed self (move semantics)
    OwnedSelf,
    /// `:@Type` — immutable borrow (same-type)
    Borrow(String),
    /// `~@Type` — mutable borrow (same-type)
    MutBorrow(String),
}

/// Trait bound: `{|a&display|}` — type param with trait constraints.
#[derive(Debug, Clone, PartialEq)]
pub struct TraitBound {
    pub name: String,        // the combined name (e.g., "a&display")
    pub bounds: Vec<String>, // individual traits via & (e.g., ["a", "display"])
    pub span: Span,
}

/// A type reference (may be parameterized, borrowed, etc.)
#[derive(Debug, Clone, PartialEq)]
pub enum TypeRef {
    /// Simple named type: `U32`, `String`
    Named(String),
    /// Parameterized type: `Vec{T}`
    Parameterized(String, Vec<TypeRef>),
    /// Self type
    SelfType,
    /// Borrowed type: `:String` in struct fields
    Borrowed(Box<TypeRef>),
    /// Trait bound: `{|a&display|}` — constrained generic
    Bound(TraitBound),
}

impl TypeRef {
    pub fn name(&self) -> &str {
        match self {
            TypeRef::Named(n) => n,
            TypeRef::Parameterized(n, _) => n,
            TypeRef::SelfType => "Self",
            TypeRef::Borrowed(inner) => inner.name(),
            TypeRef::Bound(tb) => &tb.name,
        }
    }
}

/// Method body — computed or matching.
#[derive(Debug, Clone, PartialEq)]
pub enum Body {
    /// Stub: `___`
    Stub,
    /// Computed body: `[ stmts ]` — Luna, explicit ^return
    Block(Vec<Spanned<Expr>>),
    /// Matching body: `(| arms |)` — pattern dispatch, each arm returns
    MatchBody(Vec<MatchMethodArm>),
    /// Tail-recursive computed body: `[| stmts |]` — codegen generates loop {}
    TailBlock(Vec<Spanned<Expr>>),
}

/// Expression.
#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    /// Same-type binding: `@Radius.new(Value(5.0))` — name matches existing type
    SameTypeNew(String, Vec<Spanned<Expr>>),
    /// Sub-type binding: `@Area F64.new(expr)` — creates wrapper
    SubTypeNew(String, TypeRef, Vec<Spanned<Expr>>),
    /// Mutable binding: `~@Counter U32.new(0)`
    MutableNew(String, TypeRef, Vec<Spanned<Expr>>),
    /// Mutable set: `~@Counter.set(expr)`
    MutableSet(String, Box<Spanned<Expr>>),
    /// Two-step declaration: `Name Type` (no @, no value yet)
    SubTypeDecl(String, TypeRef),
    /// Two-step allocation: `@Name.new(expr)` (after SubTypeDecl)
    DeferredNew(String, Vec<Spanned<Expr>>),
    /// Instance reference: `@Name`
    InstanceRef(String),
    /// Return: `^expr`
    Return(Box<Spanned<Expr>>),
    /// Field/method access: `expr.name`
    Access(Box<Spanned<Expr>>, String),
    /// Inline match: `(| targets (pattern) result ... |)`
    MatchExpr(MatchExprData),
    /// Inline evaluation: `[expr expr ...]` — Luna brackets
    InlineEval(Vec<Spanned<Expr>>),
    /// Binary op
    BinOp(Box<Spanned<Expr>>, BinOp, Box<Spanned<Expr>>),
    /// Literal integer
    IntLit(i64),
    /// Literal float
    FloatLit(f64),
    /// Literal string
    StringLit(String),
    /// Const reference: `!Name`
    ConstRef(String),
    /// Stub: `___`
    Stub,
    /// Grouped: `(expr)`
    Group(Box<Spanned<Expr>>),
    /// StdOut: `StdOut expr`
    StdOut(Box<Spanned<Expr>>),
    /// Bare PascalCase name (variant, type, etc.)
    BareName(String),
    /// Struct construction: `TypeName(Field(val) Field(val))`
    StructConstruct(String, Vec<(String, Spanned<Expr>)>),
    /// Method call with arguments: `expr.method(args...)`
    MethodCall(Box<Spanned<Expr>>, String, Vec<Spanned<Expr>>),
    /// Comprehension: `[| @Source expr {guard} |]`
    /// ;; STATUS: removed from spec, kept for backward compat (codegen + db tests)
    Comprehension {
        source: Box<Spanned<Expr>>,
        output: Option<Box<Spanned<Expr>>>,
        guard: Option<Box<Spanned<Expr>>>,
    },
    /// Error propagation: `expr?`
    ErrorProp(Box<Spanned<Expr>>),
    /// Range: `start..end` or `start..=end`
    Range {
        start: Box<Spanned<Expr>>,
        end: Box<Spanned<Expr>>,
        inclusive: bool,
    },
    /// Yield: `>expr`
    Yield(Box<Spanned<Expr>>),
}

/// Data for an inline match expression: `(| targets (pattern) result ... |)`
#[derive(Debug, Clone, PartialEq)]
pub struct MatchExprData {
    /// Match targets: `@Instance`, etc.
    pub targets: Vec<Spanned<Expr>>,
    /// Match arms
    pub arms: Vec<MatchArm>,
}

/// Binary operator.
#[derive(Debug, Clone, PartialEq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    Eq,
    Neq,
    Lt,
    Gt,
    Lte,
    Gte,
    And,
    Or,
}

/// Match arm in an inline `(| ... |)` expression.
/// Always commit — inline match doesn't backtrack.
#[derive(Debug, Clone, PartialEq)]
pub struct MatchArm {
    pub patterns: Vec<Pattern>,
    pub body: Vec<Spanned<Expr>>,
    pub span: Span,
}

/// Match arm in a matching method body `(| ... |)`.
/// Supports commit, backtrack, and destructure arms.
#[derive(Debug, Clone, PartialEq)]
pub struct MatchMethodArm {
    pub kind: ArmKind,
    pub patterns: Vec<Pattern>,
    pub body: Vec<Spanned<Expr>>,
    /// Destructure elements + tail binding (only for ArmKind::Destructure)
    pub destructure: Option<(Vec<DestructureElement>, String)>,
    pub span: Span,
}

/// Whether a match arm commits, backtracks, or destructures.
#[derive(Debug, Clone, PartialEq)]
pub enum ArmKind {
    /// `(pattern) result` — first match wins
    Commit,
    /// `[pattern] result` — try, if result fails, next arm
    Backtrack,
    /// `[pattern | @Rest] result` — split sequence, bind tail
    Destructure,
}

/// Pattern in a match arm.
#[derive(Debug, Clone, PartialEq)]
pub enum Pattern {
    /// Variant name: `Fire`
    Variant(String),
    /// Bool literal in pattern: `True` / `False`
    BoolLit(bool),
    /// Wildcard: `_`
    Wildcard,
    /// Or-pattern: `Fire | Air`
    Or(Vec<Pattern>),
    /// Sequence destructure: `(Colon At Self | @Rest)`
    /// head patterns matched in order, tail bound to name
    Destructure {
        head: Vec<Pattern>,
        tail: Option<String>,
    },
    /// Data-carrying variant binding: `Some(@Head)`, `Borrow(@Name)`
    DataCarrying(String, Box<Pattern>),
    /// String literal pattern: `("aries")`
    StringLit(String),
    /// Instance binding in pattern: `@Name` — binds the matched value
    InstanceBind(String),
}

/// Type alias declaration: `ChartResult Result{NatalChart ChartError}`
#[derive(Debug, Clone, PartialEq)]
pub struct TypeAliasDecl {
    pub name: String,
    pub target: TypeRef,
    pub span: Span,
}

/// A single import entry: `ModuleName (Item1 Item2)` or `ModuleName (_)`
#[derive(Debug, Clone, PartialEq)]
pub struct ImportEntry {
    pub module: String,
    pub items: ImportItems,
    pub span: Span,
}

/// What items to import from a module.
#[derive(Debug, Clone, PartialEq)]
pub enum ImportItems {
    /// Specific named items
    Named(Vec<String>),
    /// Wildcard `(_)` — import everything
    Wildcard,
}

/// Module header — first lines of every .aski file
#[derive(Debug, Clone, PartialEq)]
pub struct ModuleHeader {
    /// `()` Sol block: module name (first) + exports (rest)
    pub name: String,
    pub exports: Vec<String>,
    /// `[]` Luna block: imports (optional)
    pub imports: Vec<ImportEntry>,
    /// `{}` Saturn block: compilation constraints (optional)
    pub constraints: Vec<String>,
    pub span: Span,
}

/// A complete parsed source file with header and items.
#[derive(Debug, Clone, PartialEq)]
pub struct SourceFile {
    pub header: Option<ModuleHeader>,
    pub items: Vec<Spanned<Item>>,
}

/// A destructure arm pattern element in `[pattern | @Rest]`.
#[derive(Debug, Clone, PartialEq)]
pub enum DestructureElement {
    /// Match exact variant: `LParen`, `Colon`
    ExactToken(String),
    /// Bind value: `@Name` — calls sub-parser or binds element
    Binding(String),
    /// Wildcard: `_`
    Wildcard,
}
