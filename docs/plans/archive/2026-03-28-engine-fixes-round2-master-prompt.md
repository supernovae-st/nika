# Engine Fixes Round 2 + Workflow Hardening — Master Prompt

> **Copy this entire prompt into a fresh Claude Code session at `/Users/thibaut/dev/supernovae/nika/tools`**

---

## CONTEXT

Tu prends le relais d'une session precedente qui a fait 64 commits de fixes engine (structured output, include rename, model templates, extended_thinking). Le code est maintenant v0.50.0.

Un workflow GEO-SEO audit (54 tasks, 174 edges, 17 layers) a ete cree a `/Users/thibaut/Desktop/aaayayaa/` pour stress-tester Nika. Pendant le developpement, on a decouvert des bugs supplementaires. Ce prompt documente TOUT ce qui reste a fixer.

**Approche:** TDD strict (RED → GREEN → REFACTOR). `cargo test --workspace --lib` toujours avec `--lib`. 1 fix = 1 commit.

**Bug report complet:** `docs/plans/2026-03-28-bug-report-workflow-v2.md`
**Plan precedent:** `docs/plans/2026-03-27-structured-output-and-engine-fixes.md`

---

## PHASE 0: RECHERCHE OBLIGATOIRE

Avant TOUT code, lance ces recherches:

### 0.1 — Verify les fixes deja faites

```bash
cd /Users/thibaut/dev/supernovae/nika
git log --oneline --since="2026-03-27T18:00:00" | head -40

# Verifier les 5 fixes de la session precedente
git log --oneline --since="2026-03-27T18:00:00" | grep -i "structured\|include\|model\|extended\|cwd"
```

Confirmer que ces 5 fixes sont bien mergees:
- [ ] `ae64d8827` — OpenAI native response_format for structured output
- [ ] `db242f88d` — rename imports: → include:
- [ ] `0e3797518` — resolve templates in model, provider, selector
- [ ] `a87233c3a` — extended_thinking graceful degradation
- [ ] `eb246b0f9` — resolve templates in exec cwd

### 0.2 — Analyser le code actuel du retry system

```bash
# Le retry dans l'analyzer (ou il est restreint a fetch)
grep -n "retry" tools/nika-core/src/ast/analyzer/analyze.rs | head -20

# Le retry dans le runner (task execution)
grep -n "retry\|RetryConfig\|max_attempts\|delay_ms\|backoff" tools/nika-engine/src/runtime/runner.rs | head -30

# Comment fetch: utilise retry
grep -n "retry\|RetryConfig" tools/nika-engine/src/runtime/executor/fetch.rs | head -20

# Le structured output retry (different du task retry!)
grep -n "max_retries\|retry" tools/nika-engine/src/runtime/structured_output.rs | head -20
```

**NUANCE CRITIQUE:** Il y a DEUX systemes de retry:
1. **`retry:` task-level** → delay + backoff + max_attempts pour transient failures (HTTP 429, timeout, etc). Actuellement ONLY fetch verb.
2. **`structured: { max_retries: N }`** → re-prompt with schema error feedback pour LLM output validation. Already works for infer verb.

Le bug ENG-001 concerne le #1 — task-level retry doit supporter ALL verbs.

### 0.3 — Analyser l'artifact template issue

```bash
# Comment les artifacts sont parses dans l'analyzer
grep -n "artifact\|ArtifactSpec" tools/nika-core/src/ast/analyzer/analyze.rs | head -20

# Le warning specifique
grep -n "Failed to parse artifact" tools/nika-core/src/ast/analyzer/analyze.rs | head -5

# Comment les artifacts sont resolus au runtime
grep -n "artifact\|write_artifact\|resolve.*path" tools/nika-engine/src/runtime/runner.rs | head -20
```

### 0.4 — Analyser le dry-run provider display

```bash
# Comment le dry-run affiche les tasks
grep -n "dry_run\|DryRun\|provider.*model" tools/nika-cli/src/workflow.rs | head -20

# Comment le display resout provider/model
grep -n "provider\|model" tools/nika-engine/src/display/ | head -20
```

