{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay.url = "github:oxalica/rust-overlay";
    flake-utils.url = "github:numtide/flake-utils";
    pre-commit-hooks.url = "github:cachix/git-hooks.nix";
    v-utils.url = "github:valeratrades/.github";
  };

  outputs = { self, nixpkgs, rust-overlay, flake-utils, pre-commit-hooks, v-utils }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        overlays = builtins.trace "flake.nix sourced" [ (import rust-overlay) ];
        pkgs = import nixpkgs {
          inherit system overlays;
        };

        pre-commit-check = pre-commit-hooks.lib.${system}.run (v-utils.files.preCommit { inherit pkgs; });
        manifest = (pkgs.lib.importTOML ./v_exchanges/Cargo.toml).package;
        pname = manifest.name;
        stdenv = pkgs.stdenvAdapters.useMoldLinker pkgs.stdenv;

        workflowContents = v-utils.ci { inherit pkgs; lastSupportedVersion = "nightly-2025-01-16"; jobsErrors = [ "rust-tests" ]; jobsWarnings = [ "rust-doc" "rust-clippy" "rust-machete" "rust-sort" "rust-miri" "tokei" ]; };
        readme = v-utils.readme-fw { inherit pkgs pname; lastSupportedVersion = "nightly-1.86"; rootDir = ./.; licenses = [{ name = "Blue Oak 1.0.0"; outPath = "LICENSE"; }]; badges = [ "msrv" "crates_io" "docs_rs" "loc" "ci" ]; };
      in
      {
        packages =
          let
            rust = (pkgs.rust-bin.fromRustupToolchainFile ./.cargo/rust-toolchain.toml);
            rustc = rust;
            cargo = rust;
            rustPlatform = pkgs.makeRustPlatform {
              inherit rustc cargo stdenv;
            };
          in
          {
            default = rustPlatform.buildRustPackage rec {
              inherit pname;
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
          shellHook =
            pre-commit-check.shellHook +
            ''
              mkdir -p ./.github/workflows
              rm -f ./.github/workflows/errors.yml; cp ${workflowContents.errors} ./.github/workflows/errors.yml
              rm -f ./.github/workflows/warnings.yml; cp ${workflowContents.warnings} ./.github/workflows/warnings.yml

              cp -f ${v-utils.files.licenses.blue_oak} ./LICENSE

              cargo -Zscript -q ${v-utils.hooks.appendCustom} ./.git/hooks/pre-commit
              cp -f ${(v-utils.hooks.treefmt) {inherit pkgs;}} ./.treefmt.toml
              cp -f ${(v-utils.hooks.preCommit) { inherit pkgs pname; }} ./.git/hooks/custom.sh

              mkdir -p ./.cargo
              cp -f ${(v-utils.files.rust.config {inherit pkgs;})} ./.cargo/config.toml #dbg
              cp -f ${(v-utils.files.rust.toolchain {inherit pkgs;})} ./.cargo/rust-toolchain.toml
              cp -f ${(v-utils.files.rust.rustfmt {inherit pkgs;})} ./rustfmt.toml
              cp -f ${(v-utils.files.rust.deny {inherit pkgs;})} ./deny.toml
              cp -f ${(v-utils.files.gitignore { inherit pkgs; langs = ["rs"];})} ./.gitignore

              cp -f ${readme} ./README.md
            '';

          packages = [
            mold-wrapped
            openssl
            pkg-config
            (rust-bin.fromRustupToolchainFile ./.cargo/rust-toolchain.toml)
          ] ++ pre-commit-check.enabledPackages;
        };
      }
    );
}
