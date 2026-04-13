# Session 14 — Mega-prompt Handoff (v3 — final)

> ⚠️ **SUPERSEDED 2026-04-11** — Voir `20b-session14-scope-correction.md` pour le scope exécuté.
>
> Ce document a été écrit contre HEAD `a3e8d8ab8` mais 11 commits ont landed entre sa rédaction
> et l'exécution S14. Les labels `W14-A0/A1/B0/E1` collidaient sémantiquement avec des commits
> déjà landés, et 2 bugs flaggés P0/P1 (BUG-P0 fetch guard, BUG-P1 verb-exec pre-spawn cancel)
> se sont révélés non-bugs après audit async-expert. La Phase 1 review a ramené le scope à
> **5 commits safe** (S14-α à ε). Voir 20b pour le détail + post-mortem.
>
> Le document original est conservé pour contexte historique uniquement.

---

> **Auteur / date:** Claude Sonnet 4.6, 2026-04-11
> **Source:** review 6 agents parallèles (rust-pro ×2, rust-architect ×2, code-explorer, rust-async-expert)
> **Gitignored** — local uniquement, ne jamais push.

---

## 0. Orientation rapide

Tu commences la **Session 14** de la refactorisation Nika Constellation V2.3.
Tu n'as aucun souvenir des sessions précédentes. Ce document est ton seul contexte.

```
Working dir : tools/   (submodule public dans un monorepo privé)
Tests       : cargo test --workspace --lib   (jamais --test — popups Keychain macOS)
Co-author   : Co-Authored-By: Nika 🦋 <nika@supernovae.studio>   (JAMAIS Claude)
Launch      : 2026-05-05  — refactor partiel = état valide pour lancer
Langue      : Franglais conversation, EN code/docs/commits
```

---

## 1. Baseline exact (post-S14 Wave A–B partielle, 2026-04-11)

```
HEAD     : a3e8d8ab8  docs(constellation): ARCHITECTURE.md update Session 13 (S13-E1)

MAIS — 4 commits S14 sont déjà landés DANS S13 (vérifier avec git log) :
  2ddd28ca1  feat(verb-infer): nika-verb-infer crate, 9 tests (W14-B1) ✅
  040bfad4a  feat(runtime): verb_infer adapter + infer_caps() (W14-B3) ✅
  0dc079757  fix(exec): NonZeroExit includes exit_code (S14-BUG1) ✅
  3cc49f3d1  fix(engine): remove duplicate McpInvoke event (S14-BUG2) ✅

Crates   : 33  (32 post-S13 + nika-verb-infer)
Engine   : 146,473 LOC  (−84 vs S13 grâce aux BUG fixes)
Tests    : ~10,840 (lib)
Clippy   : 0 warnings
```

> **Action pré-flight #0:** Avant tout code, lance `git log --oneline -20` et note le vrai HEAD.
> L'architecture.md est la source de vérité — lis-la.

---

## 2. Carte de l'extraction — ce qui est fait vs ce qui reste

```
VERBE       CRATE           BRIDGE ENGINE       DISPATCH ARM
─────────────────────────────────────────────────────────────
exec    ✅ S13-B1          ✅ S13-B2            ✅ S13-B4
fetch   ✅ S13-D1 (min.)   ❌ NON BRIDGÉ       ✅ S13-D2 (NotImpl)
invoke  ✅ S13-C1 (builtins) ✅ S13-C2 (partiel) ✅ S13-C3
infer   ✅ W14-B1 (min.)   ❌ NON BRIDGÉ       ✅ W14-B3 (NotImpl)
agent   ❌ N'EXISTE PAS    ❌                   ❌  (S15)
─────────────────────────────────────────────────────────────
dispatch() live path : MORT (AMEND-4) — TaskExecutor reste le chemin live
TaskExecutor struct  : VIVANT — dissolution = S15 (agent bloque)
Runner               : DANS nika-engine — migration = S15
```

### Ce que fait `nika-verb-infer` aujourd'hui (W14-B1, déjà landé)

```rust
// tools/nika-verb-infer/src/lib.rs
pub struct InferInput {
    pub request: InferRequest,   // pré-construit par le bridge engine
    pub task_id: Arc<str>,
}
pub async fn run(input: &InferInput, caps: &InferCaps<'_>, event_log: &EventLog)
    -> Result<InferOutput, VerbInferError>

// InferCaps minimale qui suffit pour ce run() :
pub struct InferCaps<'a> {
    pub provider: Arc<dyn Provider>,   // l'appel infer()
    pub cancel: &'a CancellationToken, // select! biased cancel
    // + fs_read, policy, clock, workflow_base_dir (présents, inutilisés en W14-B1)
}
```

