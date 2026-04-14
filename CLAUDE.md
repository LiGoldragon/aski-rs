# aski-rs-bootstrap — Stage1 Bootstrap Compiler

Synth-driven compiler. Reads .aski source, produces .sema (pure binary)
+ .aski-table.sema (name projection). Codegen emits Rust from .sema files.

## Branch

**`askic-bootstrap`** — not main. Main has old v0.4 code.

## Current State (2026-04-14)

- v0.16 syntax (no / separator, positional dialects)
- ~5500 lines, 26 tests, 3 nix checks
- Sema is the artifact: .sema (zero strings) + .aski-table.sema (names)
- Name enums: TypeName, VariantName, etc. — generated enums, not integers
- Flat ExprArena: ExprRef/StmtRef/BodyRef — no Box recursion
- rkyv portable: little_endian, pointer_width_32, alloc
- Name enums generated: TypeName, VariantName, FieldName, etc. with Display

## Pipeline

```
.aski → askic compile → .sema + .aski-table.sema
                              ↓
        askic rust .sema → .rs (from binary, not memory)
```

## CLI

- `askic compile file.aski --synth-dir path` → .sema + .aski-table.sema
- `askic rust file.sema` → Rust (auto-discovers .aski-table.sema)
- `askic rust file.aski --synth-dir path` → shorthand: compile + rust
- `askic deparse file.sema` → aski text from sema
- `askic roundtrip file.aski` → aski → sema binary → aski

## Architecture

```
.aski → Lexer (logos) → Synth-driven parser (dialect tables)
      → AskiWorld (parse nodes) → Lower → Sema + NameInterner
      → rkyv serialize → .sema + .aski-table.sema
      → rkyv deserialize → Codegen → Rust with name enums
```

## VCS

Jujutsu (`jj`) mandatory. Branch: `askic-bootstrap`. Push after every change.

## Language Policy

Rust only. Nix only for builds. Object/trait style — no free functions.
