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
  return unsent) or `Outcome::Failed` (the model or the tree failed; what was recorded stays).
  No step, turn, token or time quota ends a run. While a call waits, a new line answers it:
  `prompt` refuses with `AgentError::Parked`. The calls of one message run in order; the calls
  after a call that ends the turn, or after a steering line arrived, are recorded as not run,
  with the reason the model reads. A call that repeats the previous one exactly, reply
  included, is said so to the model. Every request carries the branch's latest instructions,
  the conversation (a person's line followed by its citation, `(cited as u3)`) and the Session's
  tool definitions.
- `steer` — `Steering`, shared by the host and the run: `steer` lines enter after the current
  calls, `follow_up` lines when the model would end, `drain` returns both at Stop. A blank line
  is no line.
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
