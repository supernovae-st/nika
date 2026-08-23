# Changelog

All notable changes to **Nika** are documented in this file.
The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
Nika follows [real semver toward 1.0](ROADMAP.md) — incremental quality, diamond-grade at every release (amended D-2026-06-20-N1 · was "forever-v0.x").

Nika Diamond is a ground-up rewrite on the `nika-diamond` orphan branch.
Legacy `main` is frozen at v0.79.3. Diamond starts at v0.80.0.

---
## [Unreleased]

### Added

- **`nika check --json` carries one-obvious-way lints.** `hints[]`
  rows with `kind: "one-obvious-way"` and `code: "one-obvious-way/NNN"`
  (advice starts with the rule id), the same door native-first already
  had. Warnings, never errors — `clean` stays true.

- **The next tagged binary includes the harness access class.**
  `release.yml` builds `--features local-infer,access-harness`. The
  160 KB `nika-harness` crate has been on main, API-frozen, since
  2026-08-08, and was compiled out of every downloadable binary.
  `agent:` tasks can sit on a detected harness via `--access <seat>`
  once that tag ships. Infer-grade harness (P4 · `infer:` on a seat)
  stays parked. `crates/nika-acp` stays a quarantined workspace (the
  official SDK's `preserve_order` must never unify into the engine);
  Diamond CI now runs its five batteries against `nika-harness` over
  a process boundary. `metal` stays off (candle 0.10 kernel dies at
  first token).

### Changed

- **OpenAPI lists the live POST statuses.** `GET /v1/openapi.json` names
  400, 408, 409, 413, 415, 503, and 507 on `POST /v1/jobs` next to
  200/202/401/422. `info.description` names that `POST /v1/run` is
  absent. `GET /v1/jobs/{id}/status` stays `{status}` only; diagnosis
  lives on `GET /v1/jobs/{id}` and SSE.
- **Failed HTTP jobs name a NIKA code.** `GET /v1/jobs/{id}` grows an
  optional `{error:{code,message}}` on `failed`. SSE settled/refused
  frames may carry the same redacted pair. Paths and secret-shaped
  fields stay dropped. Succeeded jobs omit `error`.
- **POST `/v1/jobs` 422 names the capture diagnosis.** A parse-fatal or
  check-fatal world returns `{error:{code,message}}` with the NIKA code
  when the engine stamped one, including analysis codes (AUTH/SEC)
  that live on `finding.code`. Symlink and other capture refuses stay
  `admission_refused`. Paths stay dropped.
- **Token-file refusal is typed.** `ServerError::Credential` now names
  unreadable, not a regular file, insecure mode, or invalid material.
  `nika serve --bind` still prints the openssl mint and never echoes
  bytes. Missing `--token-file` is unchanged. Paths stay dropped.
- **`nika doctor` uses the same token-file policy as `nika serve`.** A
  short, non-graphic, or symlink `.nika/serve.token` is a fail with the
  openssl mint, not an owner-only green. Group/world-readable still
  names `chmod 600`. The row stays silent when the file is absent.

- **`nika serve --bind` prints the token mint instead of an opaque
  credential refusal.** Missing `--token-file`, a short secret, or a
  group/world-readable file all teach
  `umask 077 && openssl rand -hex 24 > .nika/serve.token && chmod 600
  .nika/serve.token` and never echo the bytes. The HTTP door still
  refuses to mint a secret on its own.
- **The listen line names the next hop.** After bind it prints
  `GET /health`, authenticated `GET /v1/openapi.json`, and
  `POST /v1/jobs` with Bearer plus Idempotency-Key. A non-loopback
  bind adds that the blast radius is every workflow in `--workflows`.
  `GET /health` JSON stays the ADR-117 identity allowlist.
- **Action dogfood no longer runs on tags.** `install.sh` resolves the
  latest published binary, which lags the tagged tree until release.yml
  finishes. The smoke workflow now lives outside the checkout so project
  discovery cannot walk into this engine's `nika.yaml`. An additive
  `working-directory` input on the composite action is the seam.

## [0.114.0](https://github.com/supernovae-st/nika/compare/v0.113.0..v0.114.0) - 2026-08-23

**Remote execution as a loopback door.** Default `nika serve` stays the
resident ARM firer. `--bind` + `--workflows` + `--token-file` opens
authenticated HTTP: jobs, SSE, OpenAPI. The worker runs the POST-time
snapshot, not live files. Cancel, artifacts and `/v1/run` stay 404.
The workflow envelope is unchanged since v0.113.0. The project file's
retired `nika: v1` tag now refuses. Pre-1.0 real semver puts this at a
minor bump.

### Added

- **`nika serve --bind` is authenticated loopback HTTP.** Pair it with
  `--workflows` and `--token-file` (credential bytes never enter argv).
  Default `nika serve` is still the resident ARM firer. The verb is on
  `nika --help`. Listen line prints the bound address, including port 0.
  Job cancel and artifacts stay 404 until those authorities exist.
- **Job events project over SSE** at `GET /v1/jobs/{id}/events`. Frames
  are `{sequence,kind,status}` only. `GET /v1/openapi.json` is the live
  route table; cancel, artifacts and `/v1/run` are omitted.
- **`nika doctor` names the HTTP door when a token file is present.**
  Owner-only mode is OK; group/world-readable is a fail with the umask
  077 fix. The row never claims TLS — that is the reverse proxy.
  Silent when the cwd has no `.nika/serve.token` (the door is opt-in).
  systemd and Caddy examples live in `docs/ops/`.

### Changed

- **`nika check` names the ancestor `nika.yaml` spend cap.** `nika run`
  already filled `--max-cost-usd` from that file; check printed « no
  total ceiling » and advised adding the flag, on a tree that already
  had one (issue 1050). The COST line still prices the workflow; a
  `BUDGET` footnote (and a presence-gated `run_budget` object on
  `--json`) now says the number and `nika.yaml:line`. `--max-cost-usd`
  still wins. `run --dry-run` and `--help` stay silent — this is the
  surface that contradicted the file.
- **A piped `nika try` / `nika run` no longer waits on the macOS keychain
  after a green card.** The card printed, then `SecKeychainFindGenericPassword`
  blocked the main thread: a pipe cannot complete an ACL prompt. The
  keychain is skipped when stderr is not a TTY (file/env custody stay);
  a disabled journal (`nika try` · `--no-trace-file`) does not consult it
  at all.
- **The README says the trace inherits what the run read.** A `nika:read`
  of a file puts those bytes into `.nika/traces/` in the clear — correct
  for a replayable journal, unnamed until an auditor derived it (issue
  1047). The receipt paragraph now says hash-chained is not confidential.
- **`nika spec --canon` says the error-code count is the floor.** An agent
  read `count: 103` plus two `NIKA-BUILTIN-*` rows as the builtin-code
  family. The spec now carries a `scope:` sibling (nika-spec 282); this pin
  vendors it. Per-builtin and per-provider codes still live in `nika explain`.
- **BREAKING — the project file names itself (`nika.yaml`).** `nika:` carried a
  frozen `v1` tag while a workflow's `nika:` names the file: one key, two
  grammars, and every surface reading both had to know a special rule. It now
  carries the project's kebab-case **name**, the same grammar the workflow
  envelope gives its own `nika:`. The reasoning is this repo's own, written
  when the workflow envelope was nuked — a field with one legal value is not a
  version, so nothing is traded away, and the project gains a name it never
  had. The retired tag refuses **here and nowhere else**: a pre-nuke workflow
  carried a `workflow:` block beside its `nika: v1` and still refuses on that
  key, so `v1` stays free to be an ordinary workflow name; a pre-nuke project
  file had no companion key, so without a refusal the same bytes would quietly
  stop meaning « schema v1 » and start meaning « a project named v1 ». Only a
  whole marker qualifies — `vault`, `v2ray` and `v1-migration` stay names.
  Migration is one line: `nika: v1` → `nika: <your-project-name>`.

### Fixed

- **HTTP cancel now reaches the blocking worker.** Timeout and SIGTERM
  no longer mark interrupted while effects continue. Queued jobs persist
  their workflow name and are re-enqueued on the next incarnation.
- **POST captures the execution world.** A symlink or rewrite after 202
  cannot run bytes the client never admitted. GET names `execution_id`
  and `trace_id` after readmit. The service sandbox is the workflow root.

### Added

- **`nika check` names the exact builtin when a utility is 1:1.** `sleep`,
  `date`, `uuidgen`, the digest family, `yq`, `grep`/`rg`/`ag` and `find`
  used to be silent — or, for `date`/`sha256sum`, a family catalogue.
  Rule `native-first/006` answers with one builtin and its argument shape
  (`nika:wait` · `duration:`) so the author types the next line, not a
  menu. It sits at the foot of the ladder and never steals a more specific
  family. `echo` stays silent on purpose: it is the universal placeholder,
  not a builtin.
- **Tool-result spill in the `agent:` loop (opt-in seam).** Past 16 KiB a
  tool result's full text leaves the conversation for the blob store —
  the content hash IS the locator — and the model keeps a 2 KiB preview
  plus the pointer, so the context window stops re-paying bytes it cannot
  use. Nothing is discarded, and a store refusal keeps the full text:
  the spill is an optimization of the model's reading, never a gate on
  the data. Seat it with `AgentVerb::with_spill`; without the seam the
  loop is byte-unchanged.
- **`nika check` judges a project file as a project.** It applied the nine-key
  WORKFLOW envelope to every document, so a project file came back
  `NIKA-PARSE-002 missing required envelope field: tasks` — on a file
  `nika init --project-file` had just written — and following that finding's
  own advice converted a correct project file into a broken workflow. The
  discriminant is the spec's, normative and covering every document
  (`01-envelope` §The type discriminant): a `tasks:` key means WORKFLOW, its
  absence means PROJECT. Deliberately not the filename, so it still holds for
  a registry blob, an HTTP body or a fence pasted into a chat. A project now
  audits in its own vocabulary — exit 0 clean, 2 on a finding — and says what
  it governs rather than a count it never stated.
- **`nika check` notes a file whose name and filename have drifted apart.**
  Copy `foo.nika.yaml` to `bar.nika.yaml`, forget the header, and every trace
  and journal event keeps saying `foo`. It is a NOTE and never a finding:
  divergence is usually deliberate (an ordering prefix such as `01-hello` is
  stripped before comparing), and the exit code is untouched. The filename is
  a location `git mv` may change; the name is an identity that rides traces.

### Fixed

- **`nika check`'s audited line names the declared blast radius.** The
  default card said `permits declared`; the grants themselves lived
  behind `--infer-permits` and `--json` (persona 4). Cost was already
  on the card. The cell now lists the exec / tools / fs / net / env
  grants (an explicit empty block is `{}`, absent is still `none`).
- **A recovered run is no longer a green tick at a glance.** `--quiet`
  and the shareable card titled `✔` / `✓` on a run that repaired a
  task (`task_recovered` then Ok). Exit 0 is still correct — recovered
  is a success cause. The headline and the card title now carry the
  warn mark; the storyboard already named `N recovered` (persona 14).
  `trace ls` still says `completed` (no new `TraceState` this cut).
- **`--infer-permits` no longer pastes a host-file grant.** A
  `nika:read` of `/etc/passwd` (or `~/.ssh/…`) under an absent
  `permits:` block printed `fs.read: ["/etc/passwd"]`; applying that
  block greened check and run (persona 7). G-09 already withheld the
  shovel on a *declared* boundary; the AUTH-006 companion and the
  inferred YAML still handed it. Escaping paths (absolute · home ·
  `..`-climb) stay a note. The printed repair is the tool conjunct
  only.
- **`nika check` no longer panics on a decorative verb glyph.** Copying
  `⛨permits:` / `◇infer:` from nika.sh into a file, then running the
  advertised `nika check`, dumped `annotate-snippets` (`byte index N is
  not a char boundary`) with no NIKA code and no `--fix` hint. The
  snippet painter widened a point span by one BYTE into a 3-byte glyph;
  it now snaps to a char boundary and widens by one CHAR. MCP already
  taught `did you mean permits?` — the CLI default path matches that
  calm.
- **`nika try 10-compose-pipeline` stages the child it invokes.** The
  rehearsal room used to hold only the parent; check then died
  `NIKA-COMP-001` on `./10-compose-child.nika.yaml` (e2e S2 17/18 on
  81c1138f). Fixture materialize already carried `examples/fixtures/`
  ingredients; a relative `workflow: "./….nika.yaml"` pack sibling is
  the same class.
- **`for_each` over a constant that is not an array is refused at check, not
  at dispatch.** `const: { items: "x" }` with `for_each: ${{ const.items }}`
  audited clean, then died at the run with `NIKA-VAR-006` — a linter's answer,
  not a verifier's, and exactly the gap ADR-092 exists to close. The static
  lane already caught a *typed* non-array var but exempted every untyped one,
  on the rationale that « a `--var` override could pass an array ». That rule
  is inputs-only: `--var` sets an `inputs:` value and refuses unknown keys, and
  spec 01 §const is normative — a constant is « immutable across the run and
  never caller-supplied ». An untyped constant's literal IS its run value, so
  a non-array can never become one. Untyped entries are legal in `const:`
  alone, arrays and typed declarations are untouched, and all 59 shipped
  templates and examples still audit clean.
- **`nika explain` answers the token `nika check` printed in `[brackets]`,
  including a hint.** A HINT row put `jq-as-map` (or `native-first/006`)
  in the same slot as `NIKA-PARSE-019`, so the next gesture was
  `nika explain jq-as-map` and the answer was `unknown code`. A finding
  carries `code`; a hint carries `kind` — real in the data, invisible
  in the render. Explain now resolves the printed identity (the kind,
  or the numbered native-first rule) and teaches the class; MCP
  `nika_explain` speaks the same text.
- **`nika check` names the line of a PARSE refusal.** A CONFORM finding
  already carried a rustc-grade frame (`path:line:col` + caret). A PARSE
  finding — `NIKA-PARSE-017` duplicate key, `NIKA-PARSE-005` unknown
  field, any span the parser held — printed the code and left the author
  to find the site. Duplicate keys are the worst of that class: the
  message says `"a" appears twice`, so grepping `a:` returns both, and
  neither is wrong on its own. The colliding key's span was in the
  loader's hand and discarded (`span: None`). It now rides the same
  frame CONFORM uses, under the same `PARSE ✗` first line.
- **`nika arm` no longer refuses a project that sets its retention or its
  provenance floor.** Two readers parse `nika.yaml` — the project reader
  (`ceiling` · `traces.keep` · `registry.floor` · `arm`) and the cadence
  grammar. The grammar refused `traces:` and `registry:` by name as
  « round 2 » keys with a remedy that was false (« retention stays with
  the env vars ») while the project reader had just accepted them and the
  retention ladder and the provenance gate consume them — the project
  starter's own `traces:` line made `nika arm` exit 2. Measured
  2026-08-18. The grammar now admits the project's other rungs OPAQUE and
  judges nothing about them (they are judged where they are owned); the
  cadence domain's own deferred keys (`signature:` · `budget:`) stay
  refused by name. A test derived from the project reader's closed key
  set pins the parity: every key it admits, `nika arm` reads green.

## [0.113.0](https://github.com/supernovae-st/nika/compare/v0.112.0..v0.113.0) - 2026-08-21

**The couture release.** The judge already knew; the agent can now see.
Error codes and `nika explain` ride MCP, CLI JSON and wasm as one well.
Pipelines are judged per segment. Four missing authoring forms have
skeletons. Zero breaking changes since v0.112.0, so pre-1.0 real semver
puts this at a minor bump.

### Added

- **`nika check` now judges every native shell segment, not only the head.**
  Pipelines, `env` wrappers, redirections, groups, `if`/`while` prefixes and
  newlines each surface their own `native-first` hint. Comments stay comments.
- **Four form-first skeletons join `nika new`.** `classify-and-route`,
  `corpus-qa`, `document-to-fields` and `evaluate-and-optimize` ship with their
  golden and their refusing neighbour, pinned to spec `d40fe6ac`.
- **The MCP oracle now carries `instructions` and a structured example index.**
  A dirty report keeps its `NIKA-*` code on the agent channel; `nika_examples`
  without a slug returns recoverable rows instead of a bare list.
- **A green MCP check carries the contract, not just the word clean.**
- **The tightest permits boundary is offered, and only when it is knowable.**
- **The arm ledger is the firing truth; the workspace admits the arm custody
  library.**

### Changed

- **The engine carries one compile-time identity for its version, build,
  language pin and remote API axis.** `nika-runtime` now owns the typed
  `EngineIdentity` consumed by trace prologues, `nika check --json` and the CLI
  version surface. The embedded language pack is re-vendored from that exact
  `SPEC_PIN`, records the same commit in `pack/SPEC_SHA`, and the build refuses
  split identity.

### Fixed

- **A `NIKA-PARSE-*` finding is a PARSE row, including when the analyzer
  emitted it.** Missing `nika:` / `tasks:` no longer wear the CONFORM
  ladder on `nika check`, `--json` `findings[]`, wasm, or MCP — the spec
  family is the gate the agent explains from.
- **Wasm parse-fatal rows stay PARSE regardless of code**, matching CLI
  `parse_fatal_json` (a `NIKA-DAG-005` unknown `after` predicate is PARSE
  on both assemblies).
- **`--fix` cannot rewrite a digest-pinned registry cache, a symlink into
  that cache, stdin, or a device/FIFO.** The footer teaches a copy into the
  workspace; a project path that merely looks like `.nika/registry` stays a
  normal file.
- **The pinned conformance clock includes Agent Skills and current trace
  witnesses.** The harness now exercises the spec's skill lane with exact
  `AGENT`, `AUTH` and `SEC` refusal codes, recognizes the entropy/jitter law,
  and replays every current runtime-trace verdict. The heal workflow advances
  the pin and pack together on an immutable PR branch; Diamond CI independently
  re-vendors every mapped byte.
- **Reference dry-run, AUTH advice, repair and mock seats tell the truth.**
- **The clone says when its own enforcement is not armed.**
- **The plan projection API is locked.**

## [0.112.0](https://github.com/supernovae-st/nika/compare/v0.111.0..v0.112.0) - 2026-08-20

**The instrument-honesty release.** Three features and sixteen fixes, several
of which began as a measurement disagreeing with what a surface said. A cargo
test binary can no longer open the OS keychain — an ACL is bound to the
requesting binary, and a test binary's hash changes on every compile, so the
prompt could never be answered once and for all. `nika check`'s JOURNEY rung
counts model endpoints instead of tasks, ending a card that contradicted
itself four lines apart: COST read `no infer/agent tasks` while JOURNEY read
`3 model endpoints`. A fan-out that recovered now says so in its record and
not only in its prose, which is what spec 13 requires of the pair. And the
`exec:` fit lane gained an fs arm, so a leg jailed away from its own script
is refused at check instead of exiting 126 under a green card.

### Added

- **`nika list` names every workflow below the current directory.** Output is
  stable and root-relative, nested workflows are included, project metadata
  and hidden/build directories stay out, and an incomplete walk refuses
  instead of presenting a partial inventory as exhaustive.
- **Bare `nika` on a terminal opens one continuous thread.** Model turns
  stream through the existing `agent:` runtime; `/workflow` posts a workflow
  into the thread, `/run` executes it there, and Ctrl-C interrupts the active
  turn without closing the outer conversation.

### Removed

- **BREAKING — `VirtualClock` loses its dead time-mover (`nika-clock`).**
  `VirtualClock::advance` and `VirtualClock::elapsed_total` are removed:
  `advance` documented itself as « the ONLY mover of virtual time » while
  having zero production callers — virtual time never moved, so under
  `run: { clock: virtual }` (or `entropy: none | seeded(N)`, which imply
  it) a task `timeout:` budget raced an instantly-ready timer and every
  deadline was already settled at dispatch. The clock is now honestly
  FROZEN by construction (`Copy` bases, no shared offset): an author who
  needs a real deadline honored against real work must not declare
  `clock: virtual`. Wiring the task `timeout:` budget to the exec
  runner's own deadline (`linger: false`) is a follow-up wave.
- **The exec runner's process-group kill is removed
  (`nika-exec-runner`).** `terminate_group` (SIGTERM→SIGKILL the whole
  process group) plus the `process_group(0)` spawn setup were correct
  and tested but unreachable in production — they fired only on
  `TimedOut`/`Cancelled`, and the engine never assigns
  `ShellCommand.timeout` nor invokes `cancel`. Cancellation stays
  `kill_on_drop` (INV-011 · future-drop, the ADR-016 primary); detached
  grandchildren are no longer group-killed on the embedder-facing
  timeout/cancel arms, and the `nix` dependency goes with them.

### Changed

- **BREAKING — the event error codes leave the Shield reservation
  (`nika-event`).** `NIKA_420/421/422` (serialize failed · buffer full ·
  lock poisoned) were minted inside the locked Shield band (380-429), so
  a full event buffer surfaced as « Shield security policy blocked the
  operation » — a refusal its reader would read as security. They are
  renumbered into their own Observability band (800-819) as
  `NIKA_801/802/803`, constants renamed with them.

### Fixed

- **A cargo test binary no longer opens the OS keychain.** A macOS keychain
  ACL is bound to the requesting BINARY, and a test binary is
  `target/<profile>/deps/<name>-<hash>` whose hash changes on every
  recompile — so "Always Allow" grants a binary that will never exist
  again and the prompt returns forever, on every worktree, with no
  operator-side gesture that stops it. `NIKA_KEYCHAIN=off` now skips the
  custody, and a test binary skips it by default whether or not anyone
  remembers the flag. An installed `nika` and `cargo run` land outside
  `deps/` and keep their custody unchanged. All eight keyring call sites
  sit behind the flag, held there by a ratchet that walks the crate's own
  source.

- **The JOURNEY rung counts model endpoints, not tasks.** The envelope
  `model:` is a fallback for a task that HAS a model · it was applied to
  every task, so a body of builtin invokes read `3 model endpoints` while
  the COST rung four lines above read `no infer/agent tasks` — one card
  contradicting itself. `model_endpoint_of` already typed the task and
  threw the answer away on the next line. Across the shipped corpus 45 of
  99 cards carried an inflated count; each is now exactly the number of
  `infer:`/`agent:` tasks the file declares, and no verdict changed.

- **`nika check` judges every secret sharing one effect independently.** A
  sink that referenced two secrets previously retained only the first IFC
  trace, so clearing that first edge could hide an uncleared second edge.
  Direct references and task-local `with:` / `for_each` item aliases now
  produce one consent verdict per distinct secret while the existing
  singular output-propagation trace stays unchanged and bounded. Literal
  `for_each.items` secret references also appear in the data journey.

- **`nika check` now judges the script an `exec:` interpreter must open.**
  The runtime jails every `exec:` child to the declared `permits.fs` set, so
  `exec: ["bash", "leg.sh"]` with no `fs.read` grant could never open its
  own script — measured on seatbelt, the leg exits **126** with empty
  stdout. The audit was ✔ on all fourteen lanes and the run rendered it
  `✔ leg` with rc 0: a leg that did nothing, reported as a success. The
  `exec:` fit lane gained an fs arm beside its net arm (which shipped
  2026-07-29 for the same sentence one boundary over), so the escape is a
  `NIKA-SEC-004` finding at check with the one-line repair. The claim is
  narrow on purpose — only a literal argv whose program is an interpreter,
  on its script positional, through a literal `cwd:`; everything else stays
  the runtime's verdict, and the verdict models the jail (which binds a
  grant's literal prefix and never globs) rather than the stricter lexical
  walk. `--infer-permits` learned the same fact in the same change, so the
  boundary it writes cannot self-refuse the workflow it came from. Swept
  over the shipped corpus: 63 of 63 unchanged.
- **The TRIFECTA tick is derived, so it cannot outlive the gate that
  bought it.** Measured on 0.111.0, one card printed `✔ TRIFECTA … without
  a human gate` four lines above two `NIKA-SEC-014` rows proving that same
  gate lets the effect fire on 'no'; a control run with the prompt deleted
  raised `NIKA-SEC-009`, so the trifecta was complete and the tick was
  bought entirely by a rubber stamp. The lane credited a *blocking prompt*
  (one task, one key lookup) while the consent lane ran the full
  refusal-substitution walk — and the clearance discarded WHICH gate it
  credited, so a trifecta cleared by a gate and one cleared by a missing
  leg were the same empty vec. The clearance now publishes its credit and
  the rung withholds the tick where another lane refutes it, pointing at
  the code that owns the repair. No second finding: the consent row
  already names the defect and teaches the fix.

- **The `nika-error` crate-spec band table matches the one-voice
  registry.** It still read `330-379 Binding/template · 380-429 Provider`
  while the registry moved Provider to 330-379 (2026-05-11) and reserves
  380-429 for Shield. The stale rows invited exactly the collision the
  reservation exists to prevent.

## [0.111.0](https://github.com/supernovae-st/nika/compare/v0.110.0..v0.111.0) - 2026-08-19

**The authoring-loop release.** `nika check --json` now reports
`paid_ready`, `compiled` and `next` — a green parse is legal, not
best. `nika:inspect` is live: the runtime seeds the DAG at run
start and a workflow can read its own cost, records, dag_info and
threads. `nika:compose` stays loop-only (grant after `nika:done`;
checking never executes). The arm lock outlives the shot.

### Added

- **`paid_ready` · `compiled` · `next` on `nika check --json`
  (#1013).** `paid_ready` is silent only when no paid-run hint
  remains. `compiled` means the law is proven (const-fixture
  assert). `next` is the first repair. `nika explain` prints a
  **before a paid model** panel. MCP `nika_check` hard-fails
  `infer-as-law` and `digit-string-enum` only.

- **`nika:inspect` is live (#1018).** `LiveInspect` is the same
  `Arc` the dispatcher and the runtime share. The DAG is seeded at
  run start — the first task sees `available: true`. Records and
  spend mirror after each wave. Hint `inspect-unwired` is retired.
  Teaching shape: `16-inspect-self`.

- **Lesson 15 `nika:compose` on an agent whitelist (#1016).**
  Grant after `nika:done`. The model drafts YAML, gets the full
  check JSON, iterates until `valid`. A standalone `invoke:` is
  `NIKA-BUILTIN-COMPOSE-001`. Checking never executes the draft.
  Parent→child composition stays lesson 10.

- **The arm lock outlives the shot (#1015).** A fire that is
  still running keeps the project lock so a second tick cannot
  overlap the first.

### Fixed

- **Authoring seams from a paid extract wave (#1012).**
  `nika:hash` serializes structured `content:`. `nika:validate`
  parses string schemas. Scalar `anyOf` flattens into coerce.
  `for_each` + `item.field` is resume-eligible (collection is the
  input identity). Hints: `digit-string-enum` · `glob-readme` ·
  `jq-as-map` · `assert-quarantine`.

## [0.110.0](https://github.com/supernovae-st/nika/compare/v0.109.2..v0.110.0) - 2026-08-19

**The arming release.** The project file `nika.yaml` learns to PROPOSE a
schedule, and the machine learns to keep it — « le fichier propose, la
machine dispose ». The `arm:` registry carries the team's armed beats
(thirteen keys, two of them required with no default — `plafond:` and
`manqué:` — because choosing for you would be choosing who pays), one
firer computes the due window and spends under the per-tick ceiling, the
OS bridge hands launchd/systemd the calendar, and `nika serve` keeps the
same firer resident behind the wall clock. One firer, four doors (D2):
`nika arm fire`, the emitted OS units, and `serve` all end at the SAME
fire — a beat the lock, the ceiling, or the miss policy governs is
governed identically no matter who pulled the trigger.

### Added

- **The project file carries the thirteen beat keys (`arm:` · W1 ·
  #993).** Each beat spells `workflow` (a repo-relative `*.nika.yaml`
  path) · `cadence` (a 5-field cron with the zone INSIDE the expression
  — `TZ=Europe/Paris 0 9 * * 1` — or `on-webhook`, resolved against the
  embedded tzdb, never the host's) · `où` · `plafond` · `manqué` ·
  `chevauchement` · `après_saut` · `actif` · `raison` · `jusqu_au` ·
  `tolérance` · `décalage` · `par`. The grammar is CLOSED
  (`deny_unknown_fields`): an unknown key fails at parse and the refusal
  names what is known. `plafond:` (the per-tick USD ceiling) and
  `manqué:` (what "missed" means: `rattraper` · `rattraper-une-fois` ·
  `sauter`) are OBLIGATORY with no default — a default `rattraper`
  spends what nobody asked for, a default `sauter` loses a deliverable
  in silence. A suspended beat must tell its story: `actif: false`
  without `raison:` is a suspension nobody narrates, without
  `jusqu_au:` an oblivion — both refuse by name. Bare `nika arm` READS
  the registry and reports what is armed, due, suspended, or refused —
  it fires nothing.

- **`nika arm fire <label>` — the one firer (W2 · #1001).** Computes
  the planner's silence over `(last, now]`, the on-time window, and the
  miss policy; takes the project lock; refuses BEFORE spending when the
  tick would cross the beat's `plafond:`; and writes the firing record
  to the ledger. Prints exactly one stdout line, always (D8) — a skip
  is an event, said as such (`skipped <label> · inactive — <raison>`),
  never a silence.

- **`nika arm --emit launchd|systemd` — the OS bridge (W3 · #1005).**
  Renders the unit files PURE from the registry (`--write` installs
  them); per-beat units are labelled `nika.arm.<radical>` (D4 — the
  labels the firer uses, computed once); the env file rides by PATH
  only (D7 — provider keys never enter a unit). launchd's
  `StartCalendarInterval` is the cartesian product of the restricted
  fields, counted before built — past 500 dicts the emit refuses (a
  plist of n dicts is a load, not a calendar); systemd writes the sets
  inline and the zone travels in `OnCalendar=`; `Persistent=` answers
  `manqué ≠ sauter`. Every refusal teaches: D10 `TzMismatch` (launchd
  fires in the machine's zone — the remedy names `TZ=` or systemd) ·
  `Webhook` (no calendar can fire an event) · `TooManyIntervals` · the
  D6 v0 set below.

- **`nika arm disarm <label>` — the N4 gesture (W3 · #1005).** Removing
  the line does NOT disarm — the file would simply stop proposing while
  the OS keeps firing. Disarming is `actif: false` + `raison:` +
  `jusqu_au:` in the registry; the verb teaches exactly that, and
  `--tear-down` takes the emitted OS unit down with it.

- **`nika serve` — the same firer, resident (W5 · #1008).** The SAME
  `fire` as `arm fire` and the OS units (D2), driven by the wall clock
  in place of launchd/systemd. The loop reads ONLY the registry —
  judged by vocab + cadence BEFORE any shot — and its own `.nika/arm/`
  sidecar; it reloads `nika.yaml` when its mtime moves (a broken edit
  is told, and the last-good registry keeps serving); it fires what
  `due` returns through the one firer (the per-tick ceiling always
  passed · every fire is a fresh run); and it sleeps to the earliest
  next slot (≤ 60 s) racing ctrl-c/SIGTERM — a signal breaks clean
  (exit 0 · the current fire, synchronous, finishes first · the lock is
  released). No network input, no environment read, no argument beyond
  the clap surface.

### What v0 refuses — and when each arrives (D6)

A policy the firer cannot honor REFUSES, never approximates — and the
refusal names the version the support arrives with:

| Written in the file | v0's answer | Arrives with |
|---|---|---|
| `chevauchement: remplacer` | refuses — today: `sauter` (the law-⑥ default) or `file` | serve v0.2 |
| `après_saut: à-complétion` | refuses — today: `prochain-créneau` (the default) | serve v0.2 |
| `manqué: rattraper` | refuses — today: `rattraper-une-fois` or `sauter` | serve v0.2 |
| `décalage:` | refuses — today the slot fires at the instant said | serve v0.2 |
| `où: cloud` | the local firer SKIPS the beat, journaled — « le cloud exécute, le calendrier demeure au registre » | the cloud rung |
| `signature:` · `budget:` | refused at validation, by name — round-2 keys: round 1 claims nothing it cannot prove (`traces:`/`registry:` likewise) | round 2 |

### Fixed

- **The ORDER and LIFT findings render in the human lane
  (`nika-display` · #1002).**
- **LOT 3 task-body rungs + the sweep's third lock (#999).**
- **Leftover fourteen-key teaching in `nika explain` and rustdoc
  (#1007).** `NIKA-PARSE-002` taught `nika: v1` + `workflow:` + a
  `tasks:` list; `NIKA-PARSE-005` parked custom metadata in
  `description:`. The `RawWorkflow` fence, the analyzer crate doc,
  `MissingEnvelopeField`, and the LSP outline still named `workflow:`
  as a required envelope key. They now speak the live nine-key
  envelope (identity on `nika: <kebab-id>` · `tasks:` is a map ·
  prose is a `#` comment above `nika:`).

### Documentation

- **In-repo teaching speaks the nine-key envelope of 0.109** — the
  hello-example comments name the nine-key identity (#995) · the README
  gifs are rebaked for the envelope (#996) · the docs sweep of 0.109
  lands in-repo (#992).

### Build & CI

- **The release funnel runs on every push (#985)** — the gate the tag
  trusts has already run on the commit it tags.
- **apt gets a deadline (#1000)** — a mirror stall is a five-minute
  red, not a six-hour hang.
- **The pack follows the spec (#987).**

## [0.109.2](https://github.com/supernovae-st/nika/compare/v0.109.1..v0.109.2) - 2026-08-19

**The second refusal, one layer down.** `v0.109.1` fixed the fossil envelope
in the release gates and died on both Linux builders one leg later:
`[consent-run] exit=3 want=4` — since #889 (0.109.0) a workflow that
declares `permits:` refuses to START on a host with no sandbox backend
(`NIKA-1710`), and a GitHub Linux runner has no bubblewrap. The macOS
builders, where seatbelt exists, passed both gates confined. No asset
shipped under `v0.109.1` either; the binaries were fine both times. This
patch is the same tree plus the second fix and is the version consumers
install.

### Fixed

- **The Linux release builders install bubblewrap before the gates run
  (release.yml).** The Diamond CI tests-leg recipe (apt bubblewrap · detach
  ubuntu-24.04's AppArmor bwrap profile · keep unprivileged userns open) now
  runs on the two Linux builders, so the funnel e2e and the trust battery
  run CONFINED there exactly as they do on macOS — never a waiver. A gate
  that spends an `exec` under `permits:` proves the jail as a side effect;
  a host that cannot jail says so (`NIKA-1710`) instead of being waved
  through.

## [0.109.1](https://github.com/supernovae-st/nika/compare/v0.109.0..v0.109.1) - 2026-08-19

**The release the release gate refused.** `v0.109.0` was tagged from
`f58a17396` and its own pre-upload gates killed all four builders: the
funnel e2e and the trust battery still authored their fixtures in the
fourteen-key envelope (`nika: v1` + `workflow:`) the nine-key engine
refuses at parse (`[guard-dirty] missing: NIKA-SEC-014` · `[consent-run]
exit=2 want=4`). No asset was published under that tag; the binaries were
fine and the gate was the fossil, exactly as v0.106.0 died on 2026-07-27
when the battery spent an exec without a `permits:` block. `v0.109.0`
stays a tag with no release; this patch ships the same tree plus the gate
fix and is the version consumers install.

### Fixed

- **The release gates speak the live envelope (funnel e2e · trust
  battery).** Four fixtures move from `nika: v1` + `workflow: {id}` to
  `nika: <id>`. Nothing else in the gates changes: the consent-dirty leg
  still expects `NIKA-SEC-014` at `guard` and `check`, the consent-pause
  leg still expects exit 4 with the resume line, the trust battery still
  runs its exec under `permits.exec`.
- **Hygiene vector 50 · `check-release-gate-envelope`.** The two gates run
  only at tag time, so a language change on main could leave them teaching
  the previous envelope for weeks with nothing red on any push. The vector
  greps both scripts for the dead envelope forms (`nika: v1` · `workflow:`
  · `on_finally:` · `depends_on:` · `${{ vars.` · `${{ env.`) on every
  push and is RED on any hit — proven by mutation (one restored fixture →
  red).

## [0.109.0](https://github.com/supernovae-st/nika/compare/v0.108.0..v0.109.0) - 2026-08-18

**The nine-key release.** The envelope shrinks from fourteen keys to nine and
a workflow written for 0.108.0 will not check on 0.109.0 · this is the flag-day
the 0.106.0 front page announced as possible, and it lands whole. The identity
moves onto `nika:` itself (`nika: <id>` · a kebab-case name · the `workflow:`
block, its `id:` and its `description:` are gone), the value authorities are
exactly three (`inputs` · `const` · `secrets` · `config:` died with the block),
`types:` · `policy:` · `assert:` leave the envelope, and the task body loses
its second grammars: cleanup is a real task on an `unwind` edge (`on_finally:`
is dead · `graph_format: 3` carries the `finally` node), `output:` is spelled
`extract:`, `declassify:` and `inert:` merge into one door (`lift:`),
`fail_workflow` is gone (`on_error` is `recover` or `skip`), the two fan-out
knobs live INSIDE `for_each:`, and `group:` arrives (fan-in · `NIKA-DAG-008`).
Two P0 close at the surface users install: an expression sees only its INPUT
(the ambient `env` leaves the jaq function set at the three seams, with a
pinned inventory of natives that reddens if a future jaq adds one), and a
third-party receipt can no longer write the operator's clipboard (every field
rides `escape_tty`). The refusal a 0.108.0 file meets first now TEACHES where
each retired key's role went, instead of `unknown field`.

### ⚠️ Migration

#### 1 · The envelope · fourteen keys become nine (LOT 2 · ADR-113 · #909 and the sweep of 2026-08-11/13)

The live envelope is `nika` · `model` · `inputs` · `const` · `secrets` ·
`permits` · `run` · `tasks` · `outputs`. Every other top-level key refuses,
and the refusal names the destination:

| Dead form | Write instead | The teaching |
|---|---|---|
| `nika: v1` + `workflow: { id, description }` | `nika: <id>` (kebab-case) · the description as a `#` comment above it | the identity IS the envelope key · prose is demoted, never dropped |
| top-level `description:` | a `#` comment above `nika:` | shipped twice (bare · inside `workflow:`) · both dead |
| `config:` | an `inputs:` entry with `required: false` and a `default:` | a deployment-supplied value is an input with a default · authorities are exactly three |
| `types:` (`NIKA-TYPE-002` retired) | the verb's `schema:` (structured output) · a task's `returns:` | a shape rides its consumer · the ten primitives stay lowercase (spec 09) |
| `policy:` | `permits:` (`fs` · `net` · `exec` · `tools`) · `secrets:` · the unconditional laws (spec 10 · `NIKA-SEC-015` net-before-exec) | a vocabulary is not a policy · what survived is the boundary |
| `assert:` | nothing in the file · `nika trace verify` (spec 15) | obligations are proven on the sealed trace |

#### 2 · The task body · one grammar per thing

| Dead form | Write instead |
|---|---|
| `on_finally:` mini-tasks | a task of its own · `after: { <parent>: unwind }` (a `finally` node · `graph_format: 3`) · every graph judge governs it because it walks `wf.tasks` |
| `output:` | `extract:` (same shape) |
| `declassify:` list · `inert:` string | `lift:` (the law is a parameter of one door · spec 10 §the authored doors) |
| `on_error: { fail_workflow: true }` | nothing · the default IS the failure · `on_error` is `recover` or `skip` |
| task-level `max_parallel:` · `fail_fast:` | inside the block · `for_each: { items: …, max_parallel: N, fail_fast: false }` |
| `depends_on:` | `with:` bindings (the binding IS the edge) · `after: { x: success }` for control (`NIKA-PARSE-024` · unchanged since W2) |
| `graph_format: 2` pins (`*.graph.json` goldens) | regenerate · `nika inspect --format json` · never edit a projection by hand |

#### 3 · What `nika check --fix` migrates · and what it does not

The rungs are idempotent and equivalence-or-stop · **r1-identity** (NEW ·
`nika: v1` + `workflow: {id, description}` · the block, the pre-W1 scalar
`workflow: <id>`, the one-line flow form, a bare top-level `description:` ·
become `nika: <id>` with the prose demoted to a `#` comment ABOVE it, never
dropped · it STOPS, never guesses, when `nika:` already names something else,
when the block carries a foreign key, when the id is not kebab-case, or when
there is no id at all) · **w1-map** (a `tasks:` sequence becomes the map ·
atomic or nothing) · **w2-flow** (`depends_on` + body `tasks.*` reads become
`with:` bindings and `after:` predicates) · **d1-split** (the pre-0.103 string
`command:`) · **esplit** (`vars:` → `inputs:` / `const:` · classify-not-rename)
· **predicates** (`succeeded` → `success`). One `--fix` runs them all in one
loop, so a 0.108.0 file whose only sins are the identity and the tasks list
heals to green in one command. Every round is a transaction: a repair whose
text no longer parses is rolled back and reported, and the file is written
once, from committed text only.

**Still hand migrations in this release** · the `on_finally:` restructuring
(cleanup becomes its own task on an unwind edge), `output:` → `extract:`,
`declassify:`/`inert:` → `lift:`, `config:` → `inputs:` (a classification),
and the `for_each` re-nesting · the refusal teaches each destination at the
point of refusal and `--fix` leaves the file untouched rather than write a
document its own checker would reject. Measured on the pack that ships inside
this binary: 0.108.0 passes 0/40 of these examples · 0.109.0 passes 40/40 ·
every existing file outside this repo sits on the 0.108.0 side of that line
until it is migrated.

#### 4 · Two P0, closed where users install

- **An expression sees only its input (#959).** A `nika:jq` expression could
  read the ambient environment (`env.PATH`) under an ABSENT permits block
  while `check` printed « the body is pure compute so nothing escapes ». The
  retained natives leave the jaq function set at the three seams from ONE
  list (`nika_cap`), and a pinned inventory of jaq natives reddens if a
  future jaq adds one. `check` now refuses the escape at the binary users
  install.
- **A third-party receipt wrote the operator's clipboard (#958).** Three
  fields of a proof receipt (`assert` · `level` · `task`) reached the
  terminal without `escape_tty` while the helper existed in the same file ·
  a foreign evidence pack could emit OSC52. Every receipt field rides the
  escape · proven by mutation before publication.

#### 5 · The two trains (RELEASING §0)

`stable` is the newest tag · what brew · the Registry · nika-action and the
starters install. `next` is `main`, at `<next>.0-dev` between tags · a real
semver prerelease · `nika --version` and every trace say which one they are.
Stable consumers move only when a tag they can install exists.

### Added

- **The project file `nika.yaml` (D-2026-08-11-N5).** An OPTIONAL file at
  the repo root, discovered upward from the CWD the way git finds `.git`,
  carrying the four keys that are decisions of a project rather than of an
  invocation: `ceiling` (the `--max-cost-usd` flag's DEFAULT — the flag
  always wins), `traces.keep` (the retention ladder's file rung, below the
  three `NIKA_TRACE_*` env vars), `registry.floor` (a GATE, max-composed
  with `~/.nika/registry/policy.toml` — a project raises the bar, never
  lowers the operator's), and `arm:` (the team arming registry — parsed
  and shape-validated here, executed by the cadence arc). An absent file
  is today's behavior bit for bit; a present-but-broken one refuses
  before any spend, with its line. The founding wizard offers a
  commented starter on an explicit yes (scripted twin: `nika init
  --project-file`) — never laid silently. No `seat`, no `profile`, no
  permits in it: the portability test (D-2026-08-10-N2) keeps the file
  to defaults and gates, never meaning.

- **The sandbox policy: declared `permits:` now require confinement, or
  the run refuses (#889 · #822's P0 fail-open).** A workflow asserting a
  `permits:` boundary used to run UNCONFINED with a loud note when the
  host had no OS sandbox (Linux without bwrap · any platform without the
  layer) — the contract silently degraded. The severity now derives from
  the declared contract, never the machine: `NIKA_SANDBOX=auto` (the
  default) refuses a permits-declaring workflow with `exec:` children
  the host cannot jail (any block counts — even tools-only jails the
  exec child to the empty axes; permits without exec keep running),
  `require` refuses any unconfined start, and `off` is the explicit
  waiver — parsed ONCE at the composition root
  (`SandboxPolicy::judge`'s truth table: policy × confined × permits ·
  no cell yields a silent unconfined-with-permits), and an unparsable
  value refuses to start (a typo'd security knob loudly defaulting would
  be the fail-open class). The refusal is the typed NIKA-1710 (the
  NIKA-1708/1709 launch-refusal precedent — before the prologue, zero
  events, zero spend), naming the exact per-OS fix. Every waiver is
  WITNESSED: the journal's opening frame attests `sandbox_policy` +
  `sandbox_waived`, so a sealed trace SHOWS the operator chose it (the
  `resume_unverified` trust-amendment precedent). ADR-080 Q4.B amended;
  platform-gated best-effort stays for permit-less runs.

- **The doctor's sandbox row (#891 · #822 P1).** `nika doctor` was
  blind to the OS sandbox: a Linux host without `/usr/bin/bwrap` read
  green while every `exec:` and external MCP spawn ran unconfined. The
  row rides the ONE selection's `SandboxDecision` (#888 — never a third
  selector): a confined backend is Ok and names its mechanism (`Linux
  sandbox (bubblewrap) · backend id: landlock`, the host-granular
  allowlist residual named as follow-on — never a full-strength claim),
  and a `noop` WARNS with the exact per-OS fix. `doctor --json` carries
  the row on the same findings lane.

- **The thinking-budget teaching at check (#651 · leg 3).** A
  reasoning-capable model (the vendored catalog knows) seated with
  `max_tokens` but no `thinking:` now draws the `thinking-budget` hint:
  the reasoning share lives INSIDE that budget, and a heavy think
  concludes with a paid blank answer — the typed NIKA-INFER-004 failure
  at run since leg 1. The hint teaches the declaration before a token
  is spent; a templated seat defers to the run's resolution, a declared
  `thinking:` or a no-think model stays silent.
- **The pause is heard — outbound pause delivery (ADR-111).** When the
  operator sets `NIKA_NOTIFY_URL`, a run that pauses on a human gate
  POSTs its pause payload once — a CloudEvents 1.0.2 structured envelope
  (`sh.nika.run.paused`, deterministic id from trace × task) with
  Standard Webhooks headers (`webhook-id`/`webhook-timestamp` always;
  `webhook-signature` `v1,`-HMAC-SHA256 when `NIKA_NOTIFY_SECRET` holds
  a `whsec_` secret) — then journals the outcome (`notify_delivered` /
  `notify_failed`, two additive event kinds) BEFORE the seal, so the
  chain covers the delivery claim. Default OFF: no URL, no socket. The
  same SSRF floor as every engine egress judges the target (the
  exact-loopback carve-out included, so a local ntfy relay works
  as-is), and delivery failure never changes the run's verdict — the
  run exits `paused` with the same code either way. Proven red-first at
  the binary plane: default-off · signed CloudEvents delivery ·
  metadata-range refusal · unreachable-target non-fatality.
- **Traces speak the current OTel GenAI semantic conventions
  (ADR-112, Part 1).** `nika trace export` now projects an infer/agent
  task's access facts to `gen_ai.provider.name` and
  `gen_ai.request.model` · the *current* semconv names (never the
  deprecated `gen_ai.system`) · so any OTel-native viewer or eval tool
  reads the model and provider off a nika trace with zero translation.
  Provider ids normalize to the semconv well-known values where one
  exists and differs (`mistral` → `mistral_ai` · `xai` → `x_ai` ·
  `gemini` → `gcp.gemini`); every other id passes through verbatim.
  `gen_ai.response.model` stays OUT: the semconv defines it as the
  provider-reported model that SERVED, the journal captures no
  provider-reported id, and emitting the requested name there would
  assert a fact never captured. The mapping lives in the one projection
  module (`nika_dap::otel`); a
  pre-stable-semconv rename is one edit, never a scatter. Additive: the
  existing `nika.*` attributes are unchanged. (The `--format dataset`
  SFT/eval export — ADR-112 Part 2 — is proposed and gated on an
  operator go: it needs the trace to capture input prompts, a content-
  policy change, not a mere projection.)

### Changed

- **Evidence packs redact by default — the auditor gets hashes, not
  payloads.** `nika trace evidence` now builds a REDACTED pack unless
  `--full` is passed: every payload field of the copied journal (task
  outputs, model answers, tool results, failure details, shown prompts)
  is replaced by `{"sha256", "unavailable"}` — the hash of the field's
  own bytes plus the reason it stays with the operator — while every
  structural field (event kinds, chain links, digests, durations,
  verdicts) rides verbatim. The manifest still attests the ORIGINAL
  journal (`journal_sha256`, chain, head, seal), gains
  `trace.projection_sha256` (the one offline check the projection
  supports) and a `redaction` object declaring the class, because a
  pack that cannot say which class it is would wear the old one's
  trust. VERIFY.md now teaches the two classes apart: a redacted pack
  proves the run's INTEGRITY, not its CONTENT — those are two
  different asks, and content disclosure is a separate, operator-side
  gesture. `--full` keeps the historical bytes and says so, in the
  summary and in the manifest. `evidence_format` stays 1 (additive
  fields · zero programmatic consumers). Measured on a real trace:
  the redacted copy leaks zero payload bytes, each placeholder
  verifies against the disclosed value with `shasum -a 256`, and a
  zero-payload journal projects byte-identical.

### Fixed

- **`nika check --fix` can no longer write a document it cannot read.**
  Two defects, one class, measured 2026-08-18. (1) `NIKA-PARSE-005`
  carried its human teaching (a retired key's migration · the modeline
  fix · a small set's own vocabulary) in the same field as its typed
  did-you-mean, and both repairers — `--fix` and the editor quickfix —
  spliced whichever they found: `--fix` on a file carrying `workflow:`
  renamed the key to the sentence "the fields here: nika · model · …",
  announced one repair applied, and left YAML that no longer parsed;
  the shipped 0.108.0 did the same on a de-commented
  `yaml-language-server:` line. `SchemaError::UnknownField` now carries
  `suggestion` (a bare key · what a splice applies) and `teaching`
  (prose · never machine-applied) separately, and every repairer reads
  renames through the one typed door, `rename_repair()`. (2) The repair
  loop is transactional: each round starts from a savepoint, and a round
  whose text no longer parses as YAML is rolled back — rows and notes
  included — and reported as a typed refusal (`✗ FIX refused — … · the
  file is unchanged`); the byte-surgery door refuses any target with the
  shape of prose. Nothing is written except committed text, atomically.
  W1 also keeps a CRLF file CRLF (its rewritten task lines were LF).

- **The argv exec floor is judged at check, with the run's own
  predicate (#605 · NIKA-SEC-001).** `nika check` audited green an
  argv-form `exec:` command the runtime's exec floor refuses at spawn
  (`["bash","-c",…]` — interpreter inline-eval): the static lane was an
  advisory hint over a hand-mirrored eval table, and a hint cannot fail
  a file the run refuses — an `on_error: {skip: true}` leg swallowed
  the refusal as a SKIP and fleets degraded silently. The predicate now
  lives in `nika-types::exec` (the L0 leaf both sides depend on — the
  `host_in_allowlist` precedent), the check emits the `NIKA-SEC-001`
  FINDING (exit 2) for any literal argv the run would refuse, and the
  advisory hint retires. Honest scope, pinned by tests: the shell form
  and any `${{ }}`-templated argv make no static claim — the runtime
  re-judges the resolved argv pre-spawn. `nika explain NIKA-SEC-001`
  teaches exactly that split, the human render gains the EXEC rung, and
  a cross-crate agreement test pins check ≡ run on the same argv.
- **A templated `model:` resolves at run, not just at check (#824).**
  `infer.model: "${{ config.model }}"` checked green — the MODELS rung
  judges the declared default through the one shared static resolver —
  then the run handed the RAW template bytes to the provider, dying
  NIKA-INFER-001 on a string that was never a model id. The dispatch
  now renders `model:` (infer AND agent — the same one-line seam each)
  through the `${{ }}` render path `prompt:`/`system:` already took, so
  the resolved binding is what reaches the wire and the
  spec-sanctioned parameterization idiom (03 §model-by-condition ·
  08 §H20 env targeting) holds end-to-end. A declared-but-valueless
  ref now fails the task loud (NIKA-1702) instead of leaking the raw
  island to the provider. Proven red-first at the seam: the issue's
  repro workflow lands the resolved default in the captured provider
  request body, and the agent loop's mock records `mock/echo`, never
  the template.
- **`check --fix` migrates the pre-0.103 string `command:` (#572 · the
  D1 codemod).** The refusal taught the migration in prose but answered
  « no machine-applicable repairs » on the exact finding whose repair IS
  mechanical. The parser's refusal is now the typed
  `SchemaError::D1StringCommand` (same wire code — NIKA-PARSE-019 — the
  variant exists so the ladder can match it), and the D1 codemod joins
  the ladder: a string command inside an `exec:` block becomes `shell:`
  VERBATIM (the same decoded string reaches /bin/sh -c — semantics
  byte-identical) or, for a bare string of provably-inert tokens
  (no character a shell could reinterpret), the argv flow form the
  grammar prefers. A `command:` outside an exec block (an `invoke:` arg
  named `command`) is never touched; a mapping/null value STOPS with an
  honest note, never a guess. The repair ladder itself descended to
  `nika-cli-host::fix_ladder` at the 15k wall (ADR-110 · one
  architectural unit, two members), and nika-migrate's D1 lives in its
  own `d1.rs` (the 1500-file wall, ADR-023).
- **An empty `infer` answer settles FAILED, never green (#651).** A
  thinking model under a tight `max_tokens` can spend the whole budget
  on its reasoning trace and conclude with a blank visible answer — the
  run used to finish green (exit 0) over `output: ""`, the only signal
  a non-fatal console warn every downstream `${{ tasks.X.output }}`
  silently ignored. The warn is promoted to the typed failure
  `NIKA-INFER-004` (`VerbInferError::EmptyAnswer` · NIKA-435), raised at
  the verb on the exact signal the warn keyed off (blank visible answer
  + token spend — a reported reasoning split OR one undifferentiated
  output count) and carrying the same max_tokens/no-think teaching.
  Non-transient: a declared `retry:` never re-asks at the same budget
  unless the author opts in via `on_codes: [NIKA-INFER-004]`, and the
  billed round-trip rides the failure's spend. The zero-spend carve-out
  is preserved (a blank answer with zero tokens is a plain empty
  completion, not the footgun), and the `schema:` lane is untouched —
  an empty reply already dies NIKA-INFER-002 at extraction, while a
  schema-validated empty container stays a legitimate answer.
- **The resume verifies the chain before trusting the trace (ADR-099
  trust amendment).** `nika run --resume` served a trace's recorded
  successes as cache hits WITHOUT consulting the tamper-evidence chain
  the same journal carries: a chain-broken journal resumed silently
  (exit 0), propagated a forged output into a live task, and emitted a
  fresh journal whose own chain verified clean — one resume laundering
  a journal that FAILS `nika trace verify` into one that PASSES it.
  The chain is now walked BEFORE the fold (the same walk the verify
  verb runs): a broken chain refuses (exit 2 · the FILE class, one
  voice with the verify verb) naming the finding and the opt-out, while
  a crash's honest signatures (killed mid-flight · torn tail) still
  resume — crash-resumption is the use case. The opt-out is NAMED, never
  a silent default: `--resume-unverified` proceeds loudly and the NEW
  run's boot manifest journals `resume_unverified: declared` +
  `resume_unverified_finding`, so a laundered trace can never claim a
  clean ancestry silently. A chainless journal (a `--json` stream
  capture · a pre-0.96 journal) still resumes under the compat — said
  on stderr AND attested on the boot manifest (`resume_unverified:
  unchained` + the reason), so the strip-the-chain forgery (tamper,
  then delete every `chain` field to turn the walker's `Broken` into
  `Unchained`) never converts the refusal into a SILENT proceed.
  Proven red-first at the binary plane: the forgery refused by default
  (and no new journal descending from it) · the opt-out attested on the
  child journal · the stripped forgery attested `unchained` · the
  intact control journaling no claim.
- **An exact `fs` grant is judged by its effective path identity on the
  builtin arm too (NEP-0009).** Between `nika check` and `nika run`, an
  exact grant literal swapped for a symlink (e.g. `read:
  ["./allowed.txt"]` → `./oob/secret.txt`) was served by the in-process
  builtins (`nika:read` · `nika:glob` · `nika:grep` · `nika:write` ·
  `nika:edit`): the boundary re-judged the path the task NAMED, and the
  resolved-vs-resolved comparison followed the planted symlink on both
  sides — the kernel sandbox never sees an in-process read, so the arm
  suspected least was the open one. The boundary now judges the grant's
  effective path identity (the longest existing ancestor canonicalized,
  the FINAL component held lexical — the dispatch re-gate's own
  judgment, restated on the builtin seam) and refuses the divergence as
  `fs.path_mismatch` (`NIKA-SEC-004`), naming the judged prefix and the
  resolved target — one verdict voice on both arms. A symlinked
  ANCESTOR stays tolerated (`/tmp`→`/private/tmp` · a nix-store link),
  a not-yet-existing write target stays legal (law 5), and a grant that
  legitimately traverses a symlink changes verdict: declare the
  effective path instead (the NEP's documented backwards-compat). Proven
  red-first at the unit and binary planes: the swapped exact grant
  refused with the secret never served · the inside-pointing symlink
  refused as divergence · the swapped glob root refused · the honest
  tree admitted before and after.
- **The `--json` stream carries the chain — the trusted-by-default class
  is retired (ADR-099 §5 follow-on).** A `nika run --json` capture was
  the last journal shape the resume trusted by default: the stream wrote
  no `chain` field, so a captured journal resumed under the
  attested-but-unverified `unchained` compat, and tampering the capture
  met no walk. The two lanes now drive one shared chain state — the
  journal file is the stdout stream BYTE FOR BYTE — so a fresh capture
  verifies under `nika trace verify`, resumes on the verified lane (no
  notice · no attestation), and its forgery is refused (exit 2) like any
  broken journal. The compat stays for pre-chain journals and stripped
  forgeries — said, attested, never silent. Proven red-first at the
  unit and binary planes: every streamed line carries the chain · the
  capture verifies and resumes verified with no claim journaled · the
  forged capture refused · both mutations (the insert dropped · the
  head never advancing) kill their tests.
- **The builtin fs boundary's decisions ride the permit witness
  (NEP-0007 law 2 · the declared v1 residual, closed).** Until now the
  in-process arm attested a refusal only as the task's coded failure
  (`NIKA-SEC-004` in `task_failed`), and its GRANTED reads and writes
  were witnessed nowhere — an auditor could not reconstruct what
  authority the builtin arm actually exercised. The boundary's
  enforcement point now records every verdict, allow and deny alike,
  into the attempt's collector (a tokio task-local scoped by the
  runtime per attempt — the one channel that reaches the enforcement
  point inside the shared dispatcher without breaking the kernel's
  `ToolExecute` seam, and the only one that follows the attempt across
  `.await`), and the settle spine emits one `permit_checked` frame per
  decision with `plane: "fs"` — the same payload shape as every other
  plane, so the frames bind to the task that took them through the hash
  chain, on the failure path too (the deny precedes `task_failed`).
  Outside a run the slot is a no-op: telemetry never panics. Proven
  red-first at the binary plane: the permitted read journals one allow
  between `task_started` and `task_completed` · the dynamically-refused
  read journals one deny before `task_failed` with zero secret bytes
  emitted · neutralizing the record kills both planes. The per-op NET
  decisions remain the declared residual.
- **The builtin fs reads open pinned — the enforce→open race closes on
  the in-process arm (NEP-0009 law 6 · the builtin-side follow-on).**
  The dispatch guard judged the path and THEN the op opened it: between
  the two, a parallel task of the same run could swap the judged file
  for a symlink, and a plain `open(2)` would follow it — the parallel
  sibling of the sequenced pivot the re-gate killed. Every builtin READ
  (`read` · `edit`'s read phase · `grep` · `glob` · `decide`'s bundle ·
  `fetch`'s multipart parts) now runs against the judged fs: the path is
  re-judged at open time (SILENT — the guard already witnessed the op's
  one fs frame), then opened `O_NOFOLLOW`, so the KERNEL refuses a
  swapped final component (`ELOOP`) inside the syscall itself,
  atomically — no check-then-act, no window. A pre-existing
  inside-pointing symlink is still served (resolved, re-judged on the
  TARGET, re-opened pinned · the no-regression rule), a dangling link
  keeps the file-not-found verdict, and a redirect storm refuses coded
  past a hard hop bound instead of hanging. Writes need no pin: the
  atomic temp+rename lane replaces a symlinked destination, never
  follows it. Declared gaps, honestly on the record: the pin compiles
  on the tier-1 unixes (macOS · Linux) and every other target
  degenerates to enforce + plain read; `O_NOFOLLOW` pins the FINAL
  component only, so a swapped ANCESTOR directory stays a residual of
  the exec arm's `--bind-fd` follow-on class; and `chart` keeps the raw
  fs because its save lane re-reads the write-permitted artifact for the
  idempotence law. Proven red-first at the unit plane: the nofollow open
  never serves a symlink target · the swapped exact grant refuses coded
  with zero secret bytes · the inside link serves byte-exact · the loop
  and the storm refuse coded and RETURN.

### Security

- **`nika mcp --transport http` refuses a non-loopback bind without a
  bearer token (#890 · #822 P0/P1).** `NIKA_MCP_TOKEN` was optional and
  the auth gate treated its absence as OK — right for the loopback
  default, a classic misconfiguration the moment `--bind 0.0.0.0` met a
  multi-user or VPS host. The refusal is code-enforced before serving
  (`HttpServer::guard_bind_auth` judges the RESOLVED address, so
  `localhost` reads as the loopback it bound, never the spelling) and
  names both fixes: set `NIKA_MCP_TOKEN` to require a bearer, or bind a
  loopback address. Loopback without a token stays convenient,
  unchanged.

## [0.108.0](https://github.com/supernovae-st/nika/compare/v0.107.2..v0.108.0) - 2026-08-05

**The access layer arrives; the check stops trusting what it cannot
read.** Three steps of the access ratification land (`access` picks the
path, `model:` picks the intelligence — resolver, pin, and the ACP
harness class with its mock instrument), and the zero-authority scan now
refuses every `exec` spelling — the shell form included. Proven
red-first, like everything on this train.

### Added

- **Execution access · step 3 — the ACP harness class** (D-2026-08-04-N1
  · P3). The `nika-acp` spec crate lands (ACP 2.0.0 = wire v1, verified
  against the published schema) with its **mock agent in a quarantined
  workspace** — the instrument that proves the wire without a vendor in
  the loop — and the kernel gains the **`AgentBackend` seam**
  (`nika-kernel-ai` · lane-agnostic): the harness access path plugs in
  where every other lane already does, behind a trait, never a special
  case.

- **Execution access · step 2 — the deterministic resolver, the pin,
  the narration** (D-2026-08-04-N1 · steps P2.1–P2.8). `model:` picks
  the intelligence; **access** picks the path. The admission-time
  resolver is a pure function with a strict sovereign order (`local <
  mock < harness < oauth < api`, codepoint tie-break) — enumeration
  order can never change the outcome (property-tested), and every drop
  carries its witness (dimension · layer · teaching line). `--access
  <path>` on `run` and `try` pins the path: judged at the launch gates
  before the prologue (zero events · zero spend); unsatisfied refuses
  with `NIKA-1800/1801/1802`, never substitutes. Without a pin the RUN
  path is byte-unchanged — the gate never fires; the audit surfaces do
  grow (additive, the `models_catalog_warnings` precedent):
  `check --json` gains the advisory `access_plan` rows, `explain`
  gains the « access (this machine) » section, the run header
  announces an explicit pin, and `AccessPlan` records the chosen
  path's id. Run-start liveness stays the only runtime act: a dead
  pinned path refuses, never falls back.

### Fixed

- **Every catalog-miss advisory printed twice.** A duplicated block in
  the models rung pushed each `models_catalog_warnings` row two times;
  one block remains, with a test that proved red against the doubled
  version first.
- **The taught surfaces stopped misdirecting.** The injected session map
  no longer teaches doors that refuse; `--recipe starter` says which
  half a pipe cannot deliver; the taught next step works on the machine
  that just ran it; `explain`'s two golden-lane refusals teach instead
  of misdirecting; the check strictness hint says close, not add.
- **The sandbox tightened twice more.** The scratch claims its path
  exclusively — never adopts one — and the shared host tmp stops being
  an ambient grant; exec-runner's scratch moved out of egress at the
  file wall.
- **A resumed run keeps its seat.** The model override rides the trace —
  a silent seat swap refuses.
- **The pure-internal security exemption asked about the tool, not the
  call.** It asks about the call now.
- **Every pack skeleton passes its own golden lane** — the armor reaches
  the tails.

### Changed

- **A blank answer that burned tokens is a typed failure, not a green
  task (#651 · NIKA-INFER-004).** A thinking model under a tight
  `max_tokens` can spend the whole budget on its reasoning trace and
  conclude with a BLANK visible answer — the task used to settle green
  over `""` (a warn nobody downstream acts on) while every
  `${{ tasks.X.output }}` silently resolved to nothing. The OBS-E warn
  is promoted to the typed `NIKA-INFER-004` failure (engine NIKA-435),
  fail-closed with the spend attached as ledger evidence. The signal is
  deliberately narrow: blank visible answer PAIRED with token spend
  (a reported reasoning split, or an undifferentiated
  `output_tokens > 0` — the ollama path strips the think block
  upstream). A blank answer with zero spend stays green (a plain empty
  completion is not the footgun), the `schema:` lane is untouched (an
  empty reply already dies NIKA-INFER-002 at extraction; a validated
  empty container is a legitimate answer), and `on_error:` remains the
  author's named opt-out. The summary's « 7/7 done » can no longer
  count an empty paid answer as done — the failure default makes the
  honesty leg moot.
- **The anthropic default moves to `claude-sonnet-4-6`.** The catalog sat
  two generations back (`claude-sonnet-4-20250514`) while the spec's
  examples and conformance fixtures standardized on the 4.6 id — the
  docs-spec coherence vector named the contradiction on the public site.
  Six sites across the anthropic · openrouter (`anthropic/…`) · bedrock
  (`anthropic.…-v1:0`) entries follow each gateway's naming form. Parser
  and bench test strings keep the historical id on purpose (they are
  inputs, not defaults).

### Security

- **A shell-string `exec:` with no `permits:` block passed `nika check`
  green.** The zero-authority scan (F-O8 · NEP-0003) refused the argv
  spelling it can read (`command: ["rm", …]` → `NIKA-AUTH-006`) and
  deferred the shell spelling it cannot (`shell: "rm -rf …"`) to the
  runtime — the exact inversion of a security gate: the verifiable door
  refused, the unverifiable one open. Law 1 puts the exec capability in
  `Required` the moment an exec task sits in the body, whatever the
  command form; law 3's runtime deferral owns dynamic VALUES, never the
  category question, and the runtime refused both spellings all along.
  The shell form and a computed argv head now refuse at check with
  `NIKA-AUTH-006` — check ≡ run restored. Repair:
  `nika check --infer-permits` writes the block (the shell form widens
  `exec` to `true` with a note; rewrite to the array form for a program
  allowlist).

## [0.107.2](https://github.com/supernovae-st/nika/compare/v0.107.1..v0.107.2) - 2026-08-02

**The adversarial pass.** 0.107.1 shipped six boundaries; an adversary
was then set on that work with one instruction, break it. Three of six
claims fell, and hunting what the first repairs still let through found
four more. Every fix below was proven on the binary before it was
touched, and each carries the mutation that makes it fail.

### Two doors that were open

- **A permit could name a system root by shouting it.** `/root/./x*`
  closed in the morning; `/ROOT/x*` did not. macOS ships a
  case-insensitive filesystem — `/ETC` and `/etc` are the same inode —
  so an exact-match root check let a permit name any system root on the
  very platform the seatbelt backend serves. The comparison folds case
  now, and refusing these on Linux can only ever refuse a path that does
  not exist.
- **A secret was redacted in the trace and printed on stdout.** The
  redacting sink wraps the event lane, and a run's `outputs:` map is not
  an event: it rides `RunOutcome` straight to `--output json`, where the
  CLI serialized it verbatim. The static check refuses a *declared*
  egress, but not the side channel this backstop exists for — an exec
  catting a file-sourced secret, a tool echoing its input. The map is
  scrubbed now, at any depth.

### Three guards that judged less than they claimed

- **The dangerous-environment floor is proven entry by entry.** Forty
  names, eleven asserted; deleting the one that makes macOS load
  arbitrary code into every dynamically linked child left the suite
  green. Both halves are pinned now — every listed name enforced, and a
  floor naming what may never leave.
- **A tainted `nika:notify` target is judged whatever channel carries
  it.** One `${{ }}` in `channel:` made the tool unclassifiable and the
  re-gate silent: the same payload passed with rc=0 templated and rc=2
  spelled out.
- **The run guard speaks only about runs.** An unreadable payload from a
  host we do not parse denied every command in the session — `ls` came
  back « nika run blocked ». It now degrades only for bytes that could
  have carried a run.

### One law, two implementations

- **The MCP spawn composes the child environment through the same
  function as the exec runner**, which its own module doc had promised
  all along. The copy was equivalent, and equivalence was exactly what
  nothing guaranteed.


## [0.107.1](https://github.com/supernovae-st/nika/compare/v0.107.0..v0.107.1) - 2026-08-02

**The composition wave.** The 0.107 train shipped the trust OBJECTS;
an outside checkpoint then measured what a stranger actually meets and
found the composition missing — the concierge still taught a verb the
0.107 rename had killed, doctor rendered a table-declared guard in the
green word a proven one earns, and every surface composed its own next
step. Three synthetic personas (novice FR · senior dev · privacy-first)
walked the binary end to end; the fixes below each carry the transcript
line that found them.

### The doors tell the truth

- **One door catalog, one ratchet.** `DoorId{Discover,Create,Project}`
  is the single source behind every taught first-contact command; the
  five workspace states, the chat-only JSON mirror and the kit's
  `allowed-tools` all derive from it, and a new test replays EVERY
  command welcome can teach against the live clap tree. The dead
  `nika examples` the concierge kept teaching (the parser refuses it)
  is gone from text, JSON, kit and docs — and hygiene vector 45 sweeps
  the whole rendered output of three surfaces so the class cannot
  return.
- **`nika try` opens on three jobs, not thirty-nine.** The storefront
  names one contrasted trade each (support · meetings · release), every
  row a taught `try <slug>` with its pitch, closed by four doors and the
  verb legend the glyphs never had. A pipe still gets the full parsable
  shelf byte-for-byte (the editor's wire contract), and `--all` forces
  it on a terminal.

### The verdicts narrow to what they cover

- **`guard-declared` is not `guarded`.** The host receipt gains
  `guard_evidence` (declared · loaded · proven): a hook the static kit
  table declares now reads « guard-declared (kit ships hooks · unproven
  in session) », and the bare word waits for a live allow+deny canary.
  `welcome --json` carries the same companions the doctor receipt does.
- **`--plain` means ASCII in doctor too** — the verb rides the same
  sobriety seam the concierge rides; the machine lane stays byte-exact.
- **The journey names its cloud endpoints in words** — locus, retention
  and training were machine-only facts while the human line read
  identically for a mock and a cloud model.
- **`risk unbounded` carries its next move**, the red pre-run
  diagnostic ends its own line, and a sensitive voyage discloses that
  its trace keeps full task outputs in plaintext, with the removal
  handle beside it.

### The experience contracts land

- **`ExperienceStateV1` → `route()` → `NextActionV1`** — one pure
  decision table (drift ≻ unselected multi-root ≻ chat-only ≻ paused ≻
  failed ≻ findings ≻ unknown inventory ≻ clean ≻ several ≻ create ≻
  discover), one primary CTA with at most two safe exits, and gates
  asserted structurally per action across the swept state space: no
  door hides a run. `welcome --json` is its first consumer;
  `WorkflowPreviewV1` renders the same content in strict ASCII and
  Mermaid, beside the context, privacy and authority receipts.

### The boundaries hold where they claimed to

A sweep for checks that report a verdict they cannot justify found six
places where a boundary was green and open. Every one was proven on the
binary before it was touched, and every repair carries the mutation that
makes it fail.

- **A cross-origin redirect no longer carries the API key.** The header
  list that strips credentials on a redirect had been copied from a
  general-purpose HTTP client and did not know `x-api-key` — which the
  Anthropic wire sends, and which `nika:fetch` documents to workflow
  authors as the place auth rides. A 302 handed the live key to whatever
  host the redirect named. One list now serves both the redirect strip
  and the Debug redaction, and `check-credential-headers.sh` fails the
  build when a source sends an auth header the list does not name.
- **A permit can no longer name a system root through `.`.** Both
  sandbox backends refused `/root/*` and granted `/root/./x*` — they
  compared text where the kernel folds, so the grant resolved back to a
  read-write bind of the host's root inside the jail. The fold lives in
  the kernel now, shared, and the guard test walks every root in the
  const across six spellings instead of the three its doc comment named.
- **A tainted `nika:notify` target is re-gated like a `fetch` url.**
  Notify sat in the taint re-gate's tool list and did nothing: the list
  read `url` from every member, and notify's argument is `target`. The
  re-gate asks the capability table now. Measured against the reference
  oracle: parity unchanged at 216/220, all 39 examples clean.
- **The run guard stops approving what it never judged.** On the generic
  hook wire, "no opinion" and "judged clean" shared an arm, so the kit
  answered `permission: allow` to every shell command in the session.
  A command with no `nika run` in it now gets `{}` — the bytes that mean
  *behave as if this hook were not installed*. And with the binary off
  PATH the shim used to deny everything, telling the user `ls` was
  "nika run blocked"; it degrades only for commands that could have been
  ours.
- **The editor offers what the binary accepts.** Completion taught
  `description:` at top level (the parser refuses it with its own code)
  and `google/` as a provider (this binary does not resolve it — the
  runnable id is `gemini`), while never offering four keys the parser
  takes. The vocabulary derives from the parser and the catalog now.
- **The privacy gate guards the layout that exists.** Six of its seven
  patterns matched no directory that still exists, leaving the private
  content it was written for entirely unguarded, and a `git mv` skipped
  it altogether.

### The owned copy works where it landed

- **The taught command inside YOUR file runs in YOUR workspace** — all
  39 pack examples self-reference their pack path, and the copied
  `# Run ·` comment exited 3 when pasted; `nika new` re-points it to the
  destination.
- **A dropped cadence is named** — « chaque lundi … » routed on the work
  and ate the schedule half in silence; the file owns the WORK, a
  scheduler owns WHEN, and the note says so.
- **The guard judges both spellings of its own binary** (`nika` ·
  `nika-cli`): a dev-build invocation rode past the uncapped-priced-run
  refusal as no-opinion.
- The wizard's run hint follows the chosen seat instead of teaching a
  literal `$0.00` on an Ollama pick.

## [0.107.0](https://github.com/supernovae-st/nika/compare/v0.106.1..v0.107.0) - 2026-08-01

**The trust release.** An outside audit measured the experience instead of
the feature list — five waves, seventy-two receipts, twenty-two P0s — and
the arc closed every one: welcome gates its run CTA on the exact file's
verdict, a non-affirmative consent fires zero effects (NIKA-SEC-014, minted
spec-first with its conformance fixtures), the hooks' judge moved into the
binary (`nika guard`, fail-visible, bypass matrix in test), autonomy is
graded before it is colored (the risk rung never renders green past
Supervised),
the data journey names sources, destinations and secrets before the run,
and redaction works by provenance — a one-byte secret dies the same death
as a long one. Around the core the doors multiplied honestly: one client
matrix asserts the coverage of thirty-one hosts, `nika wire` previews and
never rewrites unasked, and every probed host gets a capability receipt
that separates oracle-only from integrated. The discipline held on the
inside too: the host plane descended to `nika-cli-host` at the 15k wall
(ADR-110 — one unit, two members, zero call-site churn), and the wall
itself grew a descent window, the crate-size vector warning at eighty
percent instead of ambushing the push.

**The night train (Jul 31 → Aug 1).** The teaching grammar collapsed to
three doors and the whole surface followed: `nika try` is the showroom
(bare lists · a slug runs the mock rehearsal offline by default — no
`--model mock/echo` incantation left to teach · `--model self` keeps
the example's own seat), `nika new` takes ONE positional gesture
(an example slug lands verbatim WITH its ingredients · plain words
BM25-route across the whole catalog · a lone `<name>.nika.yaml` names
a destination and gets the wizard), `nika init` founds. The `examples`
verb tree died; `evidence` and `receipt` moved under the run's dossier
as `nika trace evidence` / `nika trace receipt explain`. The same
night, four replayable persona waves judged the result end-to-end and
the last taught-line breaks fell before morning.

### The three doors (V5)

- **`nika try [slug]` — see it work, own nothing** (#796). Offline by
  default: the rehearsal seat law lives in one place and is pinned;
  the shelf, the take and the founding read as one grammar. Every
  teaching surface (welcome START block · lazy door · rescue tips ·
  recipes · briefs/AGENTS scaffold · kit skills · README · funnel e2e)
  speaks the new forms in the same release.
- **`nika new` routes the WHOLE catalog** (#778): the 26 human-worded
  jobs are visible to the one surface that takes human words — a routed
  example lands verbatim with its fixtures beside it (one shared
  materializer for every taking door), a routed skeleton instantiates,
  and below the confidence bar the clarify names facets + each entry's
  own description line.
- **The MCP oracle walks the same door** (#797 · RAMS-11): a
  plain-words miss on `nika_examples`/`nika_template` routes through
  the CLI's router — the interpretation is SAID in a leading
  `# routed:` comment, single tokens keep the unknown-key contract,
  and the adversarial-keys pin holds verbatim.
- **Every taught line survives the paste-back** (#798): the clarify
  hint is facet-aware (a skeleton carries its `<dest>.nika.yaml`, an
  example lands bare) and a spaced intent is re-echoed shell-quoted —
  the gauntlet's teach-a-command-that-breaks class, closed the night
  it was measured.

### The honest machine (the tier-B wave)

- **A dead local server stops the run in seconds, named** (#786): the
  B-5 liveness gate dials the local endpoint BEFORE any wire call —
  silent (nothing listens) and mute (accepts, never speaks) refuse
  fast with the exact repair (`ollama serve`, or rehearse with
  `mock/echo`); a keyed cloud seat, mock, or a failed probe never
  block.
- **The models rung consults the catalog before saying `resolves`**
  (#784) · **the local route stops teaching what this binary cannot
  do** (#788) · **a healthy machine reads calm** — doctor's advisory
  noise folds (#789).
- **One machine-truth** (#781): every provider count names its facet
  (`38 catalog entries · 15 wired in this build · 10 take a key`) from
  a single projection, pinned end-to-end on the rendered surfaces.
- **mock is a proven zero** (#785): the check's floor and the run card
  say the same $0.00, and the exec-floor hint drops its phantom route.
- **The fan says its tally** (#793 · V7-1): a `for_each` with
  recovered items renders `N/M ok · K recovered` — the silent fan
  class dies at ITEM level.
- **The closer never promises more than the judge checked** (#795):
  `explain`'s canon-row closer is per-class — SEC-004 teaches the
  literal/computed split (a green PERMITS is not the RUN's promise) —
  and the C-9 lessons (#787) speak one voice across CLI and MCP.
- **A closed pipe dies clean** (#783): `nika run | head` ends with the
  screen silent and the honest 141, never a Rust panic dump.
- **A UTF-8 BOM parses** (#791) · **`examples run`/`copy` history**:
  the pack resyncs landed the fanout-trap fix, the write carve-out and
  the measured-truth corpus (#782 · #790).
### Added

- **`nika init` writes the two project MCP doors.** The root `.mcp.json`
  (the standard `mcpServers` stanza FOUR agent surfaces read natively:
  Claude Code project scope · Grok Build via its Claude compat · GitHub
  Copilot CLI · Warp) and `.agents/mcp_config.json` (Antigravity CLI's
  workspace file, beside the authoring skill under the cross-vendor
  `.agents/` convention). One shared body, seventeen scaffold targets —
  a repo equipped by `nika init` now hands the read-only oracle to six
  clients with zero machine wiring.
- **`nika wire copilot` + `nika wire amp` — the wave-3 doors, shapes taken
  from the clients' own writers.** Copilot CLI gets `~/.copilot/
  mcp-config.json` with the exact `{tools: ["*"], type: "local"}` entry its
  own `copilot mcp add` writes (a copilot-added entry reads `· current`,
  never churns); Amp gets the literal dotted key `"amp.mcpServers"` in
  `~/.config/amp/settings.json` — JSONC settings get the snippet,
  byte-identical (the Zed contract). `wire all` covers twenty-one targets.
- **`nika wire` grows four client doors: grok · antigravity · kimi · kiro.**
  Grok Build gets the Codex-shaped `[mcp_servers.nika]` table in
  `~/.grok/config.toml` (comments preserved · idempotent — its Claude
  compat already merges the project `.mcp.json`, the native table survives
  a `[compat.claude]` toggle); Antigravity CLI (`agy`, the gemini-cli
  successor) gets the standalone `mcpServers` entry in
  `~/.gemini/config/mcp_config.json` per Google's migration contract;
  Kimi Code CLI gets `~/.kimi-code/mcp.json` per its two-level contract;
  Kiro CLI (the Amazon Q rebrand) gets `~/.kiro/settings/mcp.json`.
  `wire all` covers all four.

- **`nika doctor` speaks the kit↔binary handshake.** Installed plugin kits
  get one row each, probed at the rung their sessions actually load
  (Cursor local drop · Claude Code install of record · Codex per-version
  cache — marketplace clones as fallback): green on the binary's release
  train, ⚠ with the
  exact per-client refresh command when a kit lags (Claude Code names BOTH
  rungs — the half-climbed ladder is the proven trap), ⚠ with `brew upgrade
  nika` when a kit rides ahead. Patch drift is not a finding (kits are cut
  per train). The session-context hook grows the same two probes client-side:
  a missing binary teaches the install line, a train divergence names the
  direction-aware align command. `nika welcome`'s machine mirror names
  train drift in one line and routes to doctor — aligned or absent kits
  stay silent (carry information, never a lecture).
- **A green run names its fruit and stops lying.** The run card says
  what the run actually produced, and a run whose green hid an untruth
  says so out loud — the run-comprehension surface's honesty pass
  (A-2 of the first-run-truth arc · the 19-persona gauntlet).

### Fixed

- **The permits stop punishing honesty.** The trifecta explains its
  dominance rule (the dominating gate named · the bypassing data edge
  named · the taught fix provably flips the verdict green), a
  workspace-escaping path earns no machine fix but the taught narrow
  way, and an exec grant reads "exec outside the fs bounds" — never a
  default-deny misread (A-1 · Nina's BLOCKER).
- **`nika examples copy` brings the recipe's ingredients.** Every
  `examples/fixtures/…` file the copied body reads now lands beside it at
  the exact relative path the yaml names (existing files are kept, never
  clobbered) — the copied recipe's own taught offline run used to die
  `NIKA-BUILTIN-READ-001` on the missing fixture (the one rage-quit of the
  19-persona gauntlet, 2026-07-31).
- **Bare `nika` greets in a pipe too, exit 0.** The pipe used to get clap
  usage at exit 2 — an agent's first contact read as breakage, and spec §4
  reserves 2 for file findings. Both worlds now get the welcome mirror;
  `--help` stays the reference card.
- **The agent's request order is the author's `tools:` order.** The tool
  list used to ride catalog order with loop-owned intrinsics appended last,
  so `nika:done` could never come first — and the offline rehearsal
  (mock M1 invokes the first granted tool) died at turn 1 on any fs-scoped
  loop, refused by the boundary it could not satisfy. Listing `nika:done`
  first now closes the loop cleanly offline; real models are unaffected (a
  whitelist has no ranking semantics).
- **`nika explain NIKA-BUILTIN-READ-001` teaches the contract** — paths
  resolve from the RUN's working directory, with the three exits (look ·
  cd-and-run · `examples copy` lands the ingredients) — instead of the
  namespace boilerplate.
- **The taught mock rehearsal says its limit on writing workflows.** The
  mock swaps the model, not the effects: a mock re-run after a real one
  overwrites the real artifacts (a gauntlet persona lost a real
  CHANGELOG.md that way). The `explain <file>` rehearsal line on a
  writing workflow now says `file writes STILL land — rehearse before the
  real run, not after`.
- **`nika model` teaches an ungated starter id.** The suggested
  `Qwen/…-GGUF` repo answers 401 without a token; the taught id is now the
  public `unsloth/Qwen3-4B-Instruct-2507-GGUF` mirror (verified 200).
- **NIKA-SEC-014 names the real defect.** The finding said the route
  « never consumes the answer » — a route that binds the answer but never
  gates on it read that as false; it now says « never gates on the
  answer ».

- **A human gate can no longer kill a first run.** A `nika:prompt` with no
  `default:` used to die `NIKA-BUILTIN-PROMPT-001` in milliseconds on every
  text-mode run — a pipe, CI, an agent, even a human at a terminal (the
  ADR-099 durable pause was armed on the `--json` output flag, and the
  promised TTY prompter had never shipped). The rider is now armed on every
  lane: unattended runs **pause durably** (exit 4 · `workflow_paused`
  journaled · never a failure frame), and at a terminal **the gate asks you
  directly** (confirm `[y/N]` · choice by value or index · input verbatim),
  binding the answer through the same attested resume path as a manual
  `--resume --answer`. Walking away (Ctrl-D) leaves the paused trace and
  its taught resume line.
- **The paused run teaches its exact next move.** Every lane prints a
  paste-able `resume: nika run <file> … --resume <trace> --answer
  <task>=<value>` line that now **carries the run's own `--var`/`--model`**
  (a required-input workflow refused the taught line without them). The
  frame renders the pause honestly: the gate row turns `◇` amber and the
  paused card names the awaiting task.
- **Fallout counts beside the root cause.** One failed gate cancelling 22
  downstream tasks used to read `23/23 done · 1 failed` — the meter now
  says `1 failed · 22 blocked`.
- **`nika explain NIKA-BUILTIN-PROMPT-001` teaches the contract**, not the
  namespace boilerplate: the cause and all four exits (`--answer` at
  launch · resume · `default:` · the terminal ask). The `nika check`
  headless-prompt hint teaches the same working recipes.

### ✨ Features
- **catalog** — Pricing carries upstream truth with provenance ([95af0984a](https://github.com/supernovae-st/nika/commit/95af0984ac88577f5dc949e2bf57e39b29832fbe))
- **catalog** — The schema learns energy · data policy · effect hints ([814df726c](https://github.com/supernovae-st/nika/commit/814df726c4fa3e5f70ce060cf4e05d34b96c2ca8))
- **catalog** — The sourced facts enter and the refresh learns to keep them ([b4f5df37f](https://github.com/supernovae-st/nika/commit/b4f5df37f6846109c17931ab2ed532dc42c322bb))
- **catalog** — Two price catalogs face each other and nobody wins ([88f98fc39](https://github.com/supernovae-st/nika/commit/88f98fc396e57e229df3948546c25a86ba347f04))
- **catalog** — The one door opens ([03ab16f5b](https://github.com/supernovae-st/nika/commit/03ab16f5b437309dfc060af693069054039755e3))
- **catalog** — Sixteen measured energy rows from the public leaderboard ([2e2813368](https://github.com/supernovae-st/nika/commit/2e2813368e741d86c07fdbc1941d248b467f0dff))
- **check** — The energy reading joins the report (NEP-0018 · the 15k descent) ([136e3ed93](https://github.com/supernovae-st/nika/commit/136e3ed93257d9138c3807a2fdf53fe28965ce96))
- **check** — The judge-night lane lands — judged defaults, energy aggregate, adversarial greens ([b176a344d](https://github.com/supernovae-st/nika/commit/b176a344da7c3d4d322d53f6deba3cbcf4e195d6)) ([#768](https://github.com/supernovae-st/nika/pull/768))
- **check** — One verdict everywhere · a risk grade names the unbounded ([631940a45](https://github.com/supernovae-st/nika/commit/631940a45c088b4fddd652fe1dea9319eb8c87a7))
- **check** — The consent lane grades the gate, never the position ([6d1f68243](https://github.com/supernovae-st/nika/commit/6d1f68243ce3d1f67598b2fcd7e31c6f3a61b672))
- **check** — The exec floor is predicted · the gate must be affirmative ([1511b2be2](https://github.com/supernovae-st/nika/commit/1511b2be2b5eaacd8dc29cb1dbd1b3c3f3cb8649))
- **check** — False fires exactly zero effects — NIKA-SEC-014 ([1c0f708c5](https://github.com/supernovae-st/nika/commit/1c0f708c5b782757659fadeb34a0f9971b38fba3))
- **check** — The data journey names every flow before the run ([5cc366dd8](https://github.com/supernovae-st/nika/commit/5cc366dd829ae63db15fbf529757e64490e2975a))
- **cli** — Welcome gates the run cta on the exact file's verdict ([1a7053554](https://github.com/supernovae-st/nika/commit/1a70535548b9402dc93107f86280f98291d0f675))
- **cli** — Nika guard — the hook's judge lives in the binary ([50ce7a452](https://github.com/supernovae-st/nika/commit/50ce7a45256269379db7607ddeceabf80b5dff0a))
- **cli** — The context envelope resolves the workspace once ([fd143d214](https://github.com/supernovae-st/nika/commit/fd143d2146a690b92433f9dc6e3afb49cb438fb3))
- **cli** — The adoption ladder greets each rung with its own line ([72e892e42](https://github.com/supernovae-st/nika/commit/72e892e42e3adfbd3611354048796abe5226986d))
- **cli** — A failed latest run opens on the repair, never the re-run ([44847b275](https://github.com/supernovae-st/nika/commit/44847b275d5ee87cbdb6d375a22f7f11785bf8e7))
- **cli** — Doctor names each host's honest capability level ([c21ef94c8](https://github.com/supernovae-st/nika/commit/c21ef94c893fc0499721768cd78a034bff95cbb1))
- **cli** — Welcome resolves its context through the envelope ([202395c66](https://github.com/supernovae-st/nika/commit/202395c66531da5e56cab210c3aa13c60b4a8b37))
- **cli** — Doctor hands every host a capability receipt ([8c66e48a5](https://github.com/supernovae-st/nika/commit/8c66e48a539167c1cfc83ec0ced74c32f3cfb35a))
- **cli** — Wire previews, detects, and never rewrites unasked ([7ffe07e68](https://github.com/supernovae-st/nika/commit/7ffe07e68bb39c3399504f859767e95db7a5ebeb))
- **cli** — The binary reads the one client matrix ([a59df6a32](https://github.com/supernovae-st/nika/commit/a59df6a321391e7f05b108de87112953405d3b00))
- **cli** — Ascii means ascii · metrics stay local and content-free ([b81060688](https://github.com/supernovae-st/nika/commit/b81060688776e41dd9deee795dfa4446083175b5))
- **cli** — The host plane descends to its own member · nika-cli rejoins the 15k wall ([bb231b8bc](https://github.com/supernovae-st/nika/commit/bb231b8bc6c5b21f5f819b778a424155578abb48))
- **display** — The consent refusal prints its rung ([779d3d7f1](https://github.com/supernovae-st/nika/commit/779d3d7f19fafe135ddd9d102635d414d914e129))
- **estate** — The mirror stops being a convention and becomes a gate ([fc3f7aa8e](https://github.com/supernovae-st/nika/commit/fc3f7aa8e564168bd99a4cb0f277b704d21bb4bf))
- **hooks** — The session seed probes the binary and names version drift ([dd377a268](https://github.com/supernovae-st/nika/commit/dd377a268c5aac545e646ca0451a09f932ec41ae))
- **hygiene** — The crate-size wall gains its descent window ([83ca64a44](https://github.com/supernovae-st/nika/commit/83ca64a449774baaea34f2ae64e0e667e5bd0295))
- **init** — The project grows two MCP doors · one stanza, six clients ([e8b1930c4](https://github.com/supernovae-st/nika/commit/e8b1930c42317acaaa0d4e87b599083013440315))
- **kit** — The doctor command makes coherence a one-slash gesture ([23c22463a](https://github.com/supernovae-st/nika/commit/23c22463ad7e35847780afb28b2de9796e28ab96))
- **kit** — The codex page opens with three doors, not three verbs ([24e9335f6](https://github.com/supernovae-st/nika/commit/24e9335f6d217da47a999e53a135b4027bf62456))
- **lot-3** — The thin laws (F-P13..F-P23 · NEP-0014) ([#753](https://github.com/supernovae-st/nika/issues/753)) ([472c704b4](https://github.com/supernovae-st/nika/commit/472c704b4d4935d352c8fd91c325808ae7c19c18)) ([#753](https://github.com/supernovae-st/nika/pull/753))
- **media** — The hero gains the window chrome · the dead slug class dies ([bd915b85f](https://github.com/supernovae-st/nika/commit/bd915b85fa5eb25ec03053e4c855d34d9dce6e11))
- **nika-cli** — Doctor probes the installed kits and names their drift ([b20e9b338](https://github.com/supernovae-st/nika/commit/b20e9b3388eff6e1f8731bceb145d7ff39f7867d))
- **nika-cli** — Welcome names kit train drift and routes to doctor ([c38ce446a](https://github.com/supernovae-st/nika/commit/c38ce446ae22db72544f67b0c980df9bb7d11999))
- **nika-cli** — The energy rung — cost honesty in watt-hours (NEP-0018) ([e92570786](https://github.com/supernovae-st/nika/commit/e9257078654c781dd6741d32f53235c426e0a956))
- **nika-store** — The signed-memory substrate (F-P8 · lot 2) ([#755](https://github.com/supernovae-st/nika/issues/755)) ([62728587b](https://github.com/supernovae-st/nika/commit/62728587b2f80f921c9f1804cd81cab70b5fb452)) ([#755](https://github.com/supernovae-st/nika/pull/755))
- **onboard** — The intent shapes the workflow before the template ([1ea784404](https://github.com/supernovae-st/nika/commit/1ea7844045afb469ec9dd162aab23e59c8cbb7da))
- **providers** — Readiness is a ladder · local is a place ([96da9aeb9](https://github.com/supernovae-st/nika/commit/96da9aeb955bfc9a8e90603d385a9fd9237a5fe2))
- **registry** — The provenance ladder + the operator admission floor (F-P27 · NEP-0016) ([#751](https://github.com/supernovae-st/nika/issues/751)) ([51f170bb5](https://github.com/supernovae-st/nika/commit/51f170bb5907053da3a2c347e6fc5fe5374827f1)) ([#751](https://github.com/supernovae-st/nika/pull/751))
- **runtime** — Preview-commit · the judged request is the fired request (F-P6) ([#749](https://github.com/supernovae-st/nika/issues/749)) ([d5b411b36](https://github.com/supernovae-st/nika/commit/d5b411b36fa0cdf7d69218cea3ef8397394940f4)) ([#749](https://github.com/supernovae-st/nika/pull/749))
- **runtime** — Nika test runs on a simulated effects plane ([871460cbd](https://github.com/supernovae-st/nika/commit/871460cbd726eca6ab017b23531aa2ef800d6d4b))
- **runtime,check** — Human approval is a bounded, attested ticket (F-P4 · NEP-0013) ([#744](https://github.com/supernovae-st/nika/issues/744)) ([bee82f9b1](https://github.com/supernovae-st/nika/commit/bee82f9b166ed4a15e54172393b83f8ab52ae467)) ([#744](https://github.com/supernovae-st/nika/pull/744))
- **runtime,check** — The thin-laws lot 3a (F-P13 · F-P15 · F-P16 · F-P21 · NEP-0014) ([#752](https://github.com/supernovae-st/nika/issues/752)) ([5ed027cb1](https://github.com/supernovae-st/nika/commit/5ed027cb106b2ec21b6ac3505105f9ee361bc73e)) ([#752](https://github.com/supernovae-st/nika/pull/752))
- **wire** — Grok and antigravity become wire targets · the toml family is born ([34bb14833](https://github.com/supernovae-st/nika/commit/34bb14833fda59b1f6ce8833a059b0157666b87a))
- **wire** — Kimi and kiro join the doors · the estate learns the index lesson ([155bd53b6](https://github.com/supernovae-st/nika/commit/155bd53b6ea36b152ec237b12c62c5f77caa351c))
- **wire** — Copilot and amp complete the wave · shapes from the clients' own writers ([837671c86](https://github.com/supernovae-st/nika/commit/837671c86150550748afc6811b8164837f9c6292))
- **workspace** — The corpus is indexed by what it teaches, not by an invented tier ([6988af52f](https://github.com/supernovae-st/nika/commit/6988af52f71217c63900e4657a9172a8892638c0))

### 🐛 Bug Fixes
- **agent,infer** — Fail closed when a priced backend omits token usage (R3-F1) ([aa669fd2e](https://github.com/supernovae-st/nika/commit/aa669fd2e351fe5deeca57d7d3fa85ac78737d74))
- **audit** — Wave A — the general adversarial audit's law+security findings ([#764](https://github.com/supernovae-st/nika/issues/764)) ([a12deff69](https://github.com/supernovae-st/nika/commit/a12deff696f0d51185174f0d3906490692737151)) ([#764](https://github.com/supernovae-st/nika/pull/764))
- **builtin** — Shadow jaq's scan to the jq-global semantics ([7fa3e535c](https://github.com/supernovae-st/nika/commit/7fa3e535c92970b02989731f9b50f74626962227))
- **check** — Two verdict lines stop claiming more than they cover ([7fc837571](https://github.com/supernovae-st/nika/commit/7fc83757169f6c63d47ee5af847a09cf176d2a77))
- **check** — --infer-permits stops contradicting the check that reads it ([e624bc452](https://github.com/supernovae-st/nika/commit/e624bc452f665306d1ed799839f279d544e3f4d1))
- **check** — Recover the decidable conjunct the dynamic argument was taking with it ([d8ae2d933](https://github.com/supernovae-st/nika/commit/d8ae2d933f7a55dce2e298e60f64aca95eabb418))
- **check** — The hint and the cost line narrow to what they cover ([c3fc10256](https://github.com/supernovae-st/nika/commit/c3fc10256ecaf31429548bf67e3d7d4d844ac00a))
- **check** — Price composed children into the cost ceiling with call multipliers ([b8eba5125](https://github.com/supernovae-st/nika/commit/b8eba5125ccd1d80950af75f90de7d03d2cfb3a5))
- **check** — The secrets fix ladder names the layer that failed (R4-F1) ([4b8e815a2](https://github.com/supernovae-st/nika/commit/4b8e815a24905532d85400c5bf7ee44605864111))
- **check** — The exec net-fit judges literal argv URLs like an invoke (D1) ([f318566d5](https://github.com/supernovae-st/nika/commit/f318566d5b75a937a52db197556c6cc8af0a6b92))
- **check** — The types verdict names its unshaped-output blind spot, aliases included (F3) ([f8f2326e5](https://github.com/supernovae-st/nika/commit/f8f2326e5c170064e425b74c79545d34c874df36))
- **check** — The coded VAR-003 walk adopts the locked strict-binding law ([bde2ae3cc](https://github.com/supernovae-st/nika/commit/bde2ae3cc0ab9e4cea5b1f4c82a4a097bd6e98be))
- **check** — Retried non-idempotent builtins earn the retry-effects hint ([4861589ad](https://github.com/supernovae-st/nika/commit/4861589adcba3f44f20f431033aa3edac90dca96))
- **ci** — The two lanes meet · the gate fixture obeys the law it crossed ([549719e18](https://github.com/supernovae-st/nika/commit/549719e18eaa966ccb64b396273c7002e208746d))
- **cli** — Claim neither bound in unbounded cost renders ([de4c459ca](https://github.com/supernovae-st/nika/commit/de4c459ca4bf1e3849148a7bad46d1d7f068487c))
- **dap** — Refuse a required anchor on an unsealed journal ([73e3af6d3](https://github.com/supernovae-st/nika/commit/73e3af6d303eebd7b7fb1d718aad240cde9dc564))
- **dap** — A seal with lines after it is tampering, and an absent key is not forgery ([9a16ecd70](https://github.com/supernovae-st/nika/commit/9a16ecd70796a46dc9acefafe67fa86da0061018))
- **dap** — A truncated inventory never renders as zero ([a5592f07c](https://github.com/supernovae-st/nika/commit/a5592f07c177625e81cd234801a5a6bd3dde95dd))
- **dap,cli** — The lot-1 review's fixpack (terminal hygiene · fuzz ci · journal bound · otel class) ([#740](https://github.com/supernovae-st/nika/issues/740)) ([9118b1f99](https://github.com/supernovae-st/nika/commit/9118b1f9987151dd7205b6c0bc6fb689e28e3670)) ([#740](https://github.com/supernovae-st/nika/pull/740))
- **dco** — Exempt bot-authored commits by author name too ([770b8d412](https://github.com/supernovae-st/nika/commit/770b8d41206e29a282db1498de0a4d7fa098527c))
- **docs,tests** — The spell gate passes (plural-of-child repaired · ATTACH-ed) ([#759](https://github.com/supernovae-st/nika/issues/759)) ([f19e9c928](https://github.com/supernovae-st/nika/commit/f19e9c9286117f02f32ff26726f3361c38ca533e)) ([#759](https://github.com/supernovae-st/nika/pull/759))
- **drift** — Model glob walk roots and multipart parts as reads; exec poisons fs sets ([34f248180](https://github.com/supernovae-st/nika/commit/34f24818010d734463ccc22f7c9fdd30808c7d55))
- **drift** — The drift pass counts exec argv urls like the net-fit (D1 coherence) ([3f56c5f19](https://github.com/supernovae-st/nika/commit/3f56c5f19ea9d30b6990437ce311418037bbdee9))
- **estate** — The mirror gate watches pushes, not only pull requests ([2f1f2e277](https://github.com/supernovae-st/nika/commit/2f1f2e277f4db1a75b749fe8f2edc250510a7eaa))
- **estate** — The mirror carries the mode, and the rules stop tripping the linter ([c7fdcd71c](https://github.com/supernovae-st/nika/commit/c7fdcd71c7094403faa96446e2c96f587e46d194))
- **estate** — The noqa actually ships ([93b503b73](https://github.com/supernovae-st/nika/commit/93b503b738013ef80c47c64f16dea3138f8f83ff))
- **exec** — The eval floor parses per interpreter, never per prefix ([b54e95a17](https://github.com/supernovae-st/nika/commit/b54e95a176572a8f5d364b295f3c1b310a37ebd0))
- **hooks** — The edit hook names the oracle that answered ([778664fea](https://github.com/supernovae-st/nika/commit/778664feaa12ab430fb96620fb0c1d8ddc9102e7))
- **hooks** — The path is the oracle identity, the version is only a tag ([ce6ab031a](https://github.com/supernovae-st/nika/commit/ce6ab031a4146f02cb6c65c7ed8adb35956a0834))
- **hooks** — Derive commit scopes from the crate list, and stop citing a file that does not exist ([b3f50713b](https://github.com/supernovae-st/nika/commit/b3f50713b7a0c8f383d39c91caa04175c739e42c))
- **hooks** — Check-on-edit judges with the tree build by default, not the PATH (F14) ([50bd19f5d](https://github.com/supernovae-st/nika/commit/50bd19f5d333fda070370030e120ff1f31af3cfc))
- **hooks** — Derive the script surfaces too, since the gate named its own gap ([48a90712d](https://github.com/supernovae-st/nika/commit/48a90712d76d4cc72a86a60a53a38af3503c9ff6))
- **hygiene** — Vector 9 tracks the living city, not a frozen snapshot of it ([23113342b](https://github.com/supernovae-st/nika/commit/23113342b06366b23233188b8119e236b39ff131))
- **hygiene** — The lan seam declares its bypass · rustdoc links qualify ([5e6d26e45](https://github.com/supernovae-st/nika/commit/5e6d26e4555f22bd31f93d003fce1dc2f742799a))
- **kit** — Session-context never inherits the process cwd ([4405ba678](https://github.com/supernovae-st/nika/commit/4405ba67829797d6765f2c0a72c0517ec4e373e7))
- **media** — The fanout capture pointed at a path the pack flattened away ([b406174e1](https://github.com/supernovae-st/nika/commit/b406174e1d5f447b255af6fbc5bb704d64925897))
- **nika-cli** — The energy rung stops inventing an iteration, and scopes get subtotals ([419488b4d](https://github.com/supernovae-st/nika/commit/419488b4dac76a751650710269d438222a64cd42))
- **nika-cli** — The MODELS rung stops reading a template as a model id ([69c402333](https://github.com/supernovae-st/nika/commit/69c40233318d5ad35c62de2b8f8213845e62adc7))
- **nika-cli** — A human gate can no longer kill a first run ([#771](https://github.com/supernovae-st/nika/issues/771)) ([c6a2ac0cc](https://github.com/supernovae-st/nika/commit/c6a2ac0cc4b91d2451d63b0927b0a29c78510652)) ([#771](https://github.com/supernovae-st/nika/pull/771))
- **nika-runtime** — The floor fixture gains its NEP-0002 gate — the e2e twin moved, the copy did not ([2a2efcc0b](https://github.com/supernovae-st/nika/commit/2a2efcc0b2d5a3a87cc09659c7d39b1ccd316cb7))
- **onboard** — An invalid answer is said and re-asked, never defaulted ([4542a616f](https://github.com/supernovae-st/nika/commit/4542a616f145146da9fe133f6978934ea3942096))
- **pack** — The fold doubled two permits blocks in the mirror ([d8b8ef18f](https://github.com/supernovae-st/nika/commit/d8b8ef18f48a8c3e9724048d865160e5aff3b42e))
- **pack** — The canon counts and the manifest hashes tell the truth ([#758](https://github.com/supernovae-st/nika/issues/758)) ([5176f03ce](https://github.com/supernovae-st/nika/commit/5176f03ce1f5620f38058fe8b7621740982ebfc5)) ([#758](https://github.com/supernovae-st/nika/pull/758))
- **permits** — A declared fs boundary stops granting more than it declares ([0e73adc28](https://github.com/supernovae-st/nika/commit/0e73adc28206c8f2e167e8fb3afa0bbe054f427f))
- **release** — The immutable-sha checkout learns its one tag ([79bcf6e66](https://github.com/supernovae-st/nika/commit/79bcf6e66a5273c704a90a756fbd2a4253f5c5e2)) ([#738](https://github.com/supernovae-st/nika/pull/738))
- **run** — --answer pre-seeds the gates of a fresh run, no --resume needed (F4) ([ed873e3b2](https://github.com/supernovae-st/nika/commit/ed873e3b27f0d67da73bd7ab79a12589b587d055))
- **runtime** — Refuse a floor-above-budget launch at admission (NIKA-1709) ([c6c52ae0f](https://github.com/supernovae-st/nika/commit/c6c52ae0ff8f7df51f30f59c2432e6f74c96d0c2))
- **runtime** — An edited child no longer cache-hits its caller ([f86fbde07](https://github.com/supernovae-st/nika/commit/f86fbde07b38a929d9d5713b244ac8751e5065e0))
- **runtime** — The scrub redacts secrets of any length by provenance ([37f6c1716](https://github.com/supernovae-st/nika/commit/37f6c17163d62ac562fac50a3b6c9e6268e02cb3))
- **sandbox** — Admit sqlite journal sidecars and cwd/parent listings on exact-file grants ([3c767ca20](https://github.com/supernovae-st/nika/commit/3c767ca2005c6285fdb3d548a15734581770168b))
- **store,dap,event** — The memory substrate carries its own bounds (audit wave B) ([#765](https://github.com/supernovae-st/nika/issues/765)) ([4d0a9bee1](https://github.com/supernovae-st/nika/commit/4d0a9bee1405b3e966182d1348ba5e51e6e170a0)) ([#765](https://github.com/supernovae-st/nika/pull/765))
- **trifecta** — The exec verb is born-ingress, and notify's default channel is net (F2) ([b9901cad5](https://github.com/supernovae-st/nika/commit/b9901cad50c3ee4d1052e271cc1eebaa6658dfc0))
- **trifecta** — The file channel arms on arrived taint, never on born output ([fc0b69c15](https://github.com/supernovae-st/nika/commit/fc0b69c15cc1c17014105ab578dc94ad75c1782b))
- **ux** — The gauntlet's six wounds close · the taught chain runs green end to end ([ab9b5978c](https://github.com/supernovae-st/nika/commit/ab9b5978c0830322f2d8d73c14fbbac4770a21c9))

### 🔨 Refactors
- **catalog** — The entry validator splits at its facts seam ([34b17a825](https://github.com/supernovae-st/nika/commit/34b17a825e723309eccd26e7bb3509246d44b735))
- **check** — The permit-fit batteries move beside their module ([7ae63c0a5](https://github.com/supernovae-st/nika/commit/7ae63c0a552a1dc4e1939f76e42d4992b35923b4))
- **check** — The two over-limit judges split at their seams ([b02b8c3f2](https://github.com/supernovae-st/nika/commit/b02b8c3f2ba16a61392bad016a21528c6bca349f))
- **check** — The one-voice filter gets a name and its own function ([6209d6ca9](https://github.com/supernovae-st/nika/commit/6209d6ca96f4c70640cc8c311ed0178f7d84ec33))
- **cli** — The run verb's tests move to their own file ([0ce6cbdfa](https://github.com/supernovae-st/nika/commit/0ce6cbdfaacd7fc9023e8fab0e5aa0272c7a6bd2))
- **cli** — The resume surface descends to its own module, the loc cap holds ([1057859fd](https://github.com/supernovae-st/nika/commit/1057859fd93523d37b2fdf4a4920dece594100e6))
- **display** — Descend the check render and the dag art to nika-display ([e11558493](https://github.com/supernovae-st/nika/commit/e11558493c5a279d1985fd18234b96232864fdc3))
- **display** — The rung claims descend to their own module, the loc cap holds ([85a0adad5](https://github.com/supernovae-st/nika/commit/85a0adad5e0c5e69d46c51a7b4a0af95bfcc916a))
- **runtime** — Bring run and the verify tier under the 100-line cap ([7208a21f3](https://github.com/supernovae-st/nika/commit/7208a21f31802ac91d61dadf32e39b0fffd90a50))
- **runtime,dap** — Split two fns at the 100-line law (fn-length ratchet) ([fcddd872e](https://github.com/supernovae-st/nika/commit/fcddd872e3fe14227a67a4064630e4cf4eb24ef9))

### 📚 Documentation
- **adr** — Adr-109 — the composition receipt, condition by condition ([1c4064156](https://github.com/supernovae-st/nika/commit/1c4064156bca57dcdee2a235cf195fbe29d2f1be))
- **adr** — Re-issue the composition receipt — 9 rows, two at a named lower tier ([66be812c9](https://github.com/supernovae-st/nika/commit/66be812c98d158add8da6cab9f7e093d91da77ba))
- **audit** — Run 5's four regressions fixed after the operator's decisions ([616a4d77a](https://github.com/supernovae-st/nika/commit/616a4d77a425c08523c8b0a9b99ce92fc16eb70d))
- **audit** — The next session's checkpoint — run 5 domain, traps, and the queue ([d6f8b863a](https://github.com/supernovae-st/nika/commit/d6f8b863a08c81cd08fac318bf9ef234ea892fd1))
- **catalog** — The deepseek three-way resolves at the rendered page ([0a8fbf746](https://github.com/supernovae-st/nika/commit/0a8fbf746869bc7a702b014f2928022b0f535179))
- **check** — The two laws move to where a check is written and reviewed ([d9e4d43e6](https://github.com/supernovae-st/nika/commit/d9e4d43e666a3a029435961883c0a30cc7830d35))
- **honesty** — The surfaces catch up with the proven behavior ([f671f1cf7](https://github.com/supernovae-st/nika/commit/f671f1cf7efd49e443fdf26f1754b0e459543843))
- **host** — Three links to private items become plain code text ([8cd4bc729](https://github.com/supernovae-st/nika/commit/8cd4bc72942aba955a6d3f258e8363e588c7431c))
- **kit** — The delegation rule routes health to the doctor ([3d05ce215](https://github.com/supernovae-st/nika/commit/3d05ce215ed0c8e954667990aa23dbd90b07289d))
- **media** — Every drawn frame speaks the released grammar ([2a273cc32](https://github.com/supernovae-st/nika/commit/2a273cc325e2673872b52e59b86c176731aaf1fd))
- **media** — The last two stale paintings catch up with their scenes ([bdc7d3117](https://github.com/supernovae-st/nika/commit/bdc7d31175ab15eb4e8c2d84b4d5095af064ac50))
- **media** — The readme's taught slugs face the same executioner ([c16d4e891](https://github.com/supernovae-st/nika/commit/c16d4e891eee1f9dcbd484714034ff4c7de880c5))
- **media** — The renderer refuses to paint outside the frame ([8be8cc8db](https://github.com/supernovae-st/nika/commit/8be8cc8dbfaae12314d80bc5c2828b11ee25c184))
- **nika-cli** — The fixture doc catches up with its gated shape ([763439161](https://github.com/supernovae-st/nika/commit/763439161c6a3943e2860c7541ba2c8e6d19e638))
- **plans** — The cost ceiling covers half the bill, and four forks get their reasons ([27f3f4ec8](https://github.com/supernovae-st/nika/commit/27f3f4ec83e564dc7bf33318f00441ba4d60786f))
- **plans** — The contradictory-advice pair closed with the matcher ([1513d2f48](https://github.com/supernovae-st/nika/commit/1513d2f48583a0faaa9d2b62db574de7ccef7d96))
- **plans** — Sec-009 keeps its semantics, its message stops hiding the approximation ([ace795ae9](https://github.com/supernovae-st/nika/commit/ace795ae9aa25aa9da8319912c56efb96da33b8f))
- **plans** — The resume wedge does not reproduce, and that is worth recording ([c3c293852](https://github.com/supernovae-st/nika/commit/c3c2938526bfdab7cb7a22193c2f3725bd8e81c6))
- **plans** — The arc record separates shipped from written ([0e1b9bee0](https://github.com/supernovae-st/nika/commit/0e1b9bee0cb47da9375fe32aee3d0ace112c20ea))
- **plans** — The adversarial audit run 2 and the handoff closure ([eb2342054](https://github.com/supernovae-st/nika/commit/eb2342054fd6b3a220c3599b3fa309638445a422))
- **plans** — The adversarial audit run 3 — agent-loop budget + MCP pricing ([00e81c55b](https://github.com/supernovae-st/nika/commit/00e81c55b9e2eb298edf208e542984836cb17e6e))
- **readme** — The repo names its place in the city ([5fb78af4c](https://github.com/supernovae-st/nika/commit/5fb78af4c2cde55aecb1c381b570022d6fd7668f))
- **readme** — The island links every building ([56032f47d](https://github.com/supernovae-st/nika/commit/56032f47dd8fb7c6a72a783d2575dbb372836ea3))
- **readme** — The city island names which root this building carries ([45fe0d31a](https://github.com/supernovae-st/nika/commit/45fe0d31afdcb1a9c1320ee6598ccd7e3596ff34))
- **readme** — The city gains its ci district and loses its witness ([f53df4c42](https://github.com/supernovae-st/nika/commit/f53df4c425ddcd96abb20bdf79c83210aba9e850))
- **roadmap** — The skill row points at the living repo, not a dead link ([e5382e231](https://github.com/supernovae-st/nika/commit/e5382e2318ebeb405d536780f6f541bbf9e60947))
- **trace** — The verify help promised a taxonomy the verb had outgrown ([5488d2f3c](https://github.com/supernovae-st/nika/commit/5488d2f3c18ea180dbc4c41630aa7d403cc62b14))

### 🧪 Tests
- **check** — The integration twin follows the lock, and typos stops reading French ([be8ccad00](https://github.com/supernovae-st/nika/commit/be8ccad008aadcf13027f1a0c907ccd50c39b43d))
- **cli** — The check-run oracle rides the corpus ([fa08fce9b](https://github.com/supernovae-st/nika/commit/fa08fce9b84fd048b6d4aab3d1c25ca534be64e9))
- **cli** — The corpus coverage ratchet holds both axes ([92c9fef2e](https://github.com/supernovae-st/nika/commit/92c9fef2e868410f0b78d2d132ffdb523c76e593))
- **cli** — The two builtin roots meet their consumer-side gate ([97b6e435e](https://github.com/supernovae-st/nika/commit/97b6e435efe6eee460edf471dd6b7a202fde2384))
- **cli** — The ratchets absorb the reviewer's mutations ([8d3d1179d](https://github.com/supernovae-st/nika/commit/8d3d1179d5d6140af5cc94e2150bd01120132a12))
- **dap** — The rust-python differential renders one verdict (nep-0012 law 4) ([#741](https://github.com/supernovae-st/nika/issues/741)) ([40be14609](https://github.com/supernovae-st/nika/commit/40be1460946a70fbfd8f51dbaa203d6a2afa9af0)) ([#741](https://github.com/supernovae-st/nika/pull/741))
- **nika-cli** — The child's floor bounds the parent budget — adr-109 condition 4 closes ([01e45f580](https://github.com/supernovae-st/nika/commit/01e45f58095a66491838835a79807590e06e1c80))
- **pack** — The vendored canon's 17 must resolve in the shipped catalog ([3e32fd4e0](https://github.com/supernovae-st/nika/commit/3e32fd4e0efaa7807b8bc6cdbf3e8bd6577e536b))

## [0.106.1](https://github.com/supernovae-st/nika/compare/v0.106.0..v0.106.1) - 2026-07-28

**The browser release.** The checker takes the seat the spec reserved for it
(07-conformance Level 2 · « custom engines for specialized environments »):
the parse + conformance half of `nika check` compiled to WebAssembly, admitted
through the full 12-gate ceremony (ADR-107) — including a three-leg adversarial
review that found a real engine bug before any browser saw it (a jq nesting
bomb that killed the checking instance for good; the guard now refuses at
depth 128 on every surface). The artifact debuts on npm as
`@supernovae-st/nika-check-wasm`, packed by this release train itself:
manifest projected from cargo metadata, host paths remapped out of the bytes,
the tarball attached to this release + attested whether or not the registry
credential exists. In-band honesty rides every verdict — `wasm: true` and a
closed `legs:` list, so the browser half can never be mistaken for the full
binary. Around it the ladder keeps tightening: `when:`/`for_each:` stop
admitting the loop locals (the runtime pin collected its own documented
sunset), and the audited card stops calling a ceiling a floor.

### ✨ Features
- **catalog** — The pricing provenance is produced by the engine it feeds ([a30080600](https://github.com/supernovae-st/nika/commit/a300806002c5aea4bfe1990105275ba3cc16025f))
- **check** — A prompt with no default names its headless cost ([acdb032ab](https://github.com/supernovae-st/nika/commit/acdb032abc352f8fb561660a2600e5a4c48409a4))
- **check** — A structured capture nobody branches on names what it swallows ([5ea8cf2dd](https://github.com/supernovae-st/nika/commit/5ea8cf2dd02f90cb176dc68e214b623d9fc70d62))
- **check-wasm** — Admit the browser half of nika check (ADR-107 · 12 gates) ([3a8e2e843](https://github.com/supernovae-st/nika/commit/3a8e2e84369e3028d9b6221d77090dc628c81d7f))
- **cli** — Nika lsp accepts the host convention flags as no-ops ([a3c4950e2](https://github.com/supernovae-st/nika/commit/a3c4950e251d5047987592a312d32617d6751f28))
- **mcp** — The five slash commands reach every client, not three ([e7f1f5284](https://github.com/supernovae-st/nika/commit/e7f1f5284f8ef3a18d22d4079937a47dde87513b))
- **mcp** — The agent-facing oracle stops handing back a green the run gate refuses ([62ba2406e](https://github.com/supernovae-st/nika/commit/62ba2406eca3c87b332ee1e6615180198dc1af80))
- **plugin** — The codex manifest carries its public listing card ([56d6dc3e5](https://github.com/supernovae-st/nika/commit/56d6dc3e57175bbb7a9cb367781125d3b1d53766))

### 🐛 Bug Fixes
- **check** — `when:` and `for_each:` stop admitting the loop locals ([f7b680366](https://github.com/supernovae-st/nika/commit/f7b680366eea1025c9f0fd0b18405b775d14cbf9))
- **check** — Every surface that judges for the author judges like the run gate ([74d230254](https://github.com/supernovae-st/nika/commit/74d230254f2641957456c8d582ff3e13c2d5f2d4))
- **check** — The audited card stops calling a ceiling a floor ([a77078707](https://github.com/supernovae-st/nika/commit/a770787070a186033c023bf7431fc6568bae41b4))
- **kit** — The plugin taught a syntax the engine refuses ([0b362db42](https://github.com/supernovae-st/nika/commit/0b362db42fd42cd9d41865f3ff514223a0399c22))
- **mcp** — The read-only oracle says so on the wire ([a6bdf3e54](https://github.com/supernovae-st/nika/commit/a6bdf3e54b88c4322690f88740c9f6c7f07d336b))

### 🔨 Refactors
- **check** — Scan_task stops building its contexts by hand ([16bb12b97](https://github.com/supernovae-st/nika/commit/16bb12b97a191e4a32dc4db28d4266cc95b4652c))
- **check** — The traversal half of the hint lane earns its own file ([12add44a3](https://github.com/supernovae-st/nika/commit/12add44a31474f0f94f0badb7599834877a92dc9))

### 📚 Documentation
- **kit** — The kit teaches the whole language, not a third of it ([a94360f2c](https://github.com/supernovae-st/nika/commit/a94360f2c644171e544d540a4d5e4ad3ca53d1f9))
- **kit** — Composition was a whole chapter the kit never mentioned ([9cc9d6002](https://github.com/supernovae-st/nika/commit/9cc9d60025091b5133c331b01f442b473df7c42f))
- **kit** — The native-first law gains the recipe it was missing ([0b5040ca1](https://github.com/supernovae-st/nika/commit/0b5040ca196260088f9a0bcf1ef3f2e4da0c525f))
- **plans** — The false-green class, six findings and their law ([2aaa80881](https://github.com/supernovae-st/nika/commit/2aaa8088102750163945a1855889efc529ffe4dc))
- **plans** — The resource algebra — what a workflow costs and what check can prove ([71c67da3f](https://github.com/supernovae-st/nika/commit/71c67da3fdbda0e238737a49888a04a340f5a511))
- **plans** — The cost ceiling is unsound on agent loops, and the reason is a theorem ([1ca813fbb](https://github.com/supernovae-st/nika/commit/1ca813fbb1625201fa11a72cbfd59295673c74f3))
- **plans** — The resource-vector research corrects four of my own rows ([a7cd93b28](https://github.com/supernovae-st/nika/commit/a7cd93b2863ea6b820067fb320c5f2261e73eaad))
- **plans** — The DAG width is measured, and it closes the hypothesis T1 rests on ([9b172979e](https://github.com/supernovae-st/nika/commit/9b172979e0fe5de9601f970ad6aa95bef92542d8))
- **plans** — An adversarial pass killed 17 of 33 claims — the corrections lead the document ([b40839d16](https://github.com/supernovae-st/nika/commit/b40839d16ea91f38b81da451ffb7cd76d3ed5aba))

### 🧪 Tests
- **check** — The ledger fixture goes on one line, and says why ([14a14b1d5](https://github.com/supernovae-st/nika/commit/14a14b1d52d43c045aa2d46c9dc25147f135ebf3))

## [0.106.0](https://github.com/supernovae-st/nika/compare/v0.105.0..v0.106.0) - 2026-07-27

**The authority release.** Two flag-days land in the same window, and both
change what an existing file MEANS. The envelope's value forms split into the
four authorities — `vars:` and `env:` are dead, `inputs:` · `config:` ·
`const:` · `secrets:` are the whole family — and a `permits:` block stops
being optional: absent is now the EMPTY boundary, not the unconfined floor.
Around them the attestation lane arrives whole: a run declares its entropy and
its clock, boots from a manifest, seals what it covered, signs, anchors to a
public transparency log, and exports a pack an auditor reads without trusting
us. This is the first release where a workflow written yesterday can refuse to
check today. The migration is the front page, not a footnote — `nika check
--fix` applies the mechanical half and STOPS untouched rather than guess at
the rest.

### ⚠️ Migration

#### 1 · The value authorities — `vars:` and `env:` are dead (R3a « the E-split » · #669)

The law is **classify-not-rename**: every old entry moves to the authority its
ROLE commands, never to one bulk destination.

| Dead form | Classify it as | Refusal |
|---|---|---|
| `vars:` entry · a typed parameter a caller supplies | `inputs:` (typed · `required:` · `default:`) | `NIKA-VALUES-001` |
| `vars:` entry · a fixed value baked into the file | `const:` (bare literal, or `{ type, value }`) | `NIKA-VALUES-001` |
| `env:` entry · non-sensitive runtime configuration | `config:` (typed · deployment-supplied) | `NIKA-VALUES-002` |
| `env:` entry · a credential | `secrets:` (a store reference · never a literal) | `NIKA-VALUES-002` |
| `env:` entry · a name a child process must see | `permits: { env: [NAME] }` (exact names) | `NIKA-VALUES-002` |
| `${{ vars.X }}` · `${{ env.X }}` | the authority the role commands | `-001` · `-002` |
| `${{ anything_else.X }}` | the family is closed: inputs · config · const · secrets | `NIKA-VALUES-003` |

`nika check --fix` migrates the `vars:` half mechanically — comment-preserving,
idempotent, and class-aware (the name→class map comes from the file's own
block, never a blind rename). It leaves the file UNTOUCHED and names the reason
on a credential-shaped name, a typed-only declaration, a flow-style
`vars: {…}` header, a `required: true` entry without `type:`, or an empty
block: atomic-or-nothing, the codemod never guesses. **`env:` has no
mechanical repair** — re-shaping a flat string map into typed `config:`
declarations is a human classification, and the teaching says so at the point
of refusal.

Two consequences worth reading twice · `config:` resolves ONLY against the
declared block, so the engine never silently falls back to the OS environment
(every value a workflow depends on is visible in the file); and `--var` now
names `inputs:` — the flag keeps its spelling, its target moved.

#### 2 · The boundary — absent `permits:` is zero authority (F-O8 · NEP-0003 · #691)

A missing `permits:` block used to be the unconfined floor. It is now the
EMPTY boundary: a body carrying any effect with no block refuses
`NIKA-AUTH-006` at check — before a token is spent — and the runtime gates
refuse before any spawn. Only a pure-compute body stays clean, and it gets a
hint teaching `permits: {}`, the legal zero. The 39 embedded workflows that
shipped with this repo — examples, showcase tiers, templates, battery files —
each had to declare its inferred block: that is the blast radius, measured
rather than guessed. The repair is one command —
`nika check --infer-permits` prints the tightest block, and the round-trip law
holds (the inferred block re-checks clean).

Four more refusals ride the same window. Each was previously admitted:

| Refusal | What it now refuses |
|---|---|
| `NIKA-AUTH-007` | an interpolation reaching a permit BOUND (host · glob · program · env name) — a bound MUST be a literal, or the boundary is self-serve |
| `NIKA-AUTH-008` | an untrusted value whose canonical resolved form escapes the step's permit (the static twin of the runtime re-gate) |
| `NIKA-AUTH-009` | a `permits: env:` entry naming a dangerous-floor variable — the engine strips it unconditionally, so the grant is an inert dead grant |
| `NIKA-AUTH-010` | `*.example.com` in `permits.net.http` — the `*.` subdomain wildcard delegates the boundary to the zone operator (every host under the suffix, present and future). Name exact hosts, or the bare `*` when allow-all is genuinely intended |
| `NIKA-SEC-008` | a `nika:fetch` whose resolved URL path names a code-bearing class with no `inert:` door declared — the read hides an execution sink (NEP-0006) |

And one law that carries no code but changes what a child process SEES · **a
spawned child no longer inherits the engine environment**. Its environment is
COMPOSED from a cleared slate — the runner floor ∪ the names declared in
`permits: env:` ∪ the task's own `env:` map, the dangerous names stripped last
(F-O4 · NEP-0005). A workflow that relied on an ambient variable reaching an
`exec:` child must now name it.

#### 3 · The embedder cut — `nika-schema` → `nika-check` (#683 · no re-export shims)

| Before | After |
|---|---|
| `nika_schema::{analyze, AnalyzedWorkflow}` | `nika_check::{analyze, AnalyzedWorkflow}` |
| `nika_schema::{check, check_composed, infer_permits}` | `nika_check::{…}` |
| `nika_vocab::VarType` (the flat 6-enum) | the full `TypeExpr` of spec 09 |
| `NIKA-PARSE-019` (entropy × clock) | `NIKA-PARSE-026` · `-027` · `-028` |
| `NIKA-PARSE-015` (malformed typed `vars:`) | **retired · never reused** — what it refused is admitted by `TypeExpr`; what stays outside the grammar refuses `NIKA-TYPE-001` |

`nika-schema` keeps its blueprint shape (THE PARSER: AST + raw + error +
keysets). Every consumer inside this repo — cli · lsp · runtime · dap · mcp ·
verb-agent · lints · graph · display · onboard · the fuzz harness — already
points at `nika_check`; what migrates is the embedding code outside this repo.
The cut is clean by choice: a shim would have kept two names for one judgment.

The retired and re-minted codes count as breaking for anyone FILTERING on
them — a tool watching `NIKA-PARSE-019` for the entropy × clock contradiction
will never see it pass again.

#### 4 · The predicates and the type grammar (R5 · #677 · R3b · #673)

| Dead form | The respelling | Refusal |
|---|---|---|
| `after: { t: succeeded }` | `after: { t: success }` | `NIKA-DAG-005` |
| `after: { t: failed }` | `after: { t: failure }` | `NIKA-DAG-005` |
| `type: boolean` | `type: bool` — the one boolean spelling, no alias | `NIKA-TYPE-001` |
| bare `type: array` · `type: object` | `{ array: T }` · `{ object: { … } }` | `NIKA-TYPE-001` |
| a `default:` that does not fit its own `type:` | fix the value, or the type | `NIKA-DEFAULT-001` |

`skipped` and `terminal` are unchanged. `nika check --fix` applies the
predicate respelling 1:1 — flow and block `after:` forms, quoted values,
comments preserved, idempotent; a `when:` status comparison is the
`NIKA-DAG-007` class and is never touched. `NIKA-DEFAULT-001` closes a real
soundness hole: a declared default that could never satisfy its own type used
to sail through.

### Added

- **`nika-check`** — the static judgment crate (new L0 member · split from
  `nika-schema` at the 15k crate-size wall; the nika-graph/nika-dap
  precedents): the workflow analyzer (`analyze` · `AnalyzedWorkflow` · the ONE
  derived-DAG-edge computation every surface projects) plus the whole `nika
  check` ladder (`check` · `check_composed` · cost ceiling · secret-leak IFC ·
  capability-escape fit · trifecta · policy · gate reachability · the
  `RunCertificate` · `infer_permits`). Static judgment without the CLI, for
  the embedder/SDK surface (§Migration 3 for the import map).

- **`run:` — the run declares its entropy and its clock** (F-P3 · NEP-0010 ·
  #711) — a new envelope block with two keys: `entropy:` (`ambient` · `none` ·
  `{ seeded: <n> }`) and `clock:` (`system` · `virtual`). A declared pair that
  contradicts itself refuses at parse (`NIKA-PARSE-026` ambient × virtual ·
  `NIKA-PARSE-027` none|seeded × system), and an `entropy: none` that
  nevertheless consumes a structural randomness source refuses at check
  (`NIKA-PARSE-028`). `ambient` is the default when `run:` is absent — the
  honest status quo, so nothing that ran before changes. Determinism becomes a
  declaration the engine holds you to, not a hope.

- **The run's lifecycle is attested** (F-P2 · NEP-0011 draft · #718) — the
  `workflow_started` prologue becomes a boot manifest (`spec_pin` ·
  `stamper_kind` · the resolved `clock` · `seed` under a determinism demand);
  the run seal's `covers` extends additively with the folded `receipt_digest`,
  the consumed budgets and the exercised effects (the classic four-field seal
  stays byte-identical); a journal that never reached a lifecycle-terminal
  frame verifies **INCOMPLETE** — the verifier's finding, never the dying
  run's silence; and a check report stamped with a different semantic hash
  than the booting workflow refuses before the first event (the
  judged-vs-booted binding · exit 2 · semantic grain).

- **The receipt fortress — bounds are code** (F-P1 · NEP-0012 draft · #721) —
  the verifier is the one component guaranteed to parse attacker-supplied
  bytes, so its bounds became named constants on every profile: 1 MiB per
  document, a string-aware pre-parse depth scan at 32, proof-bearing arrays at
  64, identifier strings at 256. The chain walk refuses an over-long line
  BEFORE the parse (the new `LineOverLong` verdict, rendered as a FILE refusal
  by `trace verify` and an honest refusal by the evidence pack), and the
  anchor sidecar loads through the same door.

- **`nika sign` · `nika key` · `run --require-signature`** (S3 · #660 · #655) —
  workflow author-binding. `nika sign <file>` mints `<file>.minisig`
  (`--check` verifies); `nika key` is the run-signing key lifecycle (mint ·
  TOFU fingerprint · rotate — old public halves stay verifiable); `nika run
  --require-signature` refuses an unsigned or invalidly-signed workflow at
  exit 2. Runs emit a signed `run_sealed` event under run-key custody.

- **`nika trace anchor`** (#665) — notarize the journal head OUTSIDE the
  journal: the post-seal head, signed with the run key, submitted to the
  public Sigstore Rekor v2 transparency log plus an RFC 3161 timestamp,
  writing a detached `<trace>.anchor.json` sidecar. An explicit NETWORK act —
  this verb IS the opt-in. `nika trace verify` now climbs a four-tier ladder
  and reports the highest honestly-attained tier: chain OK · **SEALED** (the
  `run_sealed` signature verifies against a custody key) · **ANCHORED** (the
  sidecar verifies fully offline) · **REPLAYED** (`--replay` compares a fresh
  run; verify never re-executes).

- **`nika evidence`** (#662) — export the evidence pack for one run: journal +
  manifest + receipt + a `VERIFY.md` that tells an auditor what to run.

- **The permit-decision witness** (F-O6 · NEP-0007 · #701) — every permit
  decision is recorded in the journal, granted and refused alike, so
  `trace verify` can judge the boundary a run actually rode; and the
  check ⇔ run equivalence oracle proves the two agree. `trace_format` stays 2
  (the witness rides existing frames · no wire bump).

- **MCP tool pinning — TOFU + fail-closed drift** (#657) — a configured MCP
  server that changes its tool definitions after you approved them is the rug
  pull. Per-tool pins (blake3 over a domain-separated canonical pre-image)
  land in `.nika/mcp_pins.json` (`mcp_pins_format: 1`) beside a reviewable
  snapshot; first contact enrolls loudly, a match proceeds silently, ANY drift
  fails closed with a diff naming the CHANGED field and returns no tools. A
  hand-edited lockfile is `NIKA-MCP-004`, never a silent re-TOFU.
  **`nika mcp approve <server>`** re-pins after human review.

- **The exec sandbox is wired** (ADR-095 L6 · #642) — `permits:` jails the
  child (seatbelt on macOS · landlock/bubblewrap on Linux), the
  `SandboxSpec` network arm becomes a tri-state (#658), MCP stdio servers are
  spawned inside the same confinement (#667), and the **loopback egress
  proxy** makes the sandbox's network arm the exact projection of
  `permits.net.http` (#663 · #706 · F-P5 · NEP-0008).

- **Two new task fields** — `inert:` (the declared door for a `nika:fetch`
  whose payload is data, never code · the `NIKA-SEC-008` sanction) and
  `declassify:` (`from` · `to` · `because` · the audited secret-flow
  sanction).

- **Registry client v0.2 signatures** (#648) — minisign + TOFU on registry
  pulls (`NIKA-REG-006` · `NIKA-REG-007`).

- **`NIKA-DRIFT-001` — declared-but-unused hints in `check`** (#661) — a
  declared name or a `permits:` entry nothing in the body references. Advisory:
  it never fails the audit (the reverse direction, used-but-undeclared, is the
  hard refusal surface).

- **Two new journal event kinds** — `declassify` and `run_sealed`. Additive:
  `trace_format` stays 2, but a consumer matching the kind vocabulary
  exhaustively must learn them.

- **Spec 17 · trace** — the journal dialect becomes normative in the embedded
  pack (the NDJSON frame grammar chained by sha256, the prologue manifest, the
  closed kind vocabulary), and the pack ships the `law` + `registries` JSON
  schemas.

### Changed

- **`run:` declared-pair refusals ride their dedicated mints** — the
  parse-level entropy × clock contradictions stamp `NIKA-PARSE-026` (ambient ×
  virtual) and `NIKA-PARSE-027` (none | seeded × system), and the check-side
  `entropy: none` × structural-source judgment stamps `NIKA-PARSE-028` — was
  the registered generic `NIKA-PARSE-019`; the NEP-0010 mints landed with the
  87f764a spec pack resync.

- **The lethal-trifecta judge asks a better question** (trifecta v2.0 ·
  NEP-0002 · #643) — v1.1 asked « could this workflow complete the trifecta? »
  over the declared capability set. v2.0 asks « does untrusted content
  actually REACH an egress? », the integrity half of the flow lattice. The
  inversion that matters: an `infer:`/`agent:` output carries the taint when
  its prompt saw it — a summary of an attacker's page carries the payload.
  v2.0 findings ⊆ v1.1, so nothing that passed before starts failing.

- **`permits.tools` is enforced at run** (#639) — the last check-only axis
  closes. A tool call outside the boundary now refuses at dispatch, not only
  at check.

- **`NIKA-SEC-005` is core-visible — law 5 at every tier** (#708) — the
  net-egress floor was already emitted by `check`; only the core-tier
  conformance filter hid it. The core verdict now matches the reference
  oracle.

- **`nika trace verify` exit 0 means « the highest attained tier holds »** —
  the verdict is tiered, not binary (exit 2 broken or forged · exit 3
  unchained or a missing input).

- **The embedded spec pack catches up to the spec branch** — NEP-0004 through
  NEP-0012 land in the pack `nika spec` serves, along with the 6-namespace
  substitution family, the 25 error namespaces / 96 error codes, and the
  17-provider catalog count.

### Fixed

- **A planted symlink can no longer pivot an `fs:` grant** (H2 · NEP-0009 ·
  #710 · the CVE-2024-42472 class) — the mount-projection arm followed a
  symlinked bind SOURCE where the kernel path-walk arm refuses at open, so an
  upstream task that replaced `/ws/data` with a link to `$HOME/.ssh` made a
  later task's `fs.read: [/ws/data]` bind the wrong tree. Every grant's
  literal prefix is now re-judged as its EFFECTIVE path identity at dispatch,
  before the jail is built: a legitimately-symlinked ANCESTOR is absorbed, the
  final component stays lexical, and an identity that redirects outside the
  judged path is refused before spawn (`NIKA-SEC-004`) — never mounted under
  the judged name, never rewritten to the resolved form. The receipt does not
  lie.

- **A forged `CheckReport` cannot buy authority** (#656 · `NIKA-1707`) — the
  runtime re-derives the boundary subset at run start; a clean report over
  different bytes is not clean.

- **Resolved secrets are redacted from the journal, and journals are 0600**
  (#640).

- **Pulled model weights are sha256-verified against the Hub's declaration**
  (#641).

- **Security-boundary refusals never feed back to the model** (#638 ·
  `NIKA-468`) — a mid-loop refusal is the boundary's word, not another turn's
  context.

- **A `required: true` input with neither a `default:` nor a `--var` refuses
  at admission** (#674 · `NIKA-1708`) — before the DAG spends a task, not
  mid-run.

- **Key trust reads the public half only** (#668) — no decrypt on the print
  path.

- **The red team's residuals close** (#702 · #703 · #704) — four permit edge
  cases, three exec-runner residuals, decode-then-trim, and the `finally`
  attestation.

- **The taught flow bindings quote their islands** (#717) — an unquoted
  `${{ }}` in a YAML flow mapping is `NIKA-PARSE-001`; the shipped teaching
  surfaces no longer hand it to you.

## [0.105.0](https://github.com/supernovae-st/nika/compare/v0.104.0..v0.105.0) - 2026-07-20

### ✨ Features
- **authority** — W4 the authority — policy law, coded secret flows, certified effects ([#593](https://github.com/supernovae-st/nika/issues/593)) ([de5c359f2](https://github.com/supernovae-st/nika/commit/de5c359f2899b0b03ebb25d190e0660da8c87ff3)) ([#593](https://github.com/supernovae-st/nika/pull/593))
- **bench** — The w0 refonte baseline harness — slopes judged, three caught ([#578](https://github.com/supernovae-st/nika/issues/578)) ([21e72a80c](https://github.com/supernovae-st/nika/commit/21e72a80c8b69af130c86524d4e3c7086de1d75f)) ([#578](https://github.com/supernovae-st/nika/pull/578))
- **composition** — W-comp the composition — invoke: workflow:, specified + partial ([#596](https://github.com/supernovae-st/nika/issues/596)) ([0b85d5ce7](https://github.com/supernovae-st/nika/commit/0b85d5ce70c33ee85895685fa28d925dccb9e2a2)) ([#596](https://github.com/supernovae-st/nika/pull/596))
- **decision** — W-dec the decision — nika:decide, the deterministic kernel ([#594](https://github.com/supernovae-st/nika/issues/594)) ([c51a92698](https://github.com/supernovae-st/nika/commit/c51a926986bd62ba9bcce97f6826c50f5f5502af)) ([#594](https://github.com/supernovae-st/nika/pull/594))
- **lsp** — Every block teaches its own keys — the parser's keysets door ([#569](https://github.com/supernovae-st/nika/issues/569)) ([0d4221ae1](https://github.com/supernovae-st/nika/commit/0d4221ae150066eb9fe90cdf07c3840237049ba7)) ([#569](https://github.com/supernovae-st/nika/pull/569))
- **lsp** — Server-side rename — the map key renames everywhere at once ([#582](https://github.com/supernovae-st/nika/issues/582)) ([a3bbfc520](https://github.com/supernovae-st/nika/commit/a3bbfc52078231118c27288e76e493d553ef570f)) ([#582](https://github.com/supernovae-st/nika/pull/582))
- **lsp** — Semantic_document_format:1 — the oracle surface names its own version (W7) ([#598](https://github.com/supernovae-st/nika/issues/598)) ([57e997646](https://github.com/supernovae-st/nika/commit/57e9976462cef73a2019206d5ac19876ed556054)) ([#598](https://github.com/supernovae-st/nika/pull/598))
- **nika-cap** — The lethal trifecta judge — NIKA-SEC-009 (NEP-0002) ([#637](https://github.com/supernovae-st/nika/issues/637)) ([15dcc88f7](https://github.com/supernovae-st/nika/commit/15dcc88f7e6bfb5db416cc74070e5ece5d8f05e8)) ([#637](https://github.com/supernovae-st/nika/pull/637))
- **nika-lsp** — Loop-scoped roots by the law — item gated, index joins ([#576](https://github.com/supernovae-st/nika/issues/576)) ([f5db09288](https://github.com/supernovae-st/nika/commit/f5db092880973f4f95d2bbd256021c509126c86f)) ([#576](https://github.com/supernovae-st/nika/pull/576))
- **outcomes** — W5 the bounds — the Outcome IR, one table, cause on the record ([#595](https://github.com/supernovae-st/nika/issues/595)) ([aa8675e45](https://github.com/supernovae-st/nika/commit/aa8675e45e44060e21f3adb5f94703369eb1c0b2)) ([#595](https://github.com/supernovae-st/nika/pull/595))
- **proof** — W6 « la preuve » — semantic hash · nika.lock · assert: v1 · receipt_format:1 (the LAST pre-1.0 wave) ([#597](https://github.com/supernovae-st/nika/issues/597)) ([f5bb3f819](https://github.com/supernovae-st/nika/commit/f5bb3f81960d4edbacbbe6bec9a1ee0177f45dfc)) ([#597](https://github.com/supernovae-st/nika/pull/597))
- **schema** — W1 the map — workflow object + task map keyed by id ([#581](https://github.com/supernovae-st/nika/issues/581)) ([35f8604ac](https://github.com/supernovae-st/nika/commit/35f8604acf9d1e06ebcf7463d6d8c2fdb1ad21b3)) ([#581](https://github.com/supernovae-st/nika/pull/581))
- **schema** — W2 the flow — two doors, one graph, the gate names its refusal ([#588](https://github.com/supernovae-st/nika/issues/588)) ([21e515684](https://github.com/supernovae-st/nika/commit/21e51568457fcbcdf07f6601fc8ffd05924c1104)) ([#588](https://github.com/supernovae-st/nika/pull/588))
- **types** — W3 the contract — one type core, three relations, typed doors ([#592](https://github.com/supernovae-st/nika/issues/592)) ([249c996c8](https://github.com/supernovae-st/nika/commit/249c996c8d0f5c870270bc66940302b874e46ae1)) ([#592](https://github.com/supernovae-st/nika/pull/592))
- Nika-sandbox-landlock, the Linux command-sandbox twin ([#630](https://github.com/supernovae-st/nika/issues/630)) ([413133719](https://github.com/supernovae-st/nika/commit/413133719159a9d6b5e07dfc6c59e75171e1965d)) ([#630](https://github.com/supernovae-st/nika/pull/630))

### 🐛 Bug Fixes
- **ci** — The trust battery fixture speaks the map surface ([66f1d39b0](https://github.com/supernovae-st/nika/commit/66f1d39b067178568306a802ef49b3a0d84a67a3))
- **hooks** — Deletion-only pushes skip the pre-push gate ([#590](https://github.com/supernovae-st/nika/issues/590)) ([2906ffabc](https://github.com/supernovae-st/nika/commit/2906ffabcc65690af6e77a19ccd5b18e1ae5a7e5)) ([#590](https://github.com/supernovae-st/nika/pull/590))
- **kit** — The new command frontmatter parses — invalid yaml dies at the source ([#583](https://github.com/supernovae-st/nika/issues/583)) ([ff0cba5b8](https://github.com/supernovae-st/nika/commit/ff0cba5b85b73d551ba1fec2dad4928f823f20de)) ([#583](https://github.com/supernovae-st/nika/pull/583))
- **tests** — The oracle fixtures speak argv — the 565×570 merge race paid ([#573](https://github.com/supernovae-st/nika/issues/573)) ([d1458743e](https://github.com/supernovae-st/nika/commit/d1458743ee06301995d0e2dd59b86dde6f41f6ef)) ([#573](https://github.com/supernovae-st/nika/pull/573))

### ⚡ Performance
- **lsp** — The downstream walk goes one-pass — hover sheds its x20 slope ([#580](https://github.com/supernovae-st/nika/issues/580)) ([0b0be55a0](https://github.com/supernovae-st/nika/commit/0b0be55a01263a08c8f4e2b784e51280468c91be)) ([#580](https://github.com/supernovae-st/nika/pull/580))

### 📚 Documentation
- **adr** — Related reciprocity — the 4 one-way arrows answer back ([#587](https://github.com/supernovae-st/nika/issues/587)) ([da81e4f7e](https://github.com/supernovae-st/nika/commit/da81e4f7e649f9f7ee0df657a51091fa37dde4e7)) ([#587](https://github.com/supernovae-st/nika/pull/587))
- **agents** — Mcp oracle is 9 tools · fold dead verbs · exit-4 paused ([#622](https://github.com/supernovae-st/nika/issues/622)) ([22f18ab09](https://github.com/supernovae-st/nika/commit/22f18ab090f53f3068b3b6fe5ad3e99b6cbae497)) ([#622](https://github.com/supernovae-st/nika/pull/622))
- **changelog** — The union-rebase double Added folds to one ([d8feac8db](https://github.com/supernovae-st/nika/commit/d8feac8db0d3bd4010295fd7695b2afea2b06449))
- **changelog** — Append v0.104.0 — auto-generated ([#604](https://github.com/supernovae-st/nika/issues/604)) ([5ed6c4a27](https://github.com/supernovae-st/nika/commit/5ed6c4a279ae14d2c738deb82ab0f91a4db8fa0b)) ([#604](https://github.com/supernovae-st/nika/pull/604))
- **cite** — The citation cross-references the spec — additive only ([#636](https://github.com/supernovae-st/nika/issues/636)) ([7b22ee5ce](https://github.com/supernovae-st/nika/commit/7b22ee5ce4822125c8b9efdab4871010708b06c3)) ([#636](https://github.com/supernovae-st/nika/pull/636))
- **crate-spec** — The gate-3 verb tree drops the phantom pack ([#629](https://github.com/supernovae-st/nika/issues/629)) ([58ce0d5f9](https://github.com/supernovae-st/nika/commit/58ce0d5f914beef245aa8956a78d549d1914d149)) ([#629](https://github.com/supernovae-st/nika/pull/629))
- **crate-specs** — The teaching surfaces say graph_format 2 ([#589](https://github.com/supernovae-st/nika/issues/589)) ([86d1f2d20](https://github.com/supernovae-st/nika/commit/86d1f2d2080b4c2d9d98d99d608d6d6643ef89da)) ([#589](https://github.com/supernovae-st/nika/pull/589))
- **crate-specs** — Nika-cli exit-4 paused · fold graph/schema residues ([#625](https://github.com/supernovae-st/nika/issues/625)) ([d62a5ec71](https://github.com/supernovae-st/nika/commit/d62a5ec713069b58a68661b99034dc0abd2867b5)) ([#625](https://github.com/supernovae-st/nika/pull/625))
- **kit** — The plugin readme counts nine — nika_inspect joins at the source ([#584](https://github.com/supernovae-st/nika/issues/584)) ([ccc38b425](https://github.com/supernovae-st/nika/commit/ccc38b4254c494b7d73fdb83191f1d97a7e7371e)) ([#584](https://github.com/supernovae-st/nika/pull/584))
- **rules** — Skills check invocation is /spn-powers:system:yo ([b6c9dff6d](https://github.com/supernovae-st/nika/commit/b6c9dff6dbcaf3ccab70fd680b03085359ff7d5d))

### 🧪 Tests
- **nika-schema** — E-diff static battery pins boundary and inference ([#635](https://github.com/supernovae-st/nika/issues/635)) ([d959ce2e5](https://github.com/supernovae-st/nika/commit/d959ce2e527b90150ac345af59c4dc72c3f93e87)) ([#635](https://github.com/supernovae-st/nika/pull/635))
- **runtime** — Boundary differential — check and runtime agree on exec permits ([#620](https://github.com/supernovae-st/nika/issues/620)) ([783b346da](https://github.com/supernovae-st/nika/commit/783b346dae2353af547e0352050d40b11c61a6b7)) ([#620](https://github.com/supernovae-st/nika/pull/620))
- **schema** — Fuzz the permit checker and the infer-permits surface ([#624](https://github.com/supernovae-st/nika/issues/624)) ([cf9d3f105](https://github.com/supernovae-st/nika/commit/cf9d3f1057651e67df16503f6f4b2492890dd8f5)) ([#624](https://github.com/supernovae-st/nika/pull/624))
- Fs boundary differential — check and runtime agree on canonical globs ([#627](https://github.com/supernovae-st/nika/issues/627)) ([7bb0395cb](https://github.com/supernovae-st/nika/commit/7bb0395cb03bd195b4b34a4b4deefc0fcbc3140c)) ([#627](https://github.com/supernovae-st/nika/pull/627))

### 📦 Build
- Install bubblewrap so the landlock jail proof runs ([#632](https://github.com/supernovae-st/nika/issues/632)) ([5c95a59b6](https://github.com/supernovae-st/nika/commit/5c95a59b6ca8b05a31bdedf352a8e345451165b6)) ([#632](https://github.com/supernovae-st/nika/pull/632))
- Msrv gate + CODEOWNERS + Scorecard badge ([#634](https://github.com/supernovae-st/nika/issues/634)) ([1be89dc9e](https://github.com/supernovae-st/nika/commit/1be89dc9effb1f7969b76a8a5e62a8fad6e8b90c)) ([#634](https://github.com/supernovae-st/nika/pull/634))

### 🧹 Chore
- **ci** — The spec pin becomes a file — one mechanism across the estate ([#577](https://github.com/supernovae-st/nika/issues/577)) ([c6d743353](https://github.com/supernovae-st/nika/commit/c6d74335398956d4dd4669c0b0ed925636019cd4)) ([#577](https://github.com/supernovae-st/nika/pull/577))
- **ci** — Nika-lints joins the public-api floor — the 50th lock ([#591](https://github.com/supernovae-st/nika/issues/591)) ([1ab3e53f5](https://github.com/supernovae-st/nika/commit/1ab3e53f5e995b20179e309b4078f5d2b0984660)) ([#591](https://github.com/supernovae-st/nika/pull/591))
- **ci** — The refonte splits join the public-api floor — 53/53 ratchet closed ([#600](https://github.com/supernovae-st/nika/issues/600)) ([c39b85f8c](https://github.com/supernovae-st/nika/commit/c39b85f8c55afd59b5d74eb6f47c09afce624174)) ([#600](https://github.com/supernovae-st/nika/pull/600))
- **pack** — The pack follows the spec — 1eddca9 (pack-resync) ([#601](https://github.com/supernovae-st/nika/issues/601)) ([b9839ed33](https://github.com/supernovae-st/nika/commit/b9839ed33dcb4b8127e46c84a430511762b0b84b)) ([#601](https://github.com/supernovae-st/nika/pull/601))
- **spec-pin** — The tests leg follows the spec — 1eddca9 ([#602](https://github.com/supernovae-st/nika/issues/602)) ([1bab0d554](https://github.com/supernovae-st/nika/commit/1bab0d55428a3ce7ccd796757a2dbd9be404976b)) ([#602](https://github.com/supernovae-st/nika/pull/602))

## [0.104.0](https://github.com/supernovae-st/nika/compare/v0.103.0..v0.104.0) - 2026-07-17

### ✨ Features
- **providers** — Moonshot joins the canonical catalog (17) — kimi priced ([16c9ae9bd](https://github.com/supernovae-st/nika/commit/16c9ae9bd0da58718fabfc5efc451af552d8c5cc))

### 🐛 Bug Fixes
- **lsp,graph** — Fixtures speak argv — five tests shipped red at the tag ([faac6a5b8](https://github.com/supernovae-st/nika/commit/faac6a5b8bb6c3ccd762ea18ae9bf33d2eeb3867))
- **mcp** — Inspect fixtures speak argv — the third red crate at the tag ([8638dd1e5](https://github.com/supernovae-st/nika/commit/8638dd1e5927064e766bccd8a434c5d985a68c14))
- **mcp** — The second string-form fixture · green verified before commit ([31f4a522a](https://github.com/supernovae-st/nika/commit/31f4a522a357eebbe201d3196998dad66edc123a))
- **providers** — The profiles pin follows the role, not the number ([3b420dbd6](https://github.com/supernovae-st/nika/commit/3b420dbd6957804093d80dc6eafadd801d286a2d))

### 🧹 Chore
- **workspace** — The tag was not fmt-clean under the current rustfmt ([793f11431](https://github.com/supernovae-st/nika/commit/793f11431eec5508e25af8c3cbc300f47dce556b))

## [0.103.0](https://github.com/supernovae-st/nika/compare/v0.102.0..v0.103.0) - 2026-07-13

### Changed — BREAKING (the #75 window: the language tightens)

- **exec semantics never fork on a YAML type again** (#570 · spec#78) —
  `command:` is argv-only (execve · per-element substitution: an
  interpolated value can never break out); the NEW `shell:` field is
  the explicit dangerous door (`/bin/sh -c` · pipes/redirects · the
  blocklist attaches HERE). Exactly one of the two; a string
  `command:` rejects with the migration teaching.
- **bare `${{ tasks.X }}` is a validation error** (`NIKA-VAR-020`) —
  the task result is a record, the projection set (.output/.status/
  .error/.duration_ms) is CLOSED; the envelope is not a value.
- **the gate algebra becomes normative** (spec#78) — the when-status
  pattern is a propagation CHOICE (default gate: failure cancels the
  cascade · when-form: skip-once-then-continue), stated as a table.
- **the analyzer learns the argv world** — lints 006/007 (timeout
  wrapper · shard signature) read argv heads/tokens.

### Added

- **`nika/semanticDocument`** — the LSP's vendor-prefixed custom
  request (capability-gated via `experimental.nika`): the canonical
  `graph_format: 1` projection VERBATIM plus a span wrapper (task id →
  declaring token range). One projector, every surface — the graph
  moves to its own L0 member **`nika-graph`** (split from nika-schema
  at the 15k cap; the CLI keeps the renderers and re-exports), so
  `inspect --format json`, the LSP and future canvases read one truth.

## [0.102.0](https://github.com/supernovae-st/nika/compare/v0.101.0..v0.102.0) - 2026-07-13

### Added

- **The editor speaks the language — the LSP completes it end to end**
  (#549 · #553 · #556 · #558). The pause after a colon answers itself
  (space joins the triggers); the file teaches itself — `args:` keys
  from the tool's own schema (required first), `mode:` enums scoped to
  the enclosing tool, `${{ vars./secrets./env. }}` completions from the
  FILE's declarations, self-reference filtered (an offered cycle is a
  bug); hover cards on `tool:`/`model:`/`- id:` (the declaration reads
  its own graph — wave k/N, transitive reach, a 3-tier model
  fallback); references resolve like the graph reads — closure-aware
  jumps, `${{ }}` goto-definition to the exact declaration span.
- **`wave-sweep.sh`** — the whole release-wave version sweep, one
  pattern-anchored command (matches each carrier by ROLE, uniformity
  guard; born from the 0.101 concurrent-session misses).
- **The coherence bot watches the three drifts caught by hand**
  (action@v1 served default · starter template default · registry
  certifier pin) — with the grace ladder on the served default and a
  9/9 offline harness.

### Fixed

- **The changelog workflow heals its own stale PRs** (#555) — the
  auto-prepend guard closes superseded cliff PRs instead of stacking
  them.

## [0.101.0](https://github.com/supernovae-st/nika/compare/v0.100.0..v0.101.0) - 2026-07-13

### Added

- **The sovereign lane ships whole** (#518 · the release ruling,
  executed) — every release binary now carries `local-infer`: `nika
  model pull` → `nika model serve` → workflow `infer` against your own
  machine, no cloud, no external daemon, no build-from-source wall.
  Measured +2.4 MB. CPU on every target on purpose (darwin included):
  candle 0.10's quantized rms-norm Metal kernel is broken — a metal
  lane would die at first token. The funnel now pins the teaching:
  a feature-carrying binary must never utter « built without local
  inference ».

## [0.100.0](https://github.com/supernovae-st/nika/compare/v0.99.0..v0.100.0) - 2026-07-12

### Added
- `nika init` equips Cursor FULLY, project-side — the binary carries what
  the local plugin loader drops (it consumes MCP + skills only): the three
  kit subagents land in `.cursor/agents/`, the delegation rule in
  `.cursor/rules/`, and the three seatbelt hooks in `.cursor/hooks.json` +
  `.cursor/hooks-nika/` (exec bit set on unix). All bodies `include_str!`
  from the kit — one source, byte parity by construction; parity tests walk
  every hooks.json command back to the scaffold table (#509).

### Changed

- **One glyph table, zero-alloc live channel** (rust-pro pass) — the
  verb glyph vocabulary lives ONCE (`Theme::verb_glyph_bare` joins
  `verb_glyph`; the render-side copy dies), and the wire map's live
  channel becomes a borrowed probe (`LiveProbe = &dyn Fn(&str) -> bool`
  — a 10 Hz repaint asks the rows, it never materializes a set).

- **Less, but better: 24 verbs become 20, in six families** (operator
  Rams pass) — `graph` folds into `inspect --format json|mermaid|dot`
  (one projector, one door; `check`/`run` already draw the map),
  `schema` folds into `nika spec --schema`, `context` folds into
  `nika welcome --deep` (one mirror, two depths — the `--json` agent
  contract rides along unchanged), and `tools` folds into
  `nika catalog --tools` (one catalogue, two shelves). The COLOUR
  CHAIN is one road now: `--ascii` joins the global flags and the 23
  per-verb `--no-color`/`--ascii` twins die (`--color never` and
  `--plain` were always the umbrella — one chain, no per-verb echo).
  The help reads as the family map (make · prove · run · learn · wire
  · machine) instead of a flat 24-row wall, and every teaching surface
  (agent briefs · README · hints) speaks the new doors. Pre-1.0 · no
  aliases kept (no-legacy).

### Changed

- **The embedded pack re-syncs the taught corpus (spec #66)** — the
  binary now ships the renumbered 01-07 path (contiguous, `git mv`'d in
  spec), the six corpus bug-fixes (fetch-chain's literal recover ·
  ceo-brief's date slice · the null-safe fan-ins · the untyped
  quarantine output · config-drift's `{}` default), the offline-green
  gate lessons, the `# Needs ·` header contract, and the agent
  done-contract prompts — `nika examples` and `nika new` teach the same
  corpus the spec publishes, no drift. Engine-side foundation refs
  follow (examples/README table · run-tip fixtures).

### Added

- **The motion SSOT is parity-gated, and `test` joins the lazy door**
  — a new integration gate proves the terminal spinner constants
  mirror the vendored `design/motion.yaml` (spec #65) family by family
  (`infer·sampling` · `exec·scanline` · `invoke·roundtrip` ·
  `agent·orbit`): hand-edited drift between the CLI's motion and the
  site's tiles now dies in CI, not in a screenshot. And bare
  `nika test` resolves the workspace's only workflow exactly like
  `run`/`check` (zero→trio · several→copy-paste list) — the last
  family inconsistency at the lazy door.

- **The DAG is visible where you look** (operator: « voir la dag ! ») —
  `nika check` on a TTY now ends with the SAME themed wire art `graph
  --format ascii` speaks (the audit reads as the graph it judged;
  conformance failures skip it — no valid order exists to draw), and
  the Live run storyboard gains the LIVING MAP: one wave-column line
  under the header where every node wears its state (dim pending · the
  verb's own motion frame while running · green/red settled · `⊘`
  skipped), repainted every tick so the running node's spinner turns
  INSIDE the map. Wide runs drop ids and keep chips — the map never
  wraps. And on shapes the wire law can draw, the Live run leads every
  repaint with the FULL wire map itself (`◆ gather ───▶ ⠂ think ───▶
  · persist` — the same art as check, each node painted by its live
  state, the running node turning its verb's motion in place; the
  wave-column line stands down to it) — and the INCOMING edge pulses:
  the rail segment feeding a running node cycles density (`──╍▶` ·
  Accent) so the map shows where the run's energy flows; a still map
  is byte-stable under ticks (no idle flicker · law-tested), and the
  flow gantt's lanes wear their verb chips (`◆ discover ▕██▏` — the
  timeline speaks the same 4-verb vocabulary as the rows). The wire
  drawing itself DESCENDS to `nika-display::wires` (decoupled
  `WireGraph` — any surface with waves + deps can draw; the CLI keeps a
  thin projection bridge), freeing `nika-cli` back under the 15k wall.
  Interactive surfaces only; every sober register byte-intact.

- **One source of truth for the corpus — the loose engine copies die**
  — `engine/examples/` carried two pre-pack workflows (`pr-risk-review`
  · `image-og-pipeline`) that duplicated pack showcases from every
  angle's worst position: unversioned, unlisted, un-CI'd. Nuked; the
  folder keeps a thin pointer README (the spec pack, vendored into the
  binary, IS the gallery — `nika examples`), and the root README's
  table row runs the pack twin instead.

- **UNIFY: one street, one gesture** — the whole onboarding surface now
  reads as a single funnel. The concierge card keys its `start here`
  block on the WORKSPACE STATE (empty → the offline proof + the wizard;
  workflows without briefs → `init` adds-only; one founded workflow →
  bare `run`; several → `context` — one strong key per state, gh/bun
  law). `nika new --from` resolves EXAMPLES too (slug · filename ·
  showcase path — verbatim lessons beside the template skeletons; one
  resolution ladder, `examples copy` is the same gesture's showroom
  handle). And the founding wizard gains the sixth lane: **start from
  one example** (slug beat · Enter = `01-hello` · no model question — a
  lesson carries its own), with `nika init --example <slug>` as the
  scriptable twin. Proven end-to-end: a PTY walk of the lane and a
  first-hour smoke (copy → bare run → new-from-example → init
  --example, all green offline).

- **Bare `nika` is the concierge, and the showroom hands over the keys**
  — on a terminal, plain `nika` now answers with the welcome card (what
  this machine has · where you are · the next gesture) instead of the
  22-command wall; pipes and scripts keep the exact usage screen and
  exit 2. The missing adoption gesture ships as `nika examples copy
  <slug> [dest]` (the embedded example lands as YOUR file · next steps
  said · refuses silent overwrites · points `nika init` when no agent
  briefs sit beside it), and a green TTY `examples run` ends with the
  one-line handoff `make it yours · nika examples copy <slug>` — the
  full loop is now see → like → own → bare `nika run` finds it.

- **`nika examples run` takes `--var` and `--quiet`** — several
  examples declare required vars in their header (`04-schema-retry`
  says `--var text=…`) and the old surface had no way to pass them
  (gauntlet friction F7). `--quiet` gives the verdict-line register;
  the TTY pre-display keeps its Live-only gate.

### Added

- **`nika init` becomes the founding wizard** — bare on a terminal it
  converses (the clack-school rail): pick a project **recipe**
  (`agentic` — the 4-pattern curriculum chain → fan-out → gate → agent
  loop · `starter` · `ship` · `content` · `minimal`), the model
  (catalog-derived · local first · Enter = the offline mock), the VS
  Code DAG **canvas theme** (`nika.dag.theme` stamped into the created
  `.vscode/settings.json`), and which agent clients to **wire** to the
  MCP oracle — then scaffolds, audits every scaffolded workflow on the
  spot (audit-before-run inside the first minute), and hands over a
  ready panel. Every recipe workflow is an embedded template VERBATIM
  through the same stamp `nika new` uses, so the own-corpus law (a
  fresh scaffold checks clean) is inherited, never re-proven. The
  scriptable twins: `--recipe <name>` · `--theme <nika|editor|
  phosphor|auto>` · `--wire <client,…>`; plain `--yes` keeps the
  historical report byte-for-byte, and every question is asked BEFORE
  the first write (cancel = « nothing written », honestly).

- **The verb identity column reaches the terminal** — the four verbs
  paint their tokens-SSOT glyphs (`◇` infer · `▷` exec · `◆` invoke ·
  `✦` agent · ASCII twins `i $ @ *`) in a bright-band ANSI slot
  (identity vocabulary · never colliding with a verdict hue — the
  user's terminal theme still owns every color): `nika inspect` wave
  boxes and single rows, the `nika graph --format ascii` wire art, and
  `run --dry-run` (which now renders the same themed anatomy). The
  graph ascii art finally inherits the binary's ONE color chain instead
  of a forced monochrome — pipes stay escape-free by resolution, TTYs
  see the art they were owed.

- **`nika run` breathes while it thinks** — the Live storyboard gains
  real spinner ticks: a timer rider advances the braille phase (~10/s)
  while a task is running, so a long `infer`/`agent` await animates
  instead of freezing between settles. Live + motion only (the
  reduced-motion env, `--no-progress`, pipes, CI and `--json` never
  tick — every sober register keeps byte-identical output), and the
  frame still repaints inside one DEC-2026 synchronized frame.

- **`nika examples` becomes the organized corpus** — the flat slug dump
  grows into the taxonomy: « the path » (the 7 foundation steps · FULL
  `.nika.yaml` filenames · each file's own header title · verb chips ·
  what you see is what you type) then the showcase grouped T1→T4
  (starters · daily ops · parallel intelligence · autonomous). Every
  fact derives from the example file itself at call time (header +
  line scan — no engine catalog to rot). `show` frames the anatomy
  (file · title · verbs · task count) over the VERBATIM body and hands
  the run command; `run` PRE-DISPLAYS the source on a TTY before the
  first token (the lesson before the spend — pipes byte-unchanged).

- **`nika init` recipes ship their index** — every workflow set writes
  a generated `workflows/README.md` (the curriculum in order: file ·
  what it teaches from the template's own header · per-file check/run
  commands); the proof ladder audits workflows only. `nika tools`
  speaks the rail (categories as heads · names strong · teaching cuts
  dim).

- **`nika-onboard` — the onboarding surface becomes its own member**
  (the 15k prod-LOC wall broke at 16,027: base main sat 12 under, the
  founding wizard crossed it — per D-2026-07-09-N1 the unit descends,
  the `nika-display`/`nika-dap`/`nika-tmpl` precedents). `nika new`'s
  guided flow + `nika init`'s founding body (briefs · recipes · wizard)
  now live in `crates/nika-onboard`; the composition root keeps thin
  adapters that inject the two REAL effects (the check ladder · the MCP
  wiring) — the member converses and scaffolds, the root owns what
  proving and wiring mean. Zero observable change on every surface.

- **`nika-display` grows the structural chrome vocabulary**
  (`chrome.rs`): the wizard rail (`◆ │ └` · ASCII twins), the rounded
  panel with frame-true width math, the segment progress bar with its
  half-cell frontier, the 3-line identity banner (no figlet walls), and
  the dither pulse that only exists where motion does — every shape
  colour-through-Role only, 2-cell law, zero escapes when colour is
  off.
- **LSP code actions — the `--fix` engine in every editor**: the language
  server now answers `textDocument/codeAction` with quickfix renames
  built from the checker's typed `offending`/`suggestion` pairs (unknown
  fields · tools · args · rename-shaped conformance findings). Same
  discipline as `nika check --fix`: did-you-mean only, unique-token
  only — an ambiguous or suggestion-less finding offers nothing. One
  fix engine, projected; VS Code, Cursor, zed, helix and neovim get the
  one-click repair the terminal already had.

### Fixed

- **`on_error.recover` awaits a no-edge referent's terminal state**
  (spec 05 §recover · #291 #402) — a same-wave `recover: ${{ tasks.X.output }}`
  used to race dispatch and fail `NIKA-VAR-001` whenever the referent had
  not settled yet. Recoveries now park on the settle spine and retry as
  referents turn terminal; a workflow-end pass resolves mutual-recovery
  cycles against each side's pre-recovery failed record (history never
  rewrites). Deterministic by construction: the park table only moves on
  the sequential settle spine — the event stream stays byte-identical
  across parallelism caps.
- **A settled sibling's frames reach the trace at ITS settle, not the
  wave join** (#412) — a `kill -9` mid-wave used to lose the resume
  credit of every sibling that had already finished on the console (the
  journal held `task_scheduled` only), so `--resume` re-ran — and
  re-billed — finished work. Settles now stream through the ordered
  spine; total event order is unchanged, only the timing moves earlier.
- **Swapping the envelope `model:` re-runs a model-less infer on resume**
  (#409) — the effective default model (`--model` override, else the
  envelope line) joins those tasks' resume identity, so a model swap can
  no longer cache-hit the OLD model's output. Tasks pinning their own
  `model:` and exec tasks never re-key; older traces simply re-run once.
- **A token-burning empty answer warns on the console** (#410) — a
  thinking model that spends its whole budget inside the think block and
  settles green with "" now speaks: the OBS-E oracle covers the
  unreported-split shape (the ollama path strips thinking upstream), and
  the display renders `⚠ <task> · <warning>` above the meter on the
  final frame and the streamed close — a green run no longer silently
  feeds the empty string downstream.
- **The example rescue tip keys on the failure kind** (#145) — an exec
  `program not found` used to earn the mock-model nudge (a swap that
  cannot conjure a missing binary). Infer/provider failures keep the
  offline-preview tip; a missing program names the real dependency.
- **`nika:convert` joins the native-first/005 inventory** (#475) — the
  check hint's helper-script inventory named every native lane except
  the one that parses; "my input is YAML/CSV/TOML" was the single most
  common reason a sidecar survived it. One clause (spec row paired in
  nika-spec#64); the test pins the lane.

- **Every trace reader resolves the names `trace ls` prints** — `show` ·
  `replay` · `outputs` · `verify` · `export` · `peek` · `flow` accept the
  bare store name exactly like `rm` always did (an explicit path still
  wins · unknown handles keep their teaching error). Copy-pasting a name
  from `ls` into `show` no longer answers "No such file or directory".
- **The MCP `nika_explain` teaches the runtime namespaces** — per-builtin
  (`NIKA-BUILTIN-<NAME>-<NNN>`) and per-provider (`NIKA-PROVIDER-<NNN>`)
  codes answer over the agent lane with the same text the CLI gives (the
  teaching moved down to `nika-error::codes::namespace_help` — one text,
  one home), instead of "unknown code" on exactly the codes a failed run
  hands an agent.

- **`nika:glob` walks its pattern's literal directory prefix** — a scoped
  pattern (`hiring/inbox/*.md`) now anchors its walk (and the
  `permits.fs.read` gate that fences it) at `./hiring/inbox` instead of
  the whole cwd, so a least-privilege boundary like
  `read: ["./hiring/inbox/**"]` accepts the glob that stays inside it
  (previously: `NIKA-SEC-004` on the `.` walk root — the
  `t3-resume-screener` showcase could never run). Returned paths keep
  the exact historical `./…` byte shape (run traces and registry
  oracles hash them); a missing prefix directory yields `[]` uniformly
  (absolute patterns errored before); the SEC-004 refusal now names the
  real walk root.

### Changed

- **The openai showcase seats leave 2024** — the catalog's two openai
  showcase models are now `gpt-5.2` (default) and `gpt-5-mini` (cheap),
  matching the live API's current family and the spec's teaching
  surface (both already priced + capability-mapped; `gpt-4o` and every
  other live id keeps working verbatim through provider pass-through —
  proven with a live `openai/gpt-5.2` run, 780ms · $0.000098).

### Added

- **`nika model pull/list/rm` — first-class Hugging Face acquisition for
  the native path** ([#146](https://github.com/supernovae-st/nika/issues/146)):
  one command from the Hub to a sovereign in-process run. `pull
  owner/repo[:QUANT]` resolves the repo's file tree first (sizes BEFORE
  any byte moves; 2 GiB and over confirms, `--yes` for CI), streams the
  GGUF over the house `nika-http` seam (rustls · SSRF floor · per-hop
  redirect vetting — no `hf-hub`/`ureq` second HTTP stack), resumes an
  interrupted transfer from its `.part` via `Range:`, and brings
  `tokenizer.json` along — the exact sibling layout the serve loader
  wants. ONE canonical models dir (`~/.nika/models/<owner>/<repo>/`):
  the downloader and the resolver share it by construction (the
  brouillon-era pull/load two-dir mismatch cannot re-happen), so `nika
  model serve --model owner/repo[:QUANT]` now resolves pulled ids (and
  bare file stems) — a real path still passes straight through.
  `list` prints the dir once with id · size · file rows; `rm` reclaims
  a whole repo or one quant (sweeping a gguf-empty dir) and refuses a
  no-match WITH the installed list as the teaching surface. `HF_TOKEN`
  authenticates gated repos (the Hub answers 401 for missing repos too
  — the refusal names both lanes). `nika doctor` grows the models row:
  dir + count + bytes once anything is pulled (any build), and a
  teach-pull advisory on a sidecar build with nothing to serve. The
  fetch is CLI-level, like `registry:` pulls — a workflow's `permits:`
  never govern it. Acquisition only: the candle runtime owns
  loading/generation (ADR-091/093). The whole unit ships as the
  `nika-models` member (store · pull · serve glue) per D-2026-07-09-N1
  — `nika-cli` sat at 14,958 of the 15,000 prod-LOC cap and keeps thin
  exit-contract adapters at the unchanged public paths (the
  `nika-onboard`/`nika-display`/`nika-dap` descents' wall, the same
  house way).

- **The one earned ask: welcome + init grow the community line** (#498 —
  the traction baseline showed installs running 2.5x ahead of stars
  because the product never asks). `nika welcome`'s footer gains a third
  door on its `learn:` line (`⭐ github.com/supernovae-st/nika`, OSC-8
  linked) and `nika init`'s NEXT_BLOCK closes with the one-line ask.
  Once per surface, additive to the #158 script-stable lines, JSON
  outputs untouched — working commands (check · run) stay marketing-free
  by doctrine.

- **`nika examples run` carries the run trio** — `--var KEY=VALUE`
  (repeatable · the `nika run` contract), `--no-progress` and
  `--max-cost-usd <n>`, threaded through the same funnel `run` uses.
  Examples with `required:` vars (`19-schema-retry`) were unrunnable by
  this surface — and clap's `-- --var` tip pointed at a trailing lane
  that never existed.

- **Pre-generated shell completions ride the release tarballs** (#487)
  — each platform tarball now carries `completions/` (bash `nika.bash` ·
  zsh `_nika` · fish `nika.fish`), generated by the exact binary being
  packaged and gated non-empty before tar. Distro packages (AUR
  `nika-bin` · future nixpkgs/apk) can `install -Dm644` them without
  executing the target-arch binary in `package()` — the aarch64-on-x86
  packaging law. Packaging only, zero engine change.

- **Cursor first-class: the plugin kit gains its native format + `init`
  wires the MCP** — the agents kit (`.agents/plugins/nika/`) now ships a
  `.cursor-plugin/plugin.json` (Cursor's marketplace manifest: logo ·
  rules · skills · agents · commands · hooks · mcpServers), the
  `nika-author` subagent (the route → instantiate → fill-SLOTs → check →
  repair protocol as an agent definition), a `check-on-edit` hook
  (afterFileEdit on `*.nika.yaml` runs `nika check` — capability-honest:
  missing binary skips, never blocks), the language rule as a bundled
  file (extracted byte-identical from the `init` template), and the
  brand logo. Both existing plugin manifests bump to 0.3.0 together
  (the marketplace mirror gate pins the agreement) and the engine-root
  Claude marketplace catches up from its stale 0.1.0 two-tool
  description. `nika init` now also writes a project-scoped
  `.cursor/mcp.json` (the read-only 8-tool oracle reaches Cursor's
  agent with zero manual setup — same skip-if-exists discipline as
  every scaffold file).

- **`nika wire` wave-3b: `cline` + `continue`** (#449 — closes the
  ranked list). Cline: Cursor-style `mcpServers` record; the resolver
  picks a host IDE's EXISTING `saoudrizwan.claude-dev` globalStorage
  file (VS Code · Cursor · Windsurf · VSCodium · Insiders — the live,
  chokidar-hot-reloaded one), else the stable
  `~/.cline/data/settings/cline_mcp_settings.json` (the CLI's home
  today and the extension's after its in-flight migration). Continue:
  an OWN-FILE write at `~/.continue/mcpServers/nika.json` (the
  Claude-Desktop JSON shape its drop-dir scans) — the user's
  comment-bearing `config.yaml` is never touched, and the verdict line
  carries the reload hint (external drop-dir writes are not
  hot-reloaded). Both ride `wire all` (15 targets).

- **`registry:owner/name[@version]` refs on `check` and `run`** — the
  registry's 22 certified artifacts become consumable from the CLI:
  `nika check registry:supernovae-st/competitor-radar` audits a
  stranger's workflow before it ever touches your workspace, and `run`
  rides the same seam. The resolution is the registry-v0.1 trust chain,
  native: index → advisory MUST-refuse (`NIKA-REG-002`) → entry TOML
  re-verification (never the index alone; disagreement refuses,
  `NIKA-REG-005`) → raw https fetch capped at 1 MiB → sha256 must equal
  the pinned digest or nothing is written (`NIKA-REG-003`). Verified
  bytes cache under `~/.nika/registry/<owner>/<name>/` beside their
  digest record — a cache hit re-verifies and runs offline; a bare ref
  pins its version at first resolve and never floats. Nothing executes
  at pull time: the fetched file feeds the SAME audit-before-run
  pipeline as any local path, and a workflow's `permits:` govern its
  run, never this CLI-level fetch (the help text says so). `--fix`
  refuses registry refs (a digest-pinned artifact stays read-only —
  copy it to edit). The `nika add` install verb, org indexes
  (`--index`) and `nika.lock` stay on the ADR-106 lane (#452).

- **`graph --format mermaid` paints verb identity** — nodes carry their
  verb class and the diagram ends with the SAME classDef map every
  projected docs page uses (the shared visual vocabulary: spec
  `design/tokens.yaml`, vendored into the pack as `design-tokens.yaml`
  and exposed via `nika_pack::design_tokens()`). Byte-parity with the
  spec's showcase projector (`fill = color + 22` alpha · only drawn
  verbs get a classDef); a parity test pins the renderer's consts to
  the vendored tokens, so a spec-side hue change stays red engine-side
  until the table follows. Terminal output untouched — chrome remains
  ANSI-16 semantic (#464).

- **`nika wire` wave-3a: `claude-desktop` + `qwen`** (#449). Claude
  Desktop was the double gap — a different app with a different config
  file than Claude Code's `~/.claude.json`: the writer targets
  `claude_desktop_config.json` under the platform app-config dir
  (macOS `~/Library/Application Support/Claude/` · Windows
  `%APPDATA%\Claude\` · Linux `~/.config/Claude/`), `mcpServers` root,
  idempotent, unrelated servers preserved. Qwen Code (gemini-cli fork)
  reads the same `mcpServers` key from `~/.qwen/settings.json`. Both
  ride `wire all`. Cline/Continue remain on #449 (wave-3b — variant
  paths and YAML-list shapes verified first).

- **`nika model serve` — the sovereign sidecar's launch surface** (the
  ADR-091/093 rung `nika model pull` (#146) sequences after). A build
  with `--features local-infer` serves a Qwen3-family GGUF as a
  loopback OpenAI-compatible server, in the foreground (Ctrl-C stops
  it · one generation at a time): `nika model serve --model
  <path.gguf> [--tokenizer <path>] [--port 8712] [--model-id <id>]` —
  the tokenizer defaults to `tokenizer.json` beside the weights, the
  id to the file stem. Zero new dispatch seam: a workflow reaches it
  through the existing local base-URL lane (`export
  NIKA_LLAMACPP_BASE_URL=http://127.0.0.1:8712` · `model:
  llamacpp/<id>`). The DEFAULT binary keeps the subcommand and refuses
  with the build recipe (exit 3 · zero inference deps linked), and
  `nika doctor` gains a `sidecar` row exactly when the feature is
  built in.

- **An exact loopback literal in `permits.net.http` clears the SSRF
  floor for that host** (#395 · the ADR-092 secrets-`egress:`
  precedent: the owner's explicit act, co-located with the boundary).
  Qualifying literals: `localhost` · a `127.x.y.z` v4 literal · `::1`
  (bracketed `[::1]` accepted) — NEVER a glob, never the `*.localhost`
  family, never RFC1918/link-local/CGN/metadata (those stay
  floor-blocked even when named). The clearing is exact-host and
  host-level (ports don't participate in permits): a permitted
  `localhost` reaches its own resolved `127.0.0.1`/`::1`, while a
  rebind of that name to any other blocked range, a redirect hop to an
  un-permitted floor host, and a public DNS name resolving to loopback
  (`mylocal.dev` → `127.0.0.1`) all still refuse — the declassification
  is the literal in the file, never the resolution. check≡run same-PR:
  the floor-parity pass stops flagging the permitted literal
  (NIKA-SEC-005) and the dead-grant flag skips it; the clearing is
  stated instead — an informational line in the PERMITS panel and a
  `permits.notes` entry in the JSON report. `--infer-permits` still
  NEVER writes a loopback grant (the explicit act stays the author's);
  its honesty note now teaches the opt-in.

- **`nika check --fix` — the in-binary repair loop.** The ladder's typed
  did-you-mean suggestions become one keystroke (the `clippy --fix` /
  `eslint --fix` shape): typo'd fields (`promt:`), tools (`nika:raed`),
  args (`inpit` · the `expr`→`expression` prefix rung), `depends_on`
  targets and `${{ }}` references (fully qualified — a splice never
  strips the namespace) are renamed in place and the file re-audited
  until the loop converges. Safety over reach: typed suggestions only
  (never scraped from prose), a token must be unique in the file as a
  whole word (ambiguity is skipped with a note — a skip stays retryable
  across rounds, so the two-site case heals: the qualified reference
  first, the bare token once it stands alone), zero applied repairs =
  zero write, and the publish is temp-sibling + rename (POSIX-atomic ·
  a crash or full disk never truncates the workflow). Do-no-harm proved
  over the whole 147-workflow battery corpus: 0 files rewritten, 0
  phantom repairs. The scaffolded teaching surfaces (`nika init`'s
  AGENTS.md · copilot · CLAUDE.md · cursor rule · agent skill) all name
  the flag, pinned by a parity loop so a future capability cannot miss
  the scaffold train (#434 · #444 · #446 · #450).
- **Software Heritage badge.** The repository is archived at Software
  Heritage (save request accepted 2026-07-11 · the archive re-crawls on
  its own cadence); the README badge links the archived origin — the
  permanent, vendor-independent copy of the source.

- **Official Docker image on GHCR** — the release workflow builds and
  pushes `ghcr.io/supernovae-st/nika` (multi-arch linux/amd64 + linux/arm64 ·
  tags `latest` + semver). The image is fed from the SAME linux tarball
  artifacts the release ships — never a rebuild — so its binary is
  bit-identical to the tarballs the funnel e2e + trust battery already
  gated. The entrypoint is the whole CLI: `docker run --rm
  ghcr.io/supernovae-st/nika --version`, and `docker run -i --rm
  ghcr.io/supernovae-st/nika mcp` serves the read-only MCP oracle.
  (#442 — two rival rails from the same-day trains #463/#465 collided as
  a duplicate `jobs.docker` key that would have killed the next release
  at parse time; the artifact-fed rail survives, the rebuild rail and
  its `docker/Dockerfile` are gone.)

### Fixed

- **The same provider failure now speaks ONE error language on both
  verbs** (#468): the agent loop's mid-loop provider failure surfaced
  as a bare-numeric `NIKA-463` — outside the spec's namespace grammar,
  so `nika check` rejected it in `on_codes:` and no retry/recover
  filter could ever match the agent path — while `infer` surfaced the
  same 408 as `NIKA-INFER-001`. The agent's chained failures now carry
  the spec's shared classes on the wire: provider call → `NIKA-INFER-001`,
  final-message `schema:` gate → `NIKA-INFER-002` (the namespace follows
  the failure class, not the hosting verb — the spec's own `NIKA-SEC-002`
  precedent). `retry.on_codes: [NIKA-INFER-001]` now catches provider
  failures on BOTH verbs; the internal registry identities (463/464 ·
  `nika explain`) are unchanged.
  verb · 2026-07-11): a standalone `nika:compose` passed `check` but
  the runtime refused it (COMPOSE-001) — the loop-only rule joins the
  one shape table both surfaces read, beside its sibling `nika:done`;
  the `when:` shape teaching (`NIKA-VAR-005`) routes by declared shape
  — the bool route leads (`x == true` · `!x`), since the old
  comparison examples applied to a declared boolean traded VAR-005 for
  the no-coercion type error; and the PERMITS panel leads each row
  with the wire code (`[NIKA-SEC-004 · tools]` · floor rows
  `[NIKA-SEC-005 · net]`), so every panel is `nika explain`-able.
  Re-verdict vs the fixed binary: 36/36 (#458).
- **`nika:chart` and `nika:tts_generate` were invisible to the effect
  classification** — a chart/tts write outside the boundary passed the
  static scan and failed only at runtime, and `--infer-permits` wrote a
  boundary the run then REFUSED (the self-refusing class the analyzer
  forbids everywhere else). Both media graduates now classify: chart's
  permit-gated `out` write (plus its `compile_to` vega `.vl.json`
  sibling — one shared derivation, matching the runtime byte for byte)
  and tts's recursive `output_dir` write (the image_generate shape).
  Escape scanning, boundary inference and the per-task graph `permits`
  attribution all speak them now.

- **`graph --format json` fills the per-task `permits` attribution** —
  the field the projection declared as its contract (empty since #367)
  now carries each task's pinnable capability effects: `exec: <prog>`
  (or `exec: true` when dynamic) · `fs.read:`/`fs.write: <path>` ·
  `net.http: <host>` · `tool: <ref>` — deterministic order, the same
  effect walk `--infer-permits` aggregates into the workflow boundary,
  un-aggregated (`nika_schema::check::task_permits`). Effects too
  dynamic to pin project nothing (the check's escape lane owns that
  story — a projection never guesses). Graph clients that already read
  the field (the nika-vscode `▦ N` card chip) light up with no client
  change.

- **`graph --format json` projects the declared POLICY** — nodes gain
  `retry_max_attempts` · `timeout_ms` · `on_error`
  (`recover`/`skip`/`fail_workflow`) · `outputs` (declared binding
  names, source order). Additive, absent when undeclared
  (skip-serializing — no fake defaults), `graph_format` stays 1 per the
  tolerance contract. One voice: canvas/graph clients read the DECLARED
  policy from the projection instead of re-parsing the YAML for it (the
  nika-vscode dense cards were regex-reading these four facts
  client-side — that read becomes the pre-0.99 fallback).

- **`llms-install.md` gains the bare-metal lane · `context7.json` steers
  the retrieval surface.** The install runbook now covers the box with no
  package manager: release tarball + `SHA256SUMS` verify, with version
  discovery via the releases web redirect instead of the GitHub API
  (anonymous `api.github.com` rate-limits to 403 in shared/CI
  environments — hit live while proving this lane; the whole flow was
  then proven curl-only end to end). `context7.json` is the owner config
  Context7 reads when the repo is indexed: it points parsing at the
  teaching surfaces (docs · examples · spec pointers, engine internals
  excluded) and rides four authoring rules along every retrieval — the
  check-first loop, provider/model form, unpriced-never-free, and
  never-invent-builtins.

- **`llms.txt` lists `llms-install.md`.** The agent on-ramp's « Start
  here » now includes the install runbook — an agent landing on the raw
  repo finds the install lane before the authoring contract.

- **Agent-facing repo metadata: `llms-install.md` · `CITATION.cff` ·
  `ADOPTERS.md`.** `llms-install.md` is the install runbook written FOR
  an AI agent (the file Cline-class installers read): package-manager
  paths only in the agent lane — brew · `cargo binstall --git` · nix —
  each checksum-verified by its own flow, the shell script stays the
  human lane (the same line the skill-guard hardening drew). Verify ·
  prove offline (`nika examples run 01-hello --model mock/echo`) · wire
  MCP · uninstall clean. `CITATION.cff` makes GitHub's « Cite this
  repository » box work and is the metadata Zenodo reads at archive
  time — version/date deliberately not hand-written (releases stamp
  them); CITATIONS.md keeps crediting the research Nika builds on — the
  two files answer different questions. `ADOPTERS.md` seeds the public
  adopters table (one-line-PR invitation · « runs with nika » badge).

- **`nika check` audits several files in one call.** `nika check a.nika.yaml
  b.nika.yaml …` runs the full per-file ladder on each (every report keeps
  its own header), no stop-at-first — a broken file mid-list exits with the
  worst spec-§4 code while the files after it still audit. The single-file
  invocation is byte-identical to before, and the machine modes stay
  one-file-per-call: `--json` (`report_version: 1` is a per-file contract)
  and `--infer-permits` with several files refuse with a teach line
  (exit 3 — the invocation is wrong, no file was judged). This is the
  pre-commit/CI shape: the framework passes every staged match in one
  argv, and the hook manifest now points straight at the binary — the
  fan-out wrapper the manifest shipped with (#407) is gone before it
  ever reached a tag.
- **`.pre-commit-hooks.yaml` — `nika check` as a pre-commit hook.** The
  standard manifest the [pre-commit](https://pre-commit.com) framework
  reads from a hooked repo (actionlint · shellcheck-py · ruff all ship
  one): every hooked repo audits its staged `*.nika.yaml` at commit
  time. `language: script` with a fan-out wrapper because `nika check`
  audits ONE file per call (its report is a per-file contract) while
  pre-commit passes every staged match in one argv — each failing file
  reports in the same run and the worst spec-§4 exit survives (proven
  against the released binary: a broken file mid-list exits 2, the
  files after it still audit). The `nika` binary is assumed on PATH,
  the same trade actionlint's system hook takes. Unblocks the
  pre-commit.com listing once tagged (#407).
- **`nika wire gemini|lmstudio|junie` — wave-2 wire targets.** Three more
  hosts join the explicit, idempotent MCP wiring (#384, sibling of #330):
  Gemini CLI (`~/.gemini/settings.json` — the shared settings file, every
  unrelated key preserved), LM Studio (dedicated `mcp.json`, with the real
  macOS location honoured when the app keeps it under `~/.cache/lm-studio/`
  — upstream docs and reality disagree, lmstudio-bug-tracker#1371), and
  JetBrains Junie (project-scope `.junie/mcp/mcp.json`). Same laws as
  wave 1: create-or-repair, stale-argv migration, other servers untouched,
  `wire all` covers them. Cline stays marketplace-side by design.
- **`nix run github:supernovae-st/nika` — the flake install path.** A
  root `flake.nix` builds the exact release binary (`--bin nika-cli`,
  locked, renamed to its public name) on the four release platforms —
  zero gatekeeper, no queue, the 2026 Rust-CLI canon (helix · atuin ·
  jujutsu all ship one). The derivation needs zero native build inputs
  (the operator crate's dep tree is pure Rust + rustls) and skips tests
  by design — diamond-ci gates every merge; a dedicated `nix.yml` job
  proves the build on every PR touching the flake or the lockfile
  (#388). Pairs with the binstall metadata (#383): together they cover
  the two big « install without brew » crowds.
- **`cargo binstall nika-cli` resolves the prebuilt release binaries.** The
  binary crate ships `[package.metadata.binstall]` mapping every release
  asset (macos/linux × arm64/x64 · the binary at tarball root under its
  public name) onto binstall's resolver — the Rust-native fast path now
  fetches the published tarball instead of silently falling back to a
  source build; verification rides the per-release `SHA256SUMS` (#383).
  Proven live against the v0.99.0 assets on two targets.
- **`nika mcp --transport http` — the Streamable HTTP transport.** The
  managed-MCP hosts stdio closed on get their wire: POST-only,
  conformant-minimal per MCP rev 2025-11-25 (single
  `application/json` response · `202` notifications · `405` on the
  push-stream GET and session DELETE — a read-only, stateless audit
  server), origin-gated on every request (anti DNS-rebinding),
  loopback bind by default, optional constant-time bearer via
  `NIKA_MCP_TOKEN`, the stdio pump's own 8 MiB ceiling — zero new
  dependencies (hand-rolled HTTP/1.1, deliberately sequential). The
  protocol dispatch was always a pure function; this is just the
  second pump.
- **The machine-consumer rung, completed** — three surfaces one release
  promised each other: `nika run --dry-run --json` emits ONE versioned
  plan object (`plan_version: 1` — waves resolved to task ids · per-task
  verbs · the cost ceiling · the permits contract · caller requirements)
  instead of refusing with "no machine dry-run form yet" (#332);
  `check --json` states the AFFIRMATIVE permits
  contract (`permits`: source `declared`/`floor` · the authored block
  spec-shaped · the tightest boundary the body statically needs — the
  same derivation `--infer-permits` prints · honesty notes) (#346); and
  every finding class rides ONE `findings[]` list — `{kind, gate,
  severity, message}` on every row, `code`/`docs_url` only where a
  canonical spec code exists, with `is_clean() ⇔ findings.is_empty()`
  pinned per-class so an eleventh class can never go silently missing
  (#331). Additive: `report_version` stays 1.
- **Bare `trace show`/`verify`/`outputs`/`flow` read the workspace
  latest** — the first thing typed after a run no longer demands the
  path the run card just printed; one stderr line names the pick, zero
  traces keeps the teaching exit-3, explicit paths are unchanged (#345).
- **`--help` speaks user** — the ADR/L3/clap vocabulary leaves the first
  screen; `check` findings print the same `fix: nika explain <CODE>`
  affordance run failures always had; `explain` grows concrete
  fix-forms for the high-traffic conformance codes (DAG-001/002/003 ·
  PARSE-002) and its footer teaches instead of reading inward (#145).

### Fixed

- **A declared write permit creates its tree — `nika:write` and
  `nika:chart` agree** (#433 · use-case battery 2026-07-11). The two
  artifact writers disagreed on a missing parent directory: chart
  auto-created it, write refused (`create_dirs: true` required). When
  `permits.fs.write` declares the tree (e.g. `state/**`), that permit is
  the author's standing declaration that the tree is theirs to make — so
  the guarded seam now creates the parent after a passing write-boundary
  check, and BOTH writers inherit it. Purely additive: a write to a new
  sub-directory inside a declared write permit succeeds without
  `create_dirs`; the un-declared engine-floor corner is unchanged (write
  keeps its safety gate, chart its seam). Option C of the #433 fork
  (operator brainstorm).
- **The secrets surface teaches its own fixes** (use-case battery
  2026-07-11). Three diagnostics that made the reader consult spec 01
  §egress now carry the answer: (1) an information-flow leak names the
  EXACT sanction — `leak into invoke (task shape) … · fix: sanction it —
  egress: [{ to: "nika:jq" }] on secrets.api_key` — the tool id computed
  per sink, in both the human report and the `--json` findings[]; (2) a
  dead egress sanction is refused at parse — `to:` outside the sink
  vocabulary (a destination HOST is the classic slip) can never match, so
  it no longer passes silently reading as declassified; the refusal lists
  the set (tool id · `exec` · `infer` · `agent` · `outputs`); (3) an
  unknown secret field with no near-miss, and the non-mapping egress
  refusal, now name the whole accepted set instead of hiding it behind an
  ellipsis. Additive `SecretLeak.sink_id` (non-exhaustive struct).
- **`nika run --task` honours the whole-file audit it promises.** The
  help says « the full workflow still audits (findings stay whole-file
  faithful) » — empirically the scoped re-check REPLACED the full report
  before the clean gate looked, so a PERMITS violation or a conformance
  error in a branch outside the target's ancestor cone neither printed
  nor blocked: the scoped run exited 0 over a file `nika check` refuses.
  The whole-file gate now fires BEFORE scoping (same findings rendering,
  same exit 2 as the unscoped run — a file must be sound even to
  regenerate one block), and the scoped re-check still gates after the
  cut (#411).
- **Teaching diagnostics from the 147-workflow new-user gauntlet** —
  arithmetic in a `${{ }}` guard teaches the route instead of the raw
  tokenizer error: `+ - * / %` → « `${{ }}` is a boolean guard, not a
  calculator (v0.1 CEL subset): compute the value in a `nika:jq` task »
  — one voice in the checker lexer (nika-tmpl) AND the runtime lexer
  (nika-cel), so an uncheck'd run learns the same lesson (#394). An
  unknown extract mode teaches the canonical route on the measured
  misses (`json` → `mode: jq` with `jq: "."` · `html` → `raw`/
  `selector` · `xml`/`rss`/`atom` → `feed`/`raw` — evidence-based rows
  only, never speculation), the same hint table behind the runtime
  Display and the check voice (#397).
- **gemini 2.5 dynamic thinking no longer burns structured budgets** —
  on a `schema:` call with authored `max_tokens` and no authored
  thinking budget, the wire bounds thinking (flash 0 · pro at the API's
  own 128 floor) so the author's tokens buy OUTPUT; an authored budget
  always wins, uncapped calls keep dynamic thinking, non-2.5 models
  never gain a surprise key. Landed twice honestly: the first fix gated
  on the provider id and never fired — caught by a local capture-server
  probe and the live battery, re-proven at the wire (#300).
- **Parse-fatal `check --json` answers JSON** — the most common CI case
  emitted plain text on stdout; now ONE object (`parse_fatal: true` ·
  a `findings[]` row carrying the NIKA-PARSE code) (#331).

### Changed

- **The SSRF floor speaks at `check` — down to the wire code** (battery
  finding F3 · #395): a literal `nika:fetch`/webhook-notify URL — or a
  `permits.net.http` entry — naming a target the always-on floor
  refuses (the `localhost` family per RFC 6761 · private ranges · cloud
  metadata) is now a check escape carrying `NIKA-SEC-005`, the exact
  code the run would emit, with or without a `permits:` block.
  Previously check blessed a workflow that could never run. One static
  host oracle (`nika_types::net::host_is_blocked`) now feeds
  `nika-http`, the browser navigate gate (which gains the
  metadata-NAME block that was open there), the escape scanner, and
  `--infer-permits` (which stops synthesizing floor-blocked grants —
  honesty note instead). Dynamic URLs and DNS names resolving privately
  stay the runtime `GuardedResolver`'s half (#403).
- **The `${{ }}` expression grammar lives in `nika-tmpl`** — descended
  from `nika-schema` at the 15k crate-size wall (the trace→dap
  precedent): the scanner and the language it scans are one home,
  `nika_schema::expression` re-exports verbatim, `ExprError` renders
  byte-identically through a hand-written Display, and the AST enums
  are deliberately exhaustive (a silent wildcard arm in a secrets
  walker would be an IFC hole).
- `nika-chart` joins the public-api coverage floor (44/44 admitted lib
  crates surface-locked).

## [0.99.0](https://github.com/supernovae-st/nika/compare/v0.98.0..v0.99.0) - 2026-07-10

### Added

- **`nika:image_fx` — deterministic artistic effects, the 26th builtin
  (stdlib §Media graduate #3).** The `image editing` deferred row comes
  home zero-dep: 15 op families (dither · palette · duotone · pixelate ·
  halftone · grain · vignette · chromatic aberration · scanlines ·
  glitch · ascii …) over a hand-rolled PNG codec (full RFC 1951 dynamic
  Huffman inflate · 5-filter decode · CRC), seeded and byte-identical —
  the recipe rides the PNG `tEXt` chunk (`image_fx/v1`), the artifact
  sha256 rides the trace chain, and a re-run with the same inputs
  idempotently skips.
- **`nika:chart` — deterministic chart artifacts, the 27th builtin
  (stdlib §Media graduate #4).** Rows + a semantic spec compile to
  byte-identical SVG (sha256 → trace chain): five closed types (bar ·
  line · area_band · scatter · heatmap), typed semantics (usd ·
  duration_ms · timestamp · category …), `out:` must end in `.svg` (the
  attestation surface) and `compile_to: vega_lite` writes the `.vl.json`
  sibling. Zero dependencies; parity proven byte-exact across
  architectures (wasm32-wasip1 ≡ aarch64).
- **`nika check --model <provider/model>`** — the static preview of the
  run override: the envelope is re-priced AS IF the flag replaced the
  file's default (per-task `model:` still wins), so what check shows IS
  what run will refuse or allow.
- **Egress to `outputs` — the workflow boundary earns its valve.** The
  capture-taint law is deliberate (the provider saw the key — its
  response is not provably clean), but a workflow that calls an
  authenticated API and RETURNS the result had no sanctioned path: the
  embedded `api-upload-and-create` template failed its own audit (the
  night battery's catch), documented as the one known gap. The gap
  closes the way its own note asked: `egress: [{ to: "outputs" }]` on
  the secret declassifies the workflow boundary itself — sink-only,
  secret-specific, never authorizes a send, default-deny when absent
  (spec 01-envelope §egress). KNOWN_GAP is empty: every embedded
  template now passes its own audit, with zero exceptions.
- **The MODELS rung — every `model:` must resolve in THIS binary.** The
  ladder validated tools but never models: a vendor-cataloged provider
  the resolver cannot drive (`azure/…`) and a bare model id
  (`gpt-5-turbo`) both audited green — the bare one even wore a
  conjured price. Both are findings now (exit 2) with the fix taught
  in-line; pricing refuses to price what cannot resolve (unpriced beats
  conjured); the `--json` payload carries `models_resolve` +
  `model_findings[]`; and the SAME law guards the MCP `nika_check` lane
  (`nika_providers::resolve_refusal` — the two machine lanes cannot
  disagree).
- **`nika wire` learns `opencode` and `hermes`** — the two ecosystems
  that natively read what `nika init` writes get first-class MCP wiring
  (`wire all` now covers 8 targets). OpenCode: project-local
  `opencode.json`, its own `mcp.nika` shape, idempotent merge. Hermes:
  `~/.hermes/config.yaml` under the Zed contract — create when missing,
  recognize current, otherwise hand back the exact snippet and leave a
  foreign YAML byte-identical.
- **OpenRouter calls carry the app-attribution pair** (`HTTP-Referer:
  https://nika.sh` + `X-Title: Nika`) — runs surface as Nika on the
  public rankings instead of an anonymous key. Openrouter-profile only;
  peers may 400 on surprise headers (proven both directions).
- **The plugin ships three slash commands** (`/nika:check` ·
  `/nika:explain` · `/nika:new`) — born under `.agents/plugins/nika/`
  per the marketplace mirror law; the commands read the `--json`
  payload, not the prose.
- **`nika explain --forecast` — learned truth before a run.** Duration,
  cost and risk priors computed from YOUR local traces (`.nika/traces/`)
  — deterministic stats, never a model call, never the network. The
  honesty ladder is a type: never-run says so · one run is « last run » ·
  2-4 runs earn a min–max range · p50/p90 bands are earned at n ≥ 5
  (Hyndman & Fan type-7, the numpy/R default). Costs compose the `≥`
  floor whenever unpriced spend participates — absence stays a dash,
  never `$0`. Retried-then-passed counts as flaky, never failed; cache
  hits and other-model runs are excluded from bands and named. The
  section auto-appears in `nika explain <file>` once 3 runs exist; the
  flag forces it, and `--json` gains a versioned `forecast` key
  (internally-tagged rungs — consumers tolerate unknown kinds).
- **The missing-input trap is taught at check time** — `nika check`
  prints an `[inputs]` HINT when a `nika:read` path that resolves
  statically (a literal, or one `${{ vars.X }}` with a literal default)
  does not exist here; never an error — the file may appear at run
  time, and anything dynamic is never guessed
  (`nika_schema::check::static_read_paths`, the pure half).
- **The release tarball is funnel-gated** — `scripts/ci/funnel-e2e.sh`
  plays the stranger's first path against the EXACT tarball about to
  ship (clean HOME · offline · content asserts); a broken first-run
  never uploads.

### Fixed

- **The run card advertises the full 64-hex chain head** — `trace
  verify` printed the whole sha256 while the run card truncated to 32,
  so the taught receipts loop could only prefix-match. Byte-exact `==`
  now closes it (CI-assertable).
- **The broken editor modeline names itself — cause, not symptom.** A
  weak copier de-comments the `# yaml-language-server:` line; YAML then
  fails at the first mapping (« line 14, `nika: v1` ») while the fault
  is line 1 — repair loops chased the symptom forever (0/13 measured on
  a 14B grid). Both forms now teach the fix on the offending line, and
  the class is mirrored spec-side as conformance fixtures 014/015 —
  writing them un-crashed the oracle's scan-failure path.
- **The cost floor prices the EFFECTIVE model.** The delegation idiom
  agents are taught (`--model <p/m> --max-cost-usd <usd>`) never met
  the pre-start refusal — the floor was computed from the file's model
  while the run used the override. The budget preflight now re-prices
  the effective envelope; the mock-override preview idiom still passes.
- **The PNG heatmap speaks the SVG's quantized bins** — the design pass
  quantized the SVG cells onto the legend's 8 shared bins while the PNG
  projection of the same recipe kept the continuous ramp; one shared
  fill law now feeds both surfaces (every legend swatch IS a color a
  cell can wear).
- **Fold verdicts are terminal kinds only** — a journal truncated after
  its opening line no longer surfaces `workflow_started` as if the
  crashed run had reached a state; `nika context` reads it as
  honestly-unknown.

### Changed

- **The chart design pass — per-mode palettes, computable, never
  eyeballed.** The six-check validator refuted « Okabe-Ito is CVD-safe
  on both modes » (4 slots outside the dark lightness band · the yellow
  at 1.29:1 on white): dark becomes a SELECTED palette (same seven
  hues, its own steps, all checks green against `#0f1318`; light
  re-steps one slot, yellow → gold), series and diverging bins ride CSS
  classes through the `prefers-color-scheme` seam (ONE byte-stable
  document renders both themes), heatmap cells quantize onto the
  legend's 8 shared bins with a 2px surface gap, bars cap at 24px with
  a rounded data-end and square baseline, and the diverging midpoint
  goes near-surface graphite in dark — « no change » must recede, never
  glow. A BOTH-lists-style test guards the style block against palette
  drift.
- **`nika-dap::stats::conformal_upper` — the forecast's first THEOREM.**
  A distribution-free, finite-sample upper prediction bound (split
  conformal, order-statistic form): for exchangeable runs the NEXT one
  falls at or below the k-th order statistic with probability ≥ k/(n+1),
  exactly k/(n+1) for continuous data (arXiv:2411.11824 Theorem 3.2). The
  level arrives as a rational (the f64 route mis-computes the exact
  feasibility frontier — 0.9·10 ceils to 10); nine runs earn a
  guaranteed 90% bound, nineteen earn 95%. Proven by a deterministic
  leave-one-out property test that COUNTS the theorem, never samples
  it. Renderer wiring lands with forecast-R3.
- **`nika-dap` is its own crate — the trace-forensics plane has one
  home.** The DAP replay server moves out of `nika-cli` (the crate sat
  at 98.9% of its size cap before the forecast landed), and the seams
  every forensic reader shares descend with it: the tolerant NDJSON
  reader (`recover_events`), the tamper-evidence chain walk
  (`chain::walk` + ONE `CHAIN_GENESIS` the sink now imports — three
  private sha256 copies unified), and the source-identity hashes.
  `nika-cli` re-exports every seam at its old path — zero behavior
  change, `nika dap` answers exactly as before.

## [0.98.0](https://github.com/supernovae-st/nika/compare/v0.97.0..v0.98.0) - 2026-07-08

**Structured output goes native, the checker closes its gaps — and the
engine gets its one lexer.** Four arcs land together: the answer now
rides each provider's own grammar; `nika check` catches what used to
fail at run; costs stopped lying last cycle and now the budget can't be
dodged; and the `${{ }}` scanner becomes a crate both the checker and
the runtime share — parity by construction.

### The native-answer arc

- **Anthropic native structured output** — `output_config.format`
  replaces the instruction fallback on claude models that support it.
- **Gemini rides `responseJsonSchema`** — the lossy OpenAPI converter
  dies; the author's JSON Schema reaches the wire as written.
- **OpenAI strict-mode honesty** — schemas carrying `not`/`allOf` are
  stripped and flattened at the wire instead of 400ing; gpt-5/o-series
  get `max_completion_tokens` (legacy models and every compatible peer
  keep `max_tokens` — and the two spellings are ONE logical key, a raw
  extras `max_tokens` can no longer ride alongside the routed one).
- **Capability is a PROVIDER fact** — deepseek has no `json_schema`;
  the profile says so instead of failing live. `nika doctor` names the
  clouds that fall back to instruction mode.
- **The coercion ladder** — SAP-lite repair (case-fold enums · string
  numbers · singleton arrays) before any PAID retry; retry-loop
  economics send a numbered repair list and fast-fail on truncation.

### The audit-before-run arc

- **The compact block-sequence bomb is dead (CRITICAL)** — `- `×4000 on
  one line used to stack-overflow-ABORT every `nika check`/`run` and
  the LSP with them; the dash-run cap catches it as NIKA-PARSE-001. The
  LSP additionally proves it SERVES the bomb as a document and keeps
  answering (e2e), and floors mid-char offsets instead of panicking.
- **`max_turns` out of 1-1000** and **`for_each` over a typed
  non-array var** are check-time findings now, not run-time surprises.
- **`env:` binds at run** — a green check referencing `${{ env.X }}`
  used to fail with NIKA-VAR-001 because the runtime never bound the
  namespace it was checked against. The exact parity class
  audit-before-run exists to prevent; threaded end-to-end (gates ·
  pause payloads · resume identity — an env change re-keys, never
  wrong-skips).
- **New hints**: `exec-json-capture` (a `capture: structured` task
  whose bindings parse `.stdout | fromjson` and read nothing else of
  the record — use `capture: stdout` so a failing helper errors as
  NIKA-EXEC-001 instead of becoming data) · `native-first` (the check
  names the builtin/MCP path an exec shells out to) ·
  `schema-portability` (the grammar-blind keywords named).
- **A dirty MCP check is `isError:true`** — the agent repair loop
  (template → fill → check → repair) triggers again.

### nika-tmpl — the 41st crate

The `${{ … }}` island lexer, extracted from its two hand-duplicated
copies (the drift already shipped one check-passed/run-broke bug) into
a zero-dependency L0 leaf both consumers now share. Quote-aware close,
`\${{` escape, byte-spans, AST-free. Mutation 46/51 (90.2%) with the 5
survivors certified unkillable in-spec; parity pre-proven byte-identical
across ~337k exhaustive inputs.

### And

- The wizard escapes user strings into YAML (a backslash in the intent
  no longer scaffolds a file that fails its own check), survives spaced
  filenames, and never promises what the file doesn't carry — with PTY
  e2e coverage driving the real conversation.
- fetch vNext: form posts · multipart · bounded traverse, then the
  hardening pass (multipart cap unbypassable · cross-origin redirects
  re-pin · userinfo redacted · robots.txt bounded).
- The LSP honors `$/cancelRequest` — a request the editor discarded
  (superseded completion · rapid typing) dies before its analysis runs.
- `NIKA_<ID>_BASE_URL` reaches the CLOUDS — the locked long-tail hatch
  (D-2026-06-10-N2) gets its operator surface: point any cloud profile
  at an OpenAI-compatible endpoint (an EU host · a corporate gateway)
  with zero catalog change, paired with the scoped `NIKA_<ID>_API_KEY`.
- exec feeds stdin concurrently (a large input can no longer deadlock)
  · the runtime honors the documented key ladder (`NIKA_` prefix wins)
  · `nika init`'s AGENTS.md teaches the live clap tree · `nika welcome`
  + `nika explain <file>` — the 30-seconds narrative layer.

**Nika never lies about costs** (the cost arc, continued from 0.97):
the pricing catalog identifies itself, the math prices what providers
actually billed, agents stop hiding their LLM spend, `unknown` always
says WHY, and the operator gets a real budget lever.

### The detail (every PR)

### 🆕 Crates admitted
- **nika-pck-manifest** — Admit to workspace — all 12 gates passed ([#306](https://github.com/supernovae-st/nika/issues/306)) ([2b93da863](https://github.com/supernovae-st/nika/commit/2b93da863d94185506c74149a4f97f6826f66d1c)) ([#306](https://github.com/supernovae-st/nika/pull/306))
- **nika-tmpl** — Admit to workspace — the ONE ${{ }} island lexer, all 12 gates passed ([#302](https://github.com/supernovae-st/nika/issues/302)) ([9d80f6e22](https://github.com/supernovae-st/nika/commit/9d80f6e2225a558c0ae1a06c03d25f6cbadc2fe7)) ([#302](https://github.com/supernovae-st/nika/pull/302))

### ✨ Features
- **check** — Native-first — the check names the native path ([#258](https://github.com/supernovae-st/nika/issues/258)) ([ab361ba0d](https://github.com/supernovae-st/nika/commit/ab361ba0d259021dc4f7db9d2582e8e7ec1529ba)) ([#258](https://github.com/supernovae-st/nika/pull/258))
- **check** — Schema-portability hint — the grammar-blind keywords named ([#262](https://github.com/supernovae-st/nika/issues/262)) ([56af8f124](https://github.com/supernovae-st/nika/commit/56af8f124d172c31d3ac0adb9979764a30dbadee)) ([#262](https://github.com/supernovae-st/nika/pull/262))
- **cost** — Cost intelligence — provenance, cache-aware math, agent split, budget guard ([#271](https://github.com/supernovae-st/nika/issues/271)) ([0b571b485](https://github.com/supernovae-st/nika/commit/0b571b48524f3b1dcc2fb3355b54141bd6983c5d)) ([#271](https://github.com/supernovae-st/nika/pull/271))
- **cost** — Billed-then-failed spend reaches the frame, the totals, and the budget gate ([#288](https://github.com/supernovae-st/nika/issues/288)) ([226ada754](https://github.com/supernovae-st/nika/commit/226ada754164c9d3da47654d70445b1b5e7ccaec)) ([#288](https://github.com/supernovae-st/nika/pull/288))
- **doctor** — The health surface names the instruction-fallback clouds ([#287](https://github.com/supernovae-st/nika/issues/287)) ([5ab624c7b](https://github.com/supernovae-st/nika/commit/5ab624c7b3ceb5f1dfa01f987de542923567168c)) ([#287](https://github.com/supernovae-st/nika/pull/287))
- **lsp** — $/cancelRequest — stale requests die before they compute ([#303](https://github.com/supernovae-st/nika/issues/303)) ([2ca467e3b](https://github.com/supernovae-st/nika/commit/2ca467e3b93f23fba7ded93f5833ec575dea4e2a)) ([#303](https://github.com/supernovae-st/nika/pull/303))
- **nika-builtin** — Fetch vNext — form · multipart · bounded traverse ([#257](https://github.com/supernovae-st/nika/issues/257)) ([534c0999a](https://github.com/supernovae-st/nika/commit/534c0999a5d80339a5a3940ac98892deba6bdde4)) ([#257](https://github.com/supernovae-st/nika/pull/257))
- **nika-cli** — Wizard wow — the conversation speaks the display seam ([#261](https://github.com/supernovae-st/nika/issues/261)) ([24fcf43c4](https://github.com/supernovae-st/nika/commit/24fcf43c422913665768fb606481d3c6d5b2170a)) ([#261](https://github.com/supernovae-st/nika/pull/261))
- **nika-cli** — The scaffolded agents.md teaches the live clap tree ([#281](https://github.com/supernovae-st/nika/issues/281)) ([d47f3959c](https://github.com/supernovae-st/nika/commit/d47f3959cdec0902df271dab37fa9f7c91a9596d)) ([#281](https://github.com/supernovae-st/nika/pull/281))
- **nika-cli** — Nika welcome + nika explain <file> — the 30-seconds narrative layer ([#298](https://github.com/supernovae-st/nika/issues/298)) ([5316bf7e3](https://github.com/supernovae-st/nika/commit/5316bf7e3c69f362c6f6270263ed531941e44ae6)) ([#298](https://github.com/supernovae-st/nika/pull/298))
- **nika-cli** — The long-tail hatch gets its surface — NIKA_<ID>_BASE_URL for clouds ([#304](https://github.com/supernovae-st/nika/issues/304)) ([25db7137d](https://github.com/supernovae-st/nika/commit/25db7137dddbe6fdb7782663d1a07096cafb3bad)) ([#304](https://github.com/supernovae-st/nika/pull/304))
- **nika-cli** — The mirror speaks the colour seam — and shows the language ([#305](https://github.com/supernovae-st/nika/issues/305)) ([46511025d](https://github.com/supernovae-st/nika/commit/46511025da09fc9e585e083d01f40260731e4655)) ([#305](https://github.com/supernovae-st/nika/pull/305))
- **nika-lsp** — Hostile-input hardening — the #282 bomb served, mid-char offsets floored ([#296](https://github.com/supernovae-st/nika/issues/296)) ([f03eacc56](https://github.com/supernovae-st/nika/commit/f03eacc562f098c01e76777dc21dc5736c4482f3)) ([#296](https://github.com/supernovae-st/nika/pull/296))
- **nika-schema** — Exec-json-capture hint — capture stdout when bindings parse it ([#295](https://github.com/supernovae-st/nika/issues/295)) ([75d1602e6](https://github.com/supernovae-st/nika/commit/75d1602e6fd33286a285df11102393ecd0c70f59)) ([#295](https://github.com/supernovae-st/nika/pull/295))
- **providers** — Gemini rides responseJsonSchema — the OpenAPI converter dies ([#269](https://github.com/supernovae-st/nika/issues/269)) ([98a363b49](https://github.com/supernovae-st/nika/commit/98a363b499324eb1630c47c8ad9c859a2b351df7)) ([#269](https://github.com/supernovae-st/nika/pull/269))
- **providers** — Anthropic native structured output — output_config.format ([#264](https://github.com/supernovae-st/nika/issues/264)) ([c66c20231](https://github.com/supernovae-st/nika/commit/c66c20231aee47cdb8090e91a660268fada5c9cf)) ([#264](https://github.com/supernovae-st/nika/pull/264))
- **verb-infer** — Retry-loop economics — numbered repair list + truncation fast-fail ([#268](https://github.com/supernovae-st/nika/issues/268)) ([a92d383d2](https://github.com/supernovae-st/nika/commit/a92d383d2ce430990cb1e86f27a5e5b4d7e7ae85)) ([#268](https://github.com/supernovae-st/nika/pull/268))
- **verb-infer** — The coercion ladder — SAP-lite repair before a paid retry ([#280](https://github.com/supernovae-st/nika/issues/280)) ([a79da7963](https://github.com/supernovae-st/nika/commit/a79da79638f8c913a7d1daf710e9894e2d2decec)) ([#280](https://github.com/supernovae-st/nika/pull/280))

### 🐛 Bug Fixes
- **ci** — The coherence bot's openvsx probe points at the real extension ([#260](https://github.com/supernovae-st/nika/issues/260)) ([bfbe083ed](https://github.com/supernovae-st/nika/commit/bfbe083ed4b6d7dc8ff9b76fecd1668a9c1d88c2)) ([#260](https://github.com/supernovae-st/nika/pull/260))
- **exec-runner** — Feed stdin concurrently so a large input cannot deadlock ([#256](https://github.com/supernovae-st/nika/issues/256)) ([407252d4d](https://github.com/supernovae-st/nika/commit/407252d4d20a985d71229f01bf098162668458b8)) ([#256](https://github.com/supernovae-st/nika/pull/256))
- **nika-builtin** — Fetch vNext hardening — the crawl stops leaking, over-reading, truncating ([#272](https://github.com/supernovae-st/nika/issues/272)) ([31475f574](https://github.com/supernovae-st/nika/commit/31475f57404499f3b5d4964e0e950f8add18129f)) ([#272](https://github.com/supernovae-st/nika/pull/272))
- **nika-catalog** — Declare fetch traverse — the merged binary refused every crawl ([#263](https://github.com/supernovae-st/nika/issues/263)) ([56a6a9172](https://github.com/supernovae-st/nika/commit/56a6a9172117977442e95284bc95a1f44b69d030)) ([#263](https://github.com/supernovae-st/nika/pull/263))
- **nika-cli** — Wizard coherence — never promise what the file doesn't carry ([#266](https://github.com/supernovae-st/nika/issues/266)) ([ec245c32e](https://github.com/supernovae-st/nika/commit/ec245c32e815d8050a9bf12ce40fc71acd8dcd06)) ([#266](https://github.com/supernovae-st/nika/pull/266))
- **nika-cli** — The night batch — the why reaches the operator ([#265](https://github.com/supernovae-st/nika/issues/265)) ([758023c72](https://github.com/supernovae-st/nika/commit/758023c72d7fcc6c8f5c6ec7f25fe292d7b8b728)) ([#265](https://github.com/supernovae-st/nika/pull/265))
- **nika-cli** — The budget's unbounded warning names each reason, not the fixed disjunction ([#273](https://github.com/supernovae-st/nika/issues/273)) ([9282edbe2](https://github.com/supernovae-st/nika/commit/9282edbe2f985a801b4e3934d94276654c29aeb3)) ([#273](https://github.com/supernovae-st/nika/pull/273))
- **nika-cli** — The wizard's hand-off commands survive a spaced filename ([#275](https://github.com/supernovae-st/nika/issues/275)) ([7921bf268](https://github.com/supernovae-st/nika/commit/7921bf268def332789325d5cdb853b3f72a38765)) ([#275](https://github.com/supernovae-st/nika/pull/275))
- **nika-cli** — The wizard escapes user strings into YAML ([#276](https://github.com/supernovae-st/nika/issues/276)) ([c7cbbb588](https://github.com/supernovae-st/nika/commit/c7cbbb588a1aff4b761dd710b859a36f3dab74fd)) ([#276](https://github.com/supernovae-st/nika/pull/276))
- **nika-cli** — The runtime honors the documented key ladder — NIKA_ prefix wins ([#286](https://github.com/supernovae-st/nika/issues/286)) ([4395ea9ca](https://github.com/supernovae-st/nika/commit/4395ea9ca0710a2c417cbdd7842e86f4207ee710)) ([#286](https://github.com/supernovae-st/nika/pull/286))
- **nika-mcp** — A dirty check is isError:true — the repair loop must trigger ([#270](https://github.com/supernovae-st/nika/issues/270)) ([49a3fff8f](https://github.com/supernovae-st/nika/commit/49a3fff8fa417a3f0053e42fa17ac2041cd42391)) ([#270](https://github.com/supernovae-st/nika/pull/270))
- **nika-providers** — Gpt-5/o-series budget key — max_completion_tokens on the openai wire ([#294](https://github.com/supernovae-st/nika/issues/294)) ([90a5615c1](https://github.com/supernovae-st/nika/commit/90a5615c12fdbc76f0aae26c86597198200e51ae)) ([#294](https://github.com/supernovae-st/nika/pull/294))
- **nika-runtime** — Bind the envelope env namespace — a green check must run ([#293](https://github.com/supernovae-st/nika/issues/293)) ([9ff72e3f8](https://github.com/supernovae-st/nika/commit/9ff72e3f8b4114b3417bd1f30ce2e3e4e6e74b06)) ([#293](https://github.com/supernovae-st/nika/pull/293))
- **nika-schema** — Native-first rule 004 gets the bare-head guard its siblings have ([#278](https://github.com/supernovae-st/nika/issues/278)) ([6fc4ed65e](https://github.com/supernovae-st/nika/commit/6fc4ed65e19908b2a97dcfb6820a8cad8b103050)) ([#278](https://github.com/supernovae-st/nika/pull/278))
- **nika-schema** — Check catches an out-of-range max_turns — audit before run ([#279](https://github.com/supernovae-st/nika/issues/279)) ([89624f3a5](https://github.com/supernovae-st/nika/commit/89624f3a5984075e2b90e0ce052084aa404263e0)) ([#279](https://github.com/supernovae-st/nika/pull/279))
- **nika-schema** — A compact block-sequence bomb no longer aborts the process (CRITICAL) ([#282](https://github.com/supernovae-st/nika/issues/282)) ([e3c9a7cc8](https://github.com/supernovae-st/nika/commit/e3c9a7cc846690d9eecf04dd6821ad8bfbcda200)) ([#282](https://github.com/supernovae-st/nika/pull/282))
- **nika-schema** — Check catches for_each over a typed non-array var — audit before run ([#284](https://github.com/supernovae-st/nika/issues/284)) ([b18e7842e](https://github.com/supernovae-st/nika/commit/b18e7842e7c1f084eaf9ada697d8bb8929ae918a)) ([#284](https://github.com/supernovae-st/nika/pull/284))
- **nika-types** — Cost.rs imports String from alloc — the no_std powerset holds ([#297](https://github.com/supernovae-st/nika/issues/297)) ([48c175ac4](https://github.com/supernovae-st/nika/commit/48c175ac444cad8642e76364f49a619826f853aa)) ([#297](https://github.com/supernovae-st/nika/pull/297))
- **providers** — Openai strict rejects not/allOf — strip and flatten at the wire ([#267](https://github.com/supernovae-st/nika/issues/267)) ([9ec2c0bb2](https://github.com/supernovae-st/nika/commit/9ec2c0bb24c8057ffcb8d79ad654d32ff121d114)) ([#267](https://github.com/supernovae-st/nika/pull/267))
- **providers** — Capability is a PROVIDER fact — deepseek has no json_schema ([#283](https://github.com/supernovae-st/nika/issues/283)) ([39a2a46d2](https://github.com/supernovae-st/nika/commit/39a2a46d22a8ad27b4632e7de514f12958300b84)) ([#283](https://github.com/supernovae-st/nika/pull/283))

### 📚 Documentation
- **adr** — Ratify adr-094 — fci-004 reserved-core catches the d4 taxonomy ([#292](https://github.com/supernovae-st/nika/issues/292)) ([18a95c9aa](https://github.com/supernovae-st/nika/commit/18a95c9aa06c21e72deb927c32d64ac50ecd6874)) ([#292](https://github.com/supernovae-st/nika/pull/292))
- **nika-cli** — --max-cost-usd help names its three real limits at the point of use ([#274](https://github.com/supernovae-st/nika/issues/274)) ([011667feb](https://github.com/supernovae-st/nika/commit/011667feb2d84f735ddd748c141cfb0feaaa886e)) ([#274](https://github.com/supernovae-st/nika/pull/274))
- **pack** — Surgical vendor — provider count catches canon (16) ([#259](https://github.com/supernovae-st/nika/issues/259)) ([242377674](https://github.com/supernovae-st/nika/commit/242377674d47613e4ef52ecd5a97e20027da72e3)) ([#259](https://github.com/supernovae-st/nika/pull/259))

### 🧪 Tests
- **nika-cli** — The own-corpus law is a ratchet — every template audits clean ([#277](https://github.com/supernovae-st/nika/issues/277)) ([fabf3acb7](https://github.com/supernovae-st/nika/commit/fabf3acb7dae7b021c507709429d042ccc234d9e)) ([#277](https://github.com/supernovae-st/nika/pull/277))
- **nika-cli** — Pty e2e — the wizard conversation gets executable coverage ([#285](https://github.com/supernovae-st/nika/issues/285)) ([67ee9f2e3](https://github.com/supernovae-st/nika/commit/67ee9f2e38f60d866be51588a5369cef5ce1fdbb)) ([#285](https://github.com/supernovae-st/nika/pull/285))
- **nika-cli** — Pty hardening — raii dirs, transcript tee, the third door ([#289](https://github.com/supernovae-st/nika/issues/289)) ([d097adb05](https://github.com/supernovae-st/nika/commit/d097adb0503d64244254404740e024d6e09f9812)) ([#289](https://github.com/supernovae-st/nika/pull/289))
- **scripts** — Rail C — the live structured-output battery (P0) ([#299](https://github.com/supernovae-st/nika/issues/299)) ([9649d8421](https://github.com/supernovae-st/nika/commit/9649d8421a83c2176b5c839186f675e1c5857d72)) ([#299](https://github.com/supernovae-st/nika/pull/299))

### 🧹 Chore
- **ci** — Public-api baseline — Profile::supports_response_format joins ([#290](https://github.com/supernovae-st/nika/issues/290)) ([e1e934eea](https://github.com/supernovae-st/nika/commit/e1e934eea6ca872549e619af1620644fcad289c1)) ([#290](https://github.com/supernovae-st/nika/pull/290))
- **ci** — Public-api baseline — the Profile fn rendering from the ubuntu artifact ([c6f4f0b54](https://github.com/supernovae-st/nika/commit/c6f4f0b54f084022389c7b8f04f7d644ed0cd77d))

## [0.97.0](https://github.com/supernovae-st/nika/compare/v0.96.0..v0.97.0) - 2026-07-07

**The run becomes evidence.** 0.96 made the run a place you can visit;
0.97 makes it a record you can trust — and prices it before you pay.

### ✨ Features

- **The journal is tamper-evident** — every line carries a hash chain;
  `nika trace verify` walks it and names the first broken link ([PR 237](https://github.com/supernovae-st/nika/pull/237)).
  Export and replay check the chain BEFORE trusting the data ([PR 238](https://github.com/supernovae-st/nika/pull/238)).
- **`nika trace reproduce`** — is this run reproducible, and WHY not:
  the verdict names every non-deterministic ingredient ([PR 241](https://github.com/supernovae-st/nika/pull/241)).
- **The journal attests its engine** — `engine_version` + platform ride
  `workflow_started`: a failure report says WHICH binary WHERE ([PR 235](https://github.com/supernovae-st/nika/pull/235)).
- **Models are priced before the first run** — the vendored catalog
  refreshed from models.dev (62 rules → 602 · [PR 233](https://github.com/supernovae-st/nika/pull/233)) and `check --json`
  carries per-model rates ([PR 236](https://github.com/supernovae-st/nika/pull/236)): the editor preflight (nika-vscode
  0.97.3) shows `$in/$out per 1M` with zero spend — it lights up on this tag.
- **`check` IS the dry-run** — the plan names WHAT dispatches WHEN:
  waves, gates, blast radius, before anything runs ([PR 245](https://github.com/supernovae-st/nika/pull/245)).
- **`doctor --json`** — the diagnosis speaks machine ([PR 244](https://github.com/supernovae-st/nika/pull/244)).
- **Guided onboarding** — bare `nika init`/`nika new` converse on a
  terminal instead of demanding flags ([PR 253](https://github.com/supernovae-st/nika/pull/253)).
- **Envelope-model did-you-mean resolves like any surface** — deep/019 ([PR 239](https://github.com/supernovae-st/nika/pull/239)).

### 🐛 Bug Fixes

- **The trust surface stops lying at its edges** — the rust-pro batch
  hardens verify/export/replay edge cases ([PR 248](https://github.com/supernovae-st/nika/pull/248)),
  and the chain writer earns its anchor — the writer-side review
  batch ([PR 252](https://github.com/supernovae-st/nika/pull/252)).
- **infer bills every round-trip, not just the last** — multi-cycle
  runs report their true cost ([PR 250](https://github.com/supernovae-st/nika/pull/250)).
- **The drift warn tells a re-encode from an edit** — CRLF/BOM sources
  record `workflow_sha256_lf` (the LF normal form); dap replay compares
  content, not bytes — an editor re-encode no longer cries
  « workflow changed » ([PR 247](https://github.com/supernovae-st/nika/pull/247)).
- **dap launch gets the file-side bounds** — 64 MiB cap, regular files
  only (a device/FIFO hung the adapter), and a torn-tail journal says
  « valid prefix only » instead of replaying silently ([PR 242](https://github.com/supernovae-st/nika/pull/242)).
- **infer repairs near-miss JSON** and reaches longer stall cycles ([PR 243](https://github.com/supernovae-st/nika/pull/243)).
- **Strict `json_schema` claims travel only where earned** ([PR 246](https://github.com/supernovae-st/nika/pull/246)).

### 📦 Pack

- Vendored spec pack synced to nika-spec main — `nika:edit count:` is a
  strict non-negative integer: the string `"2"` is a loud arg error,
  never a silent replace-all (spec [PR 26](https://github.com/supernovae-st/nika-spec/pull/26)).

## [0.96.0](https://github.com/supernovae-st/nika/compare/v0.95.0..v0.96.0) - 2026-07-06

### ✨ Highlights

- **`nika dap` — time-travel replay debugging** ([PR 225](https://github.com/supernovae-st/nika/pull/225) ·
  drift warn [PR 227](https://github.com/supernovae-st/nika/pull/227)) — a Debug
  Adapter over the run journal: breakpoints in your `.nika.yaml`, step
  forward AND backward through task settles, recorded outputs in the
  Variables pane. Replay never re-executes. The editor extension's F5
  integration (shipped dark in nika-vscode 0.97) lights up on this tag.
- **`nika trace export` — every OTel tool becomes a nika viewer**
  ([PR 221](https://github.com/supernovae-st/nika/pull/221) · true durations
  [PR 223](https://github.com/supernovae-st/nika/pull/223)) — project any
  journal to OTLP/JSON lines: drag into Jaeger, POST to Grafana/Langfuse
  (cost rides `gen_ai.usage.cost`). Local file, zero collector.
- **The caller contract** ([PR 213](https://github.com/supernovae-st/nika/pull/213)) —
  `check --json` gains a `requirements` section (models · keys · secrets ·
  env the run will need) so editors and CI state blockers BEFORE any token.
- **Unknown fields teach** ([PR 228](https://github.com/supernovae-st/nika/pull/228)) —
  a typo'd verb key gets the did-you-mean: `` unknown field `infr` … — did
  you mean `infer`? `` on every surface (check · JSON · LSP · MCP).
- **The run knows its story** — `workflow_sha256` on workflow_started
  ([PR 210](https://github.com/supernovae-st/nika/pull/210)) powers drift-aware
  replay; skips and cancels say WHY (`when` + `blocked_by` ·
  [PR 211](https://github.com/supernovae-st/nika/pull/211)).

### 🐛 Fixes & hardening

- `doctor --ping` v2: true 300ms cap, sovereign order, parallel sweep
  ([PR 229](https://github.com/supernovae-st/nika/pull/229)).
- Strict boolean args close the `overwrite:` data-loss footgun
  ([PR 220](https://github.com/supernovae-st/nika/pull/220));
  `nika:convert` gains the opt-in CSV formula-injection guard
  ([PR 217](https://github.com/supernovae-st/nika/pull/217)).
- Trace export renders TRUE durations — the settle-burst law
  ([PR 223](https://github.com/supernovae-st/nika/pull/223)).

### 🏗️ Internals

- The ecosystem coherence bot: cross-repo pins checked nightly, registry
  watched, self-testing ([PR 216](https://github.com/supernovae-st/nika/pull/216)/[PR 222](https://github.com/supernovae-st/nika/pull/222)/[PR 230](https://github.com/supernovae-st/nika/pull/230)).
- CI gets the full integration battery ([PR 218](https://github.com/supernovae-st/nika/pull/218));
  the agent surface round-trips in bin_smoke — tools/call + the LSP framed
  wire ([PR 219](https://github.com/supernovae-st/nika/pull/219)).
- The pack re-vendors nika-spec main (formula_guard prose · the embedded
  jaq's honest @csv note · nika-spec#22).

### ✨ Features
- **nika-builtin** — Nika:convert formula_guard — opt-in CSV injection guard ([#217](https://github.com/supernovae-st/nika/issues/217)) ([3edb94454](https://github.com/supernovae-st/nika/commit/3edb94454ccd2d361bbf737cb6441bedbbb790eb)) ([#217](https://github.com/supernovae-st/nika/pull/217))
- **nika-cli** — Trace export — every OTel tool becomes a nika viewer ([#221](https://github.com/supernovae-st/nika/issues/221)) ([d6e6f9ee5](https://github.com/supernovae-st/nika/commit/d6e6f9ee5540e63492a5add3aceb66cdc275415f)) ([#221](https://github.com/supernovae-st/nika/pull/221))
- **nika-cli** — Nika dap — time-travel replay debugging over the run journal ([#225](https://github.com/supernovae-st/nika/issues/225)) ([b844c316c](https://github.com/supernovae-st/nika/commit/b844c316cd45a7ef025f4755fd6fe497f0e24b0a)) ([#225](https://github.com/supernovae-st/nika/pull/225))
- **nika-cli** — Dap launch warns when the workflow drifted since the run ([#227](https://github.com/supernovae-st/nika/issues/227)) ([05cdaab1e](https://github.com/supernovae-st/nika/commit/05cdaab1ec68217f3a7fb03841dbe5f7720767d4)) ([#227](https://github.com/supernovae-st/nika/pull/227))
- **nika-runtime** — The run knows its source — workflow_sha256 on workflow_started ([#210](https://github.com/supernovae-st/nika/issues/210)) ([74f53d443](https://github.com/supernovae-st/nika/commit/74f53d443f9c768a8708843ce9055ffa9af895d0)) ([#210](https://github.com/supernovae-st/nika/pull/210))
- **nika-runtime** — The skip and the cancel say WHY — when + blocked_by ([#211](https://github.com/supernovae-st/nika/issues/211)) ([dc7b31b4c](https://github.com/supernovae-st/nika/commit/dc7b31b4c6db578408630a898f28c6b256a38867)) ([#211](https://github.com/supernovae-st/nika/pull/211))
- **nika-schema** — The caller contract — a requirements section on check ([#213](https://github.com/supernovae-st/nika/issues/213)) ([e3a9bcb4a](https://github.com/supernovae-st/nika/commit/e3a9bcb4ab06421c6812fa57950482bb60f3cf68)) ([#213](https://github.com/supernovae-st/nika/pull/213))
- **nika-schema** — Unknown fields carry the did-you-mean — PARSE-005 teaches ([#228](https://github.com/supernovae-st/nika/issues/228)) ([be92b54dc](https://github.com/supernovae-st/nika/commit/be92b54dc0ade1e0d7995595733ab5d5a21c3e46)) ([#228](https://github.com/supernovae-st/nika/pull/228))

### 🐛 Bug Fixes
- **ci** — The integration suites finally run somewhere — CI gets the full battery ([#218](https://github.com/supernovae-st/nika/issues/218)) ([5160a8216](https://github.com/supernovae-st/nika/commit/5160a8216aa804fb9f7ef6238a51569fb14823cc)) ([#218](https://github.com/supernovae-st/nika/pull/218))
- **nika-builtin** — Strict boolean args — close the overwrite data-loss footgun ([#220](https://github.com/supernovae-st/nika/issues/220)) ([2fdb057dc](https://github.com/supernovae-st/nika/commit/2fdb057dcff5cdf98fafa949e5fc30bae13a9861)) ([#220](https://github.com/supernovae-st/nika/pull/220))
- **nika-builtin** — Strict count arg — close the edit over-edit footgun ([#231](https://github.com/supernovae-st/nika/issues/231)) ([59ff98a05](https://github.com/supernovae-st/nika/commit/59ff98a05bed02ee071151812bf565967a8a3238)) ([#231](https://github.com/supernovae-st/nika/pull/231))
- **nika-cli** — Trace export renders TRUE durations — the settle-burst law ([#223](https://github.com/supernovae-st/nika/issues/223)) ([412e32c27](https://github.com/supernovae-st/nika/commit/412e32c276352e0b89c6ed98032fd4904c4c9a6a)) ([#223](https://github.com/supernovae-st/nika/pull/223))
- **nika-cli** — Doctor --ping — true cap, sovereign order, parallel sweep ([#229](https://github.com/supernovae-st/nika/issues/229)) ([aa7366f17](https://github.com/supernovae-st/nika/commit/aa7366f17a7dc5ad8518768522a4cae732acdae9)) ([#229](https://github.com/supernovae-st/nika/pull/229))

### 📚 Documentation
- **adr** — Adr-106 — nika add, the registry client verb (proposed) ([#226](https://github.com/supernovae-st/nika/issues/226)) ([018b4673a](https://github.com/supernovae-st/nika/commit/018b4673a938b9e7c7c143f181152debdf283a5b)) ([#226](https://github.com/supernovae-st/nika/pull/226))
- **readme** — Shared workflows have a home — the registry pointer ([#224](https://github.com/supernovae-st/nika/issues/224)) ([047b8b838](https://github.com/supernovae-st/nika/commit/047b8b838959a22b327759a0c366c8a5ecc26001)) ([#224](https://github.com/supernovae-st/nika/pull/224))

### 🧪 Tests
- **bin_smoke** — The agent surface round-trips — tools/call + the LSP wire ([#219](https://github.com/supernovae-st/nika/issues/219)) ([6ef0d67f9](https://github.com/supernovae-st/nika/commit/6ef0d67f9c5d137e61ea7acd2ef6e88cca107483)) ([#219](https://github.com/supernovae-st/nika/pull/219))

### 📦 Build
- **coherence** — The registry is a watched first-class citizen ([#222](https://github.com/supernovae-st/nika/issues/222)) ([0110da503](https://github.com/supernovae-st/nika/commit/0110da50311ed16287d24e0705920a28fc001c61)) ([#222](https://github.com/supernovae-st/nika/pull/222))
- **coherence** — The bot self-tests before every run — and survives pre-releases ([#230](https://github.com/supernovae-st/nika/issues/230)) ([bba71ebf6](https://github.com/supernovae-st/nika/commit/bba71ebf6caa19aefc14b7e33ee9e9e30e9a1c70)) ([#230](https://github.com/supernovae-st/nika/pull/230))
- The ecosystem coherence bot — cross-repo pins, checked nightly ([#216](https://github.com/supernovae-st/nika/issues/216)) ([325ed0a8b](https://github.com/supernovae-st/nika/commit/325ed0a8b598cdbdca1265b0ea39e312448bda4b)) ([#216](https://github.com/supernovae-st/nika/pull/216))

## [0.95.0](https://github.com/supernovae-st/nika/compare/v0.94.0..v0.95.0) - 2026-07-06

### ✨ Highlights

- **The 5 local servers get their catalog face** ([PR 208](https://github.com/supernovae-st/nika/pull/208)) —
  ollama · lmstudio · llamacpp · localai · vllm join `nika catalog` (and
  `catalog --json`, so editor model pickers see them) with descriptions,
  `local`/`open-source` tags and seed models. Keyless by construction:
  a catalog edit can never invent a key gate. Sovereign models stay
  « unpriced », never « free ». 38 catalog providers total.
- **The lost-user footer** ([PR 203](https://github.com/supernovae-st/nika/pull/203)) — a bare `nika`
  suggests the next command instead of a wall of clap.
- **Hygiene: the seam-discipline vector** ([PR 207](https://github.com/supernovae-st/nika/pull/207)) +
  the chromiumoxide surface pin.

### 🐛 Fixes

- `nika:write` honors `create_dirs: false` — a missing parent is refused
  loudly instead of silently materializing a tree ([PR 196](https://github.com/supernovae-st/nika/pull/196)).
- `nika:date` resolves named timezones from the bundled tz db, not the
  OS ([PR 199](https://github.com/supernovae-st/nika/pull/199)).
- `nika:log` neutralizes terminal control sequences ([PR 204](https://github.com/supernovae-st/nika/pull/204)).
- The MCP stdio transport bounds its line reads — no unbounded buffer
  on a hostile client ([PR 209](https://github.com/supernovae-st/nika/pull/209)).

### 📚 Docs & internals

- Doc-comments go count-free (« 22-builtin » / « 5/23 » swept — the
  catalog is the count · [PR 206](https://github.com/supernovae-st/nika/pull/206)); README daily
  commands catch the 0.94 loop ([PR 205](https://github.com/supernovae-st/nika/pull/205)).
- The embedded pack re-vendors nika-spec main: count-free schema
  description (nika-spec#21) and `create_dirs` enforcement prose
  (nika-spec#20).
- `nika:validate` SSRF floor + deep-YAML totality pinned ([PR 197](https://github.com/supernovae-st/nika/pull/197));
  public-api baselines synced ([PR 198](https://github.com/supernovae-st/nika/pull/198)/[PR 200](https://github.com/supernovae-st/nika/pull/200)).

### ✨ Features
- **hygiene** — Seam-discipline vector + chromiumoxide surface pin ([#207](https://github.com/supernovae-st/nika/issues/207)) ([b8cd039aa](https://github.com/supernovae-st/nika/commit/b8cd039aa888642074b7907ff27522a710074749)) ([#207](https://github.com/supernovae-st/nika/pull/207))
- **nika-catalog** — The 5 local servers get their catalog face ([#208](https://github.com/supernovae-st/nika/issues/208)) ([7223c41b0](https://github.com/supernovae-st/nika/commit/7223c41b07c81768db59a8a866d08afc61991016)) ([#208](https://github.com/supernovae-st/nika/pull/208))
- **nika-cli** — The lost-user footer — bare nika suggests the next command ([#203](https://github.com/supernovae-st/nika/issues/203)) ([16e279935](https://github.com/supernovae-st/nika/commit/16e279935949dfc7d722cd370921987a2ccf3b4e)) ([#203](https://github.com/supernovae-st/nika/pull/203))

### 🐛 Bug Fixes
- **nika-builtin** — Write honors create_dirs:false — no silent tree ([#196](https://github.com/supernovae-st/nika/issues/196)) ([7fc3c9fc1](https://github.com/supernovae-st/nika/commit/7fc3c9fc18d44836de48ca1ab8cecbda44bb1fd6)) ([#196](https://github.com/supernovae-st/nika/pull/196))
- **nika-builtin** — Date resolves named tz from the bundled db, not the OS ([#199](https://github.com/supernovae-st/nika/issues/199)) ([228032695](https://github.com/supernovae-st/nika/commit/228032695567ae51d10939037248768771c634c7)) ([#199](https://github.com/supernovae-st/nika/pull/199))
- **nika-cli** — Log message neutralizes terminal control sequences ([#204](https://github.com/supernovae-st/nika/issues/204)) ([09c059646](https://github.com/supernovae-st/nika/commit/09c05964655359f5780113d94a10b75fbfc269ff)) ([#204](https://github.com/supernovae-st/nika/pull/204))
- **nika-mcp** — Bound the stdio transport — no unbounded line read ([#209](https://github.com/supernovae-st/nika/issues/209)) ([dd829f451](https://github.com/supernovae-st/nika/commit/dd829f45138e6e6d2ec83393eb441356841265a9)) ([#209](https://github.com/supernovae-st/nika/pull/209))

### 📚 Documentation
- **readme** — Daily commands catch the 0.94 loop ([#205](https://github.com/supernovae-st/nika/issues/205)) ([da5740747](https://github.com/supernovae-st/nika/commit/da574074799cabc01431bc83b178b1af6b15e2c0)) ([#205](https://github.com/supernovae-st/nika/pull/205))
- **src** — Stdlib counts leave the doc-comments — the catalog is the count ([#206](https://github.com/supernovae-st/nika/issues/206)) ([69929aaab](https://github.com/supernovae-st/nika/commit/69929aaabe5d3752f49986b156fd610c6c18424b)) ([#206](https://github.com/supernovae-st/nika/pull/206))

### 🧪 Tests
- **nika-builtin** — Pin validate's SSRF floor + deep-YAML totality ([#197](https://github.com/supernovae-st/nika/issues/197)) ([f96317699](https://github.com/supernovae-st/nika/commit/f963176999380946021f87e81f244f3b0ae9b938)) ([#197](https://github.com/supernovae-st/nika/pull/197))

### 🧹 Chore
- **nika-cli** — Sync public-api baseline — run::run gained no_trace_file+task_filter ([#200](https://github.com/supernovae-st/nika/issues/200)) ([68ce435cb](https://github.com/supernovae-st/nika/commit/68ce435cb084787d007c6c00201a3d905b82a76b)) ([#200](https://github.com/supernovae-st/nika/pull/200))

## [0.94.0](https://github.com/supernovae-st/nika/compare/v0.93.1..v0.94.0) - 2026-07-06

### ✨ Highlights

- **The media suite** — two new builtins take the stdlib to 25.
  `nika:image_generate` ([PR 173](https://github.com/supernovae-st/nika/pull/173) · providers v1.1 local-first + xai
  [PR 174](https://github.com/supernovae-st/nika/pull/174) · `mode: edit` M-A/M-A.2
  [PR 188](https://github.com/supernovae-st/nika/pull/188)/[PR 189](https://github.com/supernovae-st/nika/pull/189)) and
  `nika:tts_generate` ([PR 176](https://github.com/supernovae-st/nika/pull/176) · one sovereign
  `/v1/audio/speech` wire = LocalAI/Kokoro/Speaches · openai · elevenlabs · a real
  deterministic WAV under mock). Assets land on disk with provenance manifests —
  content credentials detected and preserved ([PR 177](https://github.com/supernovae-st/nika/pull/177)) ·
  the elevenlabs watermark declared ([PR 183](https://github.com/supernovae-st/nika/pull/183)).
- **Every run leaves a journal** — `.nika/traces/<ts>-<id>.ndjson` by default
  ([PR 170](https://github.com/supernovae-st/nika/pull/170) · opt out `--no-trace-file` /
  `NIKA_NO_TRACE_FILE`), and `nika run --task <id>` scopes execution to one task
  and its upstream cone. CEL expression completion lands in the LSP.
- **The catalog rides the wire** — `nika catalog --json` · `nika tools --json`
  ([PR 172](https://github.com/supernovae-st/nika/pull/172)) + their MCP twins:
  the oracle now serves 8 read-only tools, and the server speaks MCP
  2026-07-28 natively ([PR 171](https://github.com/supernovae-st/nika/pull/171)) while
  accepting 2025-11-25 clients ([PR 185](https://github.com/supernovae-st/nika/pull/185)).
- **Honest money** — real per-task spend: `cost_usd` rides `task_completed`
  ([PR 168](https://github.com/supernovae-st/nika/pull/168) · absent for mock/local,
  never a fake zero) and agent loops meter tool-reported cost
  ([PR 178](https://github.com/supernovae-st/nika/pull/178)).
- **Providers 16** — huggingface + nvidia join the canonical catalog
  ([PR 167](https://github.com/supernovae-st/nika/pull/167) · ADR-104).
- **`nika doctor --ping`** — the local ports, actually probed
  ([PR 195](https://github.com/supernovae-st/nika/pull/195) · TCP connect-only ·
  300ms cap · loopback/configured URLs only · the default run stays offline).
  `nika check` findings carry severity + a docs URL
  ([PR 184](https://github.com/supernovae-st/nika/pull/184)).

### 🧹 Chore

- **pack** — re-vendored from spec main: the 16-provider/25-builtin markers ·
  the tts error-category ladder · the new `mcp:` canon section ride the binary.
- **ci** — the test gate runs under nextest ([PR 192](https://github.com/supernovae-st/nika/pull/192)) ·
  public-api baselines canonicalized from the ubuntu artifact ·
  `iter_over_hash_type` denied ([PR 181](https://github.com/supernovae-st/nika/pull/181)).
- **cli** — round-8 visual polish ([PR 169](https://github.com/supernovae-st/nika/pull/169)) ·
  the comprehension pass ([PR 166](https://github.com/supernovae-st/nika/pull/166)) ·
  the init hand-off drops its phantom indentation ([PR 194](https://github.com/supernovae-st/nika/pull/194)).

## [0.93.1](https://github.com/supernovae-st/nika/compare/v0.93.0..v0.93.1) - 2026-07-05

### ✨ Highlights

- **The embedded pack teaches 2026 models** — `nika examples` /
  `nika new` / `nika spec` now carry the qwen3.5 cascade
  ([PR 161](https://github.com/supernovae-st/nika/pull/161)): the very
  first `nika examples run 01-hello` of a fresh install works against
  the models people actually pull in July 2026 (the v0.93.0 binary
  still embedded the pre-cascade pack — this patch closes the reach).
- README daily-commands block ([PR 162](https://github.com/supernovae-st/nika/pull/162)).

### 📚 Documentation
- **readme** — Daily commands — the full 0.93 user loop in one block ([#162](https://github.com/supernovae-st/nika/issues/162)) ([8356df6f7](https://github.com/supernovae-st/nika/commit/8356df6f7e481c089bb86f6efd907a1b1f3240a0))

### 🧹 Chore
- **pack** — Vendor the qwen3.5 teach-cascade from spec main ([#161](https://github.com/supernovae-st/nika/issues/161)) ([e8f500f73](https://github.com/supernovae-st/nika/commit/e8f500f7362b02cb44a649f861d1228fde16d0cc))

## [0.93.0](https://github.com/supernovae-st/nika/compare/v0.92.0..v0.93.0) - 2026-07-05

### ✨ Highlights

- **Local thinking-era models unbricked** — the provider-plane HTTP timeout
  rises to 180s ([PR 148](https://github.com/supernovae-st/nika/pull/148)): 2026 open-weight models (qwen3.5 · gemma4) spend
  40-90s thinking before a structured answer on consumer hardware; every
  local structured `infer:` died at the old 30s cloud-calibrated ceiling.
  Field-fix train [PR 149](https://github.com/supernovae-st/nika/pull/149) pairs it with per-task transport-deadline semantics.
- **`nika run --resume`** — durable-lite resume: the trace IS the checkpoint
  (ADR-099 · [PR 154](https://github.com/supernovae-st/nika/pull/154)): completed tasks replay from the flight recorder as
  visible cache hits · a paused/failed run continues where it stopped.
- **`nika test`** — the workflow test harness + ADR-098 provider json-mode +
  the CLI wow pass (waves · lanes · verdict card) landed in the Rounds 2+3
  train ([PR 153](https://github.com/supernovae-st/nika/pull/153)).
- **Onboarding golden path** — brew caveats walk the first 60 seconds ·
  `nika init` hands over to the next command ([PR 158](https://github.com/supernovae-st/nika/pull/158)) · the embedded pack
  carries the current spec (60s README · why-not table · llms.txt).

### 🆕 Crates admitted
- **nika-cap** — Admit to workspace — all 12 gates passed ([ea5b2090d](https://github.com/supernovae-st/nika/commit/ea5b2090de164f849369316236f587acbf69bb1d))

### ✨ Features
- **init** — Hand over to the next command · the beginner golden path ([#158](https://github.com/supernovae-st/nika/issues/158)) ([cd291a3b5](https://github.com/supernovae-st/nika/commit/cd291a3b54ff15c834d97c4af0de0ea52fa84626)) ([#158](https://github.com/supernovae-st/nika/pull/158))

### 🐛 Bug Fixes
- **cli** — Raise provider-plane http timeout to 180s for local models ([#148](https://github.com/supernovae-st/nika/issues/148)) ([f99828cbf](https://github.com/supernovae-st/nika/commit/f99828cbf728ce9f0c4ce07a1fd48271f7791573)) ([#148](https://github.com/supernovae-st/nika/pull/148))
- **nika-schema** — Char-boundary panic in go-duration unknown-unit error path ([c1f73af89](https://github.com/supernovae-st/nika/commit/c1f73af89aaf61fd65a696b6bd1894045ca2f537))
- **scripts** — Adr index generator tolerates missing optional fields ([#152](https://github.com/supernovae-st/nika/issues/152)) ([5d5982f41](https://github.com/supernovae-st/nika/commit/5d5982f418b444e415dc840e1db3ecfa94db028c)) ([#152](https://github.com/supernovae-st/nika/pull/152))

### 📚 Documentation
- **changelog** — Append v0.92.0 — auto-generated ([#147](https://github.com/supernovae-st/nika/issues/147)) ([2a42f0712](https://github.com/supernovae-st/nika/commit/2a42f0712783235630c940712e00632530b8dc52)) ([#147](https://github.com/supernovae-st/nika/pull/147))
- **workspace** — Canonical pck crate names + fix publish adr citation ([58d9514bf](https://github.com/supernovae-st/nika/commit/58d9514bf1db97809c41ca95e8a751faf1d831e7))

### 🧪 Tests
- **nika-mcp** — Regression guard for the embedded-lookup injection-safety ([4f306d5f9](https://github.com/supernovae-st/nika/commit/4f306d5f99c2055695b103a63f0b008f9a9f8d65))
- **schema** — Forward-compat anchors for the v0.2 seed clauses ([#156](https://github.com/supernovae-st/nika/issues/156)) ([5a84a4702](https://github.com/supernovae-st/nika/commit/5a84a4702586893a2bd077ee5c0f65ddd7942d07)) ([#156](https://github.com/supernovae-st/nika/pull/156))

### 🧹 Chore
- **ci** — Api-lock the last 3 uncovered lib crates ([539a039eb](https://github.com/supernovae-st/nika/commit/539a039ebb6441fe8863a0b383f407c92c8a89e2))
- **ci** — Canonicalize 16 public-api baselines from the ci artifact ([1a44b139c](https://github.com/supernovae-st/nika/commit/1a44b139c26b596aacd73d379a8e3a872d4d0566))
- **ci** — Nika-cli baseline from the ubuntu artifact ([688a851ea](https://github.com/supernovae-st/nika/commit/688a851ea2b317fe5ef894eb72f9a6829e40f6c0))
- **nika-catalog** — Regenerate public-api baseline — additive builtin arg-shape surface ([a681d6044](https://github.com/supernovae-st/nika/commit/a681d6044855842fda455be8e0c4b5b5f237f8af))
- **nika-pack** — Re-vendor the pack from the merged spec ([#157](https://github.com/supernovae-st/nika/issues/157)) ([0d5b6dd8c](https://github.com/supernovae-st/nika/commit/0d5b6dd8c46dc316a6f73c876accbc19b409c5f8)) ([#157](https://github.com/supernovae-st/nika/pull/157))
- **version** — Main → 0.93.0-dev after the v0.92.0 release ([4ad3cbd20](https://github.com/supernovae-st/nika/commit/4ad3cbd203778b0f97d0af1c055fe2bc8c77aacf))

### 💼 Other
- Field fixes: local-model timeouts, mock-from-schema, --var, sitemap examples ([#149](https://github.com/supernovae-st/nika/issues/149))

* fix(workspace): task timeout governs the provider http deadline

The provider HTTP client aborted every round-trip at a hardcoded 30s
total deadline, killing any local-model task regardless of its declared
timeout: budget (408 at 30s on timeout: "7m" · F1 field report
2026-07-04). The task budget now rides the whole chain — task.timeout →
dispatch → InferInput → InferRequest → HttpRequest — and when no budget
is declared the default is per provider CLASS: the 5 local servers get
300s (a 14B model cannot answer a real prompt in 30s), cloud keeps the
historical 30s. Streaming carries only an explicit budget (the idle-read
guard reaps stalls). The provider client's config timeout is raised to a
600s transport ceiling because reqwest arms the client-level read guard
even while awaiting response headers — it would otherwise undercut any
longer per-request deadline.

Tests: wire-level parity across all 13 wired profiles (class default ·
task budget wins · streaming explicit-only) + profile classification +
verb plumb + the real parse→check→run chain against a capturing http
seam. Baselines: nika-kernel-ai + nika-verb-infer public-api
regenerated (additive timeout field on two #[non_exhaustive] DTOs).

* feat(nika-providers): mock synthesizes schema-conformant output

mock/echo echoed the prompt regardless of an attached schema, so EVERY
structured workflow on the mock burned its 3-attempt retry budget and
died NIKA-INFER-002 — no offline CI story for exactly the workflows
that need one (F3 field report 2026-07-04). A request carrying
response_format: JsonSchema now returns a SYNTHESIZED minimal instance
of the schema as pure JSON: string → "mock" (enum → first entry ·
const/default honored) · integer → minimum ?? 0 · number → 0.0 · bool →
false · array → one item (minItems honored · allocation-capped) ·
object → the required keys recursively. Total + deterministic
(byte-stable) · unsupported keywords (pattern · $ref) degrade to an
instance that honestly fails the verb's validation. The plain echo
contract is untouched.

Tests: generator matrix validated with the SAME jsonschema crate the
verb floor uses (atlas-style enum severity + bounded integers green) ·
verb-level offline dry-run + retry-exhaustion re-pinned on a pattern
schema · mock wire pure-JSON determinism · e2e pipeline updated to the
synthesized instance.

* feat(nika-cli): repeatable --var key=value supplies workflow vars

A workflow with a required: true var and no default was UNRUNNABLE
from the CLI — nika run had no input surface, so the first
${{ vars.x }} reference died NIKA-VAR-001 with no fix available (F4
field report 2026-07-04). nika run gains a repeatable --var KEY=VALUE:
values override a declared default:, satisfy required vars, parse as
JSON when they parse (numbers · booleans · arrays) else ride as
strings, and unknown keys are refused loudly with the declared set
(exit 3 — a typo silently doing nothing would be the worst outcome).
The runtime gains the with_var_overrides builder (merged over the
envelope defaults at run start). NIKA-VAR-001 now teaches the fix on
BOTH surfaces: the runtime message appends the --var hint for vars.*
references, and nika explain NIKA-VAR-001 carries an engine-side fix
line.

Tests: required-var run green with --var / failed without · unknown
key + malformed pair refused · JSON-else-string typing · override
beats default through the real parse→check→run chain · both hint
surfaces pinned. Baselines: nika-runtime regenerated (additive
builder) · nika-cli run() signature line patched in place (the
committed file is the ubuntu-canonical artifact — a full local regen
would carry macOS render skew on unrelated lines).

* fix(nika-pack): sitemap examples bind the root array not .urls

nika:fetch mode: sitemap returns the ROOT ARRAY
[{loc, changefreq, priority}, …] — the shape nika-extract pins in its
own tests — but the embedded showcase examples bound a phantom .urls
wrapper (t2-seo-content-brief .urls[:5] · t3-competitor-radar
.urls[:8]) and the embedded spec 03-dag fan-out snippet taught
pages: ".urls[]" (doubly wrong: phantom wrapper AND a stream binding
where a binding is single-valued). Every user copying the examples hit
NIKA-VAR-004 at the first sitemap task (F5 field report 2026-07-04).
The engine is RIGHT — the teaching surface is fixed: .[:5] | map(.loc)
/ .[:8] | map(.loc) / map(.loc), plus the t2 header now documents the
sitemap shape and the mock-from-schema offline story F3 made true.

manifest.yaml sha256_16 recomputed for both files (lean-hash algorithm
cross-verified against all 27 committed rows · 27/27 match). Both fixed
examples pass nika check clean with the built binary.

FOLLOW-UP (out of this repo's scope): the pack is vendored from the
nika-spec repo via scripts/sync-pack.sh — the SSOT needs the mirror
fix (examples + 03-dag.md + manifest) before the next re-vendor, or it
reverts this.

* refactor(nika-cli): extract output-mode + dry-run helpers from run()

The F4 --var block pushed run() to 114 lines and the F1 budget plumb
pushed attempt_loop to 101 — both past the 100-line ratchet. Extract
the --output validation (output_mode) and the dry-run plan render
(render_dry_run) into named helpers; compress the attempt_loop budget
comment. Behavior identical — the extracted code is verbatim, tests
unchanged and green (110 + 78 lib).

---------
 ([90b19d589](https://github.com/supernovae-st/nika/commit/90b19d58960104d84e87235337da15cc82e3aa97)) ([#149](https://github.com/supernovae-st/nika/pull/149))
- Rounds 2+3: nika test, DX, ADR-098 json-mode, CLI wow (rebased train) ([#153](https://github.com/supernovae-st/nika/issues/153))

* fix(workspace): task timeout governs the provider http deadline

The provider HTTP client aborted every round-trip at a hardcoded 30s
total deadline, killing any local-model task regardless of its declared
timeout: budget (408 at 30s on timeout: "7m" · F1 field report
2026-07-04). The task budget now rides the whole chain — task.timeout →
dispatch → InferInput → InferRequest → HttpRequest — and when no budget
is declared the default is per provider CLASS: the 5 local servers get
300s (a 14B model cannot answer a real prompt in 30s), cloud keeps the
historical 30s. Streaming carries only an explicit budget (the idle-read
guard reaps stalls). The provider client's config timeout is raised to a
600s transport ceiling because reqwest arms the client-level read guard
even while awaiting response headers — it would otherwise undercut any
longer per-request deadline.

Tests: wire-level parity across all 13 wired profiles (class default ·
task budget wins · streaming explicit-only) + profile classification +
verb plumb + the real parse→check→run chain against a capturing http
seam. Baselines: nika-kernel-ai + nika-verb-infer public-api
regenerated (additive timeout field on two #[non_exhaustive] DTOs).

* feat(nika-providers): mock synthesizes schema-conformant output

mock/echo echoed the prompt regardless of an attached schema, so EVERY
structured workflow on the mock burned its 3-attempt retry budget and
died NIKA-INFER-002 — no offline CI story for exactly the workflows
that need one (F3 field report 2026-07-04). A request carrying
response_format: JsonSchema now returns a SYNTHESIZED minimal instance
of the schema as pure JSON: string → "mock" (enum → first entry ·
const/default honored) · integer → minimum ?? 0 · number → 0.0 · bool →
false · array → one item (minItems honored · allocation-capped) ·
object → the required keys recursively. Total + deterministic
(byte-stable) · unsupported keywords (pattern · $ref) degrade to an
instance that honestly fails the verb's validation. The plain echo
contract is untouched.

Tests: generator matrix validated with the SAME jsonschema crate the
verb floor uses (atlas-style enum severity + bounded integers green) ·
verb-level offline dry-run + retry-exhaustion re-pinned on a pattern
schema · mock wire pure-JSON determinism · e2e pipeline updated to the
synthesized instance.

* feat(nika-cli): repeatable --var key=value supplies workflow vars

A workflow with a required: true var and no default was UNRUNNABLE
from the CLI — nika run had no input surface, so the first
${{ vars.x }} reference died NIKA-VAR-001 with no fix available (F4
field report 2026-07-04). nika run gains a repeatable --var KEY=VALUE:
values override a declared default:, satisfy required vars, parse as
JSON when they parse (numbers · booleans · arrays) else ride as
strings, and unknown keys are refused loudly with the declared set
(exit 3 — a typo silently doing nothing would be the worst outcome).
The runtime gains the with_var_overrides builder (merged over the
envelope defaults at run start). NIKA-VAR-001 now teaches the fix on
BOTH surfaces: the runtime message appends the --var hint for vars.*
references, and nika explain NIKA-VAR-001 carries an engine-side fix
line.

Tests: required-var run green with --var / failed without · unknown
key + malformed pair refused · JSON-else-string typing · override
beats default through the real parse→check→run chain · both hint
surfaces pinned. Baselines: nika-runtime regenerated (additive
builder) · nika-cli run() signature line patched in place (the
committed file is the ubuntu-canonical artifact — a full local regen
would carry macOS render skew on unrelated lines).

* fix(nika-pack): sitemap examples bind the root array not .urls

nika:fetch mode: sitemap returns the ROOT ARRAY
[{loc, changefreq, priority}, …] — the shape nika-extract pins in its
own tests — but the embedded showcase examples bound a phantom .urls
wrapper (t2-seo-content-brief .urls[:5] · t3-competitor-radar
.urls[:8]) and the embedded spec 03-dag fan-out snippet taught
pages: ".urls[]" (doubly wrong: phantom wrapper AND a stream binding
where a binding is single-valued). Every user copying the examples hit
NIKA-VAR-004 at the first sitemap task (F5 field report 2026-07-04).
The engine is RIGHT — the teaching surface is fixed: .[:5] | map(.loc)
/ .[:8] | map(.loc) / map(.loc), plus the t2 header now documents the
sitemap shape and the mock-from-schema offline story F3 made true.

manifest.yaml sha256_16 recomputed for both files (lean-hash algorithm
cross-verified against all 27 committed rows · 27/27 match). Both fixed
examples pass nika check clean with the built binary.

FOLLOW-UP (out of this repo's scope): the pack is vendored from the
nika-spec repo via scripts/sync-pack.sh — the SSOT needs the mirror
fix (examples + 03-dag.md + manifest) before the next re-vendor, or it
reverts this.

* refactor(nika-cli): extract output-mode + dry-run helpers from run()

The F4 --var block pushed run() to 114 lines and the F1 budget plumb
pushed attempt_loop to 101 — both past the 100-line ratchet. Extract
the --output validation (output_mode) and the dry-run plan render
(render_dry_run) into named helpers; compress the attempt_loop budget
comment. Behavior identical — the extracted code is verbatim, tests
unchanged and green (110 + 78 lib).

* feat(nika-cli): add nika test — golden outputs under mock (F7)

The v1 workflow-testing surface the field report asked for: users had to
hand-roll e2e harnesses (atlas ships one) because the verb tree had no
test/eval/lint. nika test <file> checks (ADR-092 ladder · same gate as
run), executes under the MOCK provider (deterministic + schema-conformant
since F3 — the base that unblocked this), captures the typed outputs: as
ONE canonical JSON (pretty · key-sorted · newline-terminated) and compares
against the sibling <file>.golden.json. --update (re)writes the pin; no
golden + no --update teaches how to create one (exit 3). Mismatch renders
a readable per-path diff (golden → run · capped at 20 rows) and exits 1.

Gates affected: Gate 3 (impl) + Gate 4 (clippy 0). Test delta: +8
(update-then-match · drift mismatch · missing-golden hint · dirty-file
findings · diff paths/missing/extra/cap · elision · golden-path mapping ·
canonical bytes). 118 nika-cli lib tests green.

* fix(workspace): quoted timeout fix-form + json failure envelope (F6)

(a) nika-schema · the NIKA-PARSE-010 bare-number rejection now shows the
author's OWN value quoted with a unit ("420s" for seconds · "7m" for
minutes) — the correction becomes a copy-paste, not a spec hunt (the
field report hit `timeout: 420` and had to guess the form).

(b) nika-cli · `--output json` failures now emit ONE machine envelope
{"error":{"code":"NIKA-…"|null,"message":"…"}} on stdout — it used to
stay empty (pre-run findings) or print bare `{}` (run failures), so a
machine consumer had to scrape stderr. Covers every failure class:
parse/check findings · --var refusals · composition/executor ENV
errors · executed-and-failed runs (the folded view's failed-row detail
carries the wire code). Codes are extracted, never invented.

(c) verified, no change: the NIKA-VAR-001 explain/fix hint already
names `--var` (shipped with F4 · explain.rs cli_fix_hint plus its
pinned test).

Gates affected: Gate 3+4. Test delta: +5 (envelope shape/one-line ·
code extraction incl. per-builtin and no-false-positive · findings-line
condensing · failed-view envelope + empty-view fallback · parse-010
quoted fix-form). nika-cli 122 + nika-schema 758 lib tests green.

* feat(workspace): underspecified schemas ride json mode locally (F2)

The field-report F2 wall: the translate-payload class ({type: object} ·
or head/sections declared shapeless) 400s on OpenAI strict mode, and
fully specifying a free-form payload recursively is impossible — the
only workaround was schema-free promptware (zero guarantees).

Conservative fix, zero YAML surface (ADR-098): when the task schema is
UNDERSPECIFIED (any object without properties · any array without items
in the tree) on a strict wire, do NOT forward it — request the
provider's native JSON mode (ResponseFormat::Json), steer the shape
through the prompt instruction, validate LOCALLY against the user
schema (the existing floor + bounded retry). Fully-specified schemas
keep today's strict path verbatim; anthropic keeps the instruction
fallback; the MOCK keeps receiving the schema (its strict mode
synthesizes from anything — F3/offline goldens depend on it).

Seams: nika-verb-infer SchemaWire enum (None/Strict/JsonMode/
Instruction) + is_underspecified iterative walker · nika-providers
WireFormat::strict_rejects_underspecified (openai-compat + gemini
true · mock + anthropic false) + the ResolvedProvider delegate.

Gates affected: Gate 3+4+6-adjacent. Test delta: +9 (detection cases ·
decision table · request mapping · {type:object} green on the REAL
openai adapter with the http seam mocked + json_object on the wire ·
fully-specified stays json_schema · mock synthesis non-regression).
nika-verb-infer 42 + nika-providers 145 lib tests green.

ADR: docs/adr/adr-098-underspecified-schema-json-mode-fallback.md
(proposed + implemented · alternatives: json:true sugar · full
recursive spec · schema tightening — all rejected with reasons).
Note: docs/adr/index.json is stale at HEAD (generator exits 1 on
ADR-096 · pre-existing) — index.toml regenerated with ADR-098.

* refactor(nika-cli): keep main() and run() under the fn-length cap

Behavior-identical extraction after the F6/F7 additions tripped the
100-line ratchet (main 102 · run 104): the run clap surface moves into
a dedicated RunArgs struct (the TraceArgs idiom) unpacked by run_verb();
the runtime composition block moves into composed_runtime() with its
full composition story preserved in the doc comment.

cargo test -p nika-cli --lib: 122 passed, 0 failed. clippy -D clean.
check-fn-length.sh: OK all fns <= 100 lines.

* docs(nika-verb-infer): drop the private intra-doc link to SchemaWire

The crate-level doc linked [`SchemaWire`] — a private enum — which
fails `cargo doc --document-private-items -D warnings` (hygiene vector
28 · Gate 8 extended per ADR-015). Plain backticks carry the same
story without the broken link.

* CLI wow: wave-group inspect, live lanes + ms, waterfall, verdict card ([#151](https://github.com/supernovae-st/nika/issues/151))

* feat(nika-cli): inspect renders waves as bordered parallel groups

The flat first-parent tree hid the scheduler's proof — waves were
counted in the header but invisible in the body. inspect now renders
each parallel wave as a bordered group ("N in parallel"), single-task
waves as bare rows, and flow arrows between waves, so the DAG's real
concurrency reads at a glance (CLI wow design 2026-07-05 §2a).

- one projector law kept: the render slices the SAME GraphDoc node
 order by report.waves sizes — nothing re-derived
- --ascii parity first-class on every new glyph (box → +-|, diamond
 → #, arrow → v) + a leak test pinning zero unicode under --ascii
- boxes cap at 74 inner cells (graceful under 80 cols · overlong rows
 truncate with a mark)
- the spec §6 DAG-check footer + engineering analysis are unchanged;
 --dry-run inherits the new plan render through the same verb

* feat(nika-cli): per-task time, spend and parallel lane markers

Time and money were footer-only — no per-task duration, no per-task
cost, parallelism invisible. The storyboard now carries them first-
class (CLI wow design 2026-07-05 §2b):

- settled rows show their REAL wall time (the runtime-measured
 duration_ms field wins · stamp-span fallback) right-aligned after
 the note column; per-task spend follows when the stream reported it
- the running/retrying row shows a LIVE elapsed against the latest
 stamp the fold has seen (still a pure fold — no wall clock leaks)
- new display/flow module reconstructs each task's interval as
 [end − duration, end] (frames stamp at settle time · the measured
 duration is the wall truth) and derives the ∥ lane markers from
 honest overlap; the run verb injects the check report's wave plan
 so markers speak the scheduler's truth (siblings only), replayed
 traces fall back to pure overlap
- retries counted + per-row start/end/cost stamped in the fold
- --ascii parity (∥ → ||) with a leak test; machine lanes untouched
 (--json NDJSON verbatim · --output json stdout unchanged)

* feat(nika-cli): post-run waterfall closes the TTY final frame

Where the seconds burned was invisible after a run — the answer lived
in the trace but nothing drew it. The Live (TTY) final frame now ends
with a wall-time waterfall + an outputs pointer (design §2c):

- one scaled bar per task that RAN, offset by its reconstructed
 interval ([end − measured duration, end] — the same reconstruction
 the ∥ markers use), so real overlap reads as overlapping bars
- failed bars paint red · running/retrying cyan · per-task spend rides
 the row · a dotted time axis closes the chart
- "outputs → key (type)" names what left the run (types only — a
 pointer, never a data dump into the scrollback)
- pure fold of the run's own event stream — zero new instrumentation,
 and a solo-task run stays chart-free (a single bar is noise)
- sober registers untouched: piped / --no-progress / --quiet / machine
 modes never grow chart art (pinned in the piped bin test) · --ascii
 parity for every glyph (bars → [#] · axis dots → .)

* feat(nika-cli): shareable verdict card + trace replay renders the flow

The final frame becomes the thing you paste in a README (design §2d):
a bordered verdict card closing every Live run and every trace read —
the mini DAG-shape glyph (wave sizes as diamond runs joined by flow
arrows · "###" " => " "#" in ASCII · plan-true when the run injected
the schedule, interval-reconstructed for a bare trace), the totals
(tasks · waves · retries · wall · spend · the models the stream
named), and an "outputs → key (type)" note for runs. No verdict → no
card (a mid-run card would lie). Wide waves cap at five diamonds,
long chains at six waves; the box never breaks (fit + one inner
width, pinned).

mini-ADR · replay surface naming
Question: where does "render the waterfall from a past trace" live —
extend "nika inspect --trace <file>", or a new subcommand?
Decision: NEITHER — it rides the EXISTING "nika trace show|replay".
Why: (1) house taste = fewest verbs: trace is already the flight-
recorder reader (replay = re-render, never re-execute), and the
waterfall IS a trace read, so it belongs there; (2) inspect is locked
as the STATIC anatomy — its own module doc says "run overlays belong
to the trace surface", and an --trace flag would fork one verb into
two registers; (3) zero new verbs, zero new flags: "nika trace show
run.ndjson" now closes on the waterfall + verdict card, and "trace
replay" ends its animation on the same final frame a live TTY run
paints. The machine lanes stay byte-frozen (--json / --output json).

---------

---------
 ([da874faf6](https://github.com/supernovae-st/nika/commit/da874faf6fb4c02c1c6ebadb6a16a994be5ac02f)) ([#153](https://github.com/supernovae-st/nika/pull/153))
- Durable-lite resume — --resume, visible cache hits, durable pause ([#154](https://github.com/supernovae-st/nika/issues/154)) ([91d7cf9bf](https://github.com/supernovae-st/nika/commit/91d7cf9bfb9848e2950ede531215662612105a7d)) ([#154](https://github.com/supernovae-st/nika/pull/154))
# Changelog

All notable changes to Nika are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
Nika follows real semver toward a **1.0.0** public launch (amended
D-2026-06-20-N1) — quality over speed. v0.90.0 is the first public release.

Nika Diamond is a ground-up rewrite on an orphan branch (`main` ·
renamed 2026-05-06 from `nika-diamond`). Legacy v0.79.3 lives on
`brouillon` (renamed 2026-05-06 from `main`). Diamond starts at v0.80.0.

**Version history.** The pre-Diamond engine (the v0.1 → v0.79.3 legacy
era) is preserved in the private `nikab-legacy` reference repo — its tags
and releases were removed from this public repository in the 2026-06-21
cleanup. This public repo carries the Diamond arc only: `v0.80.0-alpha.*`
(the rewrite history) and `v0.90.0` (the first public release). This
changelog tracks the Diamond rebuild from **v0.80.0-alpha** onward.

---

## [0.92.0](https://github.com/supernovae-st/nika/compare/v0.91.0..v0.92.0) - 2026-07-03

### ✨ Features
- **media** — Permits-audit + on-error-recover motion scenes ([5ea0658bb](https://github.com/supernovae-st/nika/commit/5ea0658bbb18f9376324dd441b945ca02fdda21e))
- **nika-browser** — Guard navigate against private/loopback SSRF ([e81720031](https://github.com/supernovae-st/nika/commit/e81720031543554aa02050640e4ebbb9084fd517))
- **nika-cli** — Wire codex — mcp wiring for the codex cli ([b8246605c](https://github.com/supernovae-st/nika/commit/b8246605cfb31b92ab32972e616e7980eeb8d523))
- **nika-cli** — Codex plugin + repo-level agent skill ([4db87a89a](https://github.com/supernovae-st/nika/commit/4db87a89a7e4590b3509a8b70af4b3918634b0e3))
- **nika-cli** — Nika init scaffolds the AGENTS.md hard rules + learning surface ([45668958f](https://github.com/supernovae-st/nika/commit/45668958fea7e4313054c99ddf6728ddce2589a2))
- **nika-mcp** — Add the learning tools — schema · examples · template · canon ([0b38d3b28](https://github.com/supernovae-st/nika/commit/0b38d3b28514e3dc96190e59edefbdb5ff67e8c7))
- **nika-verb-exec** — Scrub the engine's provider API keys from exec children ([fa7a503dd](https://github.com/supernovae-st/nika/commit/fa7a503dd4b7ce4afacb2b1c78acd6e39ab17282))
- **workspace** — Claude code marketplace — one plugin dir, two ecosystems ([7e4a0b800](https://github.com/supernovae-st/nika/commit/7e4a0b80025f3a304c3bf8d84bb77dddf8351db2))

### 🐛 Bug Fixes
- **nika-cel** — Compare int64 exactly, not through f64 ([406af0072](https://github.com/supernovae-st/nika/commit/406af0072500c4f48c6c9e003274544be099e333))
- **nika-cli** — Cold-first-run truth — readme dual-line + tip guard ([484ca318b](https://github.com/supernovae-st/nika/commit/484ca318b91268233b90069e225deb15d162303b))
- **nika-pack** — Vendor the egress-sanctioned templates from nika-spec ([ede60533c](https://github.com/supernovae-st/nika/commit/ede60533c62a9e49c181b654963c811a0752856e))
- **nika-schema** — Flag the shell-string + permits.exec allowlist pairing at check ([d096698b8](https://github.com/supernovae-st/nika/commit/d096698b8d63bdfa339924d1abc470683cd8e44f))
- **nika-verb-exec** — Restrict exec env keys to POSIX names ([e33384543](https://github.com/supernovae-st/nika/commit/e33384543793c5b65ec653774d1253f6c08a55de))
- **security** — Quick-xml advisory pair · xcap bump + documented ignores ([1c7faed69](https://github.com/supernovae-st/nika/commit/1c7faed695c9204ace1c819e9066d284caa2d89f))
- **workspace** — The hero one-liner is verified-real now ([1855ada6b](https://github.com/supernovae-st/nika/commit/1855ada6bcb39c6e0486f779fae531234f9ab756))
- **workspace** — The plugin's .mcp.json was gitignored out of the publish ([4af2f2d68](https://github.com/supernovae-st/nika/commit/4af2f2d68a045f0a9823d004aa44bd5ac70fd71a))

### 🔨 Refactors
- **kernel** — Remove the deprecated cost_usd field ([2b3c33edc](https://github.com/supernovae-st/nika/commit/2b3c33edc2c40c36e0c94b9627d3ef0ae994ab9b))
- **nika-builtin** — Split date/uuid out of data.rs before the LOC cap ([a0753e716](https://github.com/supernovae-st/nika/commit/a0753e716116c0deb26d316a3d97cd6415545dec))
- **nika-schema** — Drop the now-dead shell-form program lookup ([d664944dd](https://github.com/supernovae-st/nika/commit/d664944ddd9f5f442a77818730f9d4cc18aeb95d))

### 📚 Documentation
- **adr** — Resolve ADR-093/094 id collisions — agent ADRs to 096/097 ([#144](https://github.com/supernovae-st/nika/issues/144)) ([fb01a4966](https://github.com/supernovae-st/nika/commit/fb01a4966736335a40f8e5cb5e53009afbb36995)) ([#144](https://github.com/supernovae-st/nika/pull/144))
- **changelog** — Append v0.91.0 ([#139](https://github.com/supernovae-st/nika/issues/139)) ([12a60b3b1](https://github.com/supernovae-st/nika/commit/12a60b3b162f04e631f5593863382da369bf5c08)) ([#139](https://github.com/supernovae-st/nika/pull/139))
- **clarity** — Kill stale maturity claims across visitor-reachable files ([eaa966a16](https://github.com/supernovae-st/nika/commit/eaa966a16054019453ce6f893f421bf09f2b9585))
- **crate-specs** — Drop hardcoded workspace version (anti-drift) ([4133ac0ce](https://github.com/supernovae-st/nika/commit/4133ac0cea6fc9359e4779fed98414f1af6a2882))
- **crate-specs** — Nika-cap gate-1 spec — the capability boundary as L0 ([20740b3fc](https://github.com/supernovae-st/nika/commit/20740b3fc41d723f8852574f38df5bb45e64266b))
- **dx** — De-drift the .claude commands + crate-admit skill ([6c203d6d0](https://github.com/supernovae-st/nika/commit/6c203d6d040ac9de5743a3fe2dd06c463766c7d2))
- **examples** — Real mermaid plan from nika graph + checks-clean line ([0f166dba9](https://github.com/supernovae-st/nika/commit/0f166dba924ca11d577f937676e0d24cd7e6dfb6))
- **media** — Motion media pipeline + 3 real-capture brand assets ([1a257ba98](https://github.com/supernovae-st/nika/commit/1a257ba98846097293f622e6a1f3107b5874ed39))
- **media** — V2 polish — narrative beats, data-flow pulses, 16fps gifs ([c319ae1d6](https://github.com/supernovae-st/nika/commit/c319ae1d6b07e6b909ddb5786e4c678b2f044954))
- **media** — Og + github social-preview cards from the motion pipeline ([c622dd39a](https://github.com/supernovae-st/nika/commit/c622dd39a132684fdc7f979928ac4828a764a7ba))
- **media** — Editor-diagnostics asset — the audit as you type ([3f1cb9e09](https://github.com/supernovae-st/nika/commit/3f1cb9e09d058bf454788c6435373e6fa2be3fe7))
- **media** — Workflow-gallery asset — start from a workflow ([ec089cbf8](https://github.com/supernovae-st/nika/commit/ec089cbf81b6e48b9a3e7028641dfd62171ba3a0))
- **media** — Social poster pair — the wedge + the audit ([6630f8a78](https://github.com/supernovae-st/nika/commit/6630f8a78dee134fe382efd641f78c4820bda1b7))
- **nika-builtin** — Correct the nika:jq internal-cost comment ([7f00faf3e](https://github.com/supernovae-st/nika/commit/7f00faf3e653e3ca55c3a2de395d14cd12ea6a41))
- **nika-pack** — Vendor the em-dash-swept quickstart from nika-spec ([ecc69c37a](https://github.com/supernovae-st/nika/commit/ecc69c37a316240048edff9bb27bc6f42f1a02f5))
- **readme** — Wedge-first rewrite — run-today proof + example gallery surfaced ([e4ee6f08a](https://github.com/supernovae-st/nika/commit/e4ee6f08a0c3f7ff5ff0cd1d9f2ea96c37b76962))
- **readme** — Real terminal gif — check audits, run executes (96KB) ([2ed5520ce](https://github.com/supernovae-st/nika/commit/2ed5520ce30034904f3803f958e6a70c4c756a72))
- **readme** — Embed the permits-audit + on-error-recover captures ([9e047ae85](https://github.com/supernovae-st/nika/commit/9e047ae85799268988511cca21e69a9859de3e65))
- **readme** — Plugin install block in work-with-your-agents ([e3b6f3155](https://github.com/supernovae-st/nika/commit/e3b6f31559a30fc1e55c5d7a29fd8c6565aa5acc))
- **readme** — Plugin marketplace → the lean nika-agents repo ([76a1cf8ef](https://github.com/supernovae-st/nika/commit/76a1cf8ef7375664c836241c8ff5c063cdd40160))
- **roadmap** — Fix self-contradicting release scars ([5e4c13990](https://github.com/supernovae-st/nika/commit/5e4c13990ad3c5a6e9c07cfd67b42c72f289e4ce))
- **roadmap** — The 0.92→0.95 ladder — three admissions to 42/42 ([71aa58db4](https://github.com/supernovae-st/nika/commit/71aa58db4b63ce8d4a79b80a1048141356e63f11))
- **workspace** — Prose em-dash sweep in the readme ([3f20694e8](https://github.com/supernovae-st/nika/commit/3f20694e87a2d0f6697c591106c010efd015e1d6))
- **workspace** — Join the readme to the new site pages ([587ad053e](https://github.com/supernovae-st/nika/commit/587ad053ec5ef1291a4b5d4584240df169a52c56))
- **workspace** — The hero run is real — local model register everywhere ([b5e7fecec](https://github.com/supernovae-st/nika/commit/b5e7fececa1f184e0f0494816e926a6fd7f17f1b))
- Refresh version pointers to v0.91.0 / main 0.92.0-dev ([95962d5cd](https://github.com/supernovae-st/nika/commit/95962d5cdca68b0a7992acc0f5bab6f4fce20af0))
- Fix stale version + builtin-count drift across engine docs ([4092a87de](https://github.com/supernovae-st/nika/commit/4092a87ded4e14aad92acac72b7fbec37f7b343f))

### 📦 Build
- **hygiene** — Stop nightly drift-issue spam + resync status block ([#143](https://github.com/supernovae-st/nika/issues/143)) ([c64082276](https://github.com/supernovae-st/nika/commit/c640822766a89dc8d79a5a5509fb2a552392ce52)) ([#143](https://github.com/supernovae-st/nika/pull/143))

### 🧹 Chore
- **catalog** — Refresh nika-types public-api baseline ([4a59e2551](https://github.com/supernovae-st/nika/commit/4a59e2551860b20d4a5fde14f6121c2424c0baef))
- **ci** — Lock 8 more public-api surfaces — vector 38 ratchet 27/38 → 35/38 ([1f398ef9f](https://github.com/supernovae-st/nika/commit/1f398ef9fef4ec27593c1c5baceca3e6d39d33c9))

## [0.91.0](https://github.com/supernovae-st/nika/compare/v0.90.0..v0.91.0) - 2026-06-25

### ✨ Features
- **cli** — Add explicit nika onboarding wiring ([fce281a1a](https://github.com/supernovae-st/nika/commit/fce281a1a031a711441804aaa0c381cf69e66dc6))
- **nika-cli** — Examples run --model override + offline mock hint ([ede69ccfd](https://github.com/supernovae-st/nika/commit/ede69ccfd2b025af4575293742772d5c09adf3af))
- **nika-screen** — Optional xcap backend (default-off) for headless builds ([9f2281f05](https://github.com/supernovae-st/nika/commit/9f2281f0588b124a59fa9f9a9cb4eb08c29900c4))

### 🐛 Bug Fixes
- **ci** — Clippy excludes macOS-only metal feature on the Linux runner ([93b7990a9](https://github.com/supernovae-st/nika/commit/93b7990a94d3ac71bb87c66b2b97f0c5d6d4f4dc))
- **doctor** — Recognize workspace cursor mcp config ([723fcd9e2](https://github.com/supernovae-st/nika/commit/723fcd9e2220f42c787dfe523966768c929c8f56))
- **nika-a11y** — Atspi 0.29 API — Role::Button + direct zbus dep ([53c19ef88](https://github.com/supernovae-st/nika/commit/53c19ef88552cb637fcbeb7fbbdee814efc28edf))
- **nika-a11y** — Make the atspi walk future Send ([ed055d94d](https://github.com/supernovae-st/nika/commit/ed055d94dd52bc5d56e5c3133c72202170fdc256))
- **nika-a11y** — Cfg-gate macOS-only AX helpers (Linux dead-code) ([bd9645483](https://github.com/supernovae-st/nika/commit/bd9645483fe186db1532a43c646bfa3a3977af99))
- **nika-a11y** — Backtick PascalCase in the atspi doc comment ([6e8960f51](https://github.com/supernovae-st/nika/commit/6e8960f51180ebcc20bf5cc486873a69ba14ec07))
- **nika-catalog** — Gate pricing/capabilities-conditional imports ([6648a389f](https://github.com/supernovae-st/nika/commit/6648a389f03b2f6dde75211ac5d885b18e01317e))
- **nika-types** — Gate serde so the no-default-features build compiles ([05e0de2d9](https://github.com/supernovae-st/nika/commit/05e0de2d92df29c211bcf3d1cd52029045c9604f))
- **release** — Separate dev version from brew assets ([16b9ac480](https://github.com/supernovae-st/nika/commit/16b9ac480d44f6ee110c4537eea87d07bf29a8c1))
- **typos** — Exclude generated/fuzz + allowlist domain vocab ([33411ce7c](https://github.com/supernovae-st/nika/commit/33411ce7c78fca318d6e63c5a03e0769ff062210))

### 📚 Documentation
- **changelog** — Append v0.90.0 — auto-generated ([#133](https://github.com/supernovae-st/nika/issues/133)) ([a424d559b](https://github.com/supernovae-st/nika/commit/a424d559b05cb078fa46d6d42a68dfa095db1164)) ([#133](https://github.com/supernovae-st/nika/pull/133))
- **coherence** — Fix stale facts post-ship + tag-cleanup ([de6fcbf49](https://github.com/supernovae-st/nika/commit/de6fcbf4942569e11db9a3258aae43ab36b48c36))
- **kernel** — Retire forever-v0.x doc refs (real-semver cascade) ([5b60fa0aa](https://github.com/supernovae-st/nika/commit/5b60fa0aa4023c78b93e3f8c019f2ede39ad8e44))
- **readme** — Real brew install + zero-setup quickstart; fix version residual ([b2ba50821](https://github.com/supernovae-st/nika/commit/b2ba5082140b444b40b4b8b060d7350591d1be71))
- **readme** — Document curl install + add editor-support section ([55551e4ce](https://github.com/supernovae-st/nika/commit/55551e4ce3c984adea60040d66a2b04fde38840e))
- **readme** — Correct the install-script PATH note ([895ce3a11](https://github.com/supernovae-st/nika/commit/895ce3a1182b3788b0e9ffcf5ecf0ed356b2f497))
- **release** — Clarify main versus tagged binaries ([47ff5f5d5](https://github.com/supernovae-st/nika/commit/47ff5f5d581bfe77af587df96da02bd7bb3aec9d))
- **roadmap** — Current-state reflects v0.90.0 SHIPPED (first public release) ([38218ab7c](https://github.com/supernovae-st/nika/commit/38218ab7c7dcf60f11886f05bc861941245bb68b))
- **status** — Sync dev version projections ([60c5a7828](https://github.com/supernovae-st/nika/commit/60c5a78285711a3642a1f8ec64ba56dddb107ede))

### 📦 Build
- **diamond** — Pipewire dep + ignore unmaintained paste advisory ([450477646](https://github.com/supernovae-st/nika/commit/4504776466af1e7095828629748f0c2b59bd042e))
- **diamond** — Egl/gl deps + miri toolchain pin + typos allowlist ([26d1cd098](https://github.com/supernovae-st/nika/commit/26d1cd098a56520e99aad0f91eab9cc02550bdb3))
- **diamond** — Exclude xcap from clippy features (system-lib, like metal) ([bf5d91ddc](https://github.com/supernovae-st/nika/commit/bf5d91ddc128e1024874c26dbb25ffed7fa5cd70))

### 🧹 Chore
- **hooks** — Accept nika-<crate> + diamond/coherence commit scopes ([744d1202d](https://github.com/supernovae-st/nika/commit/744d1202d2d2a1ca11200823aa80bca6cd8f9d58))
- **hooks** — Add typos to the commit-scope allowlist ([c0f7c070b](https://github.com/supernovae-st/nika/commit/c0f7c070b9e42587b12733ce1e99f95c6ee680f9))

### 🦋 New Contributors
- @github-actions[bot] made their first contribution in [#133](https://github.com/supernovae-st/nika/pull/133)

## [0.90.0](https://github.com/supernovae-st/nika/compare/v0.80.0-alpha.4..v0.90.0) - 2026-06-21

### 🆕 Crates admitted
- **nika-a11y** — Admit to workspace — all 12 gates passed ([047e180d1](https://github.com/supernovae-st/nika/commit/047e180d196c984eab516fe17a9a6d5e3bbb00d6))
- **nika-blob** — Admit to workspace — all 12 gates passed ([e91adcef2](https://github.com/supernovae-st/nika/commit/e91adcef273eeb88f24ed8c91f8828581830cc4d))
- **nika-bm25** — Admit to workspace — all 12 gates passed ([36ca8c3ee](https://github.com/supernovae-st/nika/commit/36ca8c3eed8ab2154fd9a0db59bd1a6a6de39f26))
- **nika-browser** — Admit to workspace — all 12 gates passed ([e1bce0283](https://github.com/supernovae-st/nika/commit/e1bce0283165c33e4689f1eceab89c9ff8a9cf95))
- **nika-builtin** — Admit to workspace — 12 gates (Gate 5 via GATE5-EXEMPT budget) ([16142cf60](https://github.com/supernovae-st/nika/commit/16142cf605957539a6f4b3a03aec80b9ce5d029f))
- **nika-catalog-codegen** — Admit to workspace — 12 gates passed (10/12 + 2 deferred) ([23ab2fef8](https://github.com/supernovae-st/nika/commit/23ab2fef8e080e391374d359478adafb8a2543f6))
- **nika-cli** — Admit to workspace — all 12 gates passed ([a904c2db7](https://github.com/supernovae-st/nika/commit/a904c2db7196e77d1b21a72c6756aedc07ded9a1))
- **nika-clock** — Admit to workspace — all 12 gates passed ([74a8ff483](https://github.com/supernovae-st/nika/commit/74a8ff48373c80047dfd8c85142288e3d3a7a00f))
- **nika-event** — Admit to workspace — all 12 gates passed ([d009b1dd8](https://github.com/supernovae-st/nika/commit/d009b1dd8f46cd3cfaffaaff5dccef637b1dd913))
- **nika-exec-runner** — Admit to workspace — all 12 gates passed ([7ba7b51d8](https://github.com/supernovae-st/nika/commit/7ba7b51d8915638e3366717dbb010e7e7f332ab6))
- **nika-extract** — Admit to workspace — all 12 gates passed ([28ba11760](https://github.com/supernovae-st/nika/commit/28ba1176038e0d3200c47220bf6aa7328d7c20c5))
- **nika-fs** — Admit to workspace — all 12 gates passed ([47825df4a](https://github.com/supernovae-st/nika/commit/47825df4a460d01791bb7c5ad86eda1b7faab95b))
- **nika-http** — Admit to workspace — all 12 gates passed ([221c5d5a9](https://github.com/supernovae-st/nika/commit/221c5d5a98eed2e0720e7d55f91eb93c6c717858))
- **nika-input** — Admit to workspace — all 12 gates passed ([e4686eccb](https://github.com/supernovae-st/nika/commit/e4686eccbaf354e188eaecadf8a883b10b325ecf))
- **nika-kernel-ai** — Admit to workspace — kernel split step 3 (ai sibling) ([9eb6e225c](https://github.com/supernovae-st/nika/commit/9eb6e225c1c965f0fd60111428f8e585f43cf7b8))
- **nika-kernel-core** — Admit to workspace — kernel split step 2 (base sibling) ([7180576b0](https://github.com/supernovae-st/nika/commit/7180576b0377f79e0b3273cd5e6b82509390ad3b))
- **nika-kernel-plugin** — Admit to workspace — kernel split step 5 (plugin sibling) ([27bf36d78](https://github.com/supernovae-st/nika/commit/27bf36d788ab0b77fff52af579b565363a124ca9))
- **nika-kernel-runtime** — Admit to workspace — kernel split step 4 (runtime sibling) ([393ddef86](https://github.com/supernovae-st/nika/commit/393ddef86acbc64967b4acf6b72ef90ea01245ea))
- **nika-lsp** — Admit to workspace — all 12 gates passed ([85ba7f513](https://github.com/supernovae-st/nika/commit/85ba7f513fcebb3e6dbf6273dd91362ba874a552))
- **nika-mcp** — Admit to workspace — all 12 gates passed ([850f1219f](https://github.com/supernovae-st/nika/commit/850f1219ff6d42edda92b76af23c21c68689339b))
- **nika-ocr** — Admit to workspace — all 12 gates passed ([2541a9181](https://github.com/supernovae-st/nika/commit/2541a91818f4b2e0d3a6f7ebb84e24047cd6b7f2))
- **nika-pack** — Admit to workspace — all 12 gates passed ([5f37637c3](https://github.com/supernovae-st/nika/commit/5f37637c3cd8336a26d70d32071585d8ee7ee5dc)) ([#112](https://github.com/supernovae-st/nika/pull/112))
- **nika-providers** — Admit to workspace — all 12 gates passed ([9537dcf82](https://github.com/supernovae-st/nika/commit/9537dcf82c47ba1b5be3b40e9e80f2b8dfa798c1))
- **nika-runtime** — Admit to workspace — all 12 gates passed ([2e0386d3a](https://github.com/supernovae-st/nika/commit/2e0386d3a0603957d113b8522de9c4494d73b32a))
- **nika-schema** — Admit to workspace — all 12 gates passed ([99c4dfb00](https://github.com/supernovae-st/nika/commit/99c4dfb003b7b129021875f17c4ea9d0da56b7ae))
- **nika-screen** — Admit to workspace — all 12 gates passed ([181da3148](https://github.com/supernovae-st/nika/commit/181da3148868d5a41f22711489c1e3b8a1a18157))
- **nika-verb-agent** — Admit to workspace — all 12 gates passed ([0e2900d92](https://github.com/supernovae-st/nika/commit/0e2900d92e5ffb549f3ebe2e4b4e815f75a70089))
- **nika-verb-exec** — Admit to workspace — all 12 gates passed ([9b3284979](https://github.com/supernovae-st/nika/commit/9b3284979aaff149dd5ed1ef8e6c0fe82cba0e32))
- **nika-verb-infer** — Admit to workspace — all 12 gates passed ([c5a0b3e74](https://github.com/supernovae-st/nika/commit/c5a0b3e74d1be60a65e00eac9360848e7f347dd5))
- **nika-verb-invoke** — Admit to workspace — all 12 gates passed ([11c42947a](https://github.com/supernovae-st/nika/commit/11c42947a2a0cc9cc834a7db4e9cb6494962b16d))

### ✨ Features
- **adr** — Supersedes-DAG cycle detection + self-contained contract ([8c7559cd4](https://github.com/supernovae-st/nika/commit/8c7559cd49c602e2e0601b8674695bbd28922c5b))
- **arch** — Decide + gate the kernel I/O error convention (Pattern A universal) ([e72a3e12a](https://github.com/supernovae-st/nika/commit/e72a3e12ad9b682df7e22044baa729cf92c1da28))
- **ci** — Add check-crate-gates.sh emitting olympus CrateGates JSON ([01f18a381](https://github.com/supernovae-st/nika/commit/01f18a381bb50573ac4bde632f3b1952f8a84404))
- **ci** — Real executable Gate 5 — cargo-mutants kill-floor enforcement ([97c8ad476](https://github.com/supernovae-st/nika/commit/97c8ad47627cacf243bc0e810505ddd979fdc148))
- **ci** — Mutation cross-platform calibration + public-API coverage ratchet ([6dce16899](https://github.com/supernovae-st/nika/commit/6dce1689980b742eba9bce5ed649d64056983b9d))
- **ci** — Lift public-API coverage 5→15 + project the floor into CI ([9688a2916](https://github.com/supernovae-st/nika/commit/9688a29162400211e1f28a9da0eac6c6b3eec94e))
- **ci** — Floor public-API for nika-fs/http/blob (post-merge coherence) ([5097478c2](https://github.com/supernovae-st/nika/commit/5097478c2cd1ce19ef4a6afd9a0cc316485fb42d))
- **dx** — Wire HQ dashboard hooks + /dashboard command ([3a6c11e32](https://github.com/supernovae-st/nika/commit/3a6c11e32500955ea333f3694fa15527bb577157))
- **dx** — Live roadmap projection + fix layer-count digit regex ([c00e1d16b](https://github.com/supernovae-st/nika/commit/c00e1d16bcb10b2d24c259fb5936c96aabd8f706))
- **error** — Wire NIKA-601..604 memory subsystem codes · diamond w2.1 ([8a02d152f](https://github.com/supernovae-st/nika/commit/8a02d152f429f7c21cdbf35e4804c8629a9bb7af))
- **fuzz** — Cargo-fuzz harness · 2 targets · corpus · nightly ci ([1b91aebb8](https://github.com/supernovae-st/nika/commit/1b91aebb8e06e9f3bf1677f6cf7fe5821ed24ff1))
- **hooks** — Post-commit auto-fires olympus xtask in background (wave 3a) ([220e8d9cb](https://github.com/supernovae-st/nika/commit/220e8d9cb0488e93a5f4e737c1ca4a3cfecd9c5e))
- **hygiene** — Autonomous ecosystem hygiene stack — WAVE 1+2 ([a8c01194d](https://github.com/supernovae-st/nika/commit/a8c01194da213a4c3e4b4cb36cef1327f5ab4d3c))
- **hygiene** — Vector 23 — status-claims-sync (Phase B.2) ([17602d073](https://github.com/supernovae-st/nika/commit/17602d0733876e546cde1f6dd55fbb18e6209341))
- **hygiene** — Vector 12 three-tier file-LOC + clippy too_many_lines (B.3, ADR-023) ([2225ac52f](https://github.com/supernovae-st/nika/commit/2225ac52fa080c2e88c9fb53d7d971f3a2c2444b))
- **hygiene** — Vector 25 — L0 sibling-dep fanout cap (B.4, ADR-027) ([8d35a946d](https://github.com/supernovae-st/nika/commit/8d35a946df4f6dd2bc0acabeb310b1761b0a77a0))
- **hygiene** — +vector 30 cancel-safety docs on kernel async fn (batch i.b) ([65700834a](https://github.com/supernovae-st/nika/commit/65700834a076601a424dcec3b8de5df5b6c16a21))
- **hygiene** — +vector 33 layer-deps bans (batch i.b) ([4bb4082d8](https://github.com/supernovae-st/nika/commit/4bb4082d867b81c1867931064970e576afcd48bb))
- **hygiene** — 3 new gates — ADR-081 guard enforcement + supply-chain policy ([b9e0bd75d](https://github.com/supernovae-st/nika/commit/b9e0bd75db4f825ddc3982debfa57cc0295eeaa7))
- **hygiene** — Vector 37 — error one-voice doctrine enforcement ([9c05e88d6](https://github.com/supernovae-st/nika/commit/9c05e88d6f8b4e34846be9d796fa170bbdfb2fcb))
- **hygiene** — Crate-spec LOC anchors deterministic — projector + freshness gate ([dfb3f6266](https://github.com/supernovae-st/nika/commit/dfb3f62667c97831e273f6a29fe35bb71c1b7fd7))
- **kernel** — Type the Fs traits with FsError, not std::io::Result ([5c19de82f](https://github.com/supernovae-st/nika/commit/5c19de82ff4d62f75927bbf0ef5b113d194abf8f))
- **kernel** — Type input + browser traits — Pattern A 100% uniform ([88371e56d](https://github.com/supernovae-st/nika/commit/88371e56d5cf300049b1d39e424f724b7807a3f5))
- **kernel** — Command-sandbox seam — OS confinement for the exec child ([a9abd4c3e](https://github.com/supernovae-st/nika/commit/a9abd4c3e83434014d6b8e3cfaf54e705a4e9b52))
- **kernel-a11y** — M1.3 add l0.5 io::a11y sealed traits + dtos ([e969e351f](https://github.com/supernovae-st/nika/commit/e969e351fc40fba99031e4477094779a88f82af7))
- **kernel-browser** — M1.5 add l0.5 io::browser sealed traits + dtos ([92aaf9b3c](https://github.com/supernovae-st/nika/commit/92aaf9b3cad5b2fdfe84ff4a6996a07c2d289d32))
- **kernel-input** — M1.4 add l0.5 io::input type-state trait + dtos ([0b0406167](https://github.com/supernovae-st/nika/commit/0b0406167749737e6fdd2ce13754cebdb51c11f5))
- **kernel-mock** — Enqueue_ok_with_headers — header-carrying canned responses ([cd9c98f77](https://github.com/supernovae-st/nika/commit/cd9c98f771b1ce1f28b767675a80009e1e8995d2))
- **kernel-ocr** — M1.2 add l0.5 io::ocr sealed traits + dtos ([e0bfa5b26](https://github.com/supernovae-st/nika/commit/e0bfa5b26583cbbab59ebf0fde6d9e00f105c9fb))
- **kernel-screen** — M1.1 add l0.5 io::screen sealed traits + dtos ([344b853ba](https://github.com/supernovae-st/nika/commit/344b853ba08f68aac8cc08da0b78f7974f8f9036))
- **kernel-screen** — M2.1.b1 capture_stream additive trait method ([da0a83358](https://github.com/supernovae-st/nika/commit/da0a83358d99f66ffa4aa96a3feeb357b2d1e66e))
- **kernel-vision** — M1.6 add l0.5 ai::vision sealed traits + dtos · m1 sealed ([1d3ff38e6](https://github.com/supernovae-st/nika/commit/1d3ff38e69e83f21ae5e826d1dd73247c53d0369))
- **mintlify** — Rebuild introduction with live snapshot + journey table ([58e0ab47d](https://github.com/supernovae-st/nika/commit/58e0ab47d0cb95c60606312aa0b239d7c3e7f564))
- **mintlify** — Architecture tab — layers + FCI + L0 decisions + admission ([74f18e211](https://github.com/supernovae-st/nika/commit/74f18e2115a33b4edd49af41072b4b4bdf6eab8e))
- **mintlify** — Reference — providers catalog (32 providers, 7 dialects) ([245243a38](https://github.com/supernovae-st/nika/commit/245243a38851ddb6395b1b107972050e36bac595))
- **mintlify** — Reference — capability rules (49 rules, 4 match kinds) ([2594b3b8e](https://github.com/supernovae-st/nika/commit/2594b3b8e7c160f5456af769bd6ba55d543ed495))
- **mintlify** — Concepts — architecture + providers rebuilt from current state ([ad55454e5](https://github.com/supernovae-st/nika/commit/ad55454e5bd4e495882938d0e311bb36cd4b439e))
- **mintlify** — Getting-started — honest v0.80 pre-release framing ([5e18782db](https://github.com/supernovae-st/nika/commit/5e18782db288657929c55ceab5383adace6cae99))
- **mintlify** — Architecture — ADR index (35 records, 11 thematic groups) ([1a1f2fc03](https://github.com/supernovae-st/nika/commit/1a1f2fc03ee106d2c2124542fabd941e588d2b4e))
- **mintlify** — Changelog tab — releases + roadmap + forever-v0.x ([edc91bbf9](https://github.com/supernovae-st/nika/commit/edc91bbf92c8e9d98e06da344d088ab0d6c7f641))
- **nika-a11y** — M2.3.b1 spec + b2 skeleton + guard 3 redaction ([54716278b](https://github.com/supernovae-st/nika/commit/54716278bead055726a74b364cddbedb6b09c5f9))
- **nika-a11y** — M2.3.b3 wire macos axuielement walk + ref cache ([a3ec54ee8](https://github.com/supernovae-st/nika/commit/a3ec54ee8b6538586511bf12c9394647e3cd8eb6))
- **nika-a11y** — Cross-platform linux atspi backend + guard 3 fix ([6cbd8108a](https://github.com/supernovae-st/nika/commit/6cbd8108af29993bfbe241cc1e61c5001e20581d))
- **nika-a11y** — Error one-voice — A11yError speaks NikaErrorCode ([662daf30c](https://github.com/supernovae-st/nika/commit/662daf30cff6af2207788fd7b2cb72fb5827b4be))
- **nika-bm25** — W3 admission prep · gate 1 spec + gate 3 scaffold ([3444e4131](https://github.com/supernovae-st/nika/commit/3444e41310ba8601d33b23bd8ec262092254e430))
- **nika-bm25** — Gate 3 green · pure-algo bm25 kernel shipped ([92e5d39fb](https://github.com/supernovae-st/nika/commit/92e5d39fbfb4fa848c785beb1673b14b78c9b698))
- **nika-bm25** — Gate 6 proptest + gate 7 criterion bench ([2da7c1e24](https://github.com/supernovae-st/nika/commit/2da7c1e2494fbf3751b715a42ed46190babb55a8))
- **nika-bm25** — EagerIndex — the BM25S eager sparse scoring architecture ([1fe44af34](https://github.com/supernovae-st/nika/commit/1fe44af345cae9166bd9d4b7f4c1c7abc3c2d844))
- **nika-bm25** — Activate BM25+ — the reserved delta wired through both paths ([27fa7ed1f](https://github.com/supernovae-st/nika/commit/27fa7ed1f733ce7a3814b0897e3ed068165213c1))
- **nika-bm25** — Canonical bm25+ preset — the lv-zhai delta default ([2402ed4f5](https://github.com/supernovae-st/nika/commit/2402ed4f50e1b9de1f5ad2881377935eaa2702f2))
- **nika-bm25** — MaxScore dynamic pruning + the research-conformance suite ([b7e5634a0](https://github.com/supernovae-st/nika/commit/b7e5634a0131dc778f34a96ea0dd5e1507f4e400))
- **nika-bm25,adr-039** — Sota 2026 q2 expert convergence · 10 locks ([82741170f](https://github.com/supernovae-st/nika/commit/82741170fbe893c58989723154402b863367c071))
- **nika-browser** — Scaffold security core — guard 5 pure verify ([eb6f07e0d](https://github.com/supernovae-st/nika/commit/eb6f07e0d25a82ee392f421487a8b956be81b592))
- **nika-browser** — Wire chromiumoxide backend — B.3, smoke-verified ([3e348cc43](https://github.com/supernovae-st/nika/commit/3e348cc431f8661442eda6cb13596cf8cdd60bcc))
- **nika-browser** — Guard 5 occlusion hit-test — SOTA actionability ([24e401e93](https://github.com/supernovae-st/nika/commit/24e401e930e8964a48e87703fe89bdc8c47cc70e))
- **nika-builtin** — Seed the 22-builtin dispatcher — the real tool layer (s16 · WIP) ([4ab88adeb](https://github.com/supernovae-st/nika/commit/4ab88adeb221dd55b0a1a9490ea0c38051d91604))
- **nika-builtin** — Fold the 3-lens review — transient · wait until · date six · jq bounds ([1d3025a56](https://github.com/supernovae-st/nika/commit/1d3025a5671b271c713a272609877e9e0c3d3c4f))
- **nika-builtin** — Wire the nika:fetch extract modes (step 13) ([3923767a9](https://github.com/supernovae-st/nika/commit/3923767a91dda46c50da4b8758c01e5a5a3d717f))
- **nika-builtin** — Fetch truth pass — runtime pairing mirrors · Cow decode · feed bytes · battery ([bbd8a3c19](https://github.com/supernovae-st/nika/commit/bbd8a3c195fb52ad4458846ca0a0842944197223))
- **nika-builtin** — Charset-aware decode for fetch extract modes ([c1487cc8f](https://github.com/supernovae-st/nika/commit/c1487cc8f6799a1e09ea397b998bae7969e86708))
- **nika-builtin** — Conformance pair + tz honesty — binary write · status details ([eda483459](https://github.com/supernovae-st/nika/commit/eda483459228d94696b73f28519bff8728c2626d))
- **nika-builtin** — Three Rams quick-wins — notify data · validate structured · log level clamp ([2f4b47470](https://github.com/supernovae-st/nika/commit/2f4b474702ee04c1b306fafb1be37bb5697d2331))
- **nika-builtin** — Nika:write serializes structured content to JSON ([53a721fae](https://github.com/supernovae-st/nika/commit/53a721fae5c10173742e5658d0aa82c2e0bce76e))
- **nika-catalog** — Tag enum + ParseTagError + FromStr ([cc3a1482f](https://github.com/supernovae-st/nika/commit/cc3a1482fd999c7b48796d80f3fd22bd456cf277))
- **nika-catalog** — Add tags + extra_tags fields to Provider, McpServer, Embedding ([bc21e5c23](https://github.com/supernovae-st/nika/commit/bc21e5c23a116bbf09df0c800807446869e6620b))
- **nika-catalog** — Populate tags across 3 catalogs + sort/dedup assertions ([a848b1916](https://github.com/supernovae-st/nika/commit/a848b1916bf98ff9588e20d627a7adc639e109c8))
- **nika-catalog** — Cargo features for subset compilation ([ceccfc39b](https://github.com/supernovae-st/nika/commit/ceccfc39b51fdbad965f2edf5fe3b2d5fa45931f))
- **nika-catalog** — MCP safety-tag XOR enforcement + runtime tag invariants ([83a9afaf4](https://github.com/supernovae-st/nika/commit/83a9afaf4b3d9e15a6c51b321550a5d9b44c69c2))
- **nika-catalog** — Session 2b foundation — modality + tokenizer + param_flag ([4f43db883](https://github.com/supernovae-st/nika/commit/4f43db88373a354c4ef633077e781d444e64c93a))
- **nika-catalog** — Session 2b — grow ModelCapabilities + CapPatch + codegen (no new rules) ([b2b8ce190](https://github.com/supernovae-st/nika/commit/b2b8ce1905387d42ef10fc09e5da401a1b30af62))
- **nika-catalog** — Session 2b rules — 28 capability rules + per-rule provenance ([1e0fd93bf](https://github.com/supernovae-st/nika/commit/1e0fd93bfb94df71460770e60c41f97a95e81fa7))
- **nika-catalog** — Session 3 — add 4 providers + 14 capability rules ([4d085afb0](https://github.com/supernovae-st/nika/commit/4d085afb05e2e2556568b6018db09b4ad3338502))
- **nika-catalog** — Add TokenizerFamily::Qwen variant ([4dbbe5db9](https://github.com/supernovae-st/nika/commit/4dbbe5db953890f7a1d744f0fa7653c95157c87f))
- **nika-catalog** — Pricing — add cached_input / image / reasoning axes ([1b3ea2e26](https://github.com/supernovae-st/nika/commit/1b3ea2e26f75f09aa3e160204de573884812a927))
- **nika-catalog** — Add NIKA-230..235 catalog error codes ([41198ec08](https://github.com/supernovae-st/nika/commit/41198ec08434024506ddb5cd9667824d70790d41))
- **nika-catalog** — Add context_window_tokens + max_output_tokens fields ([0fcabf6ef](https://github.com/supernovae-st/nika/commit/0fcabf6ef4c028b6a31a71817381e8b5478ffd59))
- **nika-catalog** — Add JsonMode enum, delete StructuredOutputNative ([6b74109aa](https://github.com/supernovae-st/nika/commit/6b74109aaf6dd429d22f9b07c1deb2e3dfed0a06))
- **nika-catalog** — Add ModelCapabilitiesView trait (Cortex v0.95) ([a142575f7](https://github.com/supernovae-st/nika/commit/a142575f70428aa7a196a5572b16fa802c3ed0a1))
- **nika-catalog** — Add Matcher::ContainsAny with word-boundary anchoring ([f8bcca19c](https://github.com/supernovae-st/nika/commit/f8bcca19c8aeea83a769ed5a99dea67622758dfa))
- **nika-catalog** — Promote CapRule/CapPatch/Matcher to pub #[non_exhaustive] ([69f10f5a9](https://github.com/supernovae-st/nika/commit/69f10f5a9062ff4afb23ff8485730fbdad1030b5))
- **nika-catalog** — Add Region enum and CapRule region scope dimension ([bb0a8c6e9](https://github.com/supernovae-st/nika/commit/bb0a8c6e90f7944366fcb63c5d90f0a6abce8755))
- **nika-catalog** — Toml-driven pricing, split cache_write/cache_read ([9453971db](https://github.com/supernovae-st/nika/commit/9453971db2fe62d7fcc96883e03345e4fe46d023))
- **nika-catalog** — Add CatalogDataSource trait + OverlayOrigin enum ([e88c3a78c](https://github.com/supernovae-st/nika/commit/e88c3a78ce36aa11e597ac24aa6c2140fabba058))
- **nika-catalog** — Add criterion benchmarks for catalog hot paths ([5998c39fc](https://github.com/supernovae-st/nika/commit/5998c39fc62ada5a463a5003933f372d8d7b080a))
- **nika-catalog** — Add 6 new ParamFlags (OpenRouter vocab align) ([e9e77a258](https://github.com/supernovae-st/nika/commit/e9e77a258eea3176344623790a4563c62cde4ee7))
- **nika-catalog** — Add 3 new Modalities (Embedding, Speech, ImageGen) ([892d5aff5](https://github.com/supernovae-st/nika/commit/892d5aff540c8f09bbf43b2ad44bee2c25004491))
- **nika-catalog** — Add 4 new TokenizerFamilies (LlamaV4/Granite/Glm/Grok) ([5dc3f2735](https://github.com/supernovae-st/nika/commit/5dc3f27358f612451f2bd31ea65d8ffd4f57b481))
- **nika-catalog** — Add 7 new providers + capability rules ([0415880c7](https://github.com/supernovae-st/nika/commit/0415880c79c7d5ede9dac7963e891025e9002aff))
- **nika-cel** — Admit the cel-subset/0.1 expression engine (L0) ([af4c2f8b8](https://github.com/supernovae-st/nika/commit/af4c2f8b885318f12be5886f0e006b73d403e369))
- **nika-cli** — Seed the L4 operator surface — display fold + trace reader + e2e pipeline rehearsal ([9fe99a5f8](https://github.com/supernovae-st/nika/commit/9fe99a5f80543b336da5bc4afdc3dceb216d9573))
- **nika-cli** — The static verb suite — audit a workflow before a single token ([b64c1074f](https://github.com/supernovae-st/nika/commit/b64c1074fd7623d1a4100a2c8ff617521337f245))
- **nika-cli** — Explain teaches spec codes + PLAN says the true width ([7b837d3a1](https://github.com/supernovae-st/nika/commit/7b837d3a14acbbb43cdba5781d754d481ab03733))
- **nika-cli** — The §3.1 state machine completes — retrying + cancelled ([d718f426e](https://github.com/supernovae-st/nika/commit/d718f426e8de83a14ef51d6c0679585fc2094645))
- **nika-cli** — Inspect carries the engineering read ([f523a16ea](https://github.com/supernovae-st/nika/commit/f523a16ea73164cd6645c0ad68703f4aa349b9a8))
- **nika-cli** — New routes free-form intent to the best template ([dc17c22f8](https://github.com/supernovae-st/nika/commit/dc17c22f827e53d9ee3b97f1757860a1142517f0))
- **nika-cli** — The nika run composer foundation — production seams + the two bridges ([68ed4a44c](https://github.com/supernovae-st/nika/commit/68ed4a44c5cdcb42a9776dfa3d7ebc80d6d47ff5))
- **nika-cli** — Nika run — the verb executes a checked workflow for real ([10f4ccdd6](https://github.com/supernovae-st/nika/commit/10f4ccdd64079a208aaab4d0aacb2debb0a5aec3))
- **nika-cli** — Examples run flips from refusal to execution ([6c1c93f16](https://github.com/supernovae-st/nika/commit/6c1c93f16005d002f68ee962ea07bd6e7d8255c2))
- **nika-cli** — Wire the nika lsp subcommand ([d290d809e](https://github.com/supernovae-st/nika/commit/d290d809ebd2c7d3c78ad594f7a663300663b74c))
- **nika-cli** — Explain teaches per-builtin NIKA-BUILTIN-<NAME> codes ([8d7a81754](https://github.com/supernovae-st/nika/commit/8d7a81754f95280313e3b1b1b63301176834e856))
- **nika-cli** — Nika run --output json emits the outputs: contract on stdout ([6b8dab5ce](https://github.com/supernovae-st/nika/commit/6b8dab5ce9d71b35659831a18735910e36e2b751))
- **nika-cli** — Nika doctor — environment diagnosis (spec §8 floor) ([c4b6472f0](https://github.com/supernovae-st/nika/commit/c4b6472f0ed91d1861487c63838bb99101db5f03))
- **nika-cli** — Nika init — scaffold a repo (spec §2 floor) ([65d7e24fc](https://github.com/supernovae-st/nika/commit/65d7e24fc1aff2658fd6df5466e322fbe78bc9d6))
- **nika-cli** — Check warns about required inputs before run ([2d7753e43](https://github.com/supernovae-st/nika/commit/2d7753e436f602db66d372b802f1a888fc8525e6))
- **nika-cli** — Explain teaches the NIKA-PROVIDER namespace ([88b3e0e3e](https://github.com/supernovae-st/nika/commit/88b3e0e3ef4da15034c1f3dd7c44c86e7a195e01))
- **nika-cli** — Add --no-progress/--quiet/--dry-run run flags ([eb8c8d7f7](https://github.com/supernovae-st/nika/commit/eb8c8d7f790c0665544ac251b28a0fa44e7678c2))
- **nika-compose** — The agent's self-check is nika:compose, the 23rd builtin ([44b701ac8](https://github.com/supernovae-st/nika/commit/44b701ac8fb9435811cbbf5c66e8056454a7d1e6))
- **nika-error** — Add 23 L0 foundational types + evolve kernel DTOs (ADR-033) ([d7b55b1e5](https://github.com/supernovae-st/nika/commit/d7b55b1e5f7dd9c0ce01bce563374c1a22b97404))
- **nika-error** — Cost stdlib arithmetic + checked_add/sub + remove TrustLevel::Default ([83294e026](https://github.com/supernovae-st/nika/commit/83294e026b3c06d01c20054ad933b7cc0990649b))
- **nika-error** — Register M2 computer-use code ranges — NIKA-1000..1206 ([951b3af39](https://github.com/supernovae-st/nika/commit/951b3af396e15686a0f1cee3bc26e3c972351da7))
- **nika-error** — Register verb codes NIKA-430..433 in the registry ([7a2fa2317](https://github.com/supernovae-st/nika/commit/7a2fa2317e671acd85be85b538b109453d62ead0))
- **nika-error** — Register NIKA-467 agent-stalled ([fcd5b1002](https://github.com/supernovae-st/nika/commit/fcd5b10029f5796d9f39d2ad9cf9e24dbd022edc))
- **nika-event** — Close the vocabulary over the display contract — 6 kinds + EventClass ([3820b9039](https://github.com/supernovae-st/nika/commit/3820b9039ff2f82531c2c6e8232e0b535ca2bee8))
- **nika-event** — The agent-loop telemetry vocabulary — 5 kinds + EventClass::Agent ([ea4ee3188](https://github.com/supernovae-st/nika/commit/ea4ee3188e09a3bbbb888f4141a0baec7911837b))
- **nika-exec-runner** — Argv program-floor, shell-line tripwire ([2efc48492](https://github.com/supernovae-st/nika/commit/2efc48492f14c2422aead9595f285663f759a1c0))
- **nika-exec-runner** — Strip dangerous-env injection vectors ([d5e63886a](https://github.com/supernovae-st/nika/commit/d5e63886af77fe894c75adc718c13aeb67f9e0d5))
- **nika-exec-runner** — Process-group kill — reap grandchildren ([6c5b10743](https://github.com/supernovae-st/nika/commit/6c5b10743e3308f0265b3e89ce05bf6975e4ddc7))
- **nika-exec-runner** — Wire the OS sandbox into the spawn path ([05a624f7d](https://github.com/supernovae-st/nika/commit/05a624f7de977c3b13ded9f044b36f089b6f3ffc))
- **nika-extract** — Seed the fetch extraction pipeline — 8 modes, pure (s17) ([241b2ab32](https://github.com/supernovae-st/nika/commit/241b2ab32c42bd9aee489a13b87de317700ce924))
- **nika-extract** — Science-grounded extraction wave — boilerpipe cascade + sitemap truth + RFC 8288 ([6c838f514](https://github.com/supernovae-st/nika/commit/6c838f5149f9fe892bb62bbb9425fd0b560e66e9))
- **nika-extract** — Round-2 hardening — DoS depth guard + adversarial battery + JSON-LD ([e24c875ae](https://github.com/supernovae-st/nika/commit/e24c875ae925ad0a2d569361d2bf612716e3b4d7))
- **nika-extract** — Feed mode surfaces full content + author ([35277fdd9](https://github.com/supernovae-st/nika/commit/35277fdd9e2df981c1c9d8a50f5b8a11ec9e5f99))
- **nika-extract** — Metadata mode extracts schema.org microdata ([0297334d6](https://github.com/supernovae-st/nika/commit/0297334d6e2a99f9330c3c3217adaf136ea724ad))
- **nika-extract** — Article mode → Trafilatura-grade 3-stage cascade ([2f1e1d6d5](https://github.com/supernovae-st/nika/commit/2f1e1d6d5842119c4b6d4e9a62b2dc8ec5003f48))
- **nika-extract** — Resolve lazy-loaded images to the real URL ([d0eaf328b](https://github.com/supernovae-st/nika/commit/d0eaf328b80460a40c3b979368b1327febafb45e))
- **nika-extract** — Honor <base href> in links + metadata resolution ([2d952eda2](https://github.com/supernovae-st/nika/commit/2d952eda27fb320e25cb45cf5a45f204f87bc8b3))
- **nika-extract** — Metadata title/description fall back to og/twitter ([b161a941d](https://github.com/supernovae-st/nika/commit/b161a941d9e582346fd95841e14f8b063607b182))
- **nika-extract** — Absolutize og:image/og:url/twitter:image URLs ([9dab7d9d8](https://github.com/supernovae-st/nika/commit/9dab7d9d846fd858a3bb370927f3bc109a505fbe))
- **nika-extract** — Feed items surface attached media (enclosure + MediaRSS) ([ddaee5318](https://github.com/supernovae-st/nika/commit/ddaee5318a26614c4b34acbf09f9a7867832c973))
- **nika-http** — Resolver-enforced SSRF closes the TOCTOU window ([7fdf3ac9e](https://github.com/supernovae-st/nika/commit/7fdf3ac9eb3bff777beb1bef83b1c0f913af0244))
- **nika-http** — Compression + h2 + streaming-true timeouts — the transport wave ([00c262111](https://github.com/supernovae-st/nika/commit/00c2621112b75960a739d4b399fd89da285266a3))
- **nika-infer-local** — Native SOTA decode-time algorithms — min-p, repeat-penalty, token-mask ([27f070f22](https://github.com/supernovae-st/nika/commit/27f070f221467ce24cb4646d5ac2c98cf65f4c46))
- **nika-infer-local** — The candle backend — sovereign inference runs (ADR-091) ([320b94d39](https://github.com/supernovae-st/nika/commit/320b94d39e192851b53dd8fa1d869eef4289ce95))
- **nika-infer-local** — Top-nσ sampling — native, temperature-invariant (arXiv:2411.07641) ([c4a2f0e2d](https://github.com/supernovae-st/nika/commit/c4a2f0e2d7a404cb179ca39f48aa0231dbc157ba))
- **nika-infer-local** — V1 sidecar http server — tiny_http per adr-093 ([34bb2441e](https://github.com/supernovae-st/nika/commit/34bb2441e8dc22b23dc511f3794d6e0236b9fc56))
- **nika-infer-local** — 12-gate admission as the local inference sidecar ([c5052cd4a](https://github.com/supernovae-st/nika/commit/c5052cd4a8f59d9ea5bcf1c49edd7ec470377cea))
- **nika-input** — Scaffold security core — type-state + guards 1+2 ([95fcf4002](https://github.com/supernovae-st/nika/commit/95fcf40021bc4460e75b076b6d8f13ded3aeffe2))
- **nika-input** — Wire enigo backend — B.3 + 3-lens review fixes ([c27bd850e](https://github.com/supernovae-st/nika/commit/c27bd850efb6e595d6243c8e663c643954c69443))
- **nika-kernel** — Add forward-compat seams for v0.95 Cortex + v0.100 WASM (Batch B) ([b68e58d4b](https://github.com/supernovae-st/nika/commit/b68e58d4b298561be7ea28d13fe76d37425f62e8))
- **nika-kernel** — Add 6 L0.5 traits + sealing pattern + mocks (ADR-034) ([32088a76d](https://github.com/supernovae-st/nika/commit/32088a76de26124d2ca6f3b1f4aaaebadfc264b5))
- **nika-kernel** — Inferresponse.cost: option<cost> + structured DenialKind ([0e2c3938e](https://github.com/supernovae-st/nika/commit/0e2c3938e8284b92bc2590f56271f8ea67460a4a))
- **nika-kernel** — Migrate MemoryId to UUIDv7 + deprecate cost_usd ([64afe77e3](https://github.com/supernovae-st/nika/commit/64afe77e3f916d381a5073c0dcb01013702cd466))
- **nika-kernel** — Add HttpStreamResponse::new() + #[non_exhaustive] on 20 mocks ([0aa41e8fe](https://github.com/supernovae-st/nika/commit/0aa41e8fec6c18df6c47c1a42a6e44efdd8ebbc6))
- **nika-kernel** — Add 7 forward-compat seams for v0.95/v0.100 ([a536b03ec](https://github.com/supernovae-st/nika/commit/a536b03ec4bf3b8b678352adc8027cc991388003))
- **nika-kernel** — Add prelude re-export hub (Q7) ([d967f4a7a](https://github.com/supernovae-st/nika/commit/d967f4a7a099a04647147d00aa863280aee80540))
- **nika-kernel** — Add AuditSink trait (Q12 Phase B — compliance channel) ([4be9a00a5](https://github.com/supernovae-st/nika/commit/4be9a00a51eeb020810ad4b99b9ac233e850aab3))
- **nika-kernel** — Genai_attrs OTel semconv bridge (Q13 executed) ([1ff35b759](https://github.com/supernovae-st/nika/commit/1ff35b7597264be4674582bbe71cdee34ecbe3ce))
- **nika-kernel** — Reserve WasmPluginError OutOfFuel + Trap + PluginCallContext (wave 4a r4) ([368820e42](https://github.com/supernovae-st/nika/commit/368820e42a129f33c3b10bf5dd7d8e89f047bb8c))
- **nika-kernel** — Reserve MemoryLifecycle trait with consolidate+prune (wave 4a r5) ([ac46b9ca5](https://github.com/supernovae-st/nika/commit/ac46b9ca5c7fcebda3239e3e746bd916fbaf1ca7))
- **nika-kernel** — Reserve parent_span_id + span links on SpanGuard (wave 4b #1) ([861f09bc9](https://github.com/supernovae-st/nika/commit/861f09bc9bef5dc51fc910f1d3765af26b1517c2))
- **nika-kernel** — Seal MemoryRecall/Remember/Forget per ADR-078 step 1 ([d642ee19e](https://github.com/supernovae-st/nika/commit/d642ee19e0b88329ea87342e0549f7607f7fc43e))
- **nika-kernel-ai** — Reserve ai::audio seam — stt + tts + vad traits (R6) ([1f34f3cc0](https://github.com/supernovae-st/nika/commit/1f34f3cc0a602719040a16b9979bf08652b81d39))
- **nika-kernel-ai** — Type vision + audio errors — Pattern A complete ([6af181177](https://github.com/supernovae-st/nika/commit/6af18117716e2959a96d3e301a6a4cd7904debc6))
- **nika-kernel-ai** — The tool-definition seam — ToolDefinitionProvider ([bb412572f](https://github.com/supernovae-st/nika/commit/bb412572f8b7f3b115d8b13c50aadb4a71294209))
- **nika-kernel-core** — Redact credential headers in http Debug ([8fcf9193d](https://github.com/supernovae-st/nika/commit/8fcf9193dafb7bb6cc38f4c05f55ebd1ce54d48e))
- **nika-lsp** — Editor UX — hover on task refs + bracket completion trigger ([2a3ac4a66](https://github.com/supernovae-st/nika/commit/2a3ac4a660977de14ff2fc6730010e3a43aa7f50))
- **nika-mcp** — In-binary MCP server closes the v0.81 cli floor ([7070b4636](https://github.com/supernovae-st/nika/commit/7070b46367c6bf2c9705564b3ccba843dabfeb61))
- **nika-ocr** — M2.2.b1 spec + b2 skeleton — ocrs backend, NIKA-1100..1109 ([a8013d62d](https://github.com/supernovae-st/nika/commit/a8013d62da4ee81504582ecebcf37be9bd367d2e))
- **nika-ocr** — M2.2.b3 wire real ocrs inference, close skeleton ([0a95cb157](https://github.com/supernovae-st/nika/commit/0a95cb1572cf4c3173c4933a575e2700c7fca30d))
- **nika-ocr** — Error one-voice — OcrError speaks NikaErrorCode ([a9d8ccbf4](https://github.com/supernovae-st/nika/commit/a9d8ccbf4b52984a8e632069d93fc6e835a2b48a))
- **nika-pack** — The 49-code registry + the emitted-within-registered ratchet ([990259c10](https://github.com/supernovae-st/nika/commit/990259c109ede3b497550700915b985b5083d8ce))
- **nika-providers** — Wire the gemini adapter — 14/14 providers ([5c50ffaa0](https://github.com/supernovae-st/nika/commit/5c50ffaa009847c014835baea7b04ac3cab558b6))
- **nika-runtime** — V2 spec-parity engine — concurrency + the full task pipeline ([dbb65c2c4](https://github.com/supernovae-st/nika/commit/dbb65c2c4765de87d46d6ba3d9f0bf5849b4abc5))
- **nika-runtime** — Property battery + jitter-herd fix — round 2 of the socratic pass ([2580838b6](https://github.com/supernovae-st/nika/commit/2580838b697e14455963a1e7c09f46d9e071fc82))
- **nika-runtime** — Agent telemetry wired — decisions on the canonical stream ([e269670e9](https://github.com/supernovae-st/nika/commit/e269670e914408ef23d4089f0b48772f3a9d9433))
- **nika-runtime** — Enforce permits.exec at the exec sink (NIKA-SEC-004) ([7c6cd9ceb](https://github.com/supernovae-st/nika/commit/7c6cd9ceb9350355e899bc454b08ec34c0319290))
- **nika-runtime** — Evaluate full cel-subset/0.1 via nika-cel ([0ae3c4f41](https://github.com/supernovae-st/nika/commit/0ae3c4f41ee6caf60825970fb511d7d1772a3569))
- **nika-runtime** — Output named-bindings + exec structured capture ([46e3f18fe](https://github.com/supernovae-st/nika/commit/46e3f18feac48a1f860d12387bd019e1d76ceb94))
- **nika-runtime** — Resolve workflow secrets from env/file at runtime ([b3264e4d7](https://github.com/supernovae-st/nika/commit/b3264e4d7c32a97aaef928e958309a6e5c31a8f9))
- **nika-runtime** — Warn on a reasoning model's blank answer (OBS-E) ([f18fef511](https://github.com/supernovae-st/nika/commit/f18fef51159734a02efe09510516068fc3cf81c4))
- **nika-sandbox-seatbelt** — MacOS command sandbox — adversarially verified ([c0941f145](https://github.com/supernovae-st/nika/commit/c0941f1450ac0808b22fb71ba9063bd33bb5c201))
- **nika-schema** — Scaffold crate — source tracking + error types (Round 1a) ([668a3b8bc](https://github.com/supernovae-st/nika/commit/668a3b8bc069f7ab41efb73ee3c92192fd9dc9cf))
- **nika-schema** — Add types module — 19 workflow config types (Round 2a) ([1cda7c5d0](https://github.com/supernovae-st/nika/commit/1cda7c5d05ec7bbaf0c3fdfd53ffc20a542822a3))
- **nika-schema** — Add trust, guardrails, and raw AST modules (Round 2b) ([9604d784e](https://github.com/supernovae-st/nika/commit/9604d784e25fca9e9bdde6c03f324f66e43058ee))
- **nika-schema** — Parser skeleton — top-level scalars (Round 2c) ([b85b612ca](https://github.com/supernovae-st/nika/commit/b85b612ca14bdeaee09553b6637b740c3a0677dd))
- **nika-schema** — Task-list parsing with action discriminator (Round 2d) ([2480822df](https://github.com/supernovae-st/nika/commit/2480822df8d40c3c1ce13f77a572294630bf910f))
- **nika-schema** — Task depends_on, condition, for_each (Round 2e-part-1) ([eac346c71](https://github.com/supernovae-st/nika/commit/eac346c71049282b6714c7e1e0fb243dbd9199d0))
- **nika-schema** — Codegen · invoke.tool no-drift gate + ADR-085 ([51ee7195a](https://github.com/supernovae-st/nika/commit/51ee7195a2948e39ecf46e6ee5923b4e197a3a47))
- **nika-schema** — Canonical v1 types — secrets, vars, retry, on_error, duration ([333ab9e9c](https://github.com/supernovae-st/nika/commit/333ab9e9c24401b7838e848bc931a1ff2cc1fcb5))
- **nika-schema** — Canonical raw ast + error taxonomy — envelope, task, verbs ([e5435fa2f](https://github.com/supernovae-st/nika/commit/e5435fa2fe009ed663185755e5d14d3d3d9abcba))
- **nika-schema** — Parser rewrite — canonical keys, strict/lenient, 4 verbs ([cf45bc7cf](https://github.com/supernovae-st/nika/commit/cf45bc7cf42727e05c6a30bf0d47cd34d88739a0))
- **nika-schema** — Expression module — CEL v0.1 subset, hand-rolled L0 ([abb2de0d6](https://github.com/supernovae-st/nika/commit/abb2de0d6e9633be025cb2341f9b739667aa7958))
- **nika-schema** — Analyzer — DAG topology, namespace resolution, when shape ([1e0a0ab68](https://github.com/supernovae-st/nika/commit/1e0a0ab68e9f4c804511a9894d197e76aaa6da3c))
- **nika-schema** — Spec-facing error codes — NIKA-<NS>-<NNN> surface ([a50f45bc7](https://github.com/supernovae-st/nika/commit/a50f45bc75a7ee4258b9dcec25454d628778e8e8))
- **nika-schema** — Core conformance harness + spec examples — 46/46 GREEN ([207a8db2e](https://github.com/supernovae-st/nika/commit/207a8db2e2763e3b3543ba981169d0cfa410c05f))
- **nika-schema** — One-obvious-way lint pass · 7 spec preference rules ([d3d62f797](https://github.com/supernovae-st/nika/commit/d3d62f7970c44b7ad4a35c51869baf593871f4be))
- **nika-schema** — Static binding validation · NIKA-VAR-003 at parse time ([752207aaa](https://github.com/supernovae-st/nika/commit/752207aaa9f035cf87ee91f3b3294862a4a98c9b))
- **nika-schema** — Parse the permits capability boundary ([9dfe2fda6](https://github.com/supernovae-st/nika/commit/9dfe2fda699dd9b7a2498bf8230613793beb5499))
- **nika-schema** — The check module — the nika check static pre-flight ([c369fc7ba](https://github.com/supernovae-st/nika/commit/c369fc7ba482393b854bc0f00a533199f7492c35))
- **nika-schema** — Runnable check example — the pre-flight, available now ([057b4bdd1](https://github.com/supernovae-st/nika/commit/057b4bdd15de30f7542f7083078525e5c7a02540))
- **nika-schema** — Cel parser learns cel-subset/0.1 — ternary, has, string tests ([5ad47b298](https://github.com/supernovae-st/nika/commit/5ad47b298644be245596048c723f15d5b335e4a6))
- **nika-schema** — Cost ceiling accounts for for_each fan-out ([c391e6183](https://github.com/supernovae-st/nika/commit/c391e61834839daa1bbc41ff18cf03b6b09e27df))
- **nika-schema** — Exec command string|array — the argv injection-safe form ([0a26e8703](https://github.com/supernovae-st/nika/commit/0a26e8703c57c374eaa4380b8ddfbffe6da75887))
- **nika-schema** — Spec catch-up — the four static-validator gaps close ([#121](https://github.com/supernovae-st/nika/issues/121)) ([0168f4abf](https://github.com/supernovae-st/nika/commit/0168f4abfd8360a1fc55c759b30dbf6257e7b087)) ([#121](https://github.com/supernovae-st/nika/pull/121))
- **nika-schema** — Deep conformance tier + DAG-004 + the registry remaps ([#122](https://github.com/supernovae-st/nika/issues/122)) ([3f3439cb2](https://github.com/supernovae-st/nika/commit/3f3439cb23b1f614ac0408a90202ad3ea80ecacd)) ([#122](https://github.com/supernovae-st/nika/pull/122))
- **nika-schema** — Ifc taint engine — provable information-flow control (ADR-092) ([1d1d231cd](https://github.com/supernovae-st/nika/commit/1d1d231cd749826daf5a5260ac42b8bb851be37e))
- **nika-schema** — Capability inference — --infer-permits (adr-092 #2) ([7b634d600](https://github.com/supernovae-st/nika/commit/7b634d60053c21839ceff33ee1e6d0c94508d3a1))
- **nika-schema** — Dataflow schema typing — typo'd fields caught statically (adr-092 #4) ([db7178c10](https://github.com/supernovae-st/nika/commit/db7178c1052ef92e432010de3af9b0cadcadcf75))
- **nika-schema** — Structural cost interval — retry and when:-aware envelope (adr-092 #5) ([e97e1e2a0](https://github.com/supernovae-st/nika/commit/e97e1e2a03441d60f33e709cc1d30d01f62dba56))
- **nika-schema** — Agent intelligence layer — deterministic suggestions + json repair surface ([1345259c6](https://github.com/supernovae-st/nika/commit/1345259c6dc937f37486e21b7564a9edbe294d16))
- **nika-schema** — Improvement hints — the deterministic ameliorateur ([3122669a4](https://github.com/supernovae-st/nika/commit/3122669a4cb7517eb3ebecd2a70dc97c6063b8b3))
- **nika-schema** — Analyzer did-you-mean + infallible maximal check report ([f82e2b2ef](https://github.com/supernovae-st/nika/commit/f82e2b2ef918dfc34e93d784dc17d597dc57e55f))
- **nika-schema** — Strictness hint — deterministic structured-output shape ([b1f395282](https://github.com/supernovae-st/nika/commit/b1f395282677e339bcbd325a377e6b68d79a6b07))
- **nika-schema** — Close the untrusted-input bound trio + bank proptests ([dcb76d1f1](https://github.com/supernovae-st/nika/commit/dcb76d1f119e68da58442a2337a29a6ba21c4327))
- **nika-schema** — Check example visual polish — DAG lanes + the colour seam ([c3a62b8ec](https://github.com/supernovae-st/nika/commit/c3a62b8eccf0c75f7853ed5bf62dfbed7ce8aba6))
- **nika-schema** — Builtin arg-shapes close four ledger rows + the lints corpus moves to the spec ([#123](https://github.com/supernovae-st/nika/issues/123)) ([af19c751f](https://github.com/supernovae-st/nika/commit/af19c751f4a0de132cc5dc16d610f5a5ca12872f)) ([#123](https://github.com/supernovae-st/nika/pull/123))
- **nika-schema** — Canonical theme — Role taxonomy + verb-gate colour logic ([3c69cfd2f](https://github.com/supernovae-st/nika/commit/3c69cfd2ff58ec4ff03bb7e4443bcca1ccb2df6e))
- **nika-schema** — Theme owns the glyph grammar too — first-class ASCII set ([2da0174b8](https://github.com/supernovae-st/nika/commit/2da0174b82a41aacb2566459fdd6443ae449c95d))
- **nika-schema** — Rustc-grade span diagnostics — source excerpts under findings ([634570aa3](https://github.com/supernovae-st/nika/commit/634570aa3df59580e0e53fed8cf6ef2781439b59))
- **nika-schema** — The verb theater — four execution models, animated ([10176204e](https://github.com/supernovae-st/nika/commit/10176204e1300d64028bda1602538da680761bfa))
- **nika-schema** — The event tape — real telemetry, one truth, two renderers ([d2fcfaf4d](https://github.com/supernovae-st/nika/commit/d2fcfaf4d4094aabef1541a0c732c8707888bda3))
- **nika-schema** — The tape speaks the full vocabulary — retry arc, stream, live meters ([a0b839829](https://github.com/supernovae-st/nika/commit/a0b839829a174df0818979b6c7b83201ba5c67c7))
- **nika-schema** — The third renderer — NDJSON wire for the event tape ([517798c31](https://github.com/supernovae-st/nika/commit/517798c31183d0abf754986b4701ea6cf5c99392))
- **nika-schema** — Ladder #6 — when:-gate reachability, arXiv-grounded, no SMT ([ddfff6198](https://github.com/supernovae-st/nika/commit/ddfff6198961b7953068930e75048a2d8f3ebe9f))
- **nika-schema** — Ladder #7 — the run certificate (AARA degree-1, no solver) ([fbdcd1684](https://github.com/supernovae-st/nika/commit/fbdcd16841fb85a567bc7b16d23999df1c7742a9))
- **nika-schema** — Parametric spend axis + ladder #9 first slice (metamorphic) ([2e6d045d1](https://github.com/supernovae-st/nika/commit/2e6d045d1f9db18bc309b977796ad4469fe20f90))
- **nika-schema** — Fetch arg-shape rules — closed mode set + pairings ([98db1a815](https://github.com/supernovae-st/nika/commit/98db1a815ed0b2fb98b35e9f742c0c27c1c58ef8))
- **nika-schema** — The certificate becomes CERTIFYING — witness + audit checker ([89ae459f4](https://github.com/supernovae-st/nika/commit/89ae459f4e6c91b49c205010a80990a9b0833378))
- **nika-schema** — The span axis + the research-conformance suite ([182f30807](https://github.com/supernovae-st/nika/commit/182f30807d5e7f9cb674d284c3e30cee134459a4))
- **nika-schema** — Fetch requires url — the check-time net widens ([5d3ae81c7](https://github.com/supernovae-st/nika/commit/5d3ae81c75e0ebecbdd2f32359dd21d18a0cec68))
- **nika-schema** — The parallelism rung — exact width, pinch, blast ([c929ee0dd](https://github.com/supernovae-st/nika/commit/c929ee0dd9cd9c63ea1f207fb6148c009e6e441f))
- **nika-schema** — Retry-effects hint — at-least-once made visible ([275a3ce1e](https://github.com/supernovae-st/nika/commit/275a3ce1e88439ecc55b9de97809a0126ed6e4b1))
- **nika-schema** — One-obvious-way/008 — steer to the injection-safe array form ([09255b4ff](https://github.com/supernovae-st/nika/commit/09255b4ffb26c3b4a400b731a12b6cc6ad938bd8))
- **nika-schema** — Arg-injection rule-pack — the array-form differentiator ([10623905b](https://github.com/supernovae-st/nika/commit/10623905b7f9393b8797bc82010c32914f6aa386))
- **nika-schema** — Sanctioned secret egress (IFC declassification) ([636de96ae](https://github.com/supernovae-st/nika/commit/636de96ae1dfbea79ae5c3a28bbd95333767027c))
- **nika-schema** — Flag unknown builtin arg keys in nika check ([e9f0a5f59](https://github.com/supernovae-st/nika/commit/e9f0a5f5971938eef90e536cf29b90a1af17fe6a))
- **nika-schema** — Cost pre-flight counts a static vars-array for_each ([102e99ed9](https://github.com/supernovae-st/nika/commit/102e99ed9f44209d9f83fbf02b9a13a981c7000e))
- **nika-schema** — Schema check catches enum/type + numeric-bound conflicts ([fda7e38f3](https://github.com/supernovae-st/nika/commit/fda7e38f354a148e8f577f17ac0021a3efd14da1))
- **nika-schema** — Static jq compile-check closes deep-gap 006 ([b2a62e7ad](https://github.com/supernovae-st/nika/commit/b2a62e7ad0a65399f1b604c190d859d79573a7a0))
- **nika-schema** — Static schema meta-check closes deep-gap 005 ([c82036db6](https://github.com/supernovae-st/nika/commit/c82036db685c8e4638c71a046d09f1046e69f441))
- **nika-schema** — One-obvious-way/009 warns on bare-iterator output bindings ([7fd27776d](https://github.com/supernovae-st/nika/commit/7fd27776de5465eef6b5e75fdb1ccef0495cd01f))
- **nika-screen** — Error one-voice — ScreenError speaks NikaErrorCode ([1cfbc8812](https://github.com/supernovae-st/nika/commit/1cfbc88123d7d2a3c7a5deb71dc9d71f3d22a418))
- **nika-types** — Gate no_std/alloc seam (Phase F1 — forward-compat WASM) ([d48db4897](https://github.com/supernovae-st/nika/commit/d48db48976833c299705e9b0e4a68865601ba1d3))
- **nika-types** — Reserve EmbeddingSpec (wave 4a r1, adr-029 seed) ([001ae0b6f](https://github.com/supernovae-st/nika/commit/001ae0b6f2680e4086621036203871087445f43d))
- **nika-types** — Reserve trust on MemoryFrameRef + tenant on RecallQuery (wave 4a r2+r3) ([41e8a1467](https://github.com/supernovae-st/nika/commit/41e8a1467212aa4cd6bcb6eb08a25d91bf42cbd9))
- **nika-types** — Add Timestamp + WallDuration value types (q9, wave 4b #3 seed) ([c5d292b6e](https://github.com/supernovae-st/nika/commit/c5d292b6eb8d28c60452a81d856b6a7bd379ada8))
- **nika-types** — Docid + score + rankeddoc newtypes for 9-satellite cascade ([3ae189ae7](https://github.com/supernovae-st/nika/commit/3ae189ae79c8b989713111c3e59e0eacf13f0f0c))
- **nika-types** — Extract-mode vocabulary — closed stdlib v0.1 set ([0ea48316c](https://github.com/supernovae-st/nika/commit/0ea48316c442a62b466aaa40bb5affe7670431bd))
- **nika-types** — Delay_for_ms — the shared backoff-with-jitter semantics ([2c5ee514c](https://github.com/supernovae-st/nika/commit/2c5ee514c472ca6621f17a6c69eb92264c9ed130))
- **nika-types** — Retry budget + retry-after honoring — the anti-storm kit ([f373fd645](https://github.com/supernovae-st/nika/commit/f373fd645b4c427ea52df6795e804525d8f9db44))
- **nika-verb-agent** — The intelligence layer — routing, stall guard, compose, telemetry ([f252b1500](https://github.com/supernovae-st/nika/commit/f252b1500129dd2ba086a4d31a4050709e5ed438))
- **nika-verb-agent** — Run_observed — the run-scoped observer seam ([56885ef3d](https://github.com/supernovae-st/nika/commit/56885ef3d0eed07b8071fee2f61a4e2707ebfee8))
- **nika-verb-agent** — Parallel intra-turn dispatch — concurrent resolve, request-order fold ([397d79e84](https://github.com/supernovae-st/nika/commit/397d79e84b3734663a3c8f3648020614f69c0244))
- **nika-verb-exec** — Scaffold the s10 L2 verb crate — WIP pre-admission ([733c113d8](https://github.com/supernovae-st/nika/commit/733c113d8f8a7f76a50ba5b3ab7cbbd91da34cb5))
- **nika-verb-exec** — Real argv execution, no shell (injection fix) ([75d8e59cb](https://github.com/supernovae-st/nika/commit/75d8e59cb13397aab88b3086568ae83a60149343))
- **nika-verb-infer** — Scaffold the s9 L2 verb crate — WIP pre-admission ([cf4783180](https://github.com/supernovae-st/nika/commit/cf478318096b9dc19e0ca9f739f25eaa27ce56f2))
- **nika-verb-invoke** — Scaffold the s11 L2 verb crate — WIP pre-admission ([da1d55d96](https://github.com/supernovae-st/nika/commit/da1d55d960cb454dd2c5db1477350c0a1925813f))
- **schema** — Static missing-required-arg check for all 22 builtins ([8456f1f6e](https://github.com/supernovae-st/nika/commit/8456f1f6e3909c6d83987837868c5ec36c39a61c))
- **screen** — M2.1.b2 crate skeleton + nika-1000..1009 codes ([546fb201c](https://github.com/supernovae-st/nika/commit/546fb201cb1b7b4fbf97998fa22edda2cbf0e6dc))
- **screen** — M2.1.b3 single-shot capture via xcap · close skeleton ([cf9d4cd80](https://github.com/supernovae-st/nika/commit/cf9d4cd80c7461b19352da9e40b77eb08bb65849))
- **screen** — M2.1.b4 capture_stream via mpsc · skeleton fully closed ([08a5c180a](https://github.com/supernovae-st/nika/commit/08a5c180a84c923d320139a895e840b6459a49c2))
- **screen** — M2.1.b5 adr-081 guards 6+7 real + enforced ([0daec9bf7](https://github.com/supernovae-st/nika/commit/0daec9bf7c0998b8cd28dc5eed97ced63c1fd803))
- **screen** — M2.1.b6 12-gate close · gap-3 shim carry-forward ([e975320cd](https://github.com/supernovae-st/nika/commit/e975320cd772026961d194cbd9c1e6fd245eafa6))
- **workspace** — Scaffold nika-infer-local — sovereign inference seam (ADR-091) ([00ed3ee96](https://github.com/supernovae-st/nika/commit/00ed3ee968a8221782240d593247308072cac3e0))
- **workspace** — Nika-infer-local generation-control core — template, sampling, stop ([9ef8f4a4a](https://github.com/supernovae-st/nika/commit/9ef8f4a4a6293c56a4146226cd2109dfaf7f65f4))

### 🐛 Bug Fixes
- **release** — Bump post-0.90 development to `0.91.0-dev` and fail releases whose tag does not match the Cargo workspace version, preventing Homebrew/local binaries from sharing a version while exposing different CLI flags.
- **adr** — Address review P0 + P1 findings from 3-agent swarm ([9a395bb6f](https://github.com/supernovae-st/nika/commit/9a395bb6ff782e4efbb35d3920418b540a6a93bb))
- **adr** — Resolve 13 relationship asymmetries + harden scripts ([baa680cac](https://github.com/supernovae-st/nika/commit/baa680cace9e23c91ecfe590eddc6ee387c7838e))
- **adr** — Refresh evidence paths for kernel subdir reorg ([fcbb00841](https://github.com/supernovae-st/nika/commit/fcbb008418540c50fc393051b27090af773f3b8f))
- **adr** — Adr-034 requires add adr-016 · close adr-016 enables backref ([9745be740](https://github.com/supernovae-st/nika/commit/9745be740d93e439aa54c38d0da251c73056cdd2))
- **adr** — Adr-007 + adr-014 related add adr-016 · close cascade backrefs ([ee841d42f](https://github.com/supernovae-st/nika/commit/ee841d42f211c655f7d3bd3ae58add2bc225893e))
- **adr** — Batch close 54 bidirectional related backrefs · diamond w2 cascade ([4e8ca67a7](https://github.com/supernovae-st/nika/commit/4e8ca67a7f98f1cb83925200b432fc66773492e6))
- **catalog** — Correct COMMUNITY_EXTENSIONS.md doc link post tools→crates ([9ecca6af3](https://github.com/supernovae-st/nika/commit/9ecca6af385fa93b9fc044aa1adb9933af3ed2dd))
- **catalog** — Total_cmp + name tie-break for deterministic suggestions ([ef7d873ad](https://github.com/supernovae-st/nika/commit/ef7d873ad960f49bc84bad17452a17b461174643))
- **ci** — Make crate-size glob recursive (P1-6) ([898e2de1e](https://github.com/supernovae-st/nika/commit/898e2de1ee86adbc72bf2e7b16a480f4b761018e))
- **ci** — Add scoped-fail for new crate ADR coverage (P1-5) ([593394b8b](https://github.com/supernovae-st/nika/commit/593394b8b49ea3993e4daad3a4c7cc9016703c83))
- **ci** — Add allowlists for pre-existing ratchet violations (P1-2 follow-up) ([b22e49cb9](https://github.com/supernovae-st/nika/commit/b22e49cb969931556788d2bbb4dd53b69574ce52))
- **ci** — Update allowlists + deny.toml for crates/ rename ([f780c37e5](https://github.com/supernovae-st/nika/commit/f780c37e566aee3ac9f0e4a2061702eb62e5f3a4))
- **ci** — Mutation-floor v2 — budget-as-budget + cross-platform honesty ([74f1c1409](https://github.com/supernovae-st/nika/commit/74f1c1409560d8b520d5156c52f1b13fefa52b22))
- **ci** — Vector 40 audits ai/ traits — vision + audio Pattern-A gap surfaced ([5da3a02c6](https://github.com/supernovae-st/nika/commit/5da3a02c65fb0160448a4fb1480defe218a63150))
- **ci** — Allowlist first_balanced_span — fn-length heuristic false positive ([495119ade](https://github.com/supernovae-st/nika/commit/495119adec921bae9e93a0ce50e50895f164d1a8))
- **ci** — Unblock the push train — char-literal-aware fn counter + mit-0 ([ed10784c7](https://github.com/supernovae-st/nika/commit/ed10784c7504ea91dbbea7c68909475d839becc4))
- **ci** — Adr validator learns the l1.5 service layer ([4737bf184](https://github.com/supernovae-st/nika/commit/4737bf18411ac21b53e3ac0662e08f9f0674222b))
- **ci** — Crate-size counts prod LOC — the mutation ratchet must not fight the size budget ([e023a73e8](https://github.com/supernovae-st/nika/commit/e023a73e860304f2f4b9aa5d6694cc8e29ac292f))
- **ci** — Mutation-sandbox skip at the SOURCE — the global --lib had blast radius ([9c2683625](https://github.com/supernovae-st/nika/commit/9c2683625bc13d25efb4bd3cd8a9ad59128c8765))
- **ci** — Adr-validate catches duplicate IDs — the gate was blind to collisions ([9a3c9d2b4](https://github.com/supernovae-st/nika/commit/9a3c9d2b408667f868ef5e5e17b651d18a813757))
- **ci** — Strip string literals + line comments in brace-counting gates ([d970e3f4e](https://github.com/supernovae-st/nika/commit/d970e3f4e8f835c313a9233c558f5affedf0c2c2))
- **ci** — Refresh-status survives an empty wip array ([d0fb8d17b](https://github.com/supernovae-st/nika/commit/d0fb8d17b83240166b2d93250db4261aaab78f1a))
- **cli** — Surface nika:log + nika:emit on stderr, not /dev/null ([bc5179392](https://github.com/supernovae-st/nika/commit/bc5179392f6239de5bd64465216068c202ca89cb))
- **docs** — Close 3 review-swarm P1s — MetricsExporter, sealing, L4 label ([54a402efe](https://github.com/supernovae-st/nika/commit/54a402efe2dcd9c940c94d0eff03af97a4db9540))
- **dx** — Address phase c-g review-swarm p1 findings ([d1469b5cf](https://github.com/supernovae-st/nika/commit/d1469b5cf5b101a83e362538083e715844736a03))
- **dx** — Gitnexus auto-reindex hook — match compound git commands ([56e793d0b](https://github.com/supernovae-st/nika/commit/56e793d0bc2ca6f907e5b836ab974cf49ee51ed5))
- **dx** — Apply compound-command regex to all 4 git posttooluse hooks ([4d6430e34](https://github.com/supernovae-st/nika/commit/4d6430e34b5e39f408023267104c86fae1eb493e))
- **dx** — Privacy-strict refactor + pretooluse compound regex + reindex lockfile ([66a8846bb](https://github.com/supernovae-st/nika/commit/66a8846bbf47ba176a5dd56db0f32d060f389dd5))
- **dx** — Address executive-swarm findings — privacy + injection + stale counts ([983463154](https://github.com/supernovae-st/nika/commit/98346315468abe2923faa2b6da03fb2c56540fcb))
- **dx** — Remove trailing commas from settings.json ([d24de258f](https://github.com/supernovae-st/nika/commit/d24de258f912c5975da636be00004ca49024a2f0))
- **dx** — Roadmap.sh WIP seam + L1.5 row + derived M2 frontier ([dd8a88b5f](https://github.com/supernovae-st/nika/commit/dd8a88b5fae055fe4e158268761d1d59addb23ef))
- **error** — Register kernel code ranges + private MemoryId + mock imports ([544da1ad3](https://github.com/supernovae-st/nika/commit/544da1ad3c6b7d5dd16d963dbbf8d7e93f045a94))
- **error** — Code_help covers Schema + Provider; fix stale ranges ([00352928c](https://github.com/supernovae-st/nika/commit/00352928c97cba4c92c389afc641f38c500802ad))
- **hooks** — Anchor co-author trailer check to line boundaries (P0-2) ([3f4cb1650](https://github.com/supernovae-st/nika/commit/3f4cb16501454f1757a14159fbf53b6fb54c47ea))
- **hooks** — Remove escaped backslash in privacy pattern (P0-3) ([f8e89bb1d](https://github.com/supernovae-st/nika/commit/f8e89bb1d227a08cc7b2a0f437d9e71d832f9c1b))
- **hooks** — Simplify squash co-author detection (P0-5) ([451919c68](https://github.com/supernovae-st/nika/commit/451919c685e854b4904f8a32e634442fcea3a142))
- **hooks** — Use --prefix=none + sed for reverse-dep resolution (P1-1) ([b71bf9f7c](https://github.com/supernovae-st/nika/commit/b71bf9f7c39b46ea8845e0d88b9114d4e01a1408))
- **hooks** — Use git toplevel for activity-log path (P1-9) ([7514b7942](https://github.com/supernovae-st/nika/commit/7514b79427ca4b76957f8249ab8c376c96e7bce4))
- **hooks** — Capture ratchet exit code before errexit kills subshell (P1-2) ([1991acb0e](https://github.com/supernovae-st/nika/commit/1991acb0eb210256cded149a9c7db816aa81d7c6))
- **hooks** — Portable stdin detection in force-push-guard (P1-3) ([a8092f2a5](https://github.com/supernovae-st/nika/commit/a8092f2a5365a853bd272d73bf558a62132f21f2))
- **hooks** — Wire post-rewrite hook for ADR seal check (P1-4) ([62dc35cb3](https://github.com/supernovae-st/nika/commit/62dc35cb39c13619d6c947c5b1b87dcd37c135a5))
- **hygiene** — V2 — 5 new vectors, bug fixes, catalog-verify alpha.4 ([495efc5f5](https://github.com/supernovae-st/nika/commit/495efc5f5c1eabe1d1043e1006a52444a97d690c))
- **hygiene** — Rename lefthook-engine.yml → lefthook.yml ([7a148c13c](https://github.com/supernovae-st/nika/commit/7a148c13c7d4c7faae86df64026dfb3ab252c794))
- **hygiene** — Forward pre-push stdin to force-push-guard ([ed2345055](https://github.com/supernovae-st/nika/commit/ed2345055e5f225f259e90e9282b86f8e492b3cc))
- **hygiene** — Force-push-guard works without stdin forwarding ([d6ef7989e](https://github.com/supernovae-st/nika/commit/d6ef7989ed744db28eeac8910c5c4801ffd26b0f))
- **hygiene** — Gate engine-hygiene on RED only, not YELLOW ([8167ffc9e](https://github.com/supernovae-st/nika/commit/8167ffc9e5ef2ab38546762d84e0a7affc2243d7))
- **hygiene** — Drop unused COMMIT_TYPE + FOUND_BLANK from validator ([06bda01a1](https://github.com/supernovae-st/nika/commit/06bda01a1d874f8aee3684f0e2222a49733e9931))
- **hygiene** — Expand block-private-paths self-exclusion (P1-7) ([501fc13d1](https://github.com/supernovae-st/nika/commit/501fc13d1c4eb9ce228627247f9aa3c4367c8074))
- **hygiene** — Tighten Claude trailer detection (vector 13, P0-1) ([41b6451a0](https://github.com/supernovae-st/nika/commit/41b6451a030e2fd08f5f91f628c6ab9471d7fc8b))
- **kernel** — Add displayid::new constructor · inv-19 gap ([0dfb13683](https://github.com/supernovae-st/nika/commit/0dfb13683bc755944c8a01965304b8b6b76f4f91))
- **kernel-browser** — Doc render · arc generic prose · adr-081 enables schema clean ([1245e976f](https://github.com/supernovae-st/nika/commit/1245e976f9bc4e1ffa910675f154615b70059e80))
- **kernel-core** — Redact credential headers in HttpStreamResponse Debug ([b67be8da3](https://github.com/supernovae-st/nika/commit/b67be8da30772b99cdd78d23bc38c82c3d116327))
- **kernel-mock** — Real glob matching in MockFs, not substring ([ec30145da](https://github.com/supernovae-st/nika/commit/ec30145daaa70584bd0552996ed243350058dbf4))
- **kernel-mock** — Fs/http/clock mocks implement the Dyn seams ([ea2e30902](https://github.com/supernovae-st/nika/commit/ea2e30902f3a654c3a86ebf2832579088bbf4e8e))
- **m2** — Suppress panic-payload leak in all JoinError mappings (Guard-1 class) ([f1aae8164](https://github.com/supernovae-st/nika/commit/f1aae8164df99f939eca628829e478f86347b3da))
- **mintlify** — Revert docs.json rename + Node 22 setup ([6f772968f](https://github.com/supernovae-st/nika/commit/6f772968fbeb2b086ed221d9c0d1ec4e3cb28440))
- **mintlify** — Escape <500 MDX parse error in crates.mdx ([f8f604d25](https://github.com/supernovae-st/nika/commit/f8f604d25a59c6b76af529e3dd3faccbc36d2511))
- **nika** — Polish stale docs + error messages + honest Gate-5 note ([a46a30d84](https://github.com/supernovae-st/nika/commit/a46a30d840d0bc4c455abb1a5ea9bce50412e09e))
- **nika-a11y** — Guard 3 fail-closed on secure-marker read error ([a84d96438](https://github.com/supernovae-st/nika/commit/a84d96438e97d7d6513e47032606d6debfd27d67))
- **nika-a11y** — Guard 3 scrubs the secure field's ENTIRE subtree ([8273bae27](https://github.com/supernovae-st/nika/commit/8273bae2730e5539b5d570ea2b90016199f006e2))
- **nika-blob** — Clean the temp file on write failure, not just rename ([a812217e6](https://github.com/supernovae-st/nika/commit/a812217e625fd4ac4c88022c767c28eae8ee7c81))
- **nika-bm25** — Test-scope expect allow + the stale-rlib lesson banked ([c8bbd5f03](https://github.com/supernovae-st/nika/commit/c8bbd5f03bb7eb1829122677e467f7d3c0b746d6))
- **nika-bm25** — Export PruneStats — top_k_pruned_stats return type was unnameable ([6bf6a2508](https://github.com/supernovae-st/nika/commit/6bf6a2508fde688283a43fa7a781c23b8dc47fc0))
- **nika-browser** — Harden Guard 5 — node-identity pin + no failure-downgrade ([fba1360c1](https://github.com/supernovae-st/nika/commit/fba1360c13b587cbcd4bbca596dd9ea993f1b7d6))
- **nika-browser** — Occlusion hardening — scroll-stable point + full-depth subtree ([5ec5415f9](https://github.com/supernovae-st/nika/commit/5ec5415f94f2e63b3427f5355f3476d638479f0e))
- **nika-builtin** — Prompt rejects a wrong-typed default value ([892802fcb](https://github.com/supernovae-st/nika/commit/892802fcb739be025304e9af1172294953d2e1db))
- **nika-builtin** — Nika:glob exclude string form + nika:date weeks unit ([763b15aec](https://github.com/supernovae-st/nika/commit/763b15aec7afb815b3ad623f3048e110729c013a))
- **nika-builtin** — Nika:date add/subtract calendar units via Zoned ([d787d5879](https://github.com/supernovae-st/nika/commit/d787d5879de41e34ede173c904c7ef8c7ef0d5d9))
- **nika-builtin** — Canonicalize fs paths before permits.fs match ([457f27213](https://github.com/supernovae-st/nika/commit/457f2721355e67e989ab1ae1106711970da1c745))
- **nika-builtin** — Nika:convert has_header is a strict bool ([3bab272fc](https://github.com/supernovae-st/nika/commit/3bab272fc17e1f9eeacf7d9b084caeac62b18276))
- **nika-builtin** — Glob absolute patterns + grep-on-file error ([01e49cc39](https://github.com/supernovae-st/nika/commit/01e49cc39bdaa78a812da111d8f086ca85554e9f))
- **nika-builtin** — Nika:prompt confirm-default error names mode: input ([93072824d](https://github.com/supernovae-st/nika/commit/93072824d729c1dede579d6f67b37420d3cca683))
- **nika-builtin** — Nika:inspect reports unavailable, not fake-empty ([ab44c3ee9](https://github.com/supernovae-st/nika/commit/ab44c3ee96e46ff22ee9da3c38e7bcebc215ccac))
- **nika-builtin** — Classify transient nika:fetch failures as retryable ([98507a7f2](https://github.com/supernovae-st/nika/commit/98507a7f262baefae79b9a2ddf29160f777d25d2))
- **nika-builtin** — Exact-file permits.fs path admits its own new file ([e4863ddcb](https://github.com/supernovae-st/nika/commit/e4863ddcb04aa950986446740ec77d2b6a2deba6))
- **nika-builtin** — Nika:glob strips a leading ./ from a relative pattern ([a59429475](https://github.com/supernovae-st/nika/commit/a59429475b87b98440c1c725c7ec88a8ef6a0616))
- **nika-builtin** — Map an SSRF block to NIKA-SEC-005 (F-01) ([64e5f7215](https://github.com/supernovae-st/nika/commit/64e5f7215b3f38466d80576b34d162f7979cee5f))
- **nika-builtin** — Grep re-enforces the fs boundary per matched file ([ec84d9e39](https://github.com/supernovae-st/nika/commit/ec84d9e3908cfd096d9e22abaa8ee4c10433410b))
- **nika-builtin** — Doc-integrity + model-facing description cleanup ([91c3acdfa](https://github.com/supernovae-st/nika/commit/91c3acdfa65b67e6fac2ffeef804390605e2bdcf))
- **nika-catalog** — Address 3-agent review findings (P0 + P1) ([ffe8af986](https://github.com/supernovae-st/nika/commit/ffe8af986e825d50d4d16553c676e3bd7b3f749d))
- **nika-catalog** — Address 2b review swarm — 2 P0 + 2 P1 same session ([ce0eab1bd](https://github.com/supernovae-st/nika/commit/ce0eab1bd7f74e318e3c7c20c1e049e884d21f09))
- **nika-catalog** — Validate_caps_patch — require every field on [defaults] ([731f11bfe](https://github.com/supernovae-st/nika/commit/731f11bfec9092f3711b9b5e85e1ff9f7df81f5e))
- **nika-catalog** — Canonicalise scope.providers at parse — check_any_last_in_scope ([2d1f53c15](https://github.com/supernovae-st/nika/commit/2d1f53c15277b1cd6fc7558b5c173a4df9997574))
- **nika-catalog** — Address session 3 review swarm p1/p2 findings ([fe311de4b](https://github.com/supernovae-st/nika/commit/fe311de4b59f468d991773b8989bd7baa2db9ba6))
- **nika-catalog** — Renumber NIKA-230..235 → NIKA-010..015 (code collision) ([1d9f85c13](https://github.com/supernovae-st/nika/commit/1d9f85c138f061112a6431b8ee08339ec9305890))
- **nika-catalog** — Remove broken doc link to OverlayCatalogDataSource ([e44bb0789](https://github.com/supernovae-st/nika/commit/e44bb07895b6da7adc9a8cc6572448fd12f5cd56))
- **nika-catalog** — Review swarm P0 fixes (inv #19 + region guard) ([cbc5209bb](https://github.com/supernovae-st/nika/commit/cbc5209bb04b7cfeae45a8a4a4430487efc96d10))
- **nika-catalog** — Add #[non_exhaustive] to ParseRegionError ([aedfdf4c4](https://github.com/supernovae-st/nika/commit/aedfdf4c4a9d3ec40480b8e443912c977eac81d9))
- **nika-catalog** — Wire tokenizer variants into TOML rules (review P1) ([820bd1949](https://github.com/supernovae-st/nika/commit/820bd1949b3759caf7a41047a7c43e0bc8e13e37))
- **nika-cel** — Close 2 Gate-11 review P1s + 2 forward-compat gaps ([e70950c04](https://github.com/supernovae-st/nika/commit/e70950c04e7bce21540d793eb9a7c562982ccffa))
- **nika-cel** — Bound expression depth, close stack-overflow DoS ([e4e474762](https://github.com/supernovae-st/nika/commit/e4e474762fef429d449620043a03f67488d2554c))
- **nika-cel** — Numbers compare by value on the continuous number line ([2396e6d4c](https://github.com/supernovae-st/nika/commit/2396e6d4c823a13cc56c6e479bb5fb99325e54dc))
- **nika-cli** — Fold the S6 review — §6 permits field + ANSI-safe columns + stable explain contract ([7354d40c8](https://github.com/supernovae-st/nika/commit/7354d40c85d1e1e5879a4b17abde97a4dfd7b273))
- **nika-cli** — Exempt the provider HTTP path from the SSRF guard ([ffd4cf600](https://github.com/supernovae-st/nika/commit/ffd4cf6004ddecabe971ababb926cd96f0cf0616))
- **nika-cli** — Report the resolved provider's response_format (BUG#11) ([c1355f515](https://github.com/supernovae-st/nika/commit/c1355f51548fc9bace8e4453393c2b72e1d7ea95))
- **nika-cli** — Examples-run help no longer claims the run verb is unshipped ([3cdf1b37c](https://github.com/supernovae-st/nika/commit/3cdf1b37c3e125e32a5cb6c40acf9ad581defb48))
- **nika-cli** — Parse-stage errors now carry their spec code ([b355fb918](https://github.com/supernovae-st/nika/commit/b355fb91818a37ad0b834018cb0fe27adb5c3f80))
- **nika-cli** — Escape model/tool in graph mermaid+dot labels ([a9fef08c9](https://github.com/supernovae-st/nika/commit/a9fef08c90a94c527177a3adf23f3ff3850ef9ff))
- **nika-cli** — `new --from '?'` lists templates without a dest ([7de9107ec](https://github.com/supernovae-st/nika/commit/7de9107eca08f3da4f35b90d685e78538f56220d))
- **nika-cli** — Route_intent ignores boilerplate/stopword queries ([c6aa0020e](https://github.com/supernovae-st/nika/commit/c6aa0020eadc32d2b8ac6cc8c7a2efa5ac4436c9))
- **nika-cli** — Trace recovers a crashed run's truncated tail ([b5d27519c](https://github.com/supernovae-st/nika/commit/b5d27519cecab0b6ba2d7cc7febbee7cbc643b73))
- **nika-cli** — Human run flags conflict with the machine modes ([343423a6b](https://github.com/supernovae-st/nika/commit/343423a6bc9e2750077f4fbc159fe5712f34b1aa))
- **nika-cli** — The public binary name is `nika`, not `nika-cli` ([4857d4819](https://github.com/supernovae-st/nika/commit/4857d48197d5ac2c2f9a7f28b2cc43b9ef9ceb63))
- **nika-engine** — Rules index + gitignore generated skills dir ([068e1e31b](https://github.com/supernovae-st/nika/commit/068e1e31bc2fa09aaea09088c75bf3a8645e1221))
- **nika-engine** — Reconcile memory satellite count 3 → 9 crates ([68240f922](https://github.com/supernovae-st/nika/commit/68240f922222d589cf08ece98084f97d82a0a2c3))
- **nika-error** — Remove colliding NIKA-XXX placeholders (Wave 1.3) ([2fe8401d1](https://github.com/supernovae-st/nika/commit/2fe8401d1803ab43c246b5c56adac6e1333eaff2))
- **nika-exec-runner** — Cap captured output at 64 MiB — NIKA-054 ([dd788d7d9](https://github.com/supernovae-st/nika/commit/dd788d7d9908e1e63fddf363374e027ef5ddac24))
- **nika-exec-runner** — Close the argv re-exec bypass + shell expansion TOCTOU ([31614bbd9](https://github.com/supernovae-st/nika/commit/31614bbd959b89788007262369132b0cf2afb45f))
- **nika-extract** — Depth guard — close 2 under-count bypass P0s ([ef6f13477](https://github.com/supernovae-st/nika/commit/ef6f13477c0d3598a045c31080f57e264a027f4d))
- **nika-extract** — Unblock the push gates — fn split + brace-balanced fixture ([0edf354fe](https://github.com/supernovae-st/nika/commit/0edf354fe60a6e7400d9666d0ca9326e020feebd))
- **nika-extract** — Article fallback compares trimmed length ([a5f6a7eb3](https://github.com/supernovae-st/nika/commit/a5f6a7eb30fc669335f949f30f9724e13f4ca03c))
- **nika-extract** — Review-swarm fixes + microdata property cap ([a643466c4](https://github.com/supernovae-st/nika/commit/a643466c46bdd4d3d195b6d1236a9103afe54221))
- **nika-fs** — Clean the temp file on write failure, not just rename ([9c6dfaca1](https://github.com/supernovae-st/nika/commit/9c6dfaca1a00d2344ca7d141da717e264f94e31a))
- **nika-http** — Widen the SSRF oracle to every non-public-unicast range ([52c182691](https://github.com/supernovae-st/nika/commit/52c182691bc0c89b0dc4ed755b378b57e6c97f42))
- **nika-http** — Drop the dead metadata-IP hostname entry — the IP branch is authoritative ([0bc0d71c4](https://github.com/supernovae-st/nika/commit/0bc0d71c4d0e5365f80ecc63ea71755baf7f4307))
- **nika-http** — Comma-join repeated response headers — rfc 9110 lowering ([d09a8ba47](https://github.com/supernovae-st/nika/commit/d09a8ba47fde9682775856c3f8d32169f3a8697d))
- **nika-http** — Enforce permits.net.http at runtime (NIKA-SEC-004) ([8c2d1d87a](https://github.com/supernovae-st/nika/commit/8c2d1d87aa900fa86c87cba630685572b1f9992f))
- **nika-http** — Permits.net.http gates before DNS resolution ([8e9df5568](https://github.com/supernovae-st/nika/commit/8e9df5568453340d104642559f15f4b6ed626c36))
- **nika-infer-local** — Fold the 3-lens review — 2 P1 + 4 P2/P3, e2e wire contract ([6f8ec45fd](https://github.com/supernovae-st/nika/commit/6f8ec45fde6bfd9e15fb449fa2cd0d1022fe91be))
- **nika-infer-local** — Execute the mutation audit — kill survivors, wire min-p, O(window) stop ([803469a3c](https://github.com/supernovae-st/nika/commit/803469a3c17c9b3ca1533163b7d41e00014f1211))
- **nika-infer-local** — Enforce the context window — ContextOverflow fires ([67d56b0d8](https://github.com/supernovae-st/nika/commit/67d56b0d8bf15cf361582c8c0c701bc75a0b04c6))
- **nika-infer-local** — Fold candle-backend review + gate-7 bench + GGUF arch de-hardcode ([d982f5e12](https://github.com/supernovae-st/nika/commit/d982f5e12050ab30355ce271689dd0f114b47699))
- **nika-kernel** — Seal SecretResolver + Acquire/Release CancelCtx + reserve NIKA-700..819 ([940908c7a](https://github.com/supernovae-st/nika/commit/940908c7ad28194b9415a129dbcb86910d49f551))
- **nika-kernel** — Close 5 bug-hunt findings before review swarm ([5a5b1e6fa](https://github.com/supernovae-st/nika/commit/5a5b1e6fa1df4a7ea74614b7206212d168c7a5a4))
- **nika-kernel** — Close review-swarm P1s — registry, proptest, cost bridge, oracle doc ([244dcc807](https://github.com/supernovae-st/nika/commit/244dcc807796f8c3bea8e3bd9c5e9852f2324f67))
- **nika-kernel** — Intra-doc link TenantId::DEFAULT → default_tenant (gate 8) ([fdc10d916](https://github.com/supernovae-st/nika/commit/fdc10d916361aedc2dad1e6e3eac0abf6340f380))
- **nika-kernel-mock** — Tool-executor doubles implement the Dyn seam ([b063577f4](https://github.com/supernovae-st/nika/commit/b063577f44f4af39cc05ac184570343adfc5ae31))
- **nika-lsp** — Complete admission hygiene — layer registry · status block · error-voice ([2f17dfeac](https://github.com/supernovae-st/nika/commit/2f17dfeacfaac93cc55eee982558f0e0e94998f9))
- **nika-mcp** — Fold checkpoint review — version negotiation · batch · full report ([c7dc18e25](https://github.com/supernovae-st/nika/commit/c7dc18e2530e984c806f7a62f01074b1193a83d5))
- **nika-ocr** — Saturating bbox span avoids i32 overflow panic ([3970a83e7](https://github.com/supernovae-st/nika/commit/3970a83e7f96b350aad8d68b9f6cf20154922724))
- **nika-pack** — Tojson structured data before nika:write in 3 showcases ([72f253e7e](https://github.com/supernovae-st/nika/commit/72f253e7ee2b5dd425473602d77dafa0cbd1bfb9))
- **nika-pack** — Re-sync embedded canon to SSOT (builtins 22→23 · compose) + derive-test ([1244973d3](https://github.com/supernovae-st/nika/commit/1244973d385a1e37a603ee6e069cd2737b8db14d))
- **nika-pack** — Re-vendor embedded pack from spec SSOT ([de5029602](https://github.com/supernovae-st/nika/commit/de5029602517cb520986caa72916d4ef37948e57))
- **nika-providers** — Gemini in-band errors speak the shared status table ([d6727ad7d](https://github.com/supernovae-st/nika/commit/d6727ad7df71180331df6892978bfc4a431baf00))
- **nika-providers** — Normalize openai strict structured-output schema ([6afce214a](https://github.com/supernovae-st/nika/commit/6afce214abe37fa2b0be2e9f6089e23f6a7f9bba))
- **nika-providers** — Sanitize tool names for openai + anthropic ([91c360a01](https://github.com/supernovae-st/nika/commit/91c360a016f8752b26f7144e189bcbd189748958))
- **nika-providers** — Adapt gemini structured-output schema ([a679beddb](https://github.com/supernovae-st/nika/commit/a679beddb48b64fe73cdd297719ad45f412e2fe3))
- **nika-providers** — Gemini output_tokens fold thoughts — budget guard was blind ([a5e42ebdd](https://github.com/supernovae-st/nika/commit/a5e42ebddf0702cb70a7a0a93676a0c9f368aef0))
- **nika-providers** — Rewrite oneOf to anyOf for openai strict ([ca08eca45](https://github.com/supernovae-st/nika/commit/ca08eca45c0d1224ed6bdd56ee2eb17edcc80574))
- **nika-providers** — Rewrite const + strip uniqueItems for openai ([08c8977d5](https://github.com/supernovae-st/nika/commit/08c8977d5d151deed65cb2f267d9d69a300983cb))
- **nika-providers** — Inline $ref/$defs for gemini, error on cycle ([4d2d59904](https://github.com/supernovae-st/nika/commit/4d2d59904f80e9c600c7e39b4f799a803a557a59))
- **nika-providers** — Map multi-type unions to anyOf for gemini ([232f06655](https://github.com/supernovae-st/nika/commit/232f066555731691a3a5c514c6e460dc72af6bce))
- **nika-providers** — Preserve integer enum value types for gemini ([ea1715512](https://github.com/supernovae-st/nika/commit/ea17155124105789d88f9e5daf9c1e673a1d7703))
- **nika-providers** — Rewrite const + strip uniqueItems for gemini ([718c82125](https://github.com/supernovae-st/nika/commit/718c821259720e8585d5511e825995899e91145b))
- **nika-providers** — Stringify gemini enum members (revert ea1715512) ([c274216d9](https://github.com/supernovae-st/nika/commit/c274216d9f98f4f0184f77ff9b08a6e7d1f77a98))
- **nika-runtime** — Land the v2 module files — repair the lagging-file commit ([0c38a5698](https://github.com/supernovae-st/nika/commit/0c38a56982ceb540a788302e91a882df3a2dd8a8))
- **nika-runtime** — Agent telemetry review fold — evidence survives the timeout ([7818280df](https://github.com/supernovae-st/nika/commit/7818280dfde758cc8418e10276d9196d111a96b3))
- **nika-runtime** — Review fold — registry consts, spec-code pins, the gate-scope hazard ([b9fbff9f3](https://github.com/supernovae-st/nika/commit/b9fbff9f34345d2e726cc289e958e76118804caf))
- **nika-runtime** — Deny shell under a program allowlist; fan-out on_finally permits ([4277d2bb8](https://github.com/supernovae-st/nika/commit/4277d2bb86d4aba2268f926c1536295172b071f2))
- **nika-runtime** — Wire exec cwd/env/stdin through to the spawn ([c4a4136b5](https://github.com/supernovae-st/nika/commit/c4a4136b5715db5b2d9e463b8bd3067e47c63c88))
- **nika-runtime** — Match on_codes against the user-facing spec code ([76139737d](https://github.com/supernovae-st/nika/commit/76139737d6a3ae170c9bdd477e0aabe1bd4d1524))
- **nika-runtime** — Enforce NIKA-VAR-009 — typed outputs validated at run end ([46a45a0db](https://github.com/supernovae-st/nika/commit/46a45a0db4e66f708c70ab63b50373a63dafcae2))
- **nika-runtime** — Run banner states the declared permits boundary ([530dde4cf](https://github.com/supernovae-st/nika/commit/530dde4cf05314561656854d601553e205aa4e64))
- **nika-runtime** — Cel eval errors carry spec-plane wire codes ([c5c8f19bd](https://github.com/supernovae-st/nika/commit/c5c8f19bdf67ec9d9553b6af4e5cf13a401ba317))
- **nika-runtime** — Fold adversarial review findings on the error surface ([789391b2e](https://github.com/supernovae-st/nika/commit/789391b2e42c94484a6663bfa72b87103a206a4b))
- **nika-runtime** — Render honors the backslash-escape (spec 04) ([1b0c9a82c](https://github.com/supernovae-st/nika/commit/1b0c9a82cad5fdab893fef364dc943ff21bd55de))
- **nika-runtime** — Render close-find is quote-aware (spec 04) ([f296fc10b](https://github.com/supernovae-st/nika/commit/f296fc10b871277ab05f6796c4d6057a65d61aab))
- **nika-sandbox-seatbelt** — Refuse over-granting permits (audit P1) ([f5ee11962](https://github.com/supernovae-st/nika/commit/f5ee11962a925a7f3a9b32b64f6672ae7c9924e5))
- **nika-schema** — Rename lints module to preference_rules · doc collision ([b8d3609f4](https://github.com/supernovae-st/nika/commit/b8d3609f4ba2e962cf8adabac840d339abfa29d5))
- **nika-schema** — Fold review findings — net/fs literal escapes, secret + cost fixes ([9a0c20510](https://github.com/supernovae-st/nika/commit/9a0c20510707816a115c828510f4c26a6d7626af))
- **nika-schema** — Repair-fix idiom unified + convergence test + honest gaps (review fold) ([73b24bb0b](https://github.com/supernovae-st/nika/commit/73b24bb0b7654ed8723949afa064440d33dfaf10))
- **nika-schema** — Close the proven stack-overflow class — two loud depth bounds ([41a6dcb81](https://github.com/supernovae-st/nika/commit/41a6dcb81efa4f4c3c7fad9612737f41e8ec1f78))
- **nika-schema** — Check honors the locked exit-code contract + json parse payload ([0188d4e3a](https://github.com/supernovae-st/nika/commit/0188d4e3a0cac57a41d7b411f1c8cf92c061425e))
- **nika-schema** — Fold the review-swarm round — ascii contract + Verdict + hardening ([de517b532](https://github.com/supernovae-st/nika/commit/de517b5320967c89774de78a7514157834558f9c))
- **nika-schema** — Unbreak mutation testing + kill the predicted span survivor ([f2eec1917](https://github.com/supernovae-st/nika/commit/f2eec19172b6da2475a87025ab2aa13d369ea023))
- **nika-schema** — The loud-skip allow names the right lint (print_stderr) ([a55a7428a](https://github.com/supernovae-st/nika/commit/a55a7428aeec93688d0dbefecb595086cbe69842))
- **nika-schema** — Review fold — HK bound restored, DoS cap, one voice ([250a05f72](https://github.com/supernovae-st/nika/commit/250a05f7291b9d35aaafc30ae2a9b53a51280267))
- **nika-schema** — Agent-whitelist namespace gate — reject a second colon (invoke parity) ([2a5f26ec5](https://github.com/supernovae-st/nika/commit/2a5f26ec54219ae0ea126469087659d9d5c88fca))
- **nika-schema** — Arg-injection catalog holes + per-kind suggestions (audit) ([c5a19ccf4](https://github.com/supernovae-st/nika/commit/c5a19ccf4c78847abe3f57451fa27943b6846b76))
- **nika-schema** — Treat infer/agent prompt as an IFC egress sink ([1dcaa5d2f](https://github.com/supernovae-st/nika/commit/1dcaa5d2f924c981204f7d78be891a44b755b64d))
- **nika-schema** — Closed-island CEL grammar errors are NIKA-VAR-005 not VAR-008 ([7f7816941](https://github.com/supernovae-st/nika/commit/7f7816941c47aa6e5ecdfb33e8f8400d62d636e0))
- **nika-schema** — Cap postfix chain depth — untrusted-input stack overflow ([d782f16ad](https://github.com/supernovae-st/nika/commit/d782f16ad3d1917d30412251942cae9923dcdd34))
- **nika-schema** — Intern the IFC taint-trace to kill an O(n2) DoS ([50598fac5](https://github.com/supernovae-st/nika/commit/50598fac56972ee055c7e85a965f8bdf5e6eb826))
- **nika-schema** — Bound the gate in-list scan and secrets membership ([d05abe56b](https://github.com/supernovae-st/nika/commit/d05abe56bbd935b636394dc31caf060bcfedd0a6))
- **nika-schema** — Remove quadratic dedup in when-gate literal scan ([fe9fdf72d](https://github.com/supernovae-st/nika/commit/fe9fdf72d054156e6a32291af6f5ca5b89565bbf))
- **nika-schema** — Non_exhaustive on public source-position types ([8ab013bd4](https://github.com/supernovae-st/nika/commit/8ab013bd4c57ce5888879f0d6b651a5c8953e6c5))
- **nika-schema** — Adapt flow.rs IFC test to interned-taint signature ([e1127ed74](https://github.com/supernovae-st/nika/commit/e1127ed747bfd53e81b836c7f9296771abf624f4))
- **nika-types** — Negative sub-ms remainder in WallDuration Display ([e2bd999ad](https://github.com/supernovae-st/nika/commit/e2bd999adf225840a31f0e37ed12b108270ead1d))
- **nika-types** — Permits.net.http host match is case-insensitive (RFC 4343) ([fd0c3277a](https://github.com/supernovae-st/nika/commit/fd0c3277a86918d74a8137d702f5062d60d11a8d))
- **nika-verb-agent** — Review fold — reach invariant, wire shape, amplification ([d0c5bf125](https://github.com/supernovae-st/nika/commit/d0c5bf125e3a7e2367b28a35374cb0603a762b27))
- **nika-verb-agent** — Agent:compose is not a tool invocation — stop reporting it as one ([aac3170a1](https://github.com/supernovae-st/nika/commit/aac3170a14c0c64f3a20812002920c3cb6a1390c))
- **nika-verb-agent** — Enforce agent schema at the wire (BUG#11) ([7a0e94e43](https://github.com/supernovae-st/nika/commit/7a0e94e432c4615387e4fe8d32b104524d516757))
- **nika-verb-agent** — Agent schema re-ask drops orphan tool_calls (openai) ([f82e4ba4d](https://github.com/supernovae-st/nika/commit/f82e4ba4d482ba963c0abdbd8c34d0e76ccb237b))
- **nika-verb-exec** — Fold gate-11 swarm findings — 3 lenses, 0 P0 ([f82c76d1e](https://github.com/supernovae-st/nika/commit/f82c76d1e39630b31c85333080031393ef79760e))
- **nika-verb-exec** — Reject NUL in an env value (review F2) ([62a7ff8b3](https://github.com/supernovae-st/nika/commit/62a7ff8b34a6100356f6e2c28a6160c83f085f27))
- **nika-verb-exec** — Exec blocklist hit speaks NIKA-SEC-001 not EXEC-002 ([09dd0816b](https://github.com/supernovae-st/nika/commit/09dd0816b5f59b00ae84f16cdf2d5cee208b90df))
- **nika-verb-infer** — Fold gate-11 swarm findings — 3 lenses, 0 P0 ([ff4865ff8](https://github.com/supernovae-st/nika/commit/ff4865ff8b5eec9e1c73ee382174ef1a145ab275))
- **nika-verb-invoke** — Fold gate-11 swarm findings — 3 lenses, 0 P0 ([23500f7ce](https://github.com/supernovae-st/nika/commit/23500f7ce80b5a71a539af8d0c93701093e58f0b))
- **schema** — Stop double-backticking task ids in DAG-003 + loop-local ([befd93e90](https://github.com/supernovae-st/nika/commit/befd93e901d11cfdac4a00525fd67a6cdfb497f0))
- **spec** — Correct nika-catalog-verify layer L2 → L4 ([5f9e9553f](https://github.com/supernovae-st/nika/commit/5f9e9553f4bab5e5f0ec5372eb71bf7bd3279938))
- **stabilize** — Allow MIT-0 + refresh exec-runner LOC anchor ([2a8553942](https://github.com/supernovae-st/nika/commit/2a85539425cb74accd096116e1b3fd064edebdc2))
- **workspace** — Add crates/ to index after rename ([20a49306a](https://github.com/supernovae-st/nika/commit/20a49306a247056132c54a9897376bdc53acf788))
- **workspace** — Defer nika-bm25 layer entry to W3 admission · adr-038 cleanup ([8ddb750e6](https://github.com/supernovae-st/nika/commit/8ddb750e64f605e6cc2d6753df42b52c3af4c001))
- **workspace** — Invoke tool outputs keep their structured type ([8d556aada](https://github.com/supernovae-st/nika/commit/8d556aadafe35cc0c33e5fce8cb81cdfc1b64040))

### ⚡ Performance
- **diamond** — Release profile + const fn + blueprint v1.3 amendments ([def291c2b](https://github.com/supernovae-st/nika/commit/def291c2b2f8fbbdcc86a7c8b00495d3f1808644))
- **nika-bm25** — Post-admission stabilization · architect + rust-perf converge ([7fcd75fef](https://github.com/supernovae-st/nika/commit/7fcd75feffdff3f0ce8fe33392aca5c6cf8bc806))
- **nika-extract** — Single-pass tidy_text — drop two full-size copies ([c8781a6ec](https://github.com/supernovae-st/nika/commit/c8781a6ecb010bdef54f813445ef2fa560ba9b58))

### 🔨 Refactors
- **catalog** — Reconcile builtin set to spec 26 + ADR-084 ([a7193eba0](https://github.com/supernovae-st/nika/commit/a7193eba0e59cafdaf208661d4b8a080dc30f03f))
- **catalog** — Nika:csv_to_json → nika:convert · ADR-086 rams sweep ([c346cba19](https://github.com/supernovae-st/nika/commit/c346cba19a5d69ee1422a974db87af099f56e666))
- **catalog** — Nika:sleep + wait_until → nika:wait · ADR-087 rams ([bcf5c8e63](https://github.com/supernovae-st/nika/commit/bcf5c8e63380ab4755681e7b0e368082a46de59b))
- **catalog** — 4 introspection → nika:inspect · ADR-088 rams sweep ([37af410a4](https://github.com/supernovae-st/nika/commit/37af410a4c17fe8669953eba2c4b7109ef2ecc5a))
- **ci** — Centralise the workspace-members parser in _lib.sh ([13e2e6c8c](https://github.com/supernovae-st/nika/commit/13e2e6c8c51719e5070787577d6aa1413483d02c))
- **dx** — DX file routing cleanup + mintlify v4 rename ([14edf9b68](https://github.com/supernovae-st/nika/commit/14edf9b6830f672dd00df96139ef1fdd9747375a))
- **dx** — Add commit-granularity rules ([c87fc9372](https://github.com/supernovae-st/nika/commit/c87fc937274e951dd19e0a27a3e4baa84247a318))
- **dx** — Expand post-commit hooks for admission + push reminders ([218933375](https://github.com/supernovae-st/nika/commit/2189333759299c0c90620360a0b0e75e8386a111))
- **dx** — Move hq dashboard to port 4242 + wire shadcn/magicui/tailwind/threejs MCPs ([55d8c4beb](https://github.com/supernovae-st/nika/commit/55d8c4beb03f9d1c78074eb87c33afb0f2d13fa6))
- **dx** — Count nika-screen WIP · refresh status block to fe2b ([3b2daaf02](https://github.com/supernovae-st/nika/commit/3b2daaf02eb173407438fc6264a4668fb48fb456))
- **dx** — Lock v0.90 crate target at 42 (was 40-42 range) ([ba2f65236](https://github.com/supernovae-st/nika/commit/ba2f652366000597c99177e06d38245b818b1f29))
- **dx+docs** — Hygiene all GREEN + mintlify rewrite ([cd3cde9a1](https://github.com/supernovae-st/nika/commit/cd3cde9a101ad364d49a4847903dc743b344d748))
- **error** — As_str on Category/Severity — delete explain's parallel taxonomy ([bafa20762](https://github.com/supernovae-st/nika/commit/bafa2076234fce76299a8271c390401235d136e6))
- **hygiene** — Single-source the WIP split + fix vector 3 LOC drift ([7af76eeb5](https://github.com/supernovae-st/nika/commit/7af76eeb5097274f701c0b67ecabd192838153f3))
- **kernel** — Co-locate memoryerror nikaerrorcode impl · diamond w2.2 nuke drift ([ba9bd9c1b](https://github.com/supernovae-st/nika/commit/ba9bd9c1b259c53c5e91288a6542a04141fb285c))
- **kernel** — M1 polish · contract fix h1 h2 + sprint 0 additive ([4836cf7aa](https://github.com/supernovae-st/nika/commit/4836cf7aa97aeb359a7a8b42add7b63c6a7f8162))
- **kernel** — Ec-4 ratchet · captured_at_ms → captured_at_ns · ns canonical ([438234ad2](https://github.com/supernovae-st/nika/commit/438234ad2dc98488e7a0cec980d3f52fad6f89c3))
- **kernel** — M2 trio implements the Send trait variants ([9c455e2cd](https://github.com/supernovae-st/nika/commit/9c455e2cdfe316dd5cdc63423226d626f0bedb21))
- **mintlify** — Restructure nav to 2 tabs (Guide | Reference) ([365a31d5f](https://github.com/supernovae-st/nika/commit/365a31d5f8056ed16aca1e7b14492309ae7455e5))
- **mintlify** — Reference workspace — live snapshot + constellation + delete duplicate ([8e85241af](https://github.com/supernovae-st/nika/commit/8e85241afb30744a75b3122178f8feeafe4e429f))
- **mintlify** — Split docs out to supernovae-st/nika-docs repo ([eb671f8a6](https://github.com/supernovae-st/nika/commit/eb671f8a6bc640d80d45eeeeb444905de008fd25))
- **nika-a11y** — Migrate to Pattern A — A11yError typed at the kernel ([29b749631](https://github.com/supernovae-st/nika/commit/29b74963156296a35d87a15ef2c0bca8fb5844e6))
- **nika-bm25** — Q6 split · core (pure-algo) + kernel (adapter) ([0c3b3f4a2](https://github.com/supernovae-st/nika/commit/0c3b3f4a275fe6c45b492ec27a6c7986480b04a4))
- **nika-bm25** — Revert q6 split · option e feature-gated · v1.4 reinforce ([edb72e9cb](https://github.com/supernovae-st/nika/commit/edb72e9cb2aed74f9c4ab5a1ec4d8a3fe10360af))
- **nika-bm25** — Sota rust 2026 perfecting pass post rust-pro + rust-architect audit ([3fa8e5eb8](https://github.com/supernovae-st/nika/commit/3fa8e5eb8d2f27e69ca3d2b3741e91b3ad775824))
- **nika-bm25** — Rank.rs — ONE selection algorithm, heap-bounded, tie-bug caught ([e655948f5](https://github.com/supernovae-st/nika/commit/e655948f5aeb5925b2d732aa8a4ff2474b2d3d4e))
- **nika-catalog** — Collapse public API from 3 paths to 2 — data/ is pub(crate) ([3360f1023](https://github.com/supernovae-st/nika/commit/3360f1023749a1a5a11c2d495654ce038937f904))
- **nika-catalog** — Migrate model_capabilities to TOML-driven rule table ([8c5cb4866](https://github.com/supernovae-st/nika/commit/8c5cb48668d9520dfea60a6e853a91ed7ae983be))
- **nika-catalog** — Hardening pass on Session 2a (5-agent review) ([e766a122c](https://github.com/supernovae-st/nika/commit/e766a122c2843a2ec2965964aa48222ff2c614ad))
- **nika-catalog** — Post-commit 5-agent audit findings ([9feb96956](https://github.com/supernovae-st/nika/commit/9feb9695668e309b5400930744650149e6a79cdf))
- **nika-catalog** — All_pricing — struct literals → ModelPricing::new ([e123acafc](https://github.com/supernovae-st/nika/commit/e123acafc45d7e2d6f60fb84f20c802253f96a3f))
- **nika-catalog** — Retire supports_vision — use input_modalities.contains(Image) ([34a488207](https://github.com/supernovae-st/nika/commit/34a4882073c2e2997426c12781dec254e27274c2))
- **nika-catalog** — Wire build.rs to nika-catalog-codegen · nuke twin ([0e85c9618](https://github.com/supernovae-st/nika/commit/0e85c9618b7f5416953b9606c058c904c5159bf1))
- **nika-catalog-codegen** — Satisfy push-hook ratchets · fn-length + loc-limits + machete ([074fc0614](https://github.com/supernovae-st/nika/commit/074fc0614baef084c669e38f3a138f4d97d094b6))
- **nika-cli** — Fed_back helper — the wave-3 test ducks under the fn cap ([be582b247](https://github.com/supernovae-st/nika/commit/be582b2470daff888808b966274a53d40562c822))
- **nika-clock** — Implement the ClockDyn Send companion ([9d2f58d88](https://github.com/supernovae-st/nika/commit/9d2f58d880522ff2168d34360a93840bc986214f))
- **nika-error** — Split into nika-types + nika-error ([5baeee044](https://github.com/supernovae-st/nika/commit/5baeee044d12c94b767a22cbe28cd1b81fff0e15))
- **nika-extract** — Sitemap event arms move into SitemapParser ([61e63159e](https://github.com/supernovae-st/nika/commit/61e63159e10d0060b57de4bebad9098a1e4f1658))
- **nika-http,nika-cli** — Give net the NetBoundary newtype + one capabilities_of derivation ([3f86e33ba](https://github.com/supernovae-st/nika/commit/3f86e33baa0f3101eaf7ab44dd2e60f3750b7a71))
- **nika-input** — Extract pure keymap module + structural proptest ([c7943ab9b](https://github.com/supernovae-st/nika/commit/c7943ab9bea6a470fabe6f328ec449ccc4c5fe07))
- **nika-kernel** — Prepare split — descend shared types to nika-error ([1513da3a2](https://github.com/supernovae-st/nika/commit/1513da3a2d0f7464c750f3db0c0a9ad488509c38))
- **nika-kernel** — Drop ObservabilitySink (Q12 Phase A — 5 → 4 channels) ([1119f42a5](https://github.com/supernovae-st/nika/commit/1119f42a53a6b94c5fa7fa9331da66070bf247d8))
- **nika-kernel** — Reassign provider error codes 380-429 → 330-379 ([1b812e664](https://github.com/supernovae-st/nika/commit/1b812e6643df01e2fb7dd62049568f6467acd19f))
- **nika-kernel** — Trim facade hub to actual deps — Gate 11 review finding ([a41db4098](https://github.com/supernovae-st/nika/commit/a41db4098858b6c5f1f9e313121a94a38ba4d216))
- **nika-kernel-mock** — Align MockShell to the Send-variant traits ([c5b44e170](https://github.com/supernovae-st/nika/commit/c5b44e1703614d5962b93c76e666779a4d54d5d3))
- **nika-ocr** — Migrate to Pattern A — OcrError typed at the kernel ([a6719e213](https://github.com/supernovae-st/nika/commit/a6719e2131c4c419f1ff4f93cfdb5bfdb1e33569))
- **nika-pack** — Error_codes() typed accessor — one parser, every consumer ([1e0d8b83d](https://github.com/supernovae-st/nika/commit/1e0d8b83d874aaaab86eaeb8c771dc6404d54f1f))
- **nika-providers** — Split gemini schema adapter to its own module ([4c6b0cc23](https://github.com/supernovae-st/nika/commit/4c6b0cc23ac118773bae8bbefc55e832cc3266a1))
- **nika-runtime** — Split run() under the fn-length ratchet ([47b7f43d2](https://github.com/supernovae-st/nika/commit/47b7f43d26d20fda21dc9c68f3e37dc0424ab234))
- **nika-runtime** — Extract dispatch_result from attempt_loop ([153f74d66](https://github.com/supernovae-st/nika/commit/153f74d66655dd1975f41dccdf82f7b70b85b36b))
- **nika-schema** — Nuke brouillon-era types ([5d08fd5be](https://github.com/supernovae-st/nika/commit/5d08fd5be60e7f89eecadfd0a32378e3497ac860))
- **nika-schema** — Rename parser expect to expect_token ([e6a1630f3](https://github.com/supernovae-st/nika/commit/e6a1630f36db3231fe706138a6c5b1a6610ea268))
- **nika-schema** — Split preference_rules into a tests.rs dir module ([292adc31d](https://github.com/supernovae-st/nika/commit/292adc31dcd8f4eb04a93a92a7aabb9e5371b9fe))
- **nika-screen** — Migrate to Pattern A — ScreenError typed at the kernel ([b8043ea96](https://github.com/supernovae-st/nika/commit/b8043ea96d2896a6cc99701e638a35b864740250))
- **nika-verb-agent** — Run() under the 100-line cap — extract classify_turn ([9dec6fff9](https://github.com/supernovae-st/nika/commit/9dec6fff9d9db19fa5cb9b90ca5a693ec5cecba8))
- **nika-verb-agent** — Tests split to src/tests.rs — the file cap unblocks the train ([15e0558b7](https://github.com/supernovae-st/nika/commit/15e0558b7eb0f79bde3a77b28ba125c2d5a54025))
- **nika-verb-agent** — Nuclear-review judo fold — fence parity, dead path, one predicate ([a9bd9f9d4](https://github.com/supernovae-st/nika/commit/a9bd9f9d4531d49dd65f5ab57864e965c9ccc10d))
- **schema** — Drop fetch verb · nika:fetch is a builtin not a verb ([b8e736d32](https://github.com/supernovae-st/nika/commit/b8e736d32add62009bf6be31384f71391fd80722))
- **scripts** — Cluster by responsibility + 3 READMEs ([d3099f99a](https://github.com/supernovae-st/nika/commit/d3099f99a576951a613edc7e669c3e8ab1d2f763))
- **types** — Privatise TaskId + ToolCallId inner fields (Wave 1.2) ([0e3ca19de](https://github.com/supernovae-st/nika/commit/0e3ca19dee9aff94fc2fcfa967eb8d5785ad63cb))
- **workspace** — Rename tools/ to crates/ + add layer metadata ([bb6863714](https://github.com/supernovae-st/nika/commit/bb6863714dcbddc101d9322d0a72113b53a4c16a))
- **workspace** — Split four functions under the 100-LOC fn cap ([76e4f8d0d](https://github.com/supernovae-st/nika/commit/76e4f8d0d55298751d86dd39ae0eea5240d604c9))

### 📚 Documentation
- **adr** — Bootstrap diamond adr process + 9 inaugural decisions ([4cac646e9](https://github.com/supernovae-st/nika/commit/4cac646e9d99e287654e029831370812280b7754))
- **adr** — Add adr-010 through adr-014 (5 sota improvement decisions) ([f0e032bd3](https://github.com/supernovae-st/nika/commit/f0e032bd3906d5e26441a4039e36e83c47c94517))
- **adr** — Add adr-015 expect-test for inline snapshot assertions ([751e85ec8](https://github.com/supernovae-st/nika/commit/751e85ec861107c49c004d476b4b199d093bd89f))
- **adr** — Add bidirectional cross-references in Related sections ([199119e94](https://github.com/supernovae-st/nika/commit/199119e9439e94f4f6058bd9ec9f9874e71cd5c2))
- **adr** — Add ADR DX system -- schema, scripts, updated template ([fe75f384f](https://github.com/supernovae-st/nika/commit/fe75f384f38d0a1c47ee272f1ce3fa5cc3b081d9))
- **adr** — Migrate 15 ADRs to YAML frontmatter + generate indexes ([196ba13ce](https://github.com/supernovae-st/nika/commit/196ba13cef763e64e2c11179c2fd437dfaaab738))
- **adr** — Write ADRs 016-020 — kernel design decisions (Batch F part 1) ([9b9e75d15](https://github.com/supernovae-st/nika/commit/9b9e75d1553001bd09d5003997200dc4b75f0445))
- **adr** — Write ADRs 033-034 — Phase C L0/L0.5 expansion plans ([69c284245](https://github.com/supernovae-st/nika/commit/69c2842458ee689452f3fa2cdf3714d92ac732e3))
- **adr** — Regenerate index.toml + index.json (22 ADRs) ([8bced775c](https://github.com/supernovae-st/nika/commit/8bced775c0d5bcc119d2470eec98f823ef299bfa))
- **adr** — Lock foundation v0.81 — 7 new ADRs + ADR-006 amendment ([6ee7d99de](https://github.com/supernovae-st/nika/commit/6ee7d99decc171d391d23e474d79b5c707101642))
- **adr** — Add ISP capability-axes × crate matrix (batch v.2) ([1718f2cc1](https://github.com/supernovae-st/nika/commit/1718f2cc151ee7992a6b038ec70464b5b87f8acb))
- **adr** — Regenerate index.toml + index.json (22 → 30) ([66beb4d17](https://github.com/supernovae-st/nika/commit/66beb4d177d49415f87dfdd47b975b0b981e01f0))
- **adr** — Stub ADR-029/030/031/032/035 for Wave 4A/4B reservations ([d58d981f8](https://github.com/supernovae-st/nika/commit/d58d981f8b77cbe38f600d9e4798883f47d60cff))
- **adr** — Add adr-037 bottom-up diamond progression (Accepted) ([50cbf9d2b](https://github.com/supernovae-st/nika/commit/50cbf9d2be3b05a7dd5f98540a748980682b6cc1))
- **adr** — Amend adr-028 — feature scheduling dropped per ADR-037 ([c3c4be389](https://github.com/supernovae-st/nika/commit/c3c4be3898414d738548039b8c522c72b2ab9703))
- **adr** — Add adr-036 MSRV policy stub (reserves FCI-036) ([61d3e830b](https://github.com/supernovae-st/nika/commit/61d3e830b8e77937fc3902823497665480355c0a))
- **adr** — Revert adr-028 status to accepted (schema valid enum) ([06c92950b](https://github.com/supernovae-st/nika/commit/06c92950b9da228a444f88336c3972a9da42517f))
- **adr** — Fix adr-028 date format (YYYY-MM-DD) + separate amended_date ([ac700434b](https://github.com/supernovae-st/nika/commit/ac700434b9e20f8955cd60f8052e2ccd9ae2eb47))
- **adr** — Fix adr-036/037 schema validation (id + affects_crates) ([bed1aabe0](https://github.com/supernovae-st/nika/commit/bed1aabe0b1d958c1ea3772266c5d549cf07d5c8))
- **adr** — Adr-038 nika-bm25 admission pre-flight · 12-gate readiness · proposed ([a632d3ca3](https://github.com/supernovae-st/nika/commit/a632d3ca35845e525df850dd91f7d6506e7c0fb2))
- **adr** — Nika-bm25 l1 row + workspace metadata · diamond w2.5 ([4c5fa01da](https://github.com/supernovae-st/nika/commit/4c5fa01daf18a39e5e48f4999b46a605dff0c658))
- **adr** — Adr-038 enables [] · drop ADR-NNN placeholders ([93d7f245c](https://github.com/supernovae-st/nika/commit/93d7f245c3fd877100d7713a0cd1dcd6b4bd1bcf))
- **adr** — Adr-040 cargo feature matrix · zero-cost modularity ([1934fcb62](https://github.com/supernovae-st/nika/commit/1934fcb62339d060d600931983d6fa7cb2352475))
- **adr** — Adr-039 + adr-041 + adr-042 · phase 1.5 architecture trio ([0f1ef3f25](https://github.com/supernovae-st/nika/commit/0f1ef3f25b4abd412fa528942c535140e2a90bd9))
- **adr** — Review-cycle amendments · adr-039 040 041 042 ([1612fb800](https://github.com/supernovae-st/nika/commit/1612fb80069a3399b01cd9de0fe0a21c48608506))
- **adr** — Ship ADR-078/079/080 trio · v1.5 best-architects 2030 audit close ([6317a8b27](https://github.com/supernovae-st/nika/commit/6317a8b27d13a210ed75875e8ca99263a36bfe02))
- **adr** — Adr-081 l1 effect-crate guard contract · 7 guards forever ([3e40c18b3](https://github.com/supernovae-st/nika/commit/3e40c18b3b92ea5d5498bef10643510978b411c5))
- **adr** — Adr-089 nika:json_diff jq-subsume REJECTED · keep rfc-6902 patch ([27c563c79](https://github.com/supernovae-st/nika/commit/27c563c79327d4af4da15da24d0e9a34b1808fe4))
- **adr** — Reciprocate related cross-links across the 081-089 cohort ([6c3b12a85](https://github.com/supernovae-st/nika/commit/6c3b12a8590700e44d4ec379475c29cf12ca2ee6))
- **adr** — Lock enables[] = curated-highlight (DRI D-2026-05-30) ([5cd40bb4c](https://github.com/supernovae-st/nika/commit/5cd40bb4c994f2685f7fe40e21a8651508901b1d))
- **adr** — Sovereign local inference via pure-Rust candle sidecar (ADR-091) ([eb60cbb94](https://github.com/supernovae-st/nika/commit/eb60cbb948eb839286b154b1ed21339ec250ba58))
- **adr** — Adr-092 — make nika check a verifier, not a linter ([1810d241a](https://github.com/supernovae-st/nika/commit/1810d241acd2e5b9829acbb91339e45f0865df89))
- **adr** — Adr-093 tiny_http sidecar server + adr-094 nika-pck registry architecture ([300c52a3f](https://github.com/supernovae-st/nika/commit/300c52a3f36bf772cc079e4a353b5bbbe928a753))
- **adr** — Adr-092 evidence path follows suggest.rs out of check/ ([1c6f56456](https://github.com/supernovae-st/nika/commit/1c6f56456cb3913f6dfd2a9d7c88ff5365607716))
- **adr** — Adr-092 second check-example path follows the dir migration ([6822e1206](https://github.com/supernovae-st/nika/commit/6822e120678639c3181e6773b60143c5133ad03a))
- **adr** — Exec verb security architecture (ADR-095) ([9e9462636](https://github.com/supernovae-st/nika/commit/9e9462636623299a42fc171ab333e1e6f19b1515))
- **adr** — Reconcile ADR-095 with the reserved plugin::sandbox (per-platform crates) ([651fef111](https://github.com/supernovae-st/nika/commit/651fef11157415b9ae0b52e961a7a17adbf0c9d4))
- **adr** — Nika-sandbox-seatbelt crate-spec (Gate 1) ([8734fdc44](https://github.com/supernovae-st/nika/commit/8734fdc44ae8a9b3c18678763e8f235674225881))
- **adr** — Amend ADR-002 — real semver toward a 1.0 launch ([e8c7aad64](https://github.com/supernovae-st/nika/commit/e8c7aad648c9c21d8637a05aee4392fb86d333c7))
- **adr** — Align ADR cross-refs to real semver toward 1.0 (D-2026-06-20-N1) ([a1548d8e8](https://github.com/supernovae-st/nika/commit/a1548d8e8ee6a29bab3bf6ae28b50bb61124d8ea))
- **adr-003** — Record social→structural gate enforcement (Gate 5 + ADR-081) ([65787a058](https://github.com/supernovae-st/nika/commit/65787a05844c0326f5745696a93489b3cc04060f))
- **adr-080** — V1.1 amendment · phantom-CVE scrub + nika error code migration + seccomp-bpf ([80d5da96b](https://github.com/supernovae-st/nika/commit/80d5da96bd22478c29d7df078588ed7559088312))
- **adr-090** — Structural doctrine enforcement — gates project the SSOT ([ff54083af](https://github.com/supernovae-st/nika/commit/ff54083af2911c616e06c8e38a5b3de18aecba2f))
- **agents** — Un-ignore AGENTS.md + refresh HEAD + olympus rename ([44dd2ac2f](https://github.com/supernovae-st/nika/commit/44dd2ac2faaf43598b6eb1bf90f37496988c5962))
- **agents** — De-drift the agnostic entry — projection-by-default snapshot ([91f65f237](https://github.com/supernovae-st/nika/commit/91f65f237f6f44c7f262301bf5473e5dd9e0db44))
- **agents** — Route workflow authoring to the spec protocol ([8497207c2](https://github.com/supernovae-st/nika/commit/8497207c2734b1a7f98637a4283d7898875421cd))
- **arch** — Land v0.81 forward-compat seams + L0-L4 layer registry ([61d229547](https://github.com/supernovae-st/nika/commit/61d229547fc52000b14e8b19ed393a3769dd7a8f))
- **arch** — Land L0 brainstorm decisions + dep audit alignment ([e36dd8de7](https://github.com/supernovae-st/nika/commit/e36dd8de7527a34a7ef3d16f72a21a21dd5a600b))
- **arch** — L0/l0.5 swarm audit — revert q8, add q9-q10, fix incoherences ([5e810a94a](https://github.com/supernovae-st/nika/commit/5e810a94adbb377816500588eaebac4f87e3de97))
- **arch** — Blueprint 2036 final v0.x · 10-year nika horizon ([3a6e36869](https://github.com/supernovae-st/nika/commit/3a6e3686989e7ea7469e478eac065b19e8c2457b))
- **arch** — Blueprint 2036 v1.1 · per-crate detail + best-enemies sota ([efdf7c114](https://github.com/supernovae-st/nika/commit/efdf7c114452822a757a799b5cfb98275edf0c03))
- **arch** — Blueprint v1.2 · 11/10 amplifiers + ai-2027 guardian doctrine ([f9f4f6e1a](https://github.com/supernovae-st/nika/commit/f9f4f6e1a5cebff586651e4c555e30fa60a45f82))
- **arch** — Blueprint v1.2 fold-pass · 9→4 amplifier adrs · prose-only ([1fb3a5d63](https://github.com/supernovae-st/nika/commit/1fb3a5d636dcf61de0486b6f8e700abf4986f8a5))
- **arch** — Blueprint v1.5 · best-architects 2030 discipline ratchet ([3741eae91](https://github.com/supernovae-st/nika/commit/3741eae91a52379fc9aff764777a16e7f6293660))
- **architecture** — Reconcile layer model to 6-tier L0..L5 (P0-6A) ([0bc4df618](https://github.com/supernovae-st/nika/commit/0bc4df61880ee1de97e45c61aeb21e862b0e2d4f))
- **architecture** — Add FCI-NNN and INV-NNN numbered anchors ([9fdfea52b](https://github.com/supernovae-st/nika/commit/9fdfea52b12dd0389a712fc516540f6d58ab858d))
- **architecture** — Add constellation reconciliation 2026-04-17 report ([e7bef7e74](https://github.com/supernovae-st/nika/commit/e7bef7e74bb3eb96aeab082e4ed13925911c47c2))
- **architecture** — Review fixes · templatable carve-out + honest codegen claim + registry parity ([b5c9d15ae](https://github.com/supernovae-st/nika/commit/b5c9d15ae5821ac93ee288b3c067d0776783d64b))
- **architecture** — Gate-12 error-code contract + kernel split trigger status ([5959bd88a](https://github.com/supernovae-st/nika/commit/5959bd88af6f3268f0bf3c0c4a4015cb9c459ed6))
- **architecture** — Kernel 4-way split · census freeze + 4 sibling specs ([b837b3cc4](https://github.com/supernovae-st/nika/commit/b837b3cc430fa49da85065edc8fcd46badf6584b))
- **architecture** — Kernel split step 6 — registry + status + evidence-path cascade ([a1f065efa](https://github.com/supernovae-st/nika/commit/a1f065efa137053f5ed10703508c2ed927812488))
- **architecture** — R4 error-trait completeness audit table — b5 close ([a06662dca](https://github.com/supernovae-st/nika/commit/a06662dca01db8408538c65a0b34a43f70a97917))
- **architecture** — Blueprint-2036 catalog count derives from spec canon ([de174e317](https://github.com/supernovae-st/nika/commit/de174e317357177a093e80380af8a08346d5cda5))
- **canon** — Cascade 4-verb taxonomy · fetch is a tool not a verb ([efcc4df94](https://github.com/supernovae-st/nika/commit/efcc4df946c1aaba4d7eba3faebeddc23da10e7f))
- **canon** — Lock nika: v1 envelope · ADR-082 supersedes ADR-021 ([a7d0c656f](https://github.com/supernovae-st/nika/commit/a7d0c656f7abc60ed8e12ceff9c58353eb83b377))
- **canon** — Live docs state the current canon only — narrative purged ([31335c42c](https://github.com/supernovae-st/nika/commit/31335c42c7015a94131fd042fa6f098ea0cd2d34))
- **changelog** — Record swarm-3 batch i.b + wave 3a/4a/4b/4c session ([6f394aa23](https://github.com/supernovae-st/nika/commit/6f394aa23a20c1d80b4832bcc4f1b8b51429dfae))
- **changelog** — Fix stale MCP alias count 113 to 105 (grep-verified) ([ed443d081](https://github.com/supernovae-st/nika/commit/ed443d081050c0e79ecd6b0f6f3222f6d42dd60a))
- **changelog** — Document the v0.1-v0.28 public version history ([ea4af285c](https://github.com/supernovae-st/nika/commit/ea4af285c32e78343837eff205e2ac861083eba6))
- **claude** — Sync narrative with canonical block (905 tests, 32 providers) ([8241ab7ed](https://github.com/supernovae-st/nika/commit/8241ab7edcdd9550b6979fe79b27c56ae9cea6b8))
- **claude** — Refresh auto-state HEAD to ee74d97e0 · post-rename drift fix ([611ccdf7e](https://github.com/supernovae-st/nika/commit/611ccdf7ed08aaae303df310a8e8ae9fa697c410))
- **coherence** — Deep de-stale sweep — shipped names, 1+10, no hand-counts ([bacf5385c](https://github.com/supernovae-st/nika/commit/bacf5385c71319aea8c2255d87df535b3490c3b3))
- **contributing** — Add CONTRIBUTING.md with 12-gate workflow ([1f750c800](https://github.com/supernovae-st/nika/commit/1f750c8006cdd6bc2f90da889d1be4a0cafdc3da))
- **crate-spec** — Add nika-catalog-codegen Gate 1 spec ([41da7e565](https://github.com/supernovae-st/nika/commit/41da7e565745e53d7e74210d6cf95a4dbb3899e1))
- **crate-specs** — Nika-bm25 gate table update · 7/12 shipped ([c0d3a8f40](https://github.com/supernovae-st/nika/commit/c0d3a8f40f32eb9fc2f5d712e4af84bb0b9b606c))
- **crate-specs** — Nika-bm25 gate 5 mutation 96.9% kill ✅ ([636da4a21](https://github.com/supernovae-st/nika/commit/636da4a21ca9fc9a4a756c402d5c2cc632650aae))
- **crate-specs** — Reconcile fs/http/blob gate evidence to ground truth ([a5d214c00](https://github.com/supernovae-st/nika/commit/a5d214c00cde169c4510abed902174b084a5b4b0))
- **crate-specs** — Nika-policy (s8) — design locked, impl sequenced post-kernel-migration ([130019031](https://github.com/supernovae-st/nika/commit/130019031141beb5e27f3451842ed5bd9ba53562))
- **crate-specs** — Nika-providers (s8.5) — design proposal, kernel seam verified ([8b7529540](https://github.com/supernovae-st/nika/commit/8b752954060c5dab7f4a8895017018a49a2440db))
- **crate-specs** — Nika-browser (m2.5) — gate-1 spec, backend resolved ([e8afd8f1a](https://github.com/supernovae-st/nika/commit/e8afd8f1a2e681275aa8a7cb7780b35faf067d68))
- **crate-specs** — Nika-browser — b.2+b.3 shipped, headful-default clarified ([dd111a0e4](https://github.com/supernovae-st/nika/commit/dd111a0e41558675a8b717b9909c8cff64bf6b0f))
- **crate-specs** — Scaffold Connectome climb — 10 Gate-1 specs ([#113](https://github.com/supernovae-st/nika/issues/113)) ([618cdb2df](https://github.com/supernovae-st/nika/commit/618cdb2df2a367910c187f13076428e143cd40fa)) ([#113](https://github.com/supernovae-st/nika/pull/113))
- **crate-specs** — Nika-browser — guard-5 hardened §5b + gates B.2/B.3 shipped ([fdb42d3e4](https://github.com/supernovae-st/nika/commit/fdb42d3e40f35f10cb255cdd12fdfac382a149a0))
- **crate-specs** — Nika-infer-local — Gate-1 contract + candle loop design ([311e49870](https://github.com/supernovae-st/nika/commit/311e498700ca019a4a8c05f54833b346abff04c7))
- **crate-specs** — Flip 12 stale status rows to admitted + convention readme ([e611f612a](https://github.com/supernovae-st/nika/commit/e611f612a8a5d09378ea5913e49cec5aa55619fe))
- **crate-specs** — Nika-infer-local §5bis — the connection path (build-ready) ([7ab0ed1fa](https://github.com/supernovae-st/nika/commit/7ab0ed1faefc839350551660a6573520f1945cec))
- **crate-specs** — Nika-cli display contract + runnable render prototype ([0b714eee4](https://github.com/supernovae-st/nika/commit/0b714eee4a4bea1c52c95e012e4616312437cef6))
- **crates** — Readme for codegen + verify + schema + types crates ([3ba6de3b8](https://github.com/supernovae-st/nika/commit/3ba6de3b8d7d786061534fab78c61ad9acd64b86))
- **diamond** — Refresh auto-state · post pre-w3 stabilization ([37534abaf](https://github.com/supernovae-st/nika/commit/37534abaf58187b9f3bbe6cb1df6c9a8be09334a))
- **diamond** — Pre-w3 doc quality · 5 critical fixes ([40dde1110](https://github.com/supernovae-st/nika/commit/40dde1110afb3015fde46b96916469e8a16ee52b))
- **diamond** — Ship 4 per-crate readmes + code-of-conduct + security ([d0ef54445](https://github.com/supernovae-st/nika/commit/d0ef54445d8c1565f1b02529690a301712b9e0e1))
- **diamond** — Refresh auto-state + changelog · 2026-05-12 session arc ([41797d452](https://github.com/supernovae-st/nika/commit/41797d452b78aefbd418a1a0a4760ab0029f4289))
- **diamond** — Security lens audit · adr-071/072/073 + cross-link cohesion ([20c0f21eb](https://github.com/supernovae-st/nika/commit/20c0f21eb9ddda1ba355e7a5e73962d6c1f0f025))
- **diamond** — Contributing · cross-link + branch rename carry ([5f7510a32](https://github.com/supernovae-st/nika/commit/5f7510a32cf4b5b7a275921bc27306fd9dd33987))
- **diamond** — Connectome cluster count 9→10 satellites — rerank m13 ([144c5bca6](https://github.com/supernovae-st/nika/commit/144c5bca65530fc4ca176ff3a5d45b12aa75f555))
- **diamond** — Crate tree mirrors the layer registry — shipped reality ([492068e68](https://github.com/supernovae-st/nika/commit/492068e6846c580ab7b9895860b90388cdb605ff))
- **docs** — Rebuild readme with current v0.80 state + docs.nika.sh links ([460ef9851](https://github.com/supernovae-st/nika/commit/460ef9851603f7da832f1dc974c05a56298d5154))
- **docs** — Add wave 4e mintlify split entry to unreleased ([b2b4dceeb](https://github.com/supernovae-st/nika/commit/b2b4dceeb2cc0e2ad618c72242eb698cc4952752))
- **docs** — Refresh roadmap current-state adr/hygiene numbers + docs repo ([942b18aec](https://github.com/supernovae-st/nika/commit/942b18aec843df927d64a8cf2099879ecafd3978))
- **docs** — Rewrite diamond.md to current state ([3ec8c6456](https://github.com/supernovae-st/nika/commit/3ec8c645636b8238b03265d9e85d6f0367a83d1a))
- **docs** — Purge internal handoff + superseded docs from docs/ ([9ebaf05ca](https://github.com/supernovae-st/nika/commit/9ebaf05ca48221aea5de7f0ed674e9ca2c8855b1))
- **docs** — Nika 2040 intelligence-layer vision + llms.txt agent on-ramp ([3d283738f](https://github.com/supernovae-st/nika/commit/3d283738ff7a1cc6e8d467c4d7dc8afaf6a055c7))
- **docs** — Adr-090 evidence paths — list literals, not a brace glob ([51d50ffbe](https://github.com/supernovae-st/nika/commit/51d50ffbebed6fe46aeea2366bb2d3f367d3573b))
- **docs** — Reconcile FCI-016 (public fields require non_exhaustive) ([a8d5da279](https://github.com/supernovae-st/nika/commit/a8d5da279b19a5064c5894fb01b55f9d4a1fda92))
- **docs** — Mark error one-voice unification done (was TRANSITIONAL) ([5ff7bc488](https://github.com/supernovae-st/nika/commit/5ff7bc488b31100cc9b99e70fa3fa129ecdd7dab))
- **docs** — Add machine-readable GATE5-EXEMPT marker to nika-screen spec ([4cb4a9fbc](https://github.com/supernovae-st/nika/commit/4cb4a9fbc0fe7748f81d6aeadbeff55e2fe93d88))
- **docs** — Record nika-types Gate-5 measured result + exempt marker ([5540eda3d](https://github.com/supernovae-st/nika/commit/5540eda3ddd2a6dfbb81eac8345e56deccda95f8))
- **docs** — Bm25 spec — correct false "postings list" claim (P-4 perf) ([c1301e1a8](https://github.com/supernovae-st/nika/commit/c1301e1a83109712716d25b98bef416535ce6268))
- **docs** — Attest nika-catalog Gate-5 (measured 96.8% + exempt marker) ([ba8257021](https://github.com/supernovae-st/nika/commit/ba8257021261af375320b21aa8cae414b9be672e))
- **docs** — Nika-schema parser untrusted-input DoS gates (pre-admission) ([ce8106275](https://github.com/supernovae-st/nika/commit/ce810627568a0cfb09b40eabd57c935105accfae))
- **docs** — Document kernel I/O error-typing convention (FCI-023bis) ([587b58bfb](https://github.com/supernovae-st/nika/commit/587b58bfb4c0e2eedd7264236fe83839596807c3))
- **dx** — Add NEXT_SESSION orientation + /admit command + golden-commits ([5b222f795](https://github.com/supernovae-st/nika/commit/5b222f795909691fb9577f328a8a40fc53ddba1a))
- **dx** — Sync .claude/CLAUDE.md current-state to 2026-04-15 ([5c877aad6](https://github.com/supernovae-st/nika/commit/5c877aad6a71127be7e6f9a3fb2f29b587454f0a))
- **dx** — Sync diamond-progress + roadmap + claude current-state ([a1076cc2a](https://github.com/supernovae-st/nika/commit/a1076cc2a80b1a17ff9928c0a5c71369fef97b49))
- **dx** — Sync .claude/CLAUDE.md current-state — S2b+3 done ([12e88c610](https://github.com/supernovae-st/nika/commit/12e88c610a0e10862dbb7e0f7cc2600d24e62c7a))
- **dx** — Post-hygiene spot-fix — mintlify + diamond + readme + specs ([47889add5](https://github.com/supernovae-st/nika/commit/47889add54a00831723a0c78dbcd16590045fb0c))
- **engine** — Refresh status block head to ba2f65236 post 42-lock ([07525dbc3](https://github.com/supernovae-st/nika/commit/07525dbc3bf4ebd47f02f668864bbf5e20298871))
- **engine** — Sync status block — L0 6, admitted 9/42 post nika-event ([ab9302037](https://github.com/supernovae-st/nika/commit/ab930203722cc19f9a83052115083df567245663))
- **engine** — Sync roadmap status block — L0 6, admitted 9/42 ([b6ec46dde](https://github.com/supernovae-st/nika/commit/b6ec46ddef8b2048b1b32b8fee0b640ef6d00f80))
- **engine** — Sync status block · admitted 10 of 42 · l1 2 post nika-clock ([386024d6b](https://github.com/supernovae-st/nika/commit/386024d6b5ad58e152f48ccad35ed936411cbfef))
- **fci** — De-number the provider rationale (counts rot in prose) ([a6c87bf81](https://github.com/supernovae-st/nika/commit/a6c87bf8129ba276ae518803f950b2a689706de6))
- **gate12** — Forward-compat invariants — connectome names + verb range truth ([d21902cf0](https://github.com/supernovae-st/nika/commit/d21902cf0e5b7874c5491c6dd4c9078784293197))
- **hygiene** — Refresh vector count 15/20 → 31 across 5 files ([a08077b62](https://github.com/supernovae-st/nika/commit/a08077b625555e17457716336ee2271aad147323))
- **invariants** — Extend §9 with Wave 4A/4B reservations (FCI-035) ([2027856be](https://github.com/supernovae-st/nika/commit/2027856beb0fa20bfabb227eba362fe888956fed))
- **invariants** — Correct §See-also ADR-021..028 titles + status ([3f9fdb208](https://github.com/supernovae-st/nika/commit/3f9fdb20836fedeb05687a2f9dfe7bc7bf119370))
- **kernel** — Register computer-use error ranges 1000-1499 in the hub ([aa51f00da](https://github.com/supernovae-st/nika/commit/aa51f00da36c13ab91e63e9a283c7bba4cf094d1))
- **kernel** — Browser trait — typed BrowserError refs, drop stale ErrorKind ([33ab17247](https://github.com/supernovae-st/nika/commit/33ab17247735761f25e4e0602b67e6a3a3b2e3b3))
- **mintlify** — Add crate inventory reference page ([58d009df8](https://github.com/supernovae-st/nika/commit/58d009df86be370b8277c36aa9939919568bfa56))
- **mintlify** — Dark theme diagrams + status page + live numbers ([39b53e331](https://github.com/supernovae-st/nika/commit/39b53e331c297a2448f5b6e8939504127a7fd5c3))
- **mintlify** — Sync crates.mdx numbers with ground truth ([6135a5f0c](https://github.com/supernovae-st/nika/commit/6135a5f0c9e4f8a77e026a155086f36a60fcdc9a))
- **nika** — Adr-083 cross-platform doctrine for l1 computer-use ([1e94191ea](https://github.com/supernovae-st/nika/commit/1e94191ea62564bb91cccceeb8a2400837fbc312))
- **nika** — Refresh status block + active-arc narrative to 0b558f7f8 ([847529fec](https://github.com/supernovae-st/nika/commit/847529fec849c56fb95022a391182fa5ebbdc027))
- **nika-browser** — Module doc — guard-5 hardened contract (node_ref · peek/consume) ([85e282fce](https://github.com/supernovae-st/nika/commit/85e282fceb1021c04a8db6cc4525d5459b7466ec))
- **nika-browser** — Gate-5 budget 4→5 post-occlusion — honest re-measure ([4cc631e30](https://github.com/supernovae-st/nika/commit/4cc631e30e6efc15b6fd812e55bbe4e355123638))
- **nika-builtin** — Retire GATE5-EXEMPT budget → clean FLOOR 91.3% ([a67baa391](https://github.com/supernovae-st/nika/commit/a67baa3919d109b092de125942d5a5b49a4de2be))
- **nika-catalog** — Adr-008 addendum — materialize defaults source of truth ([42fb140e4](https://github.com/supernovae-st/nika/commit/42fb140e4ff5bc4996177e84b236bed98f91f770))
- **nika-catalog** — Renumber wire decision N3 to N4 · de-collide ([86e2ebd36](https://github.com/supernovae-st/nika/commit/86e2ebd360e4df28d3ece8462d49e5a62a52927f))
- **nika-cel** — Re-measure Gate 5 post the Gate-11 fixes (0 missed) ([29b1ec288](https://github.com/supernovae-st/nika/commit/29b1ec2884dc0bbc51c17eeff93cf562d3578bc7))
- **nika-error** — Refresh NIKA-464 explain — engine now enforces schema ([9751f8843](https://github.com/supernovae-st/nika/commit/9751f8843c57ec3028efa87945d95c37c06190d0))
- **nika-kernel-core** — Fs cancel-safety teaches detach-not-abort ([4fd7cd3cc](https://github.com/supernovae-st/nika/commit/4fd7cd3cc60e97242408e4213346c2561e4d2c70))
- **nika-providers** — Fix broken intra-doc links (Gate 8) ([3a03558d5](https://github.com/supernovae-st/nika/commit/3a03558d5f7276a70606ed30e1db6c858bd578fd))
- **nika-schema** — Cascade csv_to_json → convert in 2 doc-comments ([82c5ca980](https://github.com/supernovae-st/nika/commit/82c5ca980af0f9a647fb525a8d947eb7b65d9199))
- **nika-schema** — Nika check section — shipped surface, honest gaps, next steps ([f8e350070](https://github.com/supernovae-st/nika/commit/f8e350070bf4a2f0c406b8acd31982547d805380))
- **nika-schema** — Backtick argv[0] — rustdoc read it as an intra-doc link ([d9b8f1027](https://github.com/supernovae-st/nika/commit/d9b8f1027ae41d09e984e6b99b505498ab8e9cc4))
- **nika-schema** — Spec audit row — span axis + research-conformance suite ([8ad72b33b](https://github.com/supernovae-st/nika/commit/8ad72b33b5a510c3200c940e0226a8d22b43dcb4))
- **nika-schema** — Infer/agent prompt-secret sink is now canonical (F-03) ([510c2cde0](https://github.com/supernovae-st/nika/commit/510c2cde085be0046f0a72162b37d741bd40140e))
- **nika-schema** — Author the 12-gate admission ledger (11/12 green) ([2777d83fa](https://github.com/supernovae-st/nika/commit/2777d83fa28d2b70f2068a208f94cb6480207feb))
- **nika-schema** — Record the Gate-5 floor v2 + survivor rounds 1-2 ([aa096b0c0](https://github.com/supernovae-st/nika/commit/aa096b0c04d5a687a8041e801f68135b6fc7bc09))
- **nika-types** — Doctests import nika_types, not nika_error ([e41c33708](https://github.com/supernovae-st/nika/commit/e41c33708a5cfad2cec74fea15e193ad92e3418d))
- **nika-verb-agent** — Gate 1 spec — s12 the 4th verb, impl deferred ([6f6d63cb5](https://github.com/supernovae-st/nika/commit/6f6d63cb538ac07a0f87fda58a2f4d628730d8be))
- **nika-verb-agent** — Record the ToolDefinitionProvider blocker ([7709b6e5f](https://github.com/supernovae-st/nika/commit/7709b6e5f8e9b187685c0205407b4094b2b57f3f))
- **nika-verb-agent** — Close the spec↔impl coherence debt the agent arc introduced ([3f15d4e54](https://github.com/supernovae-st/nika/commit/3f15d4e5498625dd5dfbedcf3cd67b1d0b710ac2))
- **nika-verb-exec** — Gate 1 spec — s10 second L2 verb crate ([a9df92fee](https://github.com/supernovae-st/nika/commit/a9df92fee59b17fe8f50fa1187082c0095ab6baa))
- **nika-verb-exec** — Note NIKA-442 has no spec counterpart ([0c8504e16](https://github.com/supernovae-st/nika/commit/0c8504e162a9f8045f250a4f6b535594e12ef877))
- **nika-verb-infer** — Gate 1 spec — s9 first L2 verb crate ([5943503f6](https://github.com/supernovae-st/nika/commit/5943503f6d34afa288009bf8b8411642bc9e8078))
- **nika-verb-invoke** — Gate 1 spec — s11 third L2 verb crate ([1d83e12a4](https://github.com/supernovae-st/nika/commit/1d83e12a4d9af9bedc6413d015d332ac71c31219))
- **observability** — Purge ObservabilitySink ghost refs (Q12 rev.3) ([8dc307a98](https://github.com/supernovae-st/nika/commit/8dc307a98cb4e7f25cd4273eb426f44163fb3b2c))
- **plan** — Nika run shipped — mark B1-B5 done + the CEL follow-ups ([3211457ee](https://github.com/supernovae-st/nika/commit/3211457eefe54fbc1ee5cf80c58689d61340358b))
- **plans** — Record swarm-3 audit implementation plan ([6d9c92f85](https://github.com/supernovae-st/nika/commit/6d9c92f856ab1db12365b857618c4c840b081da8))
- **readme** — Mermaid architecture + timeline, honest status, fix stale badges ([24bd193d0](https://github.com/supernovae-st/nika/commit/24bd193d0a88b0cb5d33ecb9e94fa924cbddf0b2))
- **readme** — Cross-link the Nika language spec (engine ↔ spec coherence) ([c5651a12a](https://github.com/supernovae-st/nika/commit/c5651a12ae6b54fdfd0e564967321036a3c2636b))
- **readme** — Intent-as-code framing + connectome codename ([51b4c27b9](https://github.com/supernovae-st/nika/commit/51b4c27b9cd6565e6dc29900297f42d6e71a949b))
- **readme** — Clean up for NLnet readiness · de-confuse legacy version ([b162acf1c](https://github.com/supernovae-st/nika/commit/b162acf1c9751b0eb656ca6b7c5df932c7f3add7))
- **readme** — Drop legacy nika-diamond branch note from status table ([d9121a3a5](https://github.com/supernovae-st/nika/commit/d9121a3a57aa7d15f412d5a0e26fabafba054aa5))
- **roadmap** — Refresh canonical status block to HEAD 6d9c92f85 ([ee6b60a77](https://github.com/supernovae-st/nika/commit/ee6b60a772b9cd55c9b215732636e69cbaba2958))
- **roadmap** — Fix ADR count (021-027 → 021-028) + flag Wave 4A/4B stubs ([ac961b72b](https://github.com/supernovae-st/nika/commit/ac961b72b729650c64d0b6eac5d39078b17a1b17))
- **roadmap** — Add bottom-up progression banner per ADR-037 ([c46cdcc35](https://github.com/supernovae-st/nika/commit/c46cdcc358185497d24c21d992fe02c2edfc0e26))
- **roadmap** — Restructure per ADR-037 bottom-up progression ([cb157fa0e](https://github.com/supernovae-st/nika/commit/cb157fa0efafd026c7475782239aad35b3e08fa4))
- **roadmap** — Correct "spec curates 42→26" + flag pre-d-n6 builtin lists ([e5f262c56](https://github.com/supernovae-st/nika/commit/e5f262c563f0cd7eeb298e3fa9620132d523ecf8))
- **roadmap** — Fix line 96 spec-contract "42 builtins" → 26 ([c06192270](https://github.com/supernovae-st/nika/commit/c06192270a88b7eade8aaeb49b6f6d8c113d08e4))
- **roadmap** — 14 providers · openrouter promotion cascade ([64144a0fb](https://github.com/supernovae-st/nika/commit/64144a0fb202c3be215da3c985c87560e5b9f577))
- **roadmap** — Connectome cluster 1+10 — memory section de-staled to ratified canon ([272c942a7](https://github.com/supernovae-st/nika/commit/272c942a7929fe0576ab8eea6ba160de137a426c))
- **roadmap** — Providers rows reflect the shipped shape — rig not carried ([5301dd873](https://github.com/supernovae-st/nika/commit/5301dd8733ea3b7d70e380caf11c26e2bf64ee3e))
- **roadmap** — Refresh the auto-block — 32 crates, 2325 tests (vector 23) ([60998274f](https://github.com/supernovae-st/nika/commit/60998274f142d508c1d988fb753d86d4941f43bb))
- **roadmap** — Sync status block to the nika-cli admission (38/42) ([b85b2722b](https://github.com/supernovae-st/nika/commit/b85b2722b883bd66a4bfe46bac7279ef231669b6))
- **rules** — Collapse-vs-publish status precision — proposal not locked ([b9b4b8b91](https://github.com/supernovae-st/nika/commit/b9b4b8b91625d97fa2a4ef1505b4946129454e45))
- **screen** — Add crate-spec + sync status block · hygiene v6+v23 green ([fe2be76b0](https://github.com/supernovae-st/nika/commit/fe2be76b00e875e97e450efbeb9f35f3b415493a))
- **skills** — Adopt gitnexus MCP integration guide ([2343e89f1](https://github.com/supernovae-st/nika/commit/2343e89f146b4dbacf2f5c29a21551d090c1de44))
- **spec** — Attest nika-schema Gate-5 budget + when-gate DoS mitigation ([c56eddc0f](https://github.com/supernovae-st/nika/commit/c56eddc0f9e3a84d356debb092bd7efb1c53ae5b))
- **spec-sync** — Sweep engine docs to the curated-22 + closed namespaces ([#118](https://github.com/supernovae-st/nika/issues/118)) ([d0f76c632](https://github.com/supernovae-st/nika/commit/d0f76c632ea77dde517d26b95d590e0596401d73)) ([#118](https://github.com/supernovae-st/nika/pull/118))
- **state** — Refresh auto-block + ladder prose — s7 admitted, next s8 ([7c258d83f](https://github.com/supernovae-st/nika/commit/7c258d83f7cdc44c4bed5c740074246538948d61))
- **state** — Re-sync canonical blocks from main — vector 23 green ([1ca15ac3e](https://github.com/supernovae-st/nika/commit/1ca15ac3eee320826a1766ea883a9a6213f08b28))
- **state** — Re-sync canonical blocks post-merge — vector 23 green ([bfc654c4d](https://github.com/supernovae-st/nika/commit/bfc654c4d8ddf3f5b52e684b78ec88cbe7b17579))
- **state** — Narrative — m2.4 b.2+b.3 shipped, dyn-variant canon uniform ([0a6a227af](https://github.com/supernovae-st/nika/commit/0a6a227af53ec6c47c1dc35b312441ed6dc9e98d))
- **state** — Re-sync auto-block post s8.5 — L1.5 layer row added ([c1d86f02f](https://github.com/supernovae-st/nika/commit/c1d86f02fcb46ff32fc5fc3b1aa5851999e4b81c))
- **state** — Post-rebase block re-sync — 24/42 admitted, 1459 tests ([95bce169a](https://github.com/supernovae-st/nika/commit/95bce169a952bc36869516d65d7e0e7158be3587))
- **state** — Re-splice canonical blocks — vector 23 parity ([7a51e4db7](https://github.com/supernovae-st/nika/commit/7a51e4db78a0b873c81e64b740781f499b8930c3))
- **status** — Refresh status block HEAD 6d9c92f85 → 9ebaf05ca ([393fdefa8](https://github.com/supernovae-st/nika/commit/393fdefa8f89e174cbb8ea608e699b5589414521))
- **status** — Refresh canonical block · m1 kernel sealed · 1110 tests ([98f5a61b7](https://github.com/supernovae-st/nika/commit/98f5a61b7233f9ad7e45387f8f1a86833c42d39e))
- **status** — Refresh canonical block — HEAD b5a528e84 · 1267 lib tests ([d3693506b](https://github.com/supernovae-st/nika/commit/d3693506bfd2f4b89a90148ba741d60a92cc056c))
- **status** — Correct leaked branch name in canonical block — main not feat ([20375a62e](https://github.com/supernovae-st/nika/commit/20375a62e481bd9d9998f481de0c6ede9f98d3f3))
- **status** — Refresh auto-generated block — nika-runtime admitted ([a55b0f11e](https://github.com/supernovae-st/nika/commit/a55b0f11edefa05e737c0165e90e46da69282acd))
- **status** — Refresh the auto-block — 32 crates admitted, 2325 tests ([dc34ce450](https://github.com/supernovae-st/nika/commit/dc34ce4509a52f7c4b99d58e082fdc89c3019fe8))
- **status** — Refresh the status-block HEAD to the audit-fix tip ([4d94a7179](https://github.com/supernovae-st/nika/commit/4d94a717942308b339da6c8b81846e0f4a8b79e2))
- **workspace** — Post-wave-3 coherence review — align all logs + arch docs ([e5c17e781](https://github.com/supernovae-st/nika/commit/e5c17e781d0c69f074bebffddcd4f97e6413b5e5))
- Scaffold docs.nika.sh via Mintlify ([90ce455b0](https://github.com/supernovae-st/nika/commit/90ce455b039e3965900290df67c628665d114563))
- Ultrathink alignment — zero feature lost, philosophy clear ([8a2ef99fa](https://github.com/supernovae-st/nika/commit/8a2ef99fad3bf8e20b20de59cec52e682dba00b4))
- Update CHANGELOG + ROADMAP post Phase D Session 1 ([883112cdc](https://github.com/supernovae-st/nika/commit/883112cdcdb286c690c3c36e5601ebdf397753a7))
- Align DX + rules + public docs post Phase D Session 1 ([1a29bd32f](https://github.com/supernovae-st/nika/commit/1a29bd32fc9e6a5ff665a2781f7b32c3bd47be59))
- Ecosystem bible + GitNexus safe-install protocol ([9495d1a07](https://github.com/supernovae-st/nika/commit/9495d1a07a982ae4ce816467bc58a5468bcceb59))
- Align DX + CHANGELOG + ROADMAP post Phase D Session 2a ([133ffa0ff](https://github.com/supernovae-st/nika/commit/133ffa0ff8d5094e778334f8f84eb0ea21deddee))
- Deep DX audit — fix 6 stale P0 findings ([b708edf58](https://github.com/supernovae-st/nika/commit/b708edf58c3ef8535dfa0dce082b9f79b99bb0a5))
- Update ROADMAP + CHANGELOG for Session 4A stabilization ([c4ae1ab5e](https://github.com/supernovae-st/nika/commit/c4ae1ab5e9c010c9176ec19a97dbd7388d26df2f))
- Update ROADMAP + CHANGELOG for Session 4B data enrichment ([d6d30b810](https://github.com/supernovae-st/nika/commit/d6d30b8100b9e6113a035d7a13e3d0834550f974))
- Rewrite readme from scratch · SOTA · destination not journey ([e2d6fbf16](https://github.com/supernovae-st/nika/commit/e2d6fbf1633b2287c8587f34c8c4d6ce313fa3e9))
- Pillar-1 de-hardcode — agents.md + roadmap totals + claude narrative ([7bfceee91](https://github.com/supernovae-st/nika/commit/7bfceee913cdbe376ffe0e8c8273320a526c9ac5))
- Annotate the branch rename in 7 live mentions of nika-diamond ([945b9ef8c](https://github.com/supernovae-st/nika/commit/945b9ef8cfca7a0f979ec6b033df587b755fd82a))
- One-voice release ladder + canonical mcp tool ref ([dc9227445](https://github.com/supernovae-st/nika/commit/dc9227445f1ed2be3120e62c9ecdaa1f67a00b0e))
- CITATIONS.md — credit every work the engine stands on ([c915d592e](https://github.com/supernovae-st/nika/commit/c915d592e65549660d25067750d68c6eae107f39))
- CITATIONS — the Lv & Zhai row reflects BM25+ activation ([612326942](https://github.com/supernovae-st/nika/commit/6123269420f413354de3c38ee49452e061d26749))
- Post-runtime truth sweep — counts · claims · the error census ([9dfc5fd7a](https://github.com/supernovae-st/nika/commit/9dfc5fd7a2eb9c8ee4944f30c8fe0d8d035ddeab))
- Fix versioning residuals + restore ADR-002 status + sync mirror ([060998f2e](https://github.com/supernovae-st/nika/commit/060998f2ef31865ae963cb8cc61cb54d8bed7b15))

### 🧪 Tests
- **catalog** — Kill 14 mutation survivors (CapPatchBuilder + suggest_in) ([18f529271](https://github.com/supernovae-st/nika/commit/18f529271651f7393ad3c8a066928708d95a230e))
- **error** — Proptest registry uniqueness + memory cross-mapping · diamond w2.3 ([f13847f63](https://github.com/supernovae-st/nika/commit/f13847f635ed781a20333d3a2ea86a4b9bf52f01))
- **event** — Pin EventKind wire slug — serde and as_str() must agree ([636873ebe](https://github.com/supernovae-st/nika/commit/636873ebe26983600f22d196ede48672055be8d5))
- **hygiene** — Add batch-h-plus red-team harness scaffold ([ebfa16b44](https://github.com/supernovae-st/nika/commit/ebfa16b44bae3b5ff08aaec5454e0234612dd9b1))
- **nika-a11y** — Real-walk smoke skips on no-focus, never false-fails ([f88ef62dc](https://github.com/supernovae-st/nika/commit/f88ef62dcddfda151ff719992dbc3a1dc1f8861e))
- **nika-bm25** — Gate 2 red · manning iir ch.11 fixture + tdd tests ([82b70662f](https://github.com/supernovae-st/nika/commit/82b70662fa0718b403d93ffef46c4eb6b07d255e))
- **nika-bm25** — Gate 5 mutation killers · ranking parity tests ([23d648572](https://github.com/supernovae-st/nika/commit/23d648572c61a06c2f019badeb8a375c495d70fc))
- **nika-bm25** — Gate 5 golden values · pin exact scores within 1e-9 ([be751ea63](https://github.com/supernovae-st/nika/commit/be751ea635318b22cd99d6a4cc7fbdbf0d63de98))
- **nika-bm25** — Ultrathink improvements · okapibm25 parity + 10k bench + sourced 2030 ratchet ([bedb92929](https://github.com/supernovae-st/nika/commit/bedb92929f6510ef2d6ae8e47f8722b752ccfb93))
- **nika-bm25** — Property-test the BM25 invariants over the input space ([04c62417d](https://github.com/supernovae-st/nika/commit/04c62417dbce674644f7f34646b8a53a2c5c03e2))
- **nika-browser** — Pin backend_ref + bbox guard exact — 6 mutants killed ([16c4d8631](https://github.com/supernovae-st/nika/commit/16c4d8631df6bb81e7d77b34603aff4d19879991))
- **nika-browser** — Kill consume + epoch-timestamp mutants ([02f1502ac](https://github.com/supernovae-st/nika/commit/02f1502ac03c672d86d05953307dcb3f7d6c53d5))
- **nika-builtin** — Gate-6 property tests — the seed's promise kept ([b337e4372](https://github.com/supernovae-st/nika/commit/b337e437203572c1073edf2dd0b376dec5268659))
- **nika-builtin** — The polynomial proof is completion, not wall-clock ([a43a29ec1](https://github.com/supernovae-st/nika/commit/a43a29ec19f1d6b51589ca630c38c4813a81b395))
- **nika-builtin** — Decoder padding-gate pin + the disjoint-bits note ([5968ed3cf](https://github.com/supernovae-st/nika/commit/5968ed3cf66fb455c897dc8cf6d1e8bcdfbd7fed))
- **nika-builtin** — Harden Gate-5 surfaces + 12-gate readiness table ([59984a287](https://github.com/supernovae-st/nika/commit/59984a287d6cd89e6c3360d004157a0466def45c))
- **nika-catalog** — Add pricing proptest invariants (1000 cases) ([7f9625cfa](https://github.com/supernovae-st/nika/commit/7f9625cfa1da82f9fc1be0b96bae4681768cebbb))
- **nika-catalog** — Add capabilities proptest invariants (10k cases) ([0fba30556](https://github.com/supernovae-st/nika/commit/0fba305567e6fcfe7b3b33cb474a7500a15498b7))
- **nika-catalog** — Extend merge_with + estimate_cost regression tests ([9da58d7ac](https://github.com/supernovae-st/nika/commit/9da58d7ac96ce788000e6dc87f3f31fc51e3ffbb))
- **nika-catalog-codegen** — Kill 43 mutation survivors — 87% → 98.9% ([f0b5367e3](https://github.com/supernovae-st/nika/commit/f0b5367e3e86b9162b080f2f182065eb5f8bdab2))
- **nika-cli** — Kill the display-fold mutation survivors + deny wrapper ([5bf733032](https://github.com/supernovae-st/nika/commit/5bf733032264590c8451f407e4df67cda4ca0c3e))
- **nika-cli** — Pin per-task cost attribution — graph projector 100% viable-kill ([ac1d598dc](https://github.com/supernovae-st/nika/commit/ac1d598dc5e42d45147528e8183f0c083395b96b))
- **nika-cli** — Rehearse the agent loop over the real builtin dispatcher ([2c7e60f78](https://github.com/supernovae-st/nika/commit/2c7e60f78383deb3b250cd5a5bf1a9cd65f3b819))
- **nika-cli** — Wave-2 agent rehearsal — repair · batch · security · budget · schema ([0e712ce80](https://github.com/supernovae-st/nika/commit/0e712ce80dff1b972395ac2f8dba12cb6511e078))
- **nika-cli** — Golden frames for the two new §3.1 states + doc truth ([7a8a0b694](https://github.com/supernovae-st/nika/commit/7a8a0b6948282a3b4534900e1b9ba40c7f9bd14c))
- **nika-cli** — Wave-3 rehearsal — binary round-trip + tz through the real chain ([7f41b7d90](https://github.com/supernovae-st/nika/commit/7f41b7d90a18db1013d4d4981c750d42a7e5adad))
- **nika-cli** — E2e offers reflect the 23rd builtin (nika:compose) ([638f51b5b](https://github.com/supernovae-st/nika/commit/638f51b5b463263c3bf076549af760983ea131ec))
- **nika-cli** — E2e failure card expects the spec code NIKA-EXEC-001 ([0b558f7f8](https://github.com/supernovae-st/nika/commit/0b558f7f8139bcbb4f96ba4b54decea702ad08f7))
- **nika-cli** — Un-stale two pre-existing verbs_static failures ([560a483b9](https://github.com/supernovae-st/nika/commit/560a483b943425084dd965111abf257271adc6ce))
- **nika-cli** — Pin the run verb's locked exit codes (0/1/2) ([3f533bbb7](https://github.com/supernovae-st/nika/commit/3f533bbb7eb732f4c82002f212db5bf2a1b06bc9))
- **nika-cli** — Ignore the env-dependent examples-run smoke ([e0f2be722](https://github.com/supernovae-st/nika/commit/e0f2be72249fc4e95abd031c88337df7a9035d50))
- **nika-cli** — Harden render surface to Gate-5 mutation 91% ([e0f0cfa4e](https://github.com/supernovae-st/nika/commit/e0f0cfa4e6991afc429df345496dc5b9c89ad595))
- **nika-cli** — Add the Gate-6 fold property battery ([c43a8d0cd](https://github.com/supernovae-st/nika/commit/c43a8d0cdc2401e42606f8766d9af37b52cd525c))
- **nika-error** — Proptest lattice/identity laws + sealed.rs doc truthing ([2e61823c3](https://github.com/supernovae-st/nika/commit/2e61823c3895fd0124190d1755a86c7620097fa6))
- **nika-error** — Commit proptest regression seed for codes ([73244e50f](https://github.com/supernovae-st/nika/commit/73244e50f64106dc15e251e1c5328dca3b62b301))
- **nika-event** — Defuse the stale integration landmine — contract suite synced to 17 ([856964f10](https://github.com/supernovae-st/nika/commit/856964f10531f6ed4d183104db90833f3c35cc67))
- **nika-extract** — Real-socket fetch rehearsal + mutation killers · finalize deps ([b743babce](https://github.com/supernovae-st/nika/commit/b743babce61b49a239208c60eca554938cb03da2))
- **nika-extract** — Mutation ladder 83% → 100% — every viable mutant dies ([24309511e](https://github.com/supernovae-st/nika/commit/24309511e78ef04ebd6cd8a762136f38c7119adb))
- **nika-extract** — Harden the Gate-6 totality proptest ([be1edb25e](https://github.com/supernovae-st/nika/commit/be1edb25e20718212329970c220dac44c6795be7))
- **nika-extract** — Kill 73 mutation survivors to Gate-5 93% ([361f755d7](https://github.com/supernovae-st/nika/commit/361f755d74108aa43b0fe48fe8017f2ff5a5789e))
- **nika-http** — E2e over real loopback sockets — redirect · cred-strip · caps · timeout · stream ([5fe95b405](https://github.com/supernovae-st/nika/commit/5fe95b4051b3173fd4ccad263368d809b018471a))
- **nika-http** — Fix stale tls smoke test — TEST-NET is SSRF-blocked ([e14688c0f](https://github.com/supernovae-st/nika/commit/e14688c0f2e0cb4e4d8bbb16395daf857ae98388))
- **nika-http,nika-schema** — Pin check↔runtime host-extraction parity ([89396a1e0](https://github.com/supernovae-st/nika/commit/89396a1e0d066236373983567f47d281f0aabe33))
- **nika-kernel** — Add MemoryId deserialize error path tests ([a325cd564](https://github.com/supernovae-st/nika/commit/a325cd564d2632472baf3ab65cc024887b252c0c))
- **nika-providers** — Json-mode structured-output parity ([2ce6912cb](https://github.com/supernovae-st/nika/commit/2ce6912cb10adc52f5a4adc4fe20d206d9120a51))
- **nika-providers** — Cross-provider tool-call parse parity ([a07ff35f5](https://github.com/supernovae-st/nika/commit/a07ff35f5a3b0dfb83a47ca43a1ff80c1cb5011c))
- **nika-runtime** — The theorems extend over the buffered agent telemetry ([6fee61350](https://github.com/supernovae-st/nika/commit/6fee613506b87f35d811a7fbc64f46191c049c9f))
- **nika-runtime** — Close the agent-adapter mutation gaps — attempt stamps, compose, streak ([65ebb0153](https://github.com/supernovae-st/nika/commit/65ebb015352c488757e7d7dc355e3048425cc1d4))
- **nika-runtime** — Add required path to floor wide-fan write fixture ([8f6fd84ce](https://github.com/supernovae-st/nika/commit/8f6fd84cee0882107cc925ec1e407e1ade5051cc))
- **nika-runtime** — Lock for_each on_error:skip positional-null path ([81ca81976](https://github.com/supernovae-st/nika/commit/81ca81976527ec7b2335957514314edadacc94d0))
- **nika-schema** — Pin canonical envelope contract (RED · admission-prep) ([991700643](https://github.com/supernovae-st/nika/commit/991700643630ca1b050fb65be819e2f63fae5768))
- **nika-schema** — Kill the 5 mutation survivors — 100% on the new modules ([927f23b3b](https://github.com/supernovae-st/nika/commit/927f23b3bd6a74b390a6275a342cc6ab0229c8d0))
- **nika-schema** — Snapshot-pin both glyph themes + themed chrome typography ([453d3fa5b](https://github.com/supernovae-st/nika/commit/453d3fa5bf51a338ce24e2317e2e82f1dc2057a9))
- **nika-schema** — Gate-6 properties on the reference fold ([1fa313ffb](https://github.com/supernovae-st/nika/commit/1fa313ffb7a51acb20184c73d51ec7edcd298c84))
- **nika-schema** — Fetch fixtures gain their url — the new net caught them ([c57ea8b8d](https://github.com/supernovae-st/nika/commit/c57ea8b8d56132fa5402a5686e30350fa2bad84c))
- **nika-schema** — Deep conformance verdicts against the full check() surface ([afa8c10b4](https://github.com/supernovae-st/nika/commit/afa8c10b40f823f57e10915319c9a210f532c729))
- **nika-schema** — Gate-7 criterion benchmarks for the parse hot path ([7853d3c03](https://github.com/supernovae-st/nika/commit/7853d3c034ebb7a9f071174c1b4a501ddf71adb0))
- **nika-schema** — Cover gate-5 survivor clusters across four files ([8cf1225eb](https://github.com/supernovae-st/nika/commit/8cf1225ebadd5728003ee1ecd4f7a72dd45ec7db))
- **nika-schema** — Cover the remaining preference_rules lint survivors ([42667ef19](https://github.com/supernovae-st/nika/commit/42667ef195d4d5b8d206a0641156ddd26dbeab1a))
- **nika-schema** — Close the gate-5 long-tail survivors across ten files ([2f8817f17](https://github.com/supernovae-st/nika/commit/2f8817f17a7f0211c6520f0fd704bb9d5352d4fc))
- **nika-schema** — Kill the three lexer survivors (round 4) ([6134aca00](https://github.com/supernovae-st/nika/commit/6134aca00ac1b365f8351ddfb731f02deb1dfded))
- **nika-schema** — Cover parser/mod source-bounds + check/mod codes (round 5) ([6c9a078ac](https://github.com/supernovae-st/nika/commit/6c9a078acc0935f1860c7f21716178183959c484))
- **nika-schema** — Cover read_dag cap + pinch boundaries (round 6) ([c8350ee40](https://github.com/supernovae-st/nika/commit/c8350ee40588339d7d54b2da9b39c455b801b99b))
- **nika-schema** — Pin default-gate runnable path in reach ([27e0f3ddf](https://github.com/supernovae-st/nika/commit/27e0f3ddf5429abfbaca4e3df6f442ceb9b2e4b1))
- **nika-schema** — Kill expression-parser mutation gaps (round 7) ([7bba982a0](https://github.com/supernovae-st/nika/commit/7bba982a08789ecd1e205cd93e91de1708923800))
- **nika-schema** — Add parser + check benchmark (Gate 7) ([bcc1c8f32](https://github.com/supernovae-st/nika/commit/bcc1c8f329cd4696e6ebe55abc4906821b34009a))
- **nika-types** — Proptest audit for TrustLevel lattice + ID serde roundtrip ([73518494c](https://github.com/supernovae-st/nika/commit/73518494cd1c47b25ae7add3d487148a85812126))
- **nika-types** — Loom interleaving tests for CancelCtx (inv-029, batch ii ε.2) ([3a54b80d4](https://github.com/supernovae-st/nika/commit/3a54b80d46040ec54572f5e744526a7206cb82f4))
- **nika-types** — Kill from_unix_ms + unix_us surviving mutants ([ec479108d](https://github.com/supernovae-st/nika/commit/ec479108d933705e8130258f80ce9a78b34efd03))
- **nika-types** — Kill 3 baggage.rs mutation survivors (Gate-5 gap) ([ea66e301d](https://github.com/supernovae-st/nika/commit/ea66e301d607417d7059f26e5a4c9674dad6de85))
- **nika-types** — Close Gate-5 — kill 24 mutation survivors across 5 files ([0b03a0569](https://github.com/supernovae-st/nika/commit/0b03a0569652c4b58ab21bf15ad976f07fa7fda5))
- **nika-types** — Add tab/CRLF host-extraction bypass vectors ([3cd1a346d](https://github.com/supernovae-st/nika/commit/3cd1a346da8ff02f6f2272660db60a6a8765e028))
- **nika-verb-exec** — Pin stderr-tail boundary walk; note equivalent mutant ([7b0d7477f](https://github.com/supernovae-st/nika/commit/7b0d7477f0f1ffd0cb9a5cd62b6b4491cf464dc1))
- **nika-verb-infer** — Gate 10 parity — request shaping pinned vs brouillon ([0f2f5126a](https://github.com/supernovae-st/nika/commit/0f2f5126ae17b793f74df263dfd98a81b5e1d592))
- **nika-verb-infer** — Pin render_schema cap boundaries — mutants 8/8 killed ([db070e4b3](https://github.com/supernovae-st/nika/commit/db070e4b359ad7a71bf7faff86931ebe6e7aff0f))
- **nika-verb-invoke** — Pin the control-char byte rule exactly ([42acd7826](https://github.com/supernovae-st/nika/commit/42acd782688ab6837cb06f901176464a6607b313))

### 📦 Build
- **release** — Cross-platform binary pipeline + homebrew formula bump ([a77153ae3](https://github.com/supernovae-st/nika/commit/a77153ae300fa232138af69f1c1ca88054b75be3))
- Align prod-scoped size/unwrap checks with the tests.rs convention ([4c4113181](https://github.com/supernovae-st/nika/commit/4c411318101d8d8078e4c13f153267935cf500c0))

### 🧹 Chore
- **ci** — Allowlist proptest .expect() in cfg(all(test, ...)) modules ([4c22a5c17](https://github.com/supernovae-st/nika/commit/4c22a5c175be2116e365707bd02643e621cbb7fe))
- **ci** — Add nika-kernel to tokio deny wrappers (dev-dep) ([edb7283a9](https://github.com/supernovae-st/nika/commit/edb7283a9fce8777cd8f6905b8164f3e464c2269))
- **ci** — Wire cargo-public-api snapshots (P0 — Gate 12 enforcement) ([3edfc6fa0](https://github.com/supernovae-st/nika/commit/3edfc6fa0c2e24c3644455e2869d7778bbdeee68))
- **ci** — Floor nika-exec-runner public-api + re-sync status block ([499133308](https://github.com/supernovae-st/nika/commit/4991333080d224c199c67550ed5d695352ea83e9))
- **ci** — Wire s8.5 into the gate scripts — first L1.5 crate ([4f1a9fb8d](https://github.com/supernovae-st/nika/commit/4f1a9fb8d11c68e4816698516fd405af51b83b1d))
- **ci** — Floor nika-pack public-api baseline — ratchet 23/25 ([51896b145](https://github.com/supernovae-st/nika/commit/51896b145b39f8eaedb0f88bed46fcd37259ca53))
- **ci** — Floor the 3 L2 verb-crate public-api baselines — ratchet 26/28 ([4d56b8b5b](https://github.com/supernovae-st/nika/commit/4d56b8b5bb663461ef9bc302d8ea057a4185b110))
- **ci** — Retire the first_balanced_span allowlist entry — parser fixed ([fdd5ea3f0](https://github.com/supernovae-st/nika/commit/fdd5ea3f0562e3b4506988bc8fde05fa54cdcd3a))
- **ci** — Deny wrappers — tower-http + nika-infer-local ([5e52bfce4](https://github.com/supernovae-st/nika/commit/5e52bfce442ee12579789b7d3a7c593bcda99410))
- **ci** — Unblock the train — block re-splice + infer-local exemption ([ec602f000](https://github.com/supernovae-st/nika/commit/ec602f00021b72222f93b7c092c0a20c4305d13e))
- **ci** — Green the shared push train — extract gate rows + anchors ([870b0eb3c](https://github.com/supernovae-st/nika/commit/870b0eb3c9d09a1fd8deb37e4a5278e9a45b7841))
- **ci** — Fix-forward the push train — schema clippy + adr-093 frontmatter ([d7367c242](https://github.com/supernovae-st/nika/commit/d7367c242e0a07485638f4826f0244d5dd7d6ad7))
- **claude** — Harden CC hooks — A2 + P1-10 + P1-11 ([a7311be98](https://github.com/supernovae-st/nika/commit/a7311be9817b984b4c2d8881a2c8ed853f7ff5af))
- **crates-io** — Publish=false on 7 foundation crates (Phase B.0) ([a4ed8c309](https://github.com/supernovae-st/nika/commit/a4ed8c3092fa0222cc66646c75c83c2e400df59c))
- **deny** — Scope tokio wrapper to nika-clock · l1 time effect ([b529d8759](https://github.com/supernovae-st/nika/commit/b529d8759276aef6653af659e24617d618d021b6))
- **deny** — Tokio ban-wrappers follow the kernel split ([d1b9bffa7](https://github.com/supernovae-st/nika/commit/d1b9bffa7de1aeb3989a5d378d123a99fa0164fd))
- **deny** — Allow the chromiumoxide transitive stack through l1 wrappers ([d1df5dd18](https://github.com/supernovae-st/nika/commit/d1df5dd18f02033afd61e4b6a514eea3d3ecddf8))
- **diamond** — Pre-w3 stabilization · 5 fixes ([dd2ec28e5](https://github.com/supernovae-st/nika/commit/dd2ec28e5831bc4cb66e95f5d92cb4f5409a074c))
- **dx** — Add miri + cargo-hack ci jobs + activate tokio layer bans ([7beb24dcb](https://github.com/supernovae-st/nika/commit/7beb24dcb1290a1f83c864d990840db03f5a01b4))
- **dx** — Add machete + semver-checks + typos CI + fix unused deps ([31128e9a2](https://github.com/supernovae-st/nika/commit/31128e9a209cf4204a4b7b59d2cf05d85ab33f1d))
- **dx** — Wire gitnexus session-status + auto-reindex hooks ([f7479cb08](https://github.com/supernovae-st/nika/commit/f7479cb0831dc3df7eff8dadf30bb70c650be222))
- **dx** — Add cliff.toml v2 for changelog automation (A1) ([a99abb9bd](https://github.com/supernovae-st/nika/commit/a99abb9bdc8c5d1f9e48b366d5f5e34be0b5dd77))
- **dx** — Decommission gitnexus — spn-insight replaces it (S8-S9) ([fd7b9f672](https://github.com/supernovae-st/nika/commit/fd7b9f6728c9bb8e1bd22d71ca5d4b7cf4b42ce0))
- **dx** — Remove scripts/gitnexus/ + clean remaining refs ([5d4a49a5f](https://github.com/supernovae-st/nika/commit/5d4a49a5fa79e7e5e4acfbb90624fa712ea30134))
- **dx** — Drop legacy tools/ fallback in SessionStart hook ([1ee7b31dd](https://github.com/supernovae-st/nika/commit/1ee7b31dd5087b56e9cece235fc88baf54e869f5))
- **hooks** — Add prepare-commit-msg auto-inject Nika trailer (A3) ([73ab5ff8f](https://github.com/supernovae-st/nika/commit/73ab5ff8f464d47546c54435f6870df5cc2d60c1))
- **hooks** — Register screen scope for nika-screen m2.1 ([be8cc6749](https://github.com/supernovae-st/nika/commit/be8cc6749424ce307776cd827c0b2dd76f8d27d9))
- **hygiene** — Install lefthook pipeline + hook scripts ([320563165](https://github.com/supernovae-st/nika/commit/3205631651ebc0bc4016e25d37e59ff26714673c))
- **hygiene** — Add layering enforcement + fix stale paths → 21/21 green ([27eca5cf6](https://github.com/supernovae-st/nika/commit/27eca5cf6c2fdcbfa8d30c050e5563d1079bc402))
- **hygiene** — Unblock push — fix 4 REDs from accumulated debt (Phase B.0) ([8e18475e2](https://github.com/supernovae-st/nika/commit/8e18475e2cc7febf96ae4b5dd577444630df4fd5))
- **hygiene** — Vector 27 grows the box-dyn-ok exemption marker ([bd013f8b3](https://github.com/supernovae-st/nika/commit/bd013f8b3273a01e21ac3129492efb077dd30a7b))
- **hygiene+claude** — Wire vector 25 into dashboard + cargo-yank/publish guard (K2) ([7a8b2f9fd](https://github.com/supernovae-st/nika/commit/7a8b2f9fd237defdd99a852743b619cf1970c7e2))
- **mintlify** — Status snapshot infra + purge 8 dead pages ([c5225d5b8](https://github.com/supernovae-st/nika/commit/c5225d5b821b32b811cd9a3e1c527fa15228b30b))
- **nika** — Refresh auto-state block — HEAD 3a03558d5 · 2524 lib tests ([42e8cd1bc](https://github.com/supernovae-st/nika/commit/42e8cd1bc28d1cb32c1f55755ff068ad3cc80930))
- **nika-builtin** — Drop the seed template's dead deps — machete gate ([ff2b5c59f](https://github.com/supernovae-st/nika/commit/ff2b5c59f1244caa63500c14e466d5b334b95188))
- **nika-catalog** — Mark supports_vision deprecated — session 3 decommission ([bcb2adc36](https://github.com/supernovae-st/nika/commit/bcb2adc362942e0ddb7e03c24f3eab595eee1631))
- **nika-catalog** — Machete ignore for build-dep generator ([ac3eb0263](https://github.com/supernovae-st/nika/commit/ac3eb02630e0f902d22abcc497fc1e0485c7eaf3))
- **nika-event** — Regenerate the public-api baseline — vocabulary cohort ([a716f292c](https://github.com/supernovae-st/nika/commit/a716f292cc9ea0e5766821d2719a3f56022aae8e))
- **nika-infer-local** — Converge tokenizers on candle's 0.22 — one copy ([c6ce23dc5](https://github.com/supernovae-st/nika/commit/c6ce23dc55f85646b92c1cef0ae76a80965fb538))
- **nika-pack** — Re-sync pack — spec divergence-audit hardening ([#114](https://github.com/supernovae-st/nika/issues/114)) ([24a167bda](https://github.com/supernovae-st/nika/commit/24a167bda92885a6710022e270dd2c54a25bbba9)) ([#114](https://github.com/supernovae-st/nika/pull/114))
- **nika-pack** — Re-sync pack — rounds 2+3 (spec 6c18927) ([#116](https://github.com/supernovae-st/nika/issues/116)) ([923ec04d4](https://github.com/supernovae-st/nika/commit/923ec04d4074c75a42e54eb6cc60c6ab8c9317f9)) ([#116](https://github.com/supernovae-st/nika/pull/116))
- **nika-pack** — Re-sync pack — argv exec, CEL expansion, permits, registry 30 ([#119](https://github.com/supernovae-st/nika/issues/119)) ([cbf0bbb50](https://github.com/supernovae-st/nika/commit/cbf0bbb50ad490438a15ac585abed172658f8511)) ([#119](https://github.com/supernovae-st/nika/pull/119))
- **nika-pack** — Re-sync pack — quickstart one-voice posture ([#120](https://github.com/supernovae-st/nika/issues/120)) ([688bc8357](https://github.com/supernovae-st/nika/commit/688bc83573a21cd1534b1e01e7121c13941439e0)) ([#120](https://github.com/supernovae-st/nika/pull/120))
- **nika-pack** — Re-sync extract-modes — jq one-output law · metadata-links unphantomed ([4c7de7da6](https://github.com/supernovae-st/nika/commit/4c7de7da6b28bd2cfdd7e258a9d15dc5bc4644bd))
- **nika-pack** — Re-sync 08-out-of-scope — H23 cursor-pagination posture ([0c23589ab](https://github.com/supernovae-st/nika/commit/0c23589ab106191940f00977e42f0f5daceb06b8))
- **nika-pack** — Re-sync builtins-v0.1 — notify data: field ([6b7d2eef7](https://github.com/supernovae-st/nika/commit/6b7d2eef75ced4f1859bbd3528d038a1147ca9cb))
- **nika-pack** — Re-vendor 14 clean SSOT pack files to embed ([807abedc7](https://github.com/supernovae-st/nika/commit/807abedc76d36b7e1c086908dd90bbaee0204ff9))
- **nika-pack** — Re-vendor embedded pack (egress reconcile + F-01 + F-03) ([7bdf1390d](https://github.com/supernovae-st/nika/commit/7bdf1390d3a45c7211a296d222f939dff613a4f1))
- **nika-schema** — Exclude analysis.rs graph divergers from cargo-mutants ([d2f05b970](https://github.com/supernovae-st/nika/commit/d2f05b97015ed7b06b3aba60d324f6263fbc935e))
- **nika-types** — Refresh stale public-api baseline ([78fe6a1ed](https://github.com/supernovae-st/nika/commit/78fe6a1edeae8115c871a1471261b61e058f3e08))
- **rename** — Pre-rename cascade · nika-diamond → main · main → brouillon ([94ebc0954](https://github.com/supernovae-st/nika/commit/94ebc0954593c9838932cca360a1474a5f9dfe94))
- **rename** — Post-rename cleanup · branch refs + rustls advisory ([ee74d97e0](https://github.com/supernovae-st/nika/commit/ee74d97e043633845ccfbeacce58ca4e96b5ab27))
- **state** — Sync status docs to 37/42 after admission + origin rebase ([9bdbd70e7](https://github.com/supernovae-st/nika/commit/9bdbd70e711b407994b7dc4eb8cfc6b5d25328d6))
- **status** — Regenerate baseline from single source of truth (Phase A) ([cd9602ca0](https://github.com/supernovae-st/nika/commit/cd9602ca0854510b86d4add8ac38335df711d8de))
- **status** — Refresh canonical block post-merge (20 crates, 1334 tests) ([c4fdab692](https://github.com/supernovae-st/nika/commit/c4fdab692fa9dfacb527cb530d08707600bd08e7))
- **status** — Re-sync canonical block after main merge ([4cd3fed39](https://github.com/supernovae-st/nika/commit/4cd3fed39b9c796ef80ecee1222a8a5fda7f7dfd))
- **status** — Refresh auto-block — 31 crates · 3 WIP · L4=2 · 1814 lib tests ([04301ca88](https://github.com/supernovae-st/nika/commit/04301ca88607de12273bc69d9010c0b7e5e93a48))
- **status** — Refresh auto-block — 32 crates · 29/42 admitted · L2=4 (all verbs) · 1860 lib tests ([454a764db](https://github.com/supernovae-st/nika/commit/454a764db643ce4191579fa40217c4f457177a4d))
- **status** — Refresh auto-block — 33 crates · 29/42 admitted · L1.5=3 · 1915 lib tests ([2baaaa22d](https://github.com/supernovae-st/nika/commit/2baaaa22dbe774f8087926e3fabdf1a991d4ba4a))
- **status** — Refresh auto-block — 34 crates · 2000 lib tests · L1.5=4 ([d768d0120](https://github.com/supernovae-st/nika/commit/d768d012040df441b44d6d472ec228a1e293d081))
- **workspace** — Pin zeroize=1.8 + nika-error mutation report (H8) ([47c2284cb](https://github.com/supernovae-st/nika/commit/47c2284cb8e3ce319c984f6e0685f43e85425db4))
- **workspace** — Refresh 4 stale public-api baselines ([7dd8fd081](https://github.com/supernovae-st/nika/commit/7dd8fd081d13cbd831eaabd42d0c14e5c308637b))
- **workspace** — Format the nika-compose commit output ([c3739147a](https://github.com/supernovae-st/nika/commit/c3739147aacbbedffd80c1f6a92cc3554b9ec56b))
- **workspace** — Clear pre-push hygiene RED (spec LOC, doc link, error-voice) ([1944737f4](https://github.com/supernovae-st/nika/commit/1944737f4c9949e8b2b55288c2091caae9aeb638))
- Re-version engine 0.80.0 → 0.90.0 + propagate versioning docs ([2a596209a](https://github.com/supernovae-st/nika/commit/2a596209a10d448f218de5c812b5e862d1aef65e))

### 💼 Other
- Merge remote-tracking branch 'origin/main' into feat/s4-nika-fs

# Conflicts:
#	.claude/CLAUDE.md
#	AGENTS.md
#	ROADMAP.md ([f76e397cd](https://github.com/supernovae-st/nika/commit/f76e397cd314e0fe597058bb58447fceed50a557))
- Merge remote-tracking branch 'origin/main' into feat/s4-nika-fs ([a8e734f02](https://github.com/supernovae-st/nika/commit/a8e734f02d26062d76a2134132d0ddd940569c43))
- Merge remote-tracking branch 'origin/main' into feat/s4-nika-fs ([4816e636b](https://github.com/supernovae-st/nika/commit/4816e636b77d134da7425c15f5e18ff846467f5d))
- Merge remote-tracking branch 'origin/main' into feat/s4-nika-fs ([d6bb9fd7e](https://github.com/supernovae-st/nika/commit/d6bb9fd7ee0598caf9aa84ac171c70d41c7762f7))
- Merge remote-tracking branch 'origin/main' into feat/s4-nika-fs ([2d5a3301b](https://github.com/supernovae-st/nika/commit/2d5a3301b8ea7de524bc2c76c9c4ab32285410e5))
- Merge remote-tracking branch 'origin/main' into feat/s4-nika-fs ([b1f81aafe](https://github.com/supernovae-st/nika/commit/b1f81aafef81cbca4319b1b4c01c3e4d36bac352))
- Merge remote-tracking branch 'origin/main' into feat/s4-nika-fs ([98d60e66c](https://github.com/supernovae-st/nika/commit/98d60e66ca8d4210527c5beaa57666408ce15d56))
- Merge remote-tracking branch 'origin/main' into feat/s4-nika-fs ([3421467c0](https://github.com/supernovae-st/nika/commit/3421467c020a29499dd1777c093e41648e70e8c8))
- Merge remote-tracking branch 'origin/main' into feat/s4-nika-fs ([c78d73a4a](https://github.com/supernovae-st/nika/commit/c78d73a4a95040d9b79ce9928d5a503576b731d3))
- Merge remote-tracking branch 'origin/main' into feat/s4-nika-fs ([47cb6a763](https://github.com/supernovae-st/nika/commit/47cb6a763160dba69a11414c3aff393f85ef66fc))
- Merge remote-tracking branch 'origin/main' into feat/s4-nika-fs ([6250721fd](https://github.com/supernovae-st/nika/commit/6250721fd51d83462b250f354dbf9840236b573a))
- Merge remote-tracking branch 'origin/main' into feat/s4-nika-fs ([d7636a08d](https://github.com/supernovae-st/nika/commit/d7636a08db9d3117f1591b732fee73ddaf7dc3de))
- Merge branch 'feat/s4-nika-fs' — canon purge + hygiene vector 41

Brings the docs-canon truth pass (live docs state current canon only ·
crate tree mirrors the layer registry · gate12 connectome names + verb
range truth) and scripts/hygiene/check-canon-stale-terms.sh (vector 41:
dead names cannot return).
 ([6dbca5baa](https://github.com/supernovae-st/nika/commit/6dbca5baaa09de0a005172e6422471d9ea217a6b))
- Merge branch 'feat/permits-fit-analyzer' — builtin arg-shapes ([#123](https://github.com/supernovae-st/nika/issues/123))

Brings the analyzer builtin arg-shape pass: four ledger rows close and
the lints corpus moves to the spec repo (companion nika-spec c9233c9,
already on its main).
 ([20aea2143](https://github.com/supernovae-st/nika/commit/20aea21432891cfe39c575ecf4a2402bc1036743))

### 🏁 Both WIP crates ADMITTED — the engine wip array is EMPTY (39/42 · 2026-06-21)

- **`nika-cli` crate** admitted (L4 · the operator surface · the `nika` verb
  tree: check · run · trace · inspect · graph · explain · spec · schema ·
  examples · new · doctor · pack · completions · lsp · mcp). New this admission:
  the spec §3.5 reduced surfaces — `--no-progress` (plain · CI default),
  `--quiet` (the compact verdict card), `--dry-run` (plan only · zero effects) —
  via a 3-mode `RenderMode` over a shared, drift-free failure-card render.
  - **Gate 5** mutation 91% (264/290) · **Gate 6** the fold property battery
    (`tests/fold_property.rs` — cost-conservation · one-row-per-task ·
    permutation-invariance · sequential ≡ interleaved-wave).
  - **Review swarm** caught + fixed a real P1: `--dry-run --output json` had
    corrupted the clean-JSON lane → the human flags now `conflicts_with_all`
    the machine modes (clap).
- **`nika-extract` crate** admitted (L1.5 · the 9 fetch extract modes behind
  the `nika:fetch` extract step — article Trafilatura cascade · feed · sitemap ·
  metadata + schema.org microdata · blocks · zones · page-type · links).
  - **Gate 5** mutation 79.7% → 93.2% (~50 boundary tests killed 73/81
    survivors in the heuristic functions) · **Gate 6** totality over all 9 modes.
  - **Review swarm** (3 agents): the adversarial refuter SURVIVED (totality +
    DoS-bounding hold); fixed og:video/audio absolutization + host-only search
    URLs; added a per-item microdata property cap (defense-in-depth). One agent
    finding was **rejected** verify-before-fix (`<a itemprop=url>` with no href
    → `""` is W3C-correct, not a text fallback).

### 🚚 Release engineering — cross-platform binary pipeline (2026-06-21)

- **`.github/workflows/release.yml`** — on a `vX.Y.Z` tag, builds the four
  `nika` binaries (macOS arm64/x64 · Linux arm64/x64), cuts the GitHub release
  with the exact tarballs the Homebrew formula points at, and (with a
  `TAP_GITHUB_TOKEN` secret) bumps the tap formula. Fires only on a tag —
  nothing publishes until you tag. `scripts/release/update-formula.sh` does the
  version + sha256 rewrite (runnable by hand too). Unblocks the Homebrew path
  that had no pipeline.

### 🏛️ nika-schema L0 admission — parser + analyzer + static-check (ADMITTED · all 12 gates · 2026-06-18)

- **`nika-schema` crate** admitted — the workflow AST, parser, analyzer, and
  the ADR-092 `nika check` static-check ladder (the last L0 WIP crate).
- **Gate 5** closed in BUDGET mode (`survivors ≤ 300`): 269 timeout-divergers
  + 21 enumerated equivalents, each scoped-re-verified. Rounds 1-7 (~190 tests)
  killed the floor's real-gap tail — analyzer/check collection + lint logic,
  the `read_dag` cap/pinch boundaries, the default-gate runnable path, and the
  expression-parser offset/depth/byte-scanner.
- **Security**: two complementary `when:`-gate DoS fixes integrated — a
  `MAX_GATE_LIST_ITEMS` cap on the leaf-evaluation re-scan and a `BTreeSet`
  dedup in `collect_bad_literals` (an O(n²) `Vec::contains` scan that burned
  ~3 s of CPU on a 2-task workflow before the fix). Plus 7 `#[non_exhaustive]`
  source types (FCI-002) and a parse+check criterion benchmark (Gate 7 · parse
  10-task 30.9 µs).

### 🧩 Announce ladder s19.6 · nika-lsp L4 admission — the `nika lsp` language server (ADMITTED · 12-gate closed · 2026-06-15)

- **`nika-lsp` crate** · the Nika language server (`nika lsp`, stdio) — the
  v0.1 editor brain for `.nika.yaml`. ONE crate (nika-lsp-core collapsed in as
  internal `analysis::*` modules · per `nika-invariants` + collapse-vs-publish ·
  reconciles `D-2026-06-10-N6` steps 19.6/19.7). Stack: `lsp-server` 0.7 sync
  stdio loop + `lsp-types` 0.97 · pure analysis over `nika-schema`.
- **Diagnostics** reuse the SAME ADR-092 `nika check` ladder (one source of
  truth · task-anchored ranges) · **hover** on the 4 verbs + keywords AND on a
  task reference (`depends_on` item / `${{ tasks.X }}` → the target task's id +
  verb) · **completion** (keys · verbs · `model:` providers · the workflow's own
  task ids · auto-trigger on `.` `/` `[`) · **document symbols** ·
  **go-to-definition** for task refs.
- Feeds the `nika-vscode` extension, auto-detected via `caps.lsp` once
  `nika --help` lists `lsp` — zero extension change. 124 lib tests · mutation
  96.9% · the `nika lsp` subcommand wired into `nika-cli` (owns stdout · LSP
  exit-code convention).

### 🤖 Announce ladder s12 · nika-verb-agent L2 admission (ADMITTED · 12-gate closed · 2026-06-11)

- **`nika-verb-agent` crate** · the `agent` verb executor — the multi-turn
  ReAct loop (model → whitelisted tool dispatch → results fed back → repeat)
  per `nika-spec spec/02-verbs.md §agent`. The **4th and last verb**
  (`D-2026-05-22-N18` · the verb count is 4, absolute). Generic over three
  injected kernel seams: `ProviderInferDyn` (inference) · `ToolExecuteDyn`
  via `InvokeVerb` (dispatch) · `ToolDefinitionProviderDyn` (the tool-def
  source). Zero runtime tokio dep — every turn rides the injected providers.
- **The ToolDefinitionProvider seam** (`nika-kernel-ai`) · resolves the s12
  §8 blocker found 2026-06-11: the agent hands the model its whitelisted
  tools as `ToolDef`s, but only tool NAMES were in hand — nothing enumerated
  definitions. A new kernel trait (the `ToolExecute` pattern · `Dyn` twin ·
  `ToolDefsError` → NIKA-234) + `MockToolDefinitionProvider`. The wiring
  layer implements it over the builtin catalog + (later) live MCP
  `tools/list`.
- **Loop semantics (normative · spec §2)** · terminal-1 (no tool calls →
  `Completed`) and terminal-2 (`nika:done` → `ExplicitCompletion`, with the
  `result:` arg or the last assistant message) BOTH precede the budget gate
  — a concluded answer is a success even if its turn crossed the budget.
  Budgets FAIL (max_turns → NIKA-460 · max_tokens_total → NIKA-461, `>=`
  exhaustion, checked before spending more) with `partial_output` preserved.
- **Security (spec §3 · default-deny)** · the whole tool batch is whitelist-
  validated BEFORE any dispatch (a denied sibling fails the turn with zero
  side effects · NIKA-462 immediate, not fed back). `nika:done` is loop-owned
  (never dispatched · wins over batch-mates). Model-emitted names are length-
  capped + control-char-rejected, and the violation error carries a REDACTED
  name (NIKA-450 log-injection parity). Source-supplied tool defs are
  sanitized before reaching the model.
- **The glob whitelist** · gitignore semantics canonically (a spec
  portability invariant): `*` bounded by `/` and `:`, `**` crosses them,
  `!` negation, last-match-wins. Matched by an O(n·m) DP (correct under
  interleaved `*`/`**`) + a totality proptest on the model-controlled input.
- **Structured output** · the final message validates against the task
  `schema:` (NIKA-464) with `infer.schema:` parity — bare-parse then a
  string-aware balanced-span extraction (tolerates fences + prose).
- **3-lens review swarm** (spn-nika + rust-pro + feature-dev) · all findings
  folded same session: the budget-before-completion bug, the batch-validate
  security ordering, the `**`/`*` glob backtrack gap, log-injection
  redaction, saturating token math, INV-019 `AgentOutput::new()`, the
  max_turns ceiling. NIKA_460..466 registered · hub 460-469 row · API-locked.

### 📡 Telemetry vocabulary closes over the display contract (nika-event · additive)

- **6 new `EventKind`s** · `task_retrying` · `task_cancelled` ·
  `workflow_cancelled` · `cost_incurred` · `infer_chunk` ·
  `permit_checked` — every state the run UI can show (contract §3.1
  state machine) and every live-meter refold driver the contract names
  (§3.3) is now expressible by a canonical engine event. Cancellation is
  terminal-not-failure (a decision, not a defect). `permit_checked`
  makes the declared `permits:` boundary observable at runtime (the
  ADR-092 audit moat).
- **`EventClass`** · the coarse 7-class classifier (`EventKind::class()`)
  — renderers/routers branch on stable classes, not 17 variants.
- **Reference fold** · the `nika-schema` `verbs` example consumes the
  full vocabulary: `--events` renders the whole tape digestibly; `verbs
  workflow` folds the SAME tape into the animated DAG (retry arc ↻ ·
  live stream · ticking cost meter · permits counter). The state-machine
  coverage test pins « every UI row status is event-reachable ».

### 🔌 Announce ladder s11 · nika-verb-invoke L2 admission (ADMITTED · 12-gate closed · 2026-06-11)

- **`nika-verb-invoke` crate** · the `invoke` verb executor per
  `nika-spec spec/02-verbs.md §invoke` (third of the 4 verbs). Rides the
  kernel `ToolExecuteDyn` seam with the engine's builtin+MCP dispatcher
  injected — zero tool implementation of its own, zero Cargo dep on
  `nika-builtin`/`nika-mcp`.
- **The closed-namespace contract** · the tool-ref namespace set is CLOSED
  at v1 (`nika:` · `mcp:` only · `mcp:` requires the `server/tool` slash);
  the verb does the lightweight semantic check before dispatch (grammar
  SHAPE stays the upstream `nika-schema` `NIKA-PARSE` concern). Result
  mapping: `is_error: true` → NIKA-451, dispatcher `NotFound` →
  `UnresolvableTool`, other dispatch failures → NIKA-452.
- **Security guards (swarm)** · whitespace padding and ASCII control chars
  in the tool id are rejected before it reaches a `ToolCall`/log field
  (log-injection class); the derived fallback `call_id` appends a
  process-monotonic counter so repeated same-tool invokes don't collide on
  the kernel's unique-call-id contract.
- **Error one-voice** · NIKA-450..452 registered in the Verb range; the
  verb-range help moved into a `verb_help` helper (keeps `code_help` under
  the 100-line cap).
- 16 lib tests (1 totality proptest cross-checked against an independent
  predicate) · mutation all viable killed bar one documented equivalent ·
  clippy 0 · doc 0 · layering + deny green · tag `v0.80.0-alpha.7`.

### ⚙️ Announce ladder s10 · nika-verb-exec L2 admission (ADMITTED · 12-gate closed · 2026-06-11)

- **`nika-verb-exec` crate** · the `exec` verb executor per
  `nika-spec spec/02-verbs.md §exec` (second of the 4 verbs). Rides the
  kernel `ShellRunDyn` seam with the effect injected (`TokioShell` in prod ·
  `MockShell` in tests) — zero subprocess code of its own, zero Cargo dep on
  `nika-exec-runner` (the L2→L1 inversion through the kernel trait).
  `pre_validated` is NEVER set, so the s7 runner blocklist stays the floor
  (structurally pinned by test).
- **The capture one-obvious-way split** · default modes (`stdout` · `stderr`
  · `combined`) fail the task on a non-zero exit (NIKA-440 / spec
  NIKA-EXEC-001 · with a capped stderr tail); `capture: structured` returns
  `{ stdout, stderr, exit_code }` as DATA — the workflow branches on it, the
  task succeeds.
- **Verb-boundary input guards (NIKA-442)** · a NUL byte in command/stdin
  (silent shell truncation) and a malformed env key (`=` · NUL · empty ·
  child-env corruption) are refused before the runner call — the security
  swarm's two findings.
- **Error one-voice** · NIKA-440..442 registered in the Verb range ·
  `MockShell` aligned to the Send-variant traits + gained `enqueue_result`.
- 19 lib tests (3 proptests · Gate 10 parity vs brouillon) · mutation all
  viable killed bar one documented equivalent · clippy 0 · doc 0 · layering
  + deny green · tag `v0.80.0-alpha.6`.

### 🗣️ Announce ladder s9 · nika-verb-infer L2 admission (ADMITTED · 12-gate closed · 2026-06-11)

- **`nika-verb-infer` crate** · FIRST L2 verb crate — the `infer` verb executor
  per `nika-spec spec/02-verbs.md §infer` (one of the 4 verbs locked forever ·
  D-2026-05-22-N18). Resolves `model: provider/name` through the s8.5
  `nika-providers` registry (D-N17: providers live BELOW the verbs · no
  verb→verb sideways dep), shapes the kernel `InferRequest`, returns the full
  `InferResponse` for the future L3 engine's event/cost seam.
- **Structured-output floor in-crate** · `schema:` tasks get native
  `ResponseFormat::JsonSchema` when the profile supports it (instruction
  fallback otherwise), lenient JSON extraction (bare → fenced → first balanced
  string-aware span), `jsonschema` 0.33 validation (compiled ONCE per run —
  an uncompilable schema is NIKA-432 with zero paid round-trips), and a
  bounded validation retry (default 2 · spec-sanctioned before NIKA-INFER-002).
  Schema text re-injected into prompts is capped at 4096 chars.
- **Error one-voice** · `VerbInferError` speaks `NikaErrorCode` via the new
  registry-owned NIKA-430..433 (Verb range 430-479 opened · same pattern as
  the M2 computer-use ranges) · transience inherited from `ProviderError`,
  never overridden.
- **Gate 11 swarm (3 lenses · 0 P0)** folded same-session: compile-once
  validator · u8→u32 attempts counter (closes the u8::MAX budget saturation
  loop) · schema render cap · both transience branches pinned.
- 33 lib tests (3 proptests · Gate 10 parity vs brouillon shaping pinned) ·
  mutation 95.8% overall + 8/8 on the cap helpers · clippy 0 · doc 0 ·
  layering + deny bans green. New workspace dep `jsonschema` (default-features
  off · no network resolver).

### ♿ Phase 2 M2.3 · nika-a11y L1 admission (ADMITTED · 12-gate closed · 2026-05-25)

- **`nika-a11y` crate** · third computer-use L1 effect crate · implements the
  L0.5 `io::a11y::AccessibilityTree` trait (`snapshot` + `find` + `resolve_ref`)
  exposing the active window's accessibility tree as `AxNode` records. **macOS-first**
  (decision §4 of `docs/crate-specs/nika-a11y.md`): backend via the safe
  **`accessibility` 0.2** crate (`AXUIElement` · `TreeWalker` · the unsafe
  `ApplicationServices` FFI is encapsulated → crate stays `unsafe_code = forbid`);
  Linux `atspi` / Windows `uiautomation` deferred to a consumer signal (LOCK-031).
  B.1 spec (backend research: 3 vetted permissive crates verified on crates.io)
  → B.2 skeleton (`A11yError` NIKA-1200..1206 · `AxBackend` · `snapshot`/`find`/
  `resolve_ref` route through a `walk_tree` placeholder returning `BackendNotWired`).
- **ADR-081 Guard 3 (AX-secure-field redaction · MANDATORY-at-admission) is
  headless-complete at B.2** · a pure recursive tree-transform (`redact_secure_fields`
  / `is_secure_field`) strips `value` from any secure-text node (macOS
  `AXSecureTextField` subrole · AT-SPI `STATE_SENSITIVE`) to `None` (zero leak),
  applied before any node leaves the crate. The pure `find` filter
  (`matches_query` + depth-bounded `collect_matches`) ships too. 12 lib tests
  (incl. a proptest pinning the redaction invariant) · clippy 0 · doc 0 ·
  `cargo-machete` clean · `cargo deny` ok. `nika-a11y` added to `deny.toml`
  tokio wrapper allowlist. API primary-source verified via context7
  (`/eiz/accessibility`) before recommending the backend.
- **B.3 macOS `AXUIElement` walk wired** · `system_wide().focused_window()`
  rooted recursive `build_node` (role/label/value/subrole → `AxNode`) inside
  `spawn_blocking` (the `!Send` handle stays worker-local · CANCEL SAFETY) ·
  macOS-gated deps `accessibility` 0.2 + `core-foundation` 0.10 (CFString/CFType
  reads · all upstream symbols — `focused_window` · `value().downcast::<CFString>()`
  · `children().iter()` · `subrole()` — verified against the crate source before
  use). Non-macOS compiles to `BackendUnavailable` (NIKA-1205). `resolve_ref`
  backed by a `Mutex<Option<AxNode>>` cache of the last redacted snapshot + pure
  `find_by_id`. Pure `ax_role_from_str`. Closed the `BackendNotWired` placeholder
  (NIKA-1200 retired · slot reserved). `bbox` deferred (`None` · frame→`Rect`
  refinement).
- **B.4 12-gate close · ADMITTED** · extracted the pure `assemble_node` (role
  map + empty-title/subrole filter + `AxNode::new`) out of the FFI `build_node`
  to maximize headless coverage; added a `MAX_WALK_DEPTH` (512) recursion cap so
  an untrusted/deep/cyclic focused-app tree can't overflow the stack (caught by
  the Foreman-direct review). **Gate 5 mutation 34/41 viable caught (82.9 %)** ·
  100 % of the headless surface · 7 `AXUIElement`-walk mutants documented-exempt
  per ADR-003 Rule 2 (`docs/crate-specs/nika-a11y.md` §7.1). **Gate 11** ·
  sub-agents hit the 1M-context credit wall → Foreman-direct 3-lens review
  (PE-5.1 · rust-pro + Diamond + bug-hunt · all ADMIT). 14 lib tests + 1
  `#[ignore]` smoke · clippy 0 · doc 0 · machete clean · deny ok · workspace
  `--lib` 1170. Workspace 13/42 admitted · WIP nika-schema only.

### 🔤 Phase 2 M2.2 · nika-ocr L1 admission (ADMITTED · 12-gate closed · 2026-05-25)

- **`nika-ocr` crate** · second computer-use L1 effect crate · implements the
  L0.5 `io::ocr::OcrEngine` trait (`read` + `read_region`) via the pure-Rust
  **`ocrs` 0.12** engine (**`rten` 0.24** runtime · no C system dep · keeps
  `unsafe_code = forbid`). B.1 spec → B.2 skeleton (`OcrError` NIKA-1100..1109
  · pure frame/region validation · `BackendNotWired` placeholder) → B.3 real
  inference: `OcrBackend::with_models(detection, recognition)` eager-loads two
  `.rten` weight files from **explicit local paths** (sovereignty Rule 1 ·
  reads local files only · NEVER auto-downloads · models are operator/daemon-
  provisioned), `read`/`read_region` validate the RGBA8 `Frame` purely then run
  `prepare_input → detect_words → find_text_lines → recognize_text` inside
  `tokio::task::spawn_blocking` (the sync CPU-bound engine runs off the async
  runtime · kernel CANCEL SAFETY: a dropped future abandons the read with no
  side effects). The B.2 `BackendNotWired` placeholder is CLOSED (NIKA-1100
  retired · slot reserved) per `skeleton-option-a-pattern.md` §5.
- **`nika-ocr` 12-gate close (B.4)** · admitted — all 12 gates green
  (registry L1 · ADR-081 inherits 7-guard contract, owns none mandatory ·
  `#[non_exhaustive]` · zero-unwrap src · ~290 LOC · NIKA-1101..1109 ·
  cancel-safety · `test --workspace --lib` 1156 · clippy 0 · `cargo doc` 0 ·
  `cargo-machete` clean · `cargo deny` ok). **Gate 5 mutation 81/87 viable
  caught (93.1 %)** · 100 % of headless-reachable logic · 6 model-inference
  mutants documented-exempt per ADR-003 Rule 2 (need real `.rten` weights ·
  `docs/crate-specs/nika-ocr.md` §6.1). Pure helpers (`rgba_to_rgb` ·
  `crop_rgba` · `words_bbox_union` · `validate_frame` · `validate_region`)
  proptested + 100 % mutation-killed. **Gate 11 review** · sub-agents hit the
  1M-context credit wall → Foreman-direct 3-lens review per
  `orchestrator-autonomous-v6.md` PE-5.1 (rust-pro + Diamond-discipline +
  bug-hunt · all ADMIT · 1 P1 stale-module-doc fixed). Deps: `+ocrs +rten`
  (workspace) `+tokio` rt + `tempfile` dev · `nika-ocr` added to `deny.toml`
  tokio wrapper allowlist. API primary-source verified via context7
  (`/robertknight/ocrs`) before wiring · no phantom symbols.

### 🖥️ Phase 2 M2.1 · nika-screen L1 admission (ADMITTED · 12-gate closed · 2026-05-23)

- **`nika-kernel` `io::screen`** · NEW `capture_stream` additive trait method +
  `FrameStream` type alias (`Pin<Box<dyn Stream<Item = io::Result<Frame>> + Send>>`),
  the canonical kernel streaming idiom (cohérent `ai::provider::InferEventStream`).
  Zero breaking change · uses `futures-core` (NOT `tokio-stream`, which is
  L0.5 layer-banned per `Cargo.toml`). Begins the M2.1 6-batch dispatch (B.1).
- **`crate-layer-registry`** · `nika-screen` registered L1 — first computer-use
  effect crate (Gate 1). ADR-081 7-guard contract already shipped (`3e40c18b3`).
- **`nika-screen` crate** · B.2 skeleton (`ScreenError` NIKA-1000..1009 · 10 codes
  · `ScreenBackend` + consent/LED guard skeletons) → B.3 single-shot capture WIRED
  via `xcap` 0.9.5 (`list_displays` / `capture_full` / `capture_region` · sync OS
  calls wrapped in `spawn_blocking` so the `!Send` `Monitor` stays worker-local and
  dropped futures surrender promptly · zero-copy RGBA8 `Frame`) → B.4 wires
  `capture_stream` (bounded `tokio::mpsc` + dedicated capture thread · ~30fps
  cadence · drop-stop cancellation via channel-close · `futures_core::Stream`
  adapter over `poll_recv`). All 4 `ScreenCapture` methods now real — the B.2
  `BackendNotWired` skeleton is fully CLOSED. B.5 makes the ADR-081 guards real
  + ENFORCED · a fail-closed `ConsentGate` (guard 7 · in-memory · session-scoped
  · revocable · per-frame re-check inside the stream worker) gates every pixel
  capture, and a RAII `LedIndicator` (guard 6 · engaged-count) stays lit for the
  whole capture. xcap encapsulates the OS FFI
  (objc2 / x11 / windows) so the crate is `unsafe_code = forbid`-clean. Plan-dep
  correction · the
  plan's `nokhwa` is a WEBCAM lib (docs.rs verbatim); `xcap` is the screen-capture
  crate (per `cross-source-validation.md` §2.7).
- **`nika-screen` 12-gate close (B.6)** · admitted as the first L1 effect crate —
  all 12 gates green (registry · ADR-081 · `#[non_exhaustive]` · zero-unwrap ·
  LOC 943 · NIKA-1000..1009 · cancel-safety · `test --workspace --lib` 1125 ·
  clippy 0 · `cargo deny` ok · forward-compat). GAP-3 `From<ScreenPoint>` shim
  CARRIED FORWARD to M2.4 `nika-input` · `ScreenPoint` is a `cockpit_overlay`
  (Olympus) type, so a `From` impl in `nika-screen` would violate cross-flow
  D-2026-05-08-N1 (Nika→Olympus) and is an `io::input` (cursor) concern, not
  `io::screen`; the conversion lives on the Olympus consumer side (where
  `cockpit-input-injection` already mirrors it).

### ⚡ Perf profile + craft amendments (2026-05-12)

Pre-W3 perf-craft + architecture polish per 2-agent SOTA audit
(`spn-rust:rust-async-expert` + `spn-rust:rust-perf` parallel) ·

- **`Cargo.toml [profile.release]`** · `lto=fat` + `codegen-units=1` +
  `strip=symbols` + `panic=unwind` + `debug=line-tables-only` +
  `incremental=false` · matches ADR-061 SLSA L3 prep · ~5-10% perf
  delta on BGE-M3 cosine + BM25 + RRF hot paths · 2× build cost
  release only · dev unaffected.
- **`Cargo.toml [profile.bench]`** · inherits release + `debug=true`
  for `cargo flamegraph` + `perf annotate` at W3 admission Gate 7.
- **4 `const fn` promotions in `nika-types`** · `Cost::new` ·
  `Cost::zero` · `Cost::is_zero` · `Trust::new` · `Trust::is_at_least` ·
  unlocks `const SATELLITE_COST: Cost = Cost::from_milli_usd(5)` at
  call-sites = zero runtime eval. `From`-trait + `Option::map` blocked
  (not const-stable yet · 2027+ horizon · per Rust 1.91 limits).
  Forward-compat per ADR-007 · `pub fn → pub const fn` non-breaking.

### 📐 BLUEPRINT_2036 v1.3 amendments (2026-05-12)

Cumulative cascade v1.0 → v1.1 → v1.2 → v1.3 per `docs/architecture/
BLUEPRINT_2036.md` frontmatter · status proposal · annual decennial
review 2027-04+.

- **v1.1 (per-crate detail + best-enemies SOTA)** · 42-crate table
  with LOC + deps + trait + Gate-9 + admission target per row ·
  Restate/LangGraph/Temporal/Mem0/Letta differentiation matrix ·
  collapse-vs-publish principle § 1.5 locked
- **v1.2 (11/10 amplifiers + guardian framing)** · 9→4 amplifier ADR
  fold (saves 5 empty shells · `socratic-research-discipline.md`
  Step 5 Option D) · §4.7 anti-Palantir + AI-2027 trajectory mapping ·
  14 prior Nika-mappings re-validated 2026-Q2
- **v1.3 (perf craft + async depth · this entry)** · §4 RRF fairness ·
  Loom scope (2-thread minimal + Shuttle PCT for full DAG) ·
  `consume_budget` cooperative scheduling · `[profile.release]`
  mirror · §4.5 ADR-066 `#[tracing::instrument]` discipline · NEW
  ADR-070 (`TaskTracker` + child-token fan-out · kernel-pure preserved
  per ADR-016 Alt-A) · ADR-041 `#[track_caller]` builder amendment

### 📚 Pre-launch hygiene shipped (2026-05-12)

- **Per-crate READMEs** · 4 missing of 8 shipped (`nika-error` ·
  `nika-catalog` · `nika-kernel` · `nika-kernel-mock`) following
  tokio/serde/thiserror SOTA pattern (~80-120L each)
- **`CODE_OF_CONDUCT.md`** · Contributor Covenant v2.1 boilerplate ·
  conduct@supernovae.studio · 4-tier enforcement ladder
- **`SECURITY.md`** · vulnerability disclosure policy · 72h ack · 90d
  disclosure · 11-row NIKA-271..389 defense layers table
- **`Cargo.toml [workspace.lints.rustdoc]`** · compile-time doc gate
  (broken_intra_doc_links=deny · private=warn · invalid_codeblock=deny)
- **`.github/workflows/diamond-ci.yml`** · semver-checks baseline ·
  `origin/nika-diamond` (renamed branch · stale since 2026-05-06) →
  `origin/main` · was silently failing

### 📚 Wave 4E — Mintlify rebuild + docs repo split (2026-04-17)

End-user documentation split out to a dedicated public repository and
rebuilt from the current workspace state.

- **`supernovae-st/nika-docs`** — new public repo, serves
  [`docs.nika.sh`](https://docs.nika.sh) via Mintlify. Replaces the
  in-engine `docs/mintlify/` directory, which is removed from this
  repo. Engine-internal docs (`docs/adr/`, `docs/architecture/`,
  `docs/crate-specs/`) stay here.
- **Mintlify content refreshed** — 2-tab navigation (Guide / Reference),
  honest v0.80 pre-release framing, live snapshot of 32 providers, 49
  capability rules, 35 ADRs (11 thematic groups), L0 architecture
  decisions, admission 12-gate walkthrough.
- **Dead pages purged** — 8 Mintlify pages that no longer mapped to the
  Diamond workspace state removed pre-split.
- Cross-links from this repo's README + ROADMAP point to
  `docs.nika.sh` for end-user content.

### ⚡ Swarm-3 Batches I.b + II ε.2/ε.3 + Wave 3A + Wave 4A + 4B seeds + Wave 4C (2026-04-17)

**Hygiene — Batch I.b vectors 30-33 (+4 new):**

- **Vector 30 `check-cancel-safety.sh`** — every `async fn` in
  `crates/nika-kernel/src/**` now carries a `// CANCEL SAFETY:` or
  `/// CANCEL SAFETY:` marker. 43 kernel methods annotated
  (cancel-safe contract: drop semantics, atomic vs non-atomic writes,
  `kill_on_drop` requirement, billing/telemetry exposure).
- **Vector 31 `check-owned-strings.sh`** — preventive ratchet: bans
  non-static `&str` in nika-catalog `pub` fields / `pub fn` return
  types. Catalog stays 100% `&'static str` per ADR-008 codegen pragma.
- **Vector 32 `check-unsafe-count.sh`** — `unsafe` token counter
  vs `scripts/hygiene/baselines/unsafe-count.txt` (currently 0).
  Substitutes cargo-geiger which is hostile to virtual manifests.
- **Vector 33 `check-layer-deps.sh`** — per-layer banned third-party
  deps (`[workspace.metadata.diamond] layer-bans`). L0 rejects 17
  deps (tokio family, rayon, async-std, smol, futures family,
  reqwest, hyper, axum, actix-web); L0.5 rejects 11.
- **Killed vector 7** (linear-issue-states stub) **and vector 18**
  (adr-dangling duplicate of vector 16).

**Wave 3A — engine post-commit hook for Olympus snapshots:**

- `scripts/hooks/post-commit-olympus-xtask.sh` wired via lefthook.
  Background `pnpm tsx olympus/scripts/xtask.ts` regenerates
  workspace.json + snapshots + hygiene-status.json on every engine
  commit; Olympus live-refreshes `/timeline`, `/graph/diff`,
  `/graph/fitness`, `/hygiene`.

**Wave 4A — v0.95 Cortex + v0.100 WASM reservations (R1-R5):**

- **R1 `EmbeddingSpec`** (`nika-types::embedding`) — Dtype,
  DistanceMetric, EmbeddingSpec; `#[non_exhaustive]` + snake_case wire.
- **R2 `MemoryFrameRef.trust: TrustLevel`** — sticky ingest taint;
  `#[serde(default)]` → UNTRUSTED fail-safe.
- **R3 `RecallQuery.tenant: TenantId`** — mandatory multi-tenant
  keyspace scope. `TenantId::default_tenant()` → `"default"`.
- **R4 `WasmPluginError::OutOfFuel` + `Trap { kind: TrapKind }` +
  `PluginCallContext`** — fuel metering, W3C-style trap taxonomy,
  per-call context with trust + cancel + budget.
- **R5 `MemoryLifecycle` trait** with default-impl consolidate/prune
  returning empty reports. Standalone; Cortex opts in at v0.95.

**Wave 4B seeds (telemetry foundations):**

- **#1 `SpanGuard.parent_span_id` + `links: Vec<SpanRef>`** — W3C
  Trace Context parent linkage unblocks Olympus `/trace`. Default
  `TracerProvider::start_child_span` backfills parent.
- **#3 `Timestamp(i64 unix_ns)` + `WallDuration(i64 nanos)`** in
  `nika-types::timestamp`. RFC 3339 Display via inlined Hinnant
  civil-from-days algorithm. Serde-transparent wire. Field retrofit
  (`_ms: u64` → `timestamp`) deferred.

**Batch II — test depth:**

- **ε.2 Loom** — `#[cfg(loom)]` interleaving tests for `CancelCtx`
  (INV-029). Conditional `[target.'cfg(loom)'.dependencies]`.
  Run explicitly via `RUSTFLAGS="--cfg loom" cargo test`.
- **ε.3 proptest audit** — 14 new properties: TrustLevel lattice
  invariants (meet/join bounds, idempotence, commutativity,
  associativity, absorption); ID serde roundtrip (TenantId,
  ProviderId, ModelId, TaskId, TraceId full 2^128 surface, SpanId
  full 2^64 surface).
- **ε.1 mutation baseline** — `cargo mutants -p nika-error` run:
  60 mutants, 31 caught, 13 missed (mostly miette::Diagnostic
  accessor returns — no observable behaviour), 16 unviable.
  Viable kill rate 70.5%. Pushing to ≥90% requires dedicated
  miette diagnostic-method assertion tests; deferred to a focused
  follow-up session.

**Batch V.2** — `docs/architecture/axes.md`: 12-axis × crate ISP
matrix with shipped/reserved/not-yet markers. Source of truth for
Olympus `/graph/architecture` edge rendering + Gate 12 audits.

**Observability locks (parallel work already landed):**

- Q12 — `ObservabilitySink` dropped (5→4 effect channels);
  `AuditSink` added as compliance-grade 5th channel.
- Q13 — `GenAiAttrs` OTel semconv bridge on Infer{Request,Response}.

**CI ratchets:**

- `cargo-public-api` snapshot workflow (Gate 12 mechanical).
- `cargo-semver-checks` workflow.
- Public-api baseline files regenerated on every reservation commit
  (`--all-features --omit auto-trait-impls` to match CI invocation).

**Forward-compat seams:**

- nika-types `no_std`/`alloc` seam at module level (F1 complete;
  shipped 2026-04-17 morning).
- F2 (full per-module cfg-gating) deferred — requires uuid dep
  re-architecture (currently in `serde` feature but used in
  non-serde struct fields in RunId/EventId/CorrelationId/MemoryId).
  Re-open trigger: uuid becomes unconditional OR UUID-backed IDs
  move to a dedicated feature separate from serde.

**Numbers at close:**

| field              | value                                      |
|--------------------|--------------------------------------------|
| HEAD               | (updated at commit time)                   |
| lib tests          | 905 (+58 this session)                     |
| integration tests  | 10                                         |
| loom tests         | 2 (cfg-gated)                              |
| clippy             | 0 warnings                                 |
| hygiene vectors    | 31 deployed (27 green / 4 yellow)          |
| crates admitted    | 6 + 1 WIP (unchanged)                      |
| ADRs               | 25+ (seeds ADR-029-032 + 035 authored)     |

### ⚡ Phase D Session 4B — Data enrichment (2026-04-16)

Pure data expansion on the structural foundation laid by Session 4A.
Zero trait/struct changes — only enum variants, TOML data, and tests.

- **6 new `ParamFlag` variants** — `BatchApi`, `ContextCaching`,
  `PredictedOutputs`, `ComputerUse`, `Citations`, `IncludeReasoning`.
  Aligned with `OpenRouter` 25-value `supported_parameters` vocabulary.
  Enum: 7→13 variants.
- **3 new `Modality` variants** — `Embedding` (vector output), `Speech`
  (TTS/ASR), `ImageGen` (text-to-image). Covers non-LLM provider
  capabilities. Enum: 5→8 variants.
- **4 new `TokenizerFamily` variants** — `LlamaV4` (~200k vocab, distinct
  from LlamaV3), `Granite` (IBM `StarCoder` BPE), `Glm` (Zhipu
  `SentencePiece`), `Grok` (xAI custom). Enum: 8→12 variants.
- **7 new providers** — nvidia-nim (FIX: inventory discrepancy),
  deepinfra, replicate, hyperbolic, writer, databricks, cloudflare.
  All `openai-chat` dialect. Count: 25→32.
- **7 new capability rules** — one `Matcher::Any` fallback per new
  provider (text-only, `json_schema` where applicable). Count: 42→49.
- `mock-full` rule updated with all 13 `ParamFlag` variants.
- Cross-catalog overlap allowlist: replicate + cloudflare (dual-role).

### ⚡ Phase D Session 4A — Catalog structural enrichment (2026-04-16)

Context-window + output-limit + JSON mode enrichment. First structural
expansion of capabilities beyond the Session 2a/2b foundation.

- **3 new CapPatch fields** — `context_window_tokens: Option<u32>`,
  `max_output_tokens: Option<u32>`, `json_mode: Option<JsonMode>`.
  Per-model context windows and output limits are now expressible in the
  TOML-driven capability resolver.
- **`JsonMode` enum** — `Schema` (tool_use enforcement) / `Object`
  (unstructured json_object mode). Per-provider granularity.
- **`ContainsAny` matcher** — word-boundary-anchored substring matching
  with left/right boundary chars (`-`, `_`, `/`, `.`, `@`). Prevents
  "sonnet-4" from matching "sonnet-4-60" (the `6` after "sonnet-4" is
  not a boundary character).
- **`#[non_exhaustive]` on 20 mock structs** — all `nika-kernel-mock`
  types now enforce invariant #19 (attribute + `pub fn new()`).
- **`HttpStreamResponse::new()`** — invariant #19 compliance for the
  only `#[non_exhaustive]` struct that was missing a constructor.
- **12-field merge_with regression guard** — all CapPatch fields covered
  by a single test with confirmed RED on removal.
- **estimate_cost edge cases** — zero tokens → $0.00, nonexistent model → None.
- **MemoryId deserialize error paths** — missing `mem-` prefix and invalid
  UUID now have dedicated tests.
- Token count: 625 → **630 lib tests** (+5).

### 🛡️ Phase C Wave 3 — Stabilization + review-swarm defense (2026-04-16)

Hardening pass after the foundational-types expansion. Mutation testing,
proptest campaigns, and a 3-agent review swarm closed all P0/P1 findings.

- **Seal `SecretResolver`** — `cargo-expand` verified private supertrait;
  community can't implement, allowing future method additions (P1-1).
- **`CancelCtx` Acquire/Release** — correctness fix for v0.95 DAG cancel
  semantics (P1-6). Drop guard prevents leaked tokens.
- **Reserve NIKA-700..819** + `Category::Memory` / `WasmPlugin` / `Sandbox`
  / `Observability` — error-code real estate for v0.95+ subsystems.
- **Cost stdlib arithmetic** — `Add`/`Sub`/`AddAssign`/`SubAssign` with
  panic-in-debug, wrap-in-release semantics. `checked_add` / `checked_sub`
  for fallible callers.
- **Remove `TrustLevel::Default`** — safe-by-default inversion (P1-2).
  All trust must be explicitly stated.
- **`InferResponse.cost: Option<Cost>`** — structured cost replaces the
  deprecated `cost_usd` float. Provider-side cost tracking now type-safe.
- **Structured `DenialKind`** — replaces `CapabilityDenied { reason: String }`
  with enum variants (`FsReadNotGranted`, `FsWriteNotGranted`, `NetEgressBlocked`,
  `ExecBlocked`, `EnvReadBlocked`, `Custom`).
- **20 proptest lattice/identity laws** — cost commutativity, associativity,
  identity; trust lattice meet/join; baggage merge idempotence (integration tests).
- **MemoryId UUIDv7** — `MemoryId(u128)` → `MemoryId { uuid: Uuid }`.
  Time-sortable, standard format, `Display`/`FromStr` roundtrip.
- **`#[deprecated]` cost_usd** on `InferResponse`, `AgentOutcome`,
  `AgentCheckpoint` + `Cost::to_usd_f64()` bridge for deprecation window.
- **Pin zeroize=1.8** — workspace-wide version lock for `SecretString`.
- **cargo-mutants 88.5% kill rate** on nika-error L0 (cost/trust/baggage).
- Token count: 572 → **585 lib / 621 total** (+13 lib, +49 total).

### ⚡ Phase C Wave 2 — L0 foundational types + L0.5 traits (2026-04-16)

23 pure-data types landed in L0 crates, 6 kernel traits in L0.5, plus
forward-compat seams for v0.95 Cortex and v0.100 WASM.

- **23 L0 value types** across nika-error and nika-kernel — cost, budget,
  trust, retry, schema versioning, baggage, resource URI, content hash,
  memory frame, deny kind, cancel context, plugin DTOs, sandbox policy,
  observability event.
- **6 L0.5 kernel traits** — `IdGenerator`, `SecretResolver`, `MetricsExporter`,
  `TracerProvider`, `EventSink`, `BillingSink`. Sealed: `SecretResolver`,
  `EventSink`, `BillingSink`. Open: `IdGenerator`, `MetricsExporter`,
  `TracerProvider`. All have mock implementations in nika-kernel-mock.
- **Sealing pattern** — `Provider`, `EventSink`, `BillingSink`,
  `SecretResolver` now sealed via `mod sealed { pub trait Sealed {} }`.
  Open traits (`MemoryStore`, `EmbeddingProvider`, `ToolExecutor`) remain
  community-implementable.
- **Forward-compat seams** — `cancel.rs`, `plugin.rs`, `sandbox.rs`,
  `observability.rs` in nika-kernel. `MemoryFrame` gains reserved
  `Option<_>` fields (`cipher`, `provenance`, `retention`, `redactions`).
- **ADRs 016-020** — cancellation, streaming, runtime, retry, WASM
  (Batch F part 1). **ADRs 033-034** — L0/L0.5 expansion plans.
- Token count: 416 → **572** (+156 tests).

### ⚡ Phase D Session 2a — TOML-driven model capabilities (2026-04-14)

Zero-allocation capability resolver migrated from hardcoded Rust to a
TOML-driven rule table. Zero-alloc, proptest-verified, forward-compatible.

- **`data/model-capabilities.toml`** — 9 ordered rules covering OpenAI o-series,
  GPT-5, Claude family, Anthropic catch-all, DeepSeek reasoner, DeepSeek any,
  and xAI Grok-4. Schema `nika/model-capabilities@1.0`. First-match-wins
  semantics with build-time FK checks (providers must exist in
  `llm-providers.toml`, api_dialect must be in the closed dialect set).
- **`src/types/capabilities.rs`** — `CapPatch` (5 `Option<T>` fields,
  `const fn merge_with`, `fn materialize`), `Matcher` (Any/Exact/ExactAny/PrefixAny,
  zero-alloc `eq_ignore_ascii_case`), `Rule` (providers + api_dialect scope + matcher + caps).
- **`build/capabilities.rs`** — extracted from `build.rs` (380 LOC) to stay under
  the 1500-LOC-per-file budget. Validates TOML schema, FK checks, closed-set
  enum validation, all-None rule prevention, emits static Rust arrays at compile time.
- **`api_dialect`** — `Option<&'static str>` added to all 21 providers in
  `llm-providers.toml`. Closed set: anthropic / openai-chat / openai-responses /
  gemini / cohere / ai21 / bedrock / voyage / mock. Reserved for Session 2b+
  dialect-scoped rule authoring.
- **`supports_thinking` → `reasoning` rename** — aligns with 2026 industry
  convention (LiteLLM `supports_reasoning`, models.dev `reasoning`, OpenRouter
  `reasoning`). No compat shim (forever-v0.x nuke-and-rebuild).
- **`TokenLimitParam::MaxOutputTokens`** — variant added (OpenAI Responses API
  future-proofing). No rule maps to it yet; the `#[non_exhaustive]` enum can
  grow without a schema bump.
- **Proptest parity harness** — 10,000 random (provider, model) pairs compared
  against frozen legacy body in `mod parity_tests`. Regex widened to cover slash
  syntax, uppercase, underscore (HF-style), long names.
- **Insta snapshot** — 31 golden (provider, model) pairs reviewable under
  `src/data/snapshots/`.
- **Invariant #19 FULL** — 15 `new()` constructors across the crate (every
  `#[non_exhaustive]` public struct). Includes: `ProviderModel`, `Provider`,
  `ProviderModel`, `McpServer`, `Embedding`, `TransformDef`, `Builtin`,
  `EnvVarSpec`, `McpPackage`, `McpRemote`, `ModelCapabilities`, `ModelPricing`,
  `CostEstimate`, `ParseTagError`, `ParseCategoryError`, `Suggestion`.
- **Gate 8 GREEN** — `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps` clean.
  8+ broken intra-doc links fixed across the crate.
- **5-agent review** — rust-architect + rust-pro + rust-perf + spn-nika +
  feature-dev:code-reviewer. All P0/P1 findings addressed in same session
  across 2 hardening commits.

### 🏷️ Phase D Session 1 — Tag vocabulary + Cargo features (2026-04-14)

Typed tag system for catalog entries, Cargo feature gating, and Shield
safety invariant enforcement.

- **42-variant `Tag` enum** (`#[non_exhaustive]`) — model I/O modalities,
  reasoning/generation behaviour, economics, deployment/sovereignty,
  specialisation, domain, and MCP-server permissioning. Kebab-case wire
  format (`Tag::as_str()` + `FromStr`). Locked as enum (not `&str`) so
  pck authors get compile errors on typos.
- **`tags` + `extra_tags` fields** on `Provider`, `McpServer`, `Embedding` —
  `&'static [Tag]` (validated at build time) + `&'static [&'static str]`
  (passthrough escape hatch for community-specific vocabulary).
- **All 139 catalog entries tagged** (21 providers + 13 embeddings + 105 MCP
  servers). build.rs enforces: known tags only, sorted, deduplicated, and
  MCP entries MUST carry exactly one of `read-only` / `destructive` (Shield
  security-filter invariant, compile-time enforced).
- **Cargo features for subset compilation** — `full` (default), `minimal`,
  `mcp`, `providers`, `embeddings`, `pricing`, `capabilities`,
  `builtins-transforms`, `extension-author`. Community crates depend on
  `features = ["extension-author"]` for types-only (no bundled data).
- **7 runtime tag invariant tests** — XOR, Budget/Frontier mutex,
  Embedding/Reranker presence, sort/dedup codegen integrity, spot-checks
  (anthropic tags, stripe MCP tags).
- **COMMUNITY_EXTENSIONS.md** — pck-author pattern documentation for
  `nika-catalog-cn`, `nika-catalog-eu`, etc.
- **3-agent review** (spn-nika + feature-dev + rust-pro) — all P0/P1
  findings addressed: `f64::INFINITY` validation gap, `#[allow(dead_code)]`
  scoping, `tag_variant` drift guard, `Tag::Sandbox` doc clarification,
  `extra_tags` Gate 1 SAFETY note, version pin fix.

### ⚙️ Hygiene + automation (2026-04-14 PM)

Autonomous ecosystem hygiene stack added to prevent drift over the 11-12 month build:

- **15-vector hygiene dashboard** (`scripts/hygiene/check-all.sh`) — MEMORY HEAD,
  crate count, LOC, CHANGELOG, ROADMAP, crate specs, Linear, GitHub milestones,
  org profile, CITATION, unwraps, file LOC cap, Claude coauthor leak, private
  path leak, cargo audit. Green/yellow/red table, exit codes 0/1/2.
- **Claude Code hooks** — PreToolUse blocks 5 dangerous ops (force push,
  `git add -A`, `cargo test --test`, checkout main, `--no-verify`); PostToolUse
  inspects HEAD commit for Claude coauthor + auto-runs hygiene on admissions;
  SessionStart injects grep-verified HEAD + crate count + hygiene state.
- **Skills** — `/gate-check` and `/crate-admit` for 12-gate discipline;
  `review-swarm.md` subagent for parallel 3-agent review (Gate 11).
- **CI workflows** — `hygiene-nightly.yml` (cron 3h UTC, idempotent drift issue),
  `forward-compat.yml` (cargo-public-api + cargo-semver-checks on PR),
  `changelog-cliff.yml` (auto-PR prepend CHANGELOG on tag push).
- **git-cliff config** (`cliff.toml`) — groups match content pipeline.

## [0.80.0-alpha.4] - 2026-04-14

### 🆕 Crate admitted: nika-catalog-verify

The immune system.

Where `nika-catalog` answers "what do we know?" in O(1) from compile-time data,
`nika-catalog-verify` answers "is what we know still true?" It probes real
package registries (npm, PyPI, Docker) and remote MCP endpoints in parallel,
producing a JSON drift report. Binary, not library — runs nightly from CI or
on-demand via `cargo run -p nika-catalog-verify`.

This is the second catalog crate and the first L4 binary admitted. It exists
because static catalogs decay: a package gets deprecated, an API endpoint goes
away, a provider renames a model. Without verify, the catalog silently rots.

Exempted from Gate 5 (mutation ≥90%) because binary I/O code produces
tautological mutations. Gate 10 (legacy parity) is N/A — this is new tooling.

| Metric | Value |
|--------|-------|
| LOC | ~600 |
| Tests | partial (logic only, I/O excluded) |
| Clippy warnings | 0 |
| Unwraps in src/ | 0 |

Commit `a977e35b1`. 🦋

---

## [Previously Unreleased] — moved to 0.80.0-alpha.4

### 🔨 Refactors

- **nika-catalog Phase C migration** — migrating catalog data from hardcoded
  Rust arrays to `data/*.toml` source files, compiled at build time via
  `build.rs` + `phf_codegen`. Same zero-runtime-overhead phf maps, but the
  source of truth is now human-readable TOML. This unblocks community
  contributions to the catalog (PR a TOML file, not a Rust array).

### 🐛 Fixes

- **nika-catalog Phase A cleanup** (db0bf8e3f) — a 5-agent deep audit
  discovered 29 of our 131 MCP aliases were broken. Some pointed to
  Anthropic reference servers that were quietly deprecated ("Package no
  longer supported" on npm). Others referenced npm packages that never
  existed — Python-only tools, Go binaries, or names we'd fabricated from
  incomplete documentation. Three were community forks with zero weekly
  downloads.

  We removed all 29 and added a regression test (`removed_broken_aliases_not_present`)
  so they can't sneak back. The catalog went from 131 to 102 aliases.
  Every remaining alias now resolves to a real, installable package.

---

## [0.80.0-alpha.3] - 2026-04-13

### 🆕 Crates admitted: nika-kernel + nika-kernel-mock

The nervous system.

`nika-kernel` defines the **trait contracts for every side effect** in Nika.
It sits at L0.5 — above the pure types (error, catalog) and below the
implementations (fs, http, process, provider). Zero implementations live here.
This crate is the constitution: it says what each organ *must* do, not how.

The design follows Interface Segregation Principle to the max: ~20 fine-grained
atomic traits (`FsRead`, `FsWrite`, `HttpGet`, `ShellRun`...) grouped into ~6
super-traits of convenience (`Fs`, `HttpClient`, `ShellExecutor`, `Provider`...).
Consumers depend on exactly the surface they need — a context loader imports
`FsRead` alone, not the entire filesystem umbrella.

All async traits use `trait_variant` (Rust 1.91 native AFIT) instead of
`async_trait`. Zero boxing on the static dispatch path. The kernel carries no
tokio dependency — pure trait definitions that any async runtime can implement.

We also planted the **Cortex + agent-v2 hooks** now: `MemoryStore`,
`EmbeddingProvider`, `ToolExecutor`, `ContextCompressor`, and agent checkpoint
types. These won't be implemented until v0.95, but defining them in Phase 1
means we won't need breaking changes to `#[non_exhaustive]` structs later.
Forward compatibility bought cheaply.

`nika-kernel-mock` is the companion: deterministic mocks for every kernel trait
(`MockClock`, `InMemoryFs`, `MockHttp`, `MockShell`, `MockProvider`...).
Test hermeticity from day one — no test in Nika will ever touch a real
filesystem, a real network, or a real LLM provider.

| Metric | nika-kernel | nika-kernel-mock |
|--------|-------------|------------------|
| LOC | 3,369 | 1,731 |
| Tests | 99 | 88 |
| Mutation killed | 100% | 95.7% |
| Clippy warnings | 0 | 0 |
| Unwraps in src/ | 0 | 0 |

### Key decisions

- **Clock is SYNC, everything else ASYNC** — YAGNI on network time. Hot paths
  stay simple.
- **`BTreeMap` over `HashMap`** — deterministic iteration order, no hasher
  dependency. Tests are reproducible.
- **Cancel as `fn` param, not in struct** — keeps `ShellCommand` free of
  tokio-util. The kernel stays runtime-agnostic.
- **Provider = Infer + Stream + Meta** — all providers MUST stream (even mock).
  Embed and Vision are opt-in traits.
- **Errors per subsystem** — `ProviderError`, `ShellError`, `ToolExecError`,
  `MemoryError`. No god-enum.

All 12 gates passed. Commit `ef8804371`. 🦋

---

## [0.80.0-alpha.2] - 2026-04-13

### 🆕 Crate admitted: nika-catalog

The memory.

`nika-catalog` is Nika's static knowledge of the world: every LLM provider it
can talk to, every MCP server it knows how to install, every builtin tool it
ships, every pipe transform it supports, and the pricing of every model it's
seen.

The catalog is compiled into the binary at build time. No runtime I/O, no
config files, no network calls. You ask "do you know `anthropic`?" and the
answer comes back in O(1) via a [perfect hash function](https://en.wikipedia.org/wiki/Perfect_hash_function).

Why this matters: when a user writes `provider: claude` in their YAML, the
engine resolves the alias → canonical provider → model → capabilities → pricing
in a chain of zero-allocation lookups. No guessing, no fuzzy matching, no
"did you mean?" The catalog is the ground truth.

The lookup strategy is hybrid by design:
- **phf + unicase** for case-insensitive lookups (providers, MCP aliases) —
  because users write `Claude`, `claude`, `CLAUDE` and they all mean Anthropic.
- **Sorted arrays + binary_search** for case-sensitive lookups (builtins,
  transforms) — because `nika:read` and `nika:Read` are different things
  (actually `nika:Read` doesn't exist, and the catalog should say so clearly).

At admission: 16 providers, 105 MCP aliases, 63 builtins, 65 transforms,
61 model pricing entries. All from a single `cargo build`.

| Metric | Value |
|--------|-------|
| LOC | 2,235 |
| Tests | 85 |
| Mutation killed | 94.7% |
| Clippy warnings | 0 |
| Unwraps in src/ | 0 |

All 12 gates passed. Commit `55a451695`. 🦋

---

## [0.80.0-alpha.1] - 2026-04-13

### 🆕 Crate admitted: nika-error

The DNA.

Every error in Nika carries a code. `NIKA-001` means schema validation failed.
`NIKA-053` means a blocked command was attempted. `NIKA-382` means a canary
token leaked (prompt injection detected). There are hundreds of these codes,
and every single one must roundtrip through Display, parse back from a string,
serialize to JSON, and match the exact same format across every provider, every
verb, every transport layer.

`nika-error` is the crate that makes this possible. It defines:

- **`NikaErrorCode`** — a trait that every per-crate error enum must implement.
  This is the contract: if you want to be a Nika error, you carry a code, a
  severity, a category, and you format yourself as `"NIKA-XXX: message"`.
- **`NikaError`** — a `Box<dyn NikaErrorCode>` wrapper. The unified error type
  that flows through `?` propagation across the entire codebase.
- **`NikaCode`** — the code itself. Dual format: Display gives you `"NIKA-140"`,
  serde gives you `{"num":140,"category":"ast","severity":"error","slug":"ast-analysis-failure"}`.
- **`CoreError`** — cross-cutting errors that don't belong to any specific crate
  (Validation, NotFound, Unsupported, Internal).

This is the L0 anchor. Zero `nika-*` dependencies. Reachable from every crate
in the workspace. The first cell of the organism.

It also resolves **shadow zone 6** from the pre-launch audit: every admitted
`NIKA-XXX` now ships with a Display parity golden test against the legacy
format. No silent drift.

| Metric | Value |
|--------|-------|
| LOC | 1,013 |
| Tests | 44 |
| Mutation killed | 100% |
| Clippy warnings | 0 |
| Unwraps in src/ | 0 |

All 12 gates passed. Commit `42909b1c7`. 🦋

---

## [0.80.0-alpha.0] - 2026-04-13

### The beginning

Orphan branch `nika-diamond` (renamed `main` on 2026-05-06) created from scratch. No code inherited from legacy.
Clean slate, edition 2024, Rust 1.91.

From the start, the workspace enforces:
- `clippy::unwrap_used = "deny"` — zero unwraps, everywhere, always.
- `clippy::panic = "deny"` — if it can panic, it doesn't compile.
- `clippy::expect_used = "warn"` — we'll get there.

32 legacy crate directories excluded via `.gitignore` — they exist on disk
(the orphan branch inherits the working tree) but cargo ignores them. We read
legacy code via `git show main:path/to/file.rs` when we need guidance, but
nothing is copied verbatim. Every line is rewritten.

The organism's skeleton is in place. Now it grows. 🦋

---

[Unreleased]: https://github.com/supernovae-st/nika/compare/v0.80.0-alpha.4...HEAD
[0.80.0-alpha.4]: https://github.com/supernovae-st/nika/compare/v0.80.0-alpha.3...v0.80.0-alpha.4
[0.80.0-alpha.3]: https://github.com/supernovae-st/nika/compare/v0.80.0-alpha.2...v0.80.0-alpha.3
[0.80.0-alpha.2]: https://github.com/supernovae-st/nika/compare/v0.80.0-alpha.1...v0.80.0-alpha.2
[0.80.0-alpha.1]: https://github.com/supernovae-st/nika/compare/v0.80.0-alpha.0...v0.80.0-alpha.1
[0.80.0-alpha.0]: https://github.com/supernovae-st/nika/commits/v0.80.0-alpha.0

---

## The brouillon era (0.1 → 0.79 · 2026-01-01 → 2026-04-13)

The language was conceived earlier still — through private prototypes
and vibe-code experiments from **summer 2025** (the *exploration* era,
before git). It entered git on **New Year's Day 2026** (21:29 CET) on a
branch literally named `brouillon` (draft), and found its shape by
shipping: **79 minor versions in 103 days**, from « initial nika v0.1
with strict templating system » to a 138K-line monolith at v0.79.3. DAG validation landed hours after the first
commit; providers, tools, and the check-before-run discipline followed
version by version.

The era has a public spine: seven releases shipped to
[crates.io](https://crates.io/crates/nika/versions) in March 2026
(0.20.0 on March 4 → 0.47.1 on March 26 — later yanked in favor of the
rewrite, but the dated trail is permanent). The full branch and its
192 tags live in a private archive; the dated milestones are recorded
and machine-verified in the spec repo's
[timeline](https://github.com/supernovae-st/nika-spec/blob/main/timeline/timeline.yaml).

The version line is continuous — 0.80 picks up where 0.79 stopped —
because the LANGUAGE is the continuity. The engine is the rewrite.
