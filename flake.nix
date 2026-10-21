{
  description = "Environnement dev - Suite créative";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
  };

  outputs = { self, nixpkgs }:
    let
      system = "x86_64-linux";
      pkgs = nixpkgs.legacyPackages.${system};
      # Loaded at runtime (dlopen) by winit / wgpu: never found through
      # the FHS paths on NixOS, hence LD_LIBRARY_PATH in the dev shell
      # and in the packaged wrapper.
      runtimeLibs = with pkgs; [
        vulkan-loader
        libxkbcommon
        wayland
        libx11
        libxcursor
        libxi
        libxrandr
      ];
      cargoToml = builtins.fromTOML (builtins.readFile ./apps/photo/Cargo.toml);
    in
    {
      devShells.${system}.default = pkgs.mkShell {
        buildInputs = with pkgs; [
          cargo
          rustc
          rustfmt
          clippy
          rust-analyzer
          # Dépendances requises par wgpu sur Linux
          pkg-config
          vulkan-loader
          libxkbcommon
          wayland
        ];
        LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath runtimeLibs;
      };

      # `nix run github:vabyz971/Cygnus` / `nix build .#cygnus-photo`:
      # Cygnus Photo built from source, wrapped with its runtime libraries.
      packages.${system} = rec {
        cygnus-photo = pkgs.rustPlatform.buildRustPackage {
          pname = "cygnus-photo";
          version = cargoToml.package.version;
          src = pkgs.lib.cleanSource ./.;
          cargoLock.lockFile = ./Cargo.lock;
          cargoBuildFlags = [ "-p" "photo" ];
          # The test suite needs a GPU and a display: run by the CI instead.
          doCheck = false;
          nativeBuildInputs = with pkgs; [ pkg-config makeWrapper ];
          buildInputs = runtimeLibs;
          postInstall = ''
            mv $out/bin/photo $out/bin/cygnus-photo
            wrapProgram $out/bin/cygnus-photo \
              --prefix LD_LIBRARY_PATH : ${pkgs.lib.makeLibraryPath runtimeLibs}
          '';
          meta = {
            description = "Cygnus Photo — open-source photo, vector and layout editor";
            license = pkgs.lib.licenses.gpl3Plus;
            mainProgram = "cygnus-photo";
            platforms = [ system ];
          };
        };
        default = cygnus-photo;
      };

      apps.${system}.default = {
        type = "app";
        program = "${self.packages.${system}.cygnus-photo}/bin/cygnus-photo";
      };
    };
}
