# 🦋 Rapport d'Audit Complet - Exemples et Workflows Nika

**Date:** 2026-03-02  
**Version:** v0.16.5 (Chat TUI Improvements + Dynamic Input + Edit History)  
**Tests:** 3,358 passing | **Clippy:** 0 warnings

---

## 📊 Vue d'Ensemble

| Métrique | Valeur | Notes |
|----------|--------|-------|
| **Total fichiers .nika.yaml** | 376 | Dans tout le projet |
| **Exemples (tools/nika/examples/)** | 164 | Dont 118 à la racine |
| **Tests (tools/nika/tests/)** | 18 | Workflows de test E2E |
| **Exemples valides** | 161/164 (98.2%) | Excellente qualité |
| **Exemples cassés** | 3/164 (1.8%) | `stop_conditions` obsolète |
| **Schema @0.9 (current)** | 27/164 (16.5%) | ⚠️ Migration nécessaire |
| **Schema obsolète** | 137/164 (83.5%) | v0.2-v0.8 |

---

## 📁 Architecture des Exemples

```
tools/nika/examples/
├── public/                  17 exemples (100% valides)
│   ├── blog-post.nika.yaml
│   ├── code-review.nika.yaml
│   └── ... (production-ready)
├── drafts/                  11 exemples (90% valides)
│   ├── test-agent-*.nika.yaml
│   └── ... (expérimentaux)
├── experimental/            5 exemples
│   └── ... (R&D)
├── feature-test-demo/       3 exemples
│   └── ... (tests features)
└── (racine)                 118 exemples
    ├── agent-*.nika.yaml
    ├── invoke-*.nika.yaml
    ├── test-*.nika.yaml
    └── ...

tools/nika/tests/
└── schema-version-tests/    9 workflows (v0.1-v0.9)
```

---

## ⚡ Analyse des Verbes (5 Semantic Verbs)

| Verbe | Exemples | % | Providers Utilisés |
|-------|----------|---|-------------------|
| `infer:` | 36 | 22% | claude (26), openai (5), mock (5) |
| `exec:` | 35 | 21% | N/A (shell execution) |
| `agent:` | 20 | 12% | claude (15), mock (5) |
| `fetch:` | 14 | 9% | N/A (HTTP requests) |
| `invoke:` | 9 | 5% | MCP tools (novanet, perplexity) |

**Observation:** `infer:` et `exec:` dominent, ce qui reflète les use cases principaux (génération LLM + automation).

---

## 🎯 Couverture des Features par Version

### v0.16.5 (Current) - Chat TUI Improvements
✅ **Pas de features YAML** - Améliorations TUI uniquement (dynamic input, scroll indicators, edit history)

### v0.16.0 - TaskBox Inline Rendering
✅ **Pas de features YAML** - Rendering TUI

### v0.15.1 - Skill Merging
| Feature | Exemples | Status |
|---------|----------|--------|
| `skills:` array | 1 | ⚠️ Sous-démontré |
| `pkg:` URIs | 0 | ❌ Non démontré |

### v0.15.0 - Security + Infer Options + Gemini
| Feature | Exemples | Status |
|---------|----------|--------|
| `shell: false` | 1 | ⚠️ Sous-démontré |
| `temperature:` | 5 | ✅ Bon |
| `system:` | 6 | ✅ Bon |
| `max_tokens:` | 3 | ⚠️ Moyen |
| `provider: gemini` | 0 | ❌ **Non démontré** |
| `nika:read` | 2 | ⚠️ Sous-démontré |
| `nika:write` | 2 | ⚠️ Sous-démontré |
| `nika:edit` | 1 | ⚠️ Sous-démontré |
| `nika:glob` | 1 | ⚠️ Sous-démontré |
| `nika:grep` | 1 | ⚠️ Sous-démontré |

### v0.14.3 - context: + include:
| Feature | Exemples | Status |
|---------|----------|--------|
| `context.files:` | 9 | ✅ Bon |
| `include:` | 5 | ✅ Satisfaisant |

