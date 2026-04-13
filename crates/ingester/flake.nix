{
  description = "Flake for ingester";
  
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, rust-overlay, utils }:
    utils.lib.eachDefaultSystem (system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs {
          inherit system overlays;
        };

        # Use nightly rust for feature gates
        rustVersion = pkgs.rust-bin.nightly.latest.default;

        isDarwin = pkgs.stdenv.isDarwin;
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
          
          # macOS specific
          libiconv
        ];
      in
      {
        devShell = with pkgs; mkShell {
          inherit buildInputs;
          
          shellHook = ''
            # Initialize rustup if not already done
            if ! command -v rustup &> /dev/null; then
              rustup-init -y --no-modify-path
            fi

            rustup install nightly
            rustup default nightly
            
            echo "tx_sub development environment"
            echo "Rust version: $(rustc --version)"
            echo "Cargo version: $(cargo --version)"
          '';

          # Environment variables for building dependencies
          RUST_SRC_PATH = "${rustVersion}/lib/rustlib/src/rust/library";
          RUSTUP_TOOLCHAIN = "nightly";
          
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
      }
    );
}
