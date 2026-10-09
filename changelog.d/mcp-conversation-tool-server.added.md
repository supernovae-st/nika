- **`nika-mcp` serves a Session's tools to the agent leading its conversation.**
  `conversation::ToolServer` lists the session's `nika/author-tools@0` definitions and relays
  each call of a listed tool to the session over Streamable HTTP, bound to `127.0.0.1` on an
  ephemeral port with a bearer minted for that conversation alone and always required. A tool the
  session does not list never reaches it; a failed call returns `isError: true`. Connections are
  served side by side under the same bounds as `nika mcp --http`, and the session receives one
  call at a time. The server interprets nothing and grants nothing. A stdio bridge to the same
  server exists as a library; no CLI starts it yet.
