- **An author stuck repeating one failing call is told to change it.** On Gemini 3.1 Pro the
  author copied the note Nika leaves in its history in place of an earlier candidate's document
  and sent it as a new candidate 80 times in 15 minutes, each refused « needs the complete
  document in `source` ». That refusal now names the copied note and the two ways out (the whole
  document, or `candidate_edit`), and a call that returns the same reply three times in a row is
  told that repeating it cannot change the reply. Nothing stops the conversation: no quota.
