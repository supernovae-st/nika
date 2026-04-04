# SDK + OpenAPI Handoff Plan

> **Date**: 2026-04-03
> **Author**: Thibaut + Claude
> **Status**: Ready for execution
> **Scope**: Fix all 15 gaps from code review, publish-ready SDK

## Context

We just completed a major rewrite of `@supernovae-st/nika-client` (the TypeScript SDK for `nika serve`) and added aide OpenAPI 3.1 auto-generation to the Rust server. Everything compiles, tests pass (78 SDK + 74 Rust), and CI is green on macOS/Linux.

This plan fixes ALL remaining gaps before `npm publish`.

### Repos

| Repo | Path | Current version |
|------|------|-----------------|
| nika (server) | `/Users/thibaut/dev/supernovae/nika` | v0.63.0 |
| nika-client (SDK) | `/Users/thibaut/dev/supernovae/nika-client` | v0.1.0 → align to v0.63.0 |

### Architecture

```
Rust types (#[derive(Serialize, JsonSchema)])
       │
aide 0.16 → OpenAPI 3.1 spec
       │  GET /v1/openapi.json (behind auth)
       ▼
openapi-typescript → src/generated/openapi.d.ts
       │
@supernovae-st/nika-client (manual wrapper + auto types)
```

### What already works

- SDK: namespace pattern (`nika.jobs.*`, `nika.workflows.*`), 6 error classes, custom fetch, logger, webhook HMAC, binary artifacts, AbortSignal, dual CJS/ESM, 78 tests
- Server: aide ApiRouter, JsonSchema on 5 typed structs, doc functions on all 10 routes, `GET /v1/openapi.json`, 74 tests
- Coverage script: `npm run check:coverage` detects SDK ↔ server drift
- Type generation: `npm run generate:types` fetches spec and generates TS types

---

## Fixes — 15 gaps, 4 phases

### Phase 1: SDK publish blockers (nika-client)

**1.1 Rewrite README.md completely**

The current README uses the OLD flat API (`nika.run()`) which doesn't exist anymore. Rewrite from scratch.

Must include:
- Install: `npm install @supernovae-st/nika-client`
- Quick start with namespace pattern: `nika.jobs.submit()`, `nika.jobs.run()`, `nika.jobs.stream()`
- Config table with ALL 9 options: url, token, timeout, retries, pollInterval, pollTimeout, pollBackoff, **fetch**, **logger**
- Methods table organized by namespace:
  - `nika.jobs.*`: submit, status, cancel, run, stream, artifacts, artifact, artifactJson, **artifactBinary**, runAndCollect
  - `nika.workflows.*`: list, reload, **source**
  - `nika.health()`
  - `Nika.verifyWebhook()` (static)
- ALL 6 error classes with correct hierarchy:
  ```
  NikaError (base — catch this for ALL SDK errors)
  ├── NikaAPIError (HTTP errors: status, body, requestId)
  ├── NikaConnectionError (network: DNS, TCP, abort)
  ├── NikaTimeoutError (request or poll timeout)
  └── NikaJobError (job failed: exitCode, job object)
      └── NikaJobCancelledError (job cancelled)
  ```
- SSE event types table (8 types)
- AbortSignal example (cancel a running poll)
- Custom fetch example (logging middleware)
- Webhook verification example
- License: **AGPL-3.0-or-later** (NOT MIT)

**1.2 Create LICENSE file**

Create `/Users/thibaut/dev/supernovae/nika-client/LICENSE` with the full AGPL-3.0-or-later text.
Add `"LICENSE"` to the `"files"` array in package.json.

**1.3 Create CHANGELOG.md**

```markdown
# Changelog

## 0.63.0 (2026-04-03)

Initial public release. Full rewrite from v0.1.0.

### Features
- Namespace pattern: `nika.jobs.*`, `nika.workflows.*`
- 6 typed error classes (all extend NikaError)
- Custom fetch injection for testing/middleware
- Logger interface (debug, info, warn, error)
- SSE streaming with 60s idle timeout
- Binary artifact download (Uint8Array)
- Parallel artifact collection in runAndCollect
- AbortSignal on run(), runAndCollect(), stream()
- Webhook HMAC-SHA256 verification (Stripe-style)
- Dual CJS/ESM build
- SDK coverage check script
- OpenAPI type generation script

### Breaking Changes
- API changed from flat to namespace pattern
- Error hierarchy completely redesigned
- License changed from MIT to AGPL-3.0-or-later
```

