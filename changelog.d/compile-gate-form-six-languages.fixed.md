- **A human gate is read from its form in six languages, and one approval is one
  gate.** « demande-moi confirmation avant d'écrire », « pídeme confirmación
  antes de escribir », « chiedimi conferma prima di scrivere », « frag mich
  bevor du schreibst », « pergunte-me antes de enviar », « solo después de mi
  aprobación » were not read as gates while their English forms were: the gate
  matcher now knows the approval nouns, approval verbs, persons, connectors and
  asking verbs of Spanish, Italian, German and Portuguese, and a clitic person
  glued to the verb (`demande-moi`, `chiedimi`, `pergunte-me`) counts as the
  verb and the person it addresses. A request stating one approval for several
  effects ("only after I say yes: do the POST, then write the receipt")
  compiled to one prompt per effect, so a single human answer could not finish
  the run; the gated effects now share one `approval_review` that lists every
  action and shows the first one's exact content or payload, while two
  approvals each naming one effect stay two gates. A gate read in a language
  whose effect verbs the deterministic reader does not know no longer blocks a
  model proposal that names the effect: the gate finds its effect there.
