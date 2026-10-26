- **`nika check` judges a declared route over this machine's access rows.**
  A file that declares `run.access` (a `via:` or a `protocol:`) and has no
  `infer:` or `agent:` task was judged over no rows, so ACCESS said the binary
  was built without agentic CLI adapters and `run ready` was refused, while
  `nika run` resolved the same route and finished. Check now probes exactly
  when the run does: a task that dials a model, an `--access` pin, or a
  declared route.
