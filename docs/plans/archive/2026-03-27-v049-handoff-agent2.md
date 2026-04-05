# v0.49 Fixes — Agent 2 Complete Instruction Prompt

> **Copie-colle ce fichier entier comme prompt dans une nouvelle session Claude Code.**

---

## Instruction Prompt

```
ultrathink  Execute le reste du plan docs/plans/2026-03-27-v049-fixes-handoff.md.

# Contexte projet
- Nika = semantic YAML workflow engine for AI tasks (Rust, 10 crates workspace)
- Workspace root: tools/ (Cargo.toml workspace), binary: tools/nika/
- Version: v0.49.0 en cours
- Toutes les conventions dans: tools/nika/CLAUDE.md

# Ce qui est DEJA FAIT (4 commits pushes sur main)
- Phase 1.1 ✅ 4a492b15b — 10 tests model metadata dans cost.rs
- Phase 1.2 ✅ 6f08e34f9 — 6 tests dans model_cloud.rs
- Phase 2   ✅ 098c2d594 — Security: Shutdown gated auth, GetSecret doc, Debug redact
- Phase 5   ✅ 22a97db9f — indicatif ProgressBar model pull + ModelMeta dans model info

# CE QUI RESTE A FAIRE (5 phases)

## Phase 3 — Onboarding auto-trigger (EN COURS, 80% fait)

### Etat actuel
tools/nika/src/main.rs a des CHANGEMENTS NON COMMITES:
- `nika setup` ajoute a AFTER_HELP (line ~54) ✅
- `handle_result()` converti en `async fn` (line ~1332) ✅
- MissingApiKey interception ajoute dans handle_result ✅
- Call site 1 (line ~881): `handle_result(result).await;` ✅
- Call site 2 (line ~985): `return handle_result(Err(e)).await,` ✅
- Call site 3 (line ~1250): ENCORE `handle_result(result);` ← MANQUE .await

### Actions
1. Lis tools/nika/src/main.rs line 1245-1255
2. Change `handle_result(result);` → `handle_result(result).await;` (line ~1250)
3. `cd tools && cargo fmt -p nika && cargo test -p nika --lib`
4. `cargo clippy -p nika -- -D warnings`
5. Commit: `feat(cli): auto-trigger onboarding wizard on MissingApiKey + add nika setup to help`

## Phase 7.1 — Fix jobs exit code bug

### Le bug
tools/nika-cli/src/jobs.rs line 74-79:
```rust
if !client.socket_exists() {
    eprintln!("✗ daemon not running — start with: nika daemon start");
    return Ok(());  // ← BUG: exit code 0 masque l'erreur
}
```

### Fix
```rust
if !client.socket_exists() {
    return Err(NikaError::ConfigError {
        reason: "daemon not running — start with: nika daemon start".into(),
    });
}
```

### Actions
1. Lis tools/nika-cli/src/jobs.rs lines 70-85
2. Applique le fix
3. Verifie l'import NikaError est present (sinon ajoute `use nika_engine::error::NikaError;`)
4. `cd tools && cargo fmt -p nika-cli && cargo test -p nika-cli --lib -- jobs`
5. Commit: `fix(cli): jobs returns Err when daemon not running (was Ok masking failure)`

## Phase 4 — Daemon IPC pour provider set/delete

### Contexte
- tools/nika/src/cli/provider.rs a `ProviderAction::Set` (line ~183) et `ProviderAction::Delete` (line ~318)
- nika-daemon dep est deja dans nika-cli/Cargo.toml mais PAS dans tools/nika/Cargo.toml
- Le client: `nika_daemon::client::DaemonClient` avec `.set_secret()` et `.delete_secret()`
- Le socket path: `nika_daemon::daemon_socket_path()`

### Actions
1. Ajoute a tools/nika/Cargo.toml sous `[target.'cfg(unix)'.dependencies]`:
   ```toml
   nika-daemon = { workspace = true }
   ```
   (Cree la section si elle n'existe pas)

2. Dans tools/nika/src/cli/provider.rs, `ProviderAction::Set`, AVANT `NikaKeyring::set()` (line ~266):
   ```rust
   #[cfg(unix)]
   {
       let sock = nika_daemon::daemon_socket_path();
       if sock.exists() {
           let client = nika_daemon::client::DaemonClient::new(&sock);
           if client.set_secret(&provider, &api_key).await.is_ok() {
               println!("  {} stored via daemon", StatusIcon::Ok);
               // Still store in keychain as backup
           }
       }
   }
   ```

3. Meme pattern pour `ProviderAction::Delete` (line ~318), avec `client.delete_secret(&provider)`

4. `cd tools && cargo fmt -p nika && cargo test -p nika --lib`
5. Commit: `feat(cli): route provider set/delete through daemon IPC with keyring fallback`

## Phase 6 — CLI polish: cli_format adoption (6 fichiers)

### Fichier reference a lire en PREMIER
tools/nika/src/cli/provider.rs — c'est le SEUL fichier deja migre vers cli_format.
Il importe: `use nika::display::{hint, status_line, tree_connector, StatusIcon};`

### Utilities disponibles (tools/nika-engine/src/display/cli_format.rs)
- `StatusIcon::Ok/Fail/Warn/Info/Skip/Download/Hint` — icones standardisees
- `section_header("title")` → header bold + separator
- `section_header_with_subtitle("title", "count")` → header + compteur
- `key_value("label", "value")` → aligned label: value
- `key_value_width("label", "value", width)` → custom width
- `status_line(icon, "message")` → icon + message
- `status_line_with_hint(icon, "msg", "hint")` → icon + msg + dimmed hint
- `tree_connector(is_last)` → "├──" ou "└──"
- `hint("message")` → dimmed indented
- `separator(width)` → dimmed dashes
- `panel("title", width)` → box panel
- `panel_with_content("title", &lines, width)` → box panel with body

### Methode pour chaque fichier (1 COMMIT PAR FICHIER)
1. Lis le fichier entier
2. Cherche tous les `"✓".green()` `"✗".red()` `"⚠".yellow()` `"ℹ".cyan()` etc.
3. Remplace par StatusIcon::Ok/Fail/Warn/Info
4. Cherche les headers manuels (title.bold() + "─".repeat()) → section_header()
5. Ajoute l'import: `use nika_engine::display::{StatusIcon, section_header, hint, ...};`
6. Ajoute 3-5 tests unitaires
7. `cd tools && cargo fmt -p nika-cli && cargo test -p nika-cli --lib`
8. Commit separé

### Ordre et priorite
| # | Fichier | LOC | Effort | Commit scope |
|---|---------|-----|--------|-------------|
| 1 | nika-cli/src/config.rs | 284 | petit | `refactor(cli): adopt cli_format in config command` |
| 2 | nika-cli/src/trace.rs | 160 | petit | `refactor(cli): adopt cli_format in trace command` |
| 3 | nika-cli/src/daemon.rs | 343 | moyen | `refactor(cli): adopt cli_format in daemon command` |
| 4 | nika-cli/src/model.rs | 511 | moyen | `refactor(cli): adopt cli_format in model command` |
| 5 | nika-cli/src/media.rs | 966 | gros | `refactor(cli): adopt cli_format in media command` |
| 6 | nika-cli/src/doctor.rs | 1148 | gros | `refactor(cli): adopt cli_format in doctor command` |

## Phase 7.2+ — Dry-run cost estimate + spinners + provider test fallback

### 7.2 — Dry-run cost estimate
- Fichier: tools/nika/src/main.rs, function `dry_run_workflow()` (line ~2815)
- Apres le "Summary:" (line ~2944), ajoute une estimation de cout:
  ```rust
  use nika::provider::cost::{get_model_pricing, format_cost, ProviderKind};
  // Pour chaque tache infer/agent, calculer le cout estime
  // Utiliser get_model_pricing(provider, model) → pricing.calculate(estimated_input, estimated_output)
  // Afficher le total
  ```
- Estimer ~1000 tokens input, ~500 tokens output par tache LLM (heuristique raisonnable)

### 7.3 — Spinners pour commandes reseau
- `nika check --strict` — cherche le code MCP connection test, wrap dans `cliclack::spinner()`
- `nika daemon status` — dans tools/nika-cli/src/daemon.rs, wrap le ping dans spinner
- `cliclack` est deja un dep

### 7.4 — Provider test non-interactive fallback
- tools/nika/src/cli/provider.rs function `test_provider_connection()` (line ~352)
- `cliclack::spinner()` plante si pas de TTY
- Ajoute au debut:
  ```rust
  if !std::io::stderr().is_terminal() {
      println!("Testing {provider}...");
      // meme logique de test mais avec println au lieu de spinner
      return;
  }
  ```

# Regles CRITIQUES

## Testing
- TOUJOURS `cargo test --lib` (JAMAIS sans --lib → popups keychain macOS!)
- Tests workspace complet a la fin: `cargo test --workspace --lib`
- Clippy zero warnings: `cargo clippy --workspace -- -D warnings`

## Git
- 1 fix = 1 commit GRANULAIRE
- Format: `type(scope): description` (types: feat/fix/refactor/test)
- Scopes: tui, ast, runtime, mcp, provider, dag, event, cli, daemon
- TOUJOURS les 2 Co-Authored-By:
  ```
  Co-Authored-By: Nika 🦋 <nika@supernovae.studio>
  ```
- `cargo fmt -p <crate>` AVANT `git add`
- `git show --stat HEAD` APRES chaque commit (un hook peut revert)
- `git push` apres chaque phase terminee

## Code
- NikaError avec codes NIKA-XXX, pas anyhow
- Workspace root = tools/ (c'est la que cargo fmt/test/clippy marchent)
- Binary crate = tools/nika/, engine = tools/nika-engine/, cli = tools/nika-cli/

# Verification finale (apres TOUT)
```bash
cd /Users/thibaut/dev/supernovae/nika/tools
cargo test --workspace --lib        # 8300+ tests, 0 failures
cargo clippy --workspace -- -D warnings  # 0 warnings
cd /Users/thibaut/dev/supernovae/nika
git log --oneline -20               # Verifier les commits granulaires
git push
```

# Ordre d'execution recommande
Phase 3 (5 min) → Phase 7.1 (10 min) → Phase 4 (30 min) → Phase 6 (2-3h) → Phase 7.2+ (1h)

# Fichiers cles a lire
1. docs/plans/2026-03-27-v049-fixes-handoff.md — plan original (source of truth)
2. tools/nika/CLAUDE.md — conventions dev, error codes, testing
3. tools/nika-engine/src/display/cli_format.rs — les utilities a utiliser pour Phase 6
4. tools/nika/src/cli/provider.rs — REFERENCE de bonne adoption cli_format
5. tools/nika-cli/src/jobs.rs:74-79 — le bug exit code Phase 7.1
6. tools/nika/src/main.rs:1245-1255 — le .await manquant Phase 3
7. tools/nika/src/main.rs:2815-2966 — dry_run_workflow pour Phase 7.2
```

