- **The transport's account of a call is stamped on the frame and read by
  the peek.** A seat that answered only after the bounded backoff is
  readable in the sealed trace: `task_completed` carries `attempts` whenever
  a wire call happened, and `waited_ms` and `retried_on` only when the
  transport re-sent (a first-time answer reads `attempts: 1` and nothing
  else; a harness seat or an agent loop reports no transport and gets no
  field). `nika trace peek` folds the three fields into the task row
  (« 2 attempts (waited 2.0s on 429) »); one attempt says nothing, and a
  frame from an older engine renders exactly as before.
