# semac — The Sema Compiler

semac is the sema backend — the only tool that produces true
sema. It reads rkyv parse trees and produces sema binary +
Rust source.

**Only semac produces sema.** Sema is the universal typed
binary format — no strings, no unsized data, domain variants
as bytes. Everything upstream (askicc, askic) produces rkyv
data that still has strings. semac resolves strings to domain
variants, producing true sema.

semac is permanent. askic (the aski frontend) is one way to
produce rkyv parse trees. Other frontends may exist.

## What semac Produces

1. **.sema** — true sema binary (no strings, fixed-size)
2. **module.rs** — Rust codegen (the compilation target)
3. **module.aski-table.sema** — name projection for tooling

## The Pipeline

```
cc       — .aski → Rust types (bootstrap seed)
askicc   — .synth → rkyv domain-data-tree
askic    — reads rkyv data-tree → dialect state machine → rkyv parse tree
semac    — reads rkyv → produces sema + Rust (this binary)
```

Four separate binaries. semac reads rkyv. No text parsing.
No grammar processing. Just typed binary in, sema + code out.

## Current State

Not yet built for v0.17. The v0.15 engine code is archived
in `v015_archive/` as reference.

Note: v015_archive uses "domain" to mean "enum only." In v0.17,
domain means any data definition (enum + struct + newtype).
See `v015_archive/TERMINOLOGY.md`.

## Rust Style

**No free functions — methods on types always.** All Rust
will eventually be rewritten in aski, which uses methods
(traits + impls). `main` is the only exception.

## VCS

Jujutsu (`jj`) mandatory. Small files. Tests in separate files.
