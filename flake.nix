{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable-small";
    flake-utils.url = "github:numtide/flake-utils";
  };
  outputs =
    {
      nixpkgs,
      flake-utils,
      ...
    }:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        pkgs = import nixpkgs { inherit system; };
      in
      {
        devShell = pkgs.mkShell {
          env = {
            RUSTC_BOOTSTRAP = "1";
            RUST_SRC_PATH = pkgs.rustPlatform.rustLibSrc;
          };

          nativeBuildInputs = with pkgs; [
            rustc
            cargo
            clippy
            rustfmt

            mold
            cmake
            rust-analyzer
          ];

          hardeningDisable = [ "fortify" ];
        };
      }
    );
}
