#!/bin/sh
# Fix aski-generated Rust for compilation
# Run after regenerating parser_gen.rs and codegen_gen.rs

for f in src/parser_gen.rs src/codegen_gen.rs; do
  # Add imports
  case "$f" in
    *parser*) sed -i '1s/^/use crate::helpers::{StringExt, VecExt, ToI64};\n/' "$f" ;;
    *codegen*) sed -i '1s/^/use crate::helpers::StringExt;\n/' "$f" ;;
  esac

  # String::new() for empty strings
  sed -i 's/let mut \([a-z_]*\): String = ""/let mut \1: String = String::new()/g' "$f"

  # &str fixes: method returns
  sed -i 's/+ \([a-z_]*\.[a-z_]*\.to_snake()\)/+ \&\1/g' "$f"
  sed -i 's/+ \([a-z_]*\.[a-z_]*\.to_rust_type()\)/+ \&\1/g' "$f"
  sed -i 's/+ \([a-z_]*\.[a-z_]*\.to_param_type()\)/+ \&\1/g' "$f"
  sed -i 's/+ \([a-z_]*\.[a-z_]*\.strip_vec()\)/+ \&\1/g' "$f"
  sed -i 's/+ \([a-z_]*\.[a-z_]*\.all_fields_copy()\)/+ \&\1/g' "$f"
  sed -i 's/+ \([a-z_]*\.[a-z_]*\.needs_pascal_alias()\)/+ \&\1/g' "$f"
  sed -i 's/+ \(elem_type\.to_snake()\)/+ \&\1/g' "$f"
  sed -i 's/+ \(elem_type\))/+ \&\1)/g' "$f"

  # &str fixes: field access
  sed -i 's/+ type_entry\.name)/+ \&type_entry.name)/g' "$f"
  sed -i 's/+ variant_def\.name)/+ \&variant_def.name)/g' "$f"
  sed -i 's/+ field_def\.name)/+ \&field_def.name)/g' "$f"
  sed -i 's/+ snake)/+ \&snake)/g' "$f"
  sed -i 's/+ field_types)/+ \&field_types)/g' "$f"
  sed -i 's/+ elem_field_def\.name)/+ \&elem_field_def.name)/g' "$f"
  sed -i 's/+ field_def\.field_type)/+ \&field_def.field_type)/g' "$f"
  sed -i 's/+ field_types\.all_fields_copy())/+ \&field_types.all_fields_copy())/g' "$f"

  # Self method returns → &
  sed -i 's/(out + self\.\([a-z_]*\)())/\(out + \&self.\1()\)/g' "$f"

  # Accessor param fixes
  sed -i 's/by_form(&TypeForm/by_form(TypeForm/g' "$f"
  sed -i 's/by_type_id(&type_entry\.id/by_type_id(type_entry.id/g' "$f"
  sed -i 's/by_type_id(&elem_type_entry\.id/by_type_id(elem_type_entry.id/g' "$f"
  sed -i 's/by_name(&"World"/by_name("World"/g' "$f"
done

# Parser-specific fixes
f=src/parser_gen.rs
sed -i 's/skip_balanced_parens(1)/skip_balanced_parens(\&1)/g' "$f"
sed -i 's/skip_balanced_brackets(1)/skip_balanced_brackets(\&1)/g' "$f"
sed -i 's/type_id: type_id/type_id: *type_id/g' "$f"
sed -i 's/ordinal: ordinal/ordinal: *ordinal/g' "$f"
sed -i 's/\*self\.next_id/self.next_id/g' "$f"
sed -i 's/(depth == 1)/(*depth == 1)/g' "$f"
sed -i 's/(depth - 1)/(*depth - 1)/g' "$f"
sed -i 's/+ inner_type)/+ \&inner_type)/g' "$f"
sed -i 's/form: form/form: *form/g' "$f"
sed -i 's/name: name,/name: name.clone(),/g' "$f"
sed -i 's/name: v_name\b/name: v_name.clone()/g' "$f"
sed -i 's/name: f_name\b/name: f_name.clone()/g' "$f"
sed -i 's/field_type: f_type\b/field_type: f_type.clone()/g' "$f"
sed -i 's/field_type: full_type\b/field_type: full_type.clone()/g' "$f"
sed -i 's/contains_type: ""/contains_type: String::new()/g' "$f"
sed -i 's/self\.pos as usize) >= (self\.tokens\.len() as u32)\.to_i64()/(self.pos as usize) >= self.tokens.len()/g' "$f"
sed -i 's/true => ""/true => String::new()/g' "$f"
sed -i 's/self\.tokens\.from_ordinal(&self\.pos)\.text }/self.tokens.from_ordinal(\&self.pos).text.clone() }/g' "$f"

# FFI-specific fixes
sed -i 's/span: span/span: *span/g' "$f"
sed -i 's/library: library/library: library.clone()/g' "$f"
sed -i 's/aski_name: aski_name/aski_name: aski_name.clone()/g' "$f"
sed -i 's/rust_name: rust_name/rust_name: rust_name.clone()/g' "$f"
sed -i 's/return_type: ret_type/return_type: ret_type.clone()/g' "$f"

echo "Fixed generated Rust"
