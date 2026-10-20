- **An open-weight model named on another provider is no longer refused as a
  wrong seat.** `nika check` refused `scaleway/llama-3.3-70b-versatile`, or a
  local engine's `meta-llama/…` model, as served only by the one provider whose
  catalogue row lists it. An `org/model` id, or a model the pricing snapshot
  marks open-weight, now has no single owner, whoever lists it; a
  provider-exclusive id pasted on another provider (`groq/grok-4.3`) is still
  refused with its owner's id as the repair.
