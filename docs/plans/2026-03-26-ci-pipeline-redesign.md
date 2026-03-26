# CI Pipeline Redesign — Plan detaille

**Date**: 2026-03-26
**Status**: Ready to execute
**Estimated**: 1 session (~2-3h)

## Objectif

Refondre le CI pour avoir 7 workflows propres au lieu de 12, zero duplication,
un template de release beau et complet, et des notifications Telegram detaillees.

---

## Phase 1 — Supprimer les workflows redondants

### Fichiers a supprimer

1. `.github/workflows/armada-checkpoints.yml` — duplique ci.yml
2. `.github/workflows/version-lock.yml` — merge dans ci.yml
3. `.github/workflows/comprehensive-tests.yml` — merge dans ci.yml
4. `.github/workflows/copilot-setup-steps.yml` — inutile

### Fichiers a garder tels quels

- `sast.yml` — cron weekly, pas de duplication
- `stale.yml` — cron daily, pas de duplication
- `lsp.yml` — path-specific, pas de duplication
- `pr-lint.yml` — 2 lignes, specifique aux PR titles
- `validate-workflows.yml` — path-specific nika yaml
- `release-plz.yml` — version bump + changelog auto

---

## Phase 2 — Nouveau ci.yml from scratch

### Trigger
```yaml
on:
  push:
    branches: [main]
  pull_request:
    branches: [main]
  workflow_dispatch:
```

### Jobs (8 jobs, parallelises au max)

```
check          → format + clippy + docs (fast, ~2 min)
test           → nextest --workspace --lib (needs: check, ~5 min)
test-features  → --no-default-features + --all-features (needs: check)
coverage       → cargo-llvm-cov + codecov upload (needs: check)
security       → cargo-audit + cargo-deny (needs: check)
semver         → cargo-semver-checks (needs: check)
validate       → nika check sur examples + schema tests (needs: check)
summary        → PR comment avec resultats (needs: all, if: PR)
```

### Checks dans chaque job

**check:**
- `cargo fmt --all --check`
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- `cargo doc --workspace --no-deps` (RUSTDOCFLAGS=-Dwarnings)
- Version 0.x.x enforcement (1 ligne)

**test:**
- `cargo nextest run --workspace --lib`
- `cargo test --doc` (doc examples)
- Count tests et output

**test-features:**
- `cargo check --workspace --no-default-features` (core compile sans features)
- `cargo check --workspace --all-features` (toutes features marchent ensemble)

**coverage:**
- `cargo llvm-cov nextest --workspace --lib --lcov --output-path lcov.info`
- Upload codecov
- Seuil 70%

**security:**
- `cargo audit`
- `cargo deny check` (licenses + bans + sources)
- `cargo machete` (deps inutilisees)

**semver:**
- `cargo-semver-checks-action@v2`
- continue-on-error: true (warning, pas bloquant)

