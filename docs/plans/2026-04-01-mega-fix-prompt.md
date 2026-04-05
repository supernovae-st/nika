# MEGA FIX v2 — nika CLI UX + remaining serve issues

> **Copie-colle ce fichier entier comme premier message a un agent Claude Code.**
> Working directory: `/Users/thibaut/dev/supernovae/nika`

---

## Situation (updated April 1 18:30)

Deux agents ont deja fait beaucoup de travail :

### Ce qui est DEJA FIX (ne pas refaire)

| Bug | Fix | Commit |
|-----|-----|--------|
| BUG-1 (output pollue) | ExecutorMode default = Embedded | `ffa1ceafe` |
| BUG-2 (env leak) | Explicit API key allowlist, no NIKA_VAULT_PASSPHRASE | `a6c9eeded` |
| BUG-3 (shutdown signal) | shutdown_tx.send() moved into signal closure | dans lib.rs |
| BUG-5 (SSE orphans) | EventBus cleanup in WorkerGuard drop | `8973cd8d1` |
| BUG-7 (metrics auth) | Auth middleware on metrics | `0fe340d4e` |
| BUG-8 (webhook SSRF) | DNS pinning + redirect blocking | `16cc98aac` + `3a9d17652` |
| BUG-6 (input validation) | Input key validation hardening | `a6c9eeded` |
| BUG-9 (panic warning) | Executor warning at startup | `a6c9eeded` |
| nika.toml migration | `[serve]` section in nika.toml | `ffa1ceafe` |
| BLAKE3 artifacts | Content-addressable checksums | `6b959c88d` |

### Ce qui RESTE A FIXER

Les bugs CLI UX trouves pendant le setup VPS. Ce sont les plus impactants pour l'utilisateur.

---

## Lis ces fichiers d'abord

1. **`docs/plans/2026-04-01-cli-ux-improvements.md`** — 21 bugs/improvements CLI avec details
2. **`CLAUDE.md`** — Language reference
3. **`tools/nika-cli/src/onboarding.rs`** — Le wizard onboarding

---

## PRIORITE 1 : Wizard onboarding loop (BLOCKER)

**Symptome :** Sur un VPS, apres `nika keys set openai` (qui dit "Connection OK"), chaque `nika infer "prompt" -p openai` relance le wizard "Welcome to Nika! Let's set up your first provider".

