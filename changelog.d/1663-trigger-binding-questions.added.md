- **A schedule's binding values are asked beside the candidate, as choices.** A request that
  states a cadence (« every weekday at 8, … ») now carries four optional questions next to
  `requested_trigger`: `trigger.timezone` (text), `trigger.missed` and `trigger.overlap`
  (closed choices spelled by the project and cadence grammars: `rattraper` ·
  `rattraper-une-fois` · `sauter`; `sauter` · `file` · `remplacer`) and `trigger.ceiling`
  (a positive number, USD). They never block a ready candidate; an answer is admitted against
  the grammar's spellings and echoed on `requested_trigger` (`timezone` · `missed` ·
  `overlap` · `ceiling`), a wrong one is a finding and the question stays. The wire gains
  `"type": "choice"` with `options: [{key, label}]` on such questions, and
  `provenance.suggested_file` (a kebab file name for whoever saves the candidate). Ten
  contract fixtures under `crates/nika-compile/tests/fixtures/contract/` record the
  document of each supported state; a test refuses any drift from the live wire.
