# aski-rs — Rust Backend

Reads aski source, emits Rust code. Sole codegen is codegen_v3.
Bootstrap pipeline green at v0.4.0.4.

## Current State (2026-04-05)

- 86 tests pass (82 unit + 4 integration)
- Only v0.9 spec exists — all pre-v0.8 removed
- Multi-file compilation via `compile_directory` API
- Variant construction `PascalName(expr)` in match arms
- DataCarrying pattern binding `Parsed(@Toks)` extracts inner values
- Auto-deref primitive named params in arithmetic
- `PartialEq`/`Eq` derived on all structs
- camelCase traits → PascalCase Rust conversion
- BindType (PascalCase-only binding for type references)
- Grammar rules are live — injected during parsing, module-scoped via topo-sort
- No serde_json — JSON eliminated, World relations are canonical

## Architecture

```
.aski → Lexer (logos) → Grammar Rules (PEG interpreter) → AST → IR → Codegen v3 → Rust
```

## v0.9 Language Features

- PascalCase = nouns (domains, fields), camelCase = verbs (traits, methods)
- No closures, no guards, no loops, no comprehension, no contracts
- Iteration via collection traits (map, filter, each) + tail recursion
- `#` = yield, `>` = greater-than only
- `{}` destructure arms in matching bodies
- Module headers: `()` identity+exports, `[]` imports, `{}` constraints
- `&` trait combination replaces where clauses
- Actor model: observe borrows (`:@Self`), transform moves (`@Self`)

## Module headers in Kernel

Module headers `() [] {}` are preserved in Kernel Aski. Identical codegen
produces identical Rust, which means identical library fingerprints. Cargo
can reuse compiled artifacts when only dependents change, not the library.

## usize in generated code

Aski's type system is fixed-size: U8, U16, U32, U64, I64, F64.
No platform-dependent types. `usize` appears only as an ephemeral
runtime detail in generated Rust for Vec operations (`.get(x as usize)`,
`.len() as u32`). Never stored, never serialized, never visible to aski.
See src/codegen_v3.rs module doc.

## VCS

Jujutsu (`jj`) mandatory. Git is storage backend only.

## Language Policy

Rust only for application logic. Nix only for builds.
