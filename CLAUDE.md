# semac — Sema Generator

The sema generation stage of the sema engine. Mechanical tree walk
over askic's typed parse tree. Produces .sema binary (pure ordinals,
no strings) + .aski-table.sema (name projection) + Rust codegen.

Trusts askic completely — no re-validation.

## Current State

Not yet built for v0.16. The v0.15 engine code is archived
in `v015_archive/` as reference for the rewrite.

## The Sema Engine

```
aski-core  →  askicc  →  askic  →  semac
(anatomy)    (bootstrap)  (compiler)  (sema gen)
```

## VCS

Jujutsu (`jj`) mandatory. Object/trait Rust style. Small files.
