- **`nika arm --emit` no longer arms a beat that cannot run.** `nika arm` reported a beat
  UNREADY (a required input with no binding source, a missing workflow, an unpriced
  model), yet `--emit` printed its unit and `--write` installed it, so the OS would fire
  a run that could only fail. The emission now refuses while any active local beat is
  UNREADY, naming each one with the blockers `nika arm` shows: fix the beat, or suspend
  it with `actif: false`. A beat whose only open question is an unknown (a child
  workflow whose model only the run decides) now reads READY, as `nika arm fire` fires
  it, and `nika arm` still names the unknown.
