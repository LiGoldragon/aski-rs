{
  description = "semac — Stage 3: sema binary compiler";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    fenix = {
      url = "github:nix-community/fenix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    crane.url = "github:ipetkov/crane";
    askic = {
      url = "github:LiGoldragon/askic";
      inputs.nixpkgs.follows = "nixpkgs";
      inputs.fenix.follows = "fenix";
      inputs.crane.follows = "crane";
    };
  };

  outputs = { self, nixpkgs, fenix, crane, askic, ... }:
    let
      system = "x86_64-linux";
      pkgs = nixpkgs.legacyPackages.${system};
      toolchain = fenix.packages.${system}.stable.toolchain;
      craneLib = (crane.mkLib pkgs).overrideToolchain toolchain;

      askic-bin = askic.packages.${system}.askic;
      askicc-bin = askic.inputs.askicc.packages.${system}.askicc;
      synth-dialect = askic.inputs.askicc.packages.${system}.synth-dialect;

      src = pkgs.lib.cleanSourceWith {
        src = ./.;
        filter = path: type:
          (craneLib.filterCargoSources path type)
          || (builtins.match ".*\\.aski$" path != null)
          || (builtins.match ".*\\.synth$" path != null);
      };

      commonArgs = {
        inherit src;
        pname = "semac";
        version = "0.16.0";
        nativeBuildInputs = [ askicc-bin askic-bin ];
        SYNTH_DIR = "${synth-dialect}";
      };

      cargoArtifacts = craneLib.buildDepsOnly commonArgs;

      semac = craneLib.buildPackage (commonArgs // {
        inherit cargoArtifacts;
      });

    in {
      packages.${system} = {
        default = semac;
        inherit semac;
      };

      checks.${system} = {
        build = semac;
        cargo-tests = craneLib.cargoTest (commonArgs // {
          inherit cargoArtifacts;
        });
      };

      devShells.${system}.default = craneLib.devShell {
        packages = [ askicc-bin askic-bin pkgs.rust-analyzer ];
        SYNTH_DIR = "${synth-dialect}";
      };
    };
}
