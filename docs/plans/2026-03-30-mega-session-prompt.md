# MEGA SESSION PROMPT — Overnight E2E Testing + Fix Everything

> **Copy-paste this entire file as the first message of a new Claude Code session.**
> Budget: $20-30 API | Providers: OpenAI, xAI, Gemini | Mode: Fix + commit every bug

---

## CONTEXT

Tu travailles sur **Nika** (`/Users/thibaut/dev/supernovae/nika`), un workflow engine YAML pour l'IA en Rust.
- **Version actuelle:** v0.53.0 (+ 9 post-release commits)
- **Tests:** 9,015 passing, 0 failures
- **Codebase:** 356K LOC Rust, 12 crates, 644 workflows
- **Providers dispo:** OpenAI ✓, xAI ✓, Gemini ✓, Anthropic ✗ (no credits)
- **Binary:** `./tools/target/debug/nika` ou `./tools/target/release/nika`
- **Tests:** `cd tools && cargo test --workspace --lib`
- **Build:** `cd tools && cargo build -p nika`

### Règles absolues
1. **TDD** — test failing d'abord, fix, verify
2. **1 fix = 1 commit** — `type(scope): description` + co-authors
3. **cargo test --workspace --lib** doit passer après CHAQUE fix
4. **JAMAIS cargo test sans --lib** (keychain popup)
5. **Structured output prompts** doivent être en langage NATUREL, jamais mentionner JSON
6. **AGPL-3.0-or-later** pour tous les crates

### Handoffs à lire
- `docs/plans/2026-03-30-v054-master-handoff.md` — 25 bugs triés par priorité
- `docs/plans/2026-03-30-feature-deep-audit.md` — feature completeness matrix
- `docs/plans/2026-03-30-overnight-mega-plan.md` — plan des 6 phases

---

## PHASE 1: FIX P0 BLOCKERS (~2h)

### 1.1 JSON Schema — 9 champs manquants
Le JSON schema embarqué (`tools/nika-engine/schemas/nika-workflow.schema.json` + `tools/nika/schemas/`) est incomplet.

**Ajouter ces champs au schema:**
- Workflow-level: `max_duration_secs` (integer), `pkg` (object)
- Task-level: `timeout` (integer), `max_tokens` (integer), `temperature` (number), `system` (string), `extended_thinking` (boolean), `thinking_budget` (integer), `response_format` (string enum: text/json/markdown)

**Après:** rebuild le binaire (`cargo build -p nika`) et vérifier que `nika check` accepte ces champs.

### 1.2 Orchestrator system prompt — exemples YAML
`orchestrate.rs:20` — le system prompt de l'orchestrateur n'a PAS d'exemple de syntaxe YAML.
Le LLM génère du YAML invalide quand il appelle `nika:run` (prouvé par test réel avec OpenAI).

**Vérifier** que le fix est déjà appliqué (commit récent). Si non, ajouter un bloc `## nika:run YAML Syntax` avec un exemple complet.

### 1.3 Redact secrets dans tracing::warn
3 sites logent des commandes non-redactées:
- `exec.rs:83` — `command = %resolved_cmd` → `command = %redact_for_event(&resolved_cmd)` — **VERIFIER si déjà fixé**
- `security.rs:258` — `command = %cmd` dans shell_mode_blocklist
- `security.rs:377` — `command = %cmd` dans check_blocklist

**Note:** `redact_for_event` est dans `runtime/executor/verbs.rs` mais est `pub(crate)`. Il faut soit la rendre accessible, soit utiliser `crate::util::redact_secrets` (qui existe déjà).

### 1.4 Recursive JSON redaction
`resolve.rs` — `to_value_redacted()` ne redacte que les strings de premier niveau. Les objets/arrays imbriqués passent en clair.

**Fix:** Ajouter `redact_value_recursive(value, regex)` qui traverse les objets/arrays récursivement.

### 1.5 Confidence target validation
`nika-core/src/ast/orchestrate.rs` — `confidence_target` accepte NaN, négatif, >1.0.
**Vérifier** que `validate()` avec `clamp(0.0, 1.0)` est wired dans l'analyzer.

### 1.6 BINDING_RE regex — ajouter context
`exec.rs:46` — le regex `\{\{(with|inputs)\.[^}]+\}\}` ne capture pas `{{context.*}}`.
**Fix:** `\{\{(with|inputs|context)\.[^}]+\}\}`

**Commit chaque fix individuellement. Puis `cargo test --workspace --lib`.**

---

## PHASE 2: CRÉER 50+ WORKFLOWS (~2h)

Créer un dossier `tests/e2e-overnight/` avec 8 catégories de workflows.

### Catégorie A: Structured Output (10 workflows)
Tester schemas basiques, nested, arrays, enum, from_example, for_each+structured, repair_model, multi-provider parity.
- Providers: alterner OpenAI (gpt-4o-mini), xAI (grok-3-fast), Gemini (gemini-2.0-flash)
- **JAMAIS** mentionner JSON dans le prompt

### Catégorie B: Agent Verb (8 workflows)
Tester nika:log, nika:complete, guardrails (length+regex), limits (cost+tokens), completion modes (explicit/natural/pattern), spawn_agent.

### Catégorie C: Fetch 9 Modes (9 workflows)
Tester CHAQUE mode d'extraction avec des URLs réelles:
- markdown: httpbin.org/html
- article: Wikipedia
- metadata: github.com
- links: news.ycombinator.com
- jsonpath: jsonplaceholder.typicode.com/users avec `$[*].name`
- feed: hnrss.org/frontpage
- text+selector: example.com avec `selector: "title"`
- llm_txt: docs.anthropic.com
- response:full: httpbin.org/get

