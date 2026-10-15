- **An MCP server's network allowlist now reaches its hosts.** On macOS a
  server declared with `"network": {"allowlist": [...]}` reached no host at
  all and could not even resolve a name: its jail admitted loopback only,
  while `nika mcp approve` announced it as confined like `"allow"`. The
  spawn now starts the loopback egress proxy that the exec allowlist uses,
  points the server at it (`HTTPS_PROXY` and siblings) and fences the jail
  to the proxy's port, so exactly the declared hosts answer. The approval
  note says `through a loopback egress proxy`, and a refused host prints
  one `nika:egress REFUSED` line naming the server's allowlist.
