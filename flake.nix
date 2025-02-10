{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay.url = "github:oxalica/rust-overlay";
    flake-utils.url = "github:numtide/flake-utils";
    pre-commit-hooks.url = "github:cachix/git-hooks.nix";
    v-parts.url = "github:valeratrades/.github";
  };

  outputs = { nixpkgs, rust-overlay, flake-utils, pre-commit-hooks, v-parts, ... }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        overlays = builtins.trace "flake.nix sourced" [ (import rust-overlay) ];
        pkgs = import nixpkgs {
          inherit system overlays;
        };
        checks = {
          pre-commit-check = pre-commit-hooks.lib.${system}.run {
            src = ./.;
            hooks = {
              treefmt = {
                enable = true;
                settings = {
                  #BUG: this option does NOTHING
                  fail-on-change = false; # that's GHA's job, pre-commit hooks stricty *do*
                  formatters = with pkgs; [
                    nixpkgs-fmt
                  ];
                };
              };
            };
          };
        };
        
        workflowContents = (import ./.github/workflows/ci.nix) { inherit pkgs; last-supported-version = "nightly-2025-01-01"; workflow-parts = v-parts.workflows; };

        readme = (v-parts.readme-fw { inherit pkgs; last-supported-version = "nightly-1.85"; prj_name = "v_exchanges"; root = ./.; loc = "5167"; licenses = [{ name = "Blue Oak 1.0.0"; out_path = "LICENSE"; }]; badges = [ "msrv" "crates_io" "docs_rs" "loc" "ci" ]; }).combined;
      in
      {
        packages =
          let
            manifest = (pkgs.lib.importTOML ./Cargo.toml).package;
            rust = (pkgs.rust-bin.fromRustupToolchainFile ./.cargo/rust-toolchain.toml);
            rustc = rust;
            cargo = rust;
            stdenv = pkgs.stdenvAdapters.useMoldLinker pkgs.stdenv;
            rustPlatform = pkgs.makeRustPlatform {
              inherit rustc cargo stdenv;
            };
          in
          {
            default = rustPlatform.buildRustPackage rec {
              pname = manifest.name;
              version = manifest.version;

              buildInputs = with pkgs; [
                openssl.dev
              ];
              nativeBuildInputs = with pkgs; [ pkg-config ];
              env.PKG_CONFIG_PATH = "${pkgs.openssl.dev}/lib/pkgconfig";

              cargoLock.lockFile = ./Cargo.lock;
              src = pkgs.lib.cleanSource ./.;
            };
          };

        devShells.default = with pkgs; mkShell {
          inherit stdenv;
          shellHook = checks.pre-commit-check.shellHook + ''
            rm -f ./.github/workflows/errors.yml; cp ${workflowContents.errors} ./.github/workflows/errors.yml
            rm -f ./.github/workflows/warnings.yml; cp ${workflowContents.warnings} ./.github/workflows/warnings.yml

            cp -f ${v-parts.files.licenses.blue_oak} ./LICENSE

            cargo -Zscript -q ${v-parts.hooks.appendCustom} ./.git/hooks/pre-commit
            cp -f ${(import v-parts.hooks.treefmt {inherit pkgs;})} ./.treefmt.toml
            cp -f ${(import v-parts.files.rust.rustfmt {inherit pkgs;})} ./rustfmt.toml
            cp -f ${(import v-parts.files.rust.deny {inherit pkgs;})} ./deny.toml
            cp -f ${(import v-parts.files.rust.config {inherit pkgs;})} ./.cargo/config.toml
            cp -f ${(import v-parts.files.rust.toolchain {inherit pkgs;})} ./.cargo/rust-toolchain.toml
            cp -f ${(import v-parts.files.gitignore) { inherit pkgs; langs = ["rs"];}} ./tmp/.gitignore

            cp -f ${readme} ./README.md
          '';
          packages = [
            mold-wrapped
            openssl
            pkg-config
            (rust-bin.fromRustupToolchainFile ./.cargo/rust-toolchain.toml)
          ] ++ checks.pre-commit-check.enabledPackages;
        };
      }
    );
}

