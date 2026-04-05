Tu es l'orchestrateur autonome du projet **Nika** — un workflow engine YAML semantique pour l'IA (5 verbs, 9 providers, 39 builtin tools, 353k LOC Rust, 12 crates). Tu travailles sans intervention humaine pendant 30+ heures. Commit, push, continue. Ne t'arrete JAMAIS sauf quand tout est fini.

---

# IDENTITE DU PROJET

| Cle | Valeur |
|-----|--------|
| Projet | Nika — Semantic YAML Workflow Engine for AI |
| Schema | `nika/workflow@0.12` |
| Version actuelle | v0.51.0 → **target v0.52.0** |
| LOC | 353,250 Rust |
| Tests | 8,888+ (0 fail, 0 clippy) |
| Branche | `main` |
| Remote | `github.com:supernovae-st/nika.git` |
| Repertoire | `/Users/thibaut/dev/supernovae/nika/` |
| Workspace | `tools/` (12 crates Cargo workspace) |
| CI | 8 workflows GitHub Actions (ci, release, sast, pr-lint, lsp, validate, stale, release-plz) |
| Phase 1 Intelligence | **100% COMPLETE** |

---

# VERIFICATION INITIALE (OBLIGATOIRE)

```bash
cd /Users/thibaut/dev/supernovae/nika/tools
git log --oneline -5
cargo test --workspace --lib 2>&1 | tail -5
cargo clippy --workspace -- -D warnings 2>&1 | tail -3
cat ../docs/plans/sessions/progress.md | head -20
```

Si les tests ne passent pas ou clippy a des warnings: **REPARE D'ABORD** avant de continuer.

---

# SKILLS A UTILISER (OBLIGATOIRE)

Tu DOIS utiliser ces skills pour chaque operation. Pas d'exception.

## Pour chaque bug fix / feature:
```
Skill: test-driven-development
  1. RED: ecrire un test qui ECHOUE
  2. GREEN: implementer le fix MINIMAL
  3. REFACTOR: nettoyer si necessaire
  4. cargo test --workspace --lib → 0 failures
  5. cargo clippy --workspace -- -D warnings → 0 warnings
  6. Commit
```

## Pour chaque probleme complexe:
```
Skill: systematic-debugging
  1. Identifier le symptome exact
  2. Former une hypothese
  3. Tester l'hypothese (grep, read, run)
  4. Confirmer root cause AVANT de coder
  5. Jamais "fix and pray"
```

## Avant chaque commit:
```
Skill: verification-before-completion
  1. cargo test --workspace --lib → TOUS passent
  2. cargo clippy --workspace -- -D warnings → ZERO
  3. Les nouveaux tests testent le BON comportement
  4. Pas de regression
```

## Pour les taches mecaniques:
```
Skill: dispatching-parallel-agents
  - Dispatch 4-6 agents sur des fichiers differents
  - Chaque agent: 2-3 fichiers max
  - Verifier compilation apres chaque agent
  - Merger et commiter apres verification globale
```

## Regle absolue:
```
Skill: defense-in-depth
  - Valider a chaque couche
  - Ne jamais faire confiance a l'input
  - Si un .ok() avale une erreur → c'est un bug
  - Si un _ => {} ignore un variant → c'est un bug
```

---

# LES 5 WAVES

## WAVE 1: QUALITY BLITZ (~8h, 6 agents paralleles)

### W1.1 — Sweep `_ => {}` (74 instances → 0)

```bash
cd /Users/thibaut/dev/supernovae/nika/tools
grep -rn '_ => {}' --include='*.rs' | grep -v test | grep -v target
```

**Regle**: Chaque `_ => {}` sur un match EventKind → `tracing::warn!("unhandled: {kind:?}")`. Sur un autre enum → `tracing::debug!()`.

Dispatch 3 agents:
- Agent 1: nika-engine/src/display/ + nika-engine/src/runtime/
- Agent 2: nika-tui/src/
- Agent 3: tous les autres crates

**Commit**: `fix(quality): replace 74 empty wildcard arms with logging`

### W1.2 — Audit `unwrap_or(0)` (125 → <30)

```bash
grep -rn 'unwrap_or(0)' --include='*.rs' | grep -v test | grep -v target
```

**Regle par contexte**:
- Tokens/durations: OK (0 est un defaut valide)
- Costs: REMPLACER par explicit handling
- Array indices: REMPLACER par logging
- Parse results: REMPLACER par error

Dispatch 3 agents par crate.

**Commit**: `fix(quality): audit 125 unwrap_or(0) — fix 40+ silent defaults`

### W1.3 — Dead code nuke (385 `#[allow(dead_code)]` → <50)

```bash
grep -rn '#\[allow(dead_code)\]' --include='*.rs' | grep -v test | grep -v target
```

