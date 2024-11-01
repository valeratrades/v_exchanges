{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-parts.url = "github:hercules-ci/flake-parts";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    fenix.url = "github:nix-community/fenix";
  };

  outputs =
    inputs@{ nixpkgs, flake-parts, ... }:
    flake-parts.lib.mkFlake { inherit inputs; } {
      systems = nixpkgs.lib.systems.flakeExposed;
      perSystem =
        {
          lib,
          pkgs,
          system,
          config,
          ...
        }:
        {
          _module.args.pkgs = import nixpkgs {
            inherit system;
            overlays = [
              (inputs.fenix.overlays.default)
            ];
          };

          packages =
            let
              manifest = (pkgs.lib.importTOML ./Cargo.toml).package;
            in
            {
              default = pkgs.rustPlatform.buildRustPackage rec {
                pname = manifest.name;
                version = manifest.version;

                buildInputs = with pkgs; [
                  openssl
                  openssl.dev
                ];
                nativeBuildInputs = with pkgs; [ pkg-config ];
                env.PKG_CONFIG_PATH = "${pkgs.openssl.dev}/lib/pkgconfig";

                cargoLock.lockFile = ./Cargo.lock;
                src = pkgs.lib.cleanSource ./.;
              };
            };

          devShells.default =
            with pkgs;
            let
              toolchain = pkgs.fenix.complete.withComponents [
                "rustc"
                "cargo"
                "clippy"
              ];
            in
            mkShell {
              packages = with pkgs; [
                openssl
                rust-analyzer-nightly
                toolchain
                pkg-config
              ];
              LD_LIBRARY_PATH = lib.makeLibraryPath [ pkgs.openssl ];
            };
        };
    };
}
