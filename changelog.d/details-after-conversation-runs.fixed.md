- **`/details` says what a conversation built, saved and ran.**
  QA gauntlet q09: after two Saves and three runs it still said « no
  workflow was read in this session yet », because only a compile round
  set what it reads. With no such round, it now names the conversation
  that wrote the workflow, its last revision, the workflow saved, the
  preparation spend and the last run's trace.
