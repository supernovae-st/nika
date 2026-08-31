# Crate spec — `nika-http`

| | |
|---|---|
| Status | **ADMITTED 2026-06-10** (`221c5d5a9`) · was **L1 admission target** (Phase-B slice step 5 · announce ladder per D-2026-06-10-N6 cascade) |
| Layer | L1 — effect crate · the only production site touching `reqwest` + `tokio::net::lookup_host` |
| Design | `ReqwestHttp` impl of the L0.5 `nika_kernel::http` traits via the `*Dyn` (`Send`) companions · SECURITY-SENSITIVE (SSRF) |
| LOC budget | well under the ≤1500/file + ≤15k/crate caps (enforced live by vectors 12+24) · doc + security-contract heavy · live count · `scripts/crate-metrics.sh nika-http` |
| Function cap | ≤100 lines each |
| Crate version | tracks workspace |
| License | `AGPL-3.0-or-later` |
| Edition | 2024 |
| Publish | `false` — internal L1 effect crate |
| NIKA codes | none — the kernel http contract speaks `HttpError` (kernel-side enum: `SsrfBlocked` · `HostNotAllowed` (declared `permits.net.http` escape · the builtin maps it to spec `NIKA-SEC-004`) · `TooLarge` · `Timeout` · `Connection` · `Unsupported` · `Other`) |

---

## 1. Purpose

`nika-http` is the **production HTTP effect**. It provides `ReqwestHttp`,
the real-network implementation of the L0.5 kernel traits (`HttpGet` ·
`HttpPost` — and therefore the blanket `HttpClient`) backed by
`reqwest` (rustls — no openssl), with **SSRF defense in the effect
crate itself**: workflows fetch attacker-influenced URLs, so the
mechanism layer must be safe even before `nika-policy` (L1.5) adds
capability gating on top.

## 2. Public API

```rust
pub struct ReqwestHttp { /* reqwest::Client + HttpConfig */ }

pub struct HttpConfig {           // builder-customizable knobs
    timeout: Duration,            // default 30s (per-request override wins)
    max_redirects: u8,            // default 5 (manual loop — see §3)
    max_response_bytes: u64,      // default 64 MiB (HttpError::TooLarge)
    ssrf: SsrfMode,               // default Enforce
}
pub enum SsrfMode { Enforce, Disabled }   // Disabled = tests/internal nets · explicit opt-out

impl ReqwestHttp {
    pub fn new() -> Result<Self, HttpError>;            // defaults · NO expect (TLS init can fail)
    pub fn with_config(HttpConfig) -> Result<Self, HttpError>;
}
impl HttpGetDyn  for ReqwestHttp { get }
impl HttpPostDyn for ReqwestHttp { post · send_streaming }
```

Implementation targets the `*Dyn` trait-variant companions (`Send`
futures · base traits + `HttpClient` umbrella via blanket — same
pattern as `nika-fs`/`nika-clock`).

## 3. Security design (the Diamond upgrades vs brouillon)

Brouillon shipped a string-only URL check with two `.expect()` and
documented its own hole («reqwest doesn't support per-request redirect
policy»). Diamond closes the class:

1. **Static check** (brouillon parity · `ssrf.rs` pure fns): scheme
   allow-list (http/https only) · blocked hostnames (localhost ·
   metadata endpoints) · literal-IP ranges — loopback · RFC1918 ·
   link-local 169.254/16 (cloud metadata) · CGN 100.64/10 ·
   unspecified/broadcast · IPv6 loopback/ULA fc00::/7/link-local
   fe80::/10 · IPv4-mapped/compatible v6 re-checked as v4.
