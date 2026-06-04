{
  description = "Rust development environment for gimbap";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    fenix = {
      url = "github:nix-community/fenix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { self, nixpkgs, flake-utils, fenix }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs { inherit system; };

        targetToolchain = fenix.packages.${system}.toolchainOf {
          channel = "1.90.0";
          sha256 = "sha256-SJwZ8g0zF2WrKDVmHrVG3pD2RGoQeo24MEXnNx5FyuI=";
        };

        rustToolchain = fenix.packages.${system}.combine [
          targetToolchain.rustc
          targetToolchain.cargo
          targetToolchain.rustfmt
          targetToolchain.clippy
          targetToolchain.rust-analyzer
          targetToolchain.rust-src
        ];

      in
      {
        devShells.default = pkgs.mkShell {
          buildInputs = [
            rustToolchain
            pkgs.prek
            pkgs.pkg-config
            pkgs.openssl
          ];

          shellHook = ''
            echo "🦀 Rust (Fenix) Environment for Gimbap Loaded!"
            rustc --version
            prek --version
          '';
        };
      }
    );
}