**Le bridge `infer.rs` → `nika-verb-infer::run()` (W14-B2) est déféré à S15.**
Raison : infer.rs a 2157 LOC dont ~1980 resteraient dans le bridge après extraction
(Shield/spotlight, SkillInjector, StructuredOutputEngine, RigProvider concret, cost
calculation — tous engine-internes sans trait kernel). Gain réel : 177 LOC (8%). Non rentable.

### Ce que fait `nika-verb-invoke` aujourd'hui (S13-C1 + S13-C2)

Gère uniquement les outils `nika:*` (builtin path). La MCP path retourne
`VerbInvokeError::Validation` — `NoopMcpPool` est un placeholder.

---

## 3. Bugs à fixer avant tout (commits 0a et 0b)

### BUG-P0 — `fetch.rs:~147` — RwLockReadGuard !Send tenu à travers un await

**Violation de l'invariant sacré #7.**

```rust
// CODE ACTUEL CASSÉ dans executor/fetch.rs :
let string_decision = self.policy_enforcer.read().check_fetch(&url);
// ... quelques lignes ...
let addrs = resolve_and_pin_ssrf(host).await?;  // guard encore vivant = !Send

// FIX :
let string_decision = { self.policy_enforcer.read().check_fetch(&url) };
// Guard drop ici (fin du bloc intérieur)
if !string_decision.is_allowed() { return Err(...); }
let addrs = resolve_and_pin_ssrf(host).await?;   // OK — guard relâché
```

**Commit:** `fix(engine): drop policy_enforcer guard before SSRF await in fetch.rs (BUG-P0-S14)`

### BUG-P1 — `nika-verb-exec/src/lib.rs` — pre-spawn cancellation manquante

TokioShell gère la cancel pendant l'exécution (via `ShellCommand::cancel`),
mais pas le cas "déjà cancelled avant spawn". Fix: ajouter avant `caps.shell.run(cmd)`:

```rust
if caps.cancel.is_cancelled() {
    return Err(VerbExecError::Cancelled { task_id: input.task_id.to_string() });
}
```

**Commit:** `fix(verb-exec): add pre-spawn cancellation check (BUG-P1-S14)`

---

## 4. Plan de commits S14 — scope réel (post-review 6 agents)

### Préambule — ce qu'il NE FAUT PAS faire en S14

| Item | Raison d'exclusion |
|------|--------------------|
| `infer.rs` bridge surgery (W14-B2) | 2157→1980 LOC seulement (8%). SkillInjector + StructuredOutputEngine + Shield sans trait kernel. S15. |
| nika-verb-agent (Wave C) | 10+ TEMP engine deps irrésolvables. S15/S16. |
| TaskExecutor dissolution (Wave D5) | Bloqué sur agent. S15. |
| dispatch() live activation (Wave E) | Bloqué sur D5. S15. |
| `infer_vision`, `infer_with_tools`, `is_anthropic()` sur Provider trait | Violation invariant #17. `InferRequest` les encode déjà. INTERDIT. |

---

### Commit 0a — BUG-P0 fetch.rs guard

`fix(engine): drop policy_enforcer guard before SSRF await (BUG-P0-S14)`

Vérification: `cargo check --workspace` passe. Les tests fetch passent.

---

### Commit 0b — BUG-P1 verb-exec cancel

`fix(verb-exec): add pre-spawn cancellation check (BUG-P1-S14)`

Vérification: le test `fetch_cancelled_returns_cancelled` existant est le modèle.
Ajouter test équivalent dans nika-verb-exec si manquant.

---

### W14-A0 — `InferEvent::Done` enrichi (UNIQUEMENT)

`feat(kernel): enrich InferEvent::Done with streaming metadata (W14-A0)`

Le plan original proposait beaucoup trop. **La seule modification justifiée:**

```rust
// tools/nika-kernel/src/provider.rs
// AVANT :
Done(StopReason),

// APRÈS :
/// Terminal event — carries token usage and provider metadata.
/// #[non_exhaustive] : futurs champs (cost_usd, cached_tokens) sans breaking change.
#[non_exhaustive]
Done {
    stop_reason: StopReason,
    request_id: Option<String>,
    finish_reason_raw: Option<String>,
},
```

