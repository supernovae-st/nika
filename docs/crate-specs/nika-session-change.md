# Crate spec — `nika-session-change`

| | |
|---|---|
| Status | **WIP · MEMBER** (size-cap split of `nika-session`, a WIP crate itself · ADR-144 · D-2026-07-09-N1 · 2026-10-06) · it joins the workspace as WIP with its unit, the `nika-tui-view` precedent |
| Layer | L4 — a library surface; lateral L4→L4 edge `nika-session → nika-session-change`, never back · its own lateral edges reach `nika-cli-host` (the check facade), `nika-display` (the review rows), `nika-onboard` (the compiler's outcome) and `nika-trace` (the run facts of a paused gate), none of which depends on it |
| Design | the project change a session proposal writes, its factual review, the typed outcome a host renders and the consent record: four modules, kept under their historical paths `nika_session::{change, consent, outcome, review}` |
| IMPL | measured by `scripts/crate-metrics.sh nika-session-change` at each freeze; the crate carries what `nika-session` held in `change.rs`, `review.rs`, `outcome.rs`, `consent.rs` and the `change_fs_tests` module on 2026-10-06 (the gate's own counter: 1,541 prod LOC at the split, 1,489 of them moved · 28 unit tests, all moved with their files) |
| LOC budget | ≤15k crate · ≤1500/file · ≤100/fn |
| Crate version | tracks workspace |
| License | `AGPL-3.0-or-later` |
| Edition | 2024 (workspace-inherited) |
| Publish | `false` — member of the `nika-session` unit |
| Dependencies | **read from `Cargo.toml`, which is authoritative** · lateral L4 `nika-cli-host` (`oracle::audit_source`, default features off), `nika-display` (`check_render::review`), `nika-onboard` (`compile::CompileOutcome` and the trigger it requested), `nika-trace` (`run_view::{RunFacts, live_again}`) · `nika-schema` (the strict parser) · `nika-fs` (`OwnedDir`) · `nika-source` (the canonical program name) · `blake3`, `serde`, `serde_json`, `thiserror` · dev: `tempfile` |
| NIKA codes | none owed — `ChangeError` is a change set's refusal (outside the root · unnamed · stale · the file system), spoken by the session with its fix |

## 1. Purpose

`nika-session` stood at 14,998 prod LOC on 2026-10-06, against the 15,000 wall, with every next
change to the Session still to land. Its `change`, `review`, `outcome` and `consent` modules formed
a closed cluster: they named only each other inside the session, while the rest of the session
reached them downward. Per D-2026-07-09-N1 a size-cap split is ONE architectural unit in several
workspace members: the cluster descends here, below the session, which keeps 13,514 prod LOC
(ADR-144).

The session re-exports the four modules at their historical paths and keeps its root re-exports
(`ProjectChangeSet`, `ProposalId`, `ConsentRecord`, …), so `nika-tui`, `nika-tui-view` and
`nika-cli` compile unchanged; `crates/nika-session/tests/change_reexport.rs` compiles against
those paths as an external consumer.

## 2. The four modules

- `change` (ADR-126) — one typed change set, built once from exact bytes and consumed by BOTH the
  preview and the apply, so the two cannot diverge. A destination is contained (no `..`, no
  absolute path, a canonical program name) and witnessed with the no-follow primitive the write
  uses; every witness is checked before the first write; each file lands atomically below the
  root's own descriptor. The preview carries the engine's own audit of the exact bytes (the
  facade `nika check` uses); after apply, the real check judges the workflow as it sits on disk,
  under the human's pinned execution access when they pinned one. That check reads its world
  once, with the run's own reader (`nika_execution::ExecutionSnapshot::capture`): the workflow and
  every child it reaches are judged from the captured bytes, the world is captured again after the
  check, and only a world that held still and that the run's own admission admits, models aside
  (`nika_execution::check_world`: every child and every skill, from the captured units), names a
  `Closure` (the snapshot digest over the workflow, its children and its skills). A
  project-relative read such as the MCP registry is served from the captured world too. A world
  the run cannot capture or admit, a symlinked workflow among them, is not clean. A run request carries the witness of the checked bytes and that
  closure; a door runs only that world (`RunRequest::admits_world`, and the child's
  `--expect-world`). A paused run's gate is read from the trace's own pause event
  (`nika_trace::run_view`).
- `review` — the factual review of a Ready candidate before any consent: what it does (its tasks
  in run order, parsed by the engine's parser), when it runs, what it can touch, what changes on
  disk and what it still needs, read from the candidate's bytes, the compiler's requested boundary
  and the set's own audit rows. No model describes a workflow. A fresh candidate lands at a free
  destination; a revision replaces only the file it compiled, over the exact base bytes.
- `outcome` (ADR-133) — the typed answers a host renders and a remote host judges by identity:
  the proposal a consent names (the witness of the exact preview), the gate or the question an
  answer names, the class of a refusal. A question's identity is held with the session
  incarnation that asked it; an answer naming it answers that question in that session, or
  nothing. `Stopped` is a conversation's turn stopped by the person: how the Stop reached the
  intelligence (`StopReach`: between steps, a dropped request that may still be billed, an
  agent asked once to stop), the queued lines returned unsent and the draft revision kept.
- `reply` — the grammar of a human's reply to one compile question, pure over the question and
  the line: an offered key named alone or carried as whole tokens, a value as typed, the one
  bounded reading prompt for a value or a choice, and the verbatim whole-token copy a reading
  may bind. The Session decides when a reading runs and what it binds.
- `consent` — the append-only `.nika/consents.ndjson` under the project: what was previewed (the
  proposal's identity, every path with the witness of the bytes it was previewed over and of the
  bytes it lands), what landed, when. One JSON object per line.

Two typed readings joined these modules on 2026-10-08, so every host reads the same facts:

- `world` — where a workflow's exact bytes reach, from the check's data journey over those bytes:
  files, an exact loopback host (a service on this machine, such as a contract server or a test
  sink), a public host (a connected service), a documentation or floor-refused host (no service),
  an MCP tool or a program (destination undetermined). Hosts are judged by the floor's own
  predicates (`nika_types::net`). The reach is `local`, `local_services`, `connected` or
  `undetermined`; every `WorkflowAudit` carries it, and unaudited bytes claim none. It is a
  declared reading: the trace witnesses file and tool permits per operation, not network
  destinations, so a run does not turn it into an observation.
- `work` — the work a session holds, typed once for every host (contract
  `nika/session-work@0`, serializable): `Waiting` names what the next line answers with the
  identity an answer names (a proposal, a gate, a run's cost review by its `ReviewId`, never its
  screen or evidence); `Work` is one snapshot of the request, the
  candidate's files with their witnesses, audits and reach, the saved workflow, the last observed
  run (`current` only for the run of the workflow saved last in this session) and the rail.
  Since the same day it also names the run requested last (`RequestedRun`: the workflow, the
  names of the inputs it binds, never their values, and the reach of the bytes the check cleared
  for it) and, on `Saved`, the reach of the bytes the last consent saved; a workflow only run
  carries no saved reach. `Authoring` is the compiler's last word on the request: its status,
  the keys of the questions it asks, each diagnostic as written (kind, target, message) and the
  witness of the candidate bytes it built, proposed or not, so a host can say why nothing is
  ready. Ready there is a compiler status, never a consent or a run. Its `calls` carry the
  receipt's totals and, in call order, each call through an allowlist (`AuthoringCall`): the
  role, the instruction and schema digests, message bytes, the count of references, the output
  and time bounds, the wall time, the stop reason or the engine's failure kind, and the
  reasoning and usage the provider reported. A fact the receipt does not hold is null, never
  guessed or summed; prompts, answers, proposed objects, served model names and error text never
  pass. Its `stages` carry the time the compile's other stages took, as its decision record
  states them (`StageTimes`): the knowledge qualification's wall time (`qualification_ms`, its
  references asked of the decision seat as one batch) and each trial of a candidate in the order
  run (`TrialTime`: how far it went, its `elapsed_ms` and the host's `runtime_bound_ms`), never
  summed, null where the record states none, and absent when it states neither; no output,
  read-back or failure text passes.
  `Knowledge` is the authoring knowledge the session reads, one typed state: `admitted` (the
  release's `source` — `embedded` or `disk` —, `version`, `manifest_sha256` and who chose it:
  `default`, `conversation`, `host` or `environment`), `refused` (the named source's kind, the
  layer that named it, the refusal's stable code and its cause, never a host path) or `unread`
  (why none is read). It is a configured fact, never a call receipt. While a refused source
  holds a line that would reach a model, `Waiting::KnowledgeChoice` carries that line exactly as
  typed, between the intelligence choice and a proposal's consent in the precedence.
  `Answered` says what the last line typed for an authoring question did, as the session recorded
  it at the act: the question's witness and one act, `bound` (the key, the value and how the
  line gave it: `as_typed`, `offered_key`, `model_read` or `seat_default`), `dropped`, `restated`, `waits` (with
  the reason) or `refused` (the refusal's class). It is absent when the last line was no answer.
  The candidate's `revision` is the compiler's record of how it made the workflow, only while
  that record binds the candidate's exact bytes: a creation (`written` or `composed`, no base) or
  a revision over a base (`operations` or `replaced`), each component witnessed on those bytes.
  `RunEnd` is the one reading of the run door's exit codes. It grants nothing: consents,
  answers and runs still go through the session's own doors.
- **`tools`** — the tools a session serves to the intelligence that leads its conversation
  (`CONTRACT` = `nika/author-tools@0`), typed once for Nika's own agent loop and for an ACP
  agent's loop reaching them over MCP (`nika-mcp`'s conversation tool server). `NAMES` fixes
  the 17 tool names, each a lowercase word an MCP client mounts unchanged (`mcp__nika__<name>`).
  `SessionTools` is the one trait: `tools()` lists the `ToolDef`s (name, description, argument
  JSON Schema, read-only), `call(ToolCall)` runs one call (name, arguments as sent, `meta` = the
  caller's tool-use id when known) on the caller's thread and answers a `ToolReply` (text,
  `is_error`, `ends_turn`: Nika's loop parks its run on it; MCP returns the text). The session
  implements it against its own state; a loop or a transport only lists and relays, and no reply
  grants anything. The Session writer owns the contract's evolution.

## 3. Boundary

- The member owns no conversation, round, money gate or history: the session decides when a set
  is proposed, previewed, consented to and applied, and writes the consent record at apply.
- The seams the session reads across the boundary are public items of the member:
  `change::{ApplyAttempt, check_with_access}`, `ProjectChangeSet::{apply_attempt, project_reads}`,
  `review::{parse, propose_over, task_face, NOTHING_RAN}` and
  `outcome::{Incarnation, QuestionId::new, QuestionId::asked_by}`. They were crate-private
  inside the session; the crate boundary makes them public. No consumer outside the unit names
  them.
- `ConsentDecision` stays `#[non_exhaustive]`; across the member boundary the session matches it
  with a wildcard that names nothing it did not decide.
- This crate never depends on `nika-session`.

## 4. Related

- ADR-144 (this split) · ADR-125 (the native session) · ADR-126 (project changes from the
  session) · ADR-133 (the portable session machine) · D-2026-07-09-N1
- `docs/crate-specs/nika-session.md` · the session, the owner of the conversation

## A conversation's values on the work snapshot (2b)

`work::conversation` types what a conversation an intelligence leads holds, additively on
`nika/session-work@0`: `Binding` (a value, its `ValueRole`, the question key it fills, and its
`Provenance`: kind, the person's citation, their words, the question and option an accepted offer
names), `Delegation`, `AskedQuestion` (its identity as a witness, open or after its
prerequisites, its `Offer`s with the concrete `OfferValue`s accepting each binds), and
`Waiting::Questions` for several questions asked together. `Work::with_conversation` sets them;
a snapshot without a conversation serializes none of these keys, so its bytes are unchanged.
Values, provenance and delegations also read back (`Deserialize`): a Session keeps them across a
reopen as evidence; questions and offers never are.

An offered value of role `run_model` carries `choice` (`ModelFacts`) when this machine's model
inventory offers that model: the role it would serve, the model, the route (`via`, its class,
whether it is ready, how it bills) and the catalogue's output list price on a metered route —
none when the inventory offers no such model, never the author's words. `Work::with_queued`
sets `queued`: the lines the person sent while the conversation's last run was under way, each
with its identity, mode and state (`work::queue`).
