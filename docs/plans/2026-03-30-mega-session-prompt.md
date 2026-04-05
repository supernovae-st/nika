# MEGA SESSION v5 — Post-Overnight, Remaining Fixes + Deep Testing

> **Copie ce fichier ENTIER comme premier message dans une nouvelle session Claude Code.**
> Budget: $20-30 API | Mode: Fix remaining bugs, deep testing, exhaustive verification

---

## IDENTITY

Tu es un ingenieur senior Rust travaillant sur Nika, un workflow engine YAML pour l'IA.
Tu travailles en FULL AUTONOMIE. Tu fixes les bugs restants, executes TOUS les workflows avec de vrais providers, verifies chaque output, et documentes tout.

---

## CODEBASE

```
Repo:       /Users/thibaut/dev/supernovae/nika
Version:    v0.55.0 (released, 18 commits ahead of origin/main, NOT pushed)
Tests:      2,153 passing, 0 failing, 1 ignored
LOC:        356K Rust, 12 crates
Binary:     ./tools/target/debug/nika (10 features: native-inference, media-thumbnail, media-optimize, media-chart, fetch-html, fetch-markdown, fetch-article, fetch-feed, tui, lsp)
Workflows:  91 E2E workflows in tests/e2e-overnight/ (91/91 nika check OK)
Smoke:      41 pass, 1 security issue (G06), 1 skip (media-thumbnail feature)
```

### Providers disponibles
- **OpenAI** : gpt-4o-mini (OPENAI_API_KEY) ✓
- **xAI** : grok-3-fast (XAI_API_KEY) ✓
- **Gemini** : gemini-2.0-flash (GEMINI_API_KEY) ✓
- **Anthropic** : cle presente mais 0 credits — utiliser pour tests fallback
- **Native** : llama3.2:1b, mistral:7b — GGUF local (needs `nika model pull`)

### ATTENTION — Etat git
- **18 commits NON pushes** — `git push` en premier !
- **0 fichiers code modifies** — code clean
- **Quelques docs untracked** dans docs/research/ — pas critique

---

## DOCUMENTS A CHARGER

1. `docs/plans/2026-03-30-overnight-mega-plan.md` — Plan original (reference historique)
2. `docs/plans/2026-03-30-overnight-companion.md` — Knowledge base (limites, error codes, tests Rust)
3. `~/.claude/rules/nika.md` — Schema complet, 5 verbs, providers
4. `~/.claude/rules/nika-bugs-and-patterns.md` — Bugs connus, workarounds

---

## REGLES ABSOLUES

1. **TDD** — test failing d'abord, fix, verify
2. **1 fix = 1 commit** — `type(scope): description` + co-authors :
   ```
   Co-Authored-By: Nika 🦋 <nika@supernovae.studio>
   ```
3. **`cargo test --workspace --lib`** DOIT passer apres CHAQUE fix
4. **JAMAIS `cargo test` sans `--lib`** (keychain popup macOS)
5. **Prompts structured output** = langage NATUREL, jamais mentionner JSON
6. **AGPL-3.0-or-later** pour tous les crates
7. **`git add <specific files>`** — jamais `git add .`
8. Push apres chaque 3-5 commits

---

## SKILLS

```
/spn-powers:test-driven-development        — AVANT chaque fix
/spn-powers:verification-before-completion — AVANT de dire "c'est fait"
/spn-powers:systematic-debugging           — Quand un workflow echoue
/spn-powers:requesting-code-review         — Apres chaque phase
/spn-rust:rust                             — Pour tout code Rust
```

---

## CE QUI A DEJA ETE FAIT (ne pas refaire)

La session overnight + VPS sprint ont fixe 24/25 bugs planifies :

