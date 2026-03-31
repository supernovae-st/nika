# MEGA SESSION — Overnight Autonomous E2E + Fix Everything

> **Copie ce fichier ENTIER comme premier message dans une nouvelle session Claude Code.**
> Budget: $20-30 API | Duration: 20h+ autonome | Mode: Fix, test, commit, push

---

## IDENTITY

Tu es un ingenieur senior Rust travaillant sur Nika, un workflow engine YAML pour l'IA.
Tu travailles en FULL AUTONOMIE pendant 20h. Tu fixes des bugs, crees des tests, executes des workflows avec de vrais providers (OpenAI, xAI, Gemini, native GGUF).
Tu ne demandes JAMAIS l'aide humaine sauf si tu es BLOQUE depuis 30min.

---

## CODEBASE

```
Repo:       /Users/thibaut/dev/supernovae/nika
Version:    v0.53.0 (+ 17 post-release commits, NOT pushed)
Tests:      2,153 passing, 0 failing, 1 ignored  (cargo test --workspace --lib)
LOC:        356K Rust, 12 crates
Binary:     ./tools/target/debug/nika (10 features compiled: native-inference, media-thumbnail, media-optimize, media-chart, fetch-html, fetch-markdown, fetch-article, fetch-feed, tui, lsp)
```

### ATTENTION — Etat git au demarrage
- **17 commits locaux NON pushes** — `git push` en premier !
- **0 fichiers code modifies non commites** — code clean
- **1 doc modifie non commite** : `docs/plans/2026-03-30-mega-session-prompt.md`
- **81 workflows crees** dans `tests/e2e-overnight/` (untracked) — a commit
- **Tests : TOUS PASSENT** (2153 pass, 0 fail)
- **Smoke test : 31 pass, 1 security bug confirme (G06 = attendu)**

### Commands
```bash
# Build (full features — DEJA COMPILE, 10/22 features)
# Si besoin de rebuild:
cd /Users/thibaut/dev/supernovae/nika/tools && cargo build -p nika -F nika/native-inference -F nika/media-thumbnail -F nika/media-optimize -F nika/media-chart -F nika/fetch-html -F nika/fetch-markdown -F nika/fetch-article -F nika/fetch-feed

# Build (rapide, sans native)
cd tools && cargo build -p nika

# Tests (TOUJOURS --lib — jamais sans, sinon keychain popup macOS)
cd tools && cargo test --workspace --lib

# Run workflow
./tools/target/debug/nika run tests/e2e-overnight/A01-basic-structured.nika.yaml --no-live

# Validate workflow
./tools/target/debug/nika check tests/e2e-overnight/A01-basic-structured.nika.yaml

# Smoke test suite (tous les workflows gratuits)
bash tests/e2e-overnight/run-smoke.sh ./tools/target/debug/nika

# Check features
./tools/target/debug/nika features

# Check providers
./tools/target/debug/nika provider list

# Download native models
./tools/target/debug/nika model pull llama3.2:1b
./tools/target/debug/nika model pull mistral:7b
```

### Providers disponibles
- **OpenAI** : gpt-4o-mini (OPENAI_API_KEY) — structured output natif
- **xAI** : grok-3-fast (XAI_API_KEY) — structured output natif
- **Gemini** : gemini-2.0-flash (GEMINI_API_KEY) — tool injection
- **Anthropic** : 0 credits — SKIP tous les workflows anthropic
- **Native** : llama3.2:1b, mistral:7b — GGUF local, text-only, pas de tool calling

---

## DOCUMENTS A CHARGER (lis TOUT avant de commencer)

**OBLIGATOIRE — lis ces fichiers dans l'ordre :**
1. `docs/plans/2026-03-30-overnight-mega-plan.md` — Plan v3 detaille (9 phases, 42 bugs)
2. `docs/plans/2026-03-30-overnight-companion.md` — Knowledge base (tests Rust, limites, error codes)
3. `docs/plans/2026-03-30-v054-master-handoff.md` — 25 bugs tries par priorite
4. `docs/plans/2026-03-30-feature-deep-audit.md` — Feature completeness matrix

**REFERENCE pendant execution :**
- `~/.claude/rules/nika.md` — Schema complet, 5 verbs, providers, syntax
- `~/.claude/rules/nika-bugs-and-patterns.md` — Bugs reels, BUG-001 to BUG-013

---

