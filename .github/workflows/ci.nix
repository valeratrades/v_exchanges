{ pkgs, workflow-parts, lastSupportedVersion }:
let
  shared-base = import workflow-parts.shared.base;
  shared-jobs = {
    tokei = import workflow-parts.shared.tokei;
  };
  rust-base = import workflow-parts.rust.base;
  rust-jobs-errors = {
    tests = import workflow-parts.rust.tests { inherit lastSupportedVersion; };
    miri = import workflow-parts.rust.miri;
  };
  rust-jobs-warn = {
    doc = import workflow-parts.rust.doc;
    clippy = import workflow-parts.rust.clippy;
    machete = import workflow-parts.rust.machete;
    sort = import workflow-parts.rust.sort;
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
  errors = (pkgs.formats.yaml { }).generate "" (
    pkgs.lib.recursiveUpdate base {
      name = "Errors";
      inherit (shared-base) permissions;
      inherit (rust-base) env;
      jobs = pkgs.lib.recursiveUpdate rust-base.jobs rust-jobs-errors;
    }
  );
  warnings = (pkgs.formats.yaml { }).generate "" (
    pkgs.lib.recursiveUpdate base {
      name = "Warnings";
      inherit (shared-base) permissions;
      jobs = pkgs.lib.recursiveUpdate (pkgs.lib.recursiveUpdate shared-jobs rust-jobs-warn) rust-base.jobs;
    }
  );
}
