{
  description = "uniflowed: Unified Toolchain for Flow (React)";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
  };

  outputs = { nixpkgs, ... }:
    let
      systems = [
        "aarch64-darwin"
        "aarch64-linux"
        "x86_64-darwin"
        "x86_64-linux"
      ];
      forAllSystems = f:
        nixpkgs.lib.genAttrs systems (system:
          let
            pkgs = import nixpkgs { inherit system; };
          in
          f pkgs);
    in
    {
      devShells = forAllSystems (pkgs: {
        default = pkgs.mkShell {
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
            rustup
          ];

          RUSTUP_TOOLCHAIN = "1.98.0";

          shellHook = ''
            rustup toolchain install 1.98.0 --profile minimal --component rustfmt,clippy >/dev/null
            echo "uniflowed dev shell: Rust $(rustc --version | cut -d' ' -f2), Bun $(bun --version)"
          '';
        };
      });
    };
}
