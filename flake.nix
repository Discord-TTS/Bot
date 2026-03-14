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
              "audiopus_sys-0.2.2" = "sha256-epzB54105Iihrfyj1HZNGSLOaihLw4rUZsT+rw/sXZs=";
              "poise-0.6.1" = "sha256-qCTEkOWCpKgEXCt7apg+tiScE+X0Br0giTNNBxqNCs0=";
              "serenity-0.12.5" = "sha256-hZ5/42VjPoMqCW6+dPNMb7Awn4CUXvvn/j8PabJtGSg=";
              "serenity-voice-model-0.2.1" = "sha256-atagMk8lVxIO5lt1QBBqofBAEGpCRgW0VqPgtdCJ/zM=";
              "songbird-0.5.0" = "sha256-wacSNkIjA1rsENNPbo/KVDfoMXllrr+vA2pmPxsNzEs=";
            };
          };

          env.RUSTC_BOOTSTRAP = "1";
          buildInputs = with pkgs; [
            libopus
          ];
          nativeBuildInputs = with pkgs; [
            mold
            cmake
          ];

          doCheck = false;
          hardeningDisable = [ "fortify" ];
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
