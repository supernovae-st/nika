- **A reasoning model's own state now goes back with the turn it came with.** Gemini puts a
  thought signature on the parts of an answer and asks for it back, Anthropic signs each
  thinking block and asks for assistant turns back as received, and DeepSeek asks for the
  `reasoning_content` of earlier turns when a request carries tools. Nika dropped all three, so
  a multi-turn tool conversation sent the model its own turns without the state it asked for.
  The kernel carries that state as `ContentBlock::Reasoning` (and `InferEvent::Reasoning` on a
  stream), opaque to everything but the wire that produced it: Gemini gets each signature back
  on its own part, Anthropic its signed and redacted thinking blocks in place, and DeepSeek its
  reasoning on every assistant message of a request with tools, empty when a turn had none.
  The state never goes back alone or to another provider, and a decision never reads it as an
  answer. Two limits: a Session tree that holds reasoning blocks does not load in an older
  binary, and nothing assembles `InferEvent::Reasoning` from a stream yet, so the round-trip
  holds on buffered calls only.
