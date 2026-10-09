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
of the object's title and face tabs and uses a shorter form when needed. With
no room on the title row, it moves to the end of the object's More above/below
row, or of a last row the object leaves empty, so it never covers a line, a
face or that cue; it yields when no form fits. Expand is offered only when it
can give the object more room; Restore remains available after expansion. The
command palette offers the same action and availability. Below 60 columns or 16
rows, the focus view keeps the conversation usable until the workspace fits
again; pane access and object expansion return with that space.

Below 100 columns, with the object restored, the proposal under review or the
question at your answer line can need more rows than the conversation has; for
a question, that includes three rows of the exchange that led to it. The object
then folds to a three-row strip: its title and faces, one row and its More
above/below row. The conversation takes the rows the object gives up; scroll it
for anything longer. `F4` or the strip's Expand control still expands the
object, and Restore returns to the strip while that decision waits. Your chosen
proportions are untouched and return once the decision ends; the fold follows
the current size and decision and is never saved.

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
`Ctrl+O` (or `Tab` in an empty box) to open the command palette. Each command
has an effect, scope and short description. Choosing a slash command inserts
it into the composer; press Enter separately to send it. Escape closes the
palette and restores the draft and the panel that held keyboard focus. A view
key chosen in the palette acts from that panel. When the workspace fits,
choosing conversation navigation focuses the conversation instead. Choosing a
command returns focus to the composer. Clicking another panel closes the
palette and keeps the clicked panel's focus with the draft restored.
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
| Close the Session; nothing waiting for your answer is applied | `/quit` |
| Read the complete question at your answer line, the proposal under review or the latest shortened refusal | `F2` |

The chooser lists the commands the Session exposes, with each command's scope.
`/restore` appears only when the Session offers kept work for a fresh review.
Model names do not establish route capabilities; authoring, decision and Run
information retains its own scope. Choosing intelligence does not rewrite a
previously observed Run.

`F2` opens the Session's own words over the whole screen, read only: the
question waiting at your answer line, the proposal under review, or the latest
refusal shown in short form. `Up` / `Down`, `PgUp` / `PgDn`, `Home` and `End`
scroll them. `Esc`, `Enter` or `F2` closes the reader and returns to the same
draft, focus and view; any other key closes it and then acts as usual. Clicks
cannot reach the workspace behind it. Reading sends, saves and runs nothing.

## Keep the conversation in reach

Contiguous messages from one speaker share a bubble. You stands to the right;
Nika stands to the left. Roomy panels draw quiet outlines; short panels use a
compact label row instead. Questions, proposals waiting for Save, approval
requests and failures keep a framed card so the next decision remains visible.
Scrolling preserves the complete text in either form. A framed card you scroll
into part-way names itself on its first visible row and counts the rows above
it when that row has room for both. While you read the latest messages, a conversation shorter than its panel
rests on the panel's last row, so its latest message stays next to the
decision or composer below it; reading earlier messages keeps your place.
On a tall, side-by-side workspace, the composer has a quiet frame and a
`Your message` or `Your answer` caption; the caption is left out while a
question card stands directly above the answer line, or while a proposal or
an approval request waits: its `Save? ›` or `answer ›` prompt names the
line. The composer grows with
the draft; short and stacked panels keep the compact input. The selected
preparation intelligence has one home in the workspace header, shown as a
fact: `/intelligence` changes it, and `/status` gives its full name and the
project's location and configuration facts when the header is too narrow.
The input cursor appears only while the composer holds the keys; the draft
and its insertion position stay when focus moves to another panel.

