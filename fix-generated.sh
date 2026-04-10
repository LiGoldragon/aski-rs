#!/bin/sh
# Minimal fixes for aski-generated Rust.
# 3 issues remaining — all String ownership edge cases.
set -e
sed -i 's/type_entry_by_name(elem_type)/type_entry_by_name(\&elem_type)/g' src/codegen_gen.rs
sed -i 's/s5\.add_ffi(library, aski_name, rust_name/s5.add_ffi(library.clone(), aski_name.clone(), rust_name.clone()/' src/parser_gen.rs
sed -i 's/self\.tokens\.from_ordinal(self\.pos)\.text }/self.tokens.from_ordinal(self.pos).text.clone() }/g' src/parser_gen.rs
echo "Fixed (3 ownership edge cases)"
