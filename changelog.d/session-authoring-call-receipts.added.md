- **A Session's work snapshot names each authoring call it made.** Beside
  the totals, every call the last compile sent now appears in call order
  with its role, the digests of its instruction and answer schema, the
  bytes it sent, how many references rode with it, its output and time
  bounds, its wall time, how it ended (the provider's stop reason or the
  engine's failure kind) and the reasoning and usage the provider
  reported. What the receipt does not record stays null, never guessed,
  and no prompt, answer, proposed object or error text is carried. Both
  the native workspace and the HTTP host read it from the same snapshot.
