- **A Codex conversation's approval of Nika's own tool is now recognized.** codex-acp 1.13.1
  announces each MCP call with the server and tool Codex routed it to, then asks Codex's
  approval of it by the call's id alone (`{toolCallId, kind, status}`); Nika read only Claude
  Code's tool name, so it rejected every such ask. Codex 0.156.1 asks one before each Nika tool
  that is not read-only (`candidate_write` among them), and a rejection reaches the model as the
  user cancelling the call, so no Codex-led Session could write a candidate. A turn now keeps
  what each open call was announced as, and an ask by id alone is for that call; Codex's own
  question (`Allow the <server> MCP server to run tool "<tool>"?`) is read when codex-acp cannot
  tie the ask to one. Only an offered tool on Nika's server is allowed, once; an id never
  announced or no longer open, another server, a tool Nika does not offer and a command stay
  rejected. Claude Code's path is unchanged.
