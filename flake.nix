{
  description = "A development shell for the Jari project";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-23.11";

  outputs = { self, nixpkgs }:
    let
      allSystems = [ "x86_64-linux" "aarch64-linux" ];
      forAllSystems = f:
        nixpkgs.lib.genAttrs allSystems
        (system: f { pkgs = import nixpkgs { inherit system; }; });
    in {
      devShells = forAllSystems ({ pkgs }: {
        default = pkgs.mkShell {
          packages = with pkgs; [
            # Rust
            cargo
            clippy
            rust-analyzer
            rustc
            rustfmt
            # Javascript
            nodePackages_latest.typescript-language-server
            # Nix
            nil
            nixfmt
          ];
          shellHook = "git pull; /bin/sh \"$(git rev-parse --show-toplevel)/tracking/record.sh\" clockin";
        };
      });
    };
}
