# semac — The Sema Compiler

semac is the sema backend. It reads .sema binary and compiles
it to executable form (currently Rust source).

It reads the universal typed binary format and produces output.
Any tool that produces valid .sema can feed semac.

semac is permanent. askic (the aski frontend) is one way to
produce .sema files. Other frontends may exist in the future.

## What semac Produces

1. **module.rs** — Rust codegen (the compilation target)
2. **module.aski-table.sema** — name projection for tooling

## Current State

Not yet built for v0.17. The v0.15 engine code is archived
in `v015_archive/` as reference.

Note: v015_archive uses "domain" to mean "enum only." In v0.17,
domain means any data definition (enum + struct + newtype).
See `v015_archive/TERMINOLOGY.md`.

## Architecture

```
.sema → semac → .rs
```

semac reads .sema binary. No text parsing. No grammar
processing. Just typed binary in, executable code out.

## VCS

Jujutsu (`jj`) mandatory. Object/trait Rust style. Small files.
Tests in separate files.
