{
  description = "Automatically generate nixdoc documentation for a source tree";

  inputs = {
    nixpkgs.url = "nixpkgs/nixos-25.05";
    fenix = {
      url = "github:nix-community/fenix/monthly";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    devenv = {
      url = "github:jkaye2012/devenv";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    crane.url = "github:ipetkov/crane";
  };

  outputs =
    {
      self,
      fenix,
      nixpkgs,
      devenv,
      crane,
    }:
    devenv.lib.util.forAllSystems nixpkgs (
      system:
      let
        pkgs = nixpkgs.legacyPackages.${system};
        fenix' = fenix.packages.${system};
        crane' = (crane.mkLib pkgs).overrideToolchain fenix'.complete.toolchain;
        manifest = (pkgs.lib.importTOML ./Cargo.toml).package;
        src = crane'.cleanCargoSource ./.;

        project = devenv.lib.rust.createProject {
          inherit src;

          name = "autonixdoc";
          crane = crane';
        };
      in
      {
        devShells.${system}.default = pkgs.mkShell {
          inherit (manifest) name;

          inputsFrom = [ devenv.devShells.${system}.basic ];

          packages = with pkgs; [
            cargo-show-asm
            fenix'.complete.toolchain
            linuxPackages_latest.perf
            lldb
            nixdoc
          ];

          RUSTDOCFLAGS = "--cfg docsrs";
        };

        checks.${system} = project.checks;
        packages.${system} = project.packages;
      }
    );
}
