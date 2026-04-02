# aski-rs — Rust Backend

Reads **Kernel Aski** (the simplified, macro-expanded subset of aski).
Emits Rust code. Deliberately simple — gets thinner as aski-cc takes over.

## Architecture

```
Kernel Aski → Lexer (logos) → Parser (chumsky) → CozoDB → Codegen → Rust
```

## Module headers in Kernel

Module headers `() [] {}` are preserved in Kernel Aski. Identical codegen
produces identical Rust, which means identical library fingerprints. Cargo
can reuse compiled artifacts when only dependents change, not the library.
This is why module headers survive macro expansion — build caching.

## usize in generated code

Aski's type system is fixed-size: U8, U16, U32, U64, I64, F64.
No platform-dependent types. `usize` appears only as an ephemeral
runtime detail in generated Rust for Vec operations (`.get(x as usize)`,
`.len() as u32`). Never stored, never serialized, never visible to aski.
See src/codegen.rs module doc.

## VCS

Jujutsu (`jj`) mandatory. Git is storage backend only.

## Language Policy

Rust only for application logic. Nix only for builds.
