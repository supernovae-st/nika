- **A Session's work snapshot says what the last answer line did.** When a
  line is typed for an authoring question, the snapshot now names that
  question and the act the session performed: the value it bound and how
  the line gave it (as typed, an offered key, or a part chosen by the one
  reading call), the round dropped, the request restated in words, the
  question still waiting with the reason, or the refusal's class. It is
  recorded where the act happened, so a bound value stays bound when the
  compile that follows does not finish, and it is absent after an aside, a
  read-only command or a line for another prompt. Both the native
  workspace and the HTTP host read it from the same snapshot.
