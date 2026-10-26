- **`nika:tts_generate` speaks with no key through `provider: system`.** The
  host's own synthesizer, offline: macOS `/usr/bin/say`, or Linux
  `espeak-ng` found on the engine's `PATH`. The engine starts it itself as
  part of the builtin, with the text on standard input, so the workflow
  needs no `permits.exec`; the WAV still lands under `output_dir:` through
  `permits.fs.write`. `nika check` names the program in its plan, every
  start is journaled as a `permit_checked` frame on the `builtin` plane, and
  the cost is reported as unpriced local work (`cost_usd: null` ·
  `cost_unpriced: local_model` on the output and the task's frame), never as
  priced or free. A named voice must be installed (`say` would otherwise
  read with its default voice), and a host with no synthesizer refuses
  `NIKA-BUILTIN-TTS_GENERATE-002` naming what is missing, with no fallback
  to another provider. A narration no longer needs `exec /usr/bin/say`.
