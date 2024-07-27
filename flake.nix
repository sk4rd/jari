{
  description = "A development shell for the Jari project";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, flake-utils, nixpkgs }:
    flake-utils.lib.eachDefaultSystem (system:
      let pkgs = nixpkgs.legacyPackages.${system};
      in {
        devShells.default = pkgs.mkShell {
          packages = with pkgs; [
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

        formatter = pkgs.nixfmt;
      });
}
