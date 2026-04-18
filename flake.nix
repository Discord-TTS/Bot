{

  inputs = {
    nixpkgs-unpatched.url = "github:NixOS/nixpkgs/nixos-unstable-small";
    flake-utils.url = "github:numtide/flake-utils";

    tts-utils.url = "github:Discord-TTS/shared-workflows";
  };

  outputs =
    {
      nixpkgs-unpatched,
      flake-utils,
      tts-utils,
      ...
    }:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        pkgsUnpatched = import nixpkgs-unpatched { inherit system; };
        nixpkgs = pkgsUnpatched.applyPatches {
          name = "nixpkgs-patched";
          src = nixpkgs-unpatched;
          patches = [
            # Bumps Rust to 1.94.1
            (pkgsUnpatched.fetchpatch2 {
              url = "https://github.com/NixOS/nixpkgs/pull/503830.patch";
              hash = "sha256-wCC+IJqZJTxyteNV//+M/+31xh+Pns0tQ/qgDkSOQaQ=";
            })
            # Bumps Rust to 1.95
            (pkgsUnpatched.fetchpatch2 {
              url = "https://github.com/NixOS/nixpkgs/pull/510674.patch";
              hash = "sha256-7s7lcdifXJd31XTECFoPwrZE3UrUiMn9dHSHPXHn4dc=";
            })
          ];
        };

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
              "serenity-0.12.5" = "sha256-lyleiOtGDFH5zuHu3z4pUEE5lyyJZvZQkzH3pwu2XGA=";
            };
          };

          env.RUSTC_BOOTSTRAP = "1";
          nativeBuildInputs = with pkgs; [
            mold
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
