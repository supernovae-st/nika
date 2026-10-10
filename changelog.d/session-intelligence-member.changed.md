- **The session's intelligence, reasoners, line router and authoring door
  now live in the `nika-session-intelligence` member crate.** A structural
  split (ADR-150) that brings `nika-session` back under its production-size
  wall: every `nika_session::{authoring, intelligence, reasoner, turn}` path
  and root re-export names the same items, and no behaviour changes.
