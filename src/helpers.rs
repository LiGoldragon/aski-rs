//! Helper functions called by aski-generated code.
//! to_snake, to_rust_type, strip_vec, from_ordinal, Vec helpers.

/// Vec indexing by i64 (used by aski's fromOrdinal).
pub trait VecExt<T> {
    fn from_ordinal(&self, idx: i64) -> &T;
}

impl<T> VecExt<T> for Vec<T> {
    fn from_ordinal(&self, idx: i64) -> &T {
        &self[idx as usize]
    }
}

/// i64 conversion for u32 (used by aski's toI64 on len).
pub trait ToI64 {
    fn to_i64(self) -> i64;
}

impl ToI64 for u32 {
    fn to_i64(self) -> i64 { self as i64 }
}

pub trait StringExt {
    fn to_snake(&self) -> String;
    fn to_rust_type(&self) -> String;
    fn to_param_type(&self) -> String;
    fn strip_vec(&self) -> String;
    fn all_fields_copy(&self) -> String;
    fn needs_pascal_alias(&self) -> String;
}

impl StringExt for String {
    fn to_snake(&self) -> String { to_snake(self) }
    fn to_rust_type(&self) -> String { to_rust_type(self) }
    fn to_param_type(&self) -> String { to_param_type(self) }
    fn strip_vec(&self) -> String { strip_vec(self) }
    fn all_fields_copy(&self) -> String { if all_fields_copy(self) { "Copy, ".into() } else { String::new() } }
    fn needs_pascal_alias(&self) -> String { needs_pascal_alias(self) }
}

impl StringExt for str {
    fn to_snake(&self) -> String { to_snake(self) }
    fn to_rust_type(&self) -> String { to_rust_type(self) }
    fn to_param_type(&self) -> String { to_param_type(self) }
    fn strip_vec(&self) -> String { strip_vec(self) }
    fn all_fields_copy(&self) -> String { if all_fields_copy(self) { "Copy, ".into() } else { String::new() } }
    fn needs_pascal_alias(&self) -> String { needs_pascal_alias(self) }
}

/// Check if a Rust type is Copy-eligible (no String, no Vec).
pub fn is_copy_type(t: &str) -> bool {
    match t {
        "I8" | "I16" | "I32" | "I64" | "U8" | "U16" | "U32" | "U64" |
        "F32" | "F64" | "Bool" => true,
        _ if t.starts_with("Vec{") => false,
        "String" => false,
        // Enum types (domains) are Copy
        _ => true,
    }
}

/// Check if all fields of a struct (given as comma-separated types) are Copy.
pub fn all_fields_copy(field_types: &str) -> bool {
    if field_types.is_empty() { return true; }
    field_types.split(',').all(|t| is_copy_type(t.trim()))
}

pub fn to_snake(s: &str) -> String {
    let mut out = String::new();
    for (i, ch) in s.chars().enumerate() {
        if ch.is_uppercase() && i > 0 {
            out.push('_');
        }
        out.push(ch.to_ascii_lowercase());
    }
    out
}

pub fn to_rust_type(t: &str) -> String {
    match t {
        "F32" => "f32".into(),
        "F64" => "f64".into(),
        "I8" => "i8".into(),
        "I16" => "i16".into(),
        "I32" => "i32".into(),
        "I64" => "i64".into(),
        "U8" => "u8".into(),
        "U16" => "u16".into(),
        "U32" => "u32".into(),
        "U64" => "u64".into(),
        "Bool" => "bool".into(),
        "String" => "String".into(),
        _ if t.starts_with("Vec{") && t.ends_with('}') => {
            format!("Vec<{}>", to_rust_type(&t[4..t.len() - 1]))
        }
        _ => t.to_string(),
    }
}

/// Returns the PascalCase from_str alias line if the variant is multi-word, empty otherwise.
pub fn needs_pascal_alias(name: &str) -> String {
    let snake = to_snake(name);
    if snake.contains('_') {
        // Multi-word: emit PascalCase alias
        format!("            \"{}\" => Some(Self::{}),\n", name, name)
    } else {
        String::new()
    }
}

pub fn to_param_type(t: &str) -> String {
    match t {
        "String" => "&str".into(),
        _ => to_rust_type(t),
    }
}

pub fn strip_vec(t: &str) -> String {
    if t.starts_with("Vec{") && t.ends_with('}') {
        t[4..t.len() - 1].to_string()
    } else {
        t.to_string()
    }
}

/// Functional Vec push — used by aski's withPush.
pub trait WithPush<T> {
    fn with_push(self, item: T) -> Self;
}

impl<T> WithPush<T> for Vec<T> {
    fn with_push(mut self, item: T) -> Self {
        self.push(item);
        self
    }
}

// ═══════════════════════════════════════════════════════════════
// FFI implementations — called by aski-generated code
// ═══════════════════════════════════════════════════════════════

/// Blake3 hashing — returns 32-byte hash as Vec<u8>
pub fn blake3_hash(data: &[u8]) -> Vec<u8> {
    blake3::hash(data).as_bytes().to_vec()
}

/// HashMap operations
pub fn hash_map_new<K, V>() -> std::collections::HashMap<K, V> {
    std::collections::HashMap::new()
}

/// Byte operations
pub fn bytes_concat(a: &[u8], b: &[u8]) -> Vec<u8> {
    let mut out = a.to_vec();
    out.extend_from_slice(b);
    out
}

pub fn bytes_slice(data: &[u8], start: usize, end: usize) -> Vec<u8> {
    data[start..end].to_vec()
}

pub fn bytes_eq(a: &[u8], b: &[u8]) -> bool {
    a == b
}

/// File I/O
pub fn fs_read(path: &str) -> Vec<u8> {
    std::fs::read(path).unwrap_or_default()
}

pub fn fs_write(path: &str, data: &[u8]) -> bool {
    std::fs::write(path, data).is_ok()
}

pub fn fs_exists(path: &str) -> bool {
    std::path::Path::new(path).exists()
}

pub fn fs_mkdir_all(path: &str) -> bool {
    std::fs::create_dir_all(path).is_ok()
}

pub fn fs_list_dir(path: &str) -> Vec<String> {
    std::fs::read_dir(path)
        .map(|entries| entries
            .filter_map(|e| e.ok())
            .filter_map(|e| e.file_name().into_string().ok())
            .collect())
        .unwrap_or_default()
}

/// Sema serialization (rkyv 0.8)
pub fn rkyv_to_bytes(val: &impl for<'a> rkyv::Serialize<
    rkyv::api::high::HighSerializer<rkyv::util::AlignedVec, rkyv::ser::allocator::ArenaHandle<'a>, rkyv::rancor::Error>,
>) -> Vec<u8> {
    rkyv::api::high::to_bytes::<rkyv::rancor::Error>(val).unwrap().to_vec()
}

/// Hex encoding
pub fn to_hex(data: &[u8]) -> String {
    data.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn from_hex(s: &str) -> Vec<u8> {
    (0..s.len())
        .step_by(2)
        .filter_map(|i| u8::from_str_radix(&s[i..i+2], 16).ok())
        .collect()
}