## REGLES ABSOLUES

1. **TDD** — test failing d'abord, fix, verify. Skill: `/spn-powers:test-driven-development`
2. **1 fix = 1 commit** — `type(scope): description` + co-authors :
   ```
   Co-Authored-By: Claude <noreply@anthropic.com>
   Co-Authored-By: Nika 🦋 <nika@supernovae.studio>
   ```
3. **`cargo test --workspace --lib`** DOIT passer apres CHAQUE fix
4. **JAMAIS `cargo test` sans `--lib`** (keychain popup macOS)
5. **Prompts structured output** = langage NATUREL, JAMAIS mentionner JSON
6. **AGPL-3.0-or-later** pour tous les crates
7. **`git add <specific files>`** — jamais `git add .`
8. **Push apres chaque 3-5 commits** (checkpoint)

---

## SKILLS A UTILISER

```
/spn-powers:test-driven-development        — AVANT chaque fix
/spn-powers:verification-before-completion — AVANT de dire "c'est fait"
/spn-powers:systematic-debugging           — Quand un workflow echoue
/spn-powers:requesting-code-review         — Apres chaque phase
/spn-powers:dispatching-parallel-agents    — Quand 3+ fixes independants
/spn-powers:defense-in-depth               — Pour fixes security
/spn-powers:root-cause-tracing             — Quand un bug est deep
/spn-rust:rust                             — Pour tout code Rust
```

---

## BOUCLE SOCRATIQUE (apres CHAQUE action)

### Apres chaque fix :
1. Mon test FAIL avant et PASS apres ?
2. Ai-je grep pour des patterns similaires ailleurs ?
3. Ai-je verifie les events NDJSON ?
4. Ce fix merite une entree CHANGELOG ?
5. Ai-je commit avec les co-authors ?

### Apres chaque workflow run :
1. Le workflow a reussi pour la BONNE raison ?
2. L'output est LOGIQUEMENT correct ? (pas juste non-vide)
3. Les events attendus sont dans l'output ?
4. Si echec — le message d'erreur pointe la bonne ligne ?

### Apres chaque phase :
```bash
cargo test --workspace --lib                              # TOUS passent
bash tests/e2e-overnight/run-smoke.sh ./tools/target/debug/nika  # Smoke OK
git push                                                  # Checkpoint
```

---

## RECOVERY (si ca casse)

- **Compilation fail** : Lis l'erreur. Import manquant ? Scope ? Fix dans le meme commit.
- **Tests fail apres fix** : `cargo test -- --nocapture test_name`. Comprends POURQUOI.
- **Provider 429** : Attends 60s. Si persistent, switch provider.
- **50+ tests cassent** : `git stash`. Repense l'approche. Le fix est peut-etre faux.
- **Context window plein** : Commit, push, ecris handoff dans `docs/plans/`, nouvelle session.

---

## PLAN D'EXECUTION — 9 PHASES

### Phase 0 : Stabilize (~10min)

**Etape 1 — Push les 17 commits locaux :**
```bash
git push  # 17 commits en avance sur origin/main
```

**Etape 2 — Commit les 81 workflows E2E + docs :**
```bash
git add tests/e2e-overnight/
git add docs/plans/
git commit -m "test(e2e): add 81 overnight test workflows across 15 categories

Coverage: 44 mock + 18 openai + 10 xai + 8 gemini + 7 native + 1 fallback
- A (8): structured output — boolean, nested, arrays, enum, for_each, repair, parity
- B (5): agent verb — explicit, natural, guardrails, token_budget, file tools
- C (9): fetch 9 extract modes — markdown, article, metadata, links, jsonpath, feed, text, llm_txt, full
- D (7): DAG — linear, diamond, for_each concurrent/failfast/structured/artifact
- E (8): exec — pipes, env, timeout, glob, import, multi-verb
- F (5): media — dimensions, thumbnail, dominant_color, thumbhash, glob
- G (7): security — SSRF, path traversal, injection, blocklist, IPv6, newline, LD_PRELOAD
- H (6): real-world — blog, API, research, code review, SEO, content pipeline
- I (2): include/context — file bindings, skills injection
- M (3): multi-provider — infer parity, structured parity, fallback chain
- N (7): native GGUF — llama + mistral, structured, for_each, exec chain, mixed
- R (3): artifacts — markdown, JSON, for_each
- S (5): stress — transforms, for_each 20, diamond DAG, many vars, deep binding
- T (3): telemetry — for_each events, structured retry, redaction
- V (3): verification — redaction, error codes, artifact roundtrip
Fixtures: 2 PNG (1x1 + 10x10), skill, context, partial, smoke runner

Co-Authored-By: Claude <noreply@anthropic.com>
Co-Authored-By: Nika 🦋 <nika@supernovae.studio>"
git push
```

