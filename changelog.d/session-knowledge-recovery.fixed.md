- **A refused knowledge override no longer stops the conversation.** A Session
  whose configuration named a knowledge release it cannot verify (a historical
  `NIKA_KNOWLEDGE`) refused every model-backed request, and its only remedy was
  to quit and open the Session again. The conversation now opens as usual and its
  snapshot types the refusal (`knowledge`). A line that would reach a model waits
  before any request, exactly as typed, with one short explanation;
  `/knowledge embedded` uses the knowledge built into Nika for that conversation
  (its history keeps the choice) and resumes the line once, under the same
  intelligence and effort. Nothing falls back silently, and lines that need no
  model behave as before.
