# Architecture — semac

semac is the sema backend — the only tool that produces true sema.
It reads rkyv parse trees (from askic) verified-linked by veric and
produces sema binary + Rust source (via rsc). Sema is the universal
typed binary format — no strings, no unsized data, domain variants
as bytes.

## Role

- Reads `program.rkyv` (from veric) + domain types.
- Resolves strings to domain variants, producing true sema.
- Emits `.sema` (typed binary), `.rs` (Rust projection via rsc), and
  `.aski-table.sema` (name projection for tooling).

semac is permanent. askic (the aski frontend) is one way to produce
rkyv parse trees; other frontends may exist.

## Pipeline position

```
corec       .core → Rust with rkyv derives (bootstrap seed)
synth-core  grammar contract types
aski-core   parse tree contract types
veri-core   veric output contract
askicc      source/<surface>/*.synth → dsls.rkyv
askic       source + dsls.rkyv → per-module rkyv parse tree
veric       per-module rkyv → program.rkyv (verified, linked)
domainc     program.rkyv → Rust domain types (proc macro)
semac       program.rkyv + domain types → .sema
rsc         .sema + domain types → .rs (Rust projection)
```

semac reads rkyv. No text parsing. No grammar processing. Typed
binary in, sema + code out.

## Status

**STALE / NOT BUILT FOR v0.20.** Current repo holds only the
historical `v015_archive/` (deleted in main per recent jj log) +
this architecture description. Two major redesigns separate the
v0.15 archive from the v0.20 contracts; new implementation waits
on veri-core's D6 redesign + veric's port.

## Cross-cutting context

- Project-wide aski/sema ecosystem; distinct from the
  `signal-frame` / `sema-engine` / `signal-sema` substrate that
  workspace components consume today.
- `~/primary/skills/component-triad.md` for the workspace's
  current daemon + signal triad discipline.

## Macro-pattern integration

**Status:** integrated into the brilliant macro library pattern per `reports/designer/326-v13-spirit-complete-schema-vision.md §3` (schemas as macro-pattern instance).

**Role:** this crate is the sema-binary backend of the aski/sema pipeline. It is upstream of the workspace's current `signal-frame` / `sema-engine` substrate in conceptual terms (typed binary value language), but downstream of veric / domainc in the aski toolchain.

**Integration target:** sema-binary backend; potential future provider of the `.schema` ↔ sema-binary relation. Today's schema-engine upgrade (per `/326-v13`) uses a Rust proc-macro reading `.schema` files; if/when the aski/sema toolchain stabilises against v0.20, semac is the natural producer of the typed binary form a future schema-engine could consume directly. For the current MVP, semac is OUT of the critical path; the marking is forward-looking.

**Per-component concerns:** semac is stale (no v0.20 implementation yet). The schema-engine upgrade does not block on semac. When the aski toolchain catches up to v0.20, the schema-engine MAY converge with semac's output format — but that's a separate, later coordination, not a near-term sequencing item.

**References:**
- `reports/designer/326-v13-spirit-complete-schema-vision.md` — schema language + macro pattern
- `reports/designer/324-migration-mvp-spirit-handover-re-specification.md` — migration MVP
- `reports/operator/174-schema-import-header-design-critique-2026-05-24.md` — lowering + AssembledSchema form
