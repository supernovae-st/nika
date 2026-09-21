# nika-onboard — the onboarding surface (project bootstrap + stateless Compile)

> L4 · descended from the CLI authoring/bootstrap surface at the 15k
> prod-LOC wall (2026-07-12 · the `nika-display`/`nika-dap`/`nika-tmpl`
> precedents) — per D-2026-07-09-N1 this is the cli UNIT in a second
> member, named by parentage.

## Contract

The onboarding surfaces share one law (questions before writes · the human
keeps the hand · the proof inside the first minute):

- **`founding`** — `nika init`'s body: the briefs table (`briefs` — the
  scaffold bytes: AGENTS.md contract · per-client thin briefs · schema
  wiring), the recipe register (`recipes` — SETS over the embedded
  templates through the bootstrap-only `stamp`, ids explicit under the kebab
  law), the scripted path (`scripted_run` — file receipts and next commands),
  the canvas stamp (`nika.dag.theme` parsed-and-re-emitted into a
  CREATED settings.json, never string-spliced), and the trace cover
  (`gitignore` — adds-only: create when absent · one marked section
  appended when the human's file lacks it · never a duplicate, so a
  founded repo cannot commit its own `.nika/traces/` journals).
- **`wizard`** — the founding conversation on the clack rail (recipe ·
  model · canvas · agents), over any `BufRead`/`Write` pair.
- **`bootstrap`** — private init recipe/model prompts and stamping; no authoring
  intent router, creation command or first-workflow wizard.
- **`routing`** — read-only gallery discovery shared by MCP; it cannot author.

## Compile foundation

`compile` is a stateless in-memory authoring core behind the CLI creation door. CREATE accepts exact embedded skeleton names
and explicit request-local JSON answers. `hello` (also `01-hello`) takes the
embedded hello lesson through the same assembler with explicit `mock/echo`.
`with_workflow_id` names CREATE source explicitly; EDIT refuses this option.
Unsupported natural language returns Incomplete with an Unknown diagnostic and
no substitute workflow. A bounded EN/FR whole-clause grammar also composes
customer lookup, descriptive classification, draft and human-first refund.
This deterministic development grammar is not arbitrary natural-language understanding.

### Bounded support composition and explicit authoring

Support composition uses a private typed operation set and reusable structured
motif builders, not a support skeleton or YAML concatenation. Questions use
`model`, `const.customer_directory`, `const.refund_policy` and
`const.refund_endpoint`. The directory is an explicitly selected JSON file
mapping customer ids to records. A missing/malformed record fails, without
recovery that fabricates a customer. Required runtime inputs are `ticket`,
`customer_id`. Optional `refund_request` defaults to an empty object; a refund
request supplies its amount and currency explicitly.
The endpoint answer explicitly accepts a POST body containing
customer_id, amount and currency; its hostname alone supplies the requested
network boundary. Credentials are never invented. No named CRM integration is
silently replaced by the file lookup.
Rejected binding answers keep their stable question open beside the Missed
diagnostic: a glob directory, a bare model id (the answer must name
`<provider>/<model>`, which the host Check would otherwise refuse as
NIKA-PROVIDER), and an endpoint that is not a concrete URL, embeds credentials
in its authority or a credential-like query parameter, carries a fragment, or
uses cleartext `http` for anything but a loopback development host.

Classification returns descriptive category data, without inventing a queue
integration. Original customer/ticket facts and their source remain siblings
of generated prose. Declared claims must contain exact source anchors before
the gate can run. This proves anchor presence and preservation of original
data, not semantic truth or completeness of generated prose. Refund policy is
literal data, never parsed into an automatic eligibility rule. The blocking
human prompt shows that policy, the destination and the exact POST proposal.
The POST consumes the same proposal and the current prompt's affirmative
answer. There is no default, recovery or retry on the gate/effect. Source-only
Check does not prove destination idempotency, external business outcome or
approval freshness across executions; Run remains responsible for admission.

