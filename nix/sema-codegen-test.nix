# Pure-sema codegen test.
# Generates Rust from .sema files ONLY — no .aski source available.
# Proves the sema artifact is self-contained for codegen.

{ pkgs, askic, aski-core-src, example-sema, rustc }:

pkgs.runCommand "aski-sema-codegen-test" {
  nativeBuildInputs = [ askic rustc ];
} ''
  set -euo pipefail
  mkdir -p $out

  for sema in ${example-sema}/*.sema; do
    # Skip .aski-table.sema — only process code .sema
    case "$sema" in *.aski-table.sema) continue ;; esac

    name=$(basename "$sema" .sema)
    echo "=== $name: .sema → Rust (no .aski source) ==="
    askic rust "$sema" > "$out/$name.rs"
    rustc "$out/$name.rs" --crate-type lib -o "$out/lib$name.rlib"
    echo "  ✓ $name.sema → .rs compiles"
  done

  echo ""
  echo "=== No-strings check ==="
  for sema in ${example-sema}/*.sema; do
    case "$sema" in *.aski-table.sema) continue ;; esac
    name=$(basename "$sema" .sema)
    if strings "$sema" | grep -qiE "Element|Fire|Quality|Addition|describe|compute"; then
      echo "FAIL: $name.sema contains strings"
      exit 1
    fi
    echo "  ✓ $name.sema: no strings"
  done

  echo ""
  echo "=== All sema-codegen tests passed ==="
''
