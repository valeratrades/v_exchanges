{ pkgs ? import <nixpkgs> {} }:

pkgs.mkShell {
  buildInputs = with pkgs; [
    openssl
    openssl.dev
		patchelf
  ];
  nativeBuildInputs = with pkgs; [
    openssl.dev
    pkg-config
  ];
  PKG_CONFIG_PATH = "${pkgs.openssl.dev}/lib/pkgconfig";
	#wtf, nix
	shellHook = ''
		patchelf --set-rpath ${pkgs.openssl.out}/lib target/debug/v_exchanges
		'';
}
