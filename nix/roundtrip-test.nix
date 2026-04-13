# Roundtrip test: .aski → .sema → .rs (compiles with rustc)
# Uses the actual example .aski files from aski-core.

{ pkgs, askic, aski-core-src, rustc }:

let
  synth-dir = "${aski-core-src}/source";
  examples-dir = "${aski-core-src}/examples/hello";
in
pkgs.runCommand "askic-roundtrip-test" {
  nativeBuildInputs = [ askic rustc ];
} ''
  set -euo pipefail
  mkdir -p $out

  echo "=== Compile .aski → .sema → .rs ==="
  for aski in ${examples-dir}/*.aski; do
    name=$(basename "$aski" .aski)

    # Compile via sema shorthand (writes .sema then reads it back)
    askic rust "$aski" --synth-dir ${synth-dir} > "$out/$name.rs"
    rustc "$out/$name.rs" --crate-type lib -o "$out/lib$name.rlib"
    echo "  ✓ $name"
  done

  echo ""
  echo "=== Roundtrip .aski → .sema → .aski ==="
  for aski in ${examples-dir}/*.aski; do
    name=$(basename "$aski" .aski)
    askic roundtrip "$aski" --synth-dir ${synth-dir} > "$out/$name-roundtripped.aski"
    echo "  ✓ $name roundtrip"
  done

  echo ""
  echo "=== All roundtrip tests passed ==="
''
