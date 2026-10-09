{
  description = "aski — interactive question popups for MCP agents (Wayland, Catppuccin)";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs =
    {
      self,
      nixpkgs,
      flake-utils,
    }:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        pkgs = import nixpkgs { inherit system; };
        # Runtime libs winit dlopens at startup on Wayland (plus GL/Vulkan for iced's wgpu renderer).
        runtimeLibs = with pkgs; [
          wayland
          libxkbcommon
          libGL
          vulkan-loader
        ];
      in
      {
        packages = {
          default = self.packages.${system}.aski;
          aski = pkgs.rustPlatform.buildRustPackage {
            pname = "aski";
            version = "0.1.0";

            src = ./.;
            cargoLock.lockFile = ./Cargo.lock;

            nativeBuildInputs = with pkgs; [
              makeBinaryWrapper
            ];

            postFixup = ''
              wrapProgram $out/bin/aski \
                --prefix LD_LIBRARY_PATH : ${
                  pkgs.lib.makeLibraryPath runtimeLibs
                }
            '';

            meta = with pkgs.lib; {
              description = "Interactive question popups for MCP agents (Wayland, Catppuccin)";
              mainProgram = "aski";
              platforms = platforms.linux;
            };
          };
        };

        devShells.default = pkgs.mkShell {
          # Same libs in the dev shell so `cargo run` works from source.
          LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath runtimeLibs;
          packages = with pkgs; [
            cargo
            rustc
            rust-analyzer
            rustfmt
            clippy
          ];
        };
      }
    );
}
