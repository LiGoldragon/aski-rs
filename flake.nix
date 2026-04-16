{
  description = "semac — sema backend: rkyv parse tree → sema + Rust";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    fenix = {
      url = "github:nix-community/fenix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    crane.url = "github:ipetkov/crane";
    flake-utils.url = "github:numtide/flake-utils";
    sema-core = {
      url = "github:LiGoldragon/sema-core";
      inputs.nixpkgs.follows = "nixpkgs";
      inputs.fenix.follows = "fenix";
      inputs.crane.follows = "crane";
      inputs.flake-utils.follows = "flake-utils";
    };
  };

  outputs = { self, nixpkgs, fenix, crane, flake-utils, sema-core, ... }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = nixpkgs.legacyPackages.${system};
        toolchain = fenix.packages.${system}.stable.toolchain;
        craneLib = (crane.mkLib pkgs).overrideToolchain toolchain;

        # sema-core source — the rkyv contract for parse trees
        sema-core-source = sema-core.packages.${system}.source;

        src = pkgs.lib.cleanSourceWith {
          src = ./.;
          filter = path: type:
            craneLib.filterCargoSources path type;
        };

        commonArgs = {
          inherit src;
          pname = "semac";
          version = "0.17.0";
          # Populate flake-crates/ for Cargo path dep
          postUnpack = ''
            mkdir -p $sourceRoot/flake-crates
            cp -r ${sema-core-source} $sourceRoot/flake-crates/sema-core
            chmod -R +w $sourceRoot/flake-crates
          '';
        };

        cargoArtifacts = craneLib.buildDepsOnly commonArgs;

        semac = craneLib.buildPackage (commonArgs // {
          inherit cargoArtifacts;
        });

      in {
        packages = {
          default = semac;
          inherit semac;
        };

        checks = {
          build = semac;
          tests = craneLib.cargoTest (commonArgs // {
            inherit cargoArtifacts;
          });
        };

        devShells.default = craneLib.devShell {
          packages = [ pkgs.rust-analyzer ];
        };
      }
    );
}
