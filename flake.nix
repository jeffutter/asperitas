{
  description = "Asperitas — a musically-interactive audio effect for the Daisy Seed3";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs?ref=nixos-unstable";
    fenix.url = "github:nix-community/fenix";
    fenix.inputs.nixpkgs.follows = "nixpkgs";
  };

  outputs = { self, nixpkgs, fenix }:
    let
      system = "x86_64-linux";
      pkgs = nixpkgs.legacyPackages.${system};
      f = fenix.packages.${system};
    in
    {
      devShells.${system}.default = pkgs.mkShell {
        packages = [
          # --- Rust toolchain from fenix ---
          f.stable.cargo
          f.stable.rustc
          f.stable.clippy
          f.stable.rustfmt
          f.stable.rust-analyzer
          f.stable.rust-src

          # thumbv7em-none-eabihf stdlib for the Seed3's Cortex-M7F
          f.targets.thumbv7em-none-eabihf.stable.rust-std

          # llvm-tools (provides rust-objcopy, etc.)
          f.stable.llvm-tools

          # --- Embedded tooling ---
          pkgs.dfu-util              # probe-free flashing over Seed3 USB-C
          pkgs.probe-rs-tools        # flash + defmt/RTT logging (when ST-Link arrives)
          pkgs.cargo-binutils        # objcopy to produce raw .bin for DFU

          # --- Host audio (ALSA + pkg-config so cpal builds) ---
          pkgs.alsa-lib
          pkgs.pkg-config

          # --- General tooling ---
          pkgs.lefthook              # git hooks
          pkgs.yq-go                 # mikefarah/yq-go (NOT Python yq)
          pkgs.jq                    # JSON processing for backlog scripts
        ];
      };
    };
}
