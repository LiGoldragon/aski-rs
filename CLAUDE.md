# semac — Stage 3: Sema Compiler

Takes Stage 2's typed parse tree. Mechanical tree walk produces
.sema binary (pure ordinals, no strings) + .aski-table.sema
(name projection) + Rust codegen.

## Current State

Not yet implemented for v0.16. The v0.15 engine code is archived
in `src/v015_archive/` as reference for the rewrite.

Reusable infrastructure kept from v0.15:
- `synth/loader.rs` — hardcoded .synth parser
- `synth/types.rs` — Dialect, Rule, Item, Card, Delimiter
- `lexer.rs` — Logos tokenizer
- `engine/tokens.rs` — TokenReader

The previous v0.16 attempt in `synth_compiler/` has hand-written
enums (NodeKind, DialectKind) — these need to be derived from data
instead. That code belongs in synthc, not here.

## Repos

- **synthc** — Stage 1: .synth + .aski → data-tree + derived enums
- **askic** — Stage 2: data-tree + .aski bodies → typed parse tree
- **semac** — Stage 3: parse tree → .sema binary + codegen
- **aski** — language spec (`spec/pipeline.md`)

## Design Spec

`~/git/aski/spec/pipeline.md`

## VCS

Jujutsu (`jj`) mandatory. Object/trait Rust style. Small files.
