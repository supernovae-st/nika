# HANDOFF — v0.56.1: CI Overhaul + Build Fixes

> **Ce fichier est un prompt autonome.** Donne-le tel quel à une session Claude Code.
> L'agent doit l'exécuter de A à Z sans poser de questions.

---

## IDENTITÉ

Tu fixes tout ce qui est cassé dans la CI et le repo après la release v0.56.0. Tu ne touches PAS au code de nika-serve ni à l'engine sauf pour les bugs P0. Le gros du travail est de la plomberie CI + nettoyage repo.

## RÈGLES ABSOLUES

1. **Tests** : `cargo test --workspace --lib` — TOUJOURS `--lib`
2. **Clippy** : `cargo clippy --workspace -- -D warnings` — ZERO warnings
3. **Commits** : Conventional format. 1 fix logique = 1 commit.
```
type(scope): description

Co-Authored-By: Nika 🦋 <nika@supernovae.studio>
```
4. **Ne touche PAS** au code non concerné par le fix
5. **Lis le code AVANT** de modifier

---

## ÉTAT DES LIEUX — v0.56.0 Release CI

### Ce qui PASSE
| Job | Status |
|-----|--------|
| Check (macOS + Ubuntu) | **PASS** — fmt, clippy --all-targets, rustdoc |
| Semver Check | **PASS** |
| Feature Compatibility | **PASS** |
| CodeQL | **PASS** |
| SAST | **PASS** |
| Build macOS Intel + ARM | **PASS** |
| Build Linux x86_64 gnu + musl | **PASS** |
| Build Linux aarch64 musl | **PASS** |
| GitHub Release | **PASS** |
| Docker Image | **PASS** |
| Homebrew | **PASS** |
| AUR | **PASS** |
| crates.io | **PASS** |
| Smoke Test | **PASS** |

### Ce qui FAIL
| Job | Cause | Section |
|-----|-------|---------|
| Build Windows x64 | `nika_daemon` unix-only sans `#[cfg]` | P0-BUG-1 |
| Build aarch64-gnu (cross) | Feature `nika-daemon` n'existe plus | P0-BUG-2 |
| Security (cargo-machete) | 7 deps orphelines après keyring removal | P1-BUG-3 |
| Test (doc tests) | 18 doc examples utilisent `use nika::` au lieu de `nika_engine::` | P1-BUG-4 |
| Validate Workflows (ci.yml) | Valide seulement `tools/nika/examples/` (502), ignore `examples/` (30), `tests/` (177) | P1-BUG-5 |
| Validate Workflows (separate) | `cd tools/nika` puis `examples/*.nika.yaml` = WRONG path | P1-BUG-5 |
| Coverage | llvm-profdata corrupt (infra, ignore) | - |
| VS Code Extension | Token/config issue (pre-existing) | P2-BUG-7 |
| npm Packages | Token/version issue (pre-existing) | P2-BUG-7 |

### Désordre du repo (739 workflows éparpillés)
| Dossier | Count | Problème |
|---------|-------|----------|
| `tools/nika/examples/gates/` | 446 | Gate tests — PAS des exemples utilisateur |
| `tools/nika/examples/use-cases/` | 41 | Vrais exemples, bon endroit |
| `tools/nika/examples/dag-patterns/` | 15 | Vrais exemples, bon endroit |
| `examples/` (repo root) | 30 + 3 wrong ext | Duplique tools/nika/examples ? |
| `tests/` (repo root) | 22 e2e + 24 workflows + 92 overnight + 9 provider + 30 adversarial | Mix e2e + unit + stress |
| `docs/tests/` | 15 | Edge cases — devrait être dans tests/ |
| `workflows/` (repo root) | 4 | Production workflows NovaNet — OK |
| `course/` | 1 | Isolé, le reste est dans nika-init |
| Repo root `*.nika.yaml` | 10 | **Jamais là** — test-workflow-1 à 10 |
| `examples/hello.yaml` | 1 | **Mauvaise extension** — devrait être `.nika.yaml` |

---

# PHASE 1 — P0 BUILD FIXES (2 commits)

## P0-BUG-1 : Windows build — gate nika_daemon behind cfg(unix)

**Fichier** : `tools/nika-engine/src/secrets/fallback.rs`
**Erreur CI** : `E0433: failed to resolve: use of unresolved module or unlinked crate nika_daemon`

`nika-engine/Cargo.toml` déclare `nika-daemon` sous `[target.'cfg(unix)'.dependencies]`. Mais `fallback.rs` l'utilise sans `#[cfg(unix)]` à 3 endroits.

