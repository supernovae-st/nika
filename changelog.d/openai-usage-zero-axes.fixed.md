- **OpenAI's zero usage meters no longer void a reply's usage.** OpenAI's usage object
  carries audio and predicted-output meters on every text reply, even at zero, and Nika
  read any such key as an unknown axis, so a direct OpenAI reply never had complete
  usage. Zero meters now read as complete; a non-zero one, a cache write or an unknown
  key still leaves the usage incomplete.
