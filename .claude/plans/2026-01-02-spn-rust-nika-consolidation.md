# Plan: Consolidation SPN-Rust + Nika avec Nouveaux Outils

**Date:** 2026-01-02
**Architecture:** Layered (SPN-Rust baseline → Nika extends)
**Rigueur:** Strict (80% coverage, blocking)

---

## Vue d'ensemble

```
┌─────────────────────────────────────────────────────────────────┐
│                        ARCHITECTURE                              │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  SPN-RUST (Plugin Générique)                                     │
│  ├── rust-baseline-checks.sh    ← Checks de base réutilisables  │
│  ├── rust-tools-detect.sh       ← Détection outils installés    │
│  ├── detect-rust-project.sh     ← SessionStart (existant)       │
│  └── Configuration par défaut                                    │
│           │                                                      │
│           ▼ hérite                                               │
│  NIKA (Projet Spécifique)                                        │
│  ├── .nika-rust.toml            ← Config projet                 │
│  ├── nika-pre-commit.sh         ← Appelle baseline + extensions │
│  ├── nika-validate.sh           ← Bibliothèque enrichie         │
│  └── Checks spec-code alignment                                  │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
```

---

## Phase 1: Nettoyage Nika (Supprimer le bruit)

### 1.1 Fichiers obsolètes à supprimer
- [ ] `/nika/.claude/hooks/nika-notify.sh` - Notifications désactivées
- [ ] `/nika/.claude/hooks/session-start-check.sh` - Remplacé par nika-health-check.sh

### 1.2 Vérifier cohérence settings.json
- [ ] Confirmer que seuls les hooks actifs sont référencés
- [ ] Supprimer références aux fichiers supprimés

---

## Phase 2: Créer Configuration .nika-rust.toml

### 2.1 Format du fichier de configuration
```toml
# .nika-rust.toml - Configuration Rust pour ce projet

[checks]
# Checks BLOQUANTS (exit 1 si échec)
blocking = ["fmt", "clippy", "tests", "audit-critical"]

# Checks WARNING (continue mais affiche)
warning = ["audit-medium", "geiger", "machete", "coverage-soft"]

[coverage]
# Seuil minimum (bloquant si en dessous)
threshold = 80
# Seuil warning (entre threshold et target)
target = 90

[tools]
# Utiliser nextest si disponible
prefer_nextest = true
# Utiliser bacon pour watch mode
prefer_bacon = true

[geiger]
# Seuil d'unsafe acceptable dans le projet (pas deps)
max_unsafe_project = 10
# Warning si deps ont trop d'unsafe
warn_deps_unsafe = 50

[audit]
# Bloquer sur severité
block_severity = ["critical", "high"]
# Warning sur severité
warn_severity = ["medium", "low"]
```

---

## Phase 3: Refactorer SPN-Rust Hooks

### 3.1 Créer rust-tools-detect.sh
Script qui détecte tous les outils installés et retourne leur disponibilité.

```bash
# Sortie JSON:
{
  "nextest": true,
  "audit": true,
  "machete": true,
  "tarpaulin": true,
  "geiger": true,
  "bacon": true,
  "expand": true,
  "wizard": true,
  "update": true
}
```

### 3.2 Créer rust-baseline-checks.sh
Checks génériques réutilisables par tout projet Rust:
- cargo check
- cargo fmt --check
- cargo clippy
- cargo test (ou nextest)
- cargo audit
- cargo machete

### 3.3 Mettre à jour plugin.json
Enregistrer les nouveaux hooks dans le plugin.

---

## Phase 4: Enrichir Nika Pre-Commit

### 4.1 Nouveaux checks à ajouter
- [ ] **cargo-tarpaulin**: Coverage avec seuil configurable
- [ ] **cargo-geiger**: Count unsafe, warning si > seuil
- [ ] Lecture de `.nika-rust.toml` pour configuration