2. **DNS-resolve check** (NEW): `tokio::net::lookup_host` resolves the
   host and EVERY resolved address is range-checked — kills
   `http://2130706433/` (decimal-IP), `http://localtest.me`-class
   public-names-resolving-private, and plain private-DNS rebinds at
   request time. Honest limit (documented): the TOCTOU window between
   our resolve-check and reqwest's own connect-time resolution is
   narrowed, not eliminated (per-request IP pinning lands with
   `nika-policy`; reqwest's `resolve()` is client-level).
3. **Per-hop redirect re-check** (NEW): reqwest redirect policy is
   `Policy::none()`; the crate follows redirects ITSELF — each hop
   re-runs static + DNS checks before the next request. `follow_redirects:
   false` returns the 3xx as-is. `max_redirects` exceeded →
   `HttpError::Other` (redirect loop). Relative `Location` resolved
   against the current URL (RFC 3986 join).
4. **Response size cap** (NEW · activates the kernel's `TooLarge`):
   non-streaming reads accumulate chunks and abort past
   `max_response_bytes`; streaming responses get a counting wrapper
   stream that yields `TooLarge` mid-stream. A `Content-Length` header
   above the cap fails fast before reading.
5. **Cross-origin credential stripping** (NEW): on a redirect that
   changes origin (scheme/host/port · RFC 6454), `Authorization` ·
   `Cookie` · `Cookie2` · `Proxy-Authorization` · `WWW-Authenticate`
   are dropped before the next hop — a PUBLIC host can not bounce a
   bearer token to a DIFFERENT host (SSRF only blocks PRIVATE targets ·
   this is the public→public leak class). Mirrors reqwest's own
   `remove_sensitive_headers`, which `Policy::none()` opted us out of.
6. **Followable-only redirect**: only `301/302/303/307/308` drive the
   loop; `300/304/305/306` return verbatim (304 is a normal conditional-
   GET answer, not an error). 303→GET demotes every method EXCEPT HEAD
   (RFC 9110 §15.4.4); GET-demotion also drops `Content-*` headers.
7. **Bounded DNS + size cap**: the per-hop `lookup_host` is wrapped in a
   5s timeout (a hostile resolver can not hang the op); the response
   size cap activates the kernel's `TooLarge` (fast-fail on
   `Content-Length`, mid-read for chunked, mid-stream for streaming).
8. **Zero `.expect()`**: `new()` returns `Result` (TLS backend init is
   fallible). Mid-stream reqwest errors keep their Timeout/Connection
   identity (not collapsed to `Other`).

POST cancel-safety follows the kernel contract verbatim (NOT
cancel-safe at the application layer — idempotency is the caller's
concern). GET is cancel-safe (idempotent by spec).

**Timeout scope (documented)**: `timeout` is PER-HOP, so a redirect
chain's worst case is `(max_redirects + 1) × timeout + DNS`. Per-hop is
the conservative choice (each network operation gets its full budget);
a single total-deadline is a future ratchet if a consumer needs it.

**Residual TOCTOU (documented)**: the DNS-resolve check and reqwest's
connect-time re-resolution are two lookups — a DNS-rebind between them
defeats layer 2. Narrowed, not eliminated; per-request IP pinning is a
`nika-policy` (L1.5) ratchet.

## 4. The 12 gates

| Gate | Status | Evidence |
|---|---|---|
| 1 SPEC | ✅ | this file |
| 2 TDD | ✅ | `tests/http_contract.rs` + ssrf unit/property authored first · RED (todo! skeleton) → GREEN · 36 contract + 27 lib/unit (incl 4 proptest) |
| 3 IMPL | ✅ | ~2278 LOC src (post permits.net.http runtime boundary + resolver-enforced SSRF + cred-strip + loopback e2e · live · `scripts/crate-metrics.sh nika-http`) · zero unwrap/expect in src |
| 4 CLIPPY 0 | ✅ | `cargo clippy --workspace --all-targets -- -D warnings` GREEN |
| 5 MUTATION ≥90% | ✅ | `cargo mutants -p nika-http` · **92 mutants · 72 caught / 72 viable = 100%** (20 unviable). Every pre-review survivor (303/301/307/308 demotion branches · masked v6 embedded-v4 arm · cap boundary · resolve-layer) killed by the post-review tests: `classify_resolved` unit set · demotion method-recording fixtures · exact-cap boundary · trailing-dot + v6 proptest |
| 6 PROPERTY | ✅ | MANDATORY (security): private-v4-range→SsrfBlocked (256 cases) · public-v4→allowed · v4↔mapped-v6 verdict equivalence · foreign-scheme→blocked |
| 7 BENCH | N/A | network-bound effect crate, no algorithmic hot path (justified) |
| 8 DOCS | ✅ | `RUSTDOCFLAGS=-D warnings cargo doc --no-deps` 0 warnings · private-item rustdoc clean (vector 28) · per-method CANCEL SAFETY + §3 |
| 9 CANARY | N/A | L1 effect, no `.nika.yaml` surface until L2 verbs (justified — clock/fs precedent) |
| 10 PARITY | ✅ | all brouillon ssrf vectors re-asserted (localhost · RFC1918 · CGN · v4-mapped · metadata · file:// · public-allowed) · Diamond ADDS DNS-resolve-check + per-hop re-check + size cap + streaming + redirect demotion |
| 11 REVIEW | ✅ | 3-agent swarm 2026-06-10 · 0 P0/P1 · P2 fixed same-session (3 dead BLOCKED_HOSTNAMES entries removed + trailing-dot normalize · README added · TLS-init + 303/301/307/308 demotion tests + v6 embedded-v4 collapse) |
| 12 ATOMIC | ✅ | 1 commit · Nika 🦋 trailer |

## 5. Consumers (downstream)

`nika-providers` (s8.5 · every cloud LLM adapter rides this client),
`nika-builtin` (s16 · the `nika:fetch` tool composes nika-http +
nika-extract + nika-policy SSRF), `nika-verb-infer`/`verb-agent` (L2 via
providers), `nika-mcp` (s18 · HTTP transports). The SSRF default-Enforce
posture is what lets `nika:fetch` be safe-by-construction at the 1.0 launch.

## 6. Dependencies

| dep | why | layer-legal |
|---|---|---|
| `nika-kernel` (path) | trait + type + error contracts | L0.5 ← L1 ✓ |
| `reqwest` (workspace pin 0.12 · rustls-tls · NO default features) | the HTTP backend · promoted to `[workspace.dependencies]` this admission (was a local pin in catalog-verify — RUST_ENFORCEMENT §2 pin-once) | L1+ ✓ |
| `url` (workspace pin 2) | SSRF parse + redirect Location join (transitive via reqwest · pinned aligned) | ✓ |
| `tokio` (`net` feature) | `lookup_host` DNS-resolve check | L1+ ✓ |
| `bytes` · `futures-core` | kernel surface types (body · stream) | ✓ |
| dev: `proptest` · `tokio` rt | Gate 6 + local-socket fixtures | dev-only |

deny.toml wrappers extended: `tokio` += nika-http · `reqwest` += nika-http.
