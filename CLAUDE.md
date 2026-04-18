# semac — The Sema Compiler

semac is the sema backend — the only tool that produces true
sema. It reads rkyv parse trees (from askic) verified-linked
by veric and produces sema binary + Rust source (via rsc).

**Only semac produces sema.** Sema is the universal typed
binary format — no strings, no unsized data, domain variants
as bytes. Everything upstream (askicc, askic, veric) produces
rkyv data that still has strings. semac resolves strings to
domain variants, producing true sema.

semac is permanent. askic (the aski frontend) is one way to
produce rkyv parse trees. Other frontends may exist.

## What semac Produces

1. **.sema** — true sema binary (no strings, fixed-size)
2. **.rs** — Rust codegen via rsc (the compilation target)
3. **.aski-table.sema** — name projection for tooling

## The Pipeline

```
corec       — .core → Rust with rkyv derives (bootstrap seed)
synth-core  — grammar contract types (askicc↔askic)
aski-core   — parse tree contract types (askic↔veric↔semac)
veri-core   — veric output contract (Program, ResolutionTable)
askicc      — source/<surface>/*.synth → dsls.rkyv (dsl tree, all 4 DSLs)
askic       — reads source + dsls.rkyv → per-module rkyv parse tree
veric       — per-module rkyv → program.rkyv (verified, linked)
domainc     — program.rkyv → Rust domain types (proc macro)
semac       — program.rkyv + domain types → .sema (this binary)
rsc         — .sema + domain types → .rs (Rust projection)
```

semac reads rkyv. No text parsing. No grammar processing.
Just typed binary in, sema + code out.

## Current State

Not yet built for v0.18. The v0.15 engine code is archived
in `v015_archive/` as reference.

Note: v015_archive uses "domain" to mean "enum only." In v0.18,
domain means any data definition (enum + struct + newtype).
See `v015_archive/TERMINOLOGY.md`.

## Rust Style

**No free functions — methods on types always.** All Rust will
eventually be rewritten in aski, which uses methods (traits +
impls). `main` is the only exception.

## VCS

Jujutsu (`jj`) mandatory. Small files. Tests in separate files.
