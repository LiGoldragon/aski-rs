# Stage1 Bootstrap — Completion Design

## Design Intents

### 1. Codegen Reads Only SemaWorld

Currently `codegen.rs` takes `&AskiWorld` alongside `&SemaWorld` because
method bodies are stored as parse-node references (`body_node_id: i64`).
This cross-world reference is wrong — **SemaWorld must be self-contained**.

Codegen must work from SemaWorld alone. This means expressions, statements,
match bodies, and block bodies must be fully lowered into typed SemaWorld
structures. The parse tree is transient; SemaWorld is the artifact.

### 2. Filesystem-Regenerative Module System

SemaWorld must store everything needed to **completely regenerate the
filesystem from which the data came**. This means:

- Which files exist, their paths, their module names
- Which declarations belong to which file
- Export lists per module
- Import lists per module (module name + imported names)
- Declaration ordering within each file

If you serialize SemaWorld and deserialize it on an empty disk, you must
be able to reconstruct every `.aski` and `.main` file exactly.

### 3. Complete Stage1 Before Stage2

Stage2 (self-hosting compiler in aski) will use features that stage1
must handle: multi-param methods, allocation, mutation, match expressions
with targets, iteration. All of these must work in stage1 first.

## SemaExpr — Expression Tree

Replace `body_node_id: i64` with a fully-lowered expression tree.

```rust
pub enum SemaExpr {
    IntLit(i64),
    FloatLit(String),
    StringLit(String),
    SelfRef,                    // @Self → self
    InstanceRef(String),        // @name → local binding
    QualifiedVariant {          // Fire → Element::Fire
        domain: i64,            //   ordinal into type_names
        variant: i64,           //   ordinal into variant_names
    },
    BareName(String),           // unresolved name
    TypePath(String),           // Type/Variant → Type::Variant
    BinOp {
        op: String,
        lhs: Box<SemaExpr>,
        rhs: Box<SemaExpr>,
    },
    FieldAccess {
        object: Box<SemaExpr>,
        field: String,
    },
    MethodCall {
        object: Box<SemaExpr>,
        method: String,
        args: Vec<SemaExpr>,
    },
    Group(Box<SemaExpr>),       // (expr) — parenthesized
    Return(Box<SemaExpr>),      // ^expr
    InlineEval(Vec<SemaStatement>), // [stmts]
    MatchExpr {                 // (| target/ arms |)
        target: Option<Box<SemaExpr>>,
        arms: Vec<SemaMatchArm>,
    },
    StructConstruct {           // (Type/ (Field/ val) ...)
        type_name: String,
        fields: Vec<(String, SemaExpr)>,
    },
}

pub enum SemaStatement {
    Expr(SemaExpr),
    Allocation {                // @name (Type/ args) or @name :Type
        name: String,
        typ: Option<String>,
        init: Option<SemaExpr>,
    },
    MutAllocation {             // ~@name (Type/ args)
        name: String,
        typ: Option<String>,
        init: Option<SemaExpr>,
    },
    Mutation {                  // ~@name.set [expr]
        target: String,
        method: String,
        args: Vec<SemaExpr>,
    },
    Iteration {                 // #source [body]
        source: SemaExpr,
        body: Vec<SemaStatement>,
    },
}

pub enum SemaBody {
    Block(Vec<SemaStatement>),
    MatchBody {
        target: Option<SemaExpr>,
        arms: Vec<SemaMatchArm>,
    },
}

pub struct SemaMatchArm {
    pub patterns: Vec<SemaPattern>,
    pub result: SemaExpr,
}

pub enum SemaPattern {
    Variant(i64),               // variant name ordinal
    Or(Vec<SemaPattern>),       // (Fire | Air) → Or([Fire, Air])
}
```

SemaMethod changes:
```rust
pub struct SemaMethod {
    pub name: i64,
    pub params: Vec<SemaParam>,
    pub return_type: String,
    pub body: SemaBody,         // was: body_node_id: i64
}
```

