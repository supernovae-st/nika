- **`nika check` refuses a `nika:date` zone the run would refuse.**
  `tz: "Paris"` checked clean, then the run failed at its first
  `nika:date` task with `NIKA-BUILTIN-DATE-001 · unknown tz: Paris`. A
  literal `tz:` is now judged against the embedded IANA time-zone
  database the builtin itself reads, and an unknown zone is refused with
  the run's code, naming the zone when one is obvious (`Paris` or
  « L'heure de Paris » → `Europe/Paris`). A templated `tz:` is left to
  the run. The ARGS rung now prints every refused literal value with its
  own code and the checker's wording, where it said « has no `<arg>`
  arg ».
