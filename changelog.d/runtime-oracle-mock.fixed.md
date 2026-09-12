- **Runtime conformance models.** The check/run verdict oracle preserves a
  fixture's declared root `mock/*` variant on both sides. Its blanket
  `mock/echo` override previously changed the new text-only completion and
  budget probes into tool-calling runs. Live catalog models still run with
  the offline echo override.
