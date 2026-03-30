# Overnight Mega Plan — 6 Phases, 50+ Workflows, Fix Everything

> **Budget:** $20-30 API | **Providers:** OpenAI, xAI, Gemini (Anthropic: no credits)
> **Mode:** Fix + rapport — every bug fixed immediately, committed, pushed
> **Output:** Mega handoff prompts for future sessions

## Phase 1: Critical Blockers (~1h)
> Fix the 6 P0 issues that block everything else

1. Add YAML syntax example to orchestrator system prompt (orchestrate.rs)
2. Add 9 missing fields to JSON schema (schemas/*.json)
3. Validate confidence_target bounds (0.0-1.0)
4. Fix nika:complete schema (only result required)
5. Redact secrets in tracing::warn (exec.rs:83, security.rs:258,377)
6. Recursive JSON redaction in to_value_redacted()

## Phase 2: Workflow Factory (~3h)
> Create and run 50+ workflows testing EVERY feature combination

### Category A: Structured Output (10 workflows)
- A01: Basic schema (object, required fields) — OpenAI
- A02: Nested objects (3 levels deep) — xAI
- A03: Array of objects with constraints — Gemini
- A04: Enum + const + default values — OpenAI
- A05: from_example (inline JSON) — xAI
- A06: from_example (file reference) — Gemini
- A07: Structured + for_each (parallel schema validation) — OpenAI
- A08: Structured + retry (force L3 with bad prompt) — xAI
- A09: Structured + repair_model (cheaper model repairs) — Gemini
- A10: Multi-provider same schema (parity test) — ALL 3

### Category B: Agent Verb (8 workflows)
- B01: Simple agent with nika:log + nika:complete — OpenAI
- B02: Agent with MCP tools (if available) — xAI
- B03: Agent with guardrails (length + regex) — Gemini
- B04: Agent with limits (max_cost, max_turns) — OpenAI
- B05: Agent with skills injection — xAI
- B06: Agent spawn (depth_limit=2, nested agents) — Gemini
- B07: Agent with extended_thinking (Claude only → fallback test)
- B08: Agent completion modes (explicit vs natural vs pattern) — ALL

### Category C: Fetch All 9 Modes (9 workflows)
- C01: fetch extract: markdown (real blog URL)
- C02: fetch extract: article (news site)
- C03: fetch extract: metadata (OG tags from real site)
- C04: fetch extract: links (real page, count links)
- C05: fetch extract: jsonpath (public JSON API)
- C06: fetch extract: feed (RSS feed)
- C07: fetch extract: text + selector (CSS selector)
- C08: fetch extract: llm_txt (site with /.well-known/llm.txt)
- C09: fetch response: full (headers + status + body)

### Category D: for_each + DAG Patterns (8 workflows)
- D01: Linear chain (5 tasks sequential)
- D02: Diamond pattern (fan-out + merge)
- D03: for_each with concurrency=3, fail_fast=true
- D04: for_each with concurrency=1, fail_fast=false
- D05: for_each + structured output (validate each item)
- D06: Nested deps (A→B→C→D→E with data flow)
- D07: for_each + artifact (write each result to file)
- D08: include: (partial workflow import)

### Category E: Exec + Invoke + Builtins (8 workflows)
- E01: exec shell: true with pipes
- E02: exec with env vars and cwd
- E03: exec → infer chain (exec output → LLM analysis)
- E04: invoke nika:glob + nika:read (file discovery)
- E05: invoke nika:log + nika:assert (validation)
- E06: invoke nika:dimensions + nika:thumbhash (media)
- E07: exec + fetch chain (curl → parse → analyze)
- E08: Multi-verb workflow (exec + fetch + infer + invoke)

### Category F: Media Pipeline (5 workflows)
- F01: nika:import → nika:dimensions → nika:thumbnail
- F02: nika:pipeline (import → resize → convert → optimize)
- F03: nika:chart (bar + line + pie from JSON data)
- F04: fetch binary → artifact format: binary
- F05: nika:import → nika:dominant_color + nika:thumbhash

### Category G: Security Tests (5 workflows)
- G01: SSRF attempt (fetch blocked IP) — must fail
- G02: Path traversal (nika:read ../../etc/passwd) — must fail
- G03: Command injection (exec with metacharacters) — must fail
- G04: Shell blocklist (exec with sudo, rm -rf) — must fail
- G05: $env binding (verify secrets not leaked in output)

### Category H: Real-World Use Cases (7 workflows)
- H01: Blog scraper → summarizer → translator
- H02: API data → structured analysis → report
- H03: Multi-source research (3 URLs → merge → report)
- H04: Code review pipeline (read file → analyze → suggestions)
- H05: SEO audit (fetch metadata → analyze → score)
- H06: Content pipeline (topic → outline → sections → merge)
- H07: Data transformation (fetch JSON API → transform → artifact)

## Phase 3: Bug Hunt + Fix (~2h)
> Run all 50+ workflows, analyze failures, fix immediately

For EACH workflow:
1. `nika check` — validate syntax
2. `nika run --no-live` — execute with real API
3. Check exit code (0 = pass, 1 = fail)
4. If fail: analyze error, identify root cause, fix code, commit
5. Re-run to verify fix
6. Log: workflow, provider, result, cost, duration, bugs found

## Phase 4: Security Hardening (~1h)
> Apply all security fixes from audit

1. Redact tracing::warn in exec.rs and security.rs
2. Recursive JSON redaction
3. Extend BINDING_RE to {{context.*}}
4. Expand SECRET_RE (Stripe, Twilio, DB URIs)
5. Verify SSRF protections with real tests
6. Verify command blocklist with real tests

## Phase 5: Dead Code Nuke (~1h)
> Clean up everything flagged by audits

1. Remove MaxTurnsReached dead variant (or wire it)
2. Clean up 14 #[allow(dead_code)] annotations
3. Remove TODO stubs that won't be implemented
4. Clean up wave2 test comments
5. Remove unused imports/features
6. Run cargo clippy --workspace -- -D warnings
7. Run cargo machete (unused deps)

## Phase 6: Compile Mega Handoffs (~1h)
> Create session-ready prompts for future work

### Handoff A: "Sprint Security" (~4h session)
- All security items with exact file:line references
- Test commands to verify each fix
- Priority order

### Handoff B: "Sprint Agent+Provider" (~8h session)
- max_tokens(8192) replacement plan
- Agent scope implementation spec
- LLM guardrails implementation spec
- 8 named presets specification

### Handoff C: "Sprint Runner+Perf" (~6h session)
- O(n^2) fix design
- Semaphore fix design
- EventLog ring buffer design
- Performance benchmarks before/after

### Handoff D: "Sprint Orchestrate" (~4h session)
- System prompt with YAML examples
- E2E integration test suite
- max_rounds/max_cost enforcement
- Confidence threshold enforcement

### Handoff E: "Sprint Polish+Launch" (~4h session)
- TUI ProviderName migration
- JSON schema sync (all 9 fields)
- Dockerfile update
- README update for v0.54
- Final cargo test --workspace --lib verification

---

## Success Criteria

- [ ] 50+ workflows created and validated
- [ ] 40+ workflows run successfully with real API calls
- [ ] Every failure analyzed and either fixed or documented
- [ ] All P0 security issues fixed
- [ ] Dead code cleaned up
- [ ] 5 mega handoff prompts ready for future sessions
- [ ] cargo test --workspace --lib passes (9000+ tests)
- [ ] All fixes committed with proper messages