**Etape 3 — Verifie :**
```bash
cargo test --workspace --lib  # 2153 pass, 0 fail
bash tests/e2e-overnight/run-smoke.sh ./tools/target/debug/nika  # 31 pass, 1 security bug (G06)
```

**GATE : Build green, 0 failing tests, tout pousse.**

### Phase 1 : CRITICAL Security (~1h)
5 fixes security restants (5 deja fixes dans les commits precedents). Chacun = 1 commit.

**DEJA FIXES (ne pas refaire) :**
- ~~1.5 Recursive JSON redaction~~ — `redact_value_recursive()` avec depth limit 16
- ~~1.6 JSON schema 9 fields~~ — tous presents
- ~~1.7 BINDING_RE context~~ — regex inclut `context`
- ~~1.8 tracing::warn secrets~~ — `redact_secrets()` applique

**RESTENT A FIXER (5 bugs) :**

| # | Bug | File:Line | Fix exact |
|---|-----|-----------|-----------|
| 1.1 | Newline injection shell exec | exec.rs:38 | Changer `false` en `is_shell` dans `validate_exec_command_with_shell(&resolved_cmd, false)` |
| 1.2 | IPv6 :: SSRF bypass | policy.rs:48-71 | Ajouter `if v6 == Ipv6Addr::UNSPECIFIED { return true; }` dans le match IPv6 |
| 1.3 | SECRET_RE expansion | util/mod.rs:30 | Ajouter patterns: `ASIA[A-Z0-9]{16}`, `gh[udr]_`, `SG\\.`, `eyJ[A-Za-z0-9_-]+\\.[A-Za-z0-9_-]+\\.[A-Za-z0-9_-]+` |
| 1.4 | MCP error redaction | invoke.rs:159,173,230,488 | `redact_secrets()` sur response value avant emission McpResponse |
| 1.9 | Quoted pattern bypass | security.rs:249-255 | Appliquer `contains_unquoted()` a TOUS les patterns, pas seulement backtick |

**VALIDATION :** G06 (newline) DOIT echouer apres fix 1.1. Actuellement il passe = BUG CONFIRME.
**Le fix exact :** `exec.rs:38` — `validate_exec_command_with_shell(&resolved_cmd, false)` → `(&resolved_cmd, is_shell)`

**GATE : cargo test + G01-G07 TOUS echouent.**

### Phase 2 : Silent Bugs (~1h)
3 fixes (2.4 deja fixe). Details dans le mega plan section Phase 2.
- 2.1 unwrap_or_default() dans transform.rs:284,430 (FirstN, ToJson) — remplacer par `?` ou `.map_err()`
- 2.2 String "null" → null coercion dans verbs.rs:85-86 — supprimer le case `"null"`
- 2.3 Transform null propagation (documenter) — ajouter commentaire expliquant les 2 strategies
- ~~2.4 Artifact write failure~~ — **DEJA FIXE** (TaskFailed emis correctement)

### Phase 3 : Telemetry (~2h)
6 nouveaux events. Details dans le mega plan section Phase 3.
- ForEachItem events (3 variants)
- TaskCancelled distinct de TaskFailed
- FallbackChainExhausted
- Cost calculation warning
- StructuredOutputTimeout
- MCP reconnection

### Phase 4 : Edge Cases (~1.5h)
4 fixes. Details dans le mega plan section Phase 4.
- Cancellation dans binding resolution
- timeout=0 rejection
- for_each item count limit (MAX_FOR_EACH_ITEMS = 10_000)
- Binding from failed task warning

### Phase 5 : Workflow Factory + Execution (~4h)

**91 workflows DEJA crees** dans `tests/e2e-overnight/`. Tous passent `nika check`.
17 categories : A(8) B(5) C(9) D(7) E(8) F(10) G(7) H(6) I(2) M(3) N(7) R(3) S(5) T(3) V(3) W(3) X(2).
Providers : 49 mock + 21 openai + 11 xai + 9 gemini + 7 native + 1 fallback.
Nouveautes : W (vision openai/gemini + structured), X (combos multi-verbs), F07-F12 (charts + binary chain).
Cout estime des ~35 workflows payants : ~$2-3 total.

