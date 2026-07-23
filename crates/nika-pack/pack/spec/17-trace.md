# 17 · Trace

> Every run leaves a journal · one NDJSON file, one event per line, each
> line chained to the last by hash. The trace is the run's flight
> recorder (`nika trace show|replay|verify`), the resume substrate, and
> the stream the receipt folds FROM ([15](./15-proof.md)). This chapter
> makes the dialect NORMATIVE: `trace_format: 2`, the frame grammar, the
> chain law, the kind vocabulary, and the permit-decision witness
> (NEP-0007) · the trace stops being an engine-private dialect. What is
> written here is the OBSERVED wire, grave-not-invented: the conformance
> golden is a real run's journal, byte-verifiable.

---

## The journal (normative)

One run → at most one journal file · `<trace-dir>/<ISO-compact>-<short
-id>.ndjson` (production: `.nika/traces/`). Three laws shape it:

- **Lazy open** · file creation waits for the FIRST emitted frame (a run
  refused before its prologue · audit refusal · composition failure ·
  leaves no file).
- **Infallible rider** · journal I/O failure never changes the run's
  verdict or its primary output; the error surfaces AFTER the run as a
  note. The journal tees BESIDE the chosen primary lane, byte-identical
  with or without it.
- **Append-only** · frames are written in emission order and never
  rewritten; the chain (below) makes any rewrite evident.

## The frame (normative)

One event per line · one JSON object · the member set:

| Key | Type | Meaning |
|---|---|---|
| `chain` | string | sha256 hex of the PREVIOUS line's exact bytes (the chain law below) |
| `id` | object | `{ "uuid": "<UUIDv7>" }` · unique per frame · time-ordered |
| `timestamp` | integer | unix epoch **nanoseconds** |
| `kind` | string | the closed-per-minor kind vocabulary (below) |
| `fields` | array | ordered `{ "key": string, "value": any }` pairs · the frame's payload |
| `run` | string \| null | the run correlation id when one is set |
| `correlation` | string \| null | causal correlation across runs when one is set |

Field VALUES are JSON values (string · number · object-as-string when a
payload is itself serialized · the observed `permits_json` and `outcome`
carry serialized JSON as a string, so the frame grammar stays flat).
Sensitive payloads are hashed by the emitter, never carried raw.
**Additive law** · a reader MUST ignore an unknown member and an unknown
`fields` key (report « unrecorded », never guess); an unknown `kind` is
tolerated by verifiers (the walk continues · the chain still binds it).

## The chain (normative)

The first line's `chain` is the sha256 of the GENESIS tag; every later
line's `chain` is the sha256 hex of the previous line's EXACT bytes.
After the run, the last line's hash is the journal's HEAD · printed at
run end and carried by the seal. Verification recomputes the walk:
any byte edit, insertion, deletion, or reorder breaks every subsequent
link (`nika trace verify` · FORGED). The chain proves INTEGRITY of the
sequence; it does not prove authorship (the seal and anchor rails of
[15](./15-proof.md) carry authenticity).

## `trace_format: 2` (normative · the version)

The dialect version rides the FIRST frame (the prologue's
`trace_format` field). It versions the JOURNAL WIRE · orthogonal to
`nika: v1` (the language), `receipt_format: 1` (the folded receipt) and
`graph_format: 2` (the projection). Version 2 is THIS chapter. A wire
change that breaks a version-2 reader MUST bump it; additive fields and
additive kinds MUST NOT.

## The prologue (normative · `workflow_started`)

The first frame is `workflow_started`, and it is the run's MANIFEST.
Version-2 fields (the observed set):

