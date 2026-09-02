{
  description = "uniflowed: Unified Toolchain for Flow (React)";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    nixpkgs-x86_64-darwin.url = "github:NixOS/nixpkgs/nixpkgs-26.05-darwin";
    rust-overlay.url = "github:oxalica/rust-overlay";
    rust-overlay.inputs.nixpkgs.follows = "nixpkgs";
  };

  outputs = { nixpkgs, nixpkgs-x86_64-darwin, rust-overlay, ... }:
    let
      systems = [
        "aarch64-darwin"
        "aarch64-linux"
        "x86_64-darwin"
        "x86_64-linux"
      ];
      pkgsFor = system:
        import (if system == "x86_64-darwin" then nixpkgs-x86_64-darwin else nixpkgs) {
          inherit system;
          overlays = [ rust-overlay.overlays.default ];
        };
      forAllSystems = f:
        nixpkgs.lib.genAttrs systems (system: f (pkgsFor system));
    in
    {
      devShells = forAllSystems (pkgs: {
        default =
          let
            rustToolchain = pkgs.rust-bin.stable."1.98.0".default.override {
              extensions = [ "clippy" "rustfmt" ];
            };
          in
          pkgs.mkShell {
            packages = with pkgs; [
              bun
              cargo-nextest
              git
              gh
              just
              nodejs_24
              nixpkgs-fmt
              openssl
              pkg-config
              rustToolchain
              why3
              z3
            ];

            shellHook = ''
              echo "uniflowed dev shell: Rust $(rustc --version | cut -d' ' -f2), Bun $(bun --version)"
            '';
          };
      });
    };
}
