# Crate spec — `nika-acp` (Gate 1)

| | |
|---|---|
| Status | **SPEC** (P3.1 · D-2026-08-04-N1) — Gate 1 authored 2026-08-05; implementation is P3.2, behind the `access-harness` cargo feature (default OFF). |
| Layer | **L1 effect** — async · one effect family (drive an external agent harness over ACP) · every OS reach behind the confined-spawn seam. |
| Design | The **harness access class** made real: `agent:` tasks execute on the user's OWN authenticated agent harness (gemini-cli · qwen-code · kimi-code · codex-acp · claude-agent-acp), driven through the official Agent Client Protocol — under nika authority (permits bridge), billed honestly (subscription ≠ free), behind an explicit opt-in. The harness owns auth; nika never holds a credential (A-3). |
| Name | `nika-acp` (the protocol is the crate's whole surface — the `nika-mcp` naming precedent). |
| LOC budget | ≤6k src (est. ~3.5k). ≤1500/file, ≤100/fn. |
| Deps | `agent-client-protocol = "2"` (pinned · see §2) · `nika-kernel` (hub) · `nika-types` · tokio (workspace). dev: the mock agent fixture (§5). |
| Publish | `false` — engine-internal effect crate. |

## 1 · Why this crate exists (legitimacy is the design constraint)

Token-lifting is dead (the 01→04/2026 purge class); driving the vendor's own
harness through its official protocol is the one durable lane: the harness owns
its auth store, nika drives the session. ACP is the column — v1 stable, an
official Rust SDK, ~35 native agents, org-maintained adapters (`codex-acp`,
`claude-agent-acp`). MCP sampling was deprecated 2026-07-28 and is rejected.

The adapter process is engine INFRASTRUCTURE (like an HTTP socket), NOT a user
`exec:` — but strictly bounded (§4). No route ever changes a verb's meaning:
`agent:` ↔ harness is the natural pairing; `infer:` on a harness is P4's
evidence-gated question, refused here with a witness.

## 2 · Protocol pin (re-verified 2026-08-05 · primary sources)

- Crate `agent-client-protocol` **2.0.0** (2026-07-23 · 3.5M downloads ·
  `agentclientprotocol/rust-sdk`). SDK version ≠ wire version: 2.0.0 speaks
  **stable protocol v1**; a DRAFT `V2SessionBuilder` exists in-tree.
- Consequence: the **per-version generated-schema diff gate is mandatory** —
  a wire-v2 stabilization must fail loud at pin-bump time, never drift in.
- SDK roles: Client · Agent · Proxy · Conductor. Nika implements the
  **Client** role only.

### §2bis · The `preserve_order` wall (found 2026-08-05 · the quarantine)

The official SDK requires `serde_json/preserve_order` — **every version
since 0.15.0** (sparse-index verified; wire-v1 stability only exists after).
Cargo feature unification is workspace-global: the moment the dep entered
the diamond graph, `serde_json::Value` flipped from sorted keys (BTreeMap)
to insertion order (IndexMap) across the WHOLE engine, and five
byte-attested goldens turned red (decision receipts · dispatcher receipts ·
urlencoded forms). Byte-attested surfaces are the product's honesty layer —
they can never depend on a vendor SDK's cosmetic feature choice.

**Structural consequence (in force)** · `crates/nika-acp` is a STANDALONE
workspace (root `exclude` + its own lockfile): the mock agent + SDK-side
conformance live there; the SDK's feature graph never unifies with the
engine's. The engine-side design is the OPERATOR'S GATE, two lanes:

- **Lane A (recommended · sovereign)** · the engine speaks wire v1 through a
  small hand-rolled JSON-RPC client (initialize · session/new · prompt ·
  update · cancel · request_permission — the Client role's narrow waist),
  and THIS harness proves conformance from the outside (the official SDK
  drives the mock agent against our client over a process boundary).
  Anti-capture by construction; the official SDK stays the judge, never a
  link-time dependency.
- **Lane B (parallel · upstream)** · PR the SDK to feature-gate
  `preserve_order` (it is cosmetic for JSON-RPC). If accepted, Lane A's
  hand-rolled client can retire at a later major — evidence first.

## 3 · Public API (the seam, not the protocol)

```rust
/// One configured harness adapter — id · binary identity · version pin
/// range · per-adapter kill-switch. Ids are UNIQUE and never equal an
/// AccessClass wire string (`harness` as an id would make `--access
/// harness` ambiguous — refused at registry load · R-5d).
#[non_exhaustive]
pub struct HarnessAdapter { /* id, command, args, version_req, enabled */ }

/// A live ACP session on one adapter — serial by construction (one
/// in-flight prompt; the self-queue rides the session, the wave
/// scheduler stays untouched).
pub struct HarnessSession { /* connection, child: KillOnDrop, caps */ }

/// The kernel seam (P3.4 · lives in nika-kernel-ai, not here):
/// `AgentBackendDyn` — sibling of `ProviderInferDyn`, additive.
/// nika-acp provides the impl; nika-verb-agent consumes the trait.

/// Session lifecycle (the Client trait impl):
///   initialize(v1) → session/new { cwd, mcp_servers: [nika-mcp]? }
///   → session/prompt → session/update stream → session/cancel.
/// Every update maps to existing task events (additive fields only).
```

Errors: `NikaErrorCode` one-voice · new rows in the **NIKA-1803..1849** span
(the access block · 1800-1802 taken): adapter-not-found · version-outside-pin ·
handshake-refused · session-died · update-overflow. Each teaches its fix line.

## 4 · Confinement (the nika-mcp discipline, lifted — with ONE deliberate difference)

Lifted from `nika-mcp/src/client.rs:559-604` (the confined-spawn seam):
- OS confinement via the SAME sandbox `confine()` transform (ADR-095 Layer 6) ·
  a refusal is terminal · the receipt line prints once on live spawn.
- `KillOnDrop` child · piped stdio · bounded line reads (update-overflow
  refuses, never OOMs).
- Identity handshake: binary present + `--version` inside the adapter's pin
  range BEFORE initialize; a colliding binary name also proves its exact
  session subcommand through a bounded public help marker (`kimi acp --help`,
  not legacy `kimi --acp`) · controlled argv · no shell.

**The difference (A-3):** NO `apply_env_scrub` of the harness's own auth
store — the harness MUST read its own credentials; scrubbing its env would
break the whole legitimacy model. The child env is: the runner floor ∪ the
adapter's declared passthrough ∪ nothing of nika's secrets. Documented at the
spawn site; a negative test proves nika's own `NIKA_*_API_KEY` vars never
cross. Auth readiness reads no credential byte: it checks path metadata, obtains
a zero-read read-only handle so the OS applies UID/ACL policy, and requires the
managed-seat witness `credentials/kimi-code.json` (never another provider or
the distinct `mcp/` subtree). It honors `KIMI_CODE_HOME` and fails closed on
absent, empty, ambiguous, unreadable, swapped, or symlinked stores. This is a
configuration witness, not a token-validity claim: the first ACP session owns
expiry/parse judgment and must fail without any native-provider fallback.
A Command-auth row's login command (its exit code alone) has 30 s to answer:
valid checks were measured at 12 s and 25.8 s (2026-10-09), and the former
10 s read a signed-in seat as signed out. A check that does not answer in time,
or cannot run, is an unknown sign-in (`LoginCheck::Unanswered` · `Unreadable`,
carried as `login` on the probe facts), never signed out; admission still
requires an answer (`configured` stays fail-closed).

## 5 · The mock ACP agent (P3.3 · the load-bearing instrument)

A deterministic fake agent speaking wire v1 (a test helper bin in
`tests/`): scripted turns · permission requests inside AND outside grants ·
usage reporting · auth-absent · version-mismatch refusal · cancel. Every e2e
below runs hermetic — zero network, zero real harness in CI. A gate proven
only against a real harness is not CI-provable (instrument law).

## 6 · Authority bridge (P3.6 · confused-deputy-free)

ACP inverts capability direction: nika (Client) implements `fs.*`/`terminal/*`
for the agent and receives `session/request_permission`:

- inside a `permits:` grant → auto-answer allow · emit `permit_checked`;
- outside every grant → the EXISTING pause (exit 4 · question verbatim ·
  `--resume --answer`) — the human gate is never answered by the engine;
- `allow_always` NEVER auto-granted (the GOOSE_MODE=auto anti-pattern · A-5);
- fs/terminal client-caps are backed by the nika sandbox, not raw OS.

Negative tests: a bypass attempt refuses; the gate question surfaces verbatim.

## 7 · Billing honesty (P3.9)

Harness rows emit `access=harness` · `billing=included_quota|extra_usage|unknown`
· `UnpricedReason::SubscriptionQuota` — never a fabricated $0 (A-6/A-7). The
ledger law is untouched (`unmetered never trips` · `ledger.rs:190`); the
epilogue speaks the quota line only when observable.

## 7bis · Visible thinking (2026-10-09)

Silence is the transport's signal only when the agent is genuinely silent. Measured on Claude
Code ACP authoring calls (`@agentclientprotocol/claude-agent-acp` 0.81.1, effort `max`): no
thought frame ever arrived, so a call that thought for minutes wrote only its answer chunks at
the end, and a call still thinking at the silence allowance was cut with nothing but one status
and one usage frame. The adapter writes `agent_thought_chunk` only for non-empty thinking
text, and recent models default the SDK's `thinking.display` to `omitted` (signature-only
blocks whose text is empty). Codex ACP streams its reasoning unchanged.

So every Claude Code session the engine opens, the audited one-shot profile (authoring,
`infer:`) and the `agent:` seat alike, sends `_meta.claudeCode.options.thinking =
{type: adaptive, display: summarized}` with `session/new`: the adapter spreads those options
over its defaults, and the SDK maps them to `--thinking adaptive --thinking-display
summarized`. Adaptive thinking is the SDK's default for the models that support it, so this
changes what is displayed, never the model, the effort, a budget or a tool; an agent session
carries no other option, and its own loop, tools and settings stay its own. A thinking option a
caller already names is kept as given. Codex and every other adapter send no options. Thought
frames stay counted, never stored as text, and the 600 s silence allowance and the frames that
re-arm it are unchanged: a thought frame shows the adapter wrote one, not what the model is
doing. How the option meets a thinking setting of the user's own Claude Code configuration
(which an `agent:` session reads, and the one-shot profile does not), and whether every offered
Claude model accepts adaptive thinking, are to be verified live on the integrated binary.

## 7ter · Persistent conversation profile (2026-10-09)

A Session conversation led by an ACP agent runs in one persistent agent session whose only tools
are Nika's own (`nika_harness::conversation`, opened by `SpawnedHarness::converse`). It sits
beside the one-shot profile, which is unchanged.

- **Once, then per entry.** `session/new` once per conversation, one `session/prompt` per user
  entry, request ids from 101; the agent keeps the history and runs its own loop until it answers.
  The session's root is a fresh scratch directory; the adapter's process group, the scratch and
  the driver end with the `Conversation` handle.
- **Closed surface, no turn limit.** Claude Code receives the one-shot's audited options without
  `maxTurns`: `tools: []`, `mcpServers: {}`, `strictMcpConfig: true`, `settingSources: []`, no
  plugins, skills or agents, `allowDangerouslySkipPermissions: false`, `persistSession: false` and
  summarized thinking. Its only MCP server is Nika's, mounted through `session/new`'s
  `mcpServers` (HTTP: `type`, `name`, `url` and an `Authorization` bearer header). Codex mounts
  Nika's server from its own configuration (`mcp_servers.<name>`, every other configured server
  disabled, every tool feature off, read back with `codex mcp list` before the spawn: exactly one
  server enabled, Nika's), reads the bearer from the environment variable the configuration
  names (`NIKA_MCP_BEARER`, never the configuration or a command line), and keeps the `read-only`
  mode applied and read back. A user server already named like Nika's refuses the conversation.
- **Admission and transport.** Only claude-agent-acp 0.81.1 and codex-acp 1.13.1 are admitted,
  judged on `initialize` as the one-shot judges them. The transport follows the agent: HTTP when
  Claude Code advertises `agentCapabilities.mcpCapabilities.http` (Codex's configuration speaks
  Streamable HTTP) and Nika offers its endpoint, else Nika's stdio bridge when offered;
  otherwise the conversation is refused after `initialize`, before any session, and nothing falls
  back to the one-shot path. The opening record names the transport chosen, what the agent
  advertised, what was offered, the server and the selection applied; never the endpoint or the
  bearer.
- **Permissions.** Nika answers every `session/request_permission` itself: `allow_once` only when
  the call names one of the offered tools on Nika's server (`mcp__<server>__<tool>`, read from
  `_meta.claudeCode.toolName` or the call's `name`, the server Claude Code names beside it
  agreeing); anything else is rejected with `reject_once`, so the agent goes on without it
  (`cancelled` when no such option is offered). `allow_always` and remembered rejections are
  never chosen. Each decision is a turn beat and lands in the turn record (`allowed` count,
  `denied` tool names, plain words only).
- **Turns and Stop.** A turn's beats are its answer and thought text, its tool frames (now
  expected activity) and the permission decisions, ending with its record: the closed end
  (`end_turn`, `max_tokens`, `max_turn_requests`, `refusal`, `cancelled`, `other`), the Stops
  received and whether `session/cancel` went out (once per turn, however many Stops), the
  permissions, the bounds and the activity of §7bis's records. One turn runs at a time; a prompt
  sent mid-turn is refused on its own stream. The turn's silence allowance applies while no tool
  call is open (Nika's tool time is the Session's, bounded by its Stop), and again once Stop was
  sent; between turns there is no bound. A transport failure, an unreadable line or a turn that
  outlives its allowance ends the conversation.
- **Not yet.** `session/load` is not used: a reopened conversation starts a new session and the
  Session folds its context brief into the first prompt. The stdio bridge exists as a library
  (`nika_mcp::conversation::bridge`) but no CLI starts it yet (`nika mcp --session`), so an agent
  without HTTP MCP is refused. How codex-acp 1.13.1 presents a Nika tool call (whether it asks a
  permission, and under which name) and the real Claude adapter's frames are to be confirmed by a
  live probe.

## 8 · Non-goals (P3)

- No `infer:` on a harness (P4 · capability attestation first).
- No model selection — the resolver never picks a model (A-1/A-2 · H8).
- No new workflow YAML key · no spec change · no schema edit (through P5).
- No oauth flows (P5 · xAI device-flow pilot).
- No parallel harness sessions (serial self-queue; revisit on evidence).

## 9 · Gates plan

Gate 5 mutation ≥90% on the session state machine + the permission bridge ·
Gate 6 proptest on update-stream framing (bounded reads · interleaving) ·
Gate 9 canary = the mock-agent e2e (hermetic) · Gate 10 N/A (no legacy ACP) ·
admission ceremony full (new crate).

🦋
