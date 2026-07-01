## Diamond architecture essentials

Before modifying any crate, read:

- `docs/architecture/forward-compat-invariants.md` — 8 patterns + 10 rules
  + post-1.0 reservation policy (amended D-2026-06-20-N1 · was "v0.95/v0.100").
  **Non-negotiable.** Every admitted crate passes Gate 12 against this document.
- `docs/architecture/crate-layer-registry.md` — L0 → L4 layer discipline,
  allowed I/O axes, enforcement by `scripts/ci/check-layering.sh`.
- `ROADMAP.md` — real-semver plan toward 1.0 (amended D-2026-06-20-N1 · was "forever-v0.x"). Current 0.91.0 (latest release · main on 0.92.0-dev) → 1.0.0 launch → 1.x adds remaining crates.

### Workspace layout

Crates live under `crates/nika-*` (renamed from `tools/` during the L0-foundation phase).

### Hygiene vectors

Hygiene runs via `scripts/hygiene/check-all.sh`. The vector count grows
from **10 to 21** once Batch C lands: layering,
security-axes, cargo-geiger unsafe-deps, env-example parity, license
consistency, no-async-in-L0, catalog owned-strings, kernel-no-spawn,
no-`Box<dyn Error>`, cancel-safety docs, and case-insensitive collisions.
P0/P1 vectors run pre-commit; P2/P3 run pre-push.

### Forward-compat invariants are non-negotiable

Public API surface is protected by `cargo public-api`, `cargo semver-checks`,
`cargo deny`, and the `#[non_exhaustive]` ratchet on every public type.
Any change to a reserved trait or DTO that is not strictly additive fails
CI. See the invariants doc for the full Gate 12 checklist.