**Setup natif (si native-inference compile) :**
```bash
nika model pull llama3.2:1b    # ~1GB
nika model pull mistral:7b     # ~4GB
nika infer "Say hello" --provider native --model llama3.2:1b  # Test
```

**Ordre d'execution (optimise cout) :**
1. G01-G07 (security, gratuit) — DOIVENT TOUS ECHOUER
2. E01-E08 (exec, gratuit)
3. S01-S05 (stress, gratuit, mock)
4. D01-D06 (DAG, gratuit)
5. F01-F10 (media, gratuit)
6. C01-C09 (fetch, gratuit HTTP)
7. R01-R03 (artifacts, gratuit/mock)
8. I01-I03 (include/context, gratuit)
9. N01-N08 (native GGUF, gratuit)
10. A01-A10 (structured, API $$)
11. B01-B08 (agent, API $$$)
12. H01-H07 (real-world, API $$)
13. M01-M05 (multi-provider, API $$$)
14. T01-T05 (telemetry, API $)
15. V01-V05 (verification, API $)

**Pour chaque echec :** file:line → test failing → fix → cargo test → commit → re-run.
**Pour chaque media/artifact :** verifier PHYSIQUEMENT les fichiers sur disque.

### Phase 6 : Provider Parity (~1h)
Comparer M01-M05 entre les 5 providers. Documenter differences dans `docs/plans/overnight-results.md`.

### Phase 7 : Clippy + Final (~30min)
```bash
cargo clippy --workspace -- -D warnings
cargo test --workspace --lib
bash tests/e2e-overnight/run-smoke.sh ./tools/target/debug/nika
```

### Phase 8 : Handoffs (~1h)
Creer 5 handoff prompts dans `docs/plans/` :
- `handoff-sprint-security.md` (~4h) — tous les items security restants
- `handoff-sprint-agent.md` (~8h) — max_tokens, LLM guardrails, scope, presets
- `handoff-sprint-runner.md` (~6h) — O(n^2), semaphore, cancellation, EventLog
- `handoff-sprint-telemetry.md` (~4h) — nouveaux events, broadcast capacity
- `handoff-sprint-polish.md` (~4h) — TUI, Dockerfile, missing tests, CHANGELOG

Chaque handoff = self-contained, copiable dans une session fresh.

### Phase 9 : Rapport Final
Creer `docs/plans/overnight-results.md` avec :
- Tous les bugs fixes (commit hash, before/after)
- Tous les workflows executes (pass/fail, provider, cost, duration)
- Tous les bugs trouves pendant execution
- Provider parity analysis
- Artifact verification results
- Recommendations pour v0.54

---

## VERIFICATION PROTOCOLS

### NDJSON (apres chaque run)
```bash
./tools/target/debug/nika run workflow.nika.yaml --no-live 2>&1 | tee /tmp/run.log
grep -i "NIKA-\|error\|failed" /tmp/run.log
grep -iE "(sk-|AKIA|ASIA|ghp_|ghu_|password|eyJ)" /tmp/run.log  # DOIT ETRE VIDE
```

### Media (apres chaque F*)
```bash
ls -la .nika/media/ 2>/dev/null | head -10
for f in .nika/artifacts/*.png .nika/artifacts/*.jpg; do [ -f "$f" ] && file "$f"; done
```

### Artifacts (apres chaque R*)
```bash
ls -laR tests/e2e-overnight/output/ 2>/dev/null
find tests/e2e-overnight/output/ -empty 2>/dev/null && echo "EMPTY FILES" || echo "All OK"
for f in tests/e2e-overnight/output/*.json; do [ -f "$f" ] && jq . "$f" > /dev/null 2>&1 && echo "OK: $f" || echo "INVALID: $f"; done
```

---

## BUGS CONFIRMS PAR TESTS (de nos 68 workflows)