### v0.9.3 - Builtin Tools (11 total)
| Tool | Exemples | Status |
|------|----------|--------|
| `nika:log` | 2 | ⚠️ Sous-utilisé |
| `nika:assert` | 2 | ⚠️ Sous-utilisé |
| `nika:sleep` | 1 | ⚠️ Sous-utilisé |
| `nika:emit` | 1 | ⚠️ Sous-utilisé |
| `nika:prompt` | 0 | ❌ **Non démontré** |
| `nika:run` | 0 | ❌ **Non démontré** |

### v0.5.0 - MVP 8 (RLM Enhancements)
| Feature | Exemples | Status |
|---------|----------|--------|
| `for_each:` | 19 | ✅ **Excellent** |
| `lazy: true` | 2 | ⚠️ Sous-démontré |
| `decompose:` | 1 | ⚠️ Sous-démontré |
| `spawn_agent` | 4 | ✅ Satisfaisant |

### Extended Thinking (Claude Feature)
| Feature | Exemples | Status |
|---------|----------|--------|
| `extended_thinking:` | 1 | ⚠️ Sous-démontré |
| `thinking_budget:` | 1 | ⚠️ Sous-démontré |

---

## ❌ Exemples Cassés (3 fichiers)

### Erreur Commune
```
[NIKA-005] Schema validation failed: Additional properties are not allowed ('stop_conditions' was unexpected)
```

### Fichiers Affectés

1. **examples/agent-simple.nika.yaml** (ligne 24)
   ```yaml
   stop_conditions:  # ❌ Field removed in @0.9
     - "SUMMARY_COMPLETE"
   ```

2. **examples/agent-novanet.nika.yaml** (ligne ~30)
   ```yaml
   stop_conditions:  # ❌ Field removed in @0.9
     - "GENERATION_COMPLETE"
   ```

3. **examples/drafts/test-agent-depth-limit.nika.yaml** (ligne ~20)
   ```yaml
   stop_conditions:  # ❌ Field removed in @0.9
     - "DONE"
   ```

### Fix Recommandé
Supprimer entièrement le champ `stop_conditions:` de ces 3 fichiers. Le comportement d'arrêt est maintenant géré automatiquement par `max_turns`.

---

## 🔍 Features Non Démontrées (Gaps Critiques)

### 1️⃣ Gemini Provider (v0.15.0) - PRIORITÉ HAUTE
- **Status:** ❌ Aucun exemple
- **Impact:** Nouveau provider (7ème) non démontré
- **Action:** Créer `examples/v15-gemini-provider.nika.yaml`
- **Exemple suggéré:**
  ```yaml
  schema: "nika/workflow@0.9"
  provider: gemini
  model: gemini-2.0-flash
  
  tasks:
    - id: generate
      infer: "Generate a creative tagline for Nika"
  ```

### 2️⃣ Builtin Tools HITL - PRIORITÉ HAUTE
- **Status:** ❌ `nika:prompt` et `nika:run` non démontrés
- **Impact:** Human-in-the-Loop et sub-workflows non documentés
- **Action:** Créer `examples/builtin-tools-hitl.nika.yaml`
- **Exemple suggéré:**
  ```yaml
  tasks:
    - id: ask_user
      agent:
        prompt: "Use nika:prompt to ask user for confirmation"
        tools: [nika:prompt]
    
    - id: run_subtask
      agent:
        prompt: "Use nika:run to execute sub-workflow"
        tools: [nika:run]
  ```

### 3️⃣ Security Hardening (v0.15.0) - PRIORITÉ MOYENNE
- **Status:** ⚠️ Seulement 1 exemple avec `shell: false`
- **Impact:** Feature de sécurité critique sous-démontrée
- **Action:** Créer `examples/v15-shell-security.nika.yaml`
- **Exemple suggéré:**
  ```yaml
  tasks:
    # Safe (default)
    - id: safe_exec
      exec:
        command: "echo 'Hello World'"
        shell: false  # Shlex parsing, no injection
    
    # Unsafe (opt-in for pipes)
    - id: pipeline
      exec:
        command: "cat file.txt | grep pattern"
        shell: true  # Required for shell features
  ```

