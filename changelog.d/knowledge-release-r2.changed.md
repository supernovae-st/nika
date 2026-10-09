- **New authoring draws on the r2 knowledge release.** The build embeds
  the r2 Foundry release whole, from its own directory, and admits it
  against its issued snapshot and policy before anything is collected;
  CLI, Session and Serve authoring use it when nothing else is named.
  The a8 and R3 releases stay embedded, so a project pinned to either
  is still admitted against its own identity, and a pin naming a
  release this build does not carry is refused as an identity mismatch.
