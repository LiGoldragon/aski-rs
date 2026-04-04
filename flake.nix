{
  description = "aski-rs — Kernel Aski parser, Rust codegen, rustc integration";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    fenix = {
      url = "github:nix-community/fenix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    crane.url = "github:ipetkov/crane";
    aski-core-src = {
      url = "github:LiGoldragon/aski-core";
      flake = false;
    };
  };

  outputs = { self, nixpkgs, fenix, crane, aski-core-src, ... }:
    let
      system = "x86_64-linux";
      pkgs = nixpkgs.legacyPackages.${system};
      # Dynamic toolchain (default build + dev shell)
      toolchain = fenix.packages.${system}.stable.toolchain;
      craneLib = (crane.mkLib pkgs).overrideToolchain toolchain;

      # Static musl toolchain (bootstrap binary)
      toolchainMusl = with fenix.packages.${system}; combine [
        stable.cargo
        stable.rustc
        targets.x86_64-unknown-linux-musl.stable.rust-std
      ];
      craneMusl = (crane.mkLib pkgs).overrideToolchain toolchainMusl;

      src = pkgs.lib.cleanSourceWith {
        src = ./.;
        filter = path: type:
          (craneLib.filterCargoSources path type) ||
          (builtins.match ".*\\.aski$" path != null);
      };

      commonArgs = {
        inherit src;
        pname = "aski-rs";
        version = "0.4.0";
        postUnpack = ''
          mkdir -p source/flake-crates
          cp -r ${aski-core-src} source/flake-crates/aski-core
        '';
        ASKI_BOOTSTRAP = "1";
      };

      # ── Default (dynamic glibc) build ──
      cargoArtifacts = craneLib.buildDepsOnly commonArgs;
      aski-rs-unwrapped = craneLib.buildPackage (commonArgs // {
        inherit cargoArtifacts;
        postInstall = ''
          mkdir -p $out/share/aski-grammar
          cp -r $src/grammar/* $out/share/aski-grammar/
        '';
      });
      aski-rs = pkgs.symlinkJoin {
        name = "aski-rs-0.4.0";
        paths = [ aski-rs-unwrapped ];
        nativeBuildInputs = [ pkgs.makeWrapper ];
        postBuild = ''
          wrapProgram $out/bin/askic \
            --set ASKI_GRAMMAR_DIR "$out/share/aski-grammar"
        '';
      };

      # ── Static musl build (bootstrap FOD source) ──
      muslArgs = commonArgs // {
        CARGO_BUILD_TARGET = "x86_64-unknown-linux-musl";
        CARGO_BUILD_RUSTFLAGS = "-C target-feature=+crt-static";
      };
      cargoArtifactsMusl = craneMusl.buildDepsOnly muslArgs;
      askic-static = craneMusl.buildPackage (muslArgs // {
        cargoArtifacts = cargoArtifactsMusl;
        doCheck = false;
        postInstall = ''
          strip $out/bin/askic
        '';
      });
      # ── Bootstrap update script ──
      update-bootstrap = pkgs.writeShellScriptBin "update-bootstrap" ''
        set -euo pipefail
        export PATH="$PATH:${pkgs.lib.makeBinPath (with pkgs; [ gnused coreutils gnugrep ])}"

        ASKI_RS="''${ASKI_RS_DIR:-$(pwd)}"
        ASKI_CORE="''${ASKI_CORE_DIR:-$ASKI_RS/../aski-core}"
        REPO="LiGoldragon/aski-rs"

        # 1. Build static askic
        echo ":: Building askic-static..."
        nix build "$ASKI_RS#askic-static"

        # 2. Determine next version (subversion: v0.4.0 -> v0.4.0.1 -> v0.4.0.2)
        CURRENT=$(gh release list -R "$REPO" --limit 1 --json tagName -q '.[0].tagName')
        BASE="''${CURRENT#v}"
        IFS='.' read -ra SEGMENTS <<< "$BASE"
        if [ "''${#SEGMENTS[@]}" -le 3 ]; then
          NEXT="v''${BASE}.1"
        else
          SUB="''${SEGMENTS[-1]}"
          PREFIX="''${BASE%."$SUB"}"
          NEXT="v''${PREFIX}.$((SUB + 1))"
        fi
        echo ":: Version: $CURRENT -> $NEXT"

        # 3. Upload to GitHub release (never reuse tags)
        TMPDIR=$(mktemp -d)
        cp "$ASKI_RS/result/bin/askic" "$TMPDIR/askic-x86_64-linux"
        chmod +x "$TMPDIR/askic-x86_64-linux"
        gh release create "$NEXT" "$TMPDIR/askic-x86_64-linux" \
          -R "$REPO" \
          --title "$NEXT — bootstrap askic" \
          --notes "Static musl askic bootstrap binary."
        echo ":: Release $NEXT created"

        # 4. Prefetch with --executable to get the NAR hash (fetchurl uses
        #    outputHashMode=recursive when executable=true)
        URL="https://github.com/$REPO/releases/download/$NEXT/askic-x86_64-linux"
        echo ":: Waiting for GitHub CDN propagation..."
        sleep 15
        echo ":: Prefetching $URL ..."
        BASE32=$(nix-prefetch-url --executable --type sha256 "$URL" 2>/dev/null)
        HASH=$(nix hash convert --hash-algo sha256 --to sri "$BASE32")
        echo ":: Hash: $HASH"

        # 5. Update aski-core flake.nix
        sed -i "s|releases/download/v[^/]*/askic|releases/download/$NEXT/askic|" "$ASKI_CORE/flake.nix"
        sed -i "s|hash = \"sha256-[^\"]*\"|hash = \"$HASH\"|" "$ASKI_CORE/flake.nix"
        echo ":: Updated aski-core/flake.nix"

        # 6. Update aski-core flake.lock (pin new aski-rs rev)
        (cd "$ASKI_CORE" && nix flake update aski-rs-src)
        echo ":: Updated aski-core/flake.lock"

        # 7. Verify build
        echo ":: Verifying aski-core build..."
        (cd "$ASKI_CORE" && nix build)
        echo ":: Build verified"

        # 8. Commit and push aski-core
        (cd "$ASKI_CORE" && \
          jj commit -m "((\"nix\", \"aski-core\"), (\"update\", \"bootstrap askic $NEXT\"), (\"pipeline\", \"automated via update-bootstrap\"))" && \
          jj bookmark set main -r @- && \
          jj git push)
        echo ":: aski-core committed and pushed"

        rm -rf "$TMPDIR"
        echo ":: Done. Bootstrap updated to $NEXT"
      '';
    in {
      packages.${system} = {
        default = aski-rs;
        askic-static = askic-static;
      };
      apps.${system}.update-bootstrap = {
        type = "app";
        program = "${update-bootstrap}/bin/update-bootstrap";
      };
      devShells.${system}.default = craneLib.devShell {
        packages = with pkgs; [ rust-analyzer ];
      };
    };
}