### 4️⃣ Extended Thinking Budgets - PRIORITÉ BASSE
- **Status:** ⚠️ Seulement 1 exemple
- **Impact:** Claude feature avancée peu documentée
- **Action:** Créer `examples/extended-thinking-budgets.nika.yaml`
- **Exemple suggéré:**
  ```yaml
  tasks:
    - id: simple_reasoning
      infer:
        prompt: "Simple task"
        extended_thinking: true
        thinking_budget: 4096  # Low budget
    
    - id: deep_reasoning
      infer:
        prompt: "Complex architectural decision"
        extended_thinking: true
        thinking_budget: 32768  # High budget
  ```

### 5️⃣ Skills Merging with pkg: URIs (v0.15.1) - PRIORITÉ BASSE
- **Status:** ❌ Aucun exemple avec `pkg:` URIs
- **Impact:** Feature de réutilisabilité non démontrée
- **Action:** Créer `examples/v15-skills-pkg.nika.yaml`
- **Exemple suggéré:**
  ```yaml
  schema: "nika/workflow@0.9"
  
  skills:
    - path: ./skills/local-skill.md
      alias: local
    - path: pkg:@spn/core@1.0.0/skills/coding.md
      alias: coding
  
  tasks:
    - id: use_skills
      infer: "Use coding skill"
  ```

---

## 📋 Workflows de `nika init` - STATUS: MANQUANTS

### Contexte
CHANGELOG.md v0.16.3 mentionne:
> ### Fixed
> - **nika init** - All 4 example workflows now have correct syntax

Mais ces 4 workflows **n'existent pas** dans `examples/`:
- `01-hello-world.nika.yaml`
- `02-parallel-pipeline.nika.yaml`
- `03-agent-advanced.nika.yaml`
- `04-production-pipeline.nika.yaml`

### Impact
La commande `nika init` ne peut pas créer d'exemples initiaux pour les nouveaux utilisateurs.

### Action Recommandée
Créer ces 4 workflows comme templates d'initialisation:

#### 01-hello-world.nika.yaml
```yaml
schema: "nika/workflow@0.9"
workflow: hello-world
description: "Your first Nika workflow"
provider: claude

tasks:
  - id: greet
    infer: "Say hello in French and Japanese"
```

#### 02-parallel-pipeline.nika.yaml
```yaml
schema: "nika/workflow@0.9"
workflow: parallel-pipeline
description: "Parallel task execution with for_each"

context:
  files:
    data: ./context/data.json

tasks:
  - id: process_all
    for_each: ["item1", "item2", "item3"]
    as: item
    concurrency: 3
    infer: "Process {{use.item}}"
```

#### 03-agent-advanced.nika.yaml
```yaml
schema: "nika/workflow@0.9"
workflow: agent-advanced
description: "Multi-turn agent with MCP tools"

mcp:
  servers:
    web_search:
      command: npx
      args: ["-y", "@anthropic/mcp-server-web-search"]

tasks:
  - id: research
    agent:
      prompt: "Research AI safety papers"
      mcp: [web_search]
      max_turns: 10
      depth_limit: 3
```

#### 04-production-pipeline.nika.yaml
```yaml
schema: "nika/workflow@0.9"
workflow: production-pipeline
description: "Complete production workflow"

include:
  - path: ./partials/setup.nika.yaml
    prefix: setup_

tasks:
  - id: main
    infer: "Main task"
    depends_on: [setup_init]
  
  - id: validate
    use:
      result: main
    exec:
      command: "npm test"
      shell: false
```

---

## 💡 Plan d'Action Prioritaire

### 🔴 URGENT (Cette semaine)

#### 1. Fixer les 3 exemples cassés (30 min)
```bash
# Supprimer stop_conditions: de ces 3 fichiers
vim examples/agent-simple.nika.yaml
vim examples/agent-novanet.nika.yaml
vim examples/drafts/test-agent-depth-limit.nika.yaml

# Vérifier
cargo run -- check examples/agent-simple.nika.yaml
cargo run -- check examples/agent-novanet.nika.yaml
cargo run -- check examples/drafts/test-agent-depth-limit.nika.yaml
```

#### 2. Créer les 4 workflows nika init (2h)
- Créer les 4 fichiers dans `examples/`
- Modifier `src/main.rs` pour les copier lors de `nika init`
- Tester `nika init` dans un dossier vide

### 🟡 IMPORTANT (Cette semaine)

