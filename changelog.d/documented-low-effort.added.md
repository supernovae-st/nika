- **A draft read whole runs at its model's documented low effort.** When
  every model step of a request is a draft (write, summarize,
  translate), the reader admits the request whole with no constraint
  beside it, and the model's exact catalog row documents a `low`
  reasoning effort (DeepSeek Flash and V4 Pro today), the compiled
  workflow declares `run.reasoning.effort: low` and the proposal says
  so. On Flash, one measured 16.7k-token summary spent 83% of a
  16,384-token cap reasoning at the default effort and 69% at low; at
  the default effort the same step had ended with no answer. A
  candidate that already declares an effort or a route keeps it, and a
  model-written candidate gains one block, never a reformatted file.
  The level reaches only the model's exact direct route: behind a
  base-URL override (a gateway), every request that carries it is
  refused with nothing sent.
