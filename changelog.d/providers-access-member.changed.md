- **The access plan, the probe facts and route identity live in a size-cap member.**
  `nika-providers-access` now holds `resolve_access`, `route_identity`, the probe's data types and
  `canonical_provider`; `nika-providers` re-exports every one at its historical path, so no consumer
  changes. The provider crate drops from 14,991 to about 12,800 production lines.
