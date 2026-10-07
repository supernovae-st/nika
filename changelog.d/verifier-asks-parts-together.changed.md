- **The verifier asks the parts of a request together.** When it
  doubted a workflow, the verifier asked each part of the request one
  after another, waiting for each answer before sending the next. A
  decision service now receives the parts of one step all at once,
  each still its own question, and a model seated for decisions answers
  them in one request whose answer names each part by its id. The
  answers are read in the same order as before, so what is judged,
  recorded and repaired is unchanged; only the waiting is shorter. A
  part whose answer was sent but never read, because an earlier
  question got no answer, is recorded as sent.