| Bug | Workflow | Statut | Impact |
|-----|---------|--------|--------|
| Newline injection passe | G06 | **CONFIRME** — exec.rs:38 passe `false` au lieu de `is_shell` | SECURITY CRITICAL |
| D04 fail_fast=false | D04 | Fixe dans workflow, teste fonctionnel | OK |
| S05 deep binding | S05 | Fixe (parse_json ajou), `deep_value` resolu | OK |
| Import+dimensions | F01 | 10x10 → `{"width":10,"height":10}` | OK |
| Dominant color | F05 | Red PNG → `#fc0404` | OK |
| Thumbhash | F06 | 24-byte hash genere | OK |
| for_each 20 items | S02 | 20 items traites, concurrency=5 | OK |
| Diamond DAG | S03 | 8 tasks, merge correct | OK |
| Shell pipes | E01 | `echo hello \| wc -w` → 2 | OK |
| Timeout 2s | E08 | `sleep 10` timeout → NIKA-044 | OK |
| SSRF 169.254 | G01 | Bloque correctement | OK |
| Command $(whoami) | G03 | Bloque NIKA-053 | OK |
| sudo blocklist | G04 | Bloque correctement | OK |
| Fetch jsonpath | C05 | `"Leanne Graham"` extrait | OK |
| Artifact foreach | R03 | 3 fichiers ecrits sur disque | OK |

---

## MASTER BUG REGISTRY (mis a jour apres analyse codebase)