**Strategie**:
1. Enlever le `#[allow(dead_code)]`
2. Si ca compile → le code EST utilise, supprimer l'allow (stale)
3. Si ca ne compile pas → le code est MORT, le supprimer
4. Garder UNIQUEMENT pour les champs de structs dans trait impls

**Commit**: `refactor(quality): remove 300+ stale allow(dead_code) + delete dead code`

### W1.4 — EventKind test coverage (36 untested → 0)

```
Variants SANS tests: TaskScheduled, TaskFailed, TaskSkipped, TemplateResolved,
ProviderCalled, ProviderFallback, McpInvoke, McpResponse, McpConnected, McpError,
PresetApplied, AgentStart, AgentComplete, AgentSpawned, RecordCreated, RecordSkipped,
Log, Custom, ArtifactWritten, ArtifactFailed, MediaIntegrityCheck, MediaCleanup,
TaskRetry, FallbackTriggered, PolicyBlocked, BindingDefaultApplied, BindingTransformApplied,
BindingEnvResolved, DecomposeStarted, DecomposeCompleted, ForEachStarted, ForEachCompleted,
ProviderInitialized, BuiltinToolInvoked, StreamingDelta, ExtractApplied
```

**Pattern par variant**:
```rust
#[test]
fn test_VARIANT_event() {
    let event = EventKind::VARIANT { task_id: Arc::from("t"), ... };
    assert_eq!(event.task_id(), Some("t")); // ou None
    let json = serde_json::to_string(&event).unwrap();
    assert!(json.contains("variant_name"));
    let round: EventKind = serde_json::from_str(&json).unwrap();
    // verify fields roundtrip
}
```

**Commit**: `test(event): add coverage for 36 untested EventKind variants`

### W1.5 — Error swallowing sweep

```bash
grep -rn '\.ok()' nika-engine/src/ --include='*.rs' | grep -v test | grep -v target | head -30
```

Pour chaque `.ok()`: est-ce que l'erreur est importante?
- Si oui → `map_err(|e| tracing::warn!(...)).ok()` ou propagation avec `?`
- Si non → documenter pourquoi avec `// Intentional: reason`

**Commit**: `fix(quality): audit .ok() error swallowing — fix 20+ silent drops`

### W1.6 — Weak assertions

```bash
grep -rn 'assert!(.*\.is_ok())' --include='*.rs' | head -50
```

Remplacer par: `let val = result.expect("descriptive message");`

**Commit**: `test(quality): strengthen 50 bare is_ok() assertions`

---

## WAVE 2: SHIP IT (~8h)

### W2.1 — cargo-deny setup (1h)

Fichier: `tools/deny.toml`
```toml
[licenses]
allow = ["AGPL-3.0-or-later", "MIT", "Apache-2.0", "BSD-2-Clause", "BSD-3-Clause", "ISC", "Zlib", "Unicode-3.0", "MPL-2.0", "Unicode-DFS-2016", "OpenSSL"]
confidence-threshold = 0.8

[bans]
multiple-versions = "warn"

[advisories]
vulnerability = "deny"
unmaintained = "warn"

[sources]
unknown-registry = "deny"
unknown-git = "deny"
```

Run: `cargo deny check 2>&1` et fixer les issues.

**Commit**: `chore(quality): add cargo-deny + fix license/advisory issues`

### W2.2 — CI fixes (2h)

Fichier: `.github/workflows/ci.yml`
- Rendre `cargo deny check` un hard failure (enlever `|| true`)
- Ajouter Windows a la matrice de test
- Ajouter MSRV check (rust-version = "1.86")

Fichier: `.github/dependabot.yml` (NEW)
```yaml
version: 2
updates:
  - package-ecosystem: cargo
    directory: /tools
    schedule: { interval: weekly }
  - package-ecosystem: github-actions
    directory: /
    schedule: { interval: weekly }
```

Fichier: `tools/nika/Dockerfile` — Update VERSION from 0.40.2 to 0.52.0

**Commit**: `ci: harden CI — cargo-deny hard fail, Windows tests, MSRV, dependabot`

### W2.3 — Version bump v0.52.0 (1h)

```bash
find tools -name "Cargo.toml" -not -path "*/target/*" | xargs sed -i '' 's/version = "0.51.0"/version = "0.52.0"/'
```

Verifier: `grep -rn 'version = "0.51' tools/*/Cargo.toml`

**Commit**: `chore(release): bump all crates to v0.52.0`

### W2.4 — CHANGELOG complet (1h)

Fichier: `CHANGELOG.md` — documenter TOUT depuis v0.51.0

### W2.5 — Install script (1h)

Fichier: `install.sh` (repo root)
- Detecter OS + arch
- Telecharger latest release GitHub
- Installer dans /usr/local/bin ou ~/.local/bin
- Executer `nika doctor --fix`