`AuthoringPolicy::new(model, max_tokens, timeout)` with
`CompileRequest::with_authoring_policy` permits one call through
`compile_with_provider(request, provider)`. The caller injects the existing
kernel `ProviderInferDyn` seam. Limits are 1..8192 output tokens, at most 120
seconds, and 32768 input-intent bytes. No retries occur. The provider returns a
closed private JSON semantic plan with exact intent excerpts, operation kinds,
one effect-policy enum and unresolved regions. `intent.clarification` asks for
a complete replacement request, explicitly superseding the earlier intent; the
caller must restate all work still wanted. A fragment cannot implicitly inherit
old operations, and stale contradictions are not appended to the new request.
A reusable program for one ticket per invocation is supported; durable schedules, subscriptions, polling and
cross-run callback/deduplication handling remain outside this slice.
Invalid plans, unsupported effects, contradictory policy and unknown work stay incomplete. The compiler
owns task identities, bindings, permits, emission and Check. Neither a source
excerpt nor model confidence is a proof of semantic equivalence. Independent
qualification is still required.

The default `compile` path never calls a provider; ambient keys never opt in.
Exact skeletons, EDIT and support clauses the bounded grammar resolves stay
deterministic even through the provider seam: an opted-in provider interprets
only what the grammar cannot, and a grammar-resolved outcome keeps generation 1.
The CLI opts in with `--authoring-model`, optional `--authoring-max-tokens`
(default 2048) and `--authoring-timeout` (default 30 seconds). Only then does
its adapter use the established environment credential/endpoint ladder.
Serve remains deterministic; it does not accept this authoring policy yet.

One authoring conversation is several requests of the same intent with more
answers each round. `CompileRequest::with_plan(plan)` replays the private plan
an earlier round produced (the exact `provenance.plan` value, which names its
`strategy`): the compiler skips reading, decision seats and generative
proposals and assembles that plan with the round's answers, so every answer
round reaches the same candidate with zero provider and zero seat calls
(`decision.route = ["replayed plan"]`, cognition `deterministicOnly`, no
authoring receipt). A record that does not parse, is not anchored in the intent
or still carries unknown work is a `recorded_plan` finding, never a candidate.
`intent_sha256` is the key a transport files the record under (typographic
apostrophes folded, as the reader sees the intent) and is recorded as
`provenance.decision.intent_sha256` on every general-path outcome. The CLI
records a settled plan under `.nika/compile/<sha>.plan.json` (a self-ignoring
directory carrying the plan, the engine version and the intent hash, never the
candidate or a key), replays it on any `--answer` round of the same intent
under the same engine, and reads the intent again under `--fresh`.

Deterministic outcomes retain the exact generation-1 wire shape. A provider
attempt emits generation 2 with cognition `explicitProvider` and an
`authoring` provenance receipt: model, calls, input/output token counts (null
when unreported), and elapsed milliseconds. No plan IR is public. Generation-1
clients must not consume generation 2 until they explicitly support it.
Authoring usage is not runtime execution proof or an asserted billing cost.
The generation-2 receipt records `sampling` with null `temperature` and `seed`
(omitted requests) and `effective: providerDefaultUnknown`. Provider defaults
are not known here: no deterministic or repeatable authoring claim follows.
Deterministic assembly and provider repeatability are separate properties.

For the local JSON-directory motif, Check's trifecta ingress leg (network/MCP
content) is absent: an empty `trifecta_mitigations` list is not a credited-gate
proof. The compiler tests the derived dependency edge and execution waves, exact
approved payload binding, and approval predicate separately. This does not prove
cross-run idempotency or freshness of externally supplied runtime answers.

Refund execution is optional per ticket: `inputs.refund_request` defaults to an
empty object and no human prompt or POST occurs in that case. A nonempty request
must contain exactly a positive `amount` and nonempty `currency`; partial data
fails before approval. Only customer id, amount and currency enter the POST;
the original ticket is retained as human-review context. Policy objects remain
review data, never an automatic eligibility program.

