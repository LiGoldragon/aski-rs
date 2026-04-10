#!/bin/sh
# Fix aski-generated Rust for compilation.
# Compensates for bootstrap codegen limitations around Rust ownership.
# Goal: eliminate this script by fixing the bootstrap.

set -e

# ── Imports (bootstrap now auto-emits, just ensure no duplicates) ──
sed -i '/^use crate::helpers/d' src/codegen_gen.rs src/parser_gen.rs
sed -i '1s/^/use crate::helpers::{StringExt, VecExt, ToI64, WithPush};\n/' src/codegen_gen.rs
sed -i '1s/^/use crate::helpers::{StringExt, VecExt, ToI64, WithPush};\n/' src/parser_gen.rs

# ── rkyv derives ──
for f in src/codegen_gen.rs src/parser_gen.rs; do
  sed -i 's/#\[derive(Debug, Clone, Copy, PartialEq, Eq)\]/#[derive(Debug, Clone, Copy, PartialEq, Eq, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]/g' "$f"
  sed -i 's/#\[derive(Debug, Clone, PartialEq, Eq)\]/#[derive(Debug, Clone, PartialEq, Eq, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]/g' "$f"
  sed -i 's/#\[derive(Debug, Copy, Clone, PartialEq, Eq)\]/#[derive(Debug, Copy, Clone, PartialEq, Eq, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]/g' "$f"
done

# ── Accessor param: &TypeForm → TypeForm, &i64 → i64 ──
for f in src/codegen_gen.rs src/parser_gen.rs; do
  sed -i 's/by_form(&TypeForm/by_form(TypeForm/g' "$f"
  sed -i 's/by_type_id(&type_entry\.id/by_type_id(type_entry.id/g' "$f"
  sed -i 's/by_type_id(&elem_type_entry\.id/by_type_id(elem_type_entry.id/g' "$f"
done

# ── Codegen-specific ──
sed -i 's/type_entry_by_name(&elem_type)/type_entry_by_name(\&elem_type)/g' src/codegen_gen.rs

# ── Parser: to_world clone ──
sed -i 's/CodeWorld { types: self\.types, variants: self\.variants, fields: self\.fields, ffi_entries: self\.ffi_entries }/CodeWorld { types: self.types.clone(), variants: self.variants.clone(), fields: self.fields.clone(), ffi_entries: self.ffi_entries.clone() }/' src/parser_gen.rs

# ── Parser: withPush no & ──
sed -i 's/\.with_push(&/\.with_push(/g' src/parser_gen.rs

# ── Parser: &i64 comparisons ──
sed -i 's/(\*depth == 1)/(*depth == 1)/g; s/(\*depth - 1)/(*depth - 1)/g' src/parser_gen.rs
sed -i 's/(depth == 1)/(*depth == 1)/g; s/(depth - 1)/(*depth - 1)/g' src/parser_gen.rs
sed -i 's/self\.pos as usize) >= (self\.tokens\.len() as u32)\.to_i64()/(self.pos as usize) >= self.tokens.len()/g' src/parser_gen.rs

# ── Parser: String ownership ──
sed -i 's/self\.tokens\.from_ordinal(&self\.pos)\.text }/self.tokens.from_ordinal(\&self.pos).text.clone() }/g' src/parser_gen.rs
sed -i 's/true => ""/true => String::new()/g' src/parser_gen.rs
sed -i 's/s5\.add_ffi(&library, &aski_name, &rust_name/s5.add_ffi(library.clone(), aski_name.clone(), rust_name.clone()/' src/parser_gen.rs

echo "Fixed generated Rust"