### 4.2 Structure du hook enrichi
```
nika-pre-commit.sh
├── 1. Lire .nika-rust.toml (ou defaults)
├── 2. Sourcer rust-baseline-checks.sh (SPN-Rust)
├── 3. Exécuter checks de base
├── 4. Checks spécifiques Nika:
│   ├── Coverage (tarpaulin)
│   ├── Unsafe count (geiger)
│   ├── Schema alignment
│   ├── Action count
│   └── Error codes sync
├── 5. Appliquer règles blocking/warning
└── 6. Générer rapport
```

---

## Phase 5: Enrichir nika-validate.sh

### 5.1 Nouvelles fonctions
- `rust_coverage()` - Retourne % de coverage
- `rust_unsafe_count()` - Retourne count unsafe projet
- `rust_deps_unsafe()` - Retourne count unsafe dans deps
- `load_config()` - Parse .nika-rust.toml
- `check_tool_versions()` - Vérifie versions des outils

### 5.2 Améliorer full_report()
Ajouter sections:
- Code Coverage
- Unsafe Analysis
- Tool Versions
- Configuration active

---

## Phase 6: Nouveaux Commands SPN-Rust

### 6.1 /rust-coverage
Exécute tarpaulin et affiche rapport de coverage.

### 6.2 /rust-watch
Lance bacon en mode watch avec config optimale.

### 6.3 /rust-unsafe
Exécute geiger et affiche analyse unsafe.

### 6.4 /rust-expand <item>
Exécute cargo-expand sur un item spécifique.

---

## Phase 7: Tests et Validation

### 7.1 Tests unitaires hooks
- [ ] Test rust-tools-detect.sh retourne JSON valide
- [ ] Test rust-baseline-checks.sh sur projet propre
- [ ] Test rust-baseline-checks.sh sur projet avec erreurs

### 7.2 Tests intégration Nika
- [ ] Test complet nika-pre-commit.sh
- [ ] Test lecture .nika-rust.toml
- [ ] Test fallback si pas de config
- [ ] Test tous les seuils (coverage, geiger)

### 7.3 Tests end-to-end
- [ ] Lancer session Claude dans nika/
- [ ] Vérifier SessionStart détecte Rust + config
- [ ] Tenter commit avec code non formaté → bloqué
- [ ] Tenter commit avec coverage < 80% → bloqué
- [ ] Commit propre → passe

---

## Phase 8: Documentation

### 8.1 Mettre à jour README SPN-Rust
- Documenter nouvelle architecture
- Documenter tous les outils supportés
- Exemples de configuration

### 8.2 Créer CLAUDE.md section outils
- Liste des outils et leur usage
- Configuration recommandée

---

## Ordre d'exécution (Subagents)

| # | Tâche | Subagent | Dépendances |
|---|-------|----------|-------------|
| 1 | Nettoyage fichiers obsolètes | cleanup | - |
| 2 | Créer .nika-rust.toml | config | - |
| 3 | Créer rust-tools-detect.sh | spn-tools | - |
| 4 | Créer rust-baseline-checks.sh | spn-baseline | 3 |
| 5 | Mettre à jour plugin.json | spn-plugin | 3,4 |
| 6 | Enrichir nika-pre-commit.sh | nika-hooks | 2,4 |
| 7 | Enrichir nika-validate.sh | nika-lib | 2 |
| 8 | Créer nouveaux commands | spn-commands | - |
| 9 | Tests | testing | 1-8 |
| 10 | Documentation | docs | 1-9 |

---

## Checklist finale

- [ ] Aucun fichier obsolète dans nika/.claude/
- [ ] .nika-rust.toml créé et documenté
- [ ] SPN-Rust hooks génériques fonctionnels
- [ ] Nika hérite correctement de SPN-Rust
- [ ] Tous les nouveaux outils intégrés
- [ ] Coverage ≥ 80% enforced
- [ ] Tests passent
- [ ] Documentation à jour
