- **The knowledge door lives in a size-cap member.** `nika-onboard-knowledge` now holds the strict
  admission of a Foundry knowledge release, the releases this build embeds, the catalogue and the
  authoring pack, with their assets and pinned vectors; `nika-onboard` re-exports every item at
  `knowledge::`, so no consumer changes, and keeps `knowledge::pin`. The onboarding crate drops
  from 14,999 to about 11,090 production lines.
