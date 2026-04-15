## Diamond architecture essentials

Before modifying any crate, read:

- `docs/architecture/forward-compat-invariants.md` — 8 patterns + 10 rules
  + v0.95/v0.100 reservation policy. **Non-negotiable.** Every admitted
  crate passes Gate 12 against this document.
- `docs/architecture/crate-layer-registry.md` — L0 → L4 layer discipline,
  allowed I/O axes, enforcement by `scripts/ci/check-layering.sh`.
- `ROADMAP.md` — forever-v0.x plan, v0.81 seams, v0.90 milestones.

### Workspace layout

Crates live under `crates/nika-*` (renamed from `tools/` at v0.81).

### Hygiene vectors

Hygiene runs via `scripts/hygiene/check-all.sh`. The vector count grows
from **10 (v0.80) to 21 (v0.81)** once Batch C lands: layering,
security-axes, cargo-geiger unsafe-deps, env-example parity, license
consistency, no-async-in-L0, catalog owned-strings, kernel-no-spawn,
no-`Box<dyn Error>`, cancel-safety docs, and case-insensitive collisions.
P0/P1 vectors run pre-commit; P2/P3 run pre-push.

### Forward-compat invariants are non-negotiable

Public API surface is protected by `cargo public-api`, `cargo semver-checks`,
`cargo deny`, and the `#[non_exhaustive]` ratchet on every public type.
Any change to a reserved trait or DTO that is not strictly additive fails
CI. See the invariants doc for the full Gate 12 checklist.
