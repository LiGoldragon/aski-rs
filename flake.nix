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
    in {
      packages.${system} = {
        default = aski-rs;
        askic-static = askic-static;
      };
      devShells.${system}.default = craneLib.devShell {
        packages = with pkgs; [ rust-analyzer ];
      };
    };
}
