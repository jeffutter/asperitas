{
  description = "Asperitas — a musically-interactive audio effect for the Daisy Seed3";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs?ref=nixos-unstable";
    rust-overlay.url = "github:oxalica/rust-overlay";
    rust-overlay.inputs.nixpkgs.follows = "nixpkgs";
  };

  outputs = { self, nixpkgs, rust-overlay }:
    let
      systems = [ "x86_64-linux" "aarch64-linux" "x86_64-darwin" "aarch64-darwin" ];

      forAllSystems = f: nixpkgs.lib.genAttrs systems (system: f {
        inherit system;
        pkgs = import nixpkgs {
          inherit system;
          overlays = [ rust-overlay.overlays.default ];
        };
      });
    in
    {
      devShells = forAllSystems ({ pkgs, ... }:
        let
          # One toolchain covering host and embedded target. `override` folds
          # the thumbv7em-none-eabihf std into the same sysroot as rustc and
          # clippy-driver — building them separately gives clippy-driver its own
          # sysroot without the embedded std, and `cargo clippy` then fails with
          # E0463 "can't find crate for `core`".
          # llvm-tools-preview supplies the llvm-objcopy that cargo-binutils wraps.
          rustToolchain = pkgs.rust-bin.stable.latest.default.override {
            targets = [ "thumbv7em-none-eabihf" ];
            extensions = [ "rust-src" "rust-analyzer" "llvm-tools-preview" ];
          };
        in
        {
          default = pkgs.mkShell {
            packages = [
              # --- Rust toolchain (rustc, cargo, clippy, rustfmt + the above) ---
              rustToolchain

              # --- Embedded tooling ---
              pkgs.dfu-util              # probe-free flashing over Seed3 USB-C
              pkgs.probe-rs-tools        # flash + defmt/RTT logging (when ST-Link arrives)
              pkgs.cargo-binutils        # objcopy to produce raw .bin for DFU

              pkgs.pkg-config

              # --- General tooling ---
              pkgs.lefthook              # git hooks
              pkgs.yq-go                 # mikefarah/yq-go (NOT Python yq)
              pkgs.jq                    # JSON processing for backlog scripts
            ]
            # Host audio backend. On Linux cpal talks to ALSA and needs the
            # library + its .pc file at build time. On Darwin it uses the
            # CoreAudio/AudioToolbox frameworks, which the standard stdenv
            # already provides — nothing extra to add here.
            ++ pkgs.lib.optionals pkgs.stdenv.hostPlatform.isLinux [
              pkgs.alsa-lib
            ];

            shellHook = ''
              lefthook install
            '';
          };
        });
    };
}
