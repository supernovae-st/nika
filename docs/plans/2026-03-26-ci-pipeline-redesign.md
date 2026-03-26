# CI Pipeline Redesign — Plan detaille

**Date**: 2026-03-26
**Status**: Ready to execute
**Estimated**: 1 session (~2-3h)
**Commits**: 1 par phase (6 commits total)

## Objectif

Refondre le CI pour avoir 7 workflows propres au lieu de 12, zero duplication,
un template de release beau et complet, et des notifications Telegram detaillees.

---

## Architecture cible

```
Push to main     →  ci.yml (8 jobs, < 10 min)
Pull request     →  ci.yml (meme) + pr-lint.yml (titre)
Tag v*           →  release.yml (preflight → build → publish 9 platforms → smoke → notify)
Cron weekly      →  sast.yml (CodeQL + Semgrep + Geiger)
Cron daily       →  stale.yml
Path nika-lsp/** →  lsp.yml
Path *.nika.yaml →  validate-workflows.yml (garder separe, tres specifique)
Push main        →  release-plz.yml (version bump + changelog auto)
```

**7 workflows actifs** (+ validate-workflows.yml = 8 si on le garde).

---

## Phase 1 — NUKE les workflows redondants

### Fichiers a SUPPRIMER (git rm)

| Fichier | Raison de suppression |
|---------|----------------------|
| `.github/workflows/armada-checkpoints.yml` | Duplique ci.yml (format, lint, test, coverage, security, version = 6 jobs identiques). Les stations 7-9 (CodeRabbit, Claude, commits) sont soit des placeholders soit mergees dans ci.yml |
| `.github/workflows/version-lock.yml` | 1 seul check (0.x.x). Merge dans ci.yml job `check` en 3 lignes |
| `.github/workflows/comprehensive-tests.yml` | DAG tests, smoke, regression, benchmarks — les vrais tests sont dans ci.yml. Les benchmarks deviennent dispatch-only dans un futur workflow dedie |
| `.github/workflows/copilot-setup-steps.yml` | Inutile, ne fait que `cargo build` |

### Fichiers a GARDER (ne pas toucher)

| Fichier | Raison |
|---------|--------|
| `sast.yml` | Cron weekly, pas de duplication, role different (deep SAST) |
| `stale.yml` | Cron daily, maintenance |
| `lsp.yml` | Path-specific, tests LSP specifiques |
| `pr-lint.yml` | Tiny (15 lignes), specifique PR titles |
| `validate-workflows.yml` | Path-specific *.nika.yaml, yamllint |
| `release-plz.yml` | Version bump + changelog auto, role orthogonal |

### Nettoyage post-suppression

- `dependabot.yml` : supprimer les labels `armada` si presents
- `release-plz.toml` : supprimer les labels `armada` si presents
- `CLAUDE.md` (tools/nika/CLAUDE.md et nika/CLAUDE.md) : mettre a jour les references aux workflows
- Grep repo-wide pour "armada" et "checkpoint" : supprimer toute reference

### Verification Phase 1

```bash
# Les 4 fichiers sont supprimes
ls .github/workflows/armada-checkpoints.yml 2>&1 | grep -q "No such file"
ls .github/workflows/version-lock.yml 2>&1 | grep -q "No such file"
ls .github/workflows/comprehensive-tests.yml 2>&1 | grep -q "No such file"
ls .github/workflows/copilot-setup-steps.yml 2>&1 | grep -q "No such file"

# Les 8 restants existent
ls .github/workflows/{ci,release,release-plz,sast,stale,lsp,pr-lint,validate-workflows}.yml

# Zero reference "armada-checkpoints" ou "comprehensive-tests"
grep -r "armada-checkpoints\|comprehensive-tests\|copilot-setup-steps\|version-lock" .github/ --include="*.yml" | wc -l  # doit etre 0
```

---

## Phase 2 — Nouveau ci.yml from scratch

### SUPPRIMER l'ancien ci.yml et RECREER entierement

Ne pas editer l'ancien — le supprimer et ecrire un nouveau fichier propre.

### Structure

```yaml
name: CI
on:
  push:
    branches: [main]
  pull_request:
    branches: [main]
  workflow_dispatch:

concurrency:
  group: ci-${{ github.workflow }}-${{ github.head_ref || github.run_id }}
  cancel-in-progress: true

env:
  CARGO_TERM_COLOR: always
  RUSTFLAGS: -Dwarnings

# PAS de defaults.run.working-directory global
# Chaque job specifie son working-directory explicitement dans les steps qui en ont besoin
```

