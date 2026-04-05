# v0.69+ MEGA BRAINSTORM — 3 Axes Ouverts

> **Context**: v0.68.2 DONE. Feature freeze relaxed. 3 axes open: transforms, CLI, serve V4.
> **Method**: Socratic — questions before code. 5-agent research synthesized.
> **Decision**: Thibaut picks what ships. Everything else stashed.

---

## AXE 1: NOUVEAUX TRANSFORMS (52 existants, 287 tests)

### Constat

Le système est mature. 52 transforms + `| jq(expr)` escape hatch.
Les showcases n'utilisent que 5 transforms en pratique (to_json, shell, trim, length, join).
La question n'est pas "quoi ajouter" mais "quoi manque VRAIMENT".

### Les 3 questions Socratiques

**Q1: `replace(pattern, replacement)` — le #1 manquant?**
- C'est le transform le plus demandé implicitement (users font `| jq('gsub(...)')`)
- Regex ou string literal? Les deux? `replace("old", "new")` + `replace_regex("\\d+", "X")`?
- Ou bien: un seul `replace` qui détecte `/regex/` vs `"literal"`?
- **Risque**: regex = compile cost, security (ReDoS). Faut-on un timeout?

**Q2: Aggregation (add, min, max, sum, avg) — arrays numériques?**
- `| add` → somme d'un array de nombres. Ultra commun en analytics.
- `| min` / `| max` → extremes. Avec `| min_by(field)` / `| max_by(field)` pour objects?
- `| sum` vs `| add` — même chose? Ou `add` = concat arrays + sum numbers?
- `| avg` — mean d'un array. Utile pour benchmarks.
- **Question**: jq fait tout ça (`| jq('add')`, `| jq('min')`). Le gain est la lisibilité?

**Q3: Faut-il des transforms ou faut-il mieux documenter jq()?**
- Le `| jq(expr)` couvre 100% des cas. Chaque nouveau transform = maintenance.
- Argument PRO transforms: discoverability, autocomplete LSP, error messages claires
- Argument PRO jq: zero new code, jq est universel, déjà testé
- **Compromis possible**: top 5 transforms natifs + "jq cookbook" dans la doc?

### Propositions (ranked by ROI)

| # | Transform | Impl LOC | Tests | Justification |
|---|-----------|----------|-------|---------------|
| 1 | `replace(a, b)` | ~30 | 8 | #1 manquant, string lit only (pas regex) |
| 2 | `add` | ~15 | 5 | Sum numbers / concat arrays |
| 3 | `min` / `max` | ~20 | 6 | Array extremes |
| 4 | `min_by(f)` / `max_by(f)` | ~25 | 6 | Object array extremes |
| 5 | `sum` / `avg` | ~20 | 6 | Numeric aggregation |
| 6 | `not` | ~5 | 3 | Boolean negation |
| 7 | `has(key)` | ~10 | 4 | Object introspection |
| 8 | `truncate(n)` | ~10 | 4 | String truncation (prompts) |
| 9 | `count` | ~5 | 3 | Alias for length on arrays |
| 10 | `index_of(val)` | ~15 | 4 | Find position in array/string |

### Ce qu'on n'ajoute PAS (et pourquoi)

- `pad(N, char)` — trop spécialisé, `| jq` suffit
- `enumerate` — `nika:map` avec idx param existe
- `to_entries` / `from_entries` — jq territory
- `date/time` — énorme surface, pas le moment
- `map_values` — jq territory
- `reduce` / `fold` — trop complexe pour un pipe transform
- `walk` / `recurse` — jq territory

---

## AXE 2: NOUVELLES COMMANDES CLI (47 commandes, 4 maturity levels)

### Constat

CLI est riche mais a des trous. Les concurrents (Deno, Cargo, pnpm) ont des patterns que Nika ignore.
Biggest gap: **pas de `nika test`**. Un workflow engine sans test runner c'est comme Cargo sans `cargo test`.

### Les 5 questions Socratiques

**Q1: `nika test` — quoi exactement?**
- Option A: `nika test workflow.nika.yaml` → run avec `provider: mock`, assert exit 0
- Option B: `nika test workflow.nika.yaml --assert output.json` → compare output vs golden
- Option C: `nika test workflow.nika.yaml --snapshot` → snapshot testing (première run = baseline)
- Option D: Fichier `*.test.nika.yaml` qui est un workflow de test (meta!)
- **Question**: mock provider suffit-il? Ou faut-il des assertions sur les events?

**Q2: `nika lint` vs `nika check --lint` ?**
- `check` valide syntax + DAG. `lint` ajouterait: unused tasks, missing descriptions, perf hints
- Commande séparée ou flag sur check?
- Inspiration: `cargo clippy` = linter séparé de `cargo check`. Mais Deno fait `deno lint`.
- **Question**: suffisamment de rules pour justifier une commande séparée?