A conservative finite EN/FR sensitive-phrase backstop rejects recognized model
omissions and inserted approval over recognized automatic-refund instructions.
Approval-bypass phrases (`without approval`, `without asking`, `do not ask`,
`sans accord`, `ne pas demander`, prior or previous approval, `yesterday`,
`hier`) are matched as whole-word sequences, so `fichier` never reads as
`hier`. Such a phrase presupposes an effect: it vetoes an inserted human gate
and equally a plan that reports no effect at all. `automatic` wording alone
vetoes only an inserted gate, because it also names harmless automation.
Absence of recognized EN/FR vocabulary is inconclusive and never a veto.
This cannot prove arbitrary-language intent preservation: opted-in interpretation
remains probabilistic, and exact excerpt attribution is not semantic completeness.

EDIT requires the caller's explicit base source and offers two inputs:

- `CompileRequest::edit(base, "Set const.NAME to JSON_LITERAL")`, also allowing
  `Set const.NAME` followed by an answer to `const.NAME`.
- `CompileRequest::set_constant(base, name, literal_json)`, a structured operation
  for adapters. The bare name and JSON literal are separate arguments; no
  natural-language prompt is synthesized.

Both lower to the same bounded constant-edit operation, existing-node mutation,
assembler, canonical parser and pure Check preview. Names contain only ASCII
letters, digits or underscores; empty, invalid or absent targets and malformed
JSON preserve the original source and remain Incomplete. The operation cannot
insert nodes or edit permits, tasks or nested paths. Payloads resembling policy
or instructions remain literal data; expression islands and root objects with
both `type` and `value` are refused. Existing typed declarations retain their
type, and a type-incompatible candidate remains Incomplete under Check.

Every answer door (CREATE answers and the text, answer and structured EDIT
inputs) shares one guarded literal path. An integer answer outside the canonical
reader's exact `i64` range is refused from the answer's own text, at any depth,
before a decoder can round it; a whole f64 would otherwise satisfy even a
`type: integer` constant. Fraction and exponent answers are floats under the
f64 contract and quoted digits stay text. This is a refusal, not arbitrary
precision.

Emission must preserve the intended canonical literal projection; decoder
ambiguity or precision loss refuses the edit and retains the original source.
All unrelated semantic values survive accepted edits. EDIT replaces only the
target literal's source range for single-line scalars and flow collections;
comments, formatting, line endings and all bytes outside that range survive.
Typed constants retain the declaration around their `value`. Semantic no-ops
retain the exact original source. Block collections and multi-line scalars are
refused without changes; this is a bounded editor, not a general YAML CST.
The emitted candidate must agree with both literal readers, so an accepted
edit cannot introduce decoder drift that blocks a later unrelated edit.
The replacement token is compact JSON. DEL, the C1 controls U+0080 to U+009F
(NEL included) and the noncharacters U+FFFE and U+FFFF are written as `\uXXXX`
escapes in strings and object keys, because a YAML 1.1 reader rejects or folds
them when raw. The escape only proposes a token: the two-reader comparison
still decides every candidate, so such literals stay editable and an
unreadable one is refused. A value written by omission (`key:` with nothing
after it) is refused explicitly, since the parser marks it at the next token
and never at the target; a written `~` or `null` remains editable. A typed
declaration whose `value` is omitted or null never reaches that refusal: such
a base already fails pure Check, so the request stays Incomplete with the
source unchanged.

Scope limitations of this bounded editor, not a verified general
source-preserving edit contract:

- The replaced range is the whole literal. Comments INSIDE a replaced
  multi-line flow collection belong to that range and disappear; the outcome
  is still Ready and carries no diagnostic for them. Bytes outside the range
  survive by construction (prefix, token, suffix); no literal reader sees
  comments, so that property is demonstrated by tests, not checked at run time.
- CREATE's slot assembler retains its existing guarded re-emission behavior
  and emits block forms for multi-line strings and collections. EDIT can
  therefore refuse a constant that CREATE itself just wrote. There is no
  fallback whole-document re-emission: the guarantee is untouched bytes
  outside the literal, also for a comment-free source. Block editing is out
  of this slice.

SLOT values remain mandatory questions. Source-only Check is not environment
resolution or Run admission.

