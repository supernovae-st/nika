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
- Nika asks the harness to report a turn it could not complete as a failure
  (the `sessionFailure` extension, which codex-acp and claude-agent-acp
  implement). A rejected model or an exhausted retry limit then fails the task
  with the harness's own error, instead of returning that error as the answer.
  A failure the harness offers to retry is transient (`NIKA-1804`, so `retry:`
  may replay the task); one that asks you to sign in again reads as an expired
  sign-in; any other is a refusal (`NIKA-1805`). A harness without such a
  report still returns its error as text, and Nika does not read answer text
  to guess a failure.

**On every permission the harness asks** (journaled as `permit_checked`):

- Inside your `permits:` grants: allowed once, never "allow always".
- Outside them: the run pauses with the harness's question, followed by the
  command it names unless the title shows it, and by the reason it gives
  (`NIKA-1806`). The question is one line: control characters become spaces
  and invisible ones (direction overrides, zero-width characters) are removed,
  so the harness cannot rewrite or hide the line you answer. A title or
  command over 2,000 bytes shows its beginning and its end around the number
  of bytes left out and their SHA-256. Answer with
  `nika run --resume <trace> --answer <task>=true` (or `false`). The harness
  is shut down at the pause; the resumed run starts the task again in a new
  session, and your answer decides its first ask outside the grants, once for
  the task: an attempt that `retry:` replays never reuses it, so an ask outside
  the grants there pauses again.
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
- In `read-only`, Codex runs ordinary commands in its sandbox without asking,
  so a command it asks about asks to leave that sandbox, and Codex would run it
  with no sandbox at all. Such an ask always pauses for you, `permits.exec:
  true` included, with the command and Codex's reason in the question.
- An ask for network access is never answered automatically, whatever
  `permits.exec` grants: a fetch, or a command the harness says needs the
  network (Codex: an http or https URL, a SOCKS target such as
  `socks5Tcp network access to <host>`, or a network permission). It pauses for
  you with the target in view.
- A command that asks the harness to widen its own sandbox (Codex
  `additionalPermissions`, Claude Code `dangerouslyDisableSandbox`) pauses for
  you: no grant covers it, `permits.exec: true` included.
- An MCP tool call the harness asks you to approve always pauses for you,
  because `permits.tools` names your workflow's MCP tools, not the harness's.

## What `permits:` does not cover

What the selected mode lets the harness do **without asking** never reaches
Nika, so `permits:` does not bound it:

| Seat (measured on) | Its default | Mode Nika holds | Still done without asking |
|---|---|---|---|
| `codex` (codex-acp 1.13.1, codex 0.156.1) | `agent`: Codex's own reviewer approves | `read-only`: every approval goes to Nika | reads any file; writes the project, `/tmp`, `$TMPDIR` and its own `~/.codex` (generated images, session logs, state); runs any program inside its own sandbox, and runs its tools through a JavaScript "code mode"; calls its configured MCP tools marked `readOnlyHint: true`, remote ones included, so a Codex MCP approval rarely reaches Nika; opens pages with its hosted web tool, even under `permits: {}`, so untrusted web content reaches the model; starts every configured MCP server when the session begins; applies your global `~/.codex/AGENTS.md` and your Codex plugins. This adapter has no read-only sandbox: **even with no `fs.write` grant, the project stays writable.** Network stays off for its commands unless it asks. |
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
- The harness's own configuration still applies: its permission rules, its
  hooks and the MCP servers it configures act without Nika's judgment unless
  they ask.

If a task must not write anything, or must not read outside the project, do
not rely on `permits:` alone on a harness seat: run it on an API or local model
(the native agent loop judges every tool call), or in a container or a
throwaway checkout.

## Images and structured answers on a seat

An application such as Codex can generate images during an `agent:` turn.
Declare `images:` on the task to use them in the workflow:

```yaml
banner:
  agent:
    prompt: "Draw a 16:9 release banner, no text"
    images:
      save: ./out/release/banner.png   # optional
halftone:
  with:
    banner: ${{ tasks.banner.output.images[0].path }}
  invoke:
    tool: nika:image_fx
    args: { input: "${{ with.banner }}", out: ./out/release/banner-halftone.png, ops: [{ halftone: {} }] }
```

- `.output` becomes `{ message, images }`. `message` holds what `.output`
  would hold without `images:`. Each image carries `index` (from 0), `path`,
  `sha256`, `size_bytes`, `mime_type`, `width` and `height`.