### Job 1: check (~2 min)

```
Runs on: ubuntu-latest + macos-latest (matrix)
Steps:
  - checkout
  - rust toolchain stable
  - system deps (libdbus-1-dev pkg-config sur ubuntu)
  - rust-cache
  - cargo fmt --all --check (working-directory: tools/nika)
  - cargo clippy --workspace --all-targets --all-features -- -D warnings (working-directory: tools/nika)
  - cargo doc --workspace --no-deps (RUSTDOCFLAGS=-Dwarnings) (working-directory: tools/nika)
  - Version 0.x.x check:
      VERSION=$(grep '^version = ' tools/Cargo.toml | head -1 | sed 's/version = "\(.*\)"/\1/')
      MAJOR=$(echo "$VERSION" | cut -d. -f1)
      [ "$MAJOR" = "0" ] || exit 1
```

**ATTENTION path**: `tools/Cargo.toml` est le workspace root, PAS `tools/nika/Cargo.toml`.
Le `working-directory` doit etre `tools/nika` pour les commandes cargo, mais les greps sur Cargo.toml doivent utiliser le path depuis la racine du repo.

### Job 2: test (~5 min)

```
Runs on: ubuntu-latest + macos-latest (matrix, stable + beta optional)
Needs: check
Steps:
  - checkout
  - rust toolchain
  - system deps
  - rust-cache
  - Install cargo-nextest
  - cargo nextest run --workspace --lib (working-directory: tools/nika)
  - cargo test --workspace --doc (working-directory: tools/nika)
  - Extract test count from nextest output
  - Output test count to job summary
```

**CRITIQUE**: Toujours `--lib` pour eviter les keychain popups macOS.
**CRITIQUE**: `--workspace` pour tester tous les 12 crates.

### Job 3: test-features (~3 min)

```
Runs on: ubuntu-latest
Needs: check
Steps:
  - checkout
  - rust toolchain
  - system deps
  - rust-cache
  - cargo check --workspace --no-default-features (working-directory: tools/nika)
    → Verifie que le core compile sans TUI, sans media, sans LSP
  - cargo check --workspace --all-features (working-directory: tools/nika)
    → Verifie que toutes les features sont compatibles entre elles
```

### Job 4: coverage (~5 min)

```
Runs on: ubuntu-latest
Needs: check
Steps:
  - checkout
  - rust toolchain
  - system deps
  - rust-cache
  - Install cargo-llvm-cov
  - cargo llvm-cov nextest --workspace --lib --lcov --output-path lcov.info (working-directory: tools/nika)
  - Upload to Codecov:
      uses: codecov/codecov-action@v5
      with:
        token: ${{ secrets.CODECOV_TOKEN }}
        files: tools/nika/lcov.info
  - Check threshold:
      COVERAGE=$(cargo llvm-cov nextest --workspace --lib --summary-only | grep TOTAL | awk '{print $NF}')
      Warn if < 70%
```

### Job 5: security (~2 min)

```
Runs on: ubuntu-latest
Needs: check
Steps:
  - checkout
  - rust toolchain
  - rust-cache
  - Install cargo-audit, cargo-deny
  - cargo audit (working-directory: tools/nika)
  - cargo deny check (working-directory: tools/nika) || warn
  - Install cargo-machete
  - cargo machete (working-directory: tools/nika) || warn
```

### Job 6: semver (~3 min)

```
Runs on: ubuntu-latest
Needs: check
Steps:
  - checkout
  - rust toolchain
  - system deps
  - rust-cache
  - uses: obi1kenobi/cargo-semver-checks-action@v2
    with:
      manifest-path: tools/Cargo.toml
    continue-on-error: true
```

### Job 7: validate (~3 min)

```
Runs on: ubuntu-latest
Needs: check
Steps:
  - checkout
  - rust toolchain
  - system deps
  - rust-cache
  - cargo build --release (working-directory: tools/nika)
    → Build nika binary pour pouvoir utiliser `nika check`
  - Run nika check on all examples:
      for f in examples/*.nika.yaml; do
        ./target/release/nika check "$f" || FAILED=1
      done
  - Schema version tests:
      for f in tests/schema-version-tests/v*.nika.yaml; do
        ./target/release/nika check "$f" || FAILED=1
      done
```