The synchronous `compile` performs no file access, credential probes, provider calls or execution,
and keeps no session state. The application owns base revision selection, CAS
and materialization. This API supplies a constant-edit seam, not a Graph
implementation, general patch language or full natural-language authoring.

Private pattern-facet derivation (#1666) lives beside Compile. It parses with
the same schema door, reads Check `needed` as the membrane, and never text-scans
comments. Task ids come from the `tasks:` map only. It is not a public YAML key,
a fifth verb, or an SDK noun, and it does not change CREATE/EDIT.

Candidate retrieval (`compile::retrieve` · `compile::retrieve_by_ops`) is the
lexical recall floor beside it: BM25 over the embedded skeletons and a compact
projection of the spec's pattern-family inventory (`assets/pattern_families.json`,
Apache-2.0 development knowledge), with French/English alias tokens and a light
stemmer. A hit is a candidate to read, never a selection, a semantic truth or
authority; `tests/compile_retrieval.rs` prints and floors recall@1/@5 on seen
example intents and unseen paraphrases.

### The one machine document (`compile_version` 1)

`outcome_document(&CompileOutcome) -> serde_json::Value` is the single machine
projection of an outcome, and `COMPILE_WIRE_VERSION` its generation. Every
transport prints it: `nika compile --json` (which adds only `written`, the one
fact that adapter owns) and Serve's `POST /v1/compile` (#1670). `CompileStatus::word`
and `DiagnosticKind::word` are its status and disposition words. A transport
never re-projects an outcome, so the doors cannot drift on status words,
question shapes, the requested boundary or the Check preview. This renders
typed results: it is not a second IR and it carries no authoring semantics.

`tests/fixtures/compile_parity_v1.json` holds identical requests for every
door: a native recipe for the core and the CLI, and the verbatim HTTP body for
Serve. Each door's test asserts that it prints this core's document for the
same request. An empty `expect` means door-versus-core equality is the whole
claim; no case asserts more than the foundation suite already proves.

## The injected seams

The composition root (`nika-cli`) owns what proving and wiring MEAN;
this crate converses and scaffolds:

- `Audit` — `&dyn Fn(&str) -> Outcome` · the check ladder
  (`nika check <path>` at the root · a stub in tests).
- `Wire` — `&dyn Fn(&str, &str) -> Outcome` · the MCP wiring
  (`nika wire <client>` at the root); the wizard speaks client WORDS,
  the root resolves them on its `WireTarget` register.

`Outcome { text, code }` mirrors the root's `VerbOutput` (kept local so
the descent adds zero reverse dependency); `codes` mirrors the spec §4
exit vocabulary.

## Invariants

- **Own-corpus law (#261), inherited**: every workflow any recipe can
  scaffold is an embedded template VERBATIM through `stamp` — the
  per-recipe ratchet parses AND checks every scaffold clean (validated through
  `nika-schema`; Compile also uses that parser in production).
- **Questions before writes**: cancel at any wizard beat = « nothing
  written », honestly (PTY-pinned at the root).
- **Readable sober registers**: file rows keep the `✔ created …` / `· skipped …`
  prefixes. Created briefs explain their purpose; the team block teaches Git
  and offers `nika init --project-file --yes` before the next commands.
  `NIKA.md` is the human guide, leaving an existing `README.md` untouched.
- **Wire results are outcomes**: both doors preserve each client's receipt
  and propagate a failed wiring code. A failed wizard never emits the ready
  panel. Client names round-trip through the root's live registry without
  substituting a broader target.
- **Project hooks**: Cursor and Claude project settings point at the same
  canonical kit scripts, copied into each client's `hooks-nika/` directory.
  Claude commands anchor at the quoted `CLAUDE_PROJECT_DIR`; existing settings
  are skipped under the normal law. A declared hook is not proof a client
  has loaded it.
- **No CLI framework below the root**: `CanvasTheme` stays a plain enum
  here; the root mirrors it as its clap `ValueEnum`.

## Metrics

Live numbers come from the projector — `scripts/crate-metrics.sh
nika-onboard` (no hardcoded LOC anchor in this spec; nothing to drift).