| Gesture | Effect |
|---|---|
| `F6` / `Shift+F6` | Move keyboard focus between the project list, the conversation and the object |
| `F4`, or the object's Expand / Restore control | Expand the selected object or restore the chosen proportions |
| `Esc` in the project list or the object | Return to the composer |
| `Esc` in the conversation | Leave the workspace for the inline view; your draft stays and `Ctrl+T` returns. While Nika works, it waits for your turn |
| `Enter` in the project list | Open the selected entry for inspection |
| `Up` / `Down`, `Home` / `End` in the project list | Move the selection |
| `Left` / `Right` in the project list | Switch between the Nika and Files lists |
| `Left` / `Right` in the object | Change the object's face |
| `Up` / `Down`, `PgUp` / `PgDn`, `Home` / `End` in the object | Scroll the object |
| `r` in the object | Ask its host to read it again |
| `PgUp` / `PgDn` in the conversation | Read earlier or later messages |
| `End` in the conversation while reading earlier messages, or the `↓ latest` marker | Return to the latest messages |
| Mouse wheel over a panel | Scroll that panel |
| Click | Focus a panel, open a listed entry or face, or select an offered answer; a click never sends, answers, saves or runs |
| `Shift` + drag | Select text with your terminal, where it supports this |
| `Up` / `Down` at the first or last line of the composer | Recall lines you sent earlier; with offered answers shown and an empty box, they select an offer instead |
| `Alt+Enter` in the composer | Insert a line break (`Shift+Enter` and `Ctrl+J` also do where the terminal reports them) |
| `Ctrl+T` | Switch between full screen and inline |
| `Ctrl+L` | Redraw |

`Esc` never pauses, cancels or answers a question; type `cancel` to drop it.

Pasted text is data: it fills the box, or the palette's search while the
palette is open. Nothing is sent, answered, saved or run until you press Enter.
An open command list uses `PgUp` / `PgDn` to page its own entries; Escape closes
it before the panel's navigation keys apply again.
Input typed before a new cost decision is painted is kept whole in the
conversation and cleared from the answer box, including words set aside by
the palette. Escape cannot restore those words as a cost answer. Other new
decisions keep prior input as a draft rather than answering that decision.
New activity preserves a manual conversation reading position.
On a Run task list, Up/Down selects a task, Enter opens its detail and Backspace
returns. A reported child can be inspected from that detail without starting it.

## Answer the question in view

In the workspace, the question waiting for your reply stands directly above
your answer line: `Question`, the kind of answer it takes (offered answers, a
text answer or an exact value), whether it is required, then the Session's own
words. Its normal word budget is six rows or a quarter of the terminal's
height, whichever is more; it shows one extra row when that keeps all the words
in view. The card never spends a row on the `F2` pointer to hide a single row:
when more words remain, its last row points to `F2` (`… the whole question: F2`).
Its place in the conversation reads `↓ the question waits below`. While Nika
works, while a command list is open, in the inline and focus views, or when the
panel is too short, the conversation shows the question's words in full instead.

While the card stands there, rows that would only repeat it make room for it:
the status `Needs one answer` for this same question, and the lifecycle row of
a first draft (`Draft ● · Saved ○ · Checked ○ · Active ○ · Run ○`). Any other
status or lifecycle row, and a pending `Ctrl+C` exit, keeps its row.

Offered answers keep the compiler's order, and nothing is preselected. While
the reply box is empty and holds the keys, `Up`, `Down`, `Home` and `End`
select an offer and `Enter` sends its exact key. A click selects an offer
without sending it. Typing or pasting makes the box your answer instead. While
a question is shown this way, every line you send, a command included, is sent
with that question's identity and the Session decides what it is; an old or
unrelated question cannot turn it into a new request or Save consent. An empty
reply is refused unless the question's own words offer a default, such as
keeping the model you already chose. Type `cancel` to drop the question.
Other questions keep their words in the conversation and take your reply as an
ordinary line.

If the Session does not take a reply and its question still waits, or the reply
never reached that question, the exact text returns to the box before any newer
unsent words, or the offered choice stays selected. Nothing is replayed
automatically. When a cost decision takes the box, or when the question closes
with a refusal, your words stay in the conversation instead. The workspace never
infers from an outcome that an answer was applied: the hint describes
restoration only, and a restored draft, a received reply and a completed
workflow remain separate facts.

