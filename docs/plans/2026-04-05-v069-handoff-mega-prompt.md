# MEGA PROMPT: v0.69.0 Handoff — What's Left

> **Mode**: Full autonomy, multi-session execution
> **Codebase**: nika v0.69.0 | ~395K LOC | 15 crates | 9,975 tests | 62 builtin tools | 63 transforms
> **v0.69.0**: TAGGED + PUSHED. 13 commits. 11 transforms, 5 commands, when: fix, --resume, audit fixes.
> **Philosophy**: v0 = zero dead code, zero backward compat, zero mercy on tech debt
> **Launch**: May 5, 2026 — ~30 days from now

---

## WHAT'S DONE (v0.69.0 — DO NOT REDO)

### Transforms (52 → 63, +11)
- `replace(from, to)` — string literal replacement
- `truncate(N)` — unicode-safe string truncation
- `add` — sum numbers / concat strings / concat arrays (null on empty)
- `min` / `max` — numeric array extremes (NaN-safe, null-propagating)
- `not` — boolean negation
- `min_by(field)` / `max_by(field)` — object array extremes (dot-path)
- `sum` — alias for add on numeric arrays
- `avg` — arithmetic mean (null-safe, skips nulls in count)
- `has(key)` — object key introspection
- `f64_to_json_number()` helper — NaN/Infinity → null (shared by all aggregation)

### CLI (5 new commands + 1 flag)
- `nika version` — version, channel, build info
- `nika test` — run with mock provider, --golden file (stub), --update-snapshot (stub)
- `nika lint` — 8 best-practice rules (L001-L070)
- `nika env` — unified debug view (version, providers, paths, engine stats)
- `nika graph` — top-level alias for `nika workflow graph`
- `--resume` on `nika run` — reads latest trace, pre-populates datastore

### Bug fixes
- `when:` conditional — analyzer was setting `None` instead of copying from raw AST
- `when:` added to JSON schema
- `--resume` hardened: workflow hash verification, 50MB size limit, warns on 0 results
- NaN guard in min/max/add/avg via `f64_to_json_number()`
- Empty array `add` returns null (was 0, inconsistent with min/max/avg)

### DX
- AGENTS.md updated (63 transforms, new commands)
- nika.md updated (63 transforms, new categories)
- nika env shows 63 transforms, 62 tools

---

## WHAT'S LEFT — 4 Sprints

### Sprint A: `nika test` golden file + `nika lint` polish (2h)

**Context**: Audit found MEDIUM-2: golden file comparison is a stub that always reports OK.
This is the #1 priority — users who rely on `nika test --golden` in CI will get false confidence.

**Tasks**:

1. **Implement golden file comparison in `nika test`**
   - File: `tools/nika/src/main.rs:test_workflow()`
   - The function needs to capture workflow output (currently `run_workflow` returns `()`)
   - Option A: Pass `--output /tmp/test-output.json` to `run_workflow`, then compare
   - Option B: Access `runner.datastore().iter_results()` after run (but runner is inside run_workflow)
   - Best approach: Add an `output_capture: Option<&mut Value>` param to `run_workflow`
   - Then compare captured output vs golden file with `serde_json::Value` equality
   - On mismatch, show a diff (field by field, like insta)

2. **Implement `--update-snapshot` mode**
   - Write captured output to golden file path
   - First run creates snapshot, subsequent runs compare

3. **Add lint rule L080: missing `when:` on expensive tasks after conditional**
   - If a task has `when:` and downstream tasks don't, warn about unconditional expensive ops

4. **Add lint rule L090: duplicate task names**
   - Already caught by analyzer, but lint should show it in a friendlier way

5. **Add tests for `lint_workflow()`**
   - Unit tests with crafted `AnalyzedWorkflow` structs
   - Test each rule fires and doesn't fire correctly
   - File: `tools/nika-cli/src/lint.rs` — add `#[cfg(test)] mod tests`

**TDD sequence**:
```
1. Write test: lint_detects_missing_retry → RED
2. Create mock AnalyzedWorkflow with fetch task, no retry → GREEN
3. Write test: lint_clean_on_good_workflow → RED
4. Create mock workflow with all good practices → GREEN
5. Write test: golden_file_mismatch_detected → RED
6. Implement golden comparison → GREEN
```

**Commit**: `fix(cli): implement golden file comparison in nika test + 2 lint rules`

---

### Sprint B: Serve V4 — batch endpoint + job tags (3h)

**Context**: `nika serve` is solid V3 (Axum, SSE, embedded executor, Prometheus).
V4 adds quality-of-life features for production deployments.

**Architecture** (from 5-agent research):
- Framework: Axum (already used)
- Auth: Single bearer token (keep for now, multi-tenant deferred)
- Storage: SQLite via nika-storage (keep, PostgreSQL deferred)
- 12 existing endpoints at `/v1/*`

