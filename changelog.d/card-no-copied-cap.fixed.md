- **The native authoring card's example no longer caps its `infer`
  step at `max_tokens: 600`.** The card's own rule says to write
  `max_tokens` only when the person names a token bound and otherwise to
  leave the default to the compiler, which sizes it once the workflow's
  model is known; its example contradicted the rule, and an author that
  copied the 600 left a model that reasons before it answers no room to
  answer.