When the Session reports the value it took from your reply to the question on
screen, the conversation says `Answer taken` with the question, that exact
value and, when the Session names it, how it was read: as you typed it, one of
the offered answers, a verbatim part of your reply chosen by one reading call,
or the offered default from your selected connection. A taken value is neither
a Save nor a Run; the outcome of the draft is reported separately.

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
and a supported next step. `F2` opens the original diagnostic; closing it
returns to the kept draft without sending it. Other
errors keep their own evidence; a generic provider failure does not gain a
guarantee that nothing was sent. Reading details neither disables knowledge nor
retries a provider.

## Save and Run stay separate

A proposal waits under `Save? ›`, and the status row says
`Not saved yet · yes means Save only`. In the workspace, its review in the
conversation first says what a `yes` answers: nothing is saved yet and nothing
has run on your files. It then lists every change it lands (counting any whose
bytes no face shows), how the workflow was revised, what it reaches under one
`when it runs` heading, where it reaches as declared and its rehearsal, and
ends its facts with a quiet line naming the proposal and the identity of its
exact bytes; scroll the conversation to read every fact. `F2` reads the
Session's complete words and `/show` prints every byte.

Type `yes` and press Enter to write exactly those bytes; Nika checks them and
runs nothing. `no` discards the proposal. You can then send `run it` under an
announced ceiling. Or type `save & run` while that proposal waits: this explicitly
requests its Save followed by one Run. The reviewed source and its witnessed
world are still checked; the Save and Run keep separate outcomes. A refusal to
start the Run does not undo a successful Save. When a run's cost cannot be
estimated, a separate `yes / no / details` question approves that one run only.
The actual native journey verifies the shown source, saved bytes, one execution
and the useful output; inspecting that output or resizing repeats nothing.
Selecting,
clicking, pasting, answering a question, reading a face or changing the layout
never saves or runs. See
[Save and Run are separate](usage/conversational-session.md#save-and-run-are-separate).

## Stop and keep typing

While Nika prepares a reply or a workflow, you can keep typing. The first
`Ctrl+C` asks that preparation to stop; your conversation stays and the hint
row says so. A stop request and an observed settlement are separate facts, and
a call already sent cannot be recalled. Words sent with `Enter` during a
preparation also ask it to stop and are queued as a correction: they are sent
at the next free prompt, returned to the box when a question, proposal,
approval or choice is waiting, and kept in the conversation under a cost
decision. A command you type then waits in the box for your turn, and `Enter`
on an empty box sends nothing.

The workspace offers no Stop for a Run, and `Ctrl+C` does not stop one from
this view; while a Run executes, typing waits for your turn. A second `Ctrl+C`
leaves Nika after restoring the terminal, and leaving undoes nothing already
sent or started. With nothing running, the first press asks for a second to
leave, and any other key keeps the session. At a Run cost question or the
one-time unknown-cost choice, `Ctrl+C` declines it.

Save, Run and cost decisions keep their own explicit answers; changing the
layout or opening a panel answers none of them.

## Colour, ASCII, motion and assistive technology

- `NO_COLOR` (or `CLICOLOR=0`) turns colour off unless `CLICOLOR_FORCE` is
  set. The workspace then paints no hue; focus, selection and states keep their
  words, marks, weight, underline or reverse video.
- `nika --ascii` draws the workspace's own marks, borders and separators in
  ASCII. The Session's own words are shown as written and can still contain
  characters such as `·` or `○`.
- `NIKA_REDUCED_MOTION` (any non-empty value) keeps the activity marker still
  while retaining measured elapsed seconds, current work and Stop status. It
  rings no bell after a long turn and shows the welcome mark at once. Idle
  views stay silent.
- The full-screen workspace captures the mouse; hold Shift to select text with
  your terminal where it supports this. Every pointer action has a keyboard
  route.
- `nika --plain` (or `NIKA_TUI=0`) opens the same Session as plain lines for
  screen readers, recorders and scripts. The workspace itself has not been
  qualified with assistive technology.

For terminal behavior checks and their limits, see the
[TUI reception record](qa/tui/RECEPTION.md), written on 29 September 2026
before this workspace. Live provider and human usability qualification require
their own evidence.