**Tasks**:

1. **Batch submission endpoint: `POST /v1/batch/run`**
   - File: `tools/nika-serve/src/routes/workflows.rs`
   - Accepts JSON array of workflow submissions
   - Returns array of job IDs
   - Reuses existing `submit_job()` internally (loop, not transaction)
   - Rate limit: same per-token limit applies per-request, not per-job
   - Max batch size: 50 jobs (configurable via `NIKA_SERVE_BATCH_MAX`)

   ```rust
   async fn batch_run(
       State(state): State<AppState>,
       Json(requests): Json<Vec<RunRequest>>,
   ) -> Result<Json<Vec<RunResponse>>, ServeError> {
       if requests.len() > state.config.max_batch_size {
           return Err(ServeError::bad_request("batch too large"));
       }
       let mut responses = Vec::with_capacity(requests.len());
       for req in requests {
           responses.push(submit_job(&state, req).await?);
       }
       Ok(Json(responses))
   }
   ```

2. **Job tags/labels: metadata on job submission**
   - Add `tags: HashMap<String, String>` to `RunRequest`
   - Store in SQLite jobs table (new `tags TEXT` column, JSON serialized)
   - Return in `GET /v1/status/{id}` response
   - Filter by tag: `GET /v1/jobs?tag=env:staging`

3. **Job list endpoint: `GET /v1/jobs`**
   - File: `tools/nika-serve/src/routes/workflows.rs`
   - Returns paginated job list (limit/offset)
   - Filter by status, tag, workflow name
   - Sort by created_at desc (default)

4. **Update OpenAPI spec**
   - File: `tools/nika-serve/src/routes/mod.rs` (or wherever OpenAPI is generated)
   - Add new endpoints to spec

5. **Tests**
   - 5+ tests for batch (empty, single, max, over-max, mixed success/failure)
   - 3+ tests for tags (set, query, filter)
   - File: existing test module in workflows.rs

**TDD sequence**:
```
1. Test: batch_empty_array_returns_empty → RED
2. Implement batch endpoint → GREEN
3. Test: batch_over_max_returns_error → RED
4. Add size check → GREEN
5. Test: tags_stored_and_returned → RED
6. Add tags column + serialization → GREEN
7. Test: job_list_with_tag_filter → RED
8. Implement job list endpoint → GREEN
```

**Commits**: 2 commits
- `feat(serve): add POST /v1/batch/run endpoint`
- `feat(serve): add job tags + GET /v1/jobs list endpoint`

---

### Sprint C: `nika eval` framework (4h)

**Context**: No competitor has cross-provider evaluation built into the CLI.
This is a competitive moat feature — `nika eval workflow.nika.yaml --dataset golden.json`.

**Design**:

```yaml
# golden.json — evaluation dataset
[
  {
    "inputs": { "topic": "quantum computing" },
    "expected": {
      "tasks": {
        "research": { "output_contains": "qubit" },
        "summarize": { "output_min_words": 50, "output_max_words": 500 }
      }
    }
  },
  {
    "inputs": { "topic": "machine learning" },
    "expected": {
      "tasks": {
        "research": { "output_contains": "neural" }
      }
    }
  }
]
```

```bash
nika eval workflow.nika.yaml --dataset golden.json --provider mock
# Runs workflow once per dataset entry
# Validates task outputs against expected
# Reports: PASS/FAIL per entry, aggregated score
```

**Tasks**:

1. **Add `Eval` command to CLI**
   - File: `tools/nika/src/main.rs`
   - Args: `file`, `--dataset`, `--provider`, `--parallel`

2. **Create eval runner in nika-cli**
   - File: `tools/nika-cli/src/eval.rs`
   - Parse dataset JSON
   - Run workflow N times with different inputs
   - Collect results, validate against expected
   - Assertions: `output_contains`, `output_min_words`, `output_max_words`, `output_matches_schema`

3. **Formatted output**
   - Table: entry | status | details
   - Summary: X/N passed, Y/N failed
   - JSON output mode: `--format json`

4. **Tests**
   - Mock workflow + mock dataset → all pass
   - Mock workflow + failing assertion → correct failure report

**Commits**: 2 commits
- `feat(cli): add nika eval — workflow evaluation framework`
- `test(cli): add eval runner tests`

---

### Sprint D: DX sync + final polish (1h)

**Context**: Every release needs DX files updated. This sprint catches everything.

**Tasks**:

1. **Update CHANGELOG.md**
   - All changes since v0.68.2
   - Group by: Added, Changed, Fixed, Security

2. **Update tools/nika/CLAUDE.md source tree**
   - Reflect lint.rs, eval.rs additions
   - Update transform/tool counts

3. **Update nika.md rules file**
   - Add `when:` to task fields documentation
   - Add new CLI commands to validation section
   - Update error codes if new ones added