### Catégorie D: DAG + for_each (8 workflows)
Linear chain, diamond, for_each concurrent, for_each sequential fail_fast:false, for_each+structured, deep chain avec transforms, for_each+artifact, fan-out/merge.

### Catégorie E: Exec + Invoke (8 workflows)
Shell pipes, env vars+cwd, exec→infer chain, nika:glob, nika:log+assert, multi-verb pipeline, timeout tests.

### Catégorie F: Media Pipeline (5 workflows)
nika:import→dimensions→dominant_color→thumbhash, nika:chart (bar/line/pie), exec create file→import, glob Rust files→count, exec JSON→chart.

### Catégorie G: Security (5 workflows — tests négatifs)
SSRF (169.254.x.x), path traversal (../../), command injection ($()), sudo blocklist, LD_PRELOAD env var. **Succès = le workflow ÉCHOUE avec le bon code erreur.**

### Catégorie H: Real-World Use Cases (7 workflows)
Blog generator, API monitor, multi-source research, data pipeline, SEO audit, competitive analysis, translation pipeline.

**Pour chaque workflow:**
1. `nika check` doit passer
2. `nika run --no-live` avec vrai provider
3. Vérifier que l'output est logique et complet
4. Si échec: analyser l'erreur, fixer le code, recommit

---

## PHASE 3: BUG HUNT + FIX (~3h)

Exécuter les 50+ workflows et **fixer chaque bug trouvé**.

### Process pour chaque workflow
```bash
./tools/target/debug/nika check tests/e2e-overnight/A01.nika.yaml
./tools/target/debug/nika run tests/e2e-overnight/A01.nika.yaml --no-live
```

### Bugs connus à chercher
- Structured output qui ne parse pas le JSON correctement
- for_each qui perd des résultats
- Bindings `$task.field` qui retournent null
- Transforms (upper, trim, join, split) qui crash sur null
- fetch extract modes qui retournent du HTML brut au lieu de markdown
- Agent qui ne complète jamais (completion mode buggé)
- Artifacts qui ne s'écrivent pas
- Timeout qui ne fonctionne pas
- Provider fallback qui ne marche pas

### Pour chaque bug trouvé
1. Identifier le fichier + ligne
2. Écrire un test failing
3. Fixer le code
4. Vérifier avec `cargo test --workspace --lib`
5. Commit: `fix(scope): description`
6. Re-run le workflow pour confirmer

---

## PHASE 4: SECURITY HARDENING (~1h)

### 4.1 Expand SECRET_RE
`verbs.rs:111` — Ajouter patterns manquants:
```
sk_live_[a-zA-Z0-9]{24,}     # Stripe
sk_test_[a-zA-Z0-9]{24,}     # Stripe test
SG\.[a-zA-Z0-9_-]{20,}       # SendGrid
(postgres|mongodb|mysql)://[^:]+:[^@]+@  # DB connection strings
```

### 4.2 MCP resource read size limit
`invoke.rs:211` — le 50MB check n'est que pour tool calls, pas resource reads.
Ajouter la même vérification pour `read_resource()`.

### 4.3 Vérifier les 5 workflows security (G01-G05)
Chacun DOIT échouer avec le bon code erreur. Si un passe = bug de sécurité critique.

---

## PHASE 5: DEAD CODE NUKE (~1h)

```bash
cd tools
cargo clippy --workspace -- -D warnings  # Zero warnings
cargo machete                              # Unused deps
```

### Items à nettoyer
- 14 `#[allow(dead_code)]` annotations — vérifier si toujours nécessaires
- `MaxTurnsReached` dead variant — soit remove soit wire
- TODO(scope) stub dans agent loop — documenter ou implémenter
- `resolve_for_shell()` dans template.rs — fonction jamais appelée

### Vérification finale
```bash
cargo test --workspace --lib  # 9000+ tests
cargo clippy --workspace -- -D warnings
```

---

## PHASE 6: COMPILE MEGA HANDOFFS (~1h)

Créer 5 handoff prompts prêts à copier-coller dans de futures sessions:

### Handoff A: `docs/plans/handoff-sprint-security.md`
Sprint Security (~4h) — tous les items sécurité restants avec file:line exact.

### Handoff B: `docs/plans/handoff-sprint-agent.md`
Sprint Agent+Provider (~8h) — max_tokens(8192) x22, agent scope, LLM guardrails, 8 presets.

### Handoff C: `docs/plans/handoff-sprint-runner.md`
Sprint Runner+Perf (~6h) — O(n^2) fix, semaphore, EventLog ring buffer.

### Handoff D: `docs/plans/handoff-sprint-orchestrate.md`
Sprint Orchestrate (~4h) — system prompt, E2E tests, max_rounds/cost enforcement.

### Handoff E: `docs/plans/handoff-sprint-polish.md`
Sprint Polish+Launch (~4h) — TUI migration, schema sync, Dockerfile, README.

**Chaque handoff doit contenir:**
- Contexte complet (ce qui a été fait, ce qui reste)
- Exact file:line pour chaque item
- Commandes de vérification
- Critères de succès
- Estimated effort

---

## RÉSUMÉ DES LIVRABLES

À la fin de cette session, tu dois avoir:

- [ ] 6 P0 blockers fixés et commités
- [ ] 50+ workflows créés dans `tests/e2e-overnight/`
- [ ] 40+ workflows exécutés avec succès (real API)
- [ ] Chaque échec analysé et soit fixé soit documenté
- [ ] Security tests (G01-G05) validés
- [ ] Dead code nettoyé, clippy clean
- [ ] 5 handoff prompts prêts pour les prochaines sessions
- [ ] `cargo test --workspace --lib` passe (9000+ tests)
- [ ] Tout commité et poussé sur main
- [ ] CHANGELOG mis à jour

**Push souvent. Commite souvent. Fix tout. Sois pas superficiel.**
