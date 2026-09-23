- **The serialization cue is read on the request's own words, not only on the seat's
  detail.** On sealed lane9 (gpt-5-mini), sv3-11 « write just the number, nothing else, to
  ./out/kingfisher.txt » came out READY with the count computed by jq and a `draft` whose
  detail said « draft text containing only the numeric count, no other text or labels » — no
  serializing verb, so the number-alone cue missed and a model was asked to write a number the
  computation had produced (one call where the corpus expects zero). The cue now judges the
  citation as well: a request that asks for the number alone folds the draft whatever the seat
  called it, and a request that asks for prose keeps it.
