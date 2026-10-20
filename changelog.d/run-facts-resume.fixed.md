- **CLI resumes and answered TTY continuations retain the continued run's scheduled instant,
  its source and missed-slot facts.** Fact-reading tasks keep their valid checkpoint identities
  and prompt answers bind the question originally shown, while each leg retains fresh execution
  identity and admission. Legacy traces without these facts keep the existing behavior; malformed
  recorded facts refuse.
