- **Native authoring reads the engine's output conventions.** The native and sketch system
  messages now carry an engine-owned section after the spec's card
  (`crates/nika-compile/assets/native_output_conventions.md`). A text file is lines that each
  end with a newline, read with the assembler's own `LINES` law and written back with
  `join("\n") + "\n"`. A shape the request names is the shape written: « la liste des
  <champs> » or "only <field>" is an array of values, and « par <clé> » or "per <key>" is one
  object keyed by each value, unless the request names another shape. An unnamed shape stays
  the source's, and the seat states its choice in `notes`. Receipts name the digest
  (`identity.conventions_sha256`).
