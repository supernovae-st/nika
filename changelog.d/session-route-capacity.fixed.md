- **Session calls ask their route's capacity during continuous preparation.**
  The turn classifier, the chat reply, the decision seat's choice and
  the reasoning calls carried fixed output ceilings (8,192 tokens, a
  label ceiling, 256 for a choice) that truncated long answers and
  forced a low reasoning effort on reasoning routes. They now ask the
  route's capacity; an explicit limit given by the caller is kept.
