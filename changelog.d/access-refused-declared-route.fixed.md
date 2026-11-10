- **A refused seat no longer teaches a flag its own file refuses.** When a
  seat fails at the call, the `access_refused` record on `task_failed`
  names the next ready path and the flag that pins it. Under a declared
  `run.access.via`, that next path could be a route the declaration
  itself excludes: a machine with a ready `openai` key answered a refused
  `via: codex` call with `"pin": "--access openai"`, a flag the same file
  refuses before task 1. The record now names a next path only when the
  declaration allows it, and names none otherwise.