### Lignes à modifier

**Occurrence 1** — `load_from_daemon_or_fallback()` ligne ~94 :
```rust
// AVANT :
let nika_home = nika_daemon::daemon_dir()
    .parent()
    .map(|p| p.to_path_buf())
    .unwrap_or_else(|| dirs::home_dir().unwrap().join(".nika"));

// APRÈS :
#[cfg(unix)]
let nika_home = nika_daemon::daemon_dir()
    .parent()
    .map(|p| p.to_path_buf())
    .unwrap_or_else(|| dirs::home_dir().unwrap_or_default().join(".nika"));
#[cfg(not(unix))]
let nika_home = dirs::home_dir().unwrap_or_default().join(".nika");
```

**Occurrence 2** — `get_secret()` ligne ~135 : même pattern.

**Occurrence 3** — `has_secret()` ligne ~173 : même pattern.

### Commit
```
fix(engine): gate nika_daemon calls behind cfg(unix) in fallback.rs

nika-daemon is a cfg(unix) dependency — calls to daemon_dir() must be
gated. On non-unix, fall back to dirs::home_dir() directly.
Fixes Windows build (E0433).
```

---

## P0-BUG-2 : aarch64-gnu cross-compile — feature list outdated

**Fichier** : `.github/workflows/release.yml` ligne 195-198
**Erreur CI** : `[cross] error: Errors encountered before cross compilation`

Le step cross-compile utilise `--features tui,nika-daemon,media-core,lsp`. La feature `nika-daemon` n'a JAMAIS existé sur le binaire nika — c'était `native-keychain` qui a été supprimée en v0.56.0. Il faut aussi ajouter `serve` (nouvelle feature v0.56).

### Fix

```yaml
# AVANT (ligne 195-198) :
- name: Build (cross, full features minus keychain)
  if: matrix.cross && !matrix.docker
  working-directory: tools/nika
  run: cross build --release --target ${{ matrix.target }} --no-default-features --features tui,nika-daemon,media-core,lsp

# APRÈS :
- name: Build (cross, full features)
  if: matrix.cross && !matrix.docker
  working-directory: tools/nika
  run: cross build --release --target ${{ matrix.target }} --no-default-features --features tui,serve,media-core,lsp
```

Aussi : supprimer le commentaire "keychain" ligne 139 :
```yaml
# AVANT :
# === Docker targets (musl static, no keychain) ===
# APRÈS :
# === Docker targets (musl static) ===
```

Et sur la ligne 184, le commentaire mentionne `nika-daemon` :
```yaml
# AVANT :
# (like nika-daemon) that are not in the nika binary's dependency graph.
# APRÈS :
# Members not in the binary's dependency graph are excluded automatically.
```

### Commit
```
fix(ci): update release features after keyring removal

- Replace non-existent nika-daemon feature with serve
- Remove stale "keychain" references from release.yml
```

---

# PHASE 2 — P1 CI FIXES (3 commits)

## P1-BUG-3 : cargo-machete — 7 deps orphelines

**CI job** : Security → `cargo machete (unused deps)`
**Erreur** :
```
nika-engine:  orion, whoami, zeroize
nika-daemon:  rusqlite
nika-storage: serde_json, uuid
nika-serve:   chrono
```

### Analyse

- `nika-engine` : `orion`, `whoami`, `zeroize` étaient utilisés par NikaKeyring (supprimé en v0.56). Ils sont encore utilisés par le module `vault` mais **vault est dans nika-core**, pas nika-engine. → **Supprimer des deps de nika-engine/Cargo.toml**.
  - ATTENTION : vérifie d'abord avec `grep -rn "orion\|whoami\|zeroize" tools/nika-engine/src/` que c'est bien inutilisé. Le vault re-export dans `secrets/vault.rs` fait juste `pub use nika_core::vault::*` — pas d'usage direct.
- `nika-daemon` : `rusqlite` — le daemon faisait `pub use nika_storage::*` après l'extraction. rusqlite est maintenant dans nika-storage, pas daemon. → **Supprimer de nika-daemon/Cargo.toml**.
- `nika-storage` : `serde_json` et `uuid` — machete ne détecte pas les usages via traits/macros. Vérifie avec grep. Si vraiment utilisé → ajouter `[package.metadata.cargo-machete] ignored = [...]`. Si pas utilisé → supprimer.
- `nika-serve` : `chrono` — même chose. Vérifie si utilisé dans le code serve.

