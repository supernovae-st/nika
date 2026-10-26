# What `permits:` covers on a harness seat

An `agent:` task can run on an agent application you are signed into, a
**harness seat** (`run: { access: { via: codex, protocol: acp } }`,
`nika run --access codex`, or a seat the access plan discovers). The harness
then runs its **own** tools: its file edits, its shell, its fetches. Nika does
not execute them, so it cannot judge each one the way it judges an `exec:` task
or a `nika:*` builtin.

Nika holds the session with two levers, and `nika check` prints the result on
every such task as a `[harness]` hint (`nika explain harness`).

## What Nika enforces

**Before the prompt** (journaled as `permit_checked`, plane `agent`):

- The session works in the project the run was admitted for, never in the
  directory the Nika process happens to run from.
- Nika selects the harness's measured **asking** mode and reads it back from
  the harness's own answer (Gemini CLI only acknowledges the switch), or
  refuses to start. It never selects a mode that approves actions on its own
  (an auto-reviewer, accept-edits, full access, bypass, yolo). A mode change
  the harness reports during the turn ends the session (Gemini CLI reports one
  only as message text, which Nika does not read).
- Nika offers the harness no file or terminal methods. A harness that calls one
  anyway gets an error, never a fake success, and the refusal is journaled.

**On every permission the harness asks** (journaled as `permit_checked`):

- Inside your `permits:` grants: allowed once, never "allow always".
- Outside them: the run pauses with the harness's question verbatim
  (`NIKA-1806`). Answer with `nika run --resume <trace> --answer <task>=true`
  (or `false`).
- Paths are judged in your project's terms (`./out/**` matches the absolute path
  the harness names) and where the operating system will land: each path is
  walked from the root, component by component, through every symlink (a
  dangling one included) and every `..`. `out/link/../notes.md` with `out/link`
  pointing outside the project is judged outside, beside the link's target. A
  path that cannot be resolved (a symlink loop, an unreadable directory) pauses
  for you.
- A command is matched against `permits.exec` by its program. A harness that
  asks with a shell string (Codex, Claude Code) or without any command line
  (Gemini CLI) can only be admitted by `permits.exec: true`; otherwise the ask
  pauses for you.
- An ask for network access is never answered automatically, whatever
  `permits.exec` grants: a fetch, or a command the harness says needs the
  network (Codex: an http or https URL, a SOCKS target such as
  `socks5Tcp network access to <host>`, or a network permission). It pauses for
  you with the target in view.
- A command that asks the harness to widen its own sandbox (Codex
  `additionalPermissions`, Claude Code `dangerouslyDisableSandbox`) pauses for
  you: no grant covers it, `permits.exec: true` included.

## What `permits:` does not cover

What the selected mode lets the harness do **without asking** never reaches
Nika, so `permits:` does not bound it:

| Seat (measured on) | Its default | Mode Nika holds | Still done without asking |
|---|---|---|---|
| `codex` (codex-acp 1.13.1, codex 0.156.1) | `agent`: Codex's own reviewer approves | `read-only`: every approval goes to Nika | reads any file; writes the project, `/tmp` and `$TMPDIR`; runs any program inside its own sandbox. This adapter has no read-only sandbox: **even with no `fs.write` grant, the project stays writable.** Network stays off unless it asks. |
| `claude-code` (claude-agent-acp 0.81.1, Claude Code 2.1.280) | your `permissions.defaultMode` (`auto` lets a classifier decide) | `default` ("Manual"): edits and commands ask | reads files in the project; runs commands Claude Code classifies as read-only; fetches documentation hosts it pre-approves. `bypassPermissions` is withheld. |
| `gemini-cli` (0.62.0) | `default`, or your `--approval-mode` / settings | `default` | reads files in the project (its file tools refuse paths outside it); runs shell commands it judges safe reads; Google web searches. |
| `qwen-code`, `kimi-code`, `opencode`, `copilot`, `grok-build` | — | none: no mode is measured, so none is selected; a current mode that visibly approves on its own refuses the run | everything it does without asking. Nika judges only what it asks. |

On every seat:

- An action Nika allows runs under the harness's own sandbox, not Nika's exec
  sandbox, so `permits.fs` and `permits.net` do not confine it. Nika judges the
  network access a harness asks for, not the connections an allowed command
  opens by itself: Codex keeps its sandboxed commands offline unless they ask,
  while a command Claude Code or Gemini CLI runs has the network the machine
  has.
- In `read-only`, Codex runs ordinary commands in its sandbox without asking,
  so a command it does ask about usually wants to run outside that sandbox. Its
  ask does not say so: `permits.exec: true` admits it like any command, and
  Codex then runs it with no sandbox at all. Leave `permits.exec` unset, or
  name only the programs you expect, to keep such a command waiting for you.
- The harness's own configuration still applies: its permission rules, its
  hooks and the MCP servers it configures act without Nika's judgment unless
  they ask.

If a task must not write anything, or must not read outside the project, do
not rely on `permits:` alone on a harness seat: run it on an API or local model
(the native agent loop judges every tool call), or in a container or a
throwaway checkout.

## Reading the trace

Each harness `agent:` task journals, in order:

- one `permit_checked` with plane `agent`, gate `session mode · <mode>` and
  decision `confine` (the measured mode was selected) or `unmeasured` (the
  harness's own mode was only observed),
  whose `why` states what the harness still does without asking;
- one `permit_checked` per ask, with decision `allow`, `deny` (your answer) or
  `escalate` (paused for you). Its gate names what was judged: `network ·
  <target>` for a network ask, `sandbox · <widening>` for a sandbox widening,
  otherwise the ask's kind with its program or paths (an unresolvable path
  reads `<unresolved>`). A run paused on that question still journals the
  task's decisions, just before its `workflow_paused` frame;
- one `permit_checked` with decision `deny` for each file or terminal method the
  harness called and Nika refused.
