- **The catalog records how a model reasons, and what a `.nika` file can do about it.**
  `ModelCapabilities.reasoning_facts` says, per capability rule, whether the model reasons
  when the request names no reasoning control, whether that reasoning is spent inside the
  output cap, and how a `.nika` file turns it off on this engine (`ThinkingOff`: by default,
  a zero thinking budget, not reached, or never). DeepSeek Flash and V4 Pro, Gemini 2.5
  Flash and Pro on the Gemini API, and Claude on Anthropic's API carry recorded facts; every
  other model stays unknown rather than assumed. Tests pin the facts to what each wire sends.
