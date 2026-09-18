# nika-mcp — crate spec

| Field | Value |
|---|---|
| Status | **WIP → ADMISSION** (Phase B announce-ladder · `nika mcp` v0.1 IN-BINARY · D-2026-06-10-N6 launch floor · ADR-003 12 gates). In `workspace.metadata.diamond.wip` until this admission lands. |
| Layer | **L4 — interface** (operator/agent surface) · gated on L0 only (pure static analysis over `nika-schema` + `nika-error` + `nika-pack`) · **sync · stdio** |
| Sub-tier | L4-surface — the **agent surface**. A hand-rolled MCP server that exposes Nika's STATIC, read-only tools (9 · `nika_check` · `nika_inspect` · `nika_explain` · `nika_schema` · `nika_examples` · `nika_template` · `nika_canon` · `nika_catalog` · `nika_tools`) to any connecting client (Cursor · Claude Desktop · Zed · an agent) over newline-delimited JSON-RPC 2.0 on stdio. Reachable the day `nika --help` lists `mcp`. |
| Design | ONE crate, **zero SDK**. The transport is hand-rolled newline-delimited JSON-RPC 2.0 over stdio (`serde_json` the only wire dep) — the same « talk the protocol directly » discipline Diamond uses for provider wire formats. The protocol dispatch (`handle`) is a **PURE function**; the server half is split into `protocol` (the pure dispatcher · version negotiation · batch) + `tools` (the pure static-analysis catalog) + `lib` (the stdio I/O pump). Running a workflow is **NOT** exposed — that needs the effect-permits boundary, so the MCP surface is read-only by construction (`nika run` stays the gated, audited effectful path). The **client half** (§6) is `client` (the registry + the pin flow) · `session` (one confined stdio server · handshake · bounded `tools/list` / `tools/call`) · `pin` (the approved-definitions lockfile) · `dispatch` (the runtime plane a run rides · approved tools only) · `sandbox` (the confinement the spawn rides). |
| LOC budget | ≤15k crate · ≤1500/file · ≤100/fn (Diamond caps) · the server half ≈540 src (lib · protocol · tools) · the client half split at the file cap 2026-09-13 (`client` 1139 → `client` + `session`) |
| Crate version | tracks workspace · License `AGPL-3.0-or-later` · Edition 2024 · Publish `false` (Foundation crate · ADR-017) |
| ADRs | ADR-003 (12-gate admission) · **ADR-080 (MCP stdio · CVE · sandbox)** · D-2026-06-10-N6 (launch-surface-complete · MCP at announce) |
| Error range | **none user-facing** — protocol errors travel IN-BAND as JSON-RPC error replies (`-32601` method not found · `-32602` invalid params · `-32700` parse error · all JSON-RPC 2.0 standard). The crate's own `McpError` is an internal `thiserror` enum for a **dead transport** only (a broken stdio pipe) · NOT a `NIKA-XXXX` range (transport failures never reach the workflow author). A TOOL failure is a successful reply with `isError: true` (the model SEES it · MCP law). |
| Reference | `crates/nika-cli/src/verbs/check.rs` (the check ladder `nika_check` reuses) · `crates/nika-error/src/codes.rs` + `crates/nika-pack` (the registries `nika_explain` reads) · the MCP spec lifecycle (version negotiation MUST) |

---

## 1. Purpose

`nika-mcp` is the **agent bridge** for the engine. It turns Nika's static
guarantees into Model Context Protocol tools so any MCP client — Cursor, Claude
Desktop, Zed, or a bare agent loop — can **audit a workflow before running it**
and **learn an error code**, without a network round-trip to a hosted service
(alignment Rule 1 · the binary is self-contained · sovereign).

The server surface is **read-only by construction**. Two tools, both PURE static
analysis:

- **`nika_check`** — statically audit a `*.nika` (schema · DAG · CEL ·
  effects · permits · cost) and return the full check report, or a clean
  verdict. Auditable before a token is spent.
- **`nika_inspect`** — project the DAG as the canonical `graph_format: 3`
  document (`nika-graph::project` VERBATIM — byte-equal with `nika inspect
  --format json` and the LSP's `nika/semanticDocument.graph`, both
  law-pinned). Findings → `{"graph": null, "reason": "findings"}` — never a
  projection of an unproven DAG. An agent SEES the graph before editing it.
