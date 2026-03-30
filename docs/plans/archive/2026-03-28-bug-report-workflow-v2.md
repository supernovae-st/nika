# Bug Report — Workflow v2.1 + Engine Issues

> Generated during GEO-SEO workflow v2 development session (2026-03-28)

---

## NIKA ENGINE BUGS

### ENG-001 🔴 CRITICAL: `retry:` silently ignored on infer/exec/invoke/agent verbs

**Symptom:** Parser emits `WARN: retry: config is only supported on fetch: tasks — ignored for this verb` for every non-fetch task with retry config.

**Impact:** ALL retry configs on infer/exec/invoke/agent tasks do NOTHING. This affects:
- `check_robots` (infer) — no retry on LLM failures
- `discovery_synthesis` (infer) — no retry
- `report_per_language` (infer) — no retry
- `json_report_per_language` (infer) — no retry
- `global_report` (infer) — no retry
- `global_json_report` (infer) — no retry
- `parse_sitemap` (exec) — no retry
- `geo_score_passages` (invoke MCP) — no retry
- `audit_structured_data` (invoke MCP) — no retry

**Root cause:** In `nika-core/src/ast/analyzer/analyze.rs`, retry validation restricts to fetch verb only.

**Fix needed:** Extend retry support to ALL verbs. LLM API calls fail transiently (429, 500, timeout). MCP calls fail if server restarts. Exec commands fail on transient IO.

**Workaround:** None. Users think retry is active but it's silently dropped.

---

### ENG-002 🟠 HIGH: `artifact: path:` with templates warns at parse time

**Symptom:** `WARN: Failed to parse artifact: config, ignoring` for tasks with dynamic paths like `reports/{{with.lang.lang}}/report.md`.

**Impact:** Artifacts with template paths in `for_each` loops emit confusing warnings. The artifacts may or may not work at runtime — unclear if the warning means "skipped" or "deferred to runtime".

**Root cause:** Static analyzer tries to parse artifact config before template resolution. Templates in `artifact.path` can't be resolved at parse time inside `for_each` loops.

**Fix needed:** Either suppress the warning for template paths, or resolve templates at runtime and validate then.

---

### ENG-003 🟠 HIGH: `provider:` task-level override unclear in dry-run

**Symptom:** Dry-run shows `provider=openai model=gpt-4o-mini` for ALL tasks, even tasks with `provider: gemini` and `model: "{{inputs.creative_model}}"`.

**Impact:** Can't verify from dry-run output that multi-provider routing works. Tasks in `10-landing-page.nika.yaml` with `provider: gemini` might not actually use Gemini at runtime.

**Root cause:** Dry-run display may not resolve task-level provider/model overrides. The runtime might handle it correctly, but the dry-run output is misleading.

**Fix needed:** Dry-run should show the resolved provider+model per task, not just the workflow default.

---

### ENG-004 🟡 MEDIUM: `model:` template resolution unknown status in v0.50

**Symptom:** Commit `0e3797518` claims to fix model template resolution, but dry-run still shows `model=gpt-4o-mini` for tasks with `model: "{{inputs.deep_model}}"`.

**Impact:** Unclear if `model: "{{inputs.deep_model}}"` resolves to "gpt-4o" at runtime or stays as literal string. If literal, all "deep" tasks run on gpt-4o-mini instead of gpt-4o.

**Fix needed:** Verify at runtime. If model templates don't resolve, all deep analysis tasks (sonnet_geo_analysis, report_per_language, global_report, agents) use the wrong model.

---

### ENG-005 🟡 MEDIUM: `extended_thinking:` on `infer:` verb warning

**Symptom:** Commit `1ed73a1bc` says "warn that extended_thinking on infer: verb is not supported" — meaning extended_thinking only works on `agent:` verb, not `infer:`.

**Impact:** Our `report_per_language` and `global_report` tasks use `infer:` with `extended_thinking: true`. This will be IGNORED (not just degraded for non-Claude, but not supported at all on infer verb).

**Fix needed:** Either support extended_thinking on infer verb, or document clearly that it's agent-only. Our workflow should remove it from infer tasks if unsupported.

---

## WORKFLOW BUGS

### WF-001 🔴 CRITICAL: All retry configs are no-ops (see ENG-001)

Every `retry:` block on non-fetch tasks does nothing. If the OpenAI API returns 429 or times out, the task fails immediately with no retry.

**Affected tasks:** 9 tasks across 5 partials.

---

### WF-002 🟠 HIGH: `og_thumbnails` output never consumed

**Where:** `08-media.nika.yaml` task `og_thumbnails`

**Issue:** Generates WebP thumbnails via `nika:pipeline` but no downstream task uses `$og_thumbnails`. The thumbnails are produced but never referenced in reports or landing page.

**Fix:** Wire `$og_thumbnails` into `generate_landing_html` or remove the task.

---

### WF-003 🟠 HIGH: `fetch_og_images` fetches empty URLs

**Where:** `08-media.nika.yaml` task `fetch_og_images`

**Issue:** Iterates ALL `$extract_page_metadata` results. Pages without `og:image` have `og_image: null`. Fetch verb receives empty URL → fails.

**Impact:** Lots of failed iterations (noise). `fail_fast: false` continues but pollutes the output array with errors.

**Fix:** Filter metadata results before for_each, or add null check in fetch URL.

