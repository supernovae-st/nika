# nika-check — the static judgment crate

**Layer** L0 (pure · sync · zero I/O · zero async) · **split** from
`nika-schema` 2026-07-21 at the 15k crate-size wall (the
nika-graph/nika-dap precedents) · **publish** planned with the workspace
train.

## What it is

The static judgment over a parsed workflow, in ONE crate an embedder can
depend on without pulling the CLI:

- **`analyzer/`** — the Core conformance pass (`analyze(&RawWorkflow) ->
  Result<AnalyzedWorkflow, Vec<SchemaError>>`, collect-never-fail-fast)
  and the ONE edge computation every surface projects (`analyzer::edges`
  · `G_p` = `E_d` ∪ `E_c` + the recovery reads + the topological waves).
- **the `check` ladder** (`check(&RawWorkflow) -> CheckReport`,
  `check_composed` with an injected reader) — the audit-before-it-runs:
  cost ceiling · the IFC secret-leak/egress passes · capability-escape
  fit against the declared `permits:` · hard `policy:` · the NEP-0002
  trifecta · gate reachability · schema typing/lint · unknown/missing
  tool args · hints · the composition lane · the `RunCertificate`
  (ADR-092) · `infer_permits` synthesis. Because the language is
  statically analyzable BY CONSTRUCTION (acyclic DAG · bounded
  `for_each` · non-Turing CEL · declared effects), this answers « what
  will this workflow do, cost, and touch? » with **zero API calls and
  zero tokens spent** (spec `07-conformance.md` §`nika check`).
- **the project files lane** (`check_project`) — the root workflow's
  `{ file }` fields compose under its folder through an injected byte reader
  (spec 02 §Project files), and the composed workflow is the one every lane
  judges. Each child the composition walk loads composes from its own folder;
  its files meet the ordinary check one at a time, and a finding a file
  brings is reported with the child, task, field and path
  (`CheckReport::project_files`). A child's own read boundary stays its own
  check's. A plain `check()` reads no files.
- **composed calls** (spec 14 · NIK-273) — `check_composed` and
  `check_project` resolve every direct `invoke: workflow:` call through the
  composition walk (one parse per file, its file cap) and list it in
  `CheckReport::composed_calls` (`ComposedCall`): whether the closure brings
  untrusted content in (`ingress`, the table's born sources: `exec:` · an
  invoked `nika:fetch` · an `mcp:*` tool · an `agent:` whose whitelist admits
  ingress), and where three kinds of content entering it go (`CallFlow`, by
  the same content-flow pass run over the child): what it brings in (`born`),
  the call's arguments (`args`), and a file a tainted writer left before the
  call (`files`, read by an `exec:` or a builtin read of the closure, whatever
  the path). Each may reach the
  call's output, an egress task of the closure, or a file. The trifecta lane
  judges the call as the tasks it stands for: a source when born content
  reaches its output or a file, a sink when content reaches an egress task.
  A parent refuses `NIKA-SEC-009` exactly as the inline form would, unless a
  blocking gate of its own dominates the path; a gate inside the child is not
  credited to the caller. A refusal names the child task behind the call. A
  closure the walk cannot resolve (unreadable · unparseable · a graph that
  does not analyze · registry or templated target · a cycle · past the cap)
  sets every fact (fail-closed). `grants` lists the caller's entries the
  closure uses or may use: the needs its inference lists, and in a category
  a workflow reaches opaquely (an `exec:` · a dynamic program, host or path ·
  an agent whitelist · a harness-servable agent) the entries its own declared
  block, met with every block above it, keeps; `nika-dap`'s
  `NIKA-DRIFT-001` never names a held grant dead. The runtime trust gate
  re-derives the trifecta over the same calls, and counts a call its report
  does not list as unresolved. `effect` names the first egress-capable task
  a run of the closure reaches, whatever content reaches it, on a route
  that crosses no confirm gate of the closure (that nearest gate owns it ·
  an unresolved closure sets it): the consent lane (`NIKA-SEC-014`) judges
  such a call as that task, so a confirm gate in front of it must close
  every route on a refusal, as in front of the inline task. When every such
  route crosses a closure gate the check cannot decide from the caller (a
  `when:` on an input the caller may bind to its answer),
  `effect_conditional` is set and a refusal reaching the call is the
  advisory hint, never the blocking row. A plain `check()` resolves no call: its
  trifecta and consent lanes, like its composition and cost lanes, judge the
  caller's own tasks only.
- **the file channel** (`file_channel`, inside the content-taint pass) —
  under a declared `fs.read`, a task that reads a file a tainted write
  left carries that content on: an `exec:` whatever the file, once an
  earlier writer ran (the pass's sweep order); a builtin read (nika-cap's
  read slots: `nika:read`, `nika:grep` and its default root, `nika:glob`,
  `nika:edit`, a `decide` bundle, `image_fx`/`chart`/`image_generate`
  inputs, a `fetch` multipart part) when its path may be one the write
  reached, whatever the order (a file outlives the run order: a read
  beside the writer or before it reads it in this run or the next; the
  pass sweeps until the writers settle). A call whose closure reads files
  meets any tainted write of its caller. Paths compare lexically (folded,
  ASCII case-blind, a segment past ASCII may be any segment, `/private/tmp`
  `/var` `/etc` are their firmlinks); a template, a subprocess, a server or
  a child closure may reach any path. Only `/` sits at a known place: a
  path from the workspace, the home directory or above the workspace names
  a rooted file the rooted path ends with (holds anywhere, for a
  directory), and a rooted directory may hold any of them.
