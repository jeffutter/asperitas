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

          # probe-rs main at the merge of PR #4441 (enable halting debug in the Cortex-M reset
          # catch), which fixes --connect-under-reset on this board. Replace with plain
          # `pkgs.probe-rs-tools` once nixpkgs ships a release containing it (after 0.32.0).
          probeRs = pkgs.probe-rs-tools.overrideAttrs (old: rec {
            version = "0.32.0-unstable-2026-10-11";
            src = pkgs.fetchFromGitHub {
              owner = "probe-rs";
              repo = "probe-rs";
              rev = "5214b96b9a3ba6f42eae69cd55ad3dee49538224";
              hash = "sha256-SJ6Vmv85v07s7+w0WnLF4ZJMgEP64aOgaISHbXRk3eY=";
            };
            # nixpkgs' test skip list targets 0.32.0; six DAP-server tests on main fail in the
            # sandbox and we don't use the DAP server.
            doCheck = false;
            cargoDeps = pkgs.rustPlatform.fetchCargoVendor {
              inherit src;
              hash = "sha256-cyFWzOY6oMTXVvNiz6JBDxpzIXyx97H7thmU2cjXvy4=";
            };
          });

          # `default` is the bench shell. `ci` is the same shell minus the patched probe-rs, which a
          # runner must build from source (about 9.8 of the 10.6 minutes CI spent entering the
          # shell, TASK-072) and no CI gate uses. Every other package, the toolchain above and the
          # shellHook come from this one definition, so `rustc -vV`, the rust-cache key and the
          # elf-check provenance cannot differ between the two.
          mkDevShell = { withProbe }: pkgs.mkShell {
            packages = [
              # --- Rust toolchain (rustc, cargo, clippy, rustfmt + the above) ---
              rustToolchain

              # --- Embedded tooling ---
              pkgs.dfu-util              # probe-free flashing over Seed3 USB-C
              # ELF-based flashing, RTT + defmt logging; drives the `probe-*`
              # targets in firmware/Makefile. Lock pins probe-rs-tools 0.32.0.
              # No udev rules here: this flake ships devShells only, and Darwin
              # needs no rule for an ST-Link (the Linux caveat lives in README).
              # Built from probe-rs main (see probeRs above): stock 0.32.0 arms vector
              # catch without C_DEBUGEN, so --connect-under-reset runs past it.
              pkgs.cargo-binutils        # objcopy to produce raw .bin for DFU

              pkgs.pkg-config

              # --- General tooling ---
              pkgs.lefthook              # git hooks
              pkgs.yq-go                 # mikefarah/yq-go (NOT Python yq)
              pkgs.jq                    # JSON processing for backlog scripts
            ]
            ++ pkgs.lib.optionals withProbe [
              probeRs
            ]
            # Host audio backend. On Linux cpal talks to ALSA and needs the
            # library + its .pc file at build time. On Darwin it uses the
            # CoreAudio/AudioToolbox frameworks, which the standard stdenv
            # already provides — nothing extra to add here.
            ++ pkgs.lib.optionals pkgs.stdenv.hostPlatform.isLinux [
              pkgs.alsa-lib
            ];

            shellHook = ''
              lefthook install >&2
            '';
          };
        in
        {
          default = mkDevShell { withProbe = true; };
          ci = mkDevShell { withProbe = false; };
        });
    };
}
