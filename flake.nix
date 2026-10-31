{

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable-small";
    flake-utils.url = "github:numtide/flake-utils";

    tts-utils.url = "github:Discord-TTS/shared-workflows";
  };

  outputs =
    {
      self,
      nixpkgs,
      flake-utils,
      tts-utils,
      ...
    }:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        lib = pkgs.lib;
        pkgs = import nixpkgs { inherit system; };
        pkgDesc = (lib.importTOML ./Cargo.toml).package;
        botPkg = pkgs.rustPlatform.buildRustPackage {
          pname = pkgDesc.name;
          version = pkgDesc.version;
          meta.mainProgram = pkgDesc.name;

          src = lib.sources.cleanSource ./.;
          cargoLock = {
            lockFile = ./Cargo.lock;
            outputHashes = {
              "poise-0.7.0" = "sha256-D9pdVL4qZGJHUmUR/SQHyiimAEHT2NVuHfiNL7j9qHI=";
              "serenity-0.12.5" = "sha256-c7kQ1EAJee7DjeWMp4yRlMmYt23Jxm1Op0qY2S4buNs=";
            };
          };

          env = {
            RUSTC_BOOTSTRAP = "1";
            VERGEN_GIT_SHA = self.rev or (lib.removeSuffix "-dirty" self.dirtyRev);
            VERGEN_GIT_DIRTY = if builtins.hasAttr "dirtyRev" self then "true" else "false";
          };

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
