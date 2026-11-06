{
  description = "A Nix-flake-based Rust development environment";

  inputs = {
    nixpkgs.url = "https://flakehub.com/f/NixOS/nixpkgs/0.1"; # unstable Nixpkgs
    fenix = {
      url = "https://flakehub.com/f/nix-community/fenix/0.1";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    { self, ... }@inputs:

    let
      supportedSystems = [
        "x86_64-linux"
        "aarch64-linux"
        "aarch64-darwin"
      ];
      forEachSupportedSystem =
        f:
        inputs.nixpkgs.lib.genAttrs supportedSystems (
          system:
          f {
            inherit system;
            pkgs = import inputs.nixpkgs {
              inherit system;
              # Android SDK artifacts are unfree and their archive names vary,
              # so the dev shell opts into unfree packages wholesale.
              config.allowUnfree = true;
              overlays = [
                inputs.self.overlays.default
              ];
            };
          }
        );
    in
    {
      overlays.default = final: prev: {
        rustToolchain =
          with inputs.fenix.packages.${prev.stdenv.hostPlatform.system};
          combine (
            (with stable; [
              clippy
              rustc
              cargo
              rustfmt
              rust-src
            ])
            ++ [
              # Rust standard library for every Android ABI Tauri builds.
              targets."aarch64-linux-android".stable.rust-std
              targets."armv7-linux-androideabi".stable.rust-std
              targets."i686-linux-android".stable.rust-std
              targets."x86_64-linux-android".stable.rust-std
            ]
          );
      };

      devShells = forEachSupportedSystem (
        { pkgs, system }:
        let
          ndkVersion = "27.2.12479018";

          androidSdk = (pkgs.androidenv.override { licenseAccepted = true; }).composeAndroidPackages {
            platformVersions = [
              "35"
              "36"
            ];
            buildToolsVersions = [
              "35.0.1"
              "36.0.0"
            ];
            includeNDK = true;
            inherit ndkVersion;
            includeCmake = false;
            includeEmulator = false;
            includeSystemImages = false;
          };

          androidSdkRoot = "${androidSdk.androidsdk}/libexec/android-sdk";
          ndkRoot = "${androidSdkRoot}/ndk/${ndkVersion}";
        in
        {
          default = pkgs.mkShell {
            packages = with pkgs; [
              rustToolchain
              bun
              cargo-tauri
              cairo
              gdk-pixbuf
              glib
              gtk3
              jdk17
              libsoup_3
              openssl
              pango
              pkg-config
              webkitgtk_4_1
              cargo-deny
              cargo-edit
              cargo-watch
              rust-analyzer
              self.formatter.${system}
            ];

            env = {
              # Required by rust-analyzer
              RUST_SRC_PATH = "${pkgs.rustToolchain}/lib/rustlib/src/rust/library";

              # Android toolchain, consumed by `cargo tauri android` and Gradle.
              JAVA_HOME = "${pkgs.jdk17.home}";
              ANDROID_HOME = androidSdkRoot;
              ANDROID_SDK_ROOT = androidSdkRoot;
              NDK_HOME = ndkRoot;
              ANDROID_NDK_ROOT = ndkRoot;
            };
          };
        }
      );

      formatter = forEachSupportedSystem ({ pkgs, ... }: pkgs.nixfmt);
    };
}
