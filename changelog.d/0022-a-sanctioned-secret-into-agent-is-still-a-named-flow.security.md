- **A sanctioned secret into `agent:` is still a named flow (#1041).**
  JOURNEY takes the agent's `tools:` intersected with `permits.net.http`
  as the destination set. SECRETS stops saying « no declared secret
  reaches an effect » after the author applied the tool's own `egress:`
  advice. A sanction authorizes a flow; it does not deny the flow exists.