**1.4 Align version to v0.63.0**

In `package.json`, change `"version": "0.1.0"` → `"version": "0.63.0"`.
This aligns with the nika server version.

**1.5 Add package.json metadata**

```json
{
  "repository": {
    "type": "git",
    "url": "https://github.com/supernovae-studio/nika-client.git"
  },
  "homepage": "https://github.com/supernovae-studio/nika-client#readme",
  "author": "SuperNovae Studio <nika@supernovae.studio>",
  "keywords": ["nika", "workflow", "ai", "llm", "sdk", "typescript", "sse", "openapi"]
}
```

Also add `"LICENSE"` to the `"files"` array.

**1.6 Add test for `workflows.source()`**

In `test/client.test.ts`, add:
```typescript
describe('workflows.source()', () => {
  it('returns raw YAML as text', async () => {
    fetchSpy.mockResolvedValueOnce(textResponse('schema: "nika/workflow@0.12"\nworkflow: test'));
    const yaml = await client.workflows.source('test.nika.yaml');
    expect(yaml).toContain('nika/workflow@0.12');
    const [url] = fetchSpy.mock.calls[0];
    expect(url).toContain('/v1/workflows/test.nika.yaml/source');
  });

  it('encodes workflow name with slashes', async () => {
    fetchSpy.mockResolvedValueOnce(textResponse('schema: "nika/workflow@0.12"'));
    await client.workflows.source('sub/dir/flow.nika.yaml');
    const url = fetchSpy.mock.calls[0][0] as string;
    expect(url).toContain('sub%2Fdir%2Fflow.nika.yaml');
  });
});
```

In `test/e2e.test.ts`, add workflow source endpoint to the mock server and test it.

**Verification**: `npm test` → all pass. `npm run build` → CJS + ESM + .d.ts clean.

---

### Phase 2: OpenAPI spec completeness (nika server)

**2.1 Add security scheme to openapi.rs**

Replace the `configure()` function in `tools/nika-serve/src/openapi.rs`:

```rust
pub fn configure(api: TransformOpenApi) -> TransformOpenApi {
    api.title("Nika Serve API")
        .version(env!("CARGO_PKG_VERSION"))
        .description("HTTP API for the Nika workflow engine. Schema: nika/workflow@0.12")
        .server(aide::openapi::Server {
            url: "/".into(),
            description: Some("Current server".into()),
            ..Default::default()
        })
        .security_scheme(
            "bearerAuth",
            aide::openapi::SecurityScheme::Http {
                scheme: "bearer".into(),
                bearer_format: Some("token".into()),
                description: Some("NIKA_SERVE_TOKEN (minimum 32 characters)".into()),
                extensions: Default::default(),
            },
        )
        .security_requirement("bearerAuth")
        .license(aide::openapi::License {
            name: "AGPL-3.0-or-later".into(),
            url: Some("https://www.gnu.org/licenses/agpl-3.0.html".into()),
            ..Default::default()
        })
        .contact(aide::openapi::Contact {
            name: Some("SuperNovae Studio".into()),
            url: Some("https://supernovae.studio".into()),
            email: Some("nika@supernovae.studio".into()),
            ..Default::default()
        })
}
```

Note: Check the exact aide 0.16 API for these methods. The types are in `aide::openapi::*`. If `security_requirement` is not available on `TransformOpenApi`, add it per-route in the doc functions via `.security_requirement("bearerAuth")`.

**2.2 Add operation IDs to all doc functions**

In `tools/nika-serve/src/routes/workflows.rs`, add `.id()` to each doc function:

