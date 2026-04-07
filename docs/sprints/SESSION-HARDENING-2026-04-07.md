# Session Handoff — Post-Review Hardening Sprint

> Copy-paste the prompt block below into a new Claude Code session at `/Users/thibaut/dev/supernovae/nika`

---

## MEGA PROMPT

```
Tu travailles sur Nika, un workflow engine YAML pour l'IA.
Repo: /Users/thibaut/dev/supernovae/nika (submodule dans supernovae-hq)
Repo PUBLIC sur GitHub: supernovae-st/nika

=== ÉTAT ACTUEL ===

Version: v0.76.0
HEAD: c38a783cd
Tests: 10,426 GREEN (cargo test --workspace --lib depuis tools/)
LOC: 547K | Crates: 17 | Schema: nika/workflow@0.12
Launch: May 5, 2026 (28 jours)

Session précédente: 5 commits shipped
  - Auth L2 scope enforcement (14062bf)
  - Auth L3 RBAC roles (ef5021c)
  - Storage refactor — StorageInner enum (9fd040b)
  - PostgreSQL backend feature-gated (27e2445)
  - Storage URL config wiring (c38a783)

3 agents review ont trouvé 6 HIGH + 6 MEDIUM. Cette session fixe les 6 HIGH.

=== FIXES — 6 HIGH (1 fix = 1 commit, push après chaque) ===

--- H1: Role::parse fallback → Viewer (least privilege) ---
Fichier: tools/nika-storage/src/lib.rs:207
Avant: `_ => Self::Operator,`
Après: `_ => Self::Viewer,`
Test à updater: lib.rs:3024 — `assert_eq!(Role::parse("unknown"), Role::Operator)` → Role::Viewer
Commit: `fix(storage): Role::parse falls back to Viewer (least privilege)`

--- H2: Scope enforcement sur 6 endpoints manquants ---
Le scope check existe sur /v1/run et /v1/workflows/{name}/source.
Il MANQUE sur: get_status, list_jobs, list_workflows, cancel_job, artifacts, events.

DESIGN: Pour les endpoints job-level (get_status, cancel_job, artifacts, events),
le scope check nécessite de fetch le job d'abord pour connaître le workflow,
puis appeler `p.can_access(&job.workflow)`.

Pour les endpoints list-level (list_jobs, list_workflows), le scope filtre les résultats
plutôt que de rejeter la requête entière.

Fichiers à modifier:
  1. tools/nika-serve/src/routes/workflows.rs
     - get_status (line 233): Ajouter `principal: Option<Extension<Principal>>`
       → fetch job → check `p.can_access(&job.workflow)` → 403 si refusé
     - cancel_job (line 266): A déjà principal. Ajouter scope check APRÈS le RBAC check.
       → fetch job (line 279) → check `p.can_access(&job.workflow)` → 403
     - list_jobs (line 426): Ajouter `principal: Option<Extension<Principal>>`
       → after fetching jobs, filter: `jobs.retain(|j| p.can_access(&j.workflow))`
     - list_workflows (line 508): Ajouter `principal: Option<Extension<Principal>>`
       → after scanning dir, filter: `workflows.retain(|w| p.can_access(&w.name))`

  2. tools/nika-serve/src/routes/artifacts.rs
     - list_artifacts (line 54): Ajouter principal → fetch job → scope check
     - download_artifact (line 90): Ajouter principal → fetch job → scope check

  3. tools/nika-serve/src/events.rs
     - stream_events (line 209): Ajouter principal → fetch job → scope check

TDD: Écrire des tests d'intégration:
  - test_app_multikey("project-a/*", Operator) → GET /v1/status/{id} pour un job project-b → 403
  - test_app_multikey("project-a/*", Operator) → GET /v1/jobs → liste filtrée (pas de project-b)
  - test_app_multikey("project-a/*", Operator) → POST /v1/cancel/{id} pour job project-b → 403

Commit: `fix(serve): enforce scope on all endpoints — status, jobs, cancel, artifacts, events`

--- H3: LIMIT/OFFSET paramétrisé dans postgres.rs ---
Fichier: tools/nika-storage/src/postgres.rs
Méthode: list_jobs_filtered (line 314)
Lignes 341-345: format!(" ORDER BY created_at DESC LIMIT {limit} OFFSET {offset}")

Fix: Bind as $N parameters:
```rust
let limit = filter.limit.unwrap_or(100).min(1000);
let offset = filter.offset.unwrap_or(0);
params.push(limit.to_string());
let limit_idx = params.len();
params.push(offset.to_string());
let offset_idx = params.len();
sql.push_str(&format!(" ORDER BY created_at DESC LIMIT ${limit_idx} OFFSET ${offset_idx}"));
```
NOTE: Les params sont bindés en String puis castés. Vérifier que sqlx accepte la conversion
String → i64 pour LIMIT/OFFSET. Si non, il faut un Vec<Box<dyn Encode>> ou séparer les binds.
Alternative plus simple: garder les i64 clampés mais les binder directement avec .bind(limit).bind(offset)
après les params dynamiques. Il faudra peut-être restructurer la query construction.

Test: cargo check -p nika-storage --features postgres
Commit: `fix(storage): parameterize LIMIT/OFFSET in PostgreSQL list_jobs_filtered`

--- H4: UNIQUE(job_id, name) manquant sur job_artifacts ---
Fichier: tools/nika-storage/src/postgres.rs
Schema SCHEMA_SQL (lines 44-54): Ajouter après la table:
```sql
CREATE UNIQUE INDEX IF NOT EXISTS idx_artifacts_unique ON job_artifacts(job_id, name);
```
Ou ajouter `UNIQUE(job_id, name)` dans le CREATE TABLE.
Le ON CONFLICT (job_id, name) à line 444 requiert cette contrainte.

Test: cargo check -p nika-storage --features postgres
Commit: `fix(storage): add UNIQUE(job_id, name) constraint on job_artifacts`

--- H5: increment_retry atomique avec RETURNING ---
Fichier: tools/nika-storage/src/postgres.rs
Méthode: increment_retry (lines 279-294)

Avant: UPDATE puis SELECT séparés (race condition multi-instance)
Après:
```rust
pub async fn increment_retry(&self, id: &str) -> StorageResult<u32> {
    let row = query(
        "UPDATE jobs SET retry_count = retry_count + 1 WHERE id = $1 RETURNING retry_count"
    )
    .bind(id)
    .fetch_one(&self.pool)
    .await
    .map_err(|e| StorageError::Other(format!("increment_retry: {e}")))?;

    let count: i32 = row.get("retry_count");
    Ok(count as u32)
}
```

Test: cargo check -p nika-storage --features postgres
Commit: `fix(storage): atomic increment_retry with RETURNING clause`

--- H6: CI --all-features exclut postgres ---
Fichier: .github/workflows/ci.yml
Job: test-features (line ~152)
Avant: `cargo check --workspace --all-features`

Fix: Exclure le feature postgres car pas de service PG dans CI.
Option A (recommandée): Lister explicitement les features:
```yaml
run: cargo check -p nika-storage --no-default-features
     && cargo check -p nika-storage  # default features
     # postgres skipped — requires PG service
