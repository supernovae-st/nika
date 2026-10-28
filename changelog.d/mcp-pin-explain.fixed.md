- **`nika explain` answers the MCP pin refusals a run sends you to (NIK-254).**
  A run refused with `NIKA-MCP-006` printed `fix: nika explain
  NIKA-MCP-006`, and explain answered `unknown code` for it and for
  `NIKA-MCP-003`, `004` and `005`, the other codes the pin store mints. Each
  now says what failed and the remedy the refusal prints: re-approve a
  drifted server deliberately (`nika mcp approve <server>`), restore or
  re-enroll an untrusted `.nika/mcp_pins.json`, check the OS sandbox an MCP
  server is confined by, approve the server a tool belongs to. The MCP
  `nika_explain` tool answers the same.
