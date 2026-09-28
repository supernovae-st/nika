- **Failed HTTP builtin calls report their status as data.** A `nika:fetch`
  failure now carries `details.status_code` in `tasks.X.error`, in the
  terminal frame's `outcome` and in the sealed trace, plus `details.accepted`
  when the call declared `response.accept`, including an extraction failure
  after an accepted status. Only these typed facts cross: provider text,
  headers, bodies and URLs never do, a transport failure reports no status,
  and an error without details keeps its `{code, message, transient}` shape.
