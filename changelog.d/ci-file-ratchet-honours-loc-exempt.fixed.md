- **The CI file ratchet honours `LOC-EXEMPT`, as the hygiene dashboard does.** A source file
  with a whitelisted marker (codegen, lookup-table, enum-mega) on one of its first 5 lines may
  hold up to 5,000 lines in CI too, where it used to be refused above 2,500. Both gates read one
  file-size policy and state the same verdict for the same file.
