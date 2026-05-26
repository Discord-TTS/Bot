{

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable-small";
    flake-utils.url = "github:numtide/flake-utils";

    tts-utils.url = "github:Discord-TTS/shared-workflows";
  };

  outputs =
    {
      nixpkgs,
      flake-utils,
      tts-utils,
      ...
    }:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        pkgs = import nixpkgs { inherit system; };
        pkgDesc = (pkgs.lib.importTOML ./Cargo.toml).package;
        botPkg = pkgs.rustPlatform.buildRustPackage {
          pname = pkgDesc.name;
          version = pkgDesc.version;
          meta.mainProgram = pkgDesc.name;

          src = pkgs.lib.sources.cleanSource ./.;
          cargoLock = {
            lockFile = ./Cargo.lock;
            outputHashes = {
              "poise-0.6.1" = "sha256-6NU1UOQUz8WO77Luv7VLp/RL1May65Y7JmMWxaPbgvo=";
              "serenity-0.12.5" = "sha256-tonM8Ixc1dLRmNDjPX54gsyimMLWTiB0/LMHHfQtpNo=";
            };
          };

          env.RUSTC_BOOTSTRAP = "1";
          nativeBuildInputs = with pkgs; [
            mold
          ];

          hardeningDisable = [ "fortify" ];
          checkPhase = ''
            cargo test -p tts_commands
          '';
        };
      in
      tts-utils.mkTTSModule {
        inherit pkgs;
        package = botPkg;
        disableFortify = true;
        extraDockerContents = with pkgs; [ dockerTools.caCertificates ];
        extraDevTools = with pkgs; [
          clippy
          rustfmt
          cargo-edit
          rust-analyzer
        ];
      }
    );
}