---

## Notes pour l'agent

### Erreurs a eviter
- Ne PAS lancer `cargo test` sans `--lib` (keychain popup macOS)
- Ne PAS faire `cargo fmt` depuis /Users/thibaut/dev/supernovae/nika/ — le Cargo.toml workspace est dans tools/
- Ne PAS amender des commits — toujours NEW commit
- Ne PAS battre plusieurs fixes dans un commit — 1 fix = 1 commit
- Ne PAS oublier `git show --stat HEAD` apres chaque commit (le pre-commit hook peut revert)

### Methodologie TDD
Pour chaque phase:
1. **RED**: Ecris le test d'abord, verifie qu'il fail (ou qu'il n'existe pas)
2. **GREEN**: Ecris le code minimal pour faire passer
3. **REFACTOR**: Nettoie si necessaire
4. **VERIFY**: `cargo fmt` + `cargo test --lib` + `cargo clippy`
5. **COMMIT**: Granulaire avec co-authors
6. **VERIFY COMMIT**: `git show --stat HEAD`
7. **PUSH**: `git push`

### Code review mentale avant chaque commit
- [ ] Le code compile sans warnings?
- [ ] Les tests passent?
- [ ] Les imports sont corrects?
- [ ] Pas de `unwrap()` en code library (seulement dans les tests)?
- [ ] Les erreurs utilisent NikaError, pas anyhow?
- [ ] Le commit message suit le format conventionnel?
- [ ] Les 2 Co-Authored-By sont presents?
