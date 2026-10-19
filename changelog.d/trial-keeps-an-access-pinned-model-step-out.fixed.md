- **A workflow that pins its model's access can be proposed again.** A file whose `run.access`
  names a route or a protocol (`via: anthropic`, `protocol: api`) made the trial before a
  proposal stop with « The rehearsal engine failed » and nothing to repair, so asking for an
  API-only workflow could never reach a proposal. The trial now keeps that model step out, as it
  already did for an unpinned one, and rehearses the rest. A real run still judges the pinned
  access on its own plan.