4. **Update AGENTS.md**
   - Transform count, command count, test count

5. **Verify showcases**
   - `nika check examples/showcase/**/*.nika.yaml` — all valid
   - `nika lint examples/showcase/01-hello-world/hello.nika.yaml` — works

6. **Memory update**
   - Update `project_grand_nettoyage_strategy.md` with session results
   - Create session memory with commit list

**Commit**: `docs(dx): sync all DX files for v0.69.1`

---

## WHAT NOT TO DO (reinforced)

- No new verbs (5 verbs are sacred)
- No Egghead/memory system (post-launch)
- No TUI redesign (88K LOC, mature)
- No PostgreSQL backend for serve (SQLite WAL suffices)
- No WebSocket (SSE works fine)
- No `on_error:` fallback routing (needs design brainstorm first)
- No `nika diff` (low priority, git diff suffices)
- No multi-tenant auth / RBAC (post-launch)
- No `nika upgrade` / self-update (Homebrew handles this)

---

## AUDIT FINDINGS NOT YET FIXED (from v0.69 audit agents)

### Transform audit (rust-pro) — remaining MEDIUM findings:
- **MEDIUM-2**: `sum` is a leaky alias — also concatenates strings/arrays. Consider restricting to numbers only, or rename doc comment.
- **MEDIUM-3**: `min_by`/`max_by` silently skip items where field is missing or non-numeric. Consider debug logging when items are skipped.

### CLI audit (rust-pro) — remaining MEDIUM findings:
- **MEDIUM-3**: `Lint` missing from `should_skip_auto_setup` comment at main.rs:1850. Trivial fix.
- **MEDIUM-4**: L060 lint rule unconditionally exempts last task. Could cause false negatives if the last task is actually unused.

**Recommendation**: Fix all 4 in Sprint D as quick wins (5 min each).

---

## COMPETITIVE POSITIONING (from 5-agent research)

### Nika is AHEAD on:
1. 5-layer structured output defense (cross-provider, no competitor has this)
2. Single binary distribution (zero deps)
3. CAS + 62 builtin tools (media pipeline integrated)
4. Security-first exec (blocklist, SSRF, `| shell` mandatory)
5. Integrated learning course (12 levels, 44 exercises, 115 showcases)

### v0.69 additions that close competitive gaps:
- `when:` conditional → matches LangGraph/n8n
- `--resume` → matches Prefect/Temporal
- `nika test` → matches Deno/Cargo test culture
- `nika lint` → matches `cargo clippy` model
- `nika eval` (Sprint C) → AHEAD of all competitors (unique)

### Still behind (acceptable for launch):
- Observability UI (LangSmith has this, we have TUI + traces)
- Scheduling/cron (systemd timers suffice for now)
- `on_error:` fallback routing (design needed)

---

## TIMELINE

```
v0.69.0 ✅ TAGGED (transforms, CLI, when:, --resume, audit fixes)

Sprint A   nika test golden + lint polish           ~2h
Sprint B   Serve V4: batch + tags + job list        ~3h
Sprint C   nika eval framework                      ~4h
Sprint D   DX sync + final polish                   ~1h
                                                    ────
                                                    ~10h total

v0.70.0    Tag after Sprint D

FEATURE FREEZE (hard this time)
Bug fixes only → May 5 LAUNCH 🚀
```

---

## COMMIT STRATEGY

```
type(scope): description

Co-Authored-By: Nika 🦋 <nika@supernovae.studio>
```

1 fix = 1 commit. Tests verts. Clippy zéro. Push HTTPS.

---

## SKILLS & AGENTS

| Skill | When |
|-------|------|
| `test-driven-development` | All code changes |
| `verification-before-completion` | Before every commit |
| `systematic-debugging` | When tests break |
| `rust` | All Rust code |

| Agent | When |
|-------|------|
| `rust-pro` | Code review after each sprint |
| `rust-security` | Review serve V4 changes |
| `Explore` | Find existing patterns before implementing |

---

## VERIFICATION CHECKLIST (before v0.70 tag)

- [ ] `cargo test --workspace --lib --exclude nika-py` — 0 failures
- [ ] `cargo clippy --workspace -- -D warnings` — 0 warnings
- [ ] `cargo fmt --all --check` — clean
- [ ] `nika check examples/showcase/**/*.nika.yaml` — all valid
- [ ] `nika lint examples/showcase/01-hello-world/hello.nika.yaml` — works
- [ ] `nika version` — shows correct version
- [ ] `nika env` — shows correct transform/tool counts
- [ ] `nika test examples/showcase/01-hello-world/hello.nika.yaml` — PASS
- [ ] DX files: nika.md, CLAUDE.md, AGENTS.md accurate
- [ ] CHANGELOG entry complete
- [ ] Memory updated