Mettre à jour tous les `match event { InferEvent::Done(reason) => ... }` dans nika-engine
vers `InferEvent::Done { stop_reason, .. } =>`.

**Ce qu'il NE FAUT PAS ajouter à W14-A0:**
`infer_vision`, `infer_with_tools`, `infer_streaming` (doublon de `infer_stream`),
`is_anthropic`, `supports_vision`, `supports_thinking` — tous en violation de l'invariant #17.

Vérification: `cargo check --workspace`. `InferEvent::Done` est `#[non_exhaustive]`.

---

### W14-A1 — Fetch retry migration dans nika-verb-fetch

`refactor(engine): migrate retry loop + helpers to nika-verb-fetch (W14-A1)`

Migrer depuis `fetch.rs` vers `nika-verb-fetch` les blocs extractables (~340 LOC):
- `safe_backoff_delay` (L43-52) — logique pure
- Boucle retry avec `FetchRetryConfig` (L477-631) — migre avec le config struct
- `merge_link_hreflang` + `merge_link_hreflang_value` + `dedup_hreflang` (L1145-1206)
- `parse_retry_after` (L1209-1226)
- Tests associés (L1228-1401)

Étendre `FetchInput` dans nika-verb-fetch:
```rust
pub retry: Option<FetchRetryConfig>,          // migrer la boucle retry
pub response_mode: Option<FetchResponseMode>, // Full / Binary
```

**Ce qui RESTE dans `fetch.rs` (~1060 LOC — irréductible en S14):**
SSRF pre-check + DNS pinning, custom HTTP client build (redirect policy, cookie jar),
robots.txt check, DomainRateLimiter, response:binary → CAS + MediaRef,
llm_txt multi-sub-requests, ETag/FetchCache + 304, post-redirect SSRF check.

**AJOUTER un test de régression SSRF (GATE-S14-NEW3):**
Un test wiremock qui vérifie qu'un redirect vers `169.254.x.x` est rejeté
même après délégation à nika-verb-fetch (évite TOCTOU SSRF silencieux).

`fetch.rs` : 1401 → **~1060 LOC**.

---

### W14-B0 — McpPool real impl + cancellation

`feat(verb-invoke): wire real McpPool impl + cancel (W14-B0)`

- Créer `McpPoolAdapter` dans nika-engine wrappant `nika-mcp::McpClient`:
  ```rust
  // tools/nika-engine/src/... (# TEMP: migre vers L1 crate en S15)
  pub struct McpPoolAdapter(Arc<nika_mcp::McpClient>);
  impl McpPool for McpPoolAdapter {
      async fn call_tool(&self, server, tool, args) -> Result<Value, McpError> { ... }
      async fn read_resource(&self, uri) -> Result<String, McpError> { ... }
  }
  ```

- Mettre à jour `nika-verb-invoke::run()` pour router les non-builtins via `caps.mcp_pool`

