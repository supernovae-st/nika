- **A shared stopword no longer recalls knowledge for an authoring call.** The knowledge door's
  text recall (BM25 over the release's rows) read every word of three letters or more, so a row
  sharing only « the », « and » or « with » with the request was recalled and presented whole:
  on the release this build embeds, a ten-word authoring request reached 22 of its 26 examples
  and all 16 skills that way. The recall now reads informative words only, without the
  compiler's shared stopwords. Every row it reaches is still presented whole, with no count or
  byte quota, and the pack names its builder `knowledge-door-v7`.
