- **Typed `agent:` final-text repairs respect `max_tokens_total`.** When
  the final text does not match the task schema, each additional repair
  checks the cumulative token budget, including usage from earlier
  `nika:done` repairs. An exhausted budget returns `NIKA-AGENT-002` with
  the last assistant text and observed usage instead of requesting another
  answer. This partial output stays separate from the `nika:done` result
  being validated and updates with each text repair.
  A conforming answer still succeeds at or above the budget. This also
  covers result-less, string and null `nika:done` answers that need the
  final-text repair path.
