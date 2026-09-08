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
The journal does not yet correlate an interrupted request with a particular
job through an idempotent submission key. It therefore does not recover a run
by guessing from the latest trace, nor promise automatic resume or exactly-once
external effects. `observe_run` remains a host-supplied observation.

## Format and limits

The private format is versioned and hash chained. Unknown versions/fields,
invalid transitions, wrong project binding, truncated records and damaged
hashes are refused. Corrupt bytes are preserved. Hash chaining detects
accidental damage; a writer able to forge the entire private file can also
recompute its hashes. It is not an authentication boundary.

The journal retains operation inputs, diagnostic outcomes and conversation
projections. Diagnostic outcome strings are not a machine protocol and are
never deserialized into executable commands. The projection sent to the model
keeps the existing eight-turn window; this is distinct from journal retention.
The broker's existing redaction patterns also protect persisted text. They
are not an exhaustive secret detector. No credential store, model availability
snapshot or hidden reasoning is serialized.

The local directory and files use `OwnedDir` and its private modes (0700/0600).
Contained opens refuse symlinks. The project tree receives no conversation
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

These checks establish local conversation continuity on the tested platform.
They do not qualify power-loss behavior, cloud replication, learned memory,
cross-session retrieval, or transport parity.
