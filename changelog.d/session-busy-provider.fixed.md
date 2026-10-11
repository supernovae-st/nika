- **A busy provider is asked again, and the person is told.** When the
  conversation's route answers 429, 503 or 529, the Session waits the
  delay the provider named (else 1 s, then 2 s) and asks again, twice at
  most, each ask counted as a call. A longer named delay or a third
  rejection ends the call with the provider's own words, how many times
  it asked again and how long it waited. A money account that keeps the
  rejected call's charge as unknown is never asked past: the call ends
  there, saying so.
