# Stage1 Bootstrap — Design

## Sema Is the Artifact

The compiler's primary output is `.sema` — a pure binary file with zero
strings. Domain ordinals ARE the bytes. All typed relations, expression
trees, and module structures are stored as ordinals into name tables
that live in a separate file.

Two files per module:
```
module.sema              — code sema (pure ordinals, rkyv binary)
module.aski-table.sema   — aski name table (ordinal → name, rkyv binary)
```

The code sema is language-agnostic structure. The aski-table is one
possible name projection. Other tables could exist for different targets
(rust-table, display-table, etc.).

## Pipeline

```
.aski source
  ↓ askic compile
.sema + .aski-table.sema        ← THE ARTIFACT (two files)
  ↓ askic rust .sema
.rs source                       ← one possible projection
  ↓ rustc
binary executable
```

Every step is independently verifiable:
- `askic compile` — aski → sema (parsing + lowering)
- `askic rust` — sema → Rust (codegen from binary, not from memory)
- `askic deparse` — sema → aski (raise + deparse, proves losslessness)
- `askic roundtrip` — aski → sema → aski (full cycle)

## Typed Ordinals

Every name domain has its own newtype. Can't mix a TypeName with a
VariantName — the compiler catches it at compile time.

```rust
TypeName(u32)       — domain/struct/alias names
VariantName(u32)    — enum variant names
FieldName(u32)      — struct field names
TraitName(u32)      — trait names
MethodName(u32)     — method/param names
ModuleName(u32)     — module names
StringLiteral(u32)  — interned string literals
BindingName(u32)    — local binding names (method-scoped)
ExprRef(u32)        — index into expression arena
StmtRef(u32)        — index into statement arena
BodyRef(u32)        — index into body arena
```

## No Strings in Sema

The `.sema` binary contains zero strings. The `sema_binary_contains_no_strings`
test greps the binary for all known names and asserts none appear.

Names live in `.aski-table.sema` (the aski projection). When codegen
needs "Element", it reads from the name table via `ResolveName` trait.

The `Operator` enum is a fixed domain with 12 variants — no strings needed.

## Expression Arena

No Box, no recursion. All expressions stored in a flat arena, referenced
by `ExprRef(u32)`. Same for statements (`StmtRef`) and bodies (`BodyRef`).

This eliminates rkyv recursive type issues and is more sema-like:
everything is ordinals and indices.

```rust
struct ExprArena {
    exprs: Vec<SemaExpr>,      // indexed by ExprRef
    stmts: Vec<SemaStatement>, // indexed by StmtRef
    bodies: Vec<SemaBody>,     // indexed by BodyRef
    match_arms: Vec<SemaMatchArm>,
}
```

## Name Enums in Generated Code

Codegen emits name enums with Display impls:
```rust
pub enum TypeName { Element, Quality }
pub enum VariantName { Fire, Earth, Air, Water, Passionate, ... }
pub enum FieldName { Left, Right }
pub enum TraitName { Describe, Compute }
pub enum MethodName { Describe, Add }
```

These are generated from the NameInterner tables during compilation.
The enum variants carry their own names via Display — no separate
name table needed at runtime.

## Module System

Module header uses `{}` with camelCase key:
```aski
{elements/ Element Quality describe}
```

Disambiguated from structs (PascalCase) by casing. `!` cardinality
in synth means exactly one per file.

SemaModule stores: name ordinal, is_main flag, declaration order.
Exports live in `.aski-table.sema` (aski-level data, not sema).

## Synth Cardinality on Ordered Choice

Each `//` alternative has explicit cardinality:
```
// !{@module/ <module>}     — exactly one
// *(@Domain/ <domain>)     — zero or more
// ?[|<process>|]           — at most one
```

The engine tracks match counts per alternative and enforces limits.

## Type:path Qualified Names

`:` separates qualified paths in expressions:
```aski
Element:Fire    → Element::Fire in Rust
Type:new        → Type::new() in Rust
```

`/` is reserved for key separator inside delimiters only.

## File Structure (5256 lines, 26 tests)

```
src/
  lexer.rs              419  Logos tokenizer
  synth/
    types.rs            120  Dialect, Rule, ChoiceAlternative, Item
    loader.rs           495  hardcoded synth parser, @value, ! cardinality
  engine/
    aski_world.rs       285  AskiWorld: dialect stack, names, parse nodes
    sema.rs             461  Sema + typed ordinals + ExprArena + AskiNameTable
    tokens.rs           176  TokenReader cursor
    register.rs          37  Register trait
    parse.rs            169  Parse trait (entry points + 7 tests)
    parse_item.rs       277  ParseItem trait (delimiter dispatch + @value)
    parse_dialect.rs    199  ParseDialect trait (cardinality tracking)
    parse_expr.rs       486  ParseExpr (Pratt + statements + match)
    lower.rs            594  Lower + LowerExpr (AskiWorld → Sema)
    deparse.rs          289  Deparse (AskiWorld → aski text, full sigils)
    raise.rs            317  Raise (Sema → AskiWorld, full roundtrip)
    codegen.rs          472  Codegen (Sema → Rust, name enums, from file)
    compiler.rs         101  Compiler (multi-file, import resolution)
    sema_tests.rs       160  4 sema tests (no-strings, binary/codegen/aski roundtrip)
  bin/askic.rs          199  CLI: compile, rust, deparse, roundtrip
```
