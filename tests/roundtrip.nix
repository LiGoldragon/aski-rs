{ pkgs, askic, aski-core-src, rustc }:

let
  synth-dir = "${aski-core-src}/source";

  elements = pkgs.writeText "elements.aski" ''
    (Elements/ Element Quality describe)
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
    (Math/ Addition compute)
    {Addition/ Left U32 Right U32}
    (compute/ [(add/ :@Self U32)])
    [compute/ Addition [
      (add/ :@Self U32 [
        ^(@Self.Left + @Self.Right)
      ])
    ]]
  '';

  params = pkgs.writeText "params.aski" ''
    (Params/ Addition multiply)
    {Addition/ Left U32 Right U32}
    (multiply/ [(multiply/ :@Self @Factor U32 U32)])
    [multiply/ Addition [
      (multiply/ :@Self @Factor U32 U32 [
        ^(@Self.Left * @Factor + @Self.Right * @Factor)
      ])
    ]]
  '';

  matching = pkgs.writeText "matching.aski" ''
    (Matching/ Element Polarity categorize)
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
    (Constants/ MaxSigns Pi)
    {|MaxSigns/ U32 12|}
    {|Pi/ F64 3.14159265358979|}
  '';
in
pkgs.runCommand "askic-roundtrip-test" {
  nativeBuildInputs = [ askic rustc ];
} ''
  set -euo pipefail
  mkdir -p $out

  echo "=== Test 1: elements.aski → Rust (compiles) ==="
  askic rust ${elements} --synth-dir ${synth-dir} > $out/elements.rs
  cat $out/elements.rs
  rustc $out/elements.rs --crate-type lib -o $out/libelements.rlib
  echo "  ✓ elements.rs compiles"

  echo ""
  echo "=== Test 2: math.aski → Rust (compiles) ==="
  askic rust ${math} --synth-dir ${synth-dir} > $out/math.rs
  cat $out/math.rs
  rustc $out/math.rs --crate-type lib -o $out/libmath.rlib
  echo "  ✓ math.rs compiles"

  echo ""
  echo "=== Test 3: params.aski → Rust (multi-param methods) ==="
  askic rust ${params} --synth-dir ${synth-dir} > $out/params.rs
  cat $out/params.rs
  rustc $out/params.rs --crate-type lib -o $out/libparams.rlib
  echo "  ✓ params.rs compiles"

  echo ""
  echo "=== Test 4: matching.aski → Rust (or-patterns) ==="
  askic rust ${matching} --synth-dir ${synth-dir} > $out/matching.rs
  cat $out/matching.rs
  rustc $out/matching.rs --crate-type lib -o $out/libmatching.rlib
  echo "  ✓ matching.rs compiles"

  echo ""
  echo "=== Test 5: constants.aski → Rust (const decls) ==="
  askic rust ${constants} --synth-dir ${synth-dir} > $out/constants.rs
  cat $out/constants.rs
  rustc $out/constants.rs --crate-type lib -o $out/libconstants.rlib
  echo "  ✓ constants.rs compiles"

  echo ""
  echo "=== Test 6: elements.aski sema ==="
  askic sema ${elements} --synth-dir ${synth-dir} > $out/elements-sema.txt
  cat $out/elements-sema.txt

  echo ""
  echo "=== Test 7: math.aski sema ==="
  askic sema ${math} --synth-dir ${synth-dir} > $out/math-sema.txt
  cat $out/math-sema.txt

  echo ""
  echo "=== Test 8: elements.aski roundtrip ==="
  askic roundtrip ${elements} --synth-dir ${synth-dir} > $out/elements-roundtripped.aski
  cat $out/elements-roundtripped.aski

  echo ""
  echo "=== Test 9: math.aski roundtrip ==="
  askic roundtrip ${math} --synth-dir ${synth-dir} > $out/math-roundtripped.aski
  cat $out/math-roundtripped.aski

  echo ""
  echo "=== All 9 tests passed ==="
''
