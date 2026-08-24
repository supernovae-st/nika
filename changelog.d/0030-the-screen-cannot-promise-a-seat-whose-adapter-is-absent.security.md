- **The first screen cannot promise a seat whose adapter is absent.** A
  signed-in Claude Code no longer reads as a runnable harness seat when
  `claude-agent-acp` — a different npm package, the binary a session
  spawns — is not on PATH. `ready` now needs three things, not two: the
  app is here, it is signed in, and its ACP adapter answers. The row
  stays visible and names the one command that closes the gap
  (`npm i -g @zed-industries/claude-agent-acp`); hiding an app the person
  has would trade one lie for another. `doctor --json` stops reporting
  `ready: true` and `chosen_access: claude-agent-acp` on a machine where
  no such binary exists — an agent reads that field and acts on it.
