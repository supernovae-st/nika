# The terminal workspace

Bare `nika` in an interactive terminal opens the workspace. Start typing in the
conversation; opening a workflow or a Run object does not attach it to your next
message. The project list, conversation and object are views of the same Session.

## Arrange the same work

The header names the current layout. `F4` switches between **Session** and
**Workbench** without sending the draft or changing the selected object, its
face, the Session or a Run.

Session places the conversation beside the object from 100 columns; the project
list appears on the left from 120 columns. At narrower sizes the conversation
sits below the object and the project list remains reachable with `F6`.
Workbench gives the object more space above a compact conversation and its
composer. Below 60 columns or 16 rows, the focus view keeps the conversation
usable until the workspace fits again.

Drag a visible separator to change its proportions. With the project list or
object focused, `+` grows that region, `-` shrinks it and `0` restores its
automatic proportion. These characters remain ordinary text in the composer.
Resizing applies the chosen proportions within bounds; it does not overwrite
them.

The live host keeps settled layout changes in `$HOME/.nika/tui-layout.json`.
An unsupported or malformed preference file is preserved and a local notice
explains the problem. If saving fails, the layout remains usable for the current
session; the notice does not claim that persistence succeeded. These preferences
restore display choices only, never consent, work, model selection or a Run.

## Find an action before using it

Type `/` to browse the commands the current conversation supports, or use
`Ctrl+O` to open the command palette. Each command has an effect, scope and short
description. Choosing a slash command inserts it into the composer; press Enter
separately to send it. Escape closes the palette and restores the draft and
the panel that held keyboard focus. A view key chosen in the palette acts from
that panel; choosing a command returns focus to the composer. Clicking another
panel closes the palette and keeps the clicked panel's focus with the draft
restored.
View actions in the palette change the display directly and grant no Session
authority.

| Need | Command or key |
|---|---|
| Read the Session's commands and help | `/help` |
| Inspect the project, intelligence and context scope | `/status` |
| Understand a waiting question or gate | `/why` |
| Read the compiler's interpretation of the request | `/meaning` |
| Inspect the proposal's exact bytes | `/show` |
| Inspect authoring provenance | `/details` |
| Read the last Run's trace verdict and its limits | `/proof` |
| Show the Session's intelligence choices | `/intelligence` |
| Read the full words behind a summarized diagnostic | `F2` |

The chooser lists the commands available now. `/restore` appears only when the
Session offers kept work for a fresh review. Model names do not establish route
capabilities; authoring, decision and Run information retains its own scope.
Choosing intelligence does not rewrite a previously observed Run.

## Keep the conversation in reach

| Gesture | Effect |
|---|---|
| `F6` / `Shift+F6` | Move keyboard focus between panels |
| `Esc` from the project or object | Return to the composer |
| `Enter` in the project list | Open the selected entry for inspection |
| `Left` / `Right` in the object | Change the object's face |
| `r` in the object | Ask its host to read it again |
| `PgUp` / `PgDn` in the conversation | Read earlier or later messages |
| `End`, or the return-to-live marker | Return to the latest messages |
| Mouse wheel over a panel | Scroll that panel |
| `Alt+Enter` in the composer | Insert a line break |
| `Ctrl+T` | Switch between full screen and inline |
| `Ctrl+L` | Redraw |

Pasted text is data. It does not answer a question, authorize Save or start Run.
Input typed before a new decision is painted stays a draft rather than answering
that decision. New activity preserves a manual conversation reading position.
On a Run task list, Up/Down selects a task, Enter opens its detail and Backspace
returns. A reported child can be inspected from that detail without starting it.

## Read the evidence in its scope

Source, Plan, Graph and Check inspect the same observed candidate. A static
Check is not proof that a Run will succeed or that its business result is right.
Run, Outputs, Files and Proof describe the selected execution, including a kept
earlier result; they do not substitute today's workflow for its source witness.

A recognized knowledge-admission refusal shows a short cause, its effect scope
and a supported next step. `F2` keeps the original diagnostic available. Other
errors keep their own evidence; a generic provider failure does not gain a
guarantee that nothing was sent. Reading details neither disables knowledge nor
retries a provider.

`Ctrl+C` follows the current operation's stop contract. A stop request and an
observed settlement are separate facts. Save, Run and cost decisions keep their
own explicit answers; changing the layout or opening a panel answers none of
them.

For terminal behavior and the scope of native qualification, see the
[TUI reception evidence](qa/tui/RECEPTION.md). Live provider and human usability
qualification require their own evidence.
