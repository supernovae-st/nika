- **Nika's MCP server and client say only the protocol they speak.** The
  server answered an `initialize` asking for MCP 2026-07-28 with that
  revision, and an unknown revision with it too, yet the final 2026-07-28
  text has no handshake and neither half implements its MUSTs; it now
  answers 2025-11-25, the latest handshake revision, as does the
  conversation server, and the 0.94.0 changelog line that said it spoke
  2026-07-28 natively carries the correction. As a client, a `tools/call`
  result whose `resultType` is `input_required` or `task` is a failed call
  instead of an empty success; a reply that follows more than sixteen
  interleaved notifications is read, the wait being bounded by its deadline
  and by 8 MiB of skipped messages; and a call that times out is cancelled
  with `notifications/cancelled` before the server is stopped.
