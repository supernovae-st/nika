- **A conversation's workflow leaves a reasoning model room to answer.**
  A summary on a model that reasons before it answers (deepseek-flash),
  capped at 4096 output tokens with thinking left on, spent the whole
  cap on reasoning: the run ended with no answer, or with one cut short.
  A cap the person did not type that cannot cover the reasoning plus the
  answer, or no cap at all, is now refused before the workflow is
  proposed, with the repairs that reach the model's route: set
  `run.reasoning.effort: low` first where the model documents that
  level, then raise `max_tokens` (at its default effort the model spends
  most of a larger cap reasoning).
