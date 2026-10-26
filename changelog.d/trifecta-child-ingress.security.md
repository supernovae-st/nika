- **`NIKA-SEC-009` now judges a child workflow through the call that runs it.** A parent that
  read a private file and sent data out passed `nika check` and ran when part of the flow sat
  in a child reached through `invoke: workflow:`, while the same tasks written inline were
  refused. `nika check` and `nika run` now judge each call as the tasks it stands for, through
  every workflow below it. The call is a source of untrusted content when content the child
  brings in (a `nika:fetch`, an `exec:`, an `mcp:` tool, an `agent:` whose tools admit one)
  reaches its outputs or a file it writes. It is an egress task when untrusted content reaches
  an egress task inside it: content the child brings in, the call's arguments, or a file
  written before the call that a child `exec:` reads. The parent is refused unless a blocking
  `nika:prompt` of its own dominates the path, as for the inline form; a gate inside the
  child is not credited to the caller. The finding names the child task behind the call. A
  child the check cannot read, parse or resolve counts as all of these (fail-closed).
  `check --json` lists every direct call under `composed_calls`, resolved or not, and the
  runtime's report check re-derives the trifecta over the same calls. `NIKA-DRIFT-001` no
  longer tells you to remove a grant a child needs (its program, its tools, the files and
  hosts its own `permits:` lets its subprocess reach); removing it failed `NIKA-COMP-002` or
  the child's run. A check of the source alone (an editor, the MCP `nika_check` tool) reads
  no child: its trifecta judges the parent's own tasks only, and its `NIKA-DRIFT-001` makes
  no `permits:` claim on a file that calls a child.
