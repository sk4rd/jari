{
  description = "A development shell for the Jari project";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
  };

  outputs =
    inputs@{ self, nixpkgs, ... }:
    let
      system = "x86_64-linux";
      pkgs = import nixpkgs {
        inherit system;
      };
    in
    with pkgs;
    {
      devShells.${system}.default = mkShell {
        packages = [
          # Rust
          cargo
          clippy
          rust-analyzer
          rustc
          rustfmt
          cargo-watch

          # Javascript
          nodePackages_latest.typescript-language-server

          # Nix
          nil
        ];

        shellHook = ''
          git checkout $(git rev-parse --show-toplevel)/.tracking; git pull;
          /bin/sh "$(git rev-parse --show-toplevel)/.tracking/record.sh" clockin;
          rm $(git rev-parse --show-toplevel)/.git/hooks/*; 
          cp $(git rev-parse --show-toplevel)/.tracking/pre-commit $(git rev-parse --show-toplevel)/.git/hooks
          chmod +x $(git rev-parse --show-toplevel)/.git/hooks/pre-commit
        '';
      };

      packages.${system} = rec {
        jari = pkgs.callPackage ./nix { };
        default = jari;
      };

      nixosModules = rec {
        jari = import ./nix/module.nix inputs;
        default = jari;
      };

      formatter.${system} = nixfmt-rfc-style;
    };
}