### Méthode

1. `grep -rn "orion\|whoami\|zeroize" tools/nika-engine/src/ --include="*.rs"` → si 0 résultat, supprimer
2. `grep -rn "rusqlite" tools/nika-daemon/src/ --include="*.rs"` → si 0 résultat (juste re-export), supprimer
3. `grep -rn "serde_json\|uuid" tools/nika-storage/src/ --include="*.rs"` → si utilisé, ignore list
4. `grep -rn "chrono" tools/nika-serve/src/ --include="*.rs"` → si utilisé, ignore list
5. `cargo machete` (dans tools/) doit retourner 0

### Commit
```
fix(deps): remove orphaned dependencies found by cargo-machete
```

---

## P1-BUG-4 : 18 doc tests — `use nika::` au lieu de `nika_engine::`

**CI job** : Test → `cargo test --workspace --doc`
**Erreur** : `E0433: failed to resolve: use of unresolved module or unlinked crate nika`

Le binaire s'appelle `nika` mais le lib crate est `nika_engine`. Les doc examples font `use nika::registry::resolver` etc. Ce n'est pas résolvable en doc test context car `nika` n'est pas un lib crate.

### 18 fichiers concernés

```
nika-engine/src/ast/skill_def.rs         — 2 tests (lines 53, 138)
nika-engine/src/ast/pkg_resolver.rs      — 1 test (line 20)
nika-engine/src/binding/mention.rs       — 5 tests (lines 87, 139, 158, 230, 306)
nika-engine/src/registry/lockfile.rs     — 2 tests (lines 75, 107)
nika-engine/src/registry/operations.rs   — 4 tests (lines 57, 114, 143, 185)
nika-engine/src/registry/resolver.rs     — 3 tests (lines 107, 189, 218)
nika-engine/src/tools/mod.rs             — 1 test (line 29)
```

### Fix

Pour chaque doc example, SOIT :
- **Option A** (préféré) : Remplacer `use nika::` par `use nika_engine::` (le vrai nom du crate)
- **Option B** : Ajouter `# ` devant les lignes `use` (les masque dans la doc mais les garde dans le test)
- **Option C** : Marquer le block comme ```` ```no_run ```` ou ```` ```ignore ```` si le test a besoin d'I/O filesystem

Pour les tests qui font du filesystem (`lockfile.rs`, `operations.rs`, `resolver.rs`) → Option C (`ignore`) est acceptable.
Pour les tests purement logiques (`mention.rs`, `skill_def.rs`) → Option A.

### Commit
```
fix(engine): fix 18 broken doc test examples (use nika_engine:: not nika::)
```

---

## P1-BUG-5 : Validate Workflows — paths cassés + couverture incomplète

### Problème 1 : `validate-workflows.yml` (standalone workflow)

Ligne 53-59 : fait `cd tools/nika` puis `for f in examples/*.nika.yaml`. Mais les exemples utilisateurs sont dans `examples/` à la RACINE du repo (30 fichiers), PAS dans `tools/nika/examples/`.

Les 502 fichiers dans `tools/nika/examples/gates/` sont des gate tests internes, pas des exemples utilisateur.

### Problème 2 : `ci.yml` validate job

Ligne 306-313 : fait `find nika/examples -name "*.nika.yaml"` (depuis `tools/`). Ça valide les 502 gate tests mais IGNORE :
- `examples/` (30 exemples racine)
- `tests/` (177 test workflows)
- `docs/tests/` (15 edge cases)

### Fix

**`validate-workflows.yml`** — changer le step Validate (ligne 53-59) :
```yaml
- name: Validate workflows
  run: |
    cd tools/nika
    FAILED=0
    # Validate user-facing examples (repo root)
    for f in ../../examples/*.nika.yaml; do
      echo "Validating $f..."
      cargo run --release -- check "$f" || FAILED=1
    done
    [ "$FAILED" -eq 0 ] || exit 1
```

**`ci.yml`** — étendre le validate job (ligne 306-313) pour couvrir AUSSI les tests :
```yaml
- name: Validate examples
  working-directory: tools
  run: |
    FAILED=0
    # 1. Gate tests (tools/nika/examples/) — 502 files
    while IFS= read -r -d '' f; do
      ./target/release/nika check "$f" || FAILED=1
    done < <(find nika/examples -name "*.nika.yaml" -print0)
    # 2. User examples (repo root) — 30 files
    while IFS= read -r -d '' f; do
      ./target/release/nika check "$f" || FAILED=1
    done < <(find ../examples -name "*.nika.yaml" -print0)
    # 3. Test workflows — 177 files (ignore mock-only e2e that need API keys)
    while IFS= read -r -d '' f; do
      ./target/release/nika check "$f" 2>/dev/null || true
    done < <(find ../tests -name "*.nika.yaml" -print0)
    [ "$FAILED" -eq 0 ] || exit 1
```