---

### WF-004 🟠 HIGH: QR Code AI API key hardcoded in YAML

**Where:** `08-media.nika.yaml` line with `x-api-key: "qrc_1rMb9ZURymsrhtBWLccd1699177588260"`

**Issue:** API key in plaintext in workflow file. If committed to public repo, key is exposed.

**Fix:** Use `$env.QRCODE_AI_API_KEY` or Nika secrets system. Verify that `$env.*` works in `headers:` fields.

---

### WF-005 🟠 HIGH: `generate_landing_html` prompt overflow risk

**Where:** `10-landing-page.nika.yaml` task `generate_landing_html`

**Issue:** Injects `{{with.json_data | to_json}}` (full global JSON report) + `{{with.md_report}}` (full markdown report) + multiple media data arrays into a single prompt. For a site with 50+ pages, this could be 30-50K tokens of input.

**Impact:** Context window overflow, or truncated output (max_tokens: 8000 may not be enough for a complete HTML page).

**Fix:** Pass summaries instead of full data. Or split into 2 tasks: one for data aggregation, one for HTML generation.

---

### WF-006 🟡 MEDIUM: `extended_thinking: true` on infer tasks (see ENG-005)

**Where:** `09-reports.nika.yaml` — `report_per_language` and `global_report`

**Issue:** These are `infer:` tasks with `extended_thinking: true`. If ENG-005 is correct (infer doesn't support extended_thinking), this flag does nothing.

**Fix:** Remove from infer tasks, or move report generation to agent verb.

---

### WF-007 🟡 MEDIUM: `generate_dashboard_svg` and `generate_report_css` use `provider: gemini`

**Where:** `10-landing-page.nika.yaml`

**Issue:** These tasks specify `provider: gemini` and `model: "{{inputs.creative_model}}"`. Two unknowns:
1. Does Nika resolve `provider:` at task level when it differs from workflow default?
2. Is the Gemini API key configured? (`GEMINI_API_KEY` env var)

**Impact:** If Gemini isn't configured, these tasks fail silently or crash.

**Fix:** Add Gemini to the provider check in preflight. Or use `nika provider list` to verify.

---

### WF-008 🟡 MEDIUM: `check_llms_txt` and `check_feed` may 404 without resilience

**Where:** `01-discovery.nika.yaml`

**Issue:** Fetch to `/.well-known/llms.txt` and `/feed` will 404 on most sites. Without `response: full`, a 404 causes task failure.

**Impact:** If these tasks fail, `discovery_synthesis` is blocked (depends_on both).

**Fix:** Add `response: full` to both tasks so 404 is data, not error. Or add `fail_fast: false` behavior.

---

### WF-009 🟡 MEDIUM: `parse_sitemap.py` doesn't follow sitemap index

**Where:** `scripts/parse_sitemap.py`

**Issue:** When the sitemap is a sitemap index (type: "index"), the script returns `pages: []` and a list of sub-sitemap URLs. It does NOT fetch and parse the sub-sitemaps.

**Impact:** For qrcode-ai.com (which uses a sitemap index), we get 0 pages from the sitemap parser. All pages must come from hreflang + LLM inference.

**Fix:** Add recursive fetching in `parse_sitemap.py` — follow sub-sitemap URLs and aggregate pages.

---

### WF-010 🟢 LOW: Dead-end tasks still exist

**Where:** `08-media.nika.yaml`

**Issue:** `og_thumbnails` output is not consumed (WF-002 above). All other formerly dead-end tasks are now wired.

---

## FEATURES NOT IMPLEMENTED (honest list)

| Feature | Why not | Should we? |
|---------|---------|-----------|
| `nika:provenance` (C2PA sign) | Complexity + not critical for MVP | YES — enterprise value |
| `nika:thumbhash` | Nice-to-have for landing page UX | YES — modern pattern |
| `nika:verify` | Depends on provenance | YES — if provenance added |
| `nika:css_select` standalone | Already using extract_links/metadata | MAYBE — for breadcrumb audit |
| `agents: from:` presets | Only 2 agents, not enough duplication | NO — YAGNI |
| `stop_sequences:` | Agents use explicit completion | MAYBE — safety net |
| `agent: mcp: [geo]` | Implicit via workflow config | YES — clarity |
| `extract: text` with selector | Using html_to_md instead | MAYBE — perf optimization |
| Internal link graph (flatten+unique) | High value but complex | YES — for v3 |
| `nika:optimize` on charts | Marginal savings | NO — YAGNI |
| `nika:strip` standalone | Already in pipeline | NO — redundant |
| `nika:convert` WebP for dashboard | Marginal savings | NO — YAGNI |

---

## PRIORITY FIX ORDER

1. **ENG-001** — retry on all verbs (engine fix, critical)
2. **WF-008** — check_llms_txt/check_feed 404 resilience (response: full)
3. **WF-009** — parse_sitemap recursive fetch for sitemap index
4. **WF-003** — filter empty og_image URLs before fetch
5. **WF-004** — API key via env var
6. **WF-005** — landing page prompt overflow (summarize data)
7. **ENG-005** — clarify extended_thinking support on infer verb
8. **WF-006** — remove extended_thinking from infer tasks if unsupported
9. **ENG-002** — artifact template path warnings
10. **ENG-003** — dry-run multi-provider display
