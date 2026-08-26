- **Every shipped example now runs in CI, not just audits.** The 53-example
  pack gained a manifest-driven family gate (`pack_examples_family`): each
  example is launched through `nika try` (or a staged `nika run` for the
  human-gated ones) under the offline mock seat, hermetically. Skips are
  explicit and counted — the seatbelt-confined git/cargo examples run as
  known-fail legs that fail the gate the day they heal.
