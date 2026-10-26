- **An `agent:` task on a harness seat is held to the workflow's `permits:` as far as the
  seat allows, and `nika check` says where that stops.** On codex-acp the session kept the
  adapter's default `agent` mode, whose own reviewer approves most actions, so a Codex task
  wrote `./out` and read files outside the project without a single `permit_checked` frame.
  Nika now roots the session in the admitted project (never the server's or the shell's
  directory), selects the seat's measured asking mode (codex-acp `read-only`,
  claude-agent-acp `default` instead of your `auto` default, gemini-cli `default`) and reads
  it back before the prompt or refuses to start, never picks a mode that approves on its own,
  and ends the session if the mode moves mid-turn. The mode, every ask and every refused
  client `fs/` or `terminal/` call are journaled as `permit_checked`, also when the run pauses
  on the ask (a paused trace used to carry the question and no decision). Asks are judged in the
  project's own terms: an absolute path the harness names now matches `./out/**`, a symlink is
  judged where it leads, and a command asking for network access pauses instead of riding a
  program grant. What the mode still leaves to the harness is printed on every such task as
  a `[harness]` hint (`nika explain harness`, docs/usage/harness-agents.md): Codex, for one,
  reads any file and writes the project, `/tmp` and `$TMPDIR` inside its own sandbox even with
  no `fs.write` grant. `NIKA-DRIFT-001` no longer tells you to remove a `permits.exec` or
  `permits.fs` grant that the permission bridge judges.
