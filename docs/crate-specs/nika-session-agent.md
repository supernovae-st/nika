# Crate spec — `nika-session-agent`

| | |
|---|---|
| Status | **WIP · MEMBER** of the `nika-session` unit (ADR-153 · the ADR-144 pattern · 2026-10-09) · it joins the workspace as WIP with its unit and is admitted with it |
| Layer | L4 — a library surface; lateral L4→L4 edges `nika-session → nika-session-agent → nika-session-change`, never back |
| Design | the conversation the Session's selected intelligence leads: the loop over one model and the Session's tools, the person's steering and follow-up lines, the Session tree and its compaction, the event stream a host renders |
| IMPL | measured by `scripts/crate-metrics.sh nika-session-agent` at each freeze (1,763 prod LOC at its first commit, `scripts/ci/prod-loc.py`) |
| LOC budget | ≤15k crate · ≤1500/file · ≤100/fn |
| Crate version | tracks workspace |
| License | `AGPL-3.0-or-later` |
| Edition | 2024 (workspace-inherited) |
| Publish | `false` — member of the `nika-session` unit |
| Dependencies | **read from `Cargo.toml`, which is authoritative** · `nika-kernel` (`provider::{Message, ContentBlock, ToolDef, StopReason, TokenUsage}`, `CancelCtx`) · lateral L4 `nika-session-change` (`tools::SessionTools`, the one tool contract) · `blake3`, `serde`, `serde_json`, `thiserror` · dev: `proptest` |
| NIKA codes | none owed — `TreeError`, `AgentError` and `ModelError` are the Session's own refusals, spoken by the Session with its fix; never evaluated as a workflow task diagnostic |

## 1. Purpose

The Session is led by the intelligence the person selected: the model reads the conversation,
calls the Session's tools, reads their replies and goes on until it answers. This crate is that
conversation and nothing else. It carries calls and replies and never interprets them; the
Session's tools act on the Session, and only the Session's doors save, run or record a consent.

## 2. Modules

- `run` — `Agent` drives one conversation over a `Tree`, a `Store` (where each line goes before
  the tree takes its entry), the Session's `SessionTools`, a `Model` and a clock (`now`, Unix
  milliseconds). `Agent::prompt` starts a run on a person's line; `Agent::answer` answers the
  call the run waits on; `Agent::compact` folds the branch on request. A run ends as
  `Outcome::Answered` (the model answered without a call), `Outcome::Parked` (a call's reply
  ended the turn: the run waits for the person), `Outcome::Stopped` (Stop; the queued lines
  return unsent, and `Agent::stop_reach` says how the Stop reached the run: between steps, a
  dropped request, or an agent asked once to stop) or `Outcome::Failed` (the model or the tree
  failed; what was recorded stays).
  No step, turn, token or time quota ends a run. While a call waits, a new line answers it:
  `prompt` refuses with `AgentError::Parked`. The calls of one message run in order; the calls
  after a call that ends the turn, or after a steering line arrived, are recorded as not run,
  with the reason the model reads. A call that repeats the previous one exactly, reply
  included, is said so to the model. Every request carries the branch's latest instructions,
  the conversation (a person's line followed by its citation, `(cited as u3)`) and the Session's
  tool definitions. `run::lead` is the same contract for an agent that runs its own loop and
  keeps its own history (an ACP agent, a `Conversant`): `Agent::lead` sends the person's entry
  as one prompt (the answer to a waiting call as `The person answered, cited as u3:`), reads the
  turn's beats, asks the agent once to stop on Stop or a steering line, and records the calls a
  `Relay` served (each as the agent's call and its reply, the one that waits for the person
  parked), the transport's record (`Fact` `led_turn`) and the answer. The relay refuses a call
  between turns and after a call that ended the turn; `transcript` gives an agent whose own
  session did not see the conversation its cited lines and what the author said and asked.
- `steer` — `Steering`, shared by the host and the run: an identified ledger. Each queued line
  has an identity (`l1`, `l2`, … for the conversation) and a state a host shows: `waiting`,
  `entered` with the citation it became, or `returned` (unsent). `steer` lines enter after the
  current calls, `follow_up` lines when the model would end, `drain` returns both at Stop. A
  line is accepted only while a run reads the queue (the run opens it and closes it; what it
  did not read returns), so a line is never kept for a later run: `QueueRefused::NotReading`,
  `Blank` for a blank line, and `Full` once the run took `MAX_QUEUED` lines.
- `observe` — `Observed`, the Session's tools as both loops reach them (the relay wraps it for
  an agent with its own loop): each real call reports a `ToolStep` (its identity, the tool,
  started, finished or failed, the milliseconds it took) to the turn's sink, never its
  arguments or reply.