### Commit
```
fix(ci): validate examples from correct paths + extend coverage
```

---

# PHASE 3 — REPO CLEANUP (2 commits)

## CLEANUP-1 : Supprimer les workflows orphelins à la racine

10 fichiers `test-workflow-*.nika.yaml` sont à la racine du repo. Ils ne sont référencés nulle part.

```bash
# Vérifier qu'ils ne sont pas référencés :
grep -rn "test-workflow-" . --include="*.rs" --include="*.yml" --include="*.md" | grep -v target/
```

Si aucune référence → supprimer les 10 fichiers.

Aussi : `examples/hello.yaml` a la mauvaise extension (`.yaml` au lieu de `.nika.yaml`). Soit le renommer, soit le supprimer s'il existe déjà dans `examples/hello.nika.yaml`.

### Commit
```
chore: remove orphaned test workflows from repo root
```

---

## CLEANUP-2 : Déplacer `docs/tests/` → `tests/edge-cases/`

15 edge case workflows sont dans `docs/tests/` — ils n'ont rien à faire dans `docs/`. Ce sont des tests.

```bash
mkdir -p tests/edge-cases
mv docs/tests/edge-case-*.nika.yaml tests/edge-cases/
rmdir docs/tests 2>/dev/null || true
```

Vérifier qu'aucun fichier ne référence l'ancien path :
```bash
grep -rn "docs/tests" . --include="*.rs" --include="*.yml" --include="*.md" | grep -v target/ | grep -v CHANGELOG
```

### Commit
```
chore: move edge-case test workflows from docs/tests/ to tests/edge-cases/
```

---

# PHASE 4 — VERSION BUMP + RELEASE (1 commit)

1. Bump `tools/Cargo.toml` : `version = "0.56.0"` → `"0.56.1"`
2. `cargo test --workspace --lib` — DOIT passer
3. `cargo clippy --workspace -- -D warnings` — ZERO warnings
4. `cargo test --workspace --doc` — DOIT passer maintenant (BUG-4 fixé)
5. `cd tools && cargo machete` — DOIT retourner 0 (BUG-3 fixé)

### Commit
```
chore: bump version to 0.56.1
```

### Tag + Push
```bash
git tag -a v0.56.1 -m "v0.56.1: CI fixes + repo cleanup"
git push origin main --tags
```

---

# PHASE 5 — FIX `nika switch` SUR LA MACHINE DE THIBAUT

Le binaire local est `~/.cargo/bin/nika` à v0.51.0-dev. `nika switch` n'a jamais été initialisé. Le binaire ne se met pas à jour automatiquement.

## Le problème

```
~/.cargo/bin/nika          ← v0.51.0-dev (figé, installé via cargo install il y a longtemps)
~/.nika/bin/               ← N'EXISTE PAS
~/.nika/channel            ← N'EXISTE PAS
/opt/homebrew/bin/nika     ← N'EXISTE PAS (pas de brew install)
PATH utilise ~/.cargo/bin/nika = vieille version
```

## Fix : Setup dual channel

```bash
# 1. Build le binaire release depuis le repo
cd /Users/thibaut/dev/supernovae/nika/tools
cargo build --release -p nika

# 2. Créer la structure ~/.nika/bin/
mkdir -p ~/.nika/bin

# 3. Copier le dev binary
cp target/release/nika ~/.nika/bin/nika-dev

# 4. Créer le symlink actif
ln -sf ~/.nika/bin/nika-dev ~/.nika/bin/nika

# 5. Écrire le channel
echo "dev" > ~/.nika/channel

# 6. Vérifier que PATH a ~/.nika/bin AVANT ~/.cargo/bin
# Ajouter à ~/.zshrc si pas déjà là :
grep -q 'nika/bin' ~/.zshrc || echo 'export PATH="$HOME/.nika/bin:$PATH"' >> ~/.zshrc
source ~/.zshrc

# 7. Vérifier
which nika          # doit afficher ~/.nika/bin/nika
nika --version      # doit afficher 0.56.x (pas 0.51.0-dev)

# 8. Tester le switch
nika switch         # affiche le status (dev/release)
nika switch dev     # rebuild depuis le repo
```