### 0.5 — Web Research

Utilise `perplexity_search_web` pour:
1. **"rust retry exponential backoff async pattern"** — best practice pour retry generique
2. **"nika workflow engine yaml retry"** — voir si des users ont signale le bug

### 0.6 — Swarm d'analyse (3 agents en parallele)

**Agent 1 — Retry System Deep Dive:**
> "Read tools/nika-core/src/ast/analyzer/analyze.rs lines 550-600 and 720-740, tools/nika-engine/src/runtime/runner.rs lines 600-800, and tools/nika-engine/src/runtime/executor/fetch.rs. Trace EXACTLY how retry works for fetch: verb. Document the full code path: where RetryConfig is parsed → where it's passed to the executor → where the retry loop runs. Then propose how to extend it to infer/exec/invoke/agent verbs."

**Agent 2 — Artifact Template Resolution:**
> "Read tools/nika-core/src/ast/analyzer/analyze.rs — find where ArtifactSpec is parsed and where the warning 'Failed to parse artifact' is emitted. Then read tools/nika-engine/src/runtime/runner.rs — find where artifacts are actually written at runtime. Determine: is the parse-time warning correct (artifact truly skipped) or misleading (artifact works at runtime despite warning)?"

**Agent 3 — Dry-Run Provider Display:**
> "Read tools/nika-cli/src/workflow.rs — find the dry-run code path. Read tools/nika-engine/src/display/ — find how tasks are displayed. Determine: why does dry-run show the workflow-level provider:openai/model:gpt-4o-mini for ALL tasks instead of the task-level override? Is the display wrong (runtime is correct) or is the runtime also wrong?"

---

## PHASE 1: ENG-001 — Retry on ALL Verbs

### Le bug

`retry:` est silencieusement ignore sur infer/exec/invoke/agent tasks. Seul `fetch:` le supporte.

### Fichiers a modifier

1. `tools/nika-core/src/ast/analyzer/analyze.rs:560-580,729-736` — supprimer la restriction fetch-only
2. `tools/nika-engine/src/runtime/runner.rs` — wirer le retry config pour tous les verbs dans la task execution loop
3. `tools/nika-engine/src/runtime/executor/infer.rs` — ajouter retry wrapper
4. `tools/nika-engine/src/runtime/executor/exec.rs` — ajouter retry wrapper
5. `tools/nika-engine/src/runtime/executor/agent.rs` — ajouter retry wrapper

### Architecture du fix