**validate:**
- Build nika en release
- `nika check` sur les examples/*.nika.yaml
- Schema version tests

**summary (PR only):**
- github-script comment avec table de resultats
- Version dynamique depuis Cargo.toml
- Liste des crates du workspace

---

## Phase 3 — Template de release ameliore

### Design (PAS de header ASCII, PAS de double titre)

```markdown
## 🦋 Nika v0.48.0

> Semantic YAML Workflow Engine for AI — Schema `nika/workflow@0.12`

### What's Changed

(contenu auto-extrait du CHANGELOG.md)
Si CHANGELOG vide, auto-genere depuis git log avec format:
- **feat(scope):** description
- **fix(scope):** description

### 📦 Install

| | Method | Command |
|---|--------|---------|
| 🚀 | **Quick** | `curl -fsSL https://nika.sh/install.sh \| sh` |
| 🍺 | **Homebrew** | `brew install supernovae-st/tap/nika` |
| 📦 | **npm** | `npx @supernovae/nika` |
| 🦀 | **Cargo** | `cargo install nika` |
| 🐳 | **Docker** | `docker run --rm ghcr.io/supernovae-st/nika:0.48.0` |
| 💻 | **VS Code** | `ext install supernovae.nika-lang` |
| 🪟 | **Scoop** | `scoop bucket add nika https://github.com/supernovae-st/scoop-nika; scoop install nika` |
| 🐧 | **AUR** | `yay -S nika-bin` |

### 📥 Downloads

| Platform | Architecture | File | Size |
|----------|-------------|------|------|
| 🍎 macOS | Apple Silicon | `nika-macos-arm64-0.48.0.tar.gz` | (auto) |
| 🍎 macOS | Intel | `nika-macos-x64-0.48.0.tar.gz` | (auto) |
| 🐧 Linux | x64 | `nika-linux-x64-0.48.0.tar.gz` | (auto) |
| 🐧 Linux | ARM64 | `nika-linux-arm64-0.48.0.tar.gz` | (auto) |
| 🪟 Windows | x64 | `nika-windows-x64-0.48.0.zip` | (auto) |

> 🔐 All binaries include SHA256 checksums, SLSA provenance, and macOS notarization.

### 🌐 Available On

| Platform | Link |
|----------|------|
| GitHub | [Releases](https://github.com/supernovae-st/nika/releases/tag/v0.48.0) |
| VS Code | [Marketplace](https://marketplace.visualstudio.com/items?itemName=supernovae.nika-lang) |
| npm | [@supernovae/nika](https://www.npmjs.com/package/@supernovae/nika) |
| crates.io | [nika](https://crates.io/crates/nika) |
| Docker | [ghcr.io](https://github.com/supernovae-st/nika/pkgs/container/nika) + [Docker Hub](https://hub.docker.com/r/thibautmelen/nika) |
| Homebrew | `supernovae-st/tap` |
| Scoop | `supernovae-st/scoop-nika` |
| AUR | [nika-bin](https://aur.archlinux.org/packages/nika-bin) |

### 📊 Release Stats

| Metric | Value |
|--------|-------|
| Workspace crates | 12 |
| Tests | 8,200+ |
| Platforms | 7 binary targets |
| macOS | ✅ Signed + Notarized |
| SLSA | ✅ Build provenance |
| Docker | ✅ Multi-arch + SBOM |

---

Made with 💜 by [SuperNovae Studio](https://supernovae.studio) — Open Source, AGPL-3.0

**Full Changelog**: https://github.com/supernovae-st/nika/compare/vPREV...vCURR
```

### Implementation

Le template est genere dans le job `release` avec un script bash qui :
1. Extrait le CHANGELOG section pour cette version
2. Si vide, genere depuis `git log --oneline vPREV..HEAD`
3. Calcule les tailles de binaires depuis les artifacts
4. Trouve la version precedente pour le Full Changelog link
5. Injecte tout dans le template

---

## Phase 4 — Telegram multi-messages

### 4 messages envoyes sequentiellement

**Message 1 — Header**
```
🦋 Nika v0.48.0 released!

What's changed:
• feat(daemon): health check integration
• fix(tui): cache invalidation
• perf(engine): 2x faster template resolution

https://github.com/supernovae-st/nika/releases/tag/v0.48.0
```

**Message 2 — Platforms**
```
📦 Distribution Status

✅ GitHub Release (7 binaries)
✅ Docker (ghcr.io + Docker Hub)
✅ Homebrew tap
✅ VS Code Marketplace
✅ Open VSX
✅ npm (6 platform packages)
✅ crates.io (12 crates)
✅ Scoop bucket
⚠️ AUR (nika-bin)

8/9 platforms succeeded
```

**Message 3 — Stats**
```
📊 Release Stats

🦀 12 crates published
🧪 8,200+ tests passing
🔐 macOS signed + notarized
🛡️ SLSA provenance attested
🐳 Docker SBOM included
📦 6 npm platform packages
```

**Message 4 — One-liner**
```
🦋 Nika 0.48.0 — 8/9 ✅
brew install supernovae-st/tap/nika
npx @supernovae/nika
cargo install nika
```

---

## Phase 5 — Smoke test post-publish

### Nouveau job dans release.yml

```yaml
smoke-test:
  name: Post-publish Smoke Test
  needs: [npm-publish, crates-publish, vscode-publish]
  runs-on: ubuntu-latest
  steps:
    - name: Test npm install
      run: |
        npx @supernovae/nika --version || echo "::warning::npm smoke test failed"
    - name: Test cargo install
      run: |
        cargo install nika --version VERSION || echo "::warning::cargo smoke test failed"
    - name: Test VS Code extension
      run: |
        # Verify extension is on marketplace
        curl -s "https://marketplace.visualstudio.com/items?itemName=supernovae.nika-lang" | grep -q "nika-lang" || echo "::warning::vscode smoke test failed"
```

Ce job tourne APRES les publish jobs et verifie que les packages sont bien accessibles.

---

## Phase 6 — Nettoyage final

1. Supprimer `validate-workflows.yml` et merger dans ci.yml (job `validate`)
2. Verifier que `release-plz.yml` ne duplique rien avec `release.yml`
3. Mettre a jour CLAUDE.md avec les nouveaux noms de workflows
4. Mettre a jour la memoire

---

## Ordre d'execution

| Step | Action | Fichiers |
|------|--------|----------|
| 1 | Supprimer 4 workflows | delete armada, version-lock, comprehensive, copilot |
| 2 | Nouveau ci.yml | rewrite from scratch |
| 3 | Template release | update release.yml generate-notes step |
| 4 | Telegram 4 messages | update release.yml notify job |
| 5 | Smoke test | add job to release.yml |
| 6 | Nettoyage + docs | CLAUDE.md, memoire |
| 7 | Code review | lancer review agent |
| 8 | Commit + push | 1 commit par phase |

---

## Prompt pour l'agent

```
Tu dois refondre le CI/CD pipeline du projet Nika (Rust workspace, 12 crates).

CONTEXTE:
- Repo: /Users/thibaut/dev/supernovae/nika/
- Working directory: tools/nika
- Workspace Cargo.toml: tools/Cargo.toml
- 12 crates dans tools/nika-*/ et tools/nika/
- Test command: cargo test --workspace --lib (PAS cargo test sans --lib, ca trigger keychain)
- 18 secrets GitHub configures (voir docs/plans/2026-03-26-ci-pipeline-redesign.md)
- release.yml a deja 11 jobs qui publient sur 9 plateformes