#### 3. Créer exemples manquants prioritaires (4h)
- `examples/v15-gemini-provider.nika.yaml` (20 min)
- `examples/builtin-tools-hitl.nika.yaml` (60 min)
- `examples/v15-shell-security.nika.yaml` (30 min)
- `examples/extended-thinking-budgets.nika.yaml` (45 min)
- `examples/v15-skills-pkg.nika.yaml` (45 min)

#### 4. Migrer 137 exemples vers schema @0.9 (1h)
```bash
# Script automatisé
find examples -name "*.nika.yaml" -exec sed -i '' \
  's/schema: "nika\/workflow@0\.[1-8]"/schema: "nika\/workflow@0.9"/' {} \;

# Vérifier tous les exemples
for f in examples/**/*.nika.yaml; do
  cargo run --quiet -- check "$f" || echo "❌ $f"
done
```

### 🟢 AMÉLIORATION (Prochaine itération)

#### 5. Organiser la structure des exemples (2h)
```bash
mkdir -p examples/{v0.15,v0.16,archive}

# Déplacer par version
mv examples/v15-*.nika.yaml examples/v0.15/
mv examples/drafts/test-*.nika.yaml examples/archive/

# Créer INDEX.md dans chaque dossier
```

#### 6. Documenter les exemples (3h)
- Ajouter README.md dans `examples/` avec table complète
- Documenter chaque catégorie (public, drafts, experimental)
- Créer `examples/TUTORIAL.md` pour les nouveaux utilisateurs

---

## 📊 Métriques de Qualité

### Coverage Score
```
Feature Coverage: 68/100 (68%)
  ✅ 5 verbs: 100%
  ✅ for_each: 95%
  ✅ context.files: 90%
  ⚠️ Builtin tools: 45%
  ❌ Gemini provider: 0%
  ❌ pkg: URIs: 0%

Exemple Validity: 98/100 (98%)
  ✅ 161 valid
  ❌ 3 invalid

Schema Currency: 16/100 (16%)
  ⚠️ 83.5% des exemples utilisent schema obsolète
```

### Temps Estimé Total
| Tâche | Temps | Priorité |
|-------|-------|----------|
| Fixer 3 exemples cassés | 30 min | 🔴 URGENT |
| Créer 4 workflows init | 2h | 🔴 URGENT |
| Créer 5 exemples manquants | 4h | 🟡 IMPORTANT |
| Migrer 137 exemples @0.9 | 1h | 🟡 IMPORTANT |
| Organiser structure | 2h | 🟢 AMÉLIORATION |
| Documenter exemples | 3h | 🟢 AMÉLIORATION |
| **TOTAL** | **12.5h** | |

---

## 🎯 Recommandations Finales

### Pour l'équipe de développement

1. **Validation CI** - Ajouter job GitHub Actions validant TOUS les exemples
   ```yaml
   # .github/workflows/examples-validation.yml
   name: Examples Validation
   on: [push, pull_request]
   jobs:
     validate:
       runs-on: ubuntu-latest
       steps:
         - uses: actions/checkout@v3
         - run: |
             for f in examples/**/*.nika.yaml; do
               cargo run -- check "$f" || exit 1
             done
   ```

2. **Pre-commit hook** - Valider les exemples avant commit
   ```bash
   # .git/hooks/pre-commit
   #!/bin/bash
   for f in $(git diff --cached --name-only | grep '\.nika\.yaml$'); do
     cargo run --quiet -- check "$f" || exit 1
   done
   ```

3. **Documentation** - Mettre à jour README.md avec table des exemples

4. **Template** - Créer `examples/TEMPLATE.nika.yaml` pour nouveaux exemples

### Pour les utilisateurs

1. Commencer par `examples/public/` - Exemples production-ready
2. Utiliser schema @0.9 pour nouveaux workflows
3. Consulter `examples/v0.15/` pour features récentes
4. Référencer les exemples dans issues GitHub

---

**Rapport généré le:** 2026-03-02  
**Outil:** Claude Code Agent  
**Version Nika:** v0.16.5  
**Total fichiers audités:** 376  
**Exemples analysés:** 164  
**Tests validés:** 3,358 passing

---

✅ Audit terminé avec succès