```rust
pub fn run_docs(op: TransformOperation) -> TransformOperation {
    op.id("submitWorkflow")
      .summary("Submit a workflow for async execution")
      .description("Validates the workflow file, creates a job in SQLite, and spawns a worker.")
      .tag("jobs")
}

pub fn status_docs(op: TransformOperation) -> TransformOperation {
    op.id("getJobStatus")
      .summary("Poll job status")
      .description("Returns the current state of a job.")
      .tag("jobs")
}

pub fn cancel_docs(op: TransformOperation) -> TransformOperation {
    op.id("cancelJob")
      .summary("Cancel a running job")
      .description("Kills the worker subprocess and marks the job as cancelled.")
      .tag("jobs")
}

pub fn list_docs(op: TransformOperation) -> TransformOperation {
    op.id("listWorkflows")
      .summary("List available workflows")
      .description("Recursively scans the workflows directory for .nika.yaml files.")
      .tag("workflows")
}

pub fn source_docs(op: TransformOperation) -> TransformOperation {
    op.id("getWorkflowSource")
      .summary("Get workflow YAML source")
      .description("Returns the raw YAML content of a workflow file as plain text.")
      .tag("workflows")
}

pub fn reload_docs(op: TransformOperation) -> TransformOperation {
    op.id("reloadWorkflows")
      .summary("Reload workflows from disk")
      .description("Rescans the workflows directory and returns the refreshed list.")
      .tag("workflows")
}
```

In `tools/nika-serve/src/routes/health.rs`:
```rust
pub fn docs(op: TransformOperation) -> TransformOperation {
    op.id("healthCheck")
      .summary("Health check")
      .description("Returns 200 with version info. No authentication required.")
      .tag("system")
}
```

In `tools/nika-serve/src/routes/artifacts.rs`:
```rust
pub fn list_docs(op: TransformOperation) -> TransformOperation {
    op.id("listArtifacts")
      .summary("List job artifacts")
      .description("Returns artifacts with name, size, format, content_type, checksum.")
      .tag("artifacts")
}

pub fn download_docs(op: TransformOperation) -> TransformOperation {
    op.id("downloadArtifact")
      .summary("Download a single artifact")
      .description("Streams the file with Content-Type, Content-Length, ETag, Cache-Control.")
      .tag("artifacts")
}
```

**2.3 Type the cancel and artifacts responses**

In `tools/nika-serve/src/routes/workflows.rs`, add:
```rust
#[derive(Serialize, JsonSchema)]
pub struct CancelResponse {
    pub job_id: String,
    /// Job status after cancellation: "cancelled" or the current terminal state.
    pub status: String,
    /// Present when the job was already finished before cancel was called.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}
```

Then change `cancel_job` to return `Result<Json<CancelResponse>, ServeError>` instead of `Result<Json<Value>, ServeError>`. Update the two return sites (already-finished and cancelled) to construct `CancelResponse`.

In `tools/nika-serve/src/routes/artifacts.rs`, add:
```rust
#[derive(Serialize, JsonSchema)]
pub struct ArtifactInfo {
    pub name: String,
    pub size: u64,
    pub format: String,
    pub content_type: String,
    pub checksum: Option<String>,
}

#[derive(Serialize, JsonSchema)]
pub struct ListArtifactsResponse {
    pub job_id: String,
    pub count: usize,
    pub artifacts: Vec<ArtifactInfo>,
}
```

Change `list_artifacts` to return `Result<Json<ListArtifactsResponse>, ServeError>`.

**2.4 Document SSE in the spec (description-based approach)**

SSE is fundamentally not RESTful. Instead of trying to force it into an api_route, add a comprehensive description to the `submitWorkflow` doc function:

```rust
pub fn run_docs(op: TransformOperation) -> TransformOperation {
    op.id("submitWorkflow")
      .summary("Submit a workflow for async execution")
      .description(
          "Validates the workflow file, creates a job in SQLite, and spawns a worker.\n\n\
           ## Real-time events\n\n\
           After submission, subscribe to SSE at `GET /v1/events/{job_id}` with the same Bearer token.\n\n\
           Event types: `started`, `task_start`, `task_complete`, `task_failed`, \
           `artifact_written`, `completed`, `failed`, `cancelled`.\n\n\
           Terminal events (`completed`, `failed`, `cancelled`) close the stream."
      )
      .tag("jobs")
}
```

**Verification**: `cargo test -p nika-serve --lib` → 74+ tests pass. `cargo clippy -p nika-serve -- -D warnings` → 0 warnings. `cargo fmt -p nika-serve` → no diff.

---

### Phase 3: Scalar interactive docs (nika server)

**3.1 Enable Scalar UI**

In `tools/Cargo.toml`, update aide features:
```toml
aide = { version = "=0.16.0-alpha.3", features = ["axum", "axum-json", "scalar"] }
```

