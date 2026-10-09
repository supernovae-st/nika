- **An ACP agent can lead a Session conversation with Nika's tools alone.** The harness opens
  one persistent session per conversation (`SpawnedHarness::converse`) and sends one prompt per
  user entry; the agent keeps the history and runs its own loop with no turn limit. Claude Code
  gets the audited one-shot surface without `maxTurns` (no built-in tools, strict MCP, no
  settings, plugins, skills or agents, nothing persisted, summarized thinking) and exactly one
  MCP server, Nika's; Codex mounts it from its configuration, read back before the spawn with
  every other server disabled. The transport follows the agent's advertised HTTP MCP capability
  and is recorded; an agent that cannot mount what Nika offers is refused, never sent down the
  one-shot path. Only Nika's own tools are allowed, once; every other permission is rejected and
  recorded. Stop sends `session/cancel` once per turn, and each turn ends with its activity
  record, where tool frames are expected activity.
