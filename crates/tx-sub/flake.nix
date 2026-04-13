{
  description = "Flake for tx_sub - Solana transaction subscription service";
  
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
            # Rust toolchain with rustup
            rustup
            rust-analyzer

            # Build tools required for dependencies
            pkg-config
            cmake
            protobuf
            clang

            # SSL/TLS support (for rustls, redis, gRPC clients)
            openssl

            # Required for Solana SDK and gRPC
            curl

            # JavaScript runtime
            bun

            # macOS specific
            libiconv
          ];

          shellHook = ''
            echo "tx_sub development environment"
            echo "Rust toolchain managed by rustup (see rust-toolchain.toml)"
            echo "Rust version: $(rustc --version 2>/dev/null || echo 'not installed - rustup will install on first use')"
            echo "Cargo version: $(cargo --version 2>/dev/null || echo 'not installed - rustup will install on first use')"

            # Set up kubectl configuration
            export KUBECONFIG="$PWD/.kube/config"
            echo "KUBECONFIG set to: $KUBECONFIG"
          '';

          # For openssl-sys crate
          OPENSSL_DIR = "${pkgs.openssl.dev}";
          OPENSSL_INCLUDE_DIR = "${pkgs.openssl.dev}/include";
          OPENSSL_LIB_DIR = "${pkgs.openssl.out}/lib";

          # For protobuf compilation
          PROTOC = "${pkgs.protobuf}/bin/protoc";

          # C compiler configuration
          CC = "${pkgs.clang}/bin/clang";
          CXX = "${pkgs.clang}/bin/clang++";
          LIBCLANG_PATH = "${pkgs.llvmPackages.libclang.lib}/lib";

          # For pkg-config to find dependencies
          PKG_CONFIG_PATH = "${pkgs.openssl.dev}/lib/pkgconfig";
        };
      });
}