Le retry pour fetch: fonctionne car le `fetch.rs` executor a une boucle interne. Pour les autres verbs, le retry doit etre dans le **runner** (niveau au-dessus de l'executor), pas dans chaque executor individuellement.

Pattern recommande:
```rust
// Dans runner.rs, la task execution loop:
async fn execute_task_with_retry(
    &self,
    task: &AnalyzedTask,
    bindings: &ResolvedBindings,
    datastore: &RunContext,
) -> TaskResult {
    let retry_config = task.retry.as_ref();
    let max_attempts = retry_config.map(|r| r.max_attempts).unwrap_or(1);
    let delay_ms = retry_config.map(|r| r.delay_ms).unwrap_or(0);
    let backoff = retry_config.map(|r| r.backoff).unwrap_or(1.0);

    let mut last_error = None;
    for attempt in 0..max_attempts {
        if attempt > 0 {
            let delay = (delay_ms as f64 * backoff.powi(attempt as i32 - 1)) as u64;
            tokio::time::sleep(Duration::from_millis(delay)).await;
            tracing::info!(task_id = %task.id, attempt = attempt + 1, "retrying after {}ms", delay);
        }

        match self.execute_task_inner(task, bindings, datastore).await {
            Ok(result) => return Ok(result),
            Err(e) if is_retryable(&e) && attempt < max_attempts - 1 => {
                tracing::warn!(task_id = %task.id, error = %e, "transient failure, will retry");
                last_error = Some(e);
            }
            Err(e) => return Err(e),
        }
    }
    Err(last_error.unwrap())
}

fn is_retryable(error: &NikaError) -> bool {
    matches!(error,
        NikaError::ProviderApiError { .. } |  // LLM 429, 500, timeout
        NikaError::ExecError { .. } |          // command transient failure
        NikaError::McpError { .. } |           // MCP server restart
        NikaError::FetchError { .. }           // HTTP transient
    )
}
```

### TDD Steps

**Step 1:** Write test — infer task with retry config should NOT emit warning

```rust
#[test]
fn test_retry_on_infer_no_warning() {
    let yaml = r#"
schema: "nika/workflow@0.12"
tasks:
  - id: test
    retry:
      max_attempts: 3
      delay_ms: 1000
    infer: "hello"
"#;
    let result = analyze(parse(yaml, FileId(0)).unwrap());
    let warnings: Vec<_> = result.warnings.iter()
        .filter(|w| w.message.contains("retry"))
        .collect();
    assert!(warnings.is_empty(), "retry on infer should not warn");
}
```

**Step 2:** Run → FAIL (current code warns)

**Step 3:** Remove the fetch-only restriction in analyzer

**Step 4:** Write runtime retry test (wiremock: make OpenAI return 429 once, then 200)

**Step 5:** Implement retry wrapper in runner.rs

**Step 6:** Run `cargo test --workspace --lib` → ALL PASS

**Step 7:** Commit

---

## PHASE 2: ENG-002 — Artifact Template Path Resolution

### Le bug

`artifact: { path: "reports/{{with.lang.lang}}/report.md" }` emet un WARN au parse time.

### Investigation

Lire le code analyzer pour comprendre:
- Est-ce que l'artifact est VRAIMENT ignore (runtime ne l'ecrit pas)?
- Ou est-ce juste un warning trompeur (runtime fonctionne quand meme)?

Si c'est juste un warning trompeur: supprimer le warning quand le path contient `{{`.
Si l'artifact est vraiment ignore: fixer le runtime pour resoudre les templates au moment de l'ecriture.

### TDD Steps

**Step 1:** Write test — artifact with template path in for_each should not warn

