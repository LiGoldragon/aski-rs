# semac — Sema Generator

Takes askic's typed parse tree. Mechanical tree walk produces
.sema binary (pure ordinals, no strings) + .aski-table.sema
(name projection) + Rust codegen.

## Current State

Not yet implemented for v0.16. The v0.15 engine code is archived
in `v015_archive/` as reference for the rewrite.

## Repos

- **askicc** — bootstrap: .synth grammar + askic's .aski anatomy → data-tree
- **askic** — compiler: data-tree + .aski bodies → typed parse tree
- **semac** — sema generator: parse tree → .sema binary + codegen
- **aski** — language spec (`spec/pipeline.md`)

## VCS

Jujutsu (`jj`) mandatory. Object/trait Rust style. Small files.
