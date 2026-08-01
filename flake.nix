{
  description = "Asperitas — a musically-interactive audio effect for the Daisy Seed3";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs?ref=nixos-unstable";
  };

  outputs = { self, nixpkgs }:
    let
      system = "x86_64-linux";
      pkgs = nixpkgs.legacyPackages.${system};
    in
    {
      devShells.${system}.default = pkgs.mkShell {
        # Minimal for now: just the tooling the backlog automation needs, so the
        # Ralph loops stop depending on whatever happens to be installed on the
        # host. TASK-001 replaces this with the full toolchain — Rust with the
        # thumbv7em-none-eabihf target, dfu-util, probe-rs, cargo-binutils,
        # lefthook, and the ALSA/pkg-config deps cpal needs.
        packages = with pkgs; [
          # backlog/unblocked-todo.sh parses ticket frontmatter with these to work
          # out which tickets have all their dependencies Done.
          yq-go
          jq
        ];
      };
    };
}
