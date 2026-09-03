- **A resume cannot switch access silently, and the resident executes the
  same plan.** `nika run --resume` judges the lanes the trace's boot
  manifest recorded against the plan this machine resolves now: a moved
  lane refuses on the environment exit, naming both paths and the two
  explicit `--access` flags; an explicit `--access` names the change and
  proceeds with a notice. The lane an `infer:`/`agent:` task rode joins
  its resume identity, so a task served by a seat is never served from
  the trace on another path. `nika serve` resolves the frozen access
  plan for every resident job through the one resolver the CLI uses
  (`nika_service_execution::access`) and attaches it to the run — a job
  with no ready path refuses before its first task.
