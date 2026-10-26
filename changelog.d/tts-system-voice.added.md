- **`nika:tts_generate` speaks with no key through `provider: system`.** The
  host's own synthesizer, offline: macOS `/usr/bin/say`, or Linux
  `espeak-ng` found in an absolute directory of the engine's `PATH`. The
  engine starts it itself as part of the builtin, with the text on standard
  input, so the workflow needs no `permits.exec`. The text is spoken as
  text: every `[[` that `say` reads as an embedded command (`[[volm 0]]`
  would mute the file) or `espeak-ng` as phonemes is broken by a space, and
  the audio is capped at 64 MiB. The WAV still lands under `output_dir:`
  through `permits.fs.write`. `nika check` names the program in its plan and
  refuses a literal `model:` other than `say`/`espeak-ng` or a voice that is
  not a host voice name, as the run does. Every start is journaled as a
  `permit_checked` frame on the `builtin` plane, and the cost is reported as
  unpriced local work (`cost_usd: null` · `cost_unpriced: local_model` on
  the output and the task's frame), never as priced or free; only this
  builtin can label the run's cost that way. A named voice must be installed
  (`say` would otherwise read with its default voice), and a host with no
  synthesizer refuses `NIKA-BUILTIN-TTS_GENERATE-002` naming what is
  missing, with no fallback to another provider. A narration no longer needs
  `exec /usr/bin/say`.
