- **A stated approval now holds its effects to a human's typed yes.** Both approval laws
  (Law 3, an approval the reader binds to an effect, and Law 3b, a final approval it binds to
  none: « prépare son envoi à … · demande-moi avant de l'envoyer ») accept an approved effect
  only when its own `when:` affirms the answer of a confirm `nika:prompt` whose output it
  binds whole, judged with the analyzer's refusal substitution (Check's affirmative-consent
  reading, NEP-0020): false on a « no » and on a skipped prompt, not dead on a « yes ». An
  `after:` wait, a guard a « no » or a skip passes (`== false`, `!`, `||`, `!= false`), a
  quoted name, an unrelated binding, a derived value, a choice or input prompt, a
  `default: true`, an `on_error:` or a fan-out on the prompt are refused as APPROVAL ORDER;
  no prompt is MISSING APPROVAL. A stated final gate is no longer called INVENTED GATE over
  the final effects, and still is over an earlier one. An `mcp:` tool, a process, a child
  workflow or an agent with effect tools, whose effect the compiler cannot read, is held to
  the same guard or refused as APPROVAL UNPROVEN. `nika-check-analyzer` gains
  `gates::human_confirm` and `gates::affirmed_by`.
