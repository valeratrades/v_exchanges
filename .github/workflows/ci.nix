{ pkgs, workflow-parts, ... }:
let
  shared-base = import workflow-parts.shared.base { inherit pkgs; };
  shared-jobs = {
    tokei = import workflow-parts.shared.tokei { inherit pkgs; };
  };
  rust-base = import workflow-parts.rust.base { inherit pkgs; };
  rustc-versions = [ "nightly" "nightly-2024-10-10" ];
  rust-jobs-errors = {
    tests = import workflow-parts.rust.tests { inherit rustc-versions; };
    doc = import workflow-parts.rust.doc { inherit pkgs; };
    miri = import workflow-parts.rust.miri { inherit pkgs; };
  };
  rust-jobs-warn = {
    machete = import workflow-parts.rust.machete { inherit pkgs; };
    sort = import workflow-parts.rust.sort { inherit pkgs; };
    clippy = import workflow-parts.rust.clippy { inherit pkgs; };
  };
  base = {
    on = {
      push = { };
      pull_request = { };
      workflow_dispatch = { };
    };
  };
in
{
  errors = (pkgs.formats.yaml { }).generate "" (pkgs.lib.recursiveUpdate base {
    name = "Errors";
    inherit (shared-base) permissions;
    inherit (rust-base) env;
    jobs = pkgs.lib.recursiveUpdate rust-base.jobs rust-jobs-errors;
  });
  warnings = (pkgs.formats.yaml { }).generate "" (pkgs.lib.recursiveUpdate base {
    name = "Warnings";
    inherit (shared-base) permissions;
    jobs = pkgs.lib.recursiveUpdate shared-jobs rust-jobs-warn;
  });
}