- **`nika_explain`** — teach one error code (cause · category · fix form), from
  the numeric crate registry OR the embedded spec-code canon.

Running a workflow is deliberately **not** exposed (it needs the effect-permits
boundary); `nika run` stays the gated, audited effectful path.

## 2. Public API

- `run_stdio() -> Result<(), McpError>` — serve MCP over stdio until EOF (the
  client disconnects). The thin wiring wrapper.
- `run<R: BufRead, W: Write>(reader, writer)` (crate-private) — the transport
  pump over arbitrary byte streams. Unit-tested with in-memory buffers.
- `dispatch(&Value) -> Option<Value>` — dispatch one incoming message OR a
  JSON-RPC 2.0 batch (an array · the 2024/2025-03 revs).
- `handle(&Value) -> Option<Value>` — the PURE single-message dispatcher
  (`Some(reply)` for a request, `None` for a notification).
- `McpError` — `#[non_exhaustive]` transport-failure enum (one variant ·
  `Transport(std::io::Error)`).

## 3. Protocol surface

- **JSON-RPC 2.0** over newline-delimited stdio (one compact message per line ·
  no embedded newlines · the MCP stdio framing).
- **Methods**: `initialize` · `tools/list` · `tools/call` · `ping` · everything
  else → `-32601`.
- **Version negotiation** (spec lifecycle MUST): `initialize` ECHOES the
  client's requested `protocolVersion` when it is one of `SUPPORTED`
  (`2025-06-18` · `2025-03-26` · `2024-11-05`), else answers with the server's
  latest and lets the client decide.
- **Batch**: an array of messages returns an array of the non-empty replies, or
  `None` when every member was a notification (JSON-RPC 2.0 §6).
- A malformed line is answered with a `-32700` parse error rather than killing
  the session; a notification (no `id`) yields no reply.

## 4. Security

Read-only by construction — every tool is pure static analysis over its
arguments (parse · check · code lookup) · zero effects · zero network · no
workflow ever RUNS through MCP. That purity is the structural guarantee that
makes a tool safe to expose to any connecting client (ADR-080 · MCP stdio CVE
sandbox). The effectful `run` path is gated behind `permits:` and lives in
`nika-cli`/`nika-runtime`, never here.

## 6. The client half — registry · pins · runtime dispatch

The crate also speaks MCP as a **client**, for the servers a workflow
invokes (`invoke: { tool: "mcp:<server>/<tool>" }`). The operator guide is
`docs/ops/mcp-servers.md`; the contract:

- **`client`** — the registry `.nika/mcp_servers.json` (`mcp_servers_format: 1`
  · `servers.<name>.{command, args, network}` · strict, teaching parse) and
  the pin flow over the `ToolsListDyn` seam: `approve_server` (the operator
  re-pin · `nika mcp approve <server>`) and `connect_verified` (TOFU enroll ·
  verify · drift refusal). `StdioMcpClient` is the one-shot production
  `tools/list` (open a session, ask, drop).
- **`session`** — `StdioSession`: spawn CONFINED (the exec verb's
  `CommandSandbox` · fs = the project tree · net = the entry's arm · env =
  the runner floor), handshake, bounded request/reply, SIGKILL on drop
  (INV-011). The child's **stderr tail** (4 KiB) and the confinement note
  ride every transport failure (#1376 · a launcher dying under the sandbox
  reads its own cause). Two seams: `McpConnectDyn` (entry → session) and
  `McpSessionDyn` (`tools_list` · `tools_call` → `CallOutcome` = text
  blocks · `structuredContent` · `isError`).
- **`pin`** — `.nika/mcp_pins.json` (`mcp_pins_format: 1`): one blake3 pin
  per tool over `{name, description, inputSchema}` + the snapshot, keyed by
  the server's identity (command + args · re-pointing is drift). A
  hand-edited lockfile is `NIKA-MCP-004`, never re-anchored.