### Phase 1 Security — 100% DONE
- ~~C01~~ Newline injection (exec.rs:38 `is_shell`) — commit deb342e8d
- ~~C02~~ IPv6 :: SSRF (policy.rs UNSPECIFIED) — commit 732475d87
- ~~C03~~ SECRET_RE (ASIA, ghu_, SG., eyJ) — commit cef99a748
- ~~C04~~ MCP redaction (`redact_value()` 4 sites) — commit 8de44a5e0
- ~~C05~~ Quoted pattern (baseline)

### Phase 2 Silent Bugs — 100% DONE
- ~~H01~~ unwrap_or_default → `.expect()` — commit f1917e29b
- ~~H02~~ "null" coercion removed — commit 0f9bd73dd

### Phase 3 Telemetry — 75% DONE
- ~~H03~~ ForEachItem events (3 variants) — commit 6da352f27
- ~~H04~~ TaskCancelled event — commit 6da352f27
- ~~H05~~ FallbackChainExhausted event — commit 6da352f27

### Phase 4 Edge Cases — 50% DONE
- ~~H08~~ for_each MAX_FOR_EACH_ITEMS=10000 — commit c27fe6ae4
- ~~NB03~~ Child agent security policies — commit 67c1deb8e

### Extras fixes
- O(n^2) ready_tasks → O(remaining)
- Semaphore NIKA-028
- fail_fast=false cancelled items
- Broadcast 1024→4096
- Shell transform null
- Pipe parser quote bug
- Artifact path collisions
- Mock file schemas + depth limit
- TOCTOU race fix
- NikaVault encrypted secrets
- Provider exponential backoff
- Custom endpoint SSRF auto-allow
- Daemon hardening (systemd, SQLite, graceful shutdown)
- E2E test assertions (E01, D01, D03)

---

## CE QUI RESTE A FAIRE

### Phase A : Push + Stabilize (~5min)
```bash
git push                                    # 18 commits en attente
cargo test --workspace --lib                # 2153 pass, 0 fail
bash tests/e2e-overnight/run-smoke.sh ./tools/target/debug/nika  # 41 pass
```

### Phase B : 8 Bugs Restants (~3h)

| # | Bug | File:Line | Fix | Effort |
|---|-----|-----------|-----|--------|
| B1 | `contains_unquoted()` seulement backtick | security.rs:249-255 | Appliquer a TOUS les patterns shell-mode, pas juste `` ` `` | 30min |
| B2 | StructuredOutputTimeout event manquant | nika-event/src/log.rs + structured_output.rs | Ajouter variant + emit avant timeout error | 30min |
| B3 | Pas de cancellation dans path traversal | runner.rs:~1965 | `if cancel_token.is_cancelled()` dans la boucle | 15min |
| B4 | Binding from failed task silencieux | runner.rs:~1936 | `tracing::warn!` quand source task est Failed | 15min |
| B5 | `scope:` silently ignored | rig_agent_loop/mod.rs:298 | Emettre un warning "scope: is not yet implemented" | 15min |
| B6 | `type: llm` guardrails fail runtime | guardrails.rs + thinking.rs:57 | Rejeter au parse time avec NIKA-112 au lieu de runtime | 30min |
| B7 | Unknown model costs = silently 0.0 | cost.rs / executor/infer.rs | `tracing::warn!` quand pricing manquant | 15min |
| B8 | `enable_extractor: true` hard error | structured_output.rs | Rejeter au parse time avec message clair | 15min |

**Pour chaque bug :** TDD (test fail → fix → test pass → commit).

### Phase C : Executer TOUS les 91 Workflows (~4h)

**91 workflows deja crees.** Les executer dans cet ordre :

**GRATUIT (pas d'API) — ~50 workflows :**
1. G01-G07 (security, 7) — DOIVENT TOUS ECHOUER
2. E01-E08 (exec/invoke, 8)
3. S01-S05 (stress, 5)
4. D01-D07 (DAG/for_each, 7)
5. F01-F12 (media/chart, 10) — VERIFIER artifacts physiquement
6. C01-C09 (fetch 9 modes, 9)
7. R01-R03 (artifacts, 3)
8. I02-I03 (context/skills, 2)
9. V02-V04 (verification, 3)

**NATIF (gratuit, needs model download) — 7 workflows :**
```bash
nika model pull llama3.2:1b    # ~1GB
nika model pull mistral:7b     # ~4GB
```
10. N01-N08 (native GGUF, 7)

**API PAYANT (~$2-3) — ~35 workflows :**
11. A01-A08 (structured output, 8)
12. B01-B07 (agent, 5)
13. H01-H06 (real-world, 6)
14. W01-W03 (vision, 3)
15. X01-X02 (combos multi-verbs, 2)
16. M01-M03 (multi-provider, 3)
17. T01-T03 (telemetry, 3)
18. V01 (redaction, 1)

**Pour CHAQUE workflow :**
```bash
./tools/target/debug/nika run tests/e2e-overnight/XX.nika.yaml --no-live 2>&1 | tee /tmp/XX.log
echo "EXIT: $?"

