- **`nika check` no longer says a run would fail for a model its catalog does not
  list.** The advisory line under MODELS, the oracle's `models_catalog_warnings` and
  the MCP `nika_check` now read: not one of the provider's models in this binary's
  catalog snapshot, a typo or a model newer than the snapshot, and the provider
  decides at run time. It stays a warning, never a refusal.