**Commit**: `ci(dist): add one-line install script`

### W2.6 — Shell completions dans release (1h)

Fichier: `scripts/generate-completions.sh` (NEW)
Integrer dans le release workflow.

**Commit**: `ci(dist): add shell completions to release artifacts`

---

## WAVE 3: ENGINE POLISH (~6h)

### W3.1 — ProviderName migration engine (3h)

L'enum `ProviderName` existe dans `nika-core`. `AnalyzedTask.provider` et `AnalyzedWorkflow.provider` sont deja migres.

Migrer:
- `InferParams.provider: Option<String>` → `Option<ProviderName>`
- `AgentParams.provider: Option<String>` → `Option<ProviderName>`
- `executor.default_provider: Arc<str>` → `ProviderName`
- Tous les `.as_str()` / `.to_string()` aux frontieres

Dispatch 4 agents par fichier.

**Commit**: `refactor(engine): migrate ProviderName to engine layer`

### W3.2 — Merge dual pricing (1h)

Supprimer `nika-engine/src/provider/cost.rs`, utiliser `nika-core::catalogs::cost` partout.

**Commit**: `refactor(provider): merge dual pricing tables — nika-core canonical`

### W3.3 — Workspace dependencies (30min)

Deplacer serde, serde_json, tokio, tracing, parking_lot, dashmap dans `[workspace.dependencies]`.

**Commit**: `chore(deps): unify shared dependencies to workspace level`

### W3.4 — unreachable + TODO (1.5h)

3 `unreachable!()` → proper NikaError.
Top 50 TODO → fix, convert to NOTE, or delete.

**Commit**: `fix(quality): replace reachable unreachable!() + triage TODO markers`

---

## WAVE 4: LSP + REGISTRY (~4h)

### W4.1 — LSP extension (2h)

- `editors/vscode/src/extension.ts`: registerCommand() en haut de activate()
- `editors/vscode/package.json`: files.associations pour .nika.yaml
- Fix template_validation.rs unwrap() crash

**Commit**: `fix(lsp): extension activation + file association + crash fix`

### W4.2 — Registry fallback (1h)

Graceful offline mode quand registry unreachable.

**Commit**: `fix(registry): graceful fallback when offline`

### W4.3 — LSP completions (1h)

Completer preset: depuis le bloc agents:.

**Commit**: `feat(lsp): preset completions from agents block`

---

## WAVE 5: RELEASE (~4h)

### W5.1 — Docs finaux

- CHANGELOG.md complet
- CLAUDE.md mis a jour (39 tools, 66 events, 163 error codes)
- progress.md mis a jour

### W5.2 — Tag + push

```bash
cargo test --workspace --lib → TOUT PASSE
cargo clippy --workspace -- -D warnings → ZERO
git tag v0.52.0
git push && git push --tags
```

---

# REGLES ABSOLUES

```
1. cargo test --workspace --lib    TOUJOURS (--lib = pas de keychain macOS)
2. TDD: test FAIL → fix → test PASS → full suite → commit
3. 1 fix = 1 commit (jamais de batch non-relie)
4. Co-authors TOUJOURS:
   Co-Authored-By: Nika 🦋 <nika@supernovae.studio>
5. git push apres 2-3 commits
6. cargo clippy --workspace -- -D warnings → ZERO
7. JAMAIS commiter du code qui ne compile pas
8. Si bloque 3x → skip, note dans progress.md, continue
9. JAMAIS marquer un bug "done" sans code fix + test reel
10. AGPL-3.0-or-later pour tous les crates
```

# FORMAT DE COMMIT

```
type(scope): description

Co-Authored-By: Nika 🦋 <nika@supernovae.studio>
```

Types: feat, fix, refactor, test, perf, docs, chore, ci, style
Scopes: runtime, ast, parser, event, cli, builtin, tui, dag, binding, provider, mcp, security, lsp, daemon, core, quality, release, dist

# GESTION DU CONTEXT WINDOW

Quand le context se remplit:
1. Commit + push tout
2. Mettre a jour progress.md
3. Relancer:

```bash
claude --dangerously-skip-permissions --model opus -p "$(cat docs/plans/sessions/mega-prompt-v14-instruction.md)"
```

# METRIQUES DE SUCCES

| Metrique | Avant | Cible |
|----------|-------|-------|
| Tests | 8,888 | 9,100+ |
| `_ => {}` | 74 | 0 |
| `unwrap_or(0)` | 125 | <30 |
| `#[allow(dead_code)]` | 385 | <50 |
| EventKind tested | 30/66 | 66/66 |
| Version | v0.51.0 | v0.52.0 tagged |
| CI | cargo-deny soft | cargo-deny hard |
| ProviderName | String | typed enum |

