# Stage1 Bootstrap — Design

## Sema Is the Artifact

The compiler's primary output is `.sema` — pure binary, zero strings.
Domain variants ARE the identity. The enum discriminant is the byte
representation, but the CONCEPT is the variant — `TypeName::Element`,
not `TypeName(0)`.

Two files per module:
```
module.sema              — code sema (enum discriminants, rkyv binary)
module.aski-table.sema   — aski name table (variant → name, rkyv binary)
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

## Name Enums

Every name domain is a generated enum. The variants carry identity.
Can't mix a TypeName with a VariantName — they're different types.

```rust
// In generated Rust output:
enum TypeName { Element, Quality, Point }
enum VariantName { Fire, Earth, Air, Water, Passionate, ... }
enum FieldName { Left, Right, Horizontal, Vertical }
enum TraitName { Describe, Compute }
enum MethodName { Describe, Add, Multiply }

// In the bootstrap (u32 storage, resolved via NameInterner):
struct TypeName(u32);      // discriminant of the generated enum
struct VariantName(u32);
struct FieldName(u32);
// etc.

// Arena indices (not enums — these are positions, not identities):
struct ExprRef(u32);
struct StmtRef(u32);
struct BodyRef(u32);
```

The bootstrap uses `u32` as the machine representation of enum
discriminants. The generated Rust code has actual enums with Display.
The sema binary stores the discriminant. All three represent the
same thing: the variant IS the identity.

## No Strings in Sema

The `.sema` binary contains zero strings. Verified by test.

Names live in `.aski-table.sema` (the aski name projection). When
codegen needs the string "Element", it resolves the `TypeName::Element`
variant through the name table via `ResolveName` trait.

`Operator` is a fixed enum — `Add`, `Sub`, `Mul`, etc. — no strings.

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