- **Obligatoire — cancellation (invariant #15):**
  ```rust
  let mcp_result = tokio::select! {
      biased;
      _ = caps.cancel.cancelled() => return Err(VerbInvokeError::Cancelled { ... }),
      r = caps.mcp_pool.call_tool(server, tool, args) => r,
  };
  ```

- Supprimer `NoopMcpPool` shim

- Ajouter test de cancellation MCP dans nika-verb-invoke
  (manquant depuis S13 — voir BUG découvert en review async-expert)

---

### W14-B4 — Resource reads dans nika-verb-invoke

`feat(verb-invoke): wire resource reads via McpPool (W14-B4)`

Router `InvokeInput::resource` URIs via `caps.mcp_pool.read_resource(uri)`.
Actuellement retourne `VerbInvokeError::Validation` pour tout `resource:` field.
Nécessite W14-B0 landé en premier.

---

### W14-D0 — Tests executor non-agent vers verb crates

`refactor(engine): migrate non-agent executor tests to verb crates (W14-D0)`

Migrer les tests de `executor/tests.rs` qui testent exec, fetch, invoke, infer
vers leurs verb crates respectifs. Les tests agent restent dans `executor/tests*.rs`.

Objectif: réduire le couplage test → TaskExecutor sans supprimer la struct
(qui reste vivante jusqu'à agent extraction en S15).

---

### W14-E0 — Cleanup shims

`chore(engine): remove NullBlobStore + NullHttpClient shims (W14-E0)`

Supprimer les shims dans `invoke.rs` devenus inutiles après W14-B0.
**Nécessite W14-B0 landé.**

---

### W14-E1 — ARCHITECTURE.md S14

`docs(constellation): ARCHITECTURE.md update for Session 14 (W14-E1)`

Mettre à jour:
- Crate count: 33 → 34 (si nika-verb-infer pas encore compté)
- Engine LOC: ~146k → ~142-143k (fetch retry + cleanup)
- Confirmer Wave C et W14-B2 bridge surgery = S15
- Ajouter invariants nouveaux #14-19

---

### W14-PRE-S15 — Golden oracle pour infer bridge (écrire, ne pas activer)

`test(runtime): golden oracle captures ProviderResponded fields (W14-PRE-S15)`

**À faire avant que S15 touche `infer.rs` (leçon G2 appliquée préventivement):**

Écrire un test golden qui capture `ProviderResponded` avec ses champs concrets
(pas juste sa présence). Ce test doit passer AVANT W14-B2 (S15) et rester vert après.

Pattern: `assert_eq!(event.text_content, "expected output")` — pas juste
`assert!(events.iter().any(|e| matches!(e, EventKind::ProviderResponded { .. })))`.

---

## 5. Séquence d'exécution optimale

```
[PARALLÈLES — aucune dépendance mutuelle]
    Commit 0a   BUG-P0 (fetch.rs guard)
    Commit 0b   BUG-P1 (verb-exec cancel)
    W14-A0      InferEvent::Done enrichi (kernel)
    W14-B0      McpPool real impl (invoke)

[APRÈS Commit 0a]
    W14-A1      Fetch retry migration

[APRÈS W14-B0]
    W14-B4      Resource reads (invoke)
    W14-E0      Supprimer shims NullBlobStore/NullHttpClient

[INDÉPENDANT]
    W14-D0      Tests executor migration
    W14-PRE-S15 Golden oracle infer (écrire maintenant, utile en S15)

[DERNIER]
    W14-E1      ARCHITECTURE.md update
```

---

## 6. Phase 0 — Re-absorption obligatoire (avant tout code)

**Lire dans cet ordre:**

1. Ce document
2. `git log --oneline -25` — identifier le vrai HEAD et les commits S14 déjà landés
3. `tools/.claude/rules/architecture.md` — invariants sacrés #1-19
4. `tools/nika-kernel/src/provider.rs` — shape actuelle de `Provider` et `InferEvent`
5. `tools/nika-verb-infer/src/lib.rs` — W14-B1 déjà landé (9 tests)
6. `tools/nika-runtime/src/dispatch.rs` — skeleton S13 (confirmation dead code)
7. `tools/nika-engine/src/runtime/executor/fetch.rs` lignes 140-180 — BUG-P0 location
8. `tools/nika-verb-invoke/src/lib.rs` — S13-C1, MCP path actuel (retourne Validation)

**Script pre-flight:**
```bash
cd tools/
git log --oneline -25 | grep "W14\|S14"          # Quels commits S14 sont déjà landés ?
cargo check --workspace 2>&1 | grep "^error"      # 0
cargo check --workspace --no-default-features 2>&1 | grep "^error"  # 0 (invariant G3)
cargo clippy --workspace 2>&1 | grep "^error"     # 0
cargo test --workspace --lib 2>&1 | tail -3       # ok. NNNN passed

# BUG-P0 — localiser le guard problématique:
grep -n "policy_enforcer.read\(\)" nika-engine/src/runtime/executor/fetch.rs
# Chercher le pattern où le guard traverse un .await dans le même bloc

# InferEvent::Done actuel:
grep -n "Done\|InferEvent" nika-kernel/src/provider.rs | head -20

# Provider trait actuel (vérifier infer_stream déjà présent):
grep -n "fn infer" nika-kernel/src/provider.rs

# nika-verb-infer state (W14-B1 déjà landé ?):
ls tools/nika-verb-infer/src/ 2>/dev/null && echo "EXISTS" || echo "NOT YET"
grep -c "fn " tools/nika-verb-infer/src/lib.rs 2>/dev/null
```

---

## 7. Phase 1 — Agents parallèles (optionnel — déjà fait)

La review 6-agents a été effectuée pré-S14. Findings intégrés dans ce document.
**Ne pas relancer une Phase 1 complète** — perte de temps. À la place:

Si un doute subsiste sur un point précis, lancer 1 agent ciblé (ex: rust-pro sur
un fichier spécifique). Garder en background pendant que tu codes.

---

## 8. Diamond layers — référence

```
L0    nika-core            — AST, types, catalogs — ZERO I/O
      nika-event           — EventLog, EventKind
L0.5  nika-kernel          — Traits: Provider, FsRead/FsWrite, HttpClient,
                             ShellExecutor, BlobStore, Clock, PolicyChecker,
                             BuiltinRouter (S13), McpPool (S13)
      nika-kernel-mock     — Mocks: MockShell, MockHttpClient, MockBlobStore,
                             MockClock, MockPolicyChecker, MockProvider (W14-B1)
L1    nika-clock, nika-fs, nika-blob, nika-http, nika-exec-runner,
      nika-policy, nika-extract, nika-lsp-core
L2    nika-verb-exec    ✅ S13  (run() via ShellExecutor)
      nika-verb-fetch   ✅ S13  (run() via HttpClient, minimal)
      nika-verb-invoke  ✅ S13  (run() builtin path — MCP = NoopMcpPool)
      nika-verb-infer   ✅ W14-B1  (run() via Provider, 9 tests)
      nika-verb-agent   ❌ S15/S16
      nika-engine       ⚠️  ~146k LOC — cible ≤100k via Constellation
      nika-builtin, nika-media, nika-mcp, nika-display, nika-storage, nika-vault
L3    nika-runtime  ✅ S13  (VerbCapabilities + dispatch() dead code)
      nika-daemon
L4    nika-cli, nika-tui, nika-serve, nika-lsp, nika-sdk, nika-init
L5    nika  (~118 MB binary)
```

---

## 9. Invariants sacrés (1-19)

**De S12 (Fondation):**
1. `parking_lot::RwLockReadGuard` est `!Send` — JAMAIS tenu à travers `.await`. Clone out first.
   BUG-P0 en est la preuve concrète — fixer en commit 0a.
2. Chaque `tokio::process::Command` → `cmd.kill_on_drop(true)` avant spawn.
3. Pipe concurrent → `tokio::try_join!`. JAMAIS séquentiel.
4. Subprocess → test régression >1 MB output. Obligatoire.
5. Oracle golden = lifecycle ET output. Jamais affaiblir (G2 lesson).
6. `cargo check --no-default-features` obligatoire pour crates feature-gated (G3).
7. PolicyEnforcer: `let clone = self.policy_enforcer.read().clone();` → guard drop avant async.
8. Verb crate → min 4 tests: happy, error, cancel, event emission.
9. Caps structs → `new()` constructors (`#[non_exhaustive]` bloque struct init externe).
10. `pre_validated: true` sur ShellCommand → UNIQUEMENT depuis bridge engine.

**De S13:**
11. Shim types = dette technique. Documenter en TEMP avec date de résolution.
12. Fetch = verbe le plus dur. FetchAux reqwest-spécifique. Beta strategy validée.
13. Error format via `From` impl au boundary. Tester les strings.

**De S14 (review 6 agents):**
14. LOC estimates conservatives. Calculer blocs irréductibles AVANT de promettre une réduction.
    infer.rs bridge: 2157 → ~1980 (8% seulement). fetch.rs: 1401 → ~1060.
15. Cancellation obligatoire sur TOUT appel IO externe (MCP, HTTP, subprocess, LLM) via
    `tokio::select! { biased; _ = caps.cancel.cancelled() => ... }`.
    Test pre-cancelled obligatoire dans chaque verb crate.
16. Wave C gate: avant nika-verb-agent, compter les TEMP engine deps. Si >5 → S15.
17. **INTERDIT:** `infer_vision`, `infer_with_tools`, méthodes séparées sur Provider trait.
    Encoder via `InferRequest` (ContentBlock::Image pour vision, `tools` field pour tools).
    Invariant confirmé par les agents S14. Violation = revert.
18. TOCTOU SSRF: le bridge conserve le post-redirect check. Ne jamais déléguer à verb crate.
19. Table imports AVANT code (W14-C0 pattern): pour tout fichier >1000 LOC à migrer,
    30 min de mapping = économie de 3h+ de debug.

---

## 10. Wave C — Table import surgery (pré-calculée pour S15)

Calculée par code-explorer. Prête pour S15. Ne pas utiliser en S14.

| Fichier | Dep bloquante | Niveau | Résolvable |
|---------|--------------|--------|-----------|
| mod.rs | `McpClient` (nika-engine) | TEMP | S15 McpPoolAdapter |
| mod.rs | `NikaMcpTool`, `AgentMediaStaging` (provider::rig) | TEMP | S15 provider migration |
| mod.rs | `LimitTracker` | TEMP | S15 |
| mod.rs | `DynamicSubmitTool` | TEMP | S15 |
| mod.rs | `SkillInjector` | TEMP | S15/S16 |
| chat.rs | `find_provider`, `KNOWN_PROVIDERS` | TEMP | S15 ProviderRegistry trait |
| chat.rs | `has_provider_key` (secrets) | TEMP | S15 nika-vault |
| chat.rs | `ProviderKind` (cost) | TEMP | S15 |
| streaming.rs | `STREAM_CHUNK_TIMEOUT` | TEMP | S15 kernel constant |
| streaming.rs | `StreamChunk` (TUI type) | TEMP | S15 provider migration |
| streaming.rs | `CanaryMatchType` (Shield) | NEEDS_RUNTIME | S15+ |
| thinking.rs | `COMPLETION_MARKER`, `parse_completion_response` | TEMP | S15 |
| providers.rs | `RigProvider` direct (appel `client.completion_model()`) | TEMP | S16 |
| types.rs | aucune dep engine | MOVES_WITH_FILE | Prêt maintenant |

**Verdict: 13 TEMP deps sur 7 fichiers → Wave C = S15/S16.**

---

## 11. Décisions closes (ne pas re-débattre)

| Décision | Justification |
|----------|--------------|
| `fetch.rs` beta strategy (FetchAuxBundle concret) | Alpha (4 traits kernel) = S15. Confirmé. |
| dispatch() dead code (AMEND-4) | Activation = S15 après TaskExecutor dissolution. |
| Runner dans nika-engine | Migration = S15 (bloqué sur TaskExecutor). |
| W14-B2 (infer.rs bridge) = S15 | 8% LOC reduction, ~1980 LOC resteraient. Pas rentable. |
| Wave C = S15 | 10+ TEMP deps irrésolvables. Table Section 10. |
| `infer_vision` / `with_tools` PAS sur trait | Violation invariant #17. InferRequest suffit. |
| PolicyEnforcer pattern = `.read().clone()` | Pas Arc<RwLock<>> avec interior mutability. |

---

## 12. Done criteria S14

**Minimum (launch-safe):**
- [ ] BUG-P0 et BUG-P1 fixés et testés
- [ ] W14-A0: `InferEvent::Done { stop_reason, request_id, finish_reason_raw }` (rien d'autre)
- [ ] W14-A1: fetch retry migré, fetch.rs ~1060 LOC, test SSRF redirect
- [ ] W14-B0: McpPool real impl, cancel wrapper, NoopMcpPool supprimé
- [ ] W14-B4: resource reads via McpPool
- [ ] `cargo test --workspace --lib` passe (cible ~10,950+ tests)
- [ ] Clippy 0 warnings
- [ ] `cargo check --no-default-features` passe (G3)

**Full S14:**
- [ ] Tout le minimum
- [ ] W14-D0: tests executor non-agent migrés vers verb crates
- [ ] W14-E0: NullBlobStore + NullHttpClient supprimés
- [ ] W14-PRE-S15: golden oracle ProviderResponded fields capturés
- [ ] W14-E1: ARCHITECTURE.md mis à jour
- [ ] Engine LOC ≤ 143,000 (fetch retry migration)
- [ ] Crate count: 34

**S15 uniquement (ne pas tenter en S14):**
- infer.rs bridge surgery (W14-B2)
- nika-verb-agent (Wave C)
- TaskExecutor dissolution (Wave D5)
- dispatch() activation (Wave E)

---

## 13. Ne pas toucher

Fichiers launch-prep Thibaut (parallèles au refactor):
```
AGENTS.md, CLA.md, COMMERCIAL_LICENSE.md, CHANGELOG.md, README.md
MANIFESTO.md, CONTRIBUTING.md, .github/SECURITY.md, editors/**
docs/launch/, docs/story-april-2026/
```

---

## 14. Commit format

```
type(scope): description concise (W14-XN)

Co-Authored-By: Nika 🦋 <nika@supernovae.studio>
```

Types: `feat`, `fix`, `refactor`, `docs`, `test`, `chore`
Scopes: `kernel`, `verb-infer`, `verb-invoke`, `verb-fetch`, `runtime`, `engine`

---

**Fin du mega-prompt S14 v3 — post-review 6 agents.**
*HEAD: a3e8d8ab8 (+ commits W14-B1/B3/BUG1/BUG2 déjà landés). Crates: 33. Tests: ~10,840.*
*Wave C/D/E = S15. Plan B activé. InferEvent::Done = seule modification kernel S14.*
