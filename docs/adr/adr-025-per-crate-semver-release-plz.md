---
id: ADR-025
title: "Per-crate semver via release-plz (publishable crates only)"
status: accepted
date: "2026-04-16"
phase: "Phase H — release infrastructure"
deciders: ["@ThibautMelen"]
tags: ["semver", "release-plz", "publish", "versioning"]
affects_crates: ["nika", "nika-sdk"]
affects_layers: ["L5"]
supersedes: []
superseded_by: []
related: ["ADR-022", "ADR-036"]
requires: ["ADR-022"]
enables: []
amends: []
fci: []
inv: []
shadow_zones: []
nika_codes: []
timeline: "v0.81+"
follow_ups: ["set up release-plz GitHub workflow", "yank legacy crates.io versions"]
---

# ADR-025: Per-crate semver + release-plz

> **Implementation note (2026-08-31).** This file records the 2026-04 design,
> but the active
> release authority is the lockstep workspace sweep and tag-triggered train in
> `RELEASING.md`, `scripts/release/wave-sweep.sh`, and
> `.github/workflows/release.yml`. No `release-plz` workflow is installed.

## Context

Workspace currently uses single `version = "0.80.0"` shared across all crates.
With the foundation `publish = false` strategy (ADR-022), only `nika` (binary)
and future `nika-sdk` reach crates.io. Their semver should evolve independently
of internal foundation crate refactors.

## Decision

**Two-tier semver policy:**

1. **Foundation crates (`publish = false`)** — keep workspace-shared version
   `version.workspace = true`. Not published, semver irrelevant for ecosystem.

2. **Publishable crates (`nika`, future `nika-sdk`)** — independent semver via
   `release-plz` GitHub workflow. Each crate has its own version field, bumped
   semantically based on conventional-commit messages affecting that crate.

**Initial publishable versions:**
- `nika@0.80.0` — binary republish from legacy v0.47.1 (yank old, fresh start
  with diamond architecture once binary is wired)
- `nika-sdk@0.1.0` — first publication when v0.90 reaches stable surface

**release-plz** workflow runs on push-to-main, opens a PR with version bumps
+ CHANGELOG entries derived from commits, auto-publishes on PR merge.

## Consequences

- ✅ `nika` binary version reflects user-facing API stability, not internal
  refactor churn
- ✅ `nika-sdk` ships independently — bug-fix release doesn't bump main binary
- ✅ release-plz handles changelog generation + GitHub releases automatically
- ❌ One-time workflow setup (~2h, one commit)
- ⚠️ Conventional commit discipline becomes load-bearing (already mostly true
  via commitlint hook)

## Crates.io legacy cleanup (handled in Phase B.0)

```
cargo yank nika-core@0.74.0      # diamond renames it; legacy abandoned
cargo yank nika-event@0.74.0     # legacy
cargo yank nika-engine@0.47.1    # legacy, rebranded as nika
# nika@0.47.1 stays; republish at v0.80+ when binary is wired
```

## Reference

- release-plz docs: https://release-plz.ieni.dev/
- ADR-022 (publish=false strategy)
- `~/.claude/.../memory/feedback_publish_false_foundation_strategy.md`
