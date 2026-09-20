- **A workflow compiled from a fresh intent produces the artefact the intent
  asked for.** A clean-shell gate on seven fresh intents produced zero requested
  artefacts; the defects were the deterministic assembler's, not the model's.
  The corpus is now what the steps consume: a read file, a fetched page or a
  looked-up record is the material, and an incoming `item` input exists only
  when the request is invoked per item or supplies no other material, so a
  file → transform → write workflow no longer declares a phantom required
  `item` nor pastes it into every prompt. Every anchor law (extract and draft)
  checks against that whole corpus instead of `inputs.item` alone, so a
  correct extraction no longer dies on `NIKA-BUILTIN-ASSERT-001`. A path
  literal is one path-shaped token: prose never becomes a constant or a permit,
  a directory asks for a glob (`const.source_glob`) and fans out through
  `nika:glob`, a placeholder asks for the exact files (`const.source_paths`),
  and several explicit files become a bounded `for_each` read (with
  `max_parallel` taken from an "at most N at a time" constraint) folded into
  one document with a heading per file, one permit entry per path, never a
  `;`-joined literal. A verb whose target names a local file (merge … into
  `./out/x.md`) is a write to that file, never an endpoint question. Every
  write effect is its own task bound to the nearest upstream result of its
  kind (a structured target takes data, a prose target takes text), two writes
  to two files stay two effects, a file the request names that nothing writes
  is a question (`effect.write_<stem>.include`), and a write with nothing
  upstream is a finding instead of an invented input. A structured source
  (JSON, CSV, YAML, TOML) is decoded once for code rules, and the
  `const.rule_expression` question names the exact input object the rule
  receives.
