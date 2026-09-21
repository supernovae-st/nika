- **Per-file drafts fan out and fold back with one heading per file.** A
  request that distributes its draft over the read files (a heading word
  beside a distributive or file-name cue, or a draft clause or trigger led by
  "for each", "pour chaque", "para cada") is structure now: the fan-out is
  zipped into `{path, text}` items (`draft_items`), the draft runs `for_each`
  item under the concurrency bound with a prompt that sees only
  `${{ item.text }}`, a per-item law judges every body and anchor against its
  own item, and `draft_fold` joins the bodies under `## <file name>` headings
  in item order; the order and heading instructions leave the prompts, and the
  folded `documents` corpus is emitted only when another step reads it.
  Distributive words inside one draft's object with a single file and a single
  length cap stay one draft. A per-item request whose write target is a
  placeholder path (`./out/<name>.md`) is refused with `intent.clarification`
  instead of being lowered to one guessed file. The realized topology is
  recorded as `provenance.decision.shape` (`linear`, `fan_out_fold`,
  `fan_out_fan_in`, `multiple_outputs`, `human_gated`, with its flags).
