{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay.url = "github:oxalica/rust-overlay";
    flake-utils.url = "github:numtide/flake-utils";
    pre-commit-hooks.url = "github:cachix/git-hooks.nix";
    v-flakes.url = "github:valeratrades/v_flakes?ref=v1.5";
  };

  outputs = { self, nixpkgs, rust-overlay, flake-utils, pre-commit-hooks, v-flakes }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        overlays = builtins.trace "flake.nix sourced" [ (import rust-overlay) ];
        pkgs = import nixpkgs {
          inherit system overlays;
        };
        #NB: can't load rust-bin from nightly.latest, as there are week guarantees of which components will be available on each day.
        rust = pkgs.rust-bin.selectLatestNightlyWith (toolchain: toolchain.default.override {
          extensions = [ "rust-src" "rust-analyzer" "rust-docs" "rustc-codegen-cranelift-preview" ];
        });

        pre-commit-check = pre-commit-hooks.lib.${system}.run (v-flakes.files.preCommit { inherit pkgs; });
        manifest = (pkgs.lib.importTOML ./v_exchanges/Cargo.toml).package;
        pname = manifest.name;
        stdenv = pkgs.stdenvAdapters.useMoldLinker pkgs.stdenv;

        rs = v-flakes.rs {
          inherit pkgs rust;
          build = {
            deny = true;
            workspace = let deprecate_by = "v1.0.0"; in {
              "./v_exchanges" = [{ deprecate = { by_version = deprecate_by; force = true; }; }];
              "./v_exchanges_adapters" = [{ deprecate = { by_version = deprecate_by; force = true; }; }];
              "./v_exchanges_api_generics" = [{ deprecate = { by_version = deprecate_by; force = true; }; }];
            };
          };
          style = {
            format = true;
            modules = {
              prefer_ahash = true;
            };
          };
        };
        github = v-flakes.github {
          inherit pkgs pname rs;
          enable = true;
          lastSupportedVersion = "nightly-2025-10-12";
          jobs = {
            default = true;
            # not sure I like the `default`s option on the interface after this now {{{1
            warnings.exclude = [ "rust-doc" ];
            warnings.augment = [{ name = "rust-doc"; args = { package = "v_exchanges"; }; }];
            #,}}}1
          };
        };
        readme = v-flakes.readme-fw { inherit pkgs pname; defaults = true; lastSupportedVersion = "nightly-1.92"; rootDir = ./.; badges = [ "msrv" "crates_io" "docs_rs" "loc" "ci" ]; };
        combined = v-flakes.utils.combine [ rs github readme ];
      in
      {
        packages =
          let
            rustc = rust;
            cargo = rust;
            rust-analyzer = rust;
            miri = rust;
            rustPlatform = pkgs.makeRustPlatform {
              inherit rustc cargo rust-analyzer miri stdenv;
            };
          in
          {
            default = rustPlatform.buildRustPackage {
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

        devShells.default = pkgs.mkShell {
          inherit stdenv;
          shellHook =
            pre-commit-check.shellHook +
            combined.shellHook +
            ''
              cp -f ${(v-flakes.files.treefmt) {inherit pkgs;}} ./.treefmt.toml
            '';
          buildInputs = with pkgs; [
            mold
            openssl
            pkg-config
            rust
            (writeShellScriptBin "test_all" "cargo t && cargo t --examples")
          ] ++ pre-commit-check.enabledPackages ++ combined.enabledPackages;

          env.RUST_BACKTRACE = 1;
          env.RUST_LIB_BACKTRACE = 0;
        };
      }
    );
}
