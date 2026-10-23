- **An installed trigger whose runs would miss their API keys is never said to run on its own.**
  After the activation card had warned that a scheduled run would not see the person's keys, the
  install still answered « Installé et actif · tournera tout seul ». It now says the trigger is
  started, that every scheduled run fails until it sees the named keys, and how to give them: a
  file of `KEY=value` lines passed to `arm --emit launchd --env-file <file> --write`.
