# Linear Setup — Nika Diamond Project Tracking

## Pre-requis

- Compte Linear (gratuit suffit pour solo dev)
- `.mcp.json` déjà configuré dans le repo (Linear MCP server)

## Setup en 5 étapes (~25 min)

### Étape 1 — Créer workspace + team (5 min)

1. Aller sur [linear.app](https://linear.app)
2. Créer workspace **"SuperNovae Studio"**
3. Créer team **"Nika"** avec identifier prefix **`NIK`**
   (chaque issue sera NIK-1, NIK-2, NIK-3...)
4. Se mettre comme default assignee

### Étape 2 — GitHub integration (3 min)

1. Settings → Integrations → GitHub → Connect
2. Autoriser l'org **SuperNovae-studio**
3. Sélectionner le repo **nika**
4. Activer :
   - ✅ Auto-status on branch create (→ In Progress)
   - ✅ Auto-close on PR merge (→ Done)
5. Branch naming : `thibaut/nik-42-description`

### Étape 3 — Labels (5 min)

Créer ces labels dans Team Settings → Labels :

**Par type** :
- `craft` (rewrite propre d'une crate)
- `deletion` (nuke legacy code)
- `bug` (fix trouvé pendant admission)
- `shadow-zone` (pre-launch gate)
- `dx` (tooling, CI, hooks)

**Par layer** :
- `L0-pure`, `L0.5-kernel`, `L1-effects`, `L2-domain`,
  `L3-orchestration`, `L4-interfaces`

**Par priorité** (Linear built-in) :
- Urgent / High / Medium / Low / No priority

### Étape 4 — Tester MCP (2 min)

Dans Claude Code :

```
"List my Linear teams"
```

→ Première fois : OAuth popup dans le browser, autoriser
→ Résultat : "Team: Nika (NIK)"

Tester création :

```
"Crée une issue Linear dans Nika : test MCP connection, label dx"
```

→ Résultat : NIK-1 créé, visible dans Linear

### Étape 5 — Seed Phase 1 (10 min)

Créer un cycle ouvert (sans deadline fixe — durée = jusqu'à ce que les gates soient vertes) + issues Phase 1. Per `feedback_no_deadlines.md` et diamond-discipline Rule 6 : pas de pression timeline, quality > speed.

Via Claude Code ou manuellement :

```
Issues Phase 1 — split nika-core :

NIK-2  [L0] Admit nika-error — Gate 1-12 (label: craft, L0-pure)
NIK-3  [L0] Admit nika-catalog — Gate 1-12 (label: craft, L0-pure)
NIK-4  [L0.5] Admit nika-kernel + nika-kernel-mock (label: craft, L0.5-kernel)
NIK-5  [L0] Admit nika-schema-ast (label: craft, L0-pure)
NIK-6  [L0] Admit nika-schema-analyze (label: craft, L0-pure)
NIK-7  [L0] Admit nika-binding (label: craft, L0-pure)
```

Ajouter les 6 issues au cycle actuel.

## Usage quotidien

### Quand je commence une crate

```
"Move NIK-2 to In Progress"
```

Ou : crée une branche `thibaut/nik-2-admit-nika-error`
→ Linear auto-détecte → status = In Progress

### Quand une crate est admise (12 gates green)

```
"Move NIK-2 to Done, comment: commit SHA abc123, 800 LOC, mutation 94%"
```

Ou : merge le PR avec `NIK-2` dans le titre → auto-Done

### Pour voir l'état du projet

```
"Show me all open Nika issues in current cycle"
```

### Pour reporter une shadow zone

```
"Create Nika issue: nika serve input trust P0 shadow zone, label shadow-zone, priority urgent"
```

## Workflow complet

```
Chaque crate = 1 Linear issue
  1. Issue créée (Backlog)
  2. Ajoutée au cycle ouvert (Todo)
  3. Début travail (In Progress) — branch ou manual move
  4. 12 gates en cours (In Progress)
  5. Gates green, commit, push (In Review)
  6. PR merged / admission confirmée (Done)
```

## MCP tools disponibles via Claude Code

Le serveur Linear MCP expose :

| Commande | Action |
|----------|--------|
| Créer issue | `mcp__linear__save_issue` |
| Update issue | `mcp__linear__save_issue` (avec id) |
| Lister issues | `mcp__linear__list_issues` |
| Commenter | `mcp__linear__save_comment` |
| Voir cycles | `mcp__linear__list_cycles` |
| Voir labels | `mcp__linear__list_issue_labels` |

Tu peux dire en langage naturel "crée une issue..." et Claude traduit.

## Pricing

Linear Free tier = suffisant :
- Unlimited members
- Cycles, labels, GitHub integration
- MCP server access
- Pas besoin de plan payant pour solo dev
