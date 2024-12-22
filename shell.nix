{ pkgs ? import <nixpkgs> {} }:

pkgs.mkShell {
  buildInputs = with pkgs; [
    openssl.dev
  ];
  nativeBuildInputs = with pkgs; [
    pkg-config
  ];
}

