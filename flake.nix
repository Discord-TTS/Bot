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
              "poise-0.6.1" = "sha256-Cg8Nq4wlun5wksI+5zxYbinrx43IAwB7eA8BcTf+1sE=";
              "serenity-0.12.5" = "sha256-C4CpG/dD7RYapr5OEN01U1AFLZA6jFiQ0jqR9ixx3W0=";
              "serenity-voice-model-0.2.1" = "sha256-X8KEdROcWC3qFTfbLd9iCWmkurB+6CRRzBfTzxEcIDk=";
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

          hardeningDisable = [ "fortify" ];
        };
      in
      tts-utils.mkTTSModule {
        inherit pkgs;
        package = botPkg;
        disableFortify = true;
        extraDevTools = with pkgs; [
          clippy
          rustfmt
          rust-analyzer
        ];
      }
    );
}