### Job 8: summary (PR only)

```
Runs on: ubuntu-latest
Needs: [check, test, test-features, coverage, security, semver, validate]
If: always() && github.event_name == 'pull_request'
Permissions: pull-requests: write
Steps:
  - checkout (sparse: tools/Cargo.toml)
  - Extract version from tools/Cargo.toml
  - Generate PR comment with github-script:
      Table with all job statuses
      Version, Rust version, crate count
      Link to workflow run
```

**ATTENTION**: Le summary job ne doit PAS avoir de `working-directory` global.
Il fait un checkout sparse et lit `tools/Cargo.toml` avec le path complet depuis la racine.

### Verification Phase 2

```bash
# YAML valide
python3 -c "import yaml; yaml.safe_load(open('.github/workflows/ci.yml')); print('OK')"

# 8 jobs existent
python3 -c "
import yaml
wf = yaml.safe_load(open('.github/workflows/ci.yml'))
jobs = list(wf['jobs'].keys())
print(f'Jobs: {len(jobs)} — {jobs}')
assert len(jobs) == 8, f'Expected 8 jobs, got {len(jobs)}'
assert 'check' in jobs
assert 'test' in jobs
assert 'test-features' in jobs
assert 'coverage' in jobs
assert 'security' in jobs
assert 'semver' in jobs
assert 'validate' in jobs
assert 'summary' in jobs
"

# Pas de working-directory global
! grep -q "defaults:" .github/workflows/ci.yml || echo "WARN: has defaults block — check it"

# --lib present dans toutes les commandes test
grep "cargo.*test\|nextest" .github/workflows/ci.yml | grep -v "\-\-lib\|\-\-doc" && echo "FAIL: test without --lib" || echo "OK: all tests use --lib or --doc"
```

---

## Phase 3 — Template de release ameliore

### Remplacer le step `Generate release notes` dans release.yml

Le nouveau template :

```markdown
## 🦋 Nika v{VERSION}

> Semantic YAML Workflow Engine for AI — Schema `nika/workflow@0.12`

### What's Changed

{CHANGELOG_CONTENT ou GIT_LOG_CONTENT}

### 📦 Install

| | Method | Command |
|---|--------|---------|
| 🚀 | **Quick** | `curl -fsSL https://raw.githubusercontent.com/supernovae-st/nika/main/install.sh \| sh` |
| 🍺 | **Homebrew** | `brew install supernovae-st/tap/nika` |
| 📦 | **npm** | `npx @supernovae/nika` |
| 🦀 | **Cargo** | `cargo install nika` |
| 🐳 | **Docker** | `docker run --rm ghcr.io/supernovae-st/nika:{VERSION}` |
| 💻 | **VS Code** | Search "Nika" or `ext install supernovae.nika-lang` |
| 🪟 | **Scoop** | `scoop bucket add nika https://github.com/supernovae-st/scoop-nika && scoop install nika` |
| 🐧 | **AUR** | `yay -S nika-bin` |

### 📥 Downloads

| Platform | Architecture | File | Size |
|----------|-------------|------|------|
| 🍎 macOS | Apple Silicon | `nika-macos-arm64-{VERSION}.tar.gz` | {SIZE} |
| 🍎 macOS | Intel | `nika-macos-x64-{VERSION}.tar.gz` | {SIZE} |
| 🐧 Linux | x64 | `nika-linux-x64-{VERSION}.tar.gz` | {SIZE} |
| 🐧 Linux | ARM64 | `nika-linux-arm64-{VERSION}.tar.gz` | {SIZE} |
| 🪟 Windows | x64 | `nika-windows-x64-{VERSION}.zip` | {SIZE} |

> 🔐 All binaries include SHA256 checksums, SLSA provenance, and macOS notarization.

### 🌐 Available On

