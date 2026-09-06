---
id: ADR-135
title: "The public executable is born nika: one bin target with the public name, no packaging rename, no second executable"
status: accepted
date: "2026-09-04"
phase: "pre-1.0 · one door"
deciders: ["@ThibautMelen"]
tags: ["architecture", "build", "release", "one-door"]
affects_crates: ["nika-cli"]
affects_layers: ["L4", "L5"]
supersedes: []
superseded_by: []
related: ["ADR-110", "ADR-130"]
requires: []
enables: []
amends: []
fci: []
inv: []
shadow_zones: []
nika_codes: []
timeline: ""
follow_ups: ["when an L5 composition root ships, it takes the `nika` bin target by moving `[[bin]]` (same name), never by adding a second executable"]
---

# ADR-135 · The public executable is born `nika`

## Context

An external consumer integrating the engine (a benchmark harness, 2026-09-04)
ran the command the product's name suggests, `cargo build --release --locked
--bin nika`, and cargo refused: the workspace had no bin target named `nika`.
The executable's identity was split across six surfaces that disagreed with
the seventh. Clap said `nika` (`crates/nika-cli/src/main.rs` ·
`#[command(name = "nika", bin_name = "nika")]`), the release tarball, the
Homebrew formula (`bin.install "nika"`), the Dockerfile, the flake's
`mainProgram` and every page of the docs said `nika`, while the Cargo bin
target said `nika-cli`, so `release.yml` renamed the artifact at packaging
(`cp target/<triple>/release/nika-cli stage/nika`), `flake.nix` renamed it
again (`mv $out/bin/nika-cli $out/bin/nika`), four scripts probed
`target/*/nika-cli`, and forty integration tests read
`CARGO_BIN_EXE_nika-cli`.

The split was deliberate once: a 2026-06 comment on the bin target reserved
the name `nika` for a future L5 composition root (« this dev-named bin is the
seed it will compose »). Three months later the seed IS the product's
executable, shipped under the public name in every channel, and the
reservation only produced a rename in every packaging path and a wrong first
command for every newcomer. One Door's law applies to the build as much as to
the run: many doors (cargo, the tarball, the tap, the flake, the image, the
prompt), one name, one artifact, one path to reality.

## Decision

The bin target of the `nika-cli` package is named `nika`, and the package
runs it by default:

```toml
[package]
name        = "nika-cli"     # the layer name (L4) stays
default-run = "nika"

[[bin]]
name = "nika"
path = "src/main.rs"
```

Consequences written into the tree, in the same change:

- `cargo build --release --locked --bin nika` yields `target/release/nika`;
  `release.yml` builds `--bin nika`, signs `target/<triple>/release/nika` and
  stages that file under the same name (a copy into the staging dir, never a
  rename); `flake.nix` has no `postInstall` rename; the Homebrew formula, the
  Dockerfile and cargo-binstall (`bin-dir = "nika"`) were already speaking the
  public name and are unchanged.
- Every integration test reads `env!("CARGO_BIN_EXE_nika")`; the scripts that
  probe a built binary (`check-funnel.sh`, `check-taught-commands.sh`,
  `structured-live-battery.sh`, the kit's `check-on-edit.sh`) probe
  `target/{debug,release}/nika`.
- No second executable. A `nika-cli` alias binary is explicitly rejected: one
  door means one executable, and no external consumer ever depended on the
  Cargo target name (the only consumer that typed it was our own release
  machinery, which is what this ADR deletes).
- The identity is gated twice, from both sides of the build. Cargo's own
  reading is `crates/nika-cli/tests/public_binary_identity.rs`: it cannot
  compile unless the target is named `nika`, and it asks `cargo metadata` for
  exactly one bin target named `nika` in the workspace, owned by `nika-cli`,
  with `default_run = "nika"`. The packaging reading is the ratchet
  `scripts/ci/check-public-binary.sh` (the CI matrix and the pre-push gate):
  the manifests including cargo's auto-discovered bins, `default-run`, the
  release build line, the staging copy, and zero surviving `nika-cli` build,
  package, install or test paths. The ratchet proves itself before it judges
  (`test-public-binary.sh` · one clean tree, seven mutants, one unjudgeable
  root).
- The L5 reservation is retired. The layer registry keeps L5 as the future
  composition root that will OWN the `nika` bin target by moving `[[bin]]`
  (same name, same path to reality); ownership may move, the identity does
  not. A stable public identity outranks a Cargo target name held for an
  architecture that has not shipped.

## Consequences

### Positive
- A newcomer or a harness types the command the product's name implies and
  gets `target/release/nika`; the docs, the tarball and the build agree.
- Two rename sites (release.yml · flake.nix) and four path probes are gone;
  the artifact is the build output, byte for byte.
- A drift back is caught before merge on two independent legs (tests · ratchet).

### Negative
- Forty test files change one identifier; a fork that still reads
  `CARGO_BIN_EXE_nika-cli` fails to compile until it follows (one sed).
- `cargo build -p nika-cli` prints `Compiling nika-cli` and emits `nika`; the
  package name and the executable name differ on purpose (the layer name vs
  the public name), which the manifest comment says in one line.

### Neutral
- `nika-catalog-verify` keeps its own bin target (an internal tool, its own
  name); the gate counts `nika` alone.
- Historical documents (`CHANGELOG.md`, `docs/plans/*`) keep their
  `nika-cli` build paths as the record of what was true then.

## Evidence / Affected code

- `crates/nika-cli/Cargo.toml` — `default-run` + the `[[bin]] name = "nika"`
- `crates/nika-cli/tests/public_binary_identity.rs` — cargo's own reading
- `scripts/ci/check-public-binary.sh` · `scripts/ci/test-public-binary.sh` —
  the packaging reading and its mutants (`.github/workflows/diamond-ci.yml`
  ratchet matrix · `scripts/hooks/run-ci-ratchets.sh`)
- `.github/workflows/release.yml` — `--bin nika` · the signed path · the
  staging copy
- `flake.nix` — the rename removed
- `scripts/ci/check-funnel.sh` · `scripts/hygiene/check-taught-commands.sh` ·
  `scripts/test/structured-live-battery.sh` ·
  `.agents/plugins/nika/scripts/check-on-edit.sh` — the probes
- `docs/architecture/crate-layer-registry.md` · `DIAMOND.md` · `ROADMAP.md` ·
  `CONTRIBUTING.md` — L5 re-stated as ownership, never identity

## Alternatives considered

- **Keep `nika-cli` and rename at packaging (the status quo).** Rejected: a
  split identity that only the release machinery could resolve; the first
  command a stranger types fails.
- **Ship both `nika` and `nika-cli` executables.** Rejected: one door, one
  executable; no consumer needs the alias, and two names on PATH invite the
  wrong one into a script.
- **Rename the package to `nika`.** Rejected: the package name is the layer
  name (`nika-cli`, L4, ADR-110's unit with `nika-cli-host` and `nika-trace`);
  moving it churns every path dependency and every baseline for no public
  gain, and a future L5 crate may still want `nika` as a package name.
