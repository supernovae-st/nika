- **The `group.<name>` fan-in fold resolves at run time.** A task declaring
  `group: probes` joined the checker's `fan-in` edges and scheduled its
  consumer after every member, but the runtime bound no `group` root, so
  every `${{ group.probes }}` binding died at the boundary with
  `NIKA-VAR-001` — `nika check` green, `nika run` red. The `with:` renders
  now bind the declared membership and the fold reads one array of member
  records `{ id, status, output, duration_ms, error }` in declaration order;
  a member without a settled record keeps the read loud instead of folding
  a smaller array.