| Platform | Link |
|----------|------|
| GitHub | [v{VERSION}](https://github.com/supernovae-st/nika/releases/tag/v{VERSION}) |
| VS Code | [supernovae.nika-lang](https://marketplace.visualstudio.com/items?itemName=supernovae.nika-lang) |
| Open VSX | [supernovae.nika-lang](https://open-vsx.org/extension/supernovae/nika-lang) |
| npm | [@supernovae/nika](https://www.npmjs.com/package/@supernovae/nika) |
| crates.io | [nika](https://crates.io/crates/nika) |
| Docker | [ghcr.io](https://github.com/supernovae-st/nika/pkgs/container/nika) · [Docker Hub](https://hub.docker.com/r/thibautmelen/nika) |
| Homebrew | `supernovae-st/tap/nika` |
| Scoop | [supernovae-st/scoop-nika](https://github.com/supernovae-st/scoop-nika) |
| AUR | [nika-bin](https://aur.archlinux.org/packages/nika-bin) |

### 📊 Stats

| | Metric | Value |
|---|--------|-------|
| 🦀 | Crates | {CRATE_COUNT} workspace crates |
| 🧪 | Tests | {TEST_COUNT}+ passing |
| 📦 | npm | {NPM_COUNT} platform packages |
| 🔐 | macOS | Signed + Notarized (Developer ID) |
| 🛡️ | SLSA | Build provenance attested |
| 🐳 | Docker | Multi-arch amd64/arm64 + SBOM |

---

Made with 💜 by [SuperNovae Studio](https://supernovae.studio) — Open Source, AGPL-3.0

**Full Changelog**: https://github.com/supernovae-st/nika/compare/{PREV_TAG}...v{VERSION}
```

### Implementation du template

Le script bash dans le step doit :
1. Trouver le tag precedent : `PREV_TAG=$(git describe --tags --abbrev=0 HEAD^ 2>/dev/null || echo "v0.0.0")`
2. Extraire la section CHANGELOG pour cette version
3. Si CHANGELOG vide : `git log --oneline ${PREV_TAG}..HEAD --pretty="- %s"`
4. Calculer les tailles : `ls -lh release-assets/*.tar.gz | awk '{print $5}'`
5. Compter les crates : `ls -d tools/nika-*/Cargo.toml tools/nika/Cargo.toml | wc -l`
6. Compter les tests : `grep -r "#\[test\]" tools/ --include="*.rs" | wc -l`
7. Compter les npm packages : `ls -d packages/*/package.json | wc -l`

### Verification Phase 3

```bash
# YAML valide
python3 -c "import yaml; yaml.safe_load(open('.github/workflows/release.yml')); print('OK')"

# Pas de ASCII box header
! grep -q "╔═══" .github/workflows/release.yml || echo "FAIL: ASCII header still present"

# Template contient les sections attendues
grep -q "What's Changed" .github/workflows/release.yml && echo "OK" || echo "FAIL: missing What's Changed"
grep -q "📦 Install" .github/workflows/release.yml && echo "OK" || echo "FAIL: missing Install"
grep -q "📥 Downloads" .github/workflows/release.yml && echo "OK" || echo "FAIL: missing Downloads"
grep -q "🌐 Available On" .github/workflows/release.yml && echo "OK" || echo "FAIL: missing Available On"
grep -q "📊 Stats" .github/workflows/release.yml && echo "OK" || echo "FAIL: missing Stats"
grep -q "Full Changelog" .github/workflows/release.yml && echo "OK" || echo "FAIL: missing Full Changelog"
```

---

## Phase 4 — Telegram 4 messages

### Remplacer le job `notify` dans release.yml

Le job collecte les stats et envoie 4 messages HTML sequentiels.

**IMPORTANT**: Utiliser `format: html` (pas markdown) pour eviter les problemes d'echappement.

**Message 1 — Header + Changelog** (max ~10 commits)
```html
🦋 <b>Nika {VERSION} released!</b>

<b>What's changed:</b>
• feat(daemon): health check integration
• fix(tui): cache invalidation
• perf(engine): 2x faster template resolution

<a href="https://github.com/supernovae-st/nika/releases/tag/v{VERSION}">View release</a>
```

Le changelog est extrait de `git log --oneline PREV..HEAD | head -10`.

**Message 2 — Platform status**
```html
📦 <b>Distribution — {SUCCESS}/{TOTAL} platforms</b>

✅ GitHub Release (7 binaries)
✅ Docker (ghcr.io + Docker Hub)
{STATUS} Homebrew tap
{STATUS} VS Code + Open VSX
{STATUS} npm (6 packages)
{STATUS} crates.io (12 crates)
{STATUS} Scoop bucket
{STATUS} AUR (nika-bin)
```

**Message 3 — Stats**
```html
📊 <b>Release Stats</b>

🦀 {CRATE_COUNT} crates published
🧪 {TEST_COUNT}+ tests passing
🔐 macOS signed + notarized
🛡️ SLSA provenance attested
🐳 Docker SBOM + multi-arch
📦 {NPM_COUNT} npm platform packages
```

**Message 4 — One-liner**
```html
🦋 <b>Nika {VERSION}</b> — {SUCCESS}/{TOTAL} ✅ | <code>brew install supernovae-st/tap/nika</code> | <code>npx @supernovae/nika</code> | <code>cargo install nika</code>
```

### Implementation

Chaque message est un step `appleboy/telegram-action@v2` separe. Le job fait d'abord un checkout + stats collection, puis 4 steps d'envoi.

### Verification Phase 4

```bash
# 4 steps telegram dans le notify job
grep -c "appleboy/telegram-action" .github/workflows/release.yml  # doit etre 4

# Format HTML partout
grep -A2 "appleboy/telegram-action" .github/workflows/release.yml | grep "format:" | grep -v "html" && echo "FAIL: not all HTML" || echo "OK: all HTML"
```

---

## Phase 5 — Smoke test post-publish

### Nouveau job `smoke-test` dans release.yml

```yaml
smoke-test:
  name: Post-publish Smoke Test
  needs: [npm-publish, crates-publish, vscode-publish, update-homebrew]
  if: ${{ !inputs.dry_run }}
  runs-on: ubuntu-latest
  steps:
    - name: Wait for registry propagation
      run: sleep 60

    - name: Test npm package
      run: |
        npm info @supernovae/nika version 2>/dev/null && echo "✅ npm: package visible" || echo "⚠️ npm: not yet visible"
      continue-on-error: true

    - name: Test crates.io
      run: |
        curl -s "https://crates.io/api/v1/crates/nika" | grep -q '"newest_version"' && echo "✅ crates.io: visible" || echo "⚠️ crates.io: not yet visible"
      continue-on-error: true

    - name: Test VS Code Marketplace
      run: |
        curl -s "https://marketplace.visualstudio.com/items?itemName=supernovae.nika-lang" | grep -q "nika-lang" && echo "✅ VS Code: visible" || echo "⚠️ VS Code: not yet visible"
      continue-on-error: true

    - name: Test Docker image
      run: |
        docker manifest inspect ghcr.io/supernovae-st/nika:${{ env.RELEASE_TAG }} && echo "✅ Docker: visible" || echo "⚠️ Docker: not yet visible"
      continue-on-error: true
```

**IMPORTANT**: Le `sleep 60` est necessaire car les registres npm/crates.io prennent du temps a propager.
Tous les checks sont `continue-on-error: true` car un registry peut mettre plus de 60s.

### Le notify job doit attendre le smoke-test

```yaml
notify:
  needs: [..., smoke-test]
```

### Verification Phase 5

```bash
# smoke-test job existe
python3 -c "
import yaml
wf = yaml.safe_load(open('.github/workflows/release.yml'))
assert 'smoke-test' in wf['jobs'], 'missing smoke-test job'
print('OK: smoke-test job exists')
"

# notify depends on smoke-test
python3 -c "
import yaml
wf = yaml.safe_load(open('.github/workflows/release.yml'))
needs = wf['jobs']['notify']['needs']
assert 'smoke-test' in needs, 'notify does not wait for smoke-test'
print('OK: notify waits for smoke-test')
"
```

---

## Phase 6 — Nettoyage final

### 6a. Mettre a jour les references

```bash
# Trouver toutes les references aux workflows supprimes
grep -r "armada\|checkpoint\|comprehensive-tests\|version-lock\|copilot-setup" \
  --include="*.md" --include="*.yml" --include="*.yaml" --include="*.toml" \
  . | grep -v node_modules | grep -v target | grep -v ".git/"
```

Corriger chaque reference trouvee.

### 6b. Mettre a jour CLAUDE.md

- `tools/nika/CLAUDE.md` : section Testing, section Conventions
- `nika/CLAUDE.md` : si reference aux workflows

### 6c. Mettre a jour dependabot.yml

Supprimer les labels `armada` si presents.

### 6d. Mettre a jour release-plz.toml

Supprimer les labels `armada` si presents.

### 6e. Verification finale COMPLETE

```bash
# Nombre de workflows
echo "Workflow count: $(ls .github/workflows/*.yml | wc -l)"  # doit etre 8

# Liste des workflows
ls .github/workflows/*.yml

# YAML valide pour TOUS
for f in .github/workflows/*.yml; do
  python3 -c "import yaml; yaml.safe_load(open('$f'))" && echo "OK: $f" || echo "FAIL: $f"
done

# Zero SuperNovae-studio
grep -ri "SuperNovae-studio" --include="*.yml" --include="*.yaml" --include="*.md" --include="*.json" --include="*.toml" --include="*.rs" --include="*.js" . | grep -v node_modules | grep -v target | grep -v ".git/" | wc -l  # doit etre 0

# Zero reference aux workflows supprimes
grep -ri "armada-checkpoints\|version-lock\.yml\|comprehensive-tests\|copilot-setup-steps" --include="*.yml" --include="*.md" . | grep -v node_modules | grep -v target | grep -v ".git/" | wc -l  # doit etre 0

# ci.yml: 8 jobs
python3 -c "import yaml; print(len(yaml.safe_load(open('.github/workflows/ci.yml'))['jobs']))"  # doit etre 8

# release.yml: 12+ jobs (preflight, build, docker, release, homebrew, vscode, npm, crates, scoop, aur, smoke-test, notify)
python3 -c "import yaml; print(len(yaml.safe_load(open('.github/workflows/release.yml'))['jobs']))"  # doit etre 12

# release.yml DAG complet
python3 -c "
import yaml
wf = yaml.safe_load(open('.github/workflows/release.yml'))
for name, job in wf['jobs'].items():
    needs = job.get('needs', [])
    if isinstance(needs, str): needs = [needs]
    print(f'  {name}: needs={needs}')
"

# Test command safety: --lib present
grep -n "cargo.*test\|nextest" .github/workflows/ci.yml | grep -v "\-\-lib\|\-\-doc\|\-\-no-run\|install\|cargo-"

# Release template: no ASCII header
! grep "╔═══\|╚═══" .github/workflows/release.yml

# Telegram: 4 messages
grep -c "telegram-action" .github/workflows/release.yml  # doit etre 4

# All secrets referenced exist
python3 -c "
import yaml, re
for f in ['.github/workflows/ci.yml', '.github/workflows/release.yml']:
    content = open(f).read()
    secrets = set(re.findall(r'secrets\.(\w+)', content))
    print(f'{f}: secrets={sorted(secrets)}')
"
# Cross-reference avec: gh secret list -R supernovae-st/nika
```

---

## Prompt detaille pour l'agent

```
Tu dois refondre le CI/CD pipeline du projet Nika (Rust workspace, 12 crates).
Lis ENTIEREMENT le plan dans docs/plans/2026-03-26-ci-pipeline-redesign.md avant de commencer.
Execute phase par phase. 1 commit par phase. Verifie apres chaque phase.

=== CONTEXTE ===

Repo: /Users/thibaut/dev/supernovae/nika/
Git remote: git@github.com:supernovae-st/nika.git (org = supernovae-st, PAS SuperNovae-studio)
Workspace root: tools/Cargo.toml (PAS tools/nika/Cargo.toml)
Working directory pour cargo: tools/nika (mais PAS en global defaults)
12 crates: nika-core, nika-event, nika-lsp-core, nika-mcp, nika-media, nika-init, nika-engine, nika-daemon, nika-cli, nika-tui, nika-lsp, nika
Test command: cargo nextest run --workspace --lib (JAMAIS cargo test sans --lib = keychain popup macOS)
VS Code extension: editors/vscode/ (publisher: supernovae, extension: nika-lang)
npm packages: packages/npm/ (main) + packages/nika-{darwin-arm64,darwin-x64,linux-x64,linux-arm64,win32-x64}/

18 secrets GitHub configures:
  VSCE_PAT, OVSX_PAT, NPM_TOKEN, CARGO_REGISTRY_TOKEN, HOMEBREW_TAP_TOKEN,
  CODECOV_TOKEN, SEMGREP_APP_TOKEN, APPLE_CERTIFICATE_P12, APPLE_CERTIFICATE_PASSWORD,
  APPLE_ID, APPLE_APP_PASSWORD, APPLE_TEAM_ID, SIGNPATH_API_TOKEN,
  AUR_SSH_PRIVATE_KEY, DOCKERHUB_USERNAME, DOCKERHUB_TOKEN,
  TELEGRAM_BOT_TOKEN, TELEGRAM_CHAT_ID

=== PHASE 1: SUPPRIMER (commit: "ci: remove redundant workflows") ===

git rm ces fichiers:
  .github/workflows/armada-checkpoints.yml
  .github/workflows/version-lock.yml
  .github/workflows/comprehensive-tests.yml
  .github/workflows/copilot-setup-steps.yml

Puis grep et fix TOUTES les references a ces fichiers dans le repo:
  - dependabot.yml: supprimer labels "armada"
  - release-plz.toml: supprimer labels "armada"
  - Tout .md qui reference armada/checkpoints
  - CLAUDE.md files

VERIFICATION:
  - 8 fichiers .yml restent dans .github/workflows/
  - Zero reference aux fichiers supprimes

=== PHASE 2: NOUVEAU ci.yml (commit: "ci: rewrite CI pipeline from scratch") ===

SUPPRIMER l'ancien ci.yml. CREER un nouveau avec 8 jobs:
  check, test, test-features, coverage, security, semver, validate, summary

Voir le plan detaille pour chaque job. Points critiques:
  - PAS de defaults.run.working-directory global (ca casse les paths)
  - Chaque step cargo utilise working-directory: tools/nika
  - Les greps sur Cargo.toml utilisent tools/Cargo.toml (path absolu depuis repo root)
  - cargo fmt, clippy, test: working-directory: tools/nika
  - Version check: grep tools/Cargo.toml (PAS tools/nika/Cargo.toml qui a version.workspace = true)
  - Tests: TOUJOURS --lib (JAMAIS cargo test nu)
  - nextest pour les tests unitaires, cargo test --doc pour les doc tests
  - Coverage: cargo-llvm-cov nextest
  - Concurrency group pour cancel les runs obsoletes
  - Matrix ubuntu + macos pour check et test
  - system deps: sudo apt-get install -y libdbus-1-dev pkg-config (ubuntu seulement)

VERIFICATION:
  - python3 YAML valid
  - 8 jobs
  - zero working-directory global
  - --lib present dans toutes les commandes test
  - tools/Cargo.toml pour la version (pas tools/nika/Cargo.toml)

=== PHASE 3: TEMPLATE RELEASE (commit: "ci(release): new release notes template") ===

Dans release.yml, remplacer le step "Generate release notes" par un nouveau qui:
  1. NE GENERE PAS de header ASCII (pas de ╔═══)
  2. Commence par "## 🦋 Nika v{VERSION}"
  3. Ajoute une quote block: "> Semantic YAML Workflow Engine for AI — Schema nika/workflow@0.12"
  4. Section "### What's Changed" auto-generee:
     - Extrait du CHANGELOG.md si section existe
     - Sinon: git log --oneline PREV_TAG..HEAD (max 20 lignes)
  5. Section "### 📦 Install" — table avec emojis (8 methodes)
  6. Section "### 📥 Downloads" — table avec tailles auto-calculees depuis artifacts
  7. Section "### 🌐 Available On" — table avec liens directs vers toutes les plateformes
  8. Section "### 📊 Stats" — crates count, test count, npm count, security badges
  9. Footer: Made with 💜 + Full Changelog link (PREV_TAG...vVERSION)

Calculs dans le script:
  PREV_TAG=$(git describe --tags --abbrev=0 HEAD^ 2>/dev/null || echo "v0.0.0")
  CRATE_COUNT=$(ls -d tools/nika-*/Cargo.toml tools/nika/Cargo.toml 2>/dev/null | wc -l)
  TEST_COUNT=$(grep -r "#\[test\]" tools/ --include="*.rs" 2>/dev/null | wc -l)
  NPM_COUNT=$(ls -d packages/*/package.json 2>/dev/null | wc -l)
  Binary sizes: ls -lh release-assets/*.tar.gz + awk

VERIFICATION:
  - YAML valide
  - Pas de ╔═══ dans le fichier
  - Sections presentes: What's Changed, Install, Downloads, Available On, Stats, Full Changelog

=== PHASE 4: TELEGRAM 4 MESSAGES (commit: "ci(release): rich Telegram notifications") ===

Dans release.yml, remplacer le job notify par un nouveau qui:
  1. Checkout + collecte stats (version, crate count, test count, git log)
  2. Collecte les resultats de TOUS les jobs (needs.*.result)
  3. Envoie 4 messages sequentiels via appleboy/telegram-action@v2:
     - Message 1: 🦋 Header + changelog (git log --oneline, max 10)
     - Message 2: 📦 Platform status (✅/❌/⚠️ pour chaque plateforme)
     - Message 3: 📊 Stats (crates, tests, security)
     - Message 4: One-liner condense avec install commands

  Tous en format: html (PAS markdown, evite les problemes d'echappement)
  Tous avec disable_web_page_preview: true
  Si secrets.TELEGRAM_BOT_TOKEN est vide, skip silencieusement (if: secrets.TELEGRAM_BOT_TOKEN != '')

Le notify job doit "needs:" TOUS les autres jobs + smoke-test.
Le notify doit avoir if: always() pour envoyer meme si un job a echoue.

VERIFICATION:
  - 4 occurrences de telegram-action dans release.yml
  - Toutes en format html
  - notify needs inclut smoke-test

=== PHASE 5: SMOKE TEST (commit: "ci(release): post-publish smoke tests") ===

Ajouter un job smoke-test dans release.yml:
  needs: [npm-publish, crates-publish, vscode-publish]
  if: !inputs.dry_run
  Steps:
    - sleep 60 (attendre propagation des registres)
    - npm info @supernovae/nika version (continue-on-error)
    - curl crates.io API (continue-on-error)
    - curl VS Code Marketplace (continue-on-error)
    - docker manifest inspect ghcr.io/supernovae-st/nika:TAG (continue-on-error)

Le notify job doit dependre de smoke-test aussi.

VERIFICATION:
  - smoke-test job existe
  - notify depends on smoke-test
  - Tous les steps sont continue-on-error: true

=== PHASE 6: NETTOYAGE (commit: "ci: cleanup references and docs") ===

1. Grep TOUT le repo pour "armada", "checkpoint", "comprehensive-tests", "version-lock", "copilot-setup"
   Corriger ou supprimer chaque reference trouvee.

2. Mettre a jour:
   - tools/nika/CLAUDE.md: section Testing, Conventions
   - nika/CLAUDE.md: si references
   - dependabot.yml: labels
   - release-plz.toml: labels

3. Verification E2E finale:
   - 8 workflows .yml
   - ci.yml: 8 jobs
   - release.yml: 12 jobs (preflight, build, docker, release, homebrew, vscode, npm, crates, scoop, aur, smoke-test, notify)
   - YAML valide pour TOUS les fichiers
   - Zero SuperNovae-studio (case insensitive)
   - Zero reference aux workflows supprimes
   - --lib dans toutes les commandes test
   - tools/Cargo.toml pour la version
   - 4 telegram-action dans release.yml
   - Pas de ╔═══ dans release.yml
   - DAG complet affiché

=== REGLES STRICTES ===

- YAML: valider apres CHAQUE modification avec python3 yaml.safe_load
- Paths: tools/Cargo.toml pour workspace version, tools/nika/ pour cargo commands
- Tests: TOUJOURS --lib, JAMAIS cargo test nu (keychain macOS)
- Secrets: JAMAIS hardcoder, toujours ${{ secrets.* }}
- URLs: supernovae-st (PAS SuperNovae-studio)
- Commits: conventional format, co-authors Claude + Nika 🦋
- 1 COMMIT PAR PHASE (6 commits total)
- git push apres chaque commit

=== TDD APPROACH ===

Pour chaque phase:
1. ECRIRE les verifications d'abord (les commandes de test)
2. Implementer le changement
3. Executer les verifications
4. Si echec → corriger et re-verifier
5. Commit seulement quand TOUTES les verifications passent
```

---

## Checklist de verification finale

- [ ] 8 fichiers dans .github/workflows/
- [ ] ci.yml: 8 jobs (check, test, test-features, coverage, security, semver, validate, summary)
- [ ] release.yml: 12 jobs (preflight, build, docker, release, homebrew, vscode, npm, crates, scoop, aur, smoke-test, notify)
- [ ] Template release: pas ASCII, emojis, 6 sections, Full Changelog link
- [ ] Telegram: 4 messages HTML
- [ ] Smoke test: npm, crates, vscode, docker
- [ ] YAML valide pour TOUS les fichiers
- [ ] Zero SuperNovae-studio
- [ ] Zero reference armada/checkpoints/comprehensive/copilot
- [ ] cargo test --lib partout
- [ ] tools/Cargo.toml pour version extraction
- [ ] Pas de defaults.run.working-directory global dans ci.yml
- [ ] 6 commits, 1 par phase, tous pushes
- [ ] DAG release.yml: preflight → build → docker → release → {7 publish} → smoke → notify
