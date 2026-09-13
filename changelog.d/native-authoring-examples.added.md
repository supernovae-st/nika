- **Eight bounded native authoring skeletons and their shared lessons.**
  Add bounded batch, record validation, snapshot comparison, deduplication,
  field projection, parallel review, optional-file recovery and aggregation.
  Their filled lessons are generated from the same canonical sources used by
  the documentation, with golden outputs and runtime boundary cases.
  `nika new` names a skeleton's filled precedent, and MCP `nika_template`
  accepts `filled: true` with an exact template name. The native authoring
  gauntlet now runs in CI. Generic filler words no longer count as evidence
  for selecting a specific workflow.
