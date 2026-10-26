- **`NIKA-SEC-009` now counts untrusted content a child workflow brings in.** A parent that
  read a private file and sent data out passed `nika check` and ran when the untrusted read
  (a `nika:fetch`, an `exec:`, an `mcp:` tool, an `agent:` whose tools admit one) sat in a
  child reached through `invoke: workflow:`, while the same task written inline was refused.
  `nika check` and `nika run` now judge such a call as the ingress it stands for, through
  every workflow below it: the parent is refused unless a blocking `nika:prompt` dominates
  the path, as for the inline form, and the finding names the child task behind the call. A
  child the check cannot read or resolve counts as untrusted (fail-closed). `check --json`
  lists each resolved call under `composed_calls`, and the runtime's report check
  re-derives the trifecta over the same calls. `NIKA-DRIFT-001` no longer tells you to
  remove a grant a child needs (its program, the files its subprocess reads, its tools and
  hosts); removing it failed `NIKA-COMP-002` or the child's run. A check of the source
  alone (an editor, the MCP `nika_check` tool) reads no child and judges the parent's own
  tasks only.
