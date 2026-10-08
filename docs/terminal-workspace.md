# The terminal workspace

Bare `nika` in an interactive terminal opens the workspace. Start typing in the
conversation; opening a workflow or a Run object does not attach it to your next
message. The project list, conversation and object are views of the same Session.

## Arrange the same work

Use the selected object's **Expand** control, or `F4`, to give it more room.
**Restore** returns to the chosen proportions. Both actions keep the draft,
selected object, face, conversation reading position and Run; they send and authorize
nothing. When the object changes between compact graph rows and cards, its
reading restarts at the top so the new format begins with whole tasks.

The workspace places the conversation beside the object from 100 columns; the project
list appears on the left from 120 columns. At narrower sizes the conversation
sits below the object. When the project is folded, the header names Project,
Conversation and Object; choose a name to move keyboard focus, or use `F6`.
Expanding the object keeps the conversation beside it from 100 columns and
gives the object more height at narrower sizes. Its local control stays clear
of the object's title and face tabs, uses a shorter form when needed, and
yields when no form fits. Expand is offered only when it can give the object
more room; Restore remains available after expansion. The command palette
offers the same action and availability. Below 60 columns or 16 rows, the focus
view keeps the conversation usable until the workspace fits again; pane access
and object expansion return with that space.

Drag a visible separator to change its proportions. With the project list or
object focused, `+` grows that region, `-` shrinks it and `0` restores its
automatic proportion. These characters remain ordinary text in the composer.
Resizing applies the chosen proportions within bounds; it does not overwrite
them. On a wide expanded view, dragging the conversation separator toward the
object restores the regular arrangement at the width you choose. Shrinking
the object with `-` does the same. Restore otherwise returns to the proportions
you had before expanding.

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
that panel. When the workspace fits, choosing conversation navigation focuses
the conversation instead. Choosing a command returns focus to the composer.
Clicking another panel closes the palette and keeps the clicked panel's focus with the draft
restored.
The full diagnostic owns its whole screen: clicks cannot activate the workspace
behind it. Closing the diagnostic returns to the same draft and view.
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
| Read the full words behind the latest summarized diagnostic | `F2` |

The chooser lists the commands the Session exposes, with each command's scope. `/restore` appears only when the
Session offers kept work for a fresh review. Model names do not establish route
capabilities; authoring, decision and Run information retains its own scope.
Choosing intelligence does not rewrite a previously observed Run.

## Keep the conversation in reach

Ordinary messages share a continuous surface, identified by their speaker.
Questions, approval requests and failures keep a distinct boundary so the next
decision remains visible. Scrolling preserves the complete text in either form.
On a tall, side-by-side workspace, the composer has a quiet frame and a
`Your message` or `Your answer` caption. It grows with the draft; short and
stacked panels keep the compact input. The selected preparation intelligence
has one home in the workspace header. `/status` gives its full name and the
project's location and configuration facts when the header is too narrow.
The input cursor appears only while the composer holds the keys; the draft
and its insertion position stay when focus moves to another panel.

| Gesture | Effect |
|---|---|
| `F6` / `Shift+F6` | Move keyboard focus between panels |
| `F4`, or the object's Expand / Restore control | Expand the selected object or restore the chosen proportions |
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
An open command list uses `PgUp` / `PgDn` to page its own entries; Escape closes
it before the panel's navigation keys apply again.
Input typed before a new cost decision is painted is kept whole in the
conversation and cleared from the answer box, including words set aside by
the palette. Escape cannot restore those words as a cost answer. Other new
decisions keep prior input as a draft rather than answering that decision.
New activity preserves a manual conversation reading position.
On a Run task list, Up/Down selects a task, Enter opens its detail and Backspace
returns. A reported child can be inspected from that detail without starting it.

## Read the evidence in its scope

Source, Plan, Graph and Check inspect the same observed candidate. A static
Check is not proof that a Run will succeed or that its business result is right.
Graph leads with the task cards and a compact definition summary. Its structural
verdict is scoped to the inspected file; the read identity remains visible.
A short object uses complete dependency rows for plain value dependencies;
typed conditions, shortened labels and drawing limits retain the detailed
rendering. More above/below indicates reachable content in the same object.
Source and Check retain the detailed capture and audit facts.
Run, Outputs, Files and Proof describe the selected execution, including a kept
earlier result; they do not substitute today's workflow for its source witness.

A recognized knowledge-admission refusal shows a short cause, its effect scope
and a supported next step. `F2` keeps the original diagnostic available;
Escape or Enter returns to the kept draft without sending it. Other
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
