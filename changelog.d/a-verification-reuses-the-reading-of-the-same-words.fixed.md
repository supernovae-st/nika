- **A verification no longer reads the same request again.** Every verification in a
  conversation asked its author to read the person's request, then asked for an audit of that
  reading, even when the words had not changed. With Grok 4.7, those calls took 89 % of a
  session's reasoning tokens. The Session now keeps, in memory, the last reading audited as
  covered. The next verification of the same words, answers, model and prompts reuses it with
  no call. The judge still judges every candidate, and a reopened conversation reads and audits
  the request again.