**Q3: `nika env` / `nika info` — unifier la debug story?**
- Actuellement éparpillé: `provider list`, `config list`, `vault check`, `doctor`, `features`
- Un seul `nika env` qui montre TOUT: version, channel, providers configurés, MCP servers, paths
- Inspiration: `deno info`, `npm config list`, `rustup show`
- **Sous-question**: `nika env` ou `nika status`? Ou `nika doctor --full` suffit?

**Q4: `nika version` — vraiment nécessaire?**
- `--version` existe. Mais `nika version` est plus naturel.
- Ajout trivial (5 LOC). Mais est-ce du bruit?
- **Décision**: oui, c'est gratuit et attendu.

**Q5: `nika upgrade` — self-update binaire?**
- Risqué: binary self-update = trust chain, signature verification
- Homebrew fait ça mieux (`brew upgrade nika`)
- `nika switch dev` rebuild déjà depuis source
- **Question**: est-ce que `nika upgrade` = `brew upgrade nika` wrapper? Ou vrai self-update?

### Propositions (ranked by impact)

| # | Command | Impact | Effort | Notes |
|---|---------|--------|--------|-------|
| 1 | `nika test` | HIGH | 2h | Mock provider + exit code + optional golden file |
| 2 | `nika version` | HIGH | 15min | Trivial, attendu par tous |
| 3 | `nika env` | HIGH | 1h | Unified debug view |
| 4 | `nika lint` | MEDIUM | 3h | 10+ lint rules (unused tasks, missing desc, etc.) |
| 5 | `nika graph` (top-level) | MEDIUM | 15min | Alias pour `nika workflow graph` |
| 6 | `nika login <provider>` | MEDIUM | 30min | UX wrapper: prompt key → test → store |
| 7 | `nika diff` | MEDIUM | 2h | Semantic workflow diff (DAG comparison) |
| 8 | `nika log <job>` | LOW | 1h | Unified job log streaming |
| 9 | `nika upgrade` | LOW | 2h | Self-update (complexe, Homebrew suffit?) |
| 10 | `nika cache list/prune` | LOW | 1h | Cache is underpowered (2 subcommands) |

### UX Fixes (pas de nouvelles commandes)

| # | Fix | Effort | Notes |
|---|-----|--------|-------|
| 1 | `--format json\|text\|yaml` partout | 2h | Remplacer les `--json` incohérents |
| 2 | Help system enrichi | 1h | `nika help run` → detailed options |
| 3 | Global flags documentation | 30min | --detail, --no-live, --color |

---

## AXE 3: SERVE V4 (Axum, SQLite, SSE, 12 endpoints)

### Constat

V3 est solide: Axum, embedded executor, SSE, bearer auth, Prometheus metrics, webhooks.
Mais: single token, SQLite only, no dashboard, no scheduling, no batch.

### Les 5 questions Socratiques

**Q1: WebSocket — remplacer SSE ou en plus?**
- SSE est unidirectionnel (serveur → client). WebSocket est bidirectionnel.
- Use case WS: cancel depuis le client, envoyer des inputs mid-execution, multiplexer
- SSE fonctionne bien pour le streaming events. Faut-il vraiment WS?
- **Question**: est-ce que le SDK (nika-client) a besoin de WS? Ou c'est pour un dashboard?
- **Sous-question**: SSE + WS en parallèle? Ou migration?

**Q2: Multi-tenant auth — pour qui?**
- Single token = Thibaut seul sur son VPS. Multi-tenant = SaaS.
- Est-ce que Nika vise le SaaS avant launch? Ou c'est post-launch?
- **Options**:
  - A: API keys table dans SQLite (simple, 2h)
  - B: JWT + RBAC (complexe, 1 semaine)
  - C: Rester single token, ajouter `X-Client-Id` header pour audit log
- **Question**: quel est le use case réel avant May 5?

**Q3: Dashboard web — build or buy?**
- Option A: Static HTML dashboard served par Nika (comme le site-audit dashboard)
- Option B: Embed un mini-React/Svelte app dans le binaire
- Option C: Scalar (déjà là pour OpenAPI) suffit pour V4?
- **Question**: est-ce que le TUI (88K LOC) ne couvre pas déjà ce besoin?

**Q4: PostgreSQL — quand est-ce nécessaire?**
- SQLite scale à ~1000 concurrent reads, ~50 concurrent writes
- Nika serve avec max_concurrent=6 = jamais un problème
- PostgreSQL = scaling horizontal, mais ajoute une dep externe
- **Question**: est-ce prématuré? SQLite + WAL mode suffit pour des mois?

