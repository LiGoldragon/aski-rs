# aski-rs-bootstrap — Stage1 Bootstrap Compiler

## STATUS: FULL REWRITE IN PROGRESS

The v0.15 engine code is archived in `src/v015_archive/`. Do NOT
modify those files. The new v0.16 three-compiler pipeline is being
implemented from scratch in the new directory structure.

## Architecture

Three compilers, each producing typed rkyv-serializable data:

```
src/
  synth/              — synth loader (KEEP — hardcoded .synth parser)
  lexer.rs            — Logos tokenizer (KEEP)
  engine/tokens.rs    — TokenReader (KEEP)
  synth_compiler/     — Stage 1: .synth + .aski headers → enums + scopes
  aski_compiler/      — Stage 2: SynthOutput + .aski → typed data-tree
  sema_compiler/      — Stage 3: DataTree → .sema + codegen
  v015_archive/       — old v0.15 engine (reference only, do not use)
```

## Design Spec

Read: `~/git/Mentci/components/aski/encoder/design/v0.16/pipeline.md`
(1885 lines — complete spec with concrete Rust types for every structure)

## Key Principles

- Synth IS the grammar — 28 dialect files define everything
- Three stages, all typed, all rkyv-serializable
- Enums not integers — variant IS identity
- Data-tree with owned children — no flat arrays, no i64 IDs
- Scope-enforced — every name resolves to a known enum variant
- No strings in sema — enum discriminants ARE the bytes
- Position defines meaning — no `/` separator

## Branch

`askic-bootstrap` — push after every change.

## VCS

Jujutsu (`jj`) mandatory. Object/trait Rust style. Small files.
