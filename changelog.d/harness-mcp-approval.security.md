- **A Codex MCP tool approval no longer rides `permits.exec: true`.** codex-acp asks to approve
  its own MCP tool calls as a command ask with no command, which `exec: true` answered
  `allow_once`; it now pauses for you on the tools plane, because `permits.tools` names your
  workflow's MCP tools, not the harness's.
