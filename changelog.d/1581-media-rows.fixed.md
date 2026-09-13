- **Doctor no longer prints the unset media fallback as a listener.** The
  image and tts rows rendered `local → http://localhost:8080 default` under
  `ok` when `NIKA_IMAGE_LOCAL_URL` / `NIKA_TTS_LOCAL_URL` were unset,
  reading as a wired path on the most contended local port. Doctor never
  probes the media planes, so the rows now name the backend as unset,
  disclose the engine fallback as unprobed, and mark a set URL as
  configured, never reachable.