## Hook git auto-rebuild (optionnel)

`nika switch --setup` installe un hook post-commit qui rebuild automatiquement `nika-dev` après chaque commit. Vérifie que ça marche :

```bash
nika switch --setup
# Fait un petit commit de test
# Vérifie que nika --version change
```

## Fix Homebrew (après la release CI v0.56.1)

Quand la CI release est terminée et que le binaire macOS est publié :

```bash
brew tap supernovae-studio/tap  # si pas déjà fait
brew install supernovae-studio/tap/nika
# Maintenant nika switch release bascule sur le brew
nika switch release
nika --version   # version release (brew)
nika switch dev
nika --version   # version dev (repo local)
```

---

# PHASE 6 — DEPLOY v0.56.1 SUR nk-vps

Après que la CI release v0.56.1 est passée et que le binaire Linux x86_64 est publié :

```bash
# Vérifier que la release existe
gh release view v0.56.1

# Installer sur le VPS
ssh root@51.15.136.200 << 'EOF'
# Télécharger le binaire Linux depuis la release GitHub
RELEASE_URL=$(curl -s https://api.github.com/repos/SuperNovae-studio/nika/releases/tags/v0.56.1 \
  | grep "browser_download_url.*linux.*x86_64" \
  | head -1 | cut -d '"' -f 4)

if [ -n "$RELEASE_URL" ]; then
  curl -fsSL "$RELEASE_URL" -o /tmp/nika
  chmod +x /tmp/nika
  /tmp/nika --version
  mv /tmp/nika ~/.nika/bin/nika
  systemctl --user restart nika-daemon
  sleep 3
  ~/.nika/bin/nika --version
  systemctl --user is-active nika-daemon
else
  echo "Release URL not found — try install script instead:"
  echo "curl -fsSL https://raw.githubusercontent.com/SuperNovae-studio/nika/main/scripts/install.sh | bash"
fi
EOF
```

## Vérification E2E sur le VPS

```bash
ssh root@51.15.136.200 << 'EOF'
echo "=== Version ==="
~/.nika/bin/nika --version

echo "=== Daemon ==="
~/.nika/bin/nika daemon status

echo "=== Vault ==="
~/.nika/bin/nika vault list 2>/dev/null || ~/.nika/bin/nika provider list

echo "=== Infer via vLLM ==="
~/.nika/bin/nika infer "Say hello in 3 words" --provider qwen --model qwen3.5-27b --no-live

echo "=== Serve health ==="
# Test nika serve en mode quick (background pour 5s)
NIKA_SERVE_TOKEN=test-token ~/.nika/bin/nika serve --port 3847 --workflows /tmp &
SERVE_PID=$!
sleep 3
curl -s http://localhost:3847/health | python3 -m json.tool 2>/dev/null || echo "serve not ready"
kill $SERVE_PID 2>/dev/null
wait $SERVE_PID 2>/dev/null
echo "=== Done ==="
EOF
```

---

# PHASE 7 — VÉRIFICATION POST-PUSH

Attends que CI passe et vérifie :

```bash
# CI jobs qui DOIVENT passer maintenant :
gh run list --limit 8
# Vérifier :
# ✓ Check (macOS + Ubuntu)     — fmt + clippy + rustdoc
# ✓ Test (macOS + Ubuntu)      — nextest --lib + doc tests
# ✓ Security                   — audit + deny + machete
# ✓ Validate Workflows         — tous les exemples validés
# ✓ Semver Check
# ✓ Feature Compatibility
# ✓ Release (7/7 builds)       — incl. Windows + aarch64-gnu
```

Si la Release CI échoue sur Windows : `gh run view <id> --log-failed | tail -20` et debug.

---

## RÉSUMÉ

| Phase | Commits | Impact |
|-------|---------|--------|
| 1. P0 Build fixes | 2 | Windows + aarch64-gnu build |
| 2. P1 CI fixes | 3 | machete, doc tests, validate paths |
| 3. Repo cleanup | 2 | Orphaned files, wrong paths |
| 4. Version bump | 1 | 0.56.1 release |
| 5. nika switch | 0 | Setup local dev environment (manual) |
| 6. Deploy nk-vps | 0 | Update VPS binary v0.51 → v0.56.1 |
| 7. CI verify | 0 | All green |
| **Total** | **8 commits** | **CI green + local dev + VPS updated** |

**NE PAS** : toucher au code de nika-serve, refactorer l'engine, ajouter des features.
