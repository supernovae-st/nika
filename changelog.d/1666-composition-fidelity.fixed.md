- **Every effect that names content has a producer, and a revision check has a
  source.** The composer's rule 10 now reads every effect, not only a write: a
  `send`, `publish`, `notify`, `create`, `update` or other effect whose target
  or evidence names produced content (a reply, a report, a summary, a digest, a
  brief, a note, a message, a blurb, a draft, a translation, a recap, a memo,
  and their French, Spanish, Italian, Portuguese and German forms) needs a
  draft, an extract or a compute step, or, under a copy cue (copy, forward,
  verbatim, tel quel, as is, attach…), a source step whose material it carries
  unchanged; the reason names the noun and the target. Rule 11 refuses a
  `revision_check` obligation without a lookup to reread. The strict HOT
  admission applies the same two laws, so a deterministic reading that would
  send a reply nobody drafted escalates instead of inventing the reply.
- **A numeric rule is an operation, and a structured destination receives its
  format.** A digit beside a comparison cue (greater than, above, below, at
  least, plus grand que, supérieur à, inférieur à, mayor que, minore di,
  größer als, `>`, `≥`…) that a proposal demoted to prompt guidance is
  promoted to a compute stage anchored in the request, inserted right after
  the sources so the draft sees the computed rows, and withdrawn from the
  prompt; a size unit (words, lignes, bullets…), an attempt or turn bound and
  the concurrency bound stay guidance. A `.csv`, `.yaml` or `.toml` destination
  whose content is data gets a `nika:convert` stage (`<stem>_csv`) feeding the
  write instead of JSON text; a `.json` destination stays JSON; prose stays
  prose. A structured source is decoded only when a code rule, an endpoint
  payload or a structured write consumes it, and a `compute_summary` stage
  (`{count, totals}` over the numeric columns, rounded) gives every count or
  total a language step claims a deterministic anchor.
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
- **A lookup by identifier binds its JSON file and selects the one record.** A
  lookup detail that names one JSON file and an identifier token (digits
  beside letters, `-`, `_` or `#`, or an email; never a bare number, a date, a
  path or a URL) binds the file without a directory question, keeps the
  identifier as `const.<slug>_id` (`ticket T-4471 in ./data/tickets.json` →
  `const.ticket_id`), asks only which field holds it (`const.<slug>_id_field`,
  a stable question), and selects the record with a jq that reads an array
  directory by field and an object directory by key. A literal lookup is the
  corpus: no `inputs.item` and no `inputs.record_id` is declared, and the
  classification, the draft and every endpoint payload see the record alone,
  never the whole file. The revision recheck reuses the same selector.