**Q5: Scheduling (cron) — dans serve ou séparé?**
- Le daemon a déjà `nika job submit` (background). Ajouter cron = 200 LOC.
- Mais: scheduling + retry + dead letter = complexité opérationnelle
- **Options**:
  - A: `schedule:` field dans `POST /v1/run` (cron string)
  - B: `nika schedule` commande CLI séparée
  - C: Laisser l'utilisateur utiliser crontab/systemd timers
- **Question**: combien de users ont BESOIN de scheduling intégré?

### Propositions (ranked by impact for May 5 launch)

| # | Feature | Impact | Effort | Ship? |
|---|---------|--------|--------|-------|
| 1 | `when:` conditional tasks | HIGH | 3h | YES — compétiteurs l'ont tous |
| 2 | `--resume` re-run from failure | HIGH | 4h | YES — killer feature pour prod |
| 3 | API key management (simple) | MEDIUM | 2h | MAYBE — X-Client-Id + audit log |
| 4 | Batch submission endpoint | MEDIUM | 1h | YES — POST /v1/batch/run |
| 5 | Job tags/labels | MEDIUM | 1h | YES — metadata pour filtrage |
| 6 | WebSocket (alongside SSE) | MEDIUM | 4h | DEFER — SSE fonctionne |
| 7 | Dashboard (static HTML) | MEDIUM | 4h | DEFER — TUI + Scalar suffisent |
| 8 | Schedule/cron | LOW | 3h | DEFER — systemd timers suffisent |
| 9 | PostgreSQL backend | LOW | 8h | DEFER — SQLite WAL suffit |
| 10 | RBAC/JWT | LOW | 8h | DEFER — post-launch |

---

## SYNTHESE COMPETITIVE

### Nika est AHEAD sur

1. **Structured output 5-layer defense** — aucun concurrent n'a ça cross-provider
2. **Single binary** — zero deps vs pip/Docker/Node.js
3. **CAS + 62 builtin tools** — media pipeline intégré
4. **Security-first exec** — blocklist, SSRF, `| shell` mandatory
5. **Learning course intégré** — 12 niveaux, 44 exercices, 115 showcases

### Nika est BEHIND sur

1. **Observability** — pas de trace explorer visuel (LangSmith, Prefect ont ça)
2. **Evaluation framework** — pas de `nika eval` (PromptFlow, DSPy)
3. **Conditional execution** — pas de `when:` / `if:` (LangGraph, n8n, Make)
4. **Resume from failure** — pas de `--resume` (Prefect, Temporal)
5. **Scheduling** — pas de cron intégré (Prefect, Airflow, n8n)

### Features à NE PAS copier

1. Abstraction towers (LangChain) — 5 verbs forever
2. GUI-first (Dify/n8n) — YAML is source of truth
3. SaaS-only observability (LangSmith) — offline-first always
4. Unlimited agent autonomy (AutoGen) — every agent has a leash
5. Provider-specific optimizations — all providers, same semantics
6. Enterprise feature gating (Temporal) — all features free (AGPL)

---

## MEGA QUESTION POUR THIBAUT

Trois chemins possibles pour v0.69 :

### Chemin A: "Polish & Ship" (conservative, 2 sessions)
- 5 transforms (replace, add, min, max, not)
- 3 CLI commands (version, env, test)
- 0 serve changes
- **Risque**: rien de "wow" pour le launch

### Chemin B: "Killer Features" (ambitieux, 4 sessions)
- 5 transforms + `when:` conditional tasks
- `nika test` + `nika lint` + `nika version` + `nika env`
- `--resume` from failure + batch endpoint
- **Risque**: scope creep, bugs before launch

### Chemin C: "Competitive Moat" (agressif, 6+ sessions)
- 10 transforms + `when:` + `on_error:` fallback
- Full CLI completion (test, lint, diff, env, version, graph)
- Serve V4 (WebSocket, dashboard, scheduling)
- `nika eval` framework
- **Risque**: impossible avant May 5

### La vraie question:
> **Qu'est-ce qui fait que quelqu'un essaie Nika le jour du launch?**
> - Est-ce 5 transforms de plus? Non.
> - Est-ce `nika test`? Peut-être.
> - Est-ce `when:` conditional? Probablement.
> - Est-ce `--resume`? Oui, pour les devs sérieux.
> - Est-ce un dashboard? Non, le TUI suffit.
>
> **Proposition: Chemin B avec focus sur `when:` + `--resume` + `nika test`.**
> Ce sont les 3 features qui font la différence entre "toy" et "production-ready".

---

## NEXT STEPS

1. Thibaut choisit le chemin (A, B, C, ou mix)
2. On découpe en sprints de 30min-2h
3. TDD: test first, code second
4. 1 fix = 1 commit, tests verts, clippy zéro
5. Push HTTPS, tag quand stable
