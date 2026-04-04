# Instructions pour la prochaine session

## Contexte

Tu reprends le travail sur Nika v0.66 — sprint stabilisation.
La session précédente (2026-04-04) a produit 2 documents essentiels :

1. **Lis d'abord** : `docs/plans/2026-04-04-mega-stabilization-prompt.md`
   → Plan autonome 5 phases, findings de 11 agents spécialisés
2. **Référence** : `docs/plans/2026-04-04-v066-stabilization-handoff.md`
   → Synthèse des audits avec priorités P0/P1/P2

## Commande de lancement

Copie-colle ce prompt pour démarrer :

---

Lis `docs/plans/2026-04-04-mega-stabilization-prompt.md` en entier. C'est ton plan d'exécution.

Exécute les 5 phases dans l'ordre, en full autonomie, pendant plusieurs heures.
Utilise les superpowers : TDD (test-driven-development), verification-before-completion,
systematic-debugging, rust skill. Lance des agents en parallèle quand possible.

Règles :
- 1 fix = 1 commit. Commit message : type(scope): description
- `cargo test --workspace --lib --exclude nika-py` doit rester vert après chaque commit
- `cargo clippy --workspace -- -D warnings` zéro warnings
- Pousse régulièrement (git remote est HTTPS via gh auth)
- Mets à jour la memory à chaque milestone

Priorités absolues :
1. SECURITY : Fix les 4 vulns (exec blocklist 4KB, shell injection, cmd leak, read size)
2. CLEAN : Split data_tools.rs (2074 lignes → 6 fichiers), fix le bug InjectTool end_marker manquant
3. DX : Mettre à jour TOUTE la doc (nika.md dit 31 transforms → il y en a 51, dit 24 tools → il y en a 58)
4. VERIFY : Tester les 8 showcases, E2E sur 3 sites depuis dossier vierge
5. RELEASE : Bump v0.66.0, push tag, vérifier CI

Architecture P0 identifiée mais PAS dans ce sprint :
- vault.rs dans nika-core (viole zero I/O) → v0.67
- nika-engine dépend de nika-init (inversé) → v0.67
- jaq 3.x upgrade → v0.67

Quand tu as fini, écris un handoff dans docs/plans/ et mets à jour la memory.

---

## Stratégie d'autonomie

- **Phases 1-2** (security + refactor) : Séquentielles, TDD strict
- **Phase 3** (DX update) : Lance un agent Explore pour auditer chaque fichier .md en parallèle, puis applique les corrections
- **Phase 4** (verify) : Lance les tests E2E dans un dossier vierge `~/Desktop/nika-e2e-v066/`
- **Phase 5** (release) : Bump version, CHANGELOG, tag, push, vérifier CI

## Superpowers à utiliser

| Skill | Quand |
|-------|-------|
| `test-driven-development` | Security fixes (Phase 1) |
| `verification-before-completion` | Avant chaque commit |
| `systematic-debugging` | Si un test casse |
| `rust` | Tout le code Rust |
| `brainstorming` | Si décision d'archi à prendre |
| `requesting-code-review` | Fin de chaque phase |

## Agents à lancer

| Agent | Pour quoi |
|-------|-----------|
| `rust-pro` | Review code après split data_tools.rs |
| `rust-security` | Vérifier les 4 security fixes |
| `Explore` | Auditer les fichiers DX (.md) pour trouver les valeurs obsolètes |
| `code-reviewer` | Review finale avant tag v0.66.0 |

## Fichiers critiques à modifier

### Phase 1 (Security)
- `tools/nika-engine/src/runtime/security.rs` — blocklist scan full + redact cmd
- `tools/nika-engine/src/runtime/executor/exec.rs` — shell injection error
- `tools/nika-engine/src/tools/read.rs` — file size pre-check

### Phase 2 (Refactor)
- `tools/nika-engine/src/runtime/builtin/data_tools.rs` → split en 6 fichiers sous `builtin/data/`
- `tools/nika-engine/src/runtime/builtin/mod.rs` — update imports
- `tools/nika-engine/src/runtime/builtin/router.rs` — update imports + doc

### Phase 3 (DX)
- `~/.claude/rules/nika.md` — 51 transforms, 58 tools, nouveaux exemples
- `~/.claude/rules/nika-bugs-and-patterns.md` — ajouter nika:jq/tree_data/inject
- `~/dev/supernovae/nika/CLAUDE.md` — mettre à jour métriques
- `~/dev/supernovae/nika/tools/nika/CLAUDE.md` — 58 tools, error codes
- `AGENTS.md` — inventaire complet des tools

### Phase 4 (Verify)
- `examples/showcase/**/*.nika.yaml` — vérifier les 8
- E2E depuis `~/Desktop/nika-e2e-v066/`

### Phase 5 (Release)
- `tools/Cargo.toml` — version 0.66.0
- `CHANGELOG.md` — entrée v0.66.0

## Ce qu'il ne faut PAS faire

- Ne PAS upgrader jaq à 3.x (trop frais, defer v0.67)
- Ne PAS déplacer vault.rs hors de nika-core (P0 mais defer v0.67)
- Ne PAS casser la dépendance engine→init (defer v0.67)
- Ne PAS ajouter de nouvelles features
- Ne PAS toucher au TUI
- Ne PAS modifier les showcases (juste vérifier qu'ils passent)
