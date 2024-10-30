{ pkgs }:

with pkgs.lib;

pkgs.rustPlatform.buildRustPackage {
  pname = "jari";
  version = "main";
  cargoLock.lockFile = ./Cargo.lock;
  src = cleanSource ./.;

  meta = {
    description = "Jari - Just a Radio by Individuals";
    homepage = "https://github.com/sk4rd/jari";
    license = licenses.unlicense;
  };
}