- `tree` — `Tree`: the header (`nika/session-tree@0`, the Session, the project's storage digest,
  never a path), then one entry per line (`System`, `User`, `Assistant`, `ToolResult`, `Parked`,
  `Compaction`, `Stopped`, `Fact`), each with its parent and time, each line bound to the one
  before it by a BLAKE3 digest over its place and body. `Tree::replay` refuses a cut file
  (`Truncated`) and a line that does not follow, misspells a field or does not match its digest
  (`Damaged`, with the line), whole: nothing is reset. A citation is minted by the tree
  (`append_user`) and never reused; `cited` resolves one on the branch for the Session's
  authority checks; `parked` names the call the run waits on. `context` is what a model reads:
  every call of a message followed at once by one reply per call, in call order (a tool's reply,
  the person's answer to a call that waited, or "not run" for a call Stop left), and a summary
  standing for the entries it folded.
- `compact` — `Window` (the route's context window, the reply's reserve, the verbatim tail),
  `estimate` (the last report since the last compaction and about four characters a token after
  it), `cut` (a person's line, never an answer, with conversation before it to fold; the first
  walking back whose tail weighs at least the tail to keep, else the oldest) and `request` (the
  folded part as a transcript, citations and call identities kept, with what the summary must
  keep). A summary is no answer: nothing it streams reaches the person.
- `event` — `AgentEvent` (`nika/session-events@0`): run start and end, each request, streamed
  text and thinking, each recorded message, usage, each call's start, end or skip, each queued
  line entering, each compaction.

## 3. Contracts kept

- Only a person's line carries authority. The model's messages, tool replies, summaries and
  facts are evidence; the Session resolves the citations the model gives (`Tree::cited`).
- An entry is in the tree only once its line is durable (`Tree::append` writes, then takes).
- The loop has no quota: a long run ends on the model's answer (unit test with 120 calls).
- No I/O in the crate: the Session's history owner implements `Store` and reads the file.
- The ask door asks only what no default can stand for: `Conversation::ask` refuses a question
  that recommends an option, or that names an output in the project unless its `reopens` cites
  the person's later words, each refused question with its reason (`default`, `output`, beside
  `settled`, `delegated` and `bound`); a choice with a default is shown by `propose` in `open`.
- A schedule is never assumed: `Conversation::schedule` takes `propose`'s `schedule` only with
  the cited line's own words and a zone the person's words name or a choice the proposal shows
  offers as its default; such a choice may offer the zone, which the candidate never carries.

- A conversation keeps the workflow it saved last with the witness of the bytes saved
  (`Conversation::saved_at`), across a reopen and a new request: what a revision replaces.
- One output per workflow, except where an open name starts: a question whose offers read one
  name asks that name's start alone. An output and a question stand apart only when one of them
  is such a name and every output the other may hold lies outside it, judged by the offers each
  question showed and never by the value picked, in whichever order the person answers; a choice
  that offers one of the name's readings asks for the same output.
- What a conversation keeps holds each answer, never the question it answered. After a reopen,
  `Conversation::recall` reads back from the tree which question each kept output of the request
  answered, only when the person's line answered the call that showed or asked it, the reply names
  it, the call's input holds it once and the offer picked (or the typed line) holds the value. It
  restores that evidence alone, never kept: no question, answer, consent or authority; a missing
  or conflicting record keeps the single-output rule.
- A line the person writes in reply to a parked `ask` is kept for its request as where a reply
  stands (`Conversation::replied_by`, read back from the tree by `recall` after a reopen; a natural
  rerun, `RERUN_NOTE`, is never one). `Conversation::replies_in` locates every such line by its
  citation, bound or not, so a reply never names the workflow's file; it binds, settles and
  authorizes nothing.
