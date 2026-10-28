- **The embedded language pack follows nika-spec's `nika:video_compose`
  contract.** `SPEC_PIN` and `crates/nika-pack/pack` advance together from
  `5cd95cd` to the contract of nika-spec#405: `canon.yaml` counts 31
  builtins, the schema's builtin enum and the stdlib gain
  `nika:video_compose` (media references, integer timing with audio as the
  clock, keyframes, budgets, the raw-only invariant, the host program and its
  confinement, the verified file), spec 10 says why it is not `exec:` under
  the order law, and the `video-compose-storyboard` example joins the pack.