- Only the bytes the application sent on **this task's own session** count.
  The application also reports a path where it kept a copy (Codex: a folder
  under `~/.codex/generated_images` that every session on the machine
  shares). Nika never opens that path and never lists it as an image, so a
  task cannot pick up another session's image. Do not ask the application to
  copy its image into the project; declare `save:` instead.
- With `save:`, Nika writes the received bytes at that path through
  `nika:write`, inside `permits.fs.write` (`NIKA-SEC-004` outside it). A
  second image lands at `banner-2.png`, a third at `banner-3.png`, up to
  four. Every path is judged before any file is written: `nika check` judges
  all four for a literal path (grant the directory, `./out/release/**`, or
  the four files `--infer-permits` lists), and the run judges the received
  ones before the first write. The written file has the digest of the
  received bytes. A task whose session delivered no image fails
  `NIKA-AGENT-007` instead of succeeding with no file.
- A save is a write: two tasks that can run at the same time and save on one
  path, a `for_each` task that saves one path, or a save beside a
  `nika:write` of one of its files are refused (`NIKA-SEC-012`), like two
  `nika:write` tasks. Order them with `after:` or give each its own path.
- Nika recognises the image tool of Codex. On another application,
  `images: { save: }` receives nothing and fails `NIKA-AGENT-007`.
- `images:` belongs to a task on a seat. On an API or local model, `nika
  check` names it as a blocker and the run refuses it (`NIKA-465`): the
  engine's own loop gets its images from `nika:image_generate`.

A `schema:` on such a task is validated by Nika: the prompt carries the
schema, the final message must contain a JSON value that satisfies it, and
the validated object becomes the output (`message` under `images:`). There is
no repair turn on a seat, because a new session would replay the agent's
work; a final message that does not validate fails `NIKA-INFER-002`.

An `infer:` task over ACP (`run.access.protocol: acp`) runs one tool-free
turn. It carries:

- `schema:`: the prompt carries the schema and Nika validates the answer. An
  answer that does not validate is never the output: Nika asks again in a
  new one-turn session that receives the earlier answer and the errors, up to
  the repair budget, then fails `NIKA-INFER-002`. Each repair is one more
  prompt on your subscription.
- `vision:` files, sent as ACP image blocks, when the application declares
  image prompts at `initialize` (codex-acp and claude-agent-acp do). An
  application that does not is refused before the session starts, and so is
  a file whose bytes do not start with a PNG, JPEG, GIF or WebP signature.
  As on an API route, a `vision:` file is a read inside `permits.fs.read`:
  `nika check` judges a literal path, and the run judges every path on its
  resolved form before the session starts (`NIKA-SEC-004` outside the
  boundary, `NIKA-AUTH-006` at check with no `permits:` block). A granted
  file that cannot be read fails with the read's own code
  (`NIKA-BUILTIN-READ-001` when no file is there, `-002` otherwise), and a
  secret in a vision entry is the `infer` leak (`NIKA-SEC-006`) unless its
  `egress:` sanctions `{ to: "infer" }`.

It cannot carry `temperature`, `max_tokens`, an enabled `thinking` budget or
a `vision:` URL (Nika does not fetch it). The run refuses those before
anything is sent (`NIKA-432`), and `nika check` names them as blockers instead
of reporting the file run-ready. On a seated task, `check` also stops
suggesting `max_tokens` or `max_tokens_total`.

## Reading the trace

Each harness `agent:` task journals, in order:

- one `permit_checked` with plane `agent`, gate `session mode · <mode>` and
  decision `confine` (the measured mode was selected) or `unmeasured` (the
  harness's own mode was only observed),
  whose `why` states what the harness still does without asking;
- one `permit_checked` per ask, with decision `allow`, `deny` (your answer) or
  `escalate` (paused for you). Its gate names what was judged: `mcp · <server
  or tool call>` for an MCP tool approval, `network · <target>` for a network
  ask, `sandbox · <widening>` for a sandbox widening (its `why` names the
  command the widening is for), `sandbox · command` for a Codex command that
  asks to leave its sandbox (the command and the reason in its `why`, whole up
  to 16 KiB, then its beginning and end around the bytes left out and their
  SHA-256),
  otherwise the ask's kind with its program or paths (an unresolvable path
  reads `<unresolved>`). A run paused on that question still journals the
  task's decisions, just before its `workflow_paused` frame;
- one `permit_checked` with decision `deny` for each file or terminal method the
  harness called and Nika refused;
- one `agent_image_observed` per image: the digest and size of the received
  bytes, the local blob, the path the harness reported (never opened) and,
  under `images: { save: }`, the `saved_path` Nika wrote. `nika serve` lists
  that file among the run's artifacts with the received digest.
