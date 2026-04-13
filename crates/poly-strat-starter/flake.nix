{
  description = "poly-strat-starter development environment";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs {
          inherit system;
        };
      in
      {
        devShells.default = pkgs.mkShell {
          buildInputs = with pkgs; [
            # Rust toolchain (managed by rustup)
            rustup
            rust-analyzer

            # Native build deps used by trade-server dependency tree
            pkg-config
            cmake
            protobuf
            clang
            openssl

            # Common tools for this starter workflow
            uv
            duckdb
            git
            curl
            just
            jq
            ripgrep

            # macOS compatibility
            libiconv
          ];

          shellHook = ''
            echo "poly-strat-starter development environment"
            echo "Rust: $(rustc --version 2>/dev/null || echo 'not installed - run: rustup default stable')"
            echo "Cargo: $(cargo --version 2>/dev/null || echo 'not installed - run: rustup default stable')"
            echo "uv: $(uv --version 2>/dev/null || echo 'not available')"
          '';

          # For openssl-sys crate
          OPENSSL_DIR = "${pkgs.openssl.dev}";
          OPENSSL_INCLUDE_DIR = "${pkgs.openssl.dev}/include";
          OPENSSL_LIB_DIR = "${pkgs.openssl.out}/lib";

          # For protobuf code generation
          PROTOC = "${pkgs.protobuf}/bin/protoc";

          # C/C++ toolchain for crates with native builds
          CC = "${pkgs.clang}/bin/clang";
          CXX = "${pkgs.clang}/bin/clang++";
          LIBCLANG_PATH = "${pkgs.llvmPackages.libclang.lib}/lib";

          # For pkg-config dependency discovery
          PKG_CONFIG_PATH = "${pkgs.openssl.dev}/lib/pkgconfig";
        };
      });
}
