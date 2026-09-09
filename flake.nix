{
  description = "uniflowed: Unified Toolchain for Flow (React)";

  # This is the only flake in the repository. There was a second one under
  # `tools/nix/` for a while, holding a devShell that had drifted from this one
  # — it carried `nixpkgs-fmt` that this shell did not and lacked the OpenTofu
  # this shell has — and nothing said which of the two a contributor was meant
  # to use. `.envrc` used that one, `formal.yml` used that one, and `README.md`
  # told you to run `nix develop`, which is this one. Three answers to one
  # question is not a preference, it is three shells to keep correct.

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

      # The toolchain is *read from* `rust-toolchain.toml` rather than named
      # again here.
      #
      # It used to say `rust-bin.stable."1.98.0"`, in both this file and the
      # `tools/nix` shell, and that was not a version that had fallen behind —
      # it was a channel this workspace cannot use at all. 23 crates in Meta's
      # Flow Rust port declare `#![feature(box_patterns)]`, `uf_cli` reaches
      # every one of them through `uf_check`, and an unstable feature attribute
      # is a hard error on any stable compiler. So `nix build .#uf` could not
      # have produced a binary on any day since it was written, and
      # `nix develop` handed a contributor a shell in which the very next line
      # of `README.md` — `cargo build --release --bin uf` — cannot succeed.
      #
      # Nothing caught it because nothing ran it: the only Nix in CI was the
      # `Formal` job, which enters a shell to run `why3` and never builds a
      # crate. `.github/workflows/nix.yml` is the answer to that half.
      #
      # This is the answer to the other half. `rust-toolchain.toml` is what
      # `rustup`, `dtolnay/rust-toolchain` in CI, and every contributor's
      # `cargo` already obey, and reading it here makes this flake obey the
      # same file rather than a copy of its answer. The pin cannot drift from
      # the pin, because there is now one pin.
      rustToolchainFor = pkgs: pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;

      ufFor = pkgs:
        let
          rustToolchain = rustToolchainFor pkgs;
          rustPlatform = pkgs.makeRustPlatform {
            cargo = rustToolchain;
            rustc = rustToolchain;
          };
        in
        rustPlatform.buildRustPackage {
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

          # What `tools/upstream/sync.sh` does, done with the pinned input in
          # place of the shallow clone.
          #
          # Only `rust_port/` is taken, because that is all the sixteen path
          # dependencies in `Cargo.toml` reach, and it is what the sync script
          # checks out.
          #
          # The patches are the part that used to be missing. This step was a
          # bare `ln -s ${flow} upstream/flow`, which is the pinned commit and
          # nothing else — but a checkout of this port is defined as the pinned
          # commit *plus* `tools/upstream/patches/flow/`, on every machine and
          # in every CI job, and `tools/upstream/sync.sh` refuses to finish
          # without them. A Nix build that skipped them would still compile and
          # still pass its tests, and would answer `uf check` differently from
          # every other build of the same commit — which is exactly the silent
          # failure that directory's README exists to prevent. A symlink also
          # cannot be patched, so this is a copy.
          postPatch = ''
            rm -rf upstream/flow
            mkdir -p upstream/flow
            # The same five subtrees `tools/upstream/sync.sh` sparse-checks out,
            # and for the reason its comment gives: `flow_flowlib` `include_str!`s
            # Flow's own library definitions from `lib/`, `prelude/` and `tslib/`,
            # which sit *beside* `rust_port` rather than inside it, and
            # `evals/flow-typed/environment` carries the globals that are in
            # neither. Copying `rust_port` alone builds every crate that does not
            # embed a libdef and stops at the one that does, with
            # `couldn't read .../lib/core.js`.
            #
            # `the_flake_copies_what_the_sync_checks_out` in
            # `tools/ci/nix-first-class.sh` holds this list equal to that one.
            for subtree in rust_port lib prelude tslib evals; do
              if [ -e ${flow}/$subtree ]; then
                cp -R ${flow}/$subtree upstream/flow/$subtree
              fi
            done
            chmod -R u+w upstream/flow

            for patchFile in tools/upstream/patches/flow/[0-9][0-9][0-9][0-9]-*.patch; do
              echo "upstream/flow: applying ''${patchFile##*/}"
              patch -p1 -d upstream/flow --batch --forward < "$patchFile"
            done
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
            platforms = systems;
          };
        };

      # `programs.uf.enable`, for NixOS, nix-darwin and home-manager. The three
      # differ only in the list a package goes into, so the options are written
      # once and the `config` half is the argument.
      ufModule = installedBy: { config, lib, pkgs, ... }:
        let cfg = config.programs.uf;
        in
        {
          options.programs.uf = {
            enable = lib.mkEnableOption "uf, the unified toolchain for Flow (React)";
            package = lib.mkOption {
              type = lib.types.package;
              default = self.packages.${pkgs.stdenv.hostPlatform.system}.uf;
              defaultText = lib.literalExpression "uf.packages.\${system}.uf";
              description = ''
                The uf package to install. Installs `uf`, `ufr` and `ufx`.

                uf still needs a JavaScript host — Node.js or Bun — because
                Vite and your test bodies run there. It is not one of this
                package's dependencies because which host a project uses is
                the project's choice; `uf info` prints the one it found.
              '';
            };
          };
          config = lib.mkIf cfg.enable (installedBy cfg.package);
        };
    in
    {
      # The output a consumer who is not this repository actually wants: one
      # line in their own `nixpkgs` and `pkgs.uf` exists. It composes
      # `rust-overlay` in rather than requiring the consumer to know that uf is
      # built with a pinned nightly, which is uf's problem and not theirs.
      overlays.default = nixpkgs.lib.composeManyExtensions [
        rust-overlay.overlays.default
        (final: _prev: { uf = ufFor final; })
      ];

      nixosModules.default = ufModule (package: {
        environment.systemPackages = [ package ];
      });

      # nix-darwin takes the same option, which is why this is the same module.
      darwinModules.default = ufModule (package: {
        environment.systemPackages = [ package ];
      });

      homeManagerModules.default = ufModule (package: {
        home.packages = [ package ];
      });

      packages = forAllSystems (_system: pkgs:
        let uf = ufFor pkgs;
        in
        {
          default = uf;
          inherit uf;
        });

      apps = forAllSystems (system: _pkgs:
        let
          uf = self.packages.${system}.uf;
          app = name: {
            type = "app";
            program = "${uf}/bin/${name}";
            meta = { inherit (uf.meta) description license homepage; };
          };
        in
        {
          default = app "uf";
          uf = app "uf";
          ufr = app "ufr";
          ufx = app "ufx";
        });

      checks = forAllSystems (system: pkgs: {
        # The real proof, and the expensive one: the binary builds from this
        # tree with the pinned toolchain and the upstream patches applied.
        package = self.packages.${system}.uf;

        # And a cheap one, so that `nix flake check` on a laptop says something
        # useful in a second rather than only after a full Rust build.
        nix-fmt = pkgs.runCommand "check-nixpkgs-fmt" { nativeBuildInputs = [ pkgs.nixpkgs-fmt ]; } ''
          nixpkgs-fmt --check ${./flake.nix}
          touch $out
        '';
      });

      devShells = forAllSystems (_system: pkgs:
        let
          rustToolchain = rustToolchainFor pkgs;
        in
        {
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
              rustToolchain
              (pkgs.opentofu or pkgs.terraform)
              why3
              z3
            ];

            shellHook = ''
              # Both on stderr. A banner on stdout is not a greeting, it is
              # corruption: `nix develop . --command rustc --version` returns the
              # banner instead of the version, and so does every script that asks
              # this shell a question. The Dev shell check in `nix.yml` is what
              # noticed, by reading its own banner back as a compiler version.
              echo "uniflowed dev shell: Rust $(rustc --version | cut -d' ' -f2), Node $(node --version), Bun $(bun --version)" >&2
              echo "next: tools/upstream/sync.sh && cargo build --release --bin uf" >&2
            '';
          };
        });

      formatter = forAllSystems (_system: pkgs: pkgs.nixpkgs-fmt);
    };
}
