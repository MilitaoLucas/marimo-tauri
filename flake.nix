{
  description = "Marimo tauri";
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };
  outputs =
    {
      self,
      nixpkgs,
      rust-overlay,
    }:
    let
      system = "x86_64-linux";
      pkgs = import nixpkgs {
        inherit system;
      };
    in
    {

      devShells.${system}.default = pkgs.mkShell {
        nativeBuildInputs = with pkgs; [
          pkg-config
          wrapGAppsHook4
          cargo
          cargo-tauri # Optional, Only needed if Tauri doesn't work through the traditional way.
          nodejs # Optional, this is for if you have a js frontend
          rustc # Needed for dev server (npm tauri dev)
          uv
          python313
        ];

        buildInputs = with pkgs; [
          librsvg
          webkitgtk_4_1
        ];

        shellHook = ''
          export XDG_DATA_DIRS="$GSETTINGS_SCHEMAS_PATH" # Needed on Wayland to report the correct display scale
          export LD_LIBRARY_PATH="${
            pkgs.lib.makeLibraryPath [
              pkgs.stdenv.cc.cc.lib
              pkgs.zlib
              pkgs.libGL
              pkgs.libglvnd
              pkgs.xorg.libX11
              pkgs.xorg.libXrender
              pkgs.xorg.libXext
              pkgs.xorg.libSM
              pkgs.xorg.libICE
              pkgs.openbabel
            ]
          }:$LD_LIBRARY_PATH"

          # Keep uv's venv project-local and out of $HOME
          export UV_PROJECT_ENVIRONMENT=".venv"

          # First-run bootstrap
          if [ ! -d .venv ]; then
            echo "Creating uv project environment..."
            uv sync
          fi

          echo "Environment ready. Run: uv run jupyter lab"
        '';
      };
    };
}