- A reply to another question of an `ask` group, given by its identity after the call stopped
  waiting, is such a line too. `conversation::asking_call` finds the call from the question's
  identity on the branch (the one successful `ask` whose reply names it, its input holding each
  question once), and `Agent::with_late_reply` records the line under that call. Only the first
  line naming a call is its answer, the call's one tool result: a later one reads as any
  person's line in the context, an agent's transcript, a fold and its cut, with the Session's
  reading of that line beside it (`Agent::user` keeps the reading with the run's own line,
  whether a call waits or not, on the API and the led routes alike; never with a queued line).
  Nothing is re-asked or answered, and no consent or authority returns.
- `Conversation::answers` keeps selected offer keys and labels beside their bound values
  and cited provenance for semantic verification. An offer without business values still
  carries its selection receipt; it invents no binding. Only selected alternatives enter
  that context, including after reopening. This evidence grants neither Save nor Run.
- The native « Let Nika choose » act is not words: `Conversation::delegated_one_by` binds the
  one recommended offer of the question the host named as delegated, whatever an offer's
  label reads (with none, the author chooses). It keeps an exact continuation receipt, marked
  `delegated`, as an exact protocol pick does; a reading of free words that delegates keeps
  none. On a shown choice that recommendation is its default, so the act continues the
  shown revision's mandate (`picked_default`) once, and a replayed or reopened act mints
  nothing.

## 4. Gate evidence (admission with the unit)

| Gate | State |
|---|---|
| 1 SPEC | this document · ADR-153 |
| 2 TDD | partial: the unit and property tests were written with the code, before any build, and the first build ran them green; no red run was recorded, so the red half is owed at admission |
| 3 IMPL | compiles on the tool contract (`nika_session_change::tools`); `cargo test -p nika-session-agent --lib`: 32 passed, 0 failed |
| 4 CLIPPY | `cargo clippy -p nika-session-agent --all-targets -- -D warnings`: clean |
| 5 MUTATION | pending (`scripts/ci/check-mutation-floor.sh nika-session-agent`) |
| 6 PROPERTY | `tree::tests::any_conversation_reads_back_and_pairs_every_call`, `tree::tests::any_altered_character_is_refused` |
| 7 BENCHMARKS | N/A: a request waits on the model; a tree is read once per open, linearly |
| 8 DOCS | `RUSTDOCFLAGS="-D warnings" cargo doc -p nika-session-agent --no-deps`: clean |
| 9 CANARY | pending: the Session drives the loop in its journeys |
| 10 PARITY | N/A: a new capability, no legacy behaviour to match |
| 11 REVIEW | pending |
| 12 ATOMIC | the unit's admission commit |

## Host helpers (2d)

`tree::read_line` reads what one durable line says (its place and, for a person's line, the
citation, the words and the call it answers) without the line format leaving the crate: the
Session indexes the person's lines as they are appended, while the loop holds the tree.
`Tree::next_cite` names the citation the next person's line will get, and `Tree::last_said` the
text of the model's last message on the branch.

**Overflow.** A request over the model's context window is `ModelError::Overflow { window }`
(the agent model maps the provider's typed `ContextOverflow` to it before any retry), and a reply
that filled the window (`Reply.window_full`) is no answer: neither is recorded. The loop folds the
conversation once and asks the same step again; a step that already folded (before its request,
or once after an overflow) fails naming the window, and so does a conversation with nothing
earlier to fold. A cut falls at a person's line, or, within the run the last person's line
started, at a later step of that run whose earlier calls all have their replies, so a long run
from one line folds between its balanced steps. A summary no lighter than what it folds folds
nothing. The complete summary payload is budgeted with the conversation's UTF-8 byte/4
estimate, including instructions, focus, typed transcript and carried summary. A larger fold is
summarized in whole-object parts; a fixed prompt or indivisible object over the window less its
reserve is refused without calling the model or folding originals. Each request carries the
summary written so far; a later refusal appends no partial `Compaction`. Provider tokenization
remains authoritative, and a provider-reported summary overflow also folds nothing.
