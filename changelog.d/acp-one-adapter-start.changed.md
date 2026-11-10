- **An ACP task starts its adapter once.** For an agent application whose
  identity is its `initialize` answer (codex-acp, claude-agent-acp,
  opencode, copilot), every `agent:` task and every `infer:` task over ACP
  used to start the adapter twice: once to ask `initialize` and stop it,
  once for the session. The session's own first answer is now the one
  judged, before `session/new`. A hosted codex seat saves one adapter
  start per task, measured at about 1.6 s. The refusals are unchanged: an
  adapter that does not answer `initialize` within 30 s, exits before
  answering, answers as another program or sits outside its version pin
  is unavailable (`NIKA-1803`) and is not retried. `nika doctor` and the
  access rows still probe on their own.
