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
askicc      — source/<surface>/*.synth → dsls.rkyv (domain-data-tree, all 5 DSLs)
askic       — reads source + dsls.rkyv → per-module rkyv parse tree (domain-data-tree)
veric       — per-module rkyv → program.rkyv (verified, linked)
domainc     — program.rkyv → Rust domain types (proc macro)
semac       — program.rkyv + domain types → .sema (this binary)
rsc         — .sema + domain types → .rs (Rust projection)
```

semac reads rkyv. No text parsing. No grammar processing.
Just typed binary in, sema + code out.

## ⚠️ STATUS: STALE / NOT BUILT FOR v0.20

**Current state:** not implemented against v0.20. Repo holds
only CLAUDE.md + the v0.15 archive (`v015_archive/`) as
historical reference.

### Why it's stale

1. `v015_archive/` is from aski v0.15 — **two major redesigns old**.
   Uses "domain" to mean "enum only" (pre-v0.18 narrow sense).
   Type names and shapes have nothing in common with aski-core v0.20.
2. No working code against the current contracts.
3. Waiting on veri-core's D6 redesign + veric's port — semac reads
   program.rkyv, which doesn't exist in the current shape yet.

### How semac will land (dependency chain)

1. askic-assemble exists (v0.20)
2. askic rewritten against askic-assemble
3. veri-core D6 redesign — new program.core shape with EntityRef
4. veric ported to v0.20 aski-core + v0.20 veri-core
5. domainc implemented against veri-core
6. **THEN semac can be implemented** — reads program.rkyv + domain
   types, emits .sema (true binary) + .aski-table.sema + feeds rsc
   for .rs emission

### Note on terminology

v015_archive uses "domain" to mean "enum only." In v0.19 / v0.20,
**domain means any data definition** (enum + struct + newtype).
See `v015_archive/TERMINOLOGY.md`. Do not carry the v0.15 narrow
sense into new code.

## Rust Style

**No free functions — methods on types always.** All Rust will
eventually be rewritten in aski, which uses methods (traits +
impls). `main` is the only exception.

## VCS

Jujutsu (`jj`) mandatory. Small files. Tests in separate files.
