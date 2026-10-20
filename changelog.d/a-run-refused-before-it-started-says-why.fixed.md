- **A run refused before it started says why.** A run the engine refused before its first step
  (an access path refused, NIKA-1800, for an effort Gemini does not offer) showed only « exit 3 ·
  refused by the environment »: its reason went to a discarded stderr. The run's lane now keeps
  that reason and shows it as the run's line, never doubling a reason the stream already gave,
  and the conversation hears it with the person's next line instead of guessing a cause.
