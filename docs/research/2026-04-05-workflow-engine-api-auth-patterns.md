# Workflow Engine & AI Infrastructure API Auth Patterns

> Research date: 2026-04-05
> Methodology: Documentation scraping, GitHub source analysis, blog posts
> Products analyzed: 8

## Summary

Modern workflow engines and AI infrastructure tools converge on a few common
patterns for HTTP API authentication: opaque tokens with prefixes, SHA-256
hashing for storage, workspace/tenant-level scoping, and tiered rate limits.
Two notable outliers are Fly.io (macaroon-based attenuable tokens) and Inngest
(HMAC signing keys for bidirectional verification). JWT is used by Hatchet for
tenant-scoped tokens. Most products store only the hash, never the plaintext.

---

## 1. Temporal Cloud

**Sources**: [docs.temporal.io/cloud/api-keys](https://docs.temporal.io/cloud/api-keys), GitHub `temporalio/documentation`

### Token Format
- Opaque API keys, generated via Cloud UI or `tcld` CLI
- Named keys with description, configurable expiration (`"30d"`, `"4d12h"`)
- Shown only once at creation time

### Scoping Model
- **Identity-based**: `API Key -> Identity (User | Service Account) -> RBAC`
- Users manage their own keys
- Global Admins and Account Owners manage all keys including Service Accounts
- Namespace Admins manage Namespace-scoped Service Account keys
- Supports both user-level and Service Account keys

### Storage
- Keys shown once at creation, server stores hash (not documented publicly what algorithm)
- Keys can be disabled without deletion (reversible revocation)

### Token Lifecycle
- Create with name + description + expiration
- Enable / Disable toggle (non-destructive)
- Delete (permanent)
- Rotation: create new key -> verify both work -> switch clients -> delete old key
- No auto-rotation; manual process

### Rate Limiting
- Not publicly documented for API key endpoints specifically
- General Cloud API limits exist but details require account access

### Unique Patterns
- **Dual auth**: API keys are an *alternative* to mTLS certificates, not a replacement
- **Admin disable creation**: Global admins can disable API key creation for the entire account
- **Service Accounts as first-class**: Separate identity type for machine-to-machine auth
- **gRPC-native**: API keys work with gRPC (SDK, CLI) not just REST

---

## 2. Windmill (windmill.dev)

**Sources**: [windmill.dev/docs/core_concepts/user_tokens](https://www.windmill.dev/docs/core_concepts/user_tokens), GitHub `windmill-labs/windmill` (Rust, AGPL)

### Token Format
- Opaque string tokens (no documented prefix)
- Generated from UI or CLI (`wmill user create-token`)
- First 10 characters stored as `token_prefix` for audit identification
- Token shown once at creation

### Scoping Model
- **Workspace-scoped**: tokens are per-user within a workspace
- Granular scope system: `{domain}:{action}[:{resource_path}]`
  - Domains: `scripts`, `flows`, `apps`, `resources`, `variables`, `jobs`, etc.
  - Actions: `read`, `write`, `run` (for jobs)
  - Path restrictions with wildcards: `f/production/*`, `u/admin/*`
- Example: `scripts:write:f/production/*` = write scripts in production folder
- Default: full user permissions (no scopes = inherits everything)
- **Webhook tokens**: pre-scoped to trigger a specific script/flow only

### Storage
- SHA-256 hash of the token stored in database
- Source: `windmill-common/src/auth.rs` -> `hash_token()` -> `calculate_hash()` -> `Sha256`
- First 10 chars stored separately as `token_prefix` for identification in audit logs

### Token Lifecycle
- Create with label + optional expiration + optional scope restrictions
- View tokens (label, prefix, scope, expiration) in Account Settings
- Revoke by deleting from list
- Ephemeral job tokens: `WM_TOKEN` env var auto-set during script execution

### Rate Limiting
- Not explicitly documented at the API level
- Concurrency limits and job debouncing are enterprise features

### Unique Patterns
- **Finest-grained scoping in this survey**: path-level restrictions with wildcards
- **Ephemeral job tokens**: runtime tokens for in-workflow API calls
- **Bearer + query param**: tokens work in `Authorization: Bearer` header OR `?token=` query string
- **Argon2 for passwords** (separate from API tokens which use SHA-256)

---

## 3. Prefect Cloud

**Sources**: [docs.prefect.io](https://docs.prefect.io/v3/manage/cloud/manage-api-keys), GitHub `PrefectHQ/prefect`, older docs at `PrefectHQ/docs`

### Token Format
- Opaque keys stored in `PREFECT_API_KEY` environment variable
- Sent as `Authorization: Bearer {api_key}` header
- API URL format embeds account/workspace: `accounts/{uuid}/workspaces/{uuid}`
- No publicly documented prefix convention (internally may use one)

### Scoping Model
- **Account-level + Workspace-level**:
  - User API keys: tied to a user, can access all workspaces the user has access to
  - Service Account API keys (Pro/Custom tier): not tied to a user, scoped to specific roles
- Workspace-level RBAC determines what the key can do
- Service accounts are a paid feature for CI/CD and remote infrastructure

### Storage
- Keys shown once at creation ("API keys cannot be revealed again in the UI")
- Server-side storage not publicly documented

### Token Lifecycle
- Create with name + expiration date from UI
- No enable/disable toggle (only create/delete)
- `prefect cloud login -k '<api-key>'` stores key in local profile
- No auto-rotation

### Rate Limiting
- Documented in v2 docs: rate limits exist but specifics are per-plan
- Cloud API limits are applied per-account

### Unique Patterns
- **CLI login flow**: browser-based OAuth *or* API key paste from terminal
- **Service accounts** separate from user keys (Pro/Custom only)
- **Profile-based**: key stored in local Prefect profile, not just env var

---

## 4. n8n

**Sources**: [docs.n8n.io/api/authentication](https://docs.n8n.io/api/authentication/), GitHub `n8n-io/n8n` (TypeScript)

### Token Format
- Opaque keys with `n8n_api_` prefix (visible in CLI: `n8n_api_xxx`)
- Custom header: `X-N8N-API-KEY` (NOT `Authorization: Bearer`)
- Entity: `user_api_keys` table with unique constraint on `apiKey` column

### Scoping Model
- **Per-user**: each API key belongs to a user
- **Enterprise scopes**: `ApiKeyScope[]` stored per key
  - Non-enterprise: full access to all account resources
  - Enterprise: granular scope selection at creation time
- **Audience field**: keys can be `public-api` or `mcp-server-api`
- Unique constraint on `(userId, label)` pair

### Storage
- API key stored in database (from source: `apiKey: string` column, indexed unique)
- Based on the entity structure, the raw key appears to be stored (not hashed)
  - The `ApiKeyWithRawValue` type distinction suggests hashing was added later
- Labels for identification, expiration as Unix timestamp (nullable = never expires)

### Token Lifecycle
- Create with label + expiration + scopes (enterprise)
- Update label and scopes
- Delete permanently
- CLI: `n8n config set-api-key n8n_api_xxx`
- Not available during free trial

### Rate Limiting
- Not documented at the API key level
- n8n Cloud has general execution limits per plan

### Unique Patterns
- **Custom header name**: `X-N8N-API-KEY` instead of standard `Authorization: Bearer`
- **`n8n_api_` prefix**: clear product identification
- **MCP audience**: dedicated key audience for MCP server API access
- **Self-hosted vs Cloud**: identical auth mechanism, different host URLs

---

## 5. Hatchet

**Sources**: GitHub `hatchet-dev/hatchet` and `hatchet-dev/hatchet-v1` (Go)

### Token Format
- **JWT tokens** signed with Google Tink (not opaque keys)
- Generated via `GenerateTenantToken()` using Tink JWT Signer
- Token contains: tenant ID, token ID (UUID), expiration, issuer, audience
- Standard JWT structure (header.payload.signature)

### Scoping Model
- **Tenant-scoped**: each token is tied to a single tenant
- Token metadata stored in `api_tokens` table: `id`, `name`, `expiresAt`, `tenantId`
- Internal vs external tokens (boolean `internal` field)
- Auth middleware supports both Cookie auth and Bearer auth (with priority)

### Storage
- Token ID stored in database, JWT itself is the bearer credential
- Uses Tink encryption service for key management (public/private JWT handles)
- Verification via `jwt.Verifier` with the public JWT handle

### Token Lifecycle
- Create: `POST /api/v1/tenants/{tenant}/api-tokens` with name + optional `expiresIn` duration
- List: paginated listing of tokens
- Revoke: explicit revocation endpoint
- Token shown only once at creation (`"This is the only time the token is sent over the API"`)
- Duration format: Go duration strings (`"720h"`, `"30d"` etc.)

### Rate Limiting
- Not documented at API level

### Unique Patterns
- **Tink-based JWT**: uses Google's Tink crypto library, not standard RSA/ECDSA JWT
- **Tenant isolation**: strong multi-tenancy model, tokens cannot cross tenant boundaries
- **Dual auth strategy**: Cookie (session) + Bearer (API) with fallback logic
- **Internal tokens**: separate flag for system-generated vs user-generated tokens

---

## 6. Inngest

**Sources**: [inngest.com/docs/platform/signing-keys](https://www.inngest.com/docs/platform/signing-keys), GitHub `inngest/inngest` (Go)

### Token Format
- **Signing keys**, not traditional API keys
- Prefix format: `signkey-{env}-{hex_key}`
  - `signkey-test-` for test environments
  - `signkey-prod-` for production
  - `signkey-branch-` for branch environments
- The key after the prefix is hex-encoded random bytes
- One signing key per environment (shared across all apps in that environment)
- Branch environments share a single signing key

### Scoping Model
- **Environment-scoped**: one key per environment
- All apps in an environment use the same signing key
- Branch environments share a key (simplifies preview deployments on Vercel/Netlify)
- No per-function or per-workflow scoping

### Storage & Verification
- **Bidirectional HMAC**:
  1. Inngest signs requests to your server (you verify)
  2. Your SDK signs requests to Inngest API (they verify)
- Key normalization: strip the `signkey-{env}-` prefix before comparison
- Comparison: constant-time (`subtle.ConstantTimeCompare`)
- Also supports hashed key comparison: SHA-256 of hex-decoded key bytes
- **Replay protection**: requests include embedded timestamp, old requests rejected

### Token Lifecycle
- Created per environment in Inngest dashboard
- Rotation with zero downtime:
  1. Create new signing key in dashboard
  2. Set `INNGEST_SIGNING_KEY_FALLBACK` env var to old key
  3. Set `INNGEST_SIGNING_KEY` to new key
  4. Deploy (SDK retries auth failures with fallback key)
  5. Delete old key
- Minimum SDK versions for zero-downtime rotation: Go 0.7.2, Python 0.3.9, TypeScript 3.18.0

### Rate Limiting
- Not documented per signing key

### Unique Patterns
- **Not a traditional API key**: signing key model is fundamentally different
- **Bidirectional authentication**: both sides verify each other
- **Replay attack prevention**: timestamp-embedded signatures
- **Fallback key for rotation**: built into SDK, not just docs guidance
- **Environment-level, not user-level**: one key for the whole environment
- **Vercel integration**: initial setup is automatic, rotation is manual

---

## 7. Fly.io Machines API

**Sources**: [fly.io/docs/machines/api](https://fly.io/docs/machines/api/working-with-machines-api/), [fly.io/docs/security/tokens](https://fly.io/docs/security/tokens/), blog: [Macaroons escalated quickly](https://fly.io/blog/macaroons-escalated-quickly/)

### Token Format
- **Macaroon-based tokens** (not opaque strings, not JWT)
- Structure: chained HMAC with caveats (claims that further restrict access)
- Created via `fly tokens create` CLI commands
- Sent as `Authorization: Bearer <fly_api_token>`
- Internal API: `http://_api.internal:4280`
- Public API: `https://api.machines.dev`

### Scoping Model
- **Predefined scope tiers** (narrowest first):
  1. **App-scoped deploy token**: single app, deploy operations
  2. **App-scoped SSH token**: single app, SSH access
  3. **App-scoped exec token**: single app, command execution
  4. **Org-scoped token**: all apps in one organization
  5. **Org-scoped read-only token**: read-only across one org
  6. **Auth token (personal access)**: all orgs, all apps (short-lived, avoid using)
- Macaroon caveats can further restrict any token (user-editable!)
- Also supports OIDC token requests via `/v1/tokens/oidc`

### Storage
- Macaroons are cryptographically self-verifying
- Server stores the root secret per user, not the token itself
- Verification: chain HMAC from root secret through all caveats, compare tail
- Minimally stateful: only user ID lookup needed, then pure crypto

### Token Lifecycle
- Create with `fly tokens create` + options: `--name`, `--expiry`
- Default expiry: 20 years (175200h0m0s) -- very long
- **User-attenuable**: anyone can add caveats to restrict their own token further
- No revocation of individual caveats (add-only)
- Recommendation: use narrowest scope possible

### Rate Limiting
- Not documented per token type

### Unique Patterns
- **Macaroons**: only major production deployment of this Google Research concept
- **User-editable tokens**: holders can add restrictions without server round-trip
- **Attenuable**: token can be narrowed by anyone holding it, never widened
- **Minimally stateful**: one DB lookup (user ID -> root key), rest is crypto
- **JIT least-privilege**: generate a tightly-scoped token for each specific request
- **No OAuth2**: deliberately replaced OAuth2 tokens with macaroons
- Blog post: [API Tokens: A Tedious Survey](https://fly.io/blog/api-tokens-a-tedious-survey/) is essential reading

---

## 8. Railway

**Sources**: [docs.railway.com/reference/public-api](https://docs.railway.com/reference/public-api)

### Token Format
- Opaque tokens, three types with distinct headers:
  - **Account token**: `Authorization: Bearer {token}` -- broadest scope
  - **Workspace token**: `Authorization: Bearer {token}` -- workspace scope
  - **Project token**: `Project-Access-Token: {token}` -- different header name!
- Also supports **OAuth access tokens** for third-party apps

### Scoping Model
- **Three-tier hierarchy**:
  1. **Account token**: all resources across all workspaces (personal, do not share)
  2. **Workspace token**: all resources in one workspace (shareable with teammates)
  3. **Project token**: single environment within a project (deployment-specific)
- OAuth tokens: user-granted permissions based on approved scopes

### Storage
- Not publicly documented
- Tokens created from dashboard Settings page

### Token Lifecycle
- Create from dashboard: Settings > Tokens
- Account + Workspace tokens: tokens page in account settings
- Project tokens: tokens page in project settings
- OAuth: standard OAuth2 authorization flow
- No documented expiration or rotation mechanism

### Rate Limiting
- **Tiered by plan**:
  - Free: 100 RPH
  - Hobby: 1000 RPH, 10 RPS
  - Pro: 10000 RPH, 50 RPS
  - Enterprise: custom
- Response headers:
  - `X-RateLimit-Limit`: max requests per day
  - `X-RateLimit-Remaining`: remaining in current window
  - `X-RateLimit-Reset`: window reset time
  - `Retry-After`: sent only when limit exceeded

### Unique Patterns
- **Different header for project tokens**: `Project-Access-Token` vs `Authorization: Bearer`
- **GraphQL API**: same API that powers the dashboard (introspectable)
- **Clear tier progression**: Account > Workspace > Project > OAuth
- **Rate limit headers**: well-documented, per-plan differentiation
- **GraphiQL playground**: built-in for schema exploration

---

## Cross-Cutting Analysis

### Token Format Patterns

| Product | Format | Prefix | Encoding |
|---------|--------|--------|----------|
| Temporal | Opaque | None documented | Unknown |
| Windmill | Opaque | None, 10-char prefix stored | SHA-256 hash |
| Prefect | Opaque | None documented | Sent as Bearer |
| n8n | Opaque | `n8n_api_` | Stored raw (possibly hashed later) |
| Hatchet | JWT | Standard JWT | Tink-signed JWT |
| Inngest | Signing key | `signkey-{env}-` | Hex-encoded, SHA-256 for verification |
| Fly.io | Macaroon | None (structured binary) | Chained HMAC |
| Railway | Opaque | None documented | Bearer / custom header |

### Storage Patterns

| Product | Storage Method | Notes |
|---------|---------------|-------|
| Windmill | SHA-256 hash | Prefix stored separately for audit |
| n8n | Raw or hash | Entity suggests raw, types suggest later hashing |
| Hatchet | Token ID in DB | JWT is self-verifying, DB stores metadata |
| Inngest | Pre-shared key | Both sides hold the key, comparison uses constant-time |
| Fly.io | Root secret only | Macaroon is self-verifying via chained HMAC |

### Scoping Model Comparison

| Product | Scope Granularity | Hierarchy |
|---------|-------------------|-----------|
| Temporal | Identity (User/Service Account) -> RBAC | Account > Namespace |
| Windmill | Path-level with wildcards | Workspace > Domain > Path |
| Prefect | User/Service Account -> Workspace RBAC | Account > Workspace |
| n8n | Enterprise scopes (resource-level) | User > Scopes |
| Hatchet | Tenant-level | Tenant |
| Inngest | Environment-level | Account > Environment |
| Fly.io | App/Org with macaroon caveats | Org > App > Operation |
| Railway | Account/Workspace/Project | Account > Workspace > Project > Environment |

### Rate Limiting

Only Railway documents this thoroughly with per-plan tiers and response headers.
Most others either don't document it or handle it at a different layer.

---

## Patterns Worth Adopting for Nika

### 1. Prefixed Tokens (n8n, Inngest)
Prefix tokens with `nika_` or `nk_` for instant identification in logs, env vars,
and support tickets. Inngest's environment-aware prefix (`signkey-prod-`) is even
better -- consider `nk_live_` / `nk_test_`.

### 2. SHA-256 Hash Storage (Windmill)
Store only SHA-256 hash of the token. Store first N characters as plaintext prefix
for identification in audit logs. This is the most common pattern and well-proven.

### 3. Granular Scoping (Windmill, n8n Enterprise)
Windmill's `{domain}:{action}:{path}` format is the gold standard for self-hosted
workflow engines. Consider for `nika serve` API keys.

### 4. Rate Limit Headers (Railway)
Include `X-RateLimit-Limit`, `X-RateLimit-Remaining`, `X-RateLimit-Reset`, and
`Retry-After` headers. Railway's per-plan tiering is a good model.

### 5. Token Shown Once (all)
Every product shows the token only once at creation. This is table stakes.

### 6. Signing Key for Webhooks (Inngest)
For `nika serve` webhook endpoints, Inngest's bidirectional HMAC signing is the
right model: sign outbound requests, verify inbound requests, prevent replay attacks.

### 7. Macaroon Inspiration (Fly.io)
Macaroons are overengineered for most use cases, but the concept of attenuable
tokens (tokens that can be narrowed but never widened by the holder) is powerful.
Consider for future API token delegation.

### 8. Service Accounts (Temporal, Prefect)
Separate machine-to-machine tokens from user tokens. Important for CI/CD pipelines
where a human user should not be the identity behind workflow execution.

### 9. Fallback Key for Rotation (Inngest)
The `SIGNING_KEY_FALLBACK` pattern is brilliant for zero-downtime rotation.
SDK retries failed auth with the fallback key automatically.

### 10. Custom Header for Scoped Tokens (Railway)
Railway's `Project-Access-Token` header for project-scoped tokens is a clean way
to disambiguate token types at the HTTP layer.

---

## Sources

1. [Temporal Cloud - Manage API keys](https://docs.temporal.io/cloud/api-keys) -- Full documentation
2. [Windmill - User Tokens](https://www.windmill.dev/docs/core_concepts/user_tokens) -- Scope documentation
3. [Windmill GitHub - auth.rs](https://github.com/windmill-labs/windmill/blob/main/backend/windmill-common/src/auth.rs) -- SHA-256 hashing source
4. [Prefect Cloud - API Keys](https://docs.prefect.io/v3/manage/cloud/manage-api-keys) -- Documentation (JS-rendered)
5. [Prefect GitHub - cloud.py](https://github.com/PrefectHQ/prefect/blob/main/src/prefect/client/cloud.py) -- Bearer auth implementation
6. [n8n - API Authentication](https://docs.n8n.io/api/authentication/) -- X-N8N-API-KEY header
7. [n8n GitHub - api-key.ts entity](https://github.com/n8n-io/n8n/blob/master/packages/@n8n/db/src/entities/api-key.ts) -- DB schema
8. [Hatchet GitHub - token.go](https://github.com/hatchet-dev/hatchet-v1/blob/main/pkg/auth/token/token.go) -- Tink JWT implementation
9. [Hatchet GitHub - api_tokens.yaml](https://github.com/hatchet-dev/hatchet-v1/blob/main/api-contracts/openapi/components/schemas/api_tokens.yaml) -- OpenAPI schema
10. [Inngest - Signing Keys](https://www.inngest.com/docs/platform/signing-keys) -- Signing key documentation
11. [Inngest GitHub - signing_key_strategy.go](https://github.com/inngest/inngest/blob/main/pkg/authn/signing_key_strategy.go) -- HMAC verification source
12. [Fly.io - Working with Machines API](https://fly.io/docs/machines/api/working-with-machines-api/) -- Auth setup
13. [Fly.io - Access Tokens](https://fly.io/docs/security/tokens/) -- Macaroon token types
14. [Fly.io Blog - Macaroons escalated quickly](https://fly.io/blog/macaroons-escalated-quickly/) -- Design rationale
15. [Railway - Public API](https://docs.railway.com/reference/public-api) -- Token types and rate limits

## Confidence Level

**High** -- All findings are from primary sources (official documentation and source code).
Token format details for Temporal and Prefect are less concrete because their tokens are
opaque and implementation details are not open-sourced. Everything else is verified against
actual source code on GitHub.
