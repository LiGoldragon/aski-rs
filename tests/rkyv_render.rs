use std::io::Write;
use std::process::Command;

use aski_rs::codegen::{CodegenConfig, generate_rust_from_db_with_config};
use aski_rs::codec;
use aski_rs::db::{create_db, insert_ast};
use aski_rs::parser::parse_source;

/// End-to-end: parse .aski → generate Rust with rkyv → compile → serialize →
/// capture bytes → decode to aski syntax via codec.
#[test]
fn rkyv_serialize_then_decode_via_codec() {
    // 1. Parse render_test.aski
    let manifest = env!("CARGO_MANIFEST_DIR");
    let path = format!("{manifest}/encoder/design/v0.8/examples/render_test.aski");
    let source = std::fs::read_to_string(&path).expect("failed to read render_test.aski");

    // 2. Parse and insert into CozoDB
    let items = parse_source(&source).expect("failed to parse");
    let db = create_db().expect("failed to create db");
    insert_ast(&db, &items).expect("failed to insert AST");

    // 3. Generate Rust with rkyv=true
    let config = CodegenConfig { rkyv: true };
    let mut rust_code = generate_rust_from_db_with_config(&db, &config).expect("failed to generate");

    // 4. Append main that serializes and prints rkyv bytes
    rust_code.push_str(r#"
fn main() {
    let element = Element::Fire;
    let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&element).unwrap();
    print!("ELEMENT:");
    for b in bytes.as_ref() { print!("{:02x}", b); }
    println!();

    let placement = Placement {
        body: Planet::Jupiter,
        position: Sign::Sagittarius,
        house_num: House::Ninth,
    };
    let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&placement).unwrap();
    print!("PLACEMENT:");
    for b in bytes.as_ref() { print!("{:02x}", b); }
    println!();

    let chart = ChartSummary {
        sun_placement: Placement {
            body: Planet::Sun,
            position: Sign::Aries,
            house_num: House::First,
        },
        moon_placement: Placement {
            body: Planet::Moon,
            position: Sign::Cancer,
            house_num: House::Fourth,
        },
        dominant_element: Element::Fire,
    };
    let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&chart).unwrap();
    print!("CHART:");
    for b in bytes.as_ref() { print!("{:02x}", b); }
    println!();
}
"#);

    // 5. Build via cargo
    let dir = std::env::temp_dir();
    let proj_dir = dir.join("aski_rkyv_codec_test");
    let src_dir = proj_dir.join("src");
    std::fs::create_dir_all(&src_dir).expect("failed to create proj dir");
    std::fs::write(src_dir.join("main.rs"), &rust_code).expect("failed to write");
    std::fs::write(proj_dir.join("Cargo.toml"), r#"
[package]
name = "aski-rkyv-test"
version = "0.1.0"
edition = "2021"
[dependencies]
rkyv = { version = "0.8", features = ["bytecheck"] }
"#).expect("failed to write Cargo.toml");

    let build = Command::new("cargo")
        .arg("build").arg("--release")
        .current_dir(&proj_dir)
        .output();

    let build = match build {
        Ok(o) => o,
        Err(_) => { let _ = std::fs::remove_dir_all(&proj_dir); return; }
    };
    if !build.status.success() {
        let stderr = String::from_utf8_lossy(&build.stderr);
        panic!("build failed:\n{stderr}\n\nCode:\n{rust_code}");
    }

    // 6. Run and capture bytes
    let run = Command::new(proj_dir.join("target/release/aski-rkyv-test"))
        .output().expect("failed to run");
    assert!(run.status.success());
    let stdout = String::from_utf8_lossy(&run.stdout);

    let mut rkyv_bytes: std::collections::HashMap<String, Vec<u8>> = std::collections::HashMap::new();
    for line in stdout.lines() {
        if let Some((label, hex)) = line.split_once(':') {
            let bytes: Vec<u8> = (0..hex.len()).step_by(2)
                .filter_map(|i| u8::from_str_radix(&hex[i..i+2], 16).ok())
                .collect();
            rkyv_bytes.insert(label.to_string(), bytes);
        }
    }

    // 7. Verify rkyv bytes match expected ordinals
    let element_bytes = rkyv_bytes.get("ELEMENT").expect("no ELEMENT");
    assert_eq!(element_bytes, &[0], "Fire = ordinal 0");

    let placement_bytes = rkyv_bytes.get("PLACEMENT").expect("no PLACEMENT");
    assert_eq!(placement_bytes, &[5, 8, 8], "Jupiter=5, Sagittarius=8, Ninth=8");

    let chart_bytes = rkyv_bytes.get("CHART").expect("no CHART");
    assert_eq!(chart_bytes, &[0, 0, 0, 1, 3, 3, 0], "nested chart ordinals");

    // 8. Use codec to encode aski text → bytes, verify matches rkyv output
    let encoded = codec::encode(&db, "Placement(Body(Jupiter) Position(Sagittarius) HouseNum(Ninth))").unwrap();
    // First byte is World ordinal for Placement, rest is field ordinals
    assert_eq!(&encoded[1..], placement_bytes, "codec encode matches rkyv serialization");

    // 9. Use codec to decode rkyv bytes → aski text
    // Prepend World ordinal for Placement to the rkyv bytes
    let mut world_bytes = encoded[0..1].to_vec();
    world_bytes.extend_from_slice(placement_bytes);
    let decoded = codec::decode(&db, &world_bytes).unwrap();
    assert_eq!(decoded, "Placement(Body(Jupiter) Position(Sagittarius) HouseNum(Ninth))");

    let _ = std::fs::remove_dir_all(&proj_dir);
}
