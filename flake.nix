{
  description = "uniflowed: Unified Toolchain for Flow (React)";

  inputs = {
    flow = {
      url = "github:facebook/flow/81b0c2a3dd591c66c51167aac341d851932bd9c5";
      flake = false;
    };
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    nixpkgs-x86_64-darwin.url = "github:NixOS/nixpkgs/nixpkgs-26.05-darwin";
    rust-overlay.url = "github:oxalica/rust-overlay";
    rust-overlay.inputs.nixpkgs.follows = "nixpkgs";
  };

  outputs = { self, flow, nixpkgs, nixpkgs-x86_64-darwin, rust-overlay }:
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
        nixpkgs.lib.genAttrs systems (system: f system (pkgsFor system));
      workspace = builtins.fromTOML (builtins.readFile ./Cargo.toml);
      version = workspace.workspace.package.version;
      sourceRoot = toString ./.;
    in
    {
      packages = forAllSystems (system: pkgs:
        let
          rustToolchain = pkgs.rust-bin.stable."1.98.0".default.override {
            extensions = [ "clippy" "rustfmt" ];
          };
          rustPlatform = pkgs.makeRustPlatform {
            cargo = rustToolchain;
            rustc = rustToolchain;
          };
          uf = rustPlatform.buildRustPackage {
            pname = "uf";
            inherit version;

            src = pkgs.lib.cleanSourceWith {
              src = ./.;
              filter = path: type:
                let
                  relPath = pkgs.lib.removePrefix "${sourceRoot}/" (toString path);
                  excludedDirectories = [
                    ".direnv"
                    ".git"
                    "dist"
                    "docs/dist"
                    "infra/cloudflare/.terraform"
                    "node_modules"
                    "target"
                  ];
                  isInExcludedDirectory = directory:
                    relPath == directory || pkgs.lib.hasPrefix "${directory}/" relPath;
                in
                  !(pkgs.lib.any isInExcludedDirectory excludedDirectories
                    || relPath == "docs/router.js"
                    || pkgs.lib.hasSuffix ".tfstate" relPath
                    || pkgs.lib.hasInfix ".tfstate." relPath);
            };

            postPatch = ''
              rm -rf upstream/flow
              mkdir -p upstream
              ln -s ${flow} upstream/flow
            '';

            cargoLock.lockFile = ./Cargo.lock;
            cargoBuildFlags = [ "--package" "uf_cli" "--bins" ];
            cargoCheckFlags = [ "--package" "uf_cli" ];
            nativeBuildInputs = [ pkgs.pkg-config ];
            buildInputs = pkgs.lib.optionals pkgs.stdenv.hostPlatform.isDarwin [
              pkgs.libiconv
            ];

            installPhase = ''
              runHook preInstall
              uf_bin="$(find target -type f -path '*/release/uf' -print -quit)"
              bin_root="$(dirname "$uf_bin")"
              install -Dm755 "$bin_root/uf" "$out/bin/uf"
              install -Dm755 "$bin_root/ufr" "$out/bin/ufr"
              install -Dm755 "$bin_root/ufx" "$out/bin/ufx"
              runHook postInstall
            '';

            meta = {
              description = "Unified Toolchain for Flow (React)";
              homepage = "https://docs.uniflowed.dev";
              license = pkgs.lib.licenses.mit;
              mainProgram = "uf";
            };
          };
        in
        {
          default = uf;
          inherit uf;
        });

      apps = forAllSystems (system: pkgs:
        let
          uf = self.packages.${system}.uf;
        in
        {
          default = {
            type = "app";
            program = "${uf}/bin/uf";
          };
          uf = {
            type = "app";
            program = "${uf}/bin/uf";
          };
          ufr = {
            type = "app";
            program = "${uf}/bin/ufr";
          };
          ufx = {
            type = "app";
            program = "${uf}/bin/ufx";
          };
        });

      checks = forAllSystems (system: pkgs: {
        package = self.packages.${system}.uf;
      });

      devShells = forAllSystems (system: pkgs: {
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
              openssl
              pkg-config
              rustToolchain
              (pkgs.opentofu or pkgs.terraform)
              why3
              z3
            ];

            shellHook = ''
              echo "uniflowed dev shell: Rust $(rustc --version | cut -d' ' -f2), Node $(node --version)"
            '';
          };
      });

      formatter = forAllSystems (system: pkgs: pkgs.nixpkgs-fmt);
    };
}
