- **A Claude Code session over ACP no longer reads as silent while it
  thinks.** Every Claude Code session the engine opens (the authoring and
  `infer:` one-shot profile, and the `agent:` seat) now asks the adapter
  for adaptive thinking with its summary displayed
  (`_meta.claudeCode.options.thinking`), unless a thinking option is
  already named. Recent models otherwise default to signature-only
  thinking whose text is empty, which the adapter never streams, so a long
  thinking turn showed no frame at all until its answer, and could be cut
  as silent. This changes what is displayed, not the model, effort, budget
  or tools; thought frames stay counted, never stored as text, and the
  silence allowance is unchanged. Codex and other adapters send nothing
  new.
