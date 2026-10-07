- **A verifier's rejection now holds for the rest of the request.** A
  workflow the verifier rejected was kept from it within one compilation
  only: writing the workflow again could yield the same bytes, and the
  same verifier, asked again, could then accept them. The session now
  carries the verifier's rejections into every later compilation of the
  same request, and `nika compile` keeps them beside its plan record, in
  `.nika/compile/<sha256>.declined.json`, carried even under `--fresh`:
  the same bytes under the same verifier, for the same request, answers
  and observed files, keep the earlier rejection, with no new question,
  and the CLI says so. The parts it found missing are repaired from, so
  writing the same workflow again no longer ends the round. A corrected
  request, other answers or changed files are judged afresh, and an
  abstention is not carried, so a new round may still decide it. When
  the verifier's questions stopped early, a call refused or failed, or
  when it waited for a trial run that a later round now has, the later
  round asks only what it never answered, never what it already did.
  Serve keeps no conversation and carries none. A replayed workflow is
  also never ready in an answer round that no verifier judges, unless
  the deterministic reader planned it alone: an `--answer` round with
  neither `--authoring-model` nor `--decision-model`, or a session
  answer round compiled without a model because the money allowance
  blocks one, replays the plan and stays incomplete until a verifier
  judges it.