**Root cause probable :** `has_any_provider_key()` dans `tools/nika-cli/src/onboarding.rs` check les env vars + le vault. Sur le VPS, le vault unlock peut echouer silencieusement (machine-id issue, ou le daemon n'est pas connecte au moment du check). La fonction retourne `false` → le wizard se declenche.

**Aussi :** `nika keys set` devrait marquer l'onboarding comme done en appelant `set_no_onboarding()` apres un set reussi. Actuellement il ne le fait pas.

**Fichiers :**
- `tools/nika-cli/src/onboarding.rs` — `has_any_provider_key()`, `skip_onboarding()`, `set_no_onboarding()`
- `tools/nika-cli/src/provider.rs` — `handle_set()` devrait appeler `set_no_onboarding()`
- `tools/nika/src/main.rs` — lignes 1040, 1650-1656, 2451-2461 (les 3 endroits qui checkent l'onboarding)

**Fix :**
1. Dans `provider.rs` : apres un `set` reussi, appeler `onboarding::set_no_onboarding()`
2. Dans `onboarding.rs` : `has_any_provider_key()` — si le vault check echoue (Err), logger un warn et continuer (ne pas declencher le wizard juste parce que le vault est inaccessible)
3. Ajouter un fichier flag `~/.nika/.onboarding_done` cree par `provider set` — le check le plus fiable (pas de daemon, pas de vault, juste un fichier)
4. `has_any_provider_key()` check aussi ce fichier flag en plus de env + vault

**Test :**
```bash
# Sur un VPS ou en simulant (sans daemon)
NIKA_NO_DAEMON=1 nika keys set openai  # coller une cle
nika infer "test" -p openai                 # NE DOIT PAS lancer le wizard
```

## PRIORITE 2 : Default provider quand seul openai est configure

**Symptome :** `nika infer "prompt"` (sans `-p`) default a anthropic → fail "No provider configured" meme si openai est set.

**Root cause :** Le default provider dans le code est hardcode `anthropic`. Devrait etre le premier provider qui a une cle.

**Fichier :** `tools/nika/src/main.rs` — chercher ou le default provider est determine pour `nika infer`

**Fix :** Si aucun `--provider` n'est specifie, iterer les providers dans l'ordre et utiliser le premier qui a une cle (env ou vault). Si aucun → montrer un message clair : "No providers configured. Run: nika keys set openai"

## PRIORITE 3 : `--no-interactive` sur tous les verbs

**Symptome :** `nika infer "test" -p openai --no-interactive` → "unexpected argument"

**Fichier :** `tools/nika/src/main.rs` — les structs Args pour InferArgs, FetchArgs, InvokeArgs, AgentArgs

**Fix :** Ajouter `#[arg(long)]` pour `no_interactive: bool` sur InferArgs (et les autres verbs). Quand `no_interactive` est true, set `onboarding::set_no_onboarding()` au debut.

## PRIORITE 4 : `nika keys set` warn quand env var existe

**Symptome :** User fait `nika keys set openai`, la cle va dans le vault, mais `OPENAI_API_KEY` dans l'env ecrase le vault. Le user ne comprend pas pourquoi sa nouvelle cle ne marche pas.

**Fichier :** `tools/nika-cli/src/provider.rs` — `handle_set()`

**Fix :** Apres `set` reussi, checker si l'env var correspondante existe :
```rust
let env_var = format!("{}_API_KEY", provider_name.to_uppercase());
if std::env::var(&env_var).is_ok() {
    println!("  ⚠ {} also found in environment.", env_var);
    println!("    The env var takes priority over the vault.");
    println!("    To use the vault key: unset {}", env_var);
    println!("    Or run: nika provider migrate");
}
```

## PRIORITE 5 : Startup banner pour `nika serve`

**Symptome :** `nika serve` demarre sans rien afficher — juste un curseur qui clignote. L'utilisateur ne sait pas si ca marche.

**Fichier :** `tools/nika-serve/src/lib.rs` — `run_server()`, juste avant `axum::serve()`

**Fix :** Ajouter un banner au demarrage :
```
  🦋 Nika Serve v0.58.1

  ├── Listening    http://0.0.0.0:3000
  ├── Workflows    /home/nika/nk-jungo/workflows/ (5 files)
  ├── Executor     embedded
  ├── Max jobs     6 concurrent
  ├── Timeout      600s per job
  ├── Auth         Bearer token (64 chars)
  └── Providers    openai ✓  xai ✓  gemini ✗

  Ready. Ctrl+C to stop.
```

Lister les fichiers `.nika.yaml` dans le workflows dir. Checker quels providers ont des cles (env ou vault).

## PRIORITE 6 : `nika provider list` ameliore

**Symptome :** La liste est un flat list de noms. On voit pas les models, le source (vault/env), ni la difference cloud/vllm/local.

**Fichier :** `tools/nika-cli/src/provider.rs` — `handle_list()`

**Fix :** Afficher :
```
  Cloud Providers
  ├── ✓ openai    [sk-proj-****]  (vault)   gpt-4.1-mini, gpt-4.1, gpt-4o
  ├── ✓ xai       [xai-KVIT****]  (env)     grok-3, grok-3-mini
  └── ✗ anthropic                            → nika keys set anthropic

  Custom Endpoints
  └── (none configured — add with: nika endpoint add <name> --url <url>)

  Local
  └── ✗ native    (no GGUF model)           → nika model pull <name>

  Test
  └── ✓ mock      (always available)
```

Distinguer `(vault)` vs `(env)`. Montrer les top models par provider. Montrer un hint pour les non-configures.

## PRIORITE 7 : "Did you mean?" sur les typos

**Symptome :** `nika set provider openai` → "unrecognized subcommand 'openai'" sans suggestion.

**Fichier :** `tools/nika/src/main.rs` — post-parse error handling

**Fix :** Utiliser clap's `suggest_arg` ou ajouter un fuzzy matcher post-erreur avec Levenshtein distance. Si la distance est ≤ 2, suggerer la bonne commande.

## PRIORITE 8 : Chaque commande finit par "what's next"

**Fichier :** Tous les handlers CLI

**Principe :** Apres chaque action reussie, montrer 2-3 commandes pertinentes :
- Apres `provider set` → "Try: nika infer 'hello' -p openai"
- Apres `provider migrate` → "Keys in vault. You can: unset OPENAI_API_KEY"
- Apres `nika serve` start → "Test: curl http://localhost:3000/health"
- Apres `nika run` success → "View trace: nika trace show <id>"

---

## Regles

```
cargo test --workspace --lib    ← TOUJOURS --lib
cargo clippy --workspace        ← clean
```

- 1 fix = 1 commit
- `fix(cli): ...` ou `feat(cli): ...`
- Co-authors : Claude + Nika 🦋
- Test AVANT commit
- AGPL-3.0-or-later

## Ordre d'execution

```
1. PRIO-1 (wizard loop)        → commit → test ← LE PLUS IMPORTANT
2. PRIO-2 (default provider)   → commit → test
3. PRIO-3 (--no-interactive)   → commit → test
4. PRIO-4 (env var warning)    → commit → test
5. PRIO-5 (serve banner)       → commit → test
6. PRIO-6 (provider list)      → commit → test
7. PRIO-7 (did you mean)       → commit → test
8. PRIO-8 (what's next hints)  → commit → test
9. cargo build --release
```

## Architecture

```
tools/nika/src/main.rs              ← CLI entry, onboarding checks (3 locations)
tools/nika-cli/src/
  ├── onboarding.rs                 ← Wizard, skip_onboarding, has_any_provider_key
  ├── provider.rs                   ← provider set/list/test/migrate/get/delete
  └── lib.rs                        ← Module exports
tools/nika-serve/src/
  ├── lib.rs                        ← run_server(), startup banner location
  └── config.rs                     ← ServeConfig (already reads nika.toml)
tools/nika-core/src/vault/          ← NikaVault (XChaCha20Poly1305)
```

## Ce que tu NE dois PAS faire

- NE PAS toucher au runtime/engine (runner.rs, DAG, bindings)
- NE PAS modifier le schema YAML
- NE PAS ajouter de crates sauf si strictement necessaire
- NE PAS push sur GitHub
- NE PAS modifier les test fixtures E2E existants (ajouter OK)

## Contexte VPS pour tester

Le VPS nk-jungo-vps (51.159.87.214) a nika v0.58.1 avec :
- openai + xai configures (vault + env)
- Onboarding wizard qui loop (le bug principal)
- nika serve qui tourne sur :3000 (embedded executor)
- Workflows dans /home/nika/nk-jungo/workflows/

Les bugs CLI ont ete decouverts pendant le setup de ce VPS. La priorite est de rendre le CLI utilisable sur un serveur headless.
