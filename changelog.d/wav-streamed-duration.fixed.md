- **A streamed WAV reports its real duration.** `nika:tts_generate` read a
  streamed WAV header's placeholder `data` size (`0xFFFFFFFF`) verbatim, so
  an OpenAI narration reported `duration_ms: 89478485` (24.8 hours). The
  size is now clamped to the bytes actually present, which also gives
  `espeak-ng --stdout` audio its true length.
