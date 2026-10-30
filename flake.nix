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
      # The version is shared by every crate through [workspace.package].
      cargoToml = builtins.fromTOML (builtins.readFile ./Cargo.toml);
    in
    {
      devShells.${system}.default = pkgs.mkShell {
        packages = with pkgs; [
          cargo
          rustc
          rustfmt
          clippy
          rust-analyzer
          # License check required by the J0 milestone (deny.toml).
          cargo-deny
          # Runs the graft MCP server declared in .mcp.json through npx.
          # python3 lets node-gyp build tree-sitter when no prebuilt
          # binary matches (the C compiler comes from mkShell's stdenv).
          nodejs_22
          python3
          pkg-config
        ];
        # Libraries required by wgpu / winit on Linux.
        buildInputs = runtimeLibs;
        LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath runtimeLibs;
      };

      # `nix run github:vabyz971/Cygnus` / `nix build .#cygnus-photo`:
      # Cygnus Photo built from source, wrapped with its runtime libraries.
      packages.${system} = rec {
        cygnus-photo = pkgs.rustPlatform.buildRustPackage {
          pname = "cygnus-photo";
          version = cargoToml.workspace.package.version;
          src = pkgs.lib.cleanSource ./.;
          cargoLock.lockFile = ./Cargo.lock;
          cargoBuildFlags = [ "-p" "cygnus-photo" ];
          # The test suite needs a GPU and a display: run by the CI instead.
          doCheck = false;
          nativeBuildInputs = with pkgs; [ pkg-config makeWrapper ];
          buildInputs = runtimeLibs;
          postInstall = ''
            wrapProgram $out/bin/cygnus-photo \
              --prefix LD_LIBRARY_PATH : ${pkgs.lib.makeLibraryPath runtimeLibs}
          '';
          meta = {
            description = "Cygnus Photo — open-source non-destructive photo editor";
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
