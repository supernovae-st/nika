# Native conversation history

Bare `nika` keeps one local conversation history per canonical project root under
`~/.nika/sessions/<project-digest>/events.ndjson`. Reopening the terminal restores
the goal, explicit decisions, open questions and recent dialogue. The project,
available intelligence and engine facts are observed again on each open.

`SessionRuntime::open` and `open_with` remain ephemeral for existing embedders.
An embedder opts in with `enable_history(home)` immediately after opening. The
native terminal opts in when its home directory is available, displays the
recovery notice, and refuses to continue if history cannot be opened. Without a
home it explicitly announces a temporary conversation.

## Ownership and recovery

A nonblocking OS file lock owns the history writer for the lifetime of the
runtime. Aliases of the same canonical project share that lock. The lock file
is retained after close; deleting it would allow two different inodes to be
locked independently. This is local process ownership, not distributed fencing.

An operation appends `started` before calling the existing runtime, then
`completed` before returning its outcome to the host. The first journal record
is published with file and directory synchronization, after synchronizing each
parent during history-directory initialization; subsequent records are
appended and synchronized. An append failure blocks that runtime, including
when its caller ignores the error. Closing remains available.

An unfinished operation or a run request without an observation produces an
uncertainty notice on reopen and an explicit note in the next model context.
An I/O refusal while applying a proposal also retains its possible partial
effect. Recovery never calls a model, applies a file, or starts a workflow.

Previous proposals, gate answers and consumed identities do not regain
authority. A person must request a fresh proposal, inspect it, and consent
under current file witnesses. Restored conversation text supplies context;
it is not a permission. The ordinary apply/check/run path remains the owner
of execution.

`RunRequested` is recorded before the terminal calls the existing run path.
The terminal receives the exact trace from that invocation's `RunVerdict`,
including a paused or resumed leg. Concurrent runs and modification times
cannot select a different run's trace. A pre-run refusal supplies no trace.
The journal does not yet correlate an interrupted request with a particular
job through an idempotent submission key. It therefore does not recover a run
by guessing from the latest trace, nor promise automatic resume or exactly-once
external effects. `observe_run` remains a host-supplied observation.

A saved workflow that was rehearsed runs only over the world it was rehearsed on.
When this session observes its own run of exactly those bytes settle as
succeeded, each destination of the rehearsal that the run completed writing (a
`nika:write` that settled, the same task's write permit and its output naming
the path) is read again and bound to what it holds now. That lets the next
explicit Run replace its own output. A source is never advanced. A permit
without a completed write, a failed, paused or cancelled run, another
workflow's bytes or a missing trace advance nothing. The write and that re-read
are not atomic: a foreign write landing between them is bound as the run's
own. Any change after the re-read, and any change to a source, still withdraws
the rehearsal before the next Run.

`observe_run_leg` additionally keeps the observed execution, source hash and
receipt head/length in an optional versioned `last_run` value. Session owns its
persistence; `nika_trace::run_view::KeptRun` is only an observation, with no
history or authority. Reopen exposes it and repaints retained dialogue as history;
it does not restore task rows, outputs, a file inventory or execution. The
workspace verifies the journal again only when Proof is opened. A resumed leg
has its own execution and journal; no identity is inferred from a previous leg.
Legacy absence stays absent. An unreadable kept value is preserved and reported,
not repaired; a present null is refused. Older engines whose closed history
format lacks `last_run` refuse new records rather than silently discarding it.

## Format and limits

The private format is versioned and hash chained. Unknown versions/fields,
invalid transitions, wrong project binding, truncated records and damaged
hashes are refused. Corrupt bytes are preserved. Hash chaining detects
accidental damage; a writer able to forge the entire private file can also
recompute its hashes. It is not an authentication boundary.

The journal retains operation inputs, outcome categories and conversation
projections. New outcome diagnostics are constant category names, without
user, model or command payloads. Older diagnostic strings remain readable
but are never deserialized into executable commands. Redaction happens on
raw text before JSON escaping; Debug output is not a safe redaction input.
The projection sent to the model
keeps the existing eight-turn window; this is distinct from journal retention.
The broker's existing redaction patterns also protect persisted text. They
are not an exhaustive secret detector. No credential store, model availability
snapshot or hidden reasoning is serialized.

The local directory and files use `OwnedDir` and its private modes (0700/0600).
Contained opens refuse symlinks and special files; opening a FIFO does not
wait for another process to connect. The project tree receives no conversation
file. Input is limited to 64 KiB, a record to 1 MiB, and a journal to 16 MiB;
capacity is reserved before starting an operation. Reaching capacity refuses
new work and preserves history. Automatic compaction, archive rotation,
multiple named conversations and migration tooling are subsequent work.

## Verification

Runtime tests exercise reopen without inference, old-consent refusal, private
redaction, blocked storage, corruption, limits and symlinks. Child processes
exercise writer exclusion, normal reopen and SIGKILL during inference. The
terminal driver checks recovery and corruption exit behavior. The JSONL
`session_script` example accepts an optional `home` for process-level tests;
it is a host over the same runtime and never executes a workflow itself.

Separate CLI tests execute actual workflows: a trace that sorts ahead cannot
hide this invocation's result, paused/resumed legs keep their exact traces,
and a durable conversation applies a proposal, performs a file write, records
the result and reopens without replaying the write or accepting old consent.

These checks establish local conversation continuity on the tested platform.
They do not qualify power-loss behavior, cloud replication, learned memory,
cross-session retrieval, or transport parity.

## Byte-bound program evidence

The optional `Saved.programs` field adds bounded semantic and source-revision
records to the existing HOME history, with no parallel project file. It is
omitted when absent, so pre-existing record digests are unchanged. Older
engines whose strict `Saved` schema does not recognize it refuse newer history;
no downgrade/reset is attempted. Unknown envelope versions remain opaque and
unchanged in engines that know the field.

Records are keyed by proposal identity or saved relative path plus exact final
program bytes, carry a whole-plan digest,
and are withheld if redaction changes them. The pure codec lives in
`nika_compile_fidelity::sketch::kept` and is exposed through Onboard. It limits
the envelope to 256 KiB and sixteen records. Reopen restores evidence and the
last Save's relative file selection, never consent, a live proposal,
Run permission or past Check verdict. EDIT reconstructs the exact base again
and passes current observation and monetary admission. Equal program bytes at
different paths cannot overwrite each other's meaning; retaining a new proposal
does not alter the record for the last accepted Save.

The optional `Saved.inference_checkpoint` carries separate complete numeric accounting or
a completed unknown-cost report witness,
not program evidence or renewed permission. The same versioned checkpoint must
match the project record and its cost observation under the exclusive history
lease. No interrupted operation or dispatch marker may remain. The providers
owner restores it CLOSED or Uncertain, preserving the old total ceiling, settled
charges, active/held exposure and attempt identities. Another call requires fresh
explicit TOTAL Session admission through the canonical amendment. Older records
without this checkpoint, mismatched copies and unknown formats cannot infer a
new allowance. Old consent, proposals and Run permissions remain expired.

A `nika/completed-cost-report@1` witness retains only a project-bound digest of complete
CLOSED unknown-cost observations, never an admission account. Under the same exclusive,
concordant, uninterrupted boundary, it allows Session to offer another fresh one-time
cost review while retaining every old observation and monetary restriction. The exact
legacy numeric-codec refusal is readable only under that strict report validation.
Mixed observation families and incomplete scopes remain refused. Neither restoration
nor a fresh budget statement accepts the next unknown-cost invocation.