- **the conformance harness** (`tests/common`) — the Core, Deep, lint, runtime
  and stdlib fixture levels from the checkout pinned by `SPEC_PIN`. Core and
  Deep verdicts now include the Agent Skill resolution lane through the same
  injected reader shape as composition: an absent file maps to
  `NIKA-AGENT-003`, invalid skill bytes to `NIKA-AGENT-004`, a path outside a
  declared fs boundary to `NIKA-SEC-004`, and absent permits to
  `NIKA-AUTH-006`. Unknown expected codes fail loudly instead of being silently
  accepted by a generic validation bucket. Project files compose through
  `check_project` from the fixture's own directory, links refused per segment.

`nika-schema` keeps its blueprint shape (THE PARSER: AST + raw + error +
keysets). The dependency points DOWNWARD-ONLY: `nika-check → nika-schema`,
never the reverse — the one shared constant (`ERROR_DOCS_BASE`) lives
beside `SpecCode` in `nika_schema::error` and is re-exported here.

Filesystem permit refusals compare lexical paths. Their diagnostic names
the requested path and declared boundary, explains that the pure checker
does not resolve symlinks, and teaches consistent spelling for aliases
before changing grants. The matcher, refusal and automatic-fix policy do
not infer filesystem identity from an alias example.

The historical `failure_plan` shape `host_passwd_read` projects confirmed
filesystem refusals for `nika:read` and `nika:write`, including relative
paths. It uses the category, task and boundary provenance; wording is
copied only as the message. Missing authority retains `NIKA-AUTH-006`.

Agent whitelist exclusions restrict the offered tools. They contribute no
required authority to permit checks, inference or task capability projections;
positive whitelist rules still require their declared boundary.

Same-path mutations (`NIKA-SEC-012`) key `nika:write`, `nika:edit` and
`nika:remove_file` by their static path under lexical normalization. Two
mutations of one key must be ordered by the precedence graph `G_p`: the
scheduling edges of `analyzer::edges::derive_edges` (`with:` references,
group folds and `after:` predicates, transitively), never an `unwind` edge,
which attaches cleanup and orders nothing. A constant `for_each` mutation
races its own iterations. Quoted text that only looks like a reference is
not an edge. The judgment is lexical and static: it does not resolve
symlinks, and a dynamic path makes no claim.

## Why L0, not L1

The registry's mechanical sort (< 10 s): not the binary, not a
transport, not runtime policy, not a verb orchestrating L1 effects, not
an I/O primitive → **L0**. The code is the same pure/sync/zero-I/O code
that sat inside `nika-schema` at L0 (vector 22 no-async-in-L0 + vector
33 layer-deps bans judge it unchanged). And THREE L0 consumers make any
higher layer a `check-layering.sh` upward-dep violation: **nika-lints**
(`native_first::classify` + `analyzer::edges`), **nika-graph**
(`CheckReport` + `task_permits` + `analyzer::edges`), and
`nika-schema`'s own `skill.rs` (the docs-URL constant). L0→L0 same-layer
edges are the allowed case.

## Why a crate at all (collapse-vs-publish)

Seven consumers across L0→L4 (cli · lsp · mcp · dap · runtime ·
verb-agent · lints · graph · onboard) rode the module boundary; the
embedder/SDK consumer (VISION_2040 §1 lists `sdk` in L4) wants static
judgment without the operator surface. No existing crate is the home:
`nika-cap` is the permits *vocabulary* and never touches an AST (its own
invariant — the AST-side capability judgment is exactly what lives
here); `nika-graph` is one projection, not the judgment. Nothing in the
cluster went elsewhere — the clean cut.

## Cross-crate `#[non_exhaustive]` discipline

The raw enums (`RawAction` · `ForEachValue` · `RawCommand` ·
`VisionInput`) are `#[non_exhaustive]` in `nika-schema`; inside the
defining crate the matches compiled exhaustive, outside they demand a
wildcard. Every match in this crate carries the fail-loud arm (the
nika-graph idiom · `#[allow(clippy::unreachable, reason = …)] other =>
unreachable!`) — a future variant must teach its checker explicitly;
silently-wrong judgment is the one unacceptable outcome.

## Metrics (projected — `scripts/crate-metrics.sh nika-check`)

Counts are deliberately not copied here: derive them from
`scripts/crate-metrics.sh nika-check`, `scripts/ci/check-crate-size.sh` and the
pinned conformance checkout. The hard limits remain the machine contract;
prose cannot become a second clock. Benches: `parser_bench` (parse+check) ·
`refonte_baseline` (parse/analyze/check across topologies · W0 gate 8).

## The split gates (moved-code admission, per the nika-dap precedent)

| Gate | Verdict |
|---|---|
| SPEC (this document) | ✅ |
| Workspace member + layer metadata (`layers.nika-check = "L0"`) | ✅ |
| Layer-registry row | ✅ |
| crate-spec freshness vector 6 | ✅ (live anchor above) |
| public-api baseline + coverage floor row | ✅ |
| cargo-machete clean | ✅ |
| clippy `-D warnings` · fmt · cargo doc 0 warnings | ✅ |
| `cargo test --workspace --lib` + `--tests` green (conformance at the spec pin) | ✅ |
| MUTATION | inherited — the code moved mechanically from `nika-schema`, whose Gate-5 budget was measured at admission (290≤300); a fresh cargo-mutants run is the admission-tier follow-up, same class as the nika-graph/nika-dap moves |
