- **A verification no longer asks the author to read the same request again.** Every
  verification of a conversation asked its author to read the person's request, then asked again
  for an audit of that reading, even when the words had not changed. With Grok 4.7 those calls
  held 89 % of a session's reasoning tokens. The Session now keeps the last reading audited as
  covered and proposes it to the next verification of the same words, which still audits it.
