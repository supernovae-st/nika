- **The public executable is born `nika` (ADR-135).** The bin target of the
  `nika-cli` package is named `nika` and the package runs it by default:
  `cargo build --release --locked --bin nika` yields `target/release/nika`, the
  release builds, signs and stages that same file (no rename at packaging), the
  flake installs it without a `postInstall` rename, the tap, the image and
  cargo-binstall are unchanged, the integration tests read
  `CARGO_BIN_EXE_nika`, and the scripts that probe a built binary probe
  `target/{debug,release}/nika`. Two gates keep the identity from splitting
  again: `tests/public_binary_identity.rs` (cargo metadata · exactly one bin
  target named `nika`, owned by `nika-cli`, `default_run`) and the ratchet
  `check-public-binary.sh` (the manifests, the release line, the flake, the
  tests · self-tested against seven mutants). The « `nika` is reserved for the
  L5 composition root » comment is retired: a future root takes the target by
  moving it, never by adding a second executable.