TACHES:
1. SUPPRIMER ces fichiers:
   - .github/workflows/armada-checkpoints.yml
   - .github/workflows/version-lock.yml
   - .github/workflows/comprehensive-tests.yml
   - .github/workflows/copilot-setup-steps.yml

2. RECRIRE .github/workflows/ci.yml from scratch avec 8 jobs:
   check, test, test-features, coverage, security, semver, validate, summary
   Voir le plan detaille dans docs/plans/2026-03-26-ci-pipeline-redesign.md

3. AMELIORER le template de release notes dans release.yml:
   - PAS de header ASCII (ca casse le rendu)
   - Commencer par "## 🦋 Nika vX.Y.Z"
   - Sections: What's Changed, Install (table avec emojis), Downloads (avec tailles),
     Available On (liens), Release Stats
   - Ton friendly et personnel
   - Extraire depuis CHANGELOG.md, fallback sur git log

4. AMELIORER le job notify (Telegram) dans release.yml:
   - 4 messages sequentiels: header+changelog, platforms, stats, one-liner
   - Format HTML
   - Utiliser appleboy/telegram-action@v2

5. AJOUTER un job smoke-test dans release.yml:
   - Apres npm-publish et crates-publish
   - Tester npx @supernovae/nika --version
   - Tester curl marketplace VS Code

REGLES:
- YAML valide (tester avec python3 -c "import yaml; yaml.safe_load(open(f))")
- Pas de working-directory global qui casse les paths
- Secrets: ne JAMAIS hardcoder, toujours ${{ secrets.* }}
- Tests: toujours --lib pour eviter les keychain popups macOS
- Version: toujours extraire de tools/Cargo.toml (workspace)
- Conventional commits dans les messages de commit
- Co-authors: Claude + Nika 🦋

VERIFICATIONS:
- Apres chaque fichier modifie, valider le YAML
- Verifier le DAG des jobs (needs:)
- Verifier que tous les secrets references existent
- Grep pour "SuperNovae-studio" (doit etre 0 occurrence)
```

---

## Verification finale

Apres execution, verifier:
- [ ] 7 workflows (pas 12)
- [ ] ci.yml: 8 jobs, < 10 min sur main
- [ ] release.yml: 11+ jobs, preflight → build → publish → smoke → notify
- [ ] Template release: pas de ASCII header, emojis, sections detaillees
- [ ] Telegram: 4 messages
- [ ] Zero duplication entre workflows
- [ ] YAML valide pour tous les fichiers
- [ ] Zero "SuperNovae-studio"
- [ ] cargo test --lib partout (pas cargo test nu)