```
Option B: Ajouter un service PostgreSQL au CI (plus lourd, post-launch).
Option C: `cargo check --workspace --all-features 2>&1 || true` (dangereux, masque les erreurs).

Vérifier: Le job clippy ne passe PAS --all-features, donc pas impacté.
Commit: `fix(ci): exclude postgres feature from all-features check`

=== MEDIUM (à faire après les 6 HIGH si le temps le permet) ===

M1: Option<Principal> → Extension<Principal> sur les 5 handlers mutants
    (run_workflow, cancel_job, batch_run, get_workflow_source, reload_workflows)
    Quand Extension<T> est absent, axum retourne 500. C'est fail-closed mais
    le status code est mauvais (devrait être 401). Pragmatique: garder Option
    car le middleware est fiable. Ou ajouter un extracteur custom.
    Commit: `refactor(serve): use non-optional Principal extractor`

M2: Warning TLS sur PG connections
    postgres.rs:105-112 — log warning si sslmode absent du URL
    Commit: `fix(storage): warn when PostgreSQL URL lacks sslmode`

M3: Redact credentials dans PG error messages
    postgres.rs — wrap sqlx errors pour strip postgresql:// URLs
    Commit: `fix(storage): redact credentials from PostgreSQL error messages`

M4: add_artifacts transactionnel en PG
    postgres.rs:435-460 — wrap loop dans transaction
    Commit: `fix(storage): wrap add_artifacts in transaction for PostgreSQL`

M5: PG schema versioning
    Ajouter table schema_migrations + version check comme SQLite user_version
    Commit: `feat(storage): PostgreSQL schema migration versioning`

M6: CHANGELOG + README badges
    CHANGELOG.md — expand v0.76.0 section
    README.md — update badges (version, test count)
    Commit: `docs: update CHANGELOG v0.76.0 + README badges`

=== RÈGLES ABSOLUES ===

1. Tests: cargo test --workspace --lib depuis tools/ (ALWAYS --lib)
2. Commits: 1 fix = 1 commit. Co-author: UNIQUEMENT Nika 🦋 <nika@supernovae.studio>
3. TDD: test first → RED → implement → GREEN → verify → commit
4. Pre-commit: fmt + clippy doivent passer
5. Push après chaque commit
6. Pour H2 (scope enforcement): utiliser des agents en parallèle si besoin
   - 1 agent pour routes/workflows.rs (get_status, cancel_job, list_jobs, list_workflows)
   - 1 agent pour routes/artifacts.rs + events.rs
7. Vérifier après CHAQUE fix: cargo test --workspace --lib (10,426+ tests GREEN)

=== ORDRE D'EXÉCUTION ===

1. H1 (1 min) — Role::parse → Viewer. Le plus simple, le plus impactant.
2. H4 (2 min) — UNIQUE constraint. One-liner.
3. H5 (5 min) — RETURNING clause. One-liner.
4. H3 (15 min) — LIMIT/OFFSET paramétrisé. Petit refactor.
5. H6 (10 min) — CI fix. YAML edit.
6. H2 (1-2h) — Scope enforcement partout. Le plus gros. TDD. Agents parallèles.
7. MEDIUM (si temps) — M1-M6 dans l'ordre.

=== VÉRIFICATION FINALE ===

Après tous les HIGH:
  cargo test --workspace --lib        # 10,426+ GREEN
  cargo clippy --workspace -- -D warnings  # 0 warnings
  cargo check -p nika-storage --features postgres  # compile
  git log --oneline -10               # 6 commits propres
  git push                            # tout sur origin/main
```

---

## Review Findings Source

These fixes come from 3 parallel review agents run after shipping:
- **Code Reviewer** (spn-powers:code-reviewer): Found H1, H2, H3, M1, M8
- **Security Auditor** (spn-rust:rust-security): Found H2 gaps, H4, H5, M2, M3, M4, M5
- **Gap Analyst** (Explore): Found H6, M6, CI readiness issues

## Commit History (this sprint)

```
c38a783cd feat(serve): storage backend selection via NIKA_STORAGE_URL
27e244581 feat(storage): PostgreSQL backend behind postgres feature flag
9fd040beb refactor(storage): introduce StorageInner enum for backend dispatch
ef5021c2c feat(serve): auth L3 — RBAC with admin/operator/viewer roles
14062bf13 feat(serve): auth L2 — scope enforcement with glob matching
```