- **`dispatch`** — `McpToolPlane`, the runtime plane (#1575), handed to the
  invoke verb as `nika_runtime::McpPlane` via `dispatch::run_plane`. The law
  is **approved tools only**, judged before each effect: server in the
  registry (else `NotFound` → `NIKA-INVOKE-001`) → tool in the approved pin
  set (else `NIKA-MCP-006`, no spawn) → live `tools/list` matches the pins
  (else `NIKA-MCP-003`, session dropped) → `tools/call` (an MCP `isError` is
  `NIKA-MCP-002` · a dead pipe is `NIKA-MCP-001` transient, the session is
  reopened on the next call). One session per server per plane; the
  registry + lockfile are read once, lazily. `tool_defs()` offers the
  approved definitions (from the lockfile · no spawn) under their
  `mcp:<server>/<tool>` names to the agent universe.

Codes: `NIKA-MCP-001` (not reachable · not configured) and `NIKA-MCP-002`
(the tool call failed) are the spec's; `003` drift · `004` corrupt lockfile
· `005` sandbox refusal · `006` unapproved are the pin family's own, each
naming its remedy in the text.

---

## 5. The 12 gates (readiness map)

| Gate | Status |
|---|---|
| 1 SPEC | ✅ this file + ADR-080 |
| 2 TDD | ✅ RED→GREEN per module — `protocol` (initialize · negotiation · batch · notification-silence · all JSON-RPC codes) · `tools` (catalog · check clean/findings/non-conformance · explain known/unknown/bare-normalize · unknown-tool) · `lib` (the in-memory pump: reply · blank-skip · notification-silent · `-32700`) |
| 3 IMPL | ✅ pure dispatcher + pure tool catalog + the stdio pump · **0 `.unwrap()`/`.expect()` in `src/`** (`?` + `unwrap_or`/`ok_or`) |
| 4 CLIPPY 0 | ✅ `cargo clippy -p nika-mcp --all-targets -- -D warnings` = 0 |
| 5 MUTATION ≥90 | ✅ **24/25 caught (96%)** · `cargo-mutants -p nika-mcp`. **Documented exemption** (the 1 miss): the `run_stdio` wiring wrapper (`run(stdin.lock(), stdout.lock())`) cannot be driven by a lib test (it binds the real stdio handles); its pump logic is extracted into the generic `run<R, W>` which IS mutation-covered (the in-memory pump + flush tests), and the wrapper is exercised by the Gate-9 canary E2E. |
| 6 PROPTEST | ✅ `proptest` on the JSON-RPC dispatcher (untrusted client input): for ANY method/id a request yields a well-formed reply (id echoed · exactly one of result/error · never a panic); for ANY method a no-id message is silent. |
| 7 BENCH | ➖ N/A — a thin stdio pump over pure dispatch · no hot path (exemption per ADR-003 « if applicable »). |
| 8 DOC 0 | ✅ `RUSTDOCFLAGS="-D warnings" cargo doc -p nika-mcp --no-deps` = 0 |
| 9 CANARY E2E | ✅ a real `nika mcp` stdio session: `initialize` (handshake + serverInfo) → `tools/list` (2 tools w/ inputSchema) → `tools/call nika_explain` + `nika_check` (round-trip) → unknown-tool (`isError:true`) → bogus method (`-32601`) → clean EOF exit (no hang). |
| 10 GOLDEN PARITY | ➖ N/A — greenfield · no legacy MCP server to match (exemption per ADR-003 « where applicable »). |
| 11 REVIEW SWARM | ✅ 3 parallel reviewers — **spn-nika** (conventions · PASS) · **feature-dev** (protocol correctness) · **spn-refuter** (adversarial). Convergent findings RESOLVED: `-32600` Invalid Request for an absent/non-string `method` (was `-32601`) + an empty `[]` batch (JSON-RPC 2.0 §4/§6 · a strict client branches on the code) · the load-bearing stdio **flush barrier** is now asserted (a `Vec` writer's flush is a no-op · a counting writer witnesses it) · `dispatch`/`handle` tightened to `pub(crate)`. The **read-only guarantee was UPHELD** (the refuter confirmed the parse/check path is effect-free, DoS-capped). |
| 12 ATOMIC COMMIT | ⏳ this admission commit removes `nika-mcp` from `workspace.metadata.diamond.wip`. |