# COMMENCER

```bash
cd /Users/thibaut/dev/supernovae/nika/tools
git log --oneline -5
cargo test --workspace --lib 2>&1 | tail -5

# GO — WAVE 1 → WAVE 2 → WAVE 3 → WAVE 4 → WAVE 5 → v0.52.0
```

---

# SECTION BONUS — FINDINGS DES 11 AGENTS SPECIALISES (line numbers exactes)

## CRITICAL SECURITY (fix dans Wave 1)

| # | Bug | File:Line | Fix |
|---|-----|-----------|-----|
| SEC-1 | IPv4-compatible IPv6 `::127.0.0.1` bypasses SSRF | `policy.rs:46-68` | Add `v6.to_ipv4()` check (catches both mapped AND compatible) |
| SEC-2 | `/usr/bin/sudo` bypasses blocklist (full path) | `security.rs:28-137` | Strip path prefix from first token before blocklist match |
| SEC-3 | Symlink artifact escape via `ln -s` in exec task | `io/security.rs:436-457` | Call `validate_canonicalized_boundary()` on parent after mkdir |
| SEC-4 | `canonicalize()` failure silently skips symlink check | `io/writer.rs:229,311` | Treat canonicalize Err as ArtifactPathError |

## HIGH BUGS (fix dans Wave 1)

| # | Bug | File:Line | Fix |
|---|-----|-----------|-----|
| BUG-1 | SpawnAgentTool gets disconnected CancellationToken | `rig_agent_loop/mod.rs:263` | Pass `self.cancel_token.child_token()` |
| BUG-2 | HashMap `depths[key]` can panic | `runner.rs:1481,1504` | Use `.get().copied().unwrap_or(0)` |
| BUG-3 | `println!` not guarded by `!self.quiet` | `runner.rs:1497-1499` | Add `if !self.quiet` guard |
| BUG-4 | 5 JSONPath errors logged at debug! (should be warn!) | `run_context.rs:420,488,506,523,605` | Change to `tracing::warn!` |
| BUG-5 | 5 builtin tools use `.unwrap_or_default()` for serialization | `tools/{grep,edit,write,read,glob}.rs` | Map to `Err(NikaError::BuiltinToolError)` |
| BUG-6 | Template malformed `{{...}}` silently passes through to LLM | `template.rs:474,1100` | Add `tracing::warn!` for malformed expressions |

## PERFORMANCE (fix dans Wave 3)

| # | Finding | File:Line | Fix |
|---|---------|-----------|-----|
| PERF-1 | `resolve_alias_path` clones Value unconditionally | `template.rs:283` | Use Cow<Value> to borrow when no parse needed |
| PERF-2 | `compute_depths` is O(V^2), existing Kahn's BFS is O(V+E) | `dag/flow.rs:266-319` | Reuse `compute_layers()` result |
| PERF-3 | MCP connections sequential in agent setup | `executor/agent.rs:234-237` | Use `futures::future::join_all` |

## DEAD CODE (fix dans Wave 1)

| Verdict | Count | Items |
|---------|-------|-------|
| REMOVE allow (code IS used) | 10 | png_crc, fixture_png/jpeg, setup, Action enum, McpClient::name |
| DELETE code (truly dead) | 4 | `artifact_paths` field in runner, `setup_with_working_memory`, `RetryCondition` enum, `SetupResult.message` |
| KEEP (justified) | 10 | RAII handles, cfg-gated, public API, palette |

## ERROR HANDLING (top 5 to fix)

| # | Pattern | File:Line | Fix |
|---|---------|-----------|-----|
| ERR-1 | Workflow output silently defaults to "" | `runner.rs:2570` | warn! when get_final_output() is None |
| ERR-2 | Structured output layers discard prev errors | `structured_output.rs:318,346,365` | Accumulate layer errors in Vec |
| ERR-3 | for_each type extraction silently returns None | `runner.rs:128` | warn! for non-empty items that fail parse |
| ERR-4 | Binding transform NullInput catches ALL errors | `resolve.rs:571` | Match specifically on NullInput |
| ERR-5 | env::current_dir().unwrap_or_default() = empty PathBuf | `executor/mod.rs:273` | Return early with descriptive error |

## ARCHITECTURE NOTES (for reference, not immediate action)

- `nika-engine` (150k LOC) should eventually split into `nika-runtime` + `nika-display`
- NikaError 101 variants → 14 domain wrappers (error_domains.rs Phase 2-4)
- `nika-media → nika-mcp` inverted dependency (ContentBlock should be in nika-core)
- `nika-engine → nika-init` unnecessary (scaffolding not needed by runtime)
- Display module (12k LOC) is lowest-hanging extraction for `nika-display` crate

---

Pas de questions. Lis, code, test, commit, push, continue.
