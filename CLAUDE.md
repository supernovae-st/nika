## Diamond architecture essentials

Before modifying any crate, read:

- `docs/architecture/forward-compat-invariants.md` — 8 patterns + 10 rules
  + post-1.0 reservation policy (amended D-2026-06-20-N1 · was "v0.95/v0.100").
  **Non-negotiable.** Every admitted crate passes Gate 12 against this document.
- `docs/architecture/crate-layer-registry.md` — L0 → L4 layer discipline,
  allowed I/O axes, enforcement by `scripts/ci/check-layering.sh`.
- `ROADMAP.md` — the status of the 0.124 line (work stays focused on maintenance, security and the media capabilities workflows need; native video is planned for 0.125; no 1.0 launch planned), then the historical real-semver plan (amended D-2026-06-20-N1 · was "forever-v0.x"). The CHANGELOG top names the current release (never quote a number here — it rots).

### Workspace layout

Crates live under `crates/nika-*` (renamed from `tools/` during the L0-foundation phase).

### Hygiene vectors

Hygiene runs via `scripts/hygiene/check-all.sh` (exit 0 green, 1 yellow
only, 2 red). The script is the list of vectors: layering, security axes,
unsafe dependencies, license consistency, no async in L0, typed errors,
status and release carriers, advisories and more. Do not quote a count
here. Formatting and clippy on the touched crates run pre-commit; the
workspace tests, the whole hygiene suite and the CI ratchets run in the
pre-push gate (`lefthook.yml`, `scripts/pre-push/gate.sh`).

### Forward-compat invariants are non-negotiable

Public API surface is protected by `cargo public-api`, `cargo semver-checks`,
`cargo deny`, and the `#[non_exhaustive]` ratchet on every public type.
Any change to a reserved trait or DTO that is not strictly additive fails
CI. See the invariants doc for the full Gate 12 checklist.
