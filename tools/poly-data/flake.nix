{
  description = "poly-data: backtest data management CLI";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs { inherit system; };
        pythonEnv = pkgs.python312.withPackages (ps: with ps; [
          pip
          setuptools
          wheel
        ]);
      in
      {
        devShells.default = pkgs.mkShell {
          buildInputs = with pkgs; [
            # Python
            pythonEnv
            uv

            # Build tools
            pkg-config

            # Native libs (for duckdb wheels)
            stdenv.cc.cc.lib
            zlib

            # Utilities
            git
            curl

            # macOS compat
            libiconv
          ];

          shellHook = ''
            export LD_LIBRARY_PATH="${pkgs.stdenv.cc.cc.lib}/lib:${pkgs.zlib}/lib:$LD_LIBRARY_PATH"
            if [ -f ".venv/bin/activate" ]; then
              source .venv/bin/activate
            fi
          '';
        };
      });
}
