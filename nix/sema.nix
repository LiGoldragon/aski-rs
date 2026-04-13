# Compile all example .aski files to .sema + .aski-table.sema
# This IS the sema artifact — inspectable, reusable, no .aski needed after.

{ pkgs, askic, aski-core-src }:

let
  synth-dir = "${aski-core-src}/source";
  examples-dir = "${aski-core-src}/examples/hello";
in
pkgs.runCommand "aski-example-sema" {
  nativeBuildInputs = [ askic ];
} ''
  mkdir -p $out
  for aski in ${examples-dir}/*.aski; do
    name=$(basename "$aski" .aski)
    echo ":: compile $name.aski"
    cp "$aski" "$out/$name.aski"
    askic compile "$out/$name.aski" --synth-dir ${synth-dir}
    rm "$out/$name.aski"
  done
  echo ""
  echo ":: Sema artifacts:"
  ls -la $out/
''
