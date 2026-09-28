- **A rule reads only keys its source is known to hold.** `nika compile`
  now grounds every key a typed rule reads over one file and records how
  in the decision (`decision.grounding`): a CSV header, keys seen in the
  bounded sample of a JSON file (never proof that a key is absent), the
  request's own column list, or a human answer. A request word the file
  does not spell (« id » over `sku`, « quantity » over `units`,
  « expiry » over `expires`) is asked as a choice of the observed keys,
  never mapped by similarity; the answer only counts for the revision
  of the file it was given for, and is asked again after the file
  changes. With nothing observed, a key the request only names is asked
  for its exact spelling instead of being written into the program. A
  key present in only some records keeps the request open until it
  says what happens to the records lacking it. Keys observed in the
  request's own language stay READY with no question. `nika compile`
  now observes the files a request states (headers and keys, bounded,
  under the working directory) without an authoring model too, so a
  keyless compile stays READY when the file holds the key; library and
  Serve callers pass what they observed as knowledge.
