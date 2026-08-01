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
      # Combine host + embedded target into one rustc sysroot.
      # Fenix's f.targets.<target>.stable.rust-std is a separate package;
      # we must merge its lib/rustlib/thumbv7em* into the main rustc.
      # Also combine llvm-tools so cargo-objcopy finds llvm-objcopy.
      rustWithTarget = f.combine [
        f.stable.rustc
        f.targets.thumbv7em-none-eabihf.stable.rust-std
        f.stable.llvm-tools
      ];
    in
    {
      devShells.${system}.default = pkgs.mkShell {
        packages = [
          # --- Rust toolchain from fenix ---
          f.stable.cargo
          rustWithTarget
          f.stable.clippy
          f.stable.rustfmt
          f.stable.rust-analyzer
          f.stable.rust-src

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
