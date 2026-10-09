- **One contract for the tools a Session serves.** `nika_session_change::tools` types the
  `nika/author-tools@0` tools once for Nika's own agent loop and for an ACP agent reaching them
  over MCP: the 17 fixed tool names, `ToolDef`, `ToolCall` (with the caller's tool-use id),
  `ToolReply` (text, failure, and whether the turn ends on it) and the `SessionTools` trait the
  Session implements. It grants nothing: saves, runs and consents keep their own doors.
