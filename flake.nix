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
      toolchain = fenix.packages.${system}.stable.toolchain;
      craneLib = (crane.mkLib pkgs).overrideToolchain toolchain;

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
          (builtins.match ".*\\.aski$" path != null) ||
          (builtins.match ".*\\.synth$" path != null);
      };

      commonArgs = {
        inherit src;
        pname = "aski-rs";
        version = "0.15.0";
      };

      cargoArtifacts = craneLib.buildDepsOnly commonArgs;
      askic = craneLib.buildPackage (commonArgs // {
        inherit cargoArtifacts;
      });

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

      example-sema = import ./nix/sema.nix {
        inherit pkgs askic aski-core-src;
      };

    in {
      packages.${system} = {
        default = askic;
        inherit askic askic-static;
        sema = example-sema;
      };

      checks.${system} = {
        cargo-tests = craneLib.cargoTest (commonArgs // {
          inherit cargoArtifacts;
        });

        sema-codegen = import ./nix/sema-codegen-test.nix {
          inherit pkgs askic aski-core-src example-sema;
          rustc = toolchain;
        };

        roundtrip = import ./nix/roundtrip-test.nix {
          inherit pkgs askic aski-core-src;
          rustc = toolchain;
        };
      };

      devShells.${system}.default = craneLib.devShell {
        packages = with pkgs; [ rust-analyzer ];
      };
    };
}
