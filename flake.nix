# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#
# `nix run github:supernovae-st/nika` — the zero-gatekeeper install path
# (#388). Builds the same binary the release ships: the `nika` bin target
# of `crates/nika-cli`, `--locked`. The executable is BORN `nika`
# (ADR-135) — no rename here and none at packaging (release.yml stages
# the same file under the same name).
#
# Tests do NOT run here (`doCheck = false`): the full battery gates every
# merge in diamond-ci; the flake is an INSTALL path, not a second CI. The
# build itself is proven by .github/workflows/nix.yml on every PR that
# touches this file or the lockfile.
{
  description = "nika — the sovereign workflow engine for AI (4 verbs · audit-before-run · traces as receipts)";

  inputs = {
    # unstable: the workspace MSRV (rust-version in Cargo.toml) moves with
    # the toolchain; the stable channel's rustc lags too far behind it.
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
  };

  outputs =
    { self, nixpkgs }:
    let
      systems = [
        "x86_64-linux"
        "aarch64-linux"
        "x86_64-darwin"
        "aarch64-darwin"
      ];
      forAllSystems = f: nixpkgs.lib.genAttrs systems (system: f nixpkgs.legacyPackages.${system});
    in
    {
      packages = forAllSystems (pkgs: rec {
        nika = pkgs.rustPlatform.buildRustPackage {
          pname = "nika";
          version = (builtins.fromTOML (builtins.readFile ./Cargo.toml)).workspace.package.version;

          src = self;
          cargoLock.lockFile = ./Cargo.lock;

          # The operator-surface crate only — its dep tree is pure Rust
          # (rustls, no openssl/pkg-config), which is why this derivation
          # needs zero nativeBuildInputs; release.yml builds the same crate
          # on a bare runner with no apt step.
          buildAndTestSubdir = "crates/nika-cli";

          doCheck = false;

          meta = {
            description = "The sovereign workflow engine for AI";
            homepage = "https://nika.sh";
            license = nixpkgs.lib.licenses.agpl3Plus;
            mainProgram = "nika";
          };
        };
        default = nika;
      });

      apps = forAllSystems (pkgs: {
        default = {
          type = "app";
          program = "${self.packages.${pkgs.system}.nika}/bin/nika";
        };
      });
    };
}
