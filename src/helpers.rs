//! Helper functions called by aski-generated code.
//! to_snake, to_rust_type, strip_vec, from_ordinal.

pub trait StringExt {
    fn to_snake(&self) -> String;
    fn to_rust_type(&self) -> String;
    fn strip_vec(&self) -> String;
}

impl StringExt for String {
    fn to_snake(&self) -> String { to_snake(self) }
    fn to_rust_type(&self) -> String { to_rust_type(self) }
    fn strip_vec(&self) -> String { strip_vec(self) }
}

impl StringExt for str {
    fn to_snake(&self) -> String { to_snake(self) }
    fn to_rust_type(&self) -> String { to_rust_type(self) }
    fn strip_vec(&self) -> String { strip_vec(self) }
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

pub fn strip_vec(t: &str) -> String {
    if t.starts_with("Vec{") && t.ends_with('}') {
        t[4..t.len() - 1].to_string()
    } else {
        t.to_string()
    }
}
