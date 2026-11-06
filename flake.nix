{
  description = "GlideDesk — Lightweight, Zero-Latency Multi-Device KVM (Tauri v2 + Rust)";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay.url = "github:oxalica/rust-overlay";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, rust-overlay, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs {
          inherit system overlays;
        };

        rustToolchain = pkgs.rust-bin.stable.latest.default.override {
          extensions = [ "rust-src" "rustfmt" "clippy" ];
        };

        nativeBuildInputs = with pkgs; [
          pkg-config
          cargo
          rustc
          rustToolchain
          nodejs
          npmHooks.npmInstallHook
          wrapGAppsHook
          gobject-introspection
          cargo-tauri
        ];

        buildInputs = with pkgs; [
          glib
          gtk3
          libsoup_3
          webkitgtk_4_1
          openssl
          sqlite
          dbus
          librsvg
          libx11
          libxi
          libxtst
          libxext
          libxrandr
          libxfixes
          pulseaudio
          udev
        ];

      in
      {
        packages.default = pkgs.stdenv.mkDerivation rec {
          pname = "glidedesk";
          version = "3.2.0";

          src = self;

          nativeBuildInputs = nativeBuildInputs;
          buildInputs = buildInputs;

          # If you have npm dependencies, ensure cargo/tauri can fetch or build
          buildPhase = ''
            runHook preBuild
            export HOME=$(mktemp -d)
            
            # Build frontend assets if needed
            npm install
            npm run build

            # Build Tauri app
            cd src-tauri
            cargo tauri build --bundles app,deb,rpm
            runHook postBuild
          '';

          installPhase = ''
            runHook preInstall
            mkdir -p $out/bin
            cp src-tauri/target/release/glidedesk $out/bin/glidedesk
            runHook postInstall
          '';

          meta = with pkgs.lib; {
            description = "Lightweight, Zero-Latency Multi-Device KVM (Tauri v2 + Rust)";
            homepage = "https://github.com/GlideDesk/GlideDesk";
            license = licenses.gpl3Only;
            platforms = [ "x86_64-linux" "aarch64-linux" ];
            mainProgram = "glidedesk";
          };
        };

        devShells.default = pkgs.mkShell {
          nativeBuildInputs = nativeBuildInputs;
          buildInputs = buildInputs;

          shellHook = ''
            export WEBKIT_DISABLE_COMPOSITING_MODE=1
            echo "GlideDesk development environment loaded (Tauri v2 + Rust)."
          '';
        };
      }
    );
}