### DEJA FIXES dans les 9 commits locaux (NE PAS REFAIRE)
- ~~O(n^2) get_ready_tasks~~ — commit `de519ff`
- ~~Semaphore NIKA-028~~ — commit `ab30508`
- ~~fail_fast=false cancelled items~~ — commit `86012cd`
- ~~Broadcast 1024→4096~~ — commit `6bd7481`
- ~~Shell transform null~~ — commit `144bdd5`
- ~~Pipe parser quote bug~~ — commit `2cb58d3`
- ~~Artifact path collisions~~ — commit `20b3730`
- ~~Dockerfile VERSION~~ — commit `d6214c7`
- ~~use:/max_retries: rejection~~ — commit `33764f0`
- ~~Recursive JSON redaction~~ — resolve.rs:478
- ~~JSON schema 9 fields~~ — schemas/*.json complet
- ~~BINDING_RE context~~ — exec.rs:46
- ~~tracing::warn secrets~~ — security.rs:259,377
- ~~Artifact write failures~~ — runner.rs:1275

### RESTENT A FIXER — CRITICAL (Phase 1, 5 bugs)
- C01: Newline injection exec.rs:38 (`false` → `is_shell`)
- C02: IPv6 :: SSRF policy.rs:48
- C03: SECRET_RE util/mod.rs:30 (ASIA, ghu_, SG., eyJ)
- C04: MCP error redaction invoke.rs:159,173,230,488
- C05: Quoted pattern bypass security.rs:249

### RESTENT A FIXER — HIGH (Phase 2-4, 10 bugs)
- H01: unwrap_or_default transform.rs:284,430
- H02: "null"→null verbs.rs:85
- H03: ForEachItem events log.rs
- H04: TaskCancelled event log.rs
- H05: FallbackChainExhausted event
- H06: StructuredOutputTimeout event
- H07: Cancellation in binding runner.rs:1965
- H08: for_each no item limit runner.rs:2225
- H09: Binding from failed task runner.rs:1936
- H10: Transform null inconsistency (documenter)

### NOUVEAUX BUGS TROUVES — a ajouter aux handoffs
- **NB01**: `scope:` (full/minimal/debug) parsed mais SILENTLY IGNORED (rig_agent_loop/mod.rs:292)
- **NB02**: `type: llm` guardrails parsed OK puis FAIL runtime — devrait fail au check (thinking.rs:57)
- **NB03**: Child agents perdent security policies (spawn.rs:289 — TODO non fait)
- **NB04**: Unknown model costs = silently 0.0 (cost.rs — pas de warning)
- **NB05**: `context_budget` tronque sans warning (token_budget.rs)
- **NB06**: `native` provider bloque pour `agent:` — devrait fail au check, pas au runtime
- **NB07**: `structured: enable_extractor: true` hard error — devrait warn ou reject au parse
- **NB08**: Nombreux error codes orphelins (NIKA-007-009, 023-025, 061-069, etc.) — dead code

### AUSSI FIXES dans les 8 derniers commits
- ~~Mock file-based schemas~~ — commit `ca70f04`
- ~~Mock depth limit~~ — commit `574d30f`
- ~~Dead variant test~~ — commit `09f1811`
- ~~TOCTOU race context loading~~ — commit `108b669`
- ~~CHANGELOG Sprint 3-5~~ — commit `24e630f`
- ~~Empty repair_model fallback~~ — commit `4bbdc78`
- ~~Retry compounding docs~~ — commit `85ad3c8`
- ~~for_each error message format~~ — commit `ebbecf7`
- ~~Temp file cleanup logging~~ — commit `ebbecf7`
- ~~Test LSP include definition~~ — passe maintenant (3/3 OK)

---

## BUGS TROUVES PAR LES REVIEWERS (a integrer dans les workflows)

Les 10 agents de review ont trouve des issues dans nos 68 workflows :

1. **Aucun workflow n'a d'assertions programmatiques** — les tests verifient "exit 0" mais pas la valeur de l'output. A corriger : ajouter `nika:assert` aux workflows E01, E02, E05, E06, D01, D03.

2. **E05 nika:assert est une tautologie** — `condition: true` ne teste rien. Fix : utiliser une valeur dynamique.

3. **F10 n'est pas un test media** — c'est un glob+exec. Le deplacer ou le remplacer.

4. **A03 et A04 violent la regle "prompt naturel"** — les prompts leakent les contraintes du schema. FIXES dans cette session.

5. **B03 et B07 n'avaient pas de completion mode** — les agents auraient timeout. FIXES dans cette session.

6. **S02 dit "100" mais a 20 items** — renommer ou augmenter.

7. **Modeles non-canoniques** : `grok-3-fast` et `gemini-2.0-flash` marchent mais ne sont pas dans nika.md. Acceptable.

---

## SUCCESS CRITERIA

A la fin de cette session, tu dois avoir :

- [ ] Phase 0 : Build green, 0 failing tests, 14 commits + workflows pushes
- [ ] Phase 1 : 5 security fixes commites (5 deja faits), G01-G07 fail correctement
- [ ] Phase 2 : 3 silent bug fixes (1 deja fait)
- [ ] Phase 3 : 6 nouveaux events
- [ ] Phase 4 : 4 edge case fixes
- [ ] Phase 5 : 81+ workflows (deja crees), 60+ executes avec succes
- [ ] Phase 6 : Provider parity documente
- [ ] Phase 7 : Clippy clean, smoke OK
- [ ] Phase 8 : 5 handoffs ecrits
- [ ] Phase 9 : Rapport final
- [ ] `cargo test --workspace --lib` passe (9000+ tests)
- [ ] Tout commite et pousse sur main
- [ ] CHANGELOG mis a jour

---

---

## METHODOLOGIE BUG TRACKING

**Tiens a jour un fichier `docs/plans/overnight-buglog.md` pendant TOUTE la session :**

```markdown
# Overnight Bug Log — [date]

## Bugs FIXES (commit hash)
| # | Bug | File:Line | Commit | Before | After |
|---|-----|-----------|--------|--------|-------|

## Bugs TROUVES pendant execution
| # | Workflow | Bug | File:Line | Severity | Status |
|---|---------|-----|-----------|----------|--------|

## Workflows EXECUTES
| # | Workflow | Provider | Status | Duration | Cost | Output |
|---|---------|----------|--------|----------|------|--------|

## Features INCOMPLETES decouvertes
| # | Feature | File:Line | Status | Impact |
|---|---------|-----------|--------|--------|

## Artifacts VERIFIES
| # | Workflow | File | Size | Format Valid |
|---|---------|------|------|-------------|
```

**REGLES du bug log :**
1. CHAQUE bug trouve = une ligne, meme si tu le fixes immediatement
2. CHAQUE workflow execute = une ligne avec status/duration/cost
3. CHAQUE artifact ecrit = verifie physiquement (ls, file, jq, wc)
4. Si un workflow "passe" mais l'output est vide/faux = BUG, pas un succes
5. Si un feature est "parsed but not wired" = BUG, le documenter
6. Si un error message est confus/trompeur = BUG, le documenter
7. JAMAIS ignorer un echec — si ca marche pas, c'est un bug

---

**Push souvent. Commit souvent. Fix tout. Sois pas superficiel.**
**Skills : TDD, verification, systematic-debugging, defense-in-depth.**
**Quand tu doutes → systematic-debugging. Quand tu finis → verification-before-completion.**
**TOUT echec = bug. TOUT output vide = bug. TOUT warning = investiguer.**