**Step 2:** Check runtime behavior (peut necesiter un test d'integration)

**Step 3:** Fix based on findings

**Step 4:** Commit

---

## PHASE 3: ENG-005 — Extended Thinking on Infer Verb

### Le bug

Commit `1ed73a1bc` dit "warn that extended_thinking on infer: verb is not supported". Ca veut dire que `extended_thinking: true` sur un `infer:` task est IGNORE.

### Investigation

```bash
grep -n "extended_thinking" tools/nika-engine/src/runtime/executor/infer.rs | head -20
```

Question cle: est-ce un choix de design (extended_thinking = agent-only) ou un bug (devrait marcher sur infer aussi)?

Si c'est un choix de design:
- Documenter clairement
- Changer le workflow pour utiliser `agent:` au lieu de `infer:` pour les tasks qui ont besoin de thinking

Si c'est fixable:
- Etendre le support a infer verb
- Quand provider=claude + extended_thinking=true sur infer: utiliser l'API Claude thinking

### TDD Steps

**Step 1:** Verify current behavior with a test

**Step 2:** If design choice → update CLAUDE.md docs + remove from workflow infer tasks

**Step 3:** If fixable → implement + test

**Step 4:** Commit

---

## PHASE 4: WORKFLOW FIXES (dans le repo aaayayaa)

Apres les fixes engine, passe au workflow a `/Users/thibaut/Desktop/aaayayaa/`.

### WF-008: check_llms_txt et check_feed 404 resilience

```yaml
# Fix: ajouter response: full
- id: check_llms_txt
  fetch:
    url: "{{inputs.site_url}}/.well-known/llms.txt"
    extract: llm_txt
    response: full  # ADD THIS — 404 becomes data, not error
```

ATTENTION: verifier si `extract:` et `response: full` sont compatibles. Si non, utiliser `response: full` sans `extract:` et parser manuellement.

### WF-009: parse_sitemap.py sitemap index recursion

Modifier `scripts/parse_sitemap.py` pour suivre les sub-sitemaps:

```python
def fetch_and_parse_recursive(url, depth=0, max_depth=3):
    """Recursively fetch and parse sitemaps, following index files."""
    if depth > max_depth:
        return {'pages': [], 'langs': set()}

    content = fetch_url(url)
    result = parse_sitemap_content(content)

    if result['type'] == 'index':
        all_pages = []
        all_langs = set()
        for sub in result['sitemaps']:
            sub_result = fetch_and_parse_recursive(sub['loc'], depth + 1, max_depth)
            all_pages.extend(sub_result.get('pages', []))
            all_langs.update(sub_result.get('langs', []))
        result['pages'] = all_pages
        result['langs'] = sorted(list(all_langs))
        result['total'] = len(all_pages)
        result['type'] = 'index_expanded'

    return result
```

### WF-003: filter empty og_image URLs

```yaml
# Ajouter une infer task intermediaire qui filtre
- id: filter_og_images
  depends_on: [extract_page_metadata]
  with:
    meta_results: $extract_page_metadata
  infer:
    prompt: |
      From this metadata array, extract ONLY entries that have a non-empty og_image URL.
      {{with.meta_results | to_json}}
    model: "{{inputs.fast_model}}"
    temperature: 0.0
  structured:
    schema:
      type: object
      properties:
        pages_with_og: { type: array, items: { type: object } }
      required: [pages_with_og]

# Then fetch_og_images iterates filtered list
- id: fetch_og_images
  depends_on: [filter_og_images]
  with:
    filtered: $filter_og_images
  for_each: "{{with.filtered.pages_with_og}}"
```

OU plus simplement — utiliser le `| compact` transform si dispo, ou `| default([])`.

### WF-004: API key via env var

```yaml
headers:
  x-api-key: "{{$env.QRCODE_AI_API_KEY}}"
```

Verifier si `$env.*` fonctionne dans `headers:` (lire le code binding).

### WF-005: Landing page prompt overflow

Splitter en 2 tasks:
1. `summarize_for_landing` — reduit le JSON+MD en un summary compact (~2K tokens)
2. `generate_landing_html` — genere le HTML a partir du summary

### WF-002: Wire og_thumbnails

Ajouter `og_thumbnails: $og_thumbnails` dans le `with:` de `generate_landing_html`.

---

## PHASE 5: FEATURES MANQUANTES A IMPLEMENTER

### F-001: `nika:provenance` C2PA signing

Ajouter apres les PDF dans 09-reports.nika.yaml:

```yaml
- id: sign_global_report
  depends_on: [global_pdf]
  invoke:
    tool: "nika:provenance"
    params:
      hash: "{{inputs.output_dir}}/reports/global/report.pdf"
      manifest:
        title: "Global SEO+GEO Audit — {{inputs.brand}}"
        generator: "Nika v0.50"
```

### F-002: `nika:thumbhash` pour landing page

```yaml
- id: compute_thumbhashes
  depends_on: [render_dashboard_png, chart_seo_by_language]
  with:
    dashboard: $render_dashboard_png
  invoke:
    tool: "nika:thumbhash"
    params:
      hash: "{{with.dashboard | default('')}}"
```

### F-003: `agent: mcp: [geo]` explicit

Ajouter dans 07-geo-intelligence.nika.yaml:

```yaml
agent:
  mcp: [geo]
  tools: [nika:glob, nika:read, nika:complete]
```

### F-004: `stop_sequences:` sur les agents

```yaml
agent:
  stop_sequences: ["---", "END_ANALYSIS", "DONE"]
```

### F-005: Internal link graph analysis

Nouvelle task dans 03-mass-scan:

```yaml
- id: analyze_link_graph
  depends_on: [extract_page_links]
  with:
    all_links: $extract_page_links
  infer:
    prompt: |
      Analyze the internal link structure from these extracted links.
      Identify: link hubs, orphan pages, redirect chains.
      {{with.all_links | flatten | unique | to_json}}
```

---

## PHASE 6: VERIFICATION FINALE

### 6.1 — Tests engine

```bash
cd /Users/thibaut/dev/supernovae/nika/tools
cargo test --workspace --lib
cargo clippy --workspace -- -D warnings
```

### 6.2 — Validate workflow

```bash
cd /Users/thibaut/Desktop/aaayayaa
nika check main.nika.yaml
```

### 6.3 — Test scripts Python

```bash
cd /Users/thibaut/Desktop/aaayayaa

# parse_sitemap.py with sitemap INDEX (recursive)
python3 scripts/parse_sitemap.py "https://qrcode-ai.com/sitemap.xml" | python3 -c "import sys,json; d=json.load(sys.stdin); print(f'Type: {d[\"type\"]}, Pages: {d[\"total\"]}, Langs: {d[\"langs\"]}')"

# geo_mcp_server.py
printf '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}\n' | python3 scripts/geo_mcp_server.py
```

### 6.4 — Real run

```bash
cd /Users/thibaut/Desktop/aaayayaa
nika run main.nika.yaml \
  --input site_url="https://qrcode-ai.com" \
  --input brand="QR Code AI" \
  --input output_dir="/Users/thibaut/Desktop/aaayayaa" \
  --input fast_model="gpt-4o-mini" \
  --input deep_model="gpt-4o" \
  --input creative_model="gemini-2.5-flash" \
  --no-live 2>&1 | tee run-output.log
```

### 6.5 — Swarm review final (3 agents)

1. Code review des changes engine
2. Data flow review du workflow v2.1
3. Output quality review (si le run reussit, reviewer les reports generes)

---

## REGLES

1. **TDD STRICT** — test qui fail d'abord, toujours
2. **`--lib` TOUJOURS** — JAMAIS `cargo test` sans `--lib`
3. **1 fix = 1 commit** — format: `type(scope): description`
4. **Ne cache RIEN** — si une feature ne marche pas, dis-le clairement
5. **`retry:` doit marcher sur TOUS les verbs** — c'est la promesse #1
6. **Les artifacts doivent fonctionner avec templates** — c'est le pattern for_each standard
7. **Le dry-run doit afficher le vrai provider/model** — sinon c'est trompeur
8. **extended_thinking: supporter ou documenter clairement** — pas de zone grise

---

## LIENS

| Ressource | Chemin |
|-----------|--------|
| Bug report detaille | `docs/plans/2026-03-28-bug-report-workflow-v2.md` |
| Plan round 1 (done) | `docs/plans/2026-03-27-structured-output-and-engine-fixes.md` |
| Master prompt round 1 (done) | `docs/plans/2026-03-27-engine-fixes-master-prompt.md` |
| Workflow GEO-SEO | `/Users/thibaut/Desktop/aaayayaa/main.nika.yaml` |
| Nika dev reference | `tools/nika/CLAUDE.md` |
| Nika rules | `../../.claude/rules/nika.md` |
| Analyzer (retry restriction) | `tools/nika-core/src/ast/analyzer/analyze.rs:560-580,729-736` |
| Runner (task execution) | `tools/nika-engine/src/runtime/runner.rs:600-800` |
| Fetch executor (retry impl) | `tools/nika-engine/src/runtime/executor/fetch.rs` |
| Infer executor | `tools/nika-engine/src/runtime/executor/infer.rs` |
| Structured output | `tools/nika-engine/src/runtime/structured_output.rs` |
| Artifact writer | `tools/nika-engine/src/runtime/runner.rs` (search write_artifact) |
| Display/dry-run | `tools/nika-cli/src/workflow.rs` |
| Git workflow rules | `../../dx/.claude/rules/git-workflow.md` |

---

## CO-AUTHORS

```
Co-Authored-By: Nika 🦋 <nika@supernovae.studio>
```
