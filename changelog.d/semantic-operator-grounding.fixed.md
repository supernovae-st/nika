- **A schedule the cadence grammar cannot hold is asked, never narrowed.**
  « Every other Monday at 09:00 », « every 2 weeks », « un lundi sur
  deux », « tous les quinze jours », « twice a week » or « every 5
  hours » was READY with a coarse cadence label (`weekly`, `daily`,
  `hourly`) the request never stated. Such a period now keeps no label
  and no cron, and the compile asks its cadence before READY: a
  cadence a schedule binds (« every Monday at 09:00 ») or « manual »
  resolves it; another alternate period is refused as an answer.
  **A number a rule reads is read under one explicit law.** A compiled
  filter, total, average, minimum, maximum or ranking parsed its column
  with jq's lenient `tonumber`: « 1,5 » became 1, « Infinity » an infinite
  value, an empty cell silently dropped its record, and a ranking put
  null last and any text above every number. Every such value is now a
  finite JSON number or a plain decimal text (« 150 », « -3.5 »); any
  other value stops the run with the column and the value named, before
  anything is written. Plans recorded by earlier versions replay
  unchanged.
  **A column holding values that are not numbers is asked, not guessed.**
  « keep only the rows whose amount is above 100 » over a file whose
  amounts include null, true, « n-a » or a list was READY and then failed
  at run; « keep the 2 rows with the highest points » over null points
  ranked them lowest without saying so. The observer now counts the kind
  of every sampled value (never quoting one), and when a column a rule
  reads as a number holds anything else, the compile asks what such a
  record does before READY: skip it (the comparison is false for it, and
  a ranking or total leaves it out) or fail the run naming the value. An
  average, minimum or maximum of no number stops the run instead of
  writing 0 or null, and a skipped record never counts in an average. The
  answer is tied to the file's revision and asked again when the file or
  its kinds change.
  **A stated text also matches its observed canonical spelling.** « keep
  the rows whose statut is livré » over a file spelling « livré » with a
  combining accent (e + U+0301) was READY and wrote a header only:
  equality is byte-exact. Where the observed values of the column hold a
  spelling canonically equivalent (Unicode NFC) to the stated one, the
  filter now also matches exactly that spelling, and the decision records
  it. Equality itself is unchanged: case, compatibility forms and
  spellings the sample did not show still compare byte for byte.
