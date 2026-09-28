- **ARM readiness no longer implies activation.** The report distinguishes a registered
  schedule from a captured workflow with missing or invalid required inputs. `arm --json`
  exposes bounded readiness, binding sources and firing evidence. Both reports inspect
  history without creating sidecars or repairing caches.
