- **A finished run's card lists its steps, models and files.** Below a run's story, the
  terminal's card now opens on how the run ended in the theme's own state glyph (`✔ Ran in
  16.5s · 14 tasks · $0.034 · 1 model call`, `✖ Failed after …`), then lists each step
  with its glyph, wall time, model and cost (`✖ notify · failed: the webhook answered 500`
  in the failure colour, beside its words; `⊘ tidy · not run`), what each model cost and
  how often it was called, and every file the run reported writing with its path
  (`→ ./out/digest.md (write)`). A long run lists its first twelve steps and every
  failure, then counts the rest; unknown times, models or costs are left out.
