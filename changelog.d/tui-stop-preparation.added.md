- **Stop a preparation, or send a correction, while Nika works.** In the
  terminal session the first `Ctrl+C` during a preparation asks the Session
  to stop it and keeps the conversation; a second press still leaves. `Enter`
  with words in the box asks the same stop and queues them as a correction,
  sent as the next line when the turn ends on the free prompt; a chain of
  corrections is sent one after another. A correction never answers a
  decision: after a proposal, question, gate or choice it returns to the box
  unsent, and after a cost question the box stays empty while the transcript
  keeps the correction whole. Every queued correction is kept in input history
  like a sent line. A Run is never stopped by these keys, and the row says so.
