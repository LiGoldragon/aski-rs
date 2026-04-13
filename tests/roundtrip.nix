{ pkgs, askic, aski-core-src, rustc }:

let
  synth-dir = "${aski-core-src}/source";

  elements = pkgs.writeText "elements.aski" ''
    {elements/ Element Quality describe}
    (Element/ Fire Earth Air Water)
    (Quality/ Passionate Grounded Intellectual Intuitive)
    (describe/ [(describe/ :@Self Quality)])
    [describe/ Element [
      (describe/ :@Self Quality (|
        (Fire) Passionate
        (Earth) Grounded
        (Air) Intellectual
        (Water) Intuitive
      |))
    ]]
  '';

  math = pkgs.writeText "math.aski" ''
    {math/ Addition compute}
    {Addition/ Left U32 Right U32}
    (compute/ [(add/ :@Self U32)])
    [compute/ Addition [
      (add/ :@Self U32 [
        ^(@Self.Left + @Self.Right)
      ])
    ]]
  '';

  params = pkgs.writeText "params.aski" ''
    {params/ Addition multiply}
    {Addition/ Left U32 Right U32}
    (multiply/ [(multiply/ :@Self @Factor U32 U32)])
    [multiply/ Addition [
      (multiply/ :@Self @Factor U32 U32 [
        ^(@Self.Left * @Factor + @Self.Right * @Factor)
      ])
    ]]
  '';

  matching = pkgs.writeText "matching.aski" ''
    {matching/ Element Polarity categorize}
    (Element/ Fire Earth Air Water)
    (Polarity/ Active Receptive)
    (categorize/ [(polarity/ :@Self Polarity)])
    [categorize/ Element [
      (polarity/ :@Self Polarity (|
        (Fire | Air) Active
        (Earth | Water) Receptive
      |))
    ]]
  '';

  constants = pkgs.writeText "constants.aski" ''
    {constants/ MaxSigns Pi}
    {|MaxSigns/ U32 12|}
    {|Pi/ F64 3.14159265358979|}
  '';
in
pkgs.runCommand "askic-roundtrip-test" {
  nativeBuildInputs = [ askic rustc ];
} ''
  set -euo pipefail
  mkdir -p $out

  # ── Test 1-5: .aski → .sema → .rs (through sema artifact) ──

  for name in elements math params matching constants; do
    aski="''${!name}"
    echo "=== $name: compile to .sema ==="
    askic compile "$aski" --synth-dir ${synth-dir}
    sema="''${aski%.aski}.sema"
    table="''${aski%.aski}.aski-table.sema"
    test -f "$sema" || (echo "FAIL: $sema not found"; exit 1)
    test -f "$table" || (echo "FAIL: $table not found"; exit 1)

    echo "=== $name: .sema → Rust ==="
    askic rust "$sema" --synth-dir ${synth-dir} > "$out/$name.rs"
    cat "$out/$name.rs"

    echo "=== $name: rustc ==="
    rustc "$out/$name.rs" --crate-type lib -o "$out/lib$name.rlib"
    echo "  ✓ $name: .aski → .sema → .rs compiles"
    echo ""
  done

  # ── Test 6-7: roundtrip through .sema binary ──

  echo "=== elements roundtrip: .aski → .sema → .aski ==="
  askic roundtrip ${elements} --synth-dir ${synth-dir} > $out/elements-roundtripped.aski
  cat $out/elements-roundtripped.aski

  echo ""
  echo "=== math roundtrip: .aski → .sema → .aski ==="
  askic roundtrip ${math} --synth-dir ${synth-dir} > $out/math-roundtripped.aski
  cat $out/math-roundtripped.aski

  # ── Test 8: no strings in .sema binary ──

  echo ""
  echo "=== no-strings check ==="
  askic compile ${elements} --synth-dir ${synth-dir}
  sema="''${elements%.aski}.sema"
  if strings "$sema" | grep -q "Element\|Fire\|Quality"; then
    echo "FAIL: strings found in .sema binary"
    exit 1
  fi
  echo "  ✓ .sema binary contains no strings"

  echo ""
  echo "=== All tests passed ==="
''