SemaConst changes:
```rust
pub struct SemaConst {
    pub name: String,
    pub typ: String,
    pub value: SemaExpr,        // was: value_node_id: i64
}
```

## Module System

```rust
pub struct SemaModule {
    pub name: i64,              // ordinal into module_names
    pub file_path: String,      // original filesystem path
    pub is_main: bool,          // .main vs .aski
    pub exports: Vec<i64>,      // ordinals into type/trait/method names
    pub imports: Vec<SemaImport>,
    pub declarations: Vec<i64>, // ordered list of declaration indices
}

pub struct SemaImport {
    pub module_name: i64,       // ordinal into module_names
    pub names: Vec<i64>,        // ordinals of imported names
}
```

SemaWorld additions:
```rust
pub struct SemaWorld {
    // ... existing ...
    pub modules: Vec<SemaModule>,
    pub module_names: Vec<String>,
}
```

Each declaration (SemaType, SemaTraitDecl, SemaTraitImpl, SemaConst, SemaFfi)
gets a `module_id: i64` field linking it to its source module.

## Param Extraction

Currently `params: Vec::new()` everywhere. Lower must extract:

- `BorrowParam("Self")` → `SemaParam { name: "self", typ: "Self", borrow: Immutable }`
- `MutBorrowParam("Self")` → `SemaParam { name: "self", typ: "Self", borrow: Mutable }`
- `NamedParam("factor")` + child `TypeRef("U32")` → `SemaParam { name: "factor", typ: "U32", borrow: Owned }`
- `OwnedParam("self")` → `SemaParam { name: "self", typ: "Self", borrow: Owned }`

## Data-Carrying Variants

domain.synth already has rules for `*(@Variant/ :Type)` and
`*{@Variant/ <struct>}`. Lower must handle:

- `(@Variant/ :Type)` → `SemaVariant { wraps: type_id }` (tuple variant)
- `{@Variant/ <struct>}` → separate SemaType(Struct) + SemaVariant that wraps it

Codegen emits:
- `Variant(InnerType)` for tuple variants
- `Variant { fields }` for struct variants

## Statement Parsing

parse_statement currently falls through to parse_expr for everything.
Must handle:

1. `@name :Type` → Allocation (sub-type declaration)
2. `@name (Type/ args)` → Allocation with constructor
3. `~@name :Type` → MutAllocation
4. `~@name.set [expr]` → Mutation via set
5. `~@name.method (args)` → Mutation via method
6. `^expr` → Return (already works)
7. `#source [body]` → Iteration

## Match Enhancements

1. **Or-patterns**: `(Fire | Air)` → check for `|` in pattern parsing
2. **Target expression**: `(| @Idx == 0/ arms |)` → parse expr until `/`

## Implementation Order

1. SemaExpr types in sema_world.rs
2. Expression lowering in lower.rs (walk parse nodes → SemaExpr)
3. Update codegen.rs to read SemaExpr only (drop &AskiWorld param)
4. Param extraction in lower.rs
5. Statement parsing in parse_expr.rs
6. Statement lowering into SemaStatement
7. Match enhancements (or-patterns, target)
8. Constants + FFI completion
9. Data-carrying variants
10. Module system (SemaModule, file tracking, imports)
11. Update raise.rs for SemaExpr
12. Nix tests for each feature

## Test Strategy

Each feature gets a nix test that:
1. Compiles `.aski` source to Rust via askic
2. Compiles the generated Rust with rustc
3. Verifies output (sema dump or roundtrip)

Test files (in addition to existing elements.aski, math.aski):
- `variants.aski` — data-carrying variants
- `params.aski` — multi-param methods, mutable self
- `statements.aski` — allocation, mutation, iteration
- `matching.aski` — or-patterns, match-with-target
- `constants.aski` — constants + FFI
- `modules.aski` — multi-file with imports/exports