In `tools/nika-serve/src/routes/mod.rs`, add the Scalar route AFTER `finish_api_with`:
```rust
use aide::scalar::Scalar;

// After .finish_api_with(...):
.route("/v1/docs", Scalar::new("/v1/openapi.json").axum_route())
```

Note: Check that `aide::scalar::Scalar` exists in aide 0.16-alpha.3. If not, use the `aide-scalar` companion crate or skip this for now.

The `/v1/docs` endpoint will be behind auth (same as `/v1/openapi.json`). This is correct for a private server.

**Verification**: Start `nika serve`, open `http://localhost:3000/v1/docs` with Bearer token → interactive API explorer loads.

---

### Phase 4: DX alignment (both repos)

**4.1 Update nika CLAUDE.md**

Add a section after "## Commands":
```markdown
## HTTP API (nika serve)

`nika serve` exposes a REST + SSE API. OpenAPI 3.1 spec auto-generated via aide.

- `GET /health` — Health check (no auth)
- `POST /v1/run` — Submit workflow
- `GET /v1/status/{id}` — Poll job status
- `POST /v1/cancel/{id}` — Cancel job
- `GET /v1/events/{id}` — SSE event stream
- `GET /v1/workflows` — List workflows
- `GET /v1/workflows/{name}/source` — Raw YAML source
- `POST /v1/reload` — Rescan workflows
- `GET /v1/jobs/{id}/artifacts` — List artifacts
- `GET /v1/jobs/{id}/artifacts/{name}` — Download artifact
- `GET /v1/openapi.json` — OpenAPI 3.1 spec
- `GET /v1/docs` — Scalar API explorer

Auth: Bearer token via `NIKA_SERVE_TOKEN` env var. SDK: `@supernovae-st/nika-client`.
```

**4.2 Update nika CHANGELOG.md**

Add entry for v0.63.1 (or whatever version gets tagged):
```markdown
## 0.63.1 (2026-04-03)

### Added
- **OpenAPI 3.1**: Auto-generated spec via aide at `GET /v1/openapi.json`
- **Scalar UI**: Interactive API explorer at `GET /v1/docs`
- **Security scheme**: Bearer auth documented in spec
- **Operation IDs**: All endpoints have stable IDs for SDK generation
- **Typed responses**: `CancelResponse`, `ListArtifactsResponse` (was `Json<Value>`)

### Fixed
- **LSP**: Windows CI type inference error in `daemon_status()` (E0282)
```

---

## Commit plan

| # | Repo | Commit message | Files |
|---|------|----------------|-------|
| 1 | nika-client | `docs: rewrite README for v2 namespace API` | README.md |
| 2 | nika-client | `chore: add LICENSE (AGPL-3.0-or-later), CHANGELOG, package metadata` | LICENSE, CHANGELOG.md, package.json |
| 3 | nika-client | `chore: align version to v0.63.0 (match nika server)` | package.json |
| 4 | nika-client | `test: add workflows.source() tests` | test/client.test.ts, test/e2e.test.ts |
| 5 | nika | `feat(serve): complete OpenAPI spec — security scheme, operation IDs, typed responses` | openapi.rs, workflows.rs, artifacts.rs, health.rs |
| 6 | nika | `feat(serve): add Scalar API docs UI at /v1/docs` | Cargo.toml, routes/mod.rs |
| 7 | nika | `docs: update CLAUDE.md and CHANGELOG for OpenAPI + SDK` | CLAUDE.md, CHANGELOG.md |

After all commits: `npm publish --access public` for the SDK.

---

## Verification checklist (run after ALL commits)

```bash
# nika server
cd ~/dev/supernovae/nika/tools
cargo test -p nika-serve --lib          # 74+ tests
cargo clippy -p nika-serve -- -D warnings  # 0 warnings
cargo fmt -p nika-serve --check         # no diff

# nika-client SDK
cd ~/dev/supernovae/nika-client
npm test                                 # 80+ tests (2 new)
npm run build                            # CJS + ESM + .d.ts
npm run check:coverage ~/dev/supernovae/nika  # PASS

# Cross-check
# Start nika serve locally, then:
NIKA_TOKEN=xxx npm run generate:types    # types generated from live spec
```
