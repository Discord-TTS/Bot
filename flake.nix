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

        sharedAttrs = {
          env.RUSTC_BOOTSTRAP = true;
        };

        pkgDesc = (pkgs.lib.importTOML ./Cargo.toml).package;
        ttsBotPackage = pkgs.rustPlatform.buildRustPackage (
          sharedAttrs
          // {
            pname = pkgDesc.name;
            version = pkgDesc.version;
            meta.mainProgram = pkgDesc.name;
            env.RUSTC_BOOTSTRAP = true;

            src = pkgs.lib.sources.cleanSource ./.;
            hardeningDisable = [ "fortify3" ];
            nativeBuildInputs = [
              pkgs.mold
              pkgs.cmake
            ];
            cargoLock = {
              lockFile = ./Cargo.lock;
              allowBuiltinFetchGit = true;
            };
          }
        );
      in
      with tts-utils.mkTTSModule {
        inherit pkgs;
        package = ttsBotPackage;
      };
      {
        inherit nixpkgs package dockerImage;
        devShell = devShell.overrideAttrs (
          sharedAttrs
          // {
            hardeningDisable = [ "fortify" ];
          }
        );
      }
    );
}