| Field | Meaning |
|---|---|
| `workflow` | the workflow id |
| `permits` | the boundary posture, human-readable (`declared boundary · default-deny` \| the zero-authority note) |
| `permits_json` | the DECLARED `permits:` block, serialized verbatim · the boundary the run was judged under |
| `workflow_sha256` / `workflow_sha256_lf` | the source bytes' digest (raw · LF-normalized) |
| `semantic_hash` | the semantic identity ([15](./15-proof.md) · H(domain ‖ version ‖ JCS(IR))) |
| `sandbox` | the OS-confinement backend the run selected (`seatbelt` · `landlock` · `noop`, loudly) |
| `trace_format` | this dialect's version · `2` |
| `engine_version` | the emitting engine |
| `platform` | `os/arch` |

An engine MAY add fields (additive law); it MUST NOT drop these.

## The kind vocabulary (normative · closed per minor)

Version 2 names exactly these kinds · additive per minor, never
re-meant:

`workflow_started` · `workflow_completed` · `workflow_failed` ·
`workflow_cancelled` · `workflow_paused` · `task_scheduled` ·
`task_started` · `task_completed` · `task_failed` · `task_skipped` ·
`task_retrying` · `task_recovered` · `task_cancelled` ·
`task_cache_hit` · `verb_invoked` · `tool_invoked` ·
`checkpoint_written` · `cost_incurred` · `infer_chunk` ·
`permit_checked` · `declassify` · `run_sealed` ·
`agent_tools_selected` · `agent_nudge` · `agent_stalled` ·
`agent_compose_checked` · `agent_budget_checkpoint`

Per-task terminal frames carry the task's witness fields (the observed
`task_completed`: `task` · `note` · `duration_ms` · `def_hash` ·
`input_hash` · `output` · `outcome` · the outcome record serialized).

## The permit witness (normative · REQUIRED · NEP-0007)

Every permit decision taken at the dispatch boundary is a
`permit_checked` frame · granted AND refused alike: the exec program
gate, the tool grant, the taint re-gate ([10](./10-authority.md) ·
NEP-0004), the environment composition (NEP-0005 · the passed names),
and the data-as-code sink (NEP-0006). The per-operation fs/net
enforcement decisions live inside the builtin sinks (their refusals
surface as typed task failures; their allow-side witness is the
declared v1 residual · NEP-0007 law 2). The frame's fields name the `gate` (the
bound consulted), the `decision` (`allow` \| `deny`), the `why` (the
law applied), and the `task`. This is a CONFORMANCE requirement on the
ENGINE, not a wire bump: the frame grammar is unchanged (one more kind
in the stream), so `trace_format` stays 2. A journal from an OLDER
conformant engine simply lacks the frames: `nika trace verify` reports
a run that exercised effects with zero permit-decision frames as a
FINDING (the witness is absent · the journal is old or the engine is
not NEP-0007-conformant) · never FORGED (the chain still holds), never
a crash.

The witness is what makes invariant 4 · judged = executed = attested ·
CHECKABLE: the static judge's verdicts, the runtime's decisions, and
the journal's frames speak the same boundary, and the conformance
differential replays the same inputs through checker and engine and
FAILS on any verdict divergence.

## The fold law (pointer)

Every proof consumer reads THIS stream: the receipt folds from it
([15](./15-proof.md)), `--resume` replans from it, `nika trace
show|replay` renders it, the verify walk re-hashes it. One stream, no
side-channel.

## Conformance

A v1-conformant engine MUST ·

1. Write one chained NDJSON journal per run that emits at least one frame (lazy open · append-only · infallible rider)
2. Open with the version-2 prologue (the manifest fields above · `trace_format: 2`)
3. Chain every line to the previous line's exact bytes (genesis-tagged · the head reported at run end)
4. Emit a terminal workflow frame (`workflow_completed` \| `workflow_failed` \| `workflow_cancelled` \| `workflow_paused`) and a terminal task frame per settled task
5. Emit a `permit_checked` frame for every permit decision, granted and refused alike (NEP-0007 · the witness)
6. Tolerate unknown members, fields, and kinds when READING a journal (the additive law · report « unrecorded », never guess)

---

🦋 *Related · [15 · Proof](./15-proof.md) · [10 · Authority](./10-authority.md) · [07 · Conformance](./07-conformance.md)*