# Verifier l'output est LOGIQUEMENT correct (pas juste non-vide)
# Verifier les events dans l'output
# Verifier les secrets ne sont PAS leakes
grep -iE "(sk-|AKIA|ASIA|ghp_|ghu_|eyJ)" /tmp/XX.log  # DOIT ETRE VIDE

# Pour les media/artifacts — verifier PHYSIQUEMENT
ls -la tests/e2e-overnight/output/
file tests/e2e-overnight/output/*.png 2>/dev/null
```

**G06 NOTE IMPORTANTE :**
G06 (newline injection) PASSE encore dans le smoke test MAIS le fix C01 est applique (exec.rs:38 utilise `is_shell`).
Le probleme est que le workflow G06 ne produit pas un VRAI newline dans la commande resolue.
→ Il faut verifier : est-ce que `printf 'echo safe\\necho INJECTED'` produit reellement un `\n` dans l'output ? Si oui, le fix devrait le bloquer. Si non, le test est mal concu.
→ Tester manuellement : `printf 'echo safe\necho INJECTED'` dans un shell pour confirmer.

### Phase D : Analyse des Resultats (~1h)

**Creer `docs/plans/overnight-buglog.md` :**
```markdown
# Overnight Bug Log

## Bugs FIXES (avec commit hash)
| # | Bug | File:Line | Commit |

## Bugs TROUVES pendant execution
| # | Workflow | Bug | Severity |

## Workflows EXECUTES
| # | Workflow | Provider | Status | Duration | Output Summary |

## Features INCOMPLETES
| # | Feature | File:Line | Impact |

## Artifacts VERIFIES
| # | File | Size | Format |
```

**REGLES :** CHAQUE echec = un bug. CHAQUE output vide = un bug. JAMAIS ignorer.

### Phase E : Handoffs Finals (~1h)

Creer/mettre a jour 5 handoff prompts :

1. **`handoff-sprint-security.md`** — C05 quoted pattern + tout reste security
2. **`handoff-sprint-agent.md`** — scope wiring, LLM guardrails, max_tokens, presets
3. **`handoff-sprint-runner.md`** — cancellation binding, binding failed task
4. **`handoff-sprint-telemetry.md`** — StructuredOutputTimeout, cost warning
5. **`handoff-sprint-polish.md`** — enable_extractor, orphaned error codes, documentation

### Phase F : Rapport Final

**Creer `docs/plans/overnight-results.md` :**
- Tous les bugs fixes (avec commit)
- Tous les workflows executes (avec resultat)
- Provider parity analysis
- Artifact verification
- Vision test results
- Chart generation results
- Native GGUF results
- Recommendations pour v0.56

---

## VERIFICATION PROTOCOLS

### NDJSON
```bash
./tools/target/debug/nika run workflow.nika.yaml --no-live 2>&1 | tee /tmp/run.log
grep -i "NIKA-\|error\|failed" /tmp/run.log
grep -iE "(sk-|AKIA|ASIA|ghp_|ghu_|eyJ)" /tmp/run.log  # DOIT ETRE VIDE
```

### Media
```bash
ls -la tests/e2e-overnight/output/
file tests/e2e-overnight/output/*.png 2>/dev/null
for f in tests/e2e-overnight/output/*.json; do [ -f "$f" ] && jq . "$f" > /dev/null 2>&1 && echo "OK: $f" || echo "INVALID: $f"; done
```

### Native
```bash
grep -i "native\|gguf\|llama\|mistral" /tmp/run.log | head -5
```

---

## BOUCLE SOCRATIQUE

### Apres chaque fix :
1. Mon test FAIL avant et PASS apres ?
2. Ai-je grep pour patterns similaires ?
3. Ce fix merite CHANGELOG ?
4. Commit avec co-authors ?

### Apres chaque workflow :
1. Reussi pour la BONNE raison ?
2. Output LOGIQUEMENT correct ?
3. Events attendus presents ?
4. Secrets PAS leakes ?

### Apres chaque phase :
```bash
cargo test --workspace --lib
bash tests/e2e-overnight/run-smoke.sh ./tools/target/debug/nika
git push
```

---

## RECOVERY

- **Compilation fail** : Lire l'erreur. Fix dans le meme commit.
- **Tests fail** : `cargo test -- --nocapture test_name`. Comprendre POURQUOI.
- **Provider 429** : Attendre 60s. Switch provider si persistent.
- **50+ tests cassent** : `git stash`. Repenser l'approche.
- **Context window plein** : Commit, push, handoff, nouvelle session.

---

## BUGS RESTANTS — REGISTRE COMPLET

### A fixer dans cette session (8 bugs)
| # | Bug | File:Line | Severity |
|---|-----|-----------|----------|
| B1 | `contains_unquoted()` only backtick | security.rs:249 | MEDIUM |
| B2 | StructuredOutputTimeout event | log.rs + structured_output.rs | MEDIUM |
| B3 | Cancellation in binding traversal | runner.rs:~1965 | MEDIUM |
| B4 | Binding from failed task silent | runner.rs:~1936 | LOW |
| B5 | scope: silently ignored | rig_agent_loop/mod.rs:298 | LOW |
| B6 | type:llm guardrails runtime fail | guardrails.rs + thinking.rs:57 | MEDIUM |
| B7 | Unknown model cost = 0.0 | cost.rs | LOW |
| B8 | enable_extractor hard error | structured_output.rs | LOW |

### Documenter dans handoffs (non-bloquants)
- scope: implementation (full/minimal/debug tool sets)
- LLM guardrails type:llm implementation
- max_tokens(8192) → per-provider defaults (8 instances)
- Agent presets (from: field) — parsing existe, pas de builtin presets
- Orphaned error codes (NIKA-007-009, 023-025, 061-069)
- native provider bloque pour agent: verb
- output: vs structured: code paths dupliques

---

## SUCCESS CRITERIA

- [ ] Phase A : 18 commits pushes, build green
- [ ] Phase B : 8 bugs fixes et commites
- [ ] Phase C : 91 workflows executes, 80+ succes
- [ ] Phase C : TOUS les artifacts verifies physiquement
- [ ] Phase C : ZERO secrets leakes dans les outputs
- [ ] Phase C : G01-G07 TOUS echouent correctement
- [ ] Phase C : Vision W01-W03 fonctionnent
- [ ] Phase C : Charts F07-F09 generent des PNG valides
- [ ] Phase C : Native N01-N08 fonctionnent (si models telecharges)
- [ ] Phase D : overnight-buglog.md complet
- [ ] Phase E : 5 handoffs ecrits
- [ ] Phase F : overnight-results.md avec tout
- [ ] `cargo test --workspace --lib` passe (2153+ tests)
- [ ] Tout commite et pousse sur main

---

**TOUT echec = bug. TOUT output vide = bug. TOUT warning = investiguer.**
**Push souvent. Commit souvent. Fix tout. Sois pas superficiel.**
