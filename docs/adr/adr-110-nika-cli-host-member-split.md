---
id: ADR-110
title: "nika-cli size-cap member split: the host plane descends to nika-cli-host"
status: accepted
date: "2026-07-31"
phase: "pre-1.0 · trust-experience arc"
deciders: ["@ThibautMelen"]
tags: ["architecture", "crates", "split", "size-cap", "host-integration"]
affects_crates: ["nika-cli", "nika-cli-host"]
affects_layers: ["L4"]
supersedes: []
superseded_by: []
related: ["ADR-022", "ADR-023", "ADR-024", "ADR-135"]
requires: []
enables: []
amends: []
fci: []
inv: []
shadow_zones: []
nika_codes: []
timeline: "v0.107"
follow_ups: []
---

# ADR-110: nika-cli size-cap member split — nika-cli-host

## Context

The trust-experience arc (2026-07-31 · 22 P0 closures) grew `nika-cli`
past the 15,000 prod-LOC Diamond invariant: vector 24 measured 17,789
at the pre-push gate. The cap is a locked maintainability budget
(nika-invariants.md), not advisory. Two descents already set the
pattern: the display surface to `nika-display` (2026-07-10) and the
run composer to `nika_runtime::compose` (2026-07-22) — "compute
descends, render stays."

## Decision

Per D-2026-07-09-N1 (a size-cap split is ONE architectural unit in TWO
workspace members), the host-integration plane descends to a new L4
member crate `nika-cli-host`:

- `probe` (client/kit probes · capability levels · receipts)
- `wire` (writers · preview/detected · the clap `WireTarget` seam)
- `doctor` (the receipt surface)
- `welcome` (the front-door glance + SAMPLE)
- `clients_registry` + the vendored `data/clients.registry.yaml`
- `context_envelope` · `retention` · `metrics` · `text`
- `output` (spec §4 exit codes · `VerbOutput` · the OSC-8 link seam)

`nika-cli` re-exports every public item at its historical path
(`verbs::{doctor,probe,welcome,wire}` · `verbs::{VerbOutput, exit}` ·
`metrics` · …), so call sites and the bin dispatch stay untouched. The
`local-infer` feature forwards member-to-member. Tests coupled to
cli-side fixtures (retention×trace-store · welcome-SAMPLE×check)
relocate cli-side and exercise the re-exported paths.

## Alternatives rejected

- `LOC-EXEMPT` markers: the whitelist (codegen · lookup-table ·
  enum-mega) does not cover organic verb growth; exempting would erode
  the ratchet.
- A logic/verb split inside one crate: does not move prod LOC out of
  the crate; churns every verb body for no budget gain.
- Blocking the arc on a redesign: the cap exists to force exactly this
  mechanical descent at the moment of overflow.

## Consequences

- Vector 24 returns GREEN (`nika-cli` ≈ 13.5k prod incl. the #771
  merge; `nika-cli-host` ≈ 4.5k) with real headroom.
- `nika-cli-host` is a member of the `nika-cli` unit, not a new
  architectural unit: same L4 row, `publish = false`, one public
  surface re-exported by the operator crate.
- New `crates/nika-cli-host/public-api.txt` baseline joins the diff
  gate; `crates/nika-cli/public-api.txt` re-baselines (items become
  re-exports).
- The next growth inside `nika-cli` should descend `welcome`'s glance
  data walks before the wall bites again.

## Security boundary

None moved: `nika guard` and the run path stay in `nika-cli`; the
member carries probes/writers/render-adjacent surfaces only. The
engine's authority model (permits · cost · secrets · trace) is
untouched.

## Rollback

Reverse `git mv` + drop the member from workspace `members` and
`nika-cli` deps; the re-export shim keeps the surface stable in both
directions.
