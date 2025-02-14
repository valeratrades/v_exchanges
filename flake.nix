{ pkgs, lastSupportedVersion, jobsErrors, jobsWarnings }:
let
  shared = {
    base = ./shared/base.nix;
    tokei = ./shared/tokei.nix;
  };
  rust = {
    base = ./rust/base.nix;
    tests = import ./rust/tests.nix { inherit lastSupportedVersion; };
    doc = ./rust/doc.nix;
    miri = ./rust/miri.nix;
    clippy = ./rust/clippy.nix;
    machete = ./rust/machete.nix;
    sort = ./rust/sort.nix;
  };
  go = {
    tests = ./go/tests.nix;
    gocritic = ./go/gocritic.nix;
    security_audit = ./go/security_audit.nix;
  };

  pathToFile = path:
    let
      segments = pkgs.lib.splitString "." path;
      category = builtins.head segments;
      name = builtins.elemAt segments 1;
    in
    {
      file = (
        if category == "shared" then shared
        else if category == "rust" then rust
        else if category == "go" then go
        else throw "Unknown category: ${category}"
      ).${name};
      category = category;
    };

  # Group jobs by category and merge them separately
  groupJobsByCategory = paths:
    let
      fileInfos = map pathToFile paths;
      byCategory = pkgs.lib.groupBy (x: x.category) fileInfos;
      mergeCategory = files: pkgs.lib.foldl pkgs.lib.recursiveUpdate { } (map (x: import x.file) files);
    in
    pkgs.lib.mapAttrs (category: files: mergeCategory files) byCategory;

  constructJobs = paths:
    let
      categorizedJobs = groupJobsByCategory paths;
      # Merge categories in specific order: rust first, then others
      rustJobs = categorizedJobs.rust or { };
      sharedJobs = categorizedJobs.shared or { };
      goJobs = categorizedJobs.go or { };
    in
    rustJobs // sharedJobs // goJobs;

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
      permissions = (import shared.base).permissions;
      env = (import rust.base).env;
      jobs = constructJobs jobsErrors;
    }
  );
  warnings = (pkgs.formats.yaml { }).generate "" (
    pkgs.lib.recursiveUpdate base {
      name = "Warnings";
      permissions = (import shared.base).permissions;
      jobs = constructJobs jobsWarnings;
    }
  );
}
