# semac — Bootstrap Compiler (Rust)

## STATUS: FULL REWRITE IN PROGRESS

The v0.15 engine code is archived in `src/v015_archive/`. Do NOT
modify those files — they are reference for the rewrite.

## Architecture

Three compilers, each producing typed rkyv-serializable data:

```
src/
  synth/              — synth loader (KEEP — hardcoded .synth parser)
  lexer.rs            — Logos tokenizer (KEEP)
  engine/tokens.rs    — TokenReader (KEEP)
  synth_compiler/     — Stage 1: .synth + .aski → data-tree + derived enums
  aski_compiler/      — Stage 2: data-tree + .aski → typed parse tree
  sema_compiler/      — Stage 3: parse tree → .sema binary + codegen
  v015_archive/       — old v0.15 engine (reference only)
```

## Repos

- **synthc** (`~/git/synthc`) — 28 synth dialect files + examples
- **askic** (`~/git/askic`) — compiler binary (empty until bootstrap works)
- **semac** (`~/git/semac`) — this repo, the bootstrap compiler in Rust
- **aski** (`~/git/aski`) — language spec (`spec/pipeline.md`)

## Design Spec

Read: `~/git/aski/spec/pipeline.md`

## Key Principles

- Synth IS the grammar — 28 dialect files define everything
- Enums are derived from the data — not hand-written
- Three stages, all typed, all rkyv-serializable
- Data-tree with owned children — no flat arrays, no i64 IDs
- No strings in sema — enum discriminants ARE the bytes

## VCS

Jujutsu (`jj`) mandatory. Object/trait Rust style. Small files.
