- **« Augmente le budget du run à 1 dollar et relance » reruns under that ceiling.** A natural
  rerun line reached Run only when the classifier heard a run request and nothing else; a ceiling
  in the same line counted as something else, so the line went to the author, which raised the
  workflow's `max_tokens` instead. A line with a rerun word (« relance », « rerun », « run it
  again ») and an amount the Run reader takes now runs the current workflow once under that
  ceiling, as « run it max $1 » does, including right after a run refused before it started for
  its cost.
