# Remaining Hardening — M1 (DNS Rebinding) + M8 (unsafe set_var)

> **For Claude:** Handoff for two deferred fixes from the v0.58.1 serve hardening sprint.
> Working directory: `/Users/thibaut/dev/supernovae/nika`
> **Prerequisite:** All CRITICAL + HIGH + other MEDIUM fixes are done and pushed (2154 tests pass).

---

## M1 — DNS Rebinding in Webhook SSRF

### Problem

`validate_webhook_url()` in `tools/nika-serve/src/webhook.rs:102-162` validates the **hostname string** at server startup. But `notify()` at line 58 makes the actual HTTP request **minutes/hours later** when a job completes. DNS can change between validation and request.

**Attack:** Attacker sets `NIKA_WEBHOOK_URL=http://evil.attacker.com/hook`. At startup, DNS resolves to `1.2.3.4` (public) → validation passes. Later, DNS changes to `169.254.169.254` → webhook POST hits AWS metadata.

### Current Mitigations (partial)

- Redirects disabled (`Policy::none()`) — blocks 302→metadata
- IP literal checks cover `10.x`, `172.16-31.x`, `192.168.x`, `127.x`, `169.254.x`, `::1`, IPv6-mapped, link-local
- Userinfo and bracket bypasses fixed

### What's Missing

DNS resolution happens inside `reqwest::Client::post().send()` — we never see the resolved IP. Need to validate the IP **after** DNS resolution but **before** the connection is established.

### Fix Options

#### Option A: Pin resolved IP at startup (Recommended — simplest)

Resolve the hostname once at startup and store the IP. Use `reqwest::Client::builder().resolve()` to pin DNS:

```rust
// In WebhookConfig::from_env()
let parsed_url = url::Url::parse(&url)?;
let host = parsed_url.host_str()?;
let port = parsed_url.port_or_known_default()?;

// Resolve and validate
let addrs: Vec<std::net::SocketAddr> = tokio::net::lookup_host(format!("{host}:{port}")).await?;
for addr in &addrs {
    check_resolved_ip(addr.ip())?; // Reuse check_ipv4 logic
}

// Pin the first resolved IP
let pinned_client = reqwest::Client::builder()
    .resolve(host, addrs[0])
    .redirect(reqwest::redirect::Policy::none())
    .build()?;
```

**Pros:** Simple, deterministic. DNS lookup happens once.
**Cons:** If the webhook server changes IP (e.g. load balancer rotation), webhook delivery breaks until nika serve restarts. Requires `tokio` runtime at startup (needs `from_env()` to become async).

#### Option B: Custom DNS resolver with IP validation

Use reqwest's `hickory-dns` feature + custom resolver that validates each resolved IP before connecting:

```toml
# Cargo.toml
reqwest = { ..., features = ["hickory-dns"] }
```

```rust
// Custom resolver that rejects private IPs
// reqwest 0.13 doesn't expose resolver API directly — would need a wrapper
```

**Pros:** Validates IP on every request, handles DNS changes safely.
**Cons:** reqwest 0.13 with `rustls` doesn't expose the resolver builder. Would need `trust-dns` feature or manual DNS resolution.

#### Option C: Resolve + validate at request time (in notify())

Before calling `reqwest::Client::post()`, do a manual `tokio::net::lookup_host()` and validate:

```rust
// In notify() — before spawning the reqwest call
let parsed = url::Url::parse(&url)?;
let host = parsed.host_str().unwrap_or_default();
let port = parsed.port_or_known_default().unwrap_or(443);
let addrs = tokio::net::lookup_host(format!("{host}:{port}")).await?;
for addr in addrs {
    if is_private_ip(addr.ip()) {
        warn!("webhook DNS resolved to private IP, blocking");
        return;
    }
}
// Then proceed with reqwest call
```

**Pros:** Validates on every request, catches DNS rebinding.
**Cons:** TOCTOU still exists (DNS can change between lookup_host and reqwest connect), but window is ~milliseconds vs hours. Adds latency to every webhook.

### Recommendation

**Option A** for v0.59.0 (pin at startup). It's the simplest, covers the actual attack (DNS change over hours), and avoids adding deps. The downside (stale IP) is acceptable — webhook URLs don't change IP often, and `nika serve` restarts are cheap.

**Option C** as a follow-up for v0.60+ if multi-tenant deployment is needed.

### Files to Modify

| File | What |
|------|------|
| `tools/nika-serve/src/webhook.rs` | `from_env()` becomes async, resolve+validate DNS, store pinned client |
| `tools/nika-serve/src/lib.rs` | Call `from_env().await` in async context |
| `tools/nika-serve/src/state.rs` | No change (WebhookConfig already in AppState) |

### Tests

```rust
#[tokio::test]
async fn webhook_dns_to_private_ip_blocked() {
    // Requires DNS mock or /etc/hosts entry pointing to 127.0.0.1
    // Alternative: test check_resolved_ip() directly
}

#[test]
fn check_resolved_ip_blocks_private() {
    assert!(check_resolved_ip(IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1))).is_err());
    assert!(check_resolved_ip(IpAddr::V4(Ipv4Addr::new(8, 8, 8, 8))).is_ok());
}
```

### Verify

```bash
cargo test -p nika-serve --lib
cargo clippy -p nika-serve
```

---

## M8 — unsafe { std::env::set_var } While Tokio Threads Run

### Problem

Rust 1.66+ marks `std::env::set_var` as `unsafe` because concurrent `getenv`/`setenv` is undefined behavior on glibc. The codebase has **9 call sites** across 5 files:

| File | Line | Context | Risk |
|------|------|---------|------|
| `nika-cli/src/onboarding.rs` | 175 | After vault store, before connection test | **MEDIUM** — tokio runtime active |
| `nika-cli/src/provider.rs` | 445 | After daemon IPC set | **MEDIUM** — tokio runtime active |
| `nika-cli/src/provider.rs` | 507 | Before test_provider_connection | **MEDIUM** — tokio runtime active |
| `nika/src/main.rs` | 1234 | CLI entry, --no-interactive flag | **LOW** — early in dispatch |
| `nika-cli/src/verbs.rs` | 679 | restore_provider_env helper | **LOW** — behind ENV_LOCK mutex |
| `nika-cli/src/verbs.rs` | 780,815,851,870,918 | Test helpers | **NONE** — tests use `#[serial]` |
| `nika-cli/src/vault.rs` | 610 | Test helper | **NONE** — isolated |

### Why It Exists

The `set_var` calls serve one purpose: make API keys available to `RigProvider::from_name()` and friends that read env vars. After `nika keys set` stores a key in the vault, the current process needs the key in env for the connection test.

### Fix Plan

#### Phase 1: Replace `NIKA_NO_ONBOARDING` with AtomicBool (main.rs:1234)

Simplest fix — the env var is only read by `skip_onboarding()` in the same process.

```rust
// In nika-cli/src/onboarding.rs
use std::sync::atomic::{AtomicBool, Ordering};
static NO_ONBOARDING: AtomicBool = AtomicBool::new(false);

pub fn skip_onboarding() -> bool {
    NO_ONBOARDING.load(Ordering::Relaxed)
        || std::env::var("NIKA_NO_ONBOARDING")
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false)
}

pub fn set_no_onboarding() {
    NO_ONBOARDING.store(true, Ordering::Relaxed);
}
```

Then in `main.rs`: `cli::onboarding::set_no_onboarding()` instead of `unsafe { set_var(...) }`.

**Effort:** 10 minutes. Zero risk.

#### Phase 2: Construct RigProvider with key directly (onboarding.rs + provider.rs)

The connection test path (`test_provider_connection`) creates a `RigProvider` which reads the env var. Instead, construct the provider with the key directly:

```rust
// New method on RigProvider
pub fn from_name_with_key(name: &str, api_key: &str) -> Result<Self, NikaError> {
    // Same as from_name but uses the provided key instead of env
}

// Then in onboarding.rs:
let prov = RigProvider::from_name_with_key(&provider, &api_key)?;
match prov.infer("Say 'OK'", None, None).await { ... }
```

This requires changes to how rig-core clients are constructed. Currently:
- `openai::Client::from_env()` reads `OPENAI_API_KEY`
- `RigProvider::openai()` calls `Client::from_env()`

We'd need `Client::builder().api_key(key).build()` variants for each provider.

**Effort:** 1-2 hours. Touches `provider/rig/mod.rs` constructors.

#### Phase 3: Remove remaining set_var in production code

After Phase 2, the only `set_var` calls left are in test code (behind `#[serial]` + `ENV_LOCK`). These are acceptable — test isolation prevents UB.

The `restore_provider_env` in `verbs.rs:679` is also test-only (called from test functions). No production code path calls it.

### Files to Modify

| Phase | File | What |
|-------|------|------|
| 1 | `nika-cli/src/onboarding.rs` | Add `AtomicBool` + `set_no_onboarding()` |
| 1 | `nika/src/main.rs` | Replace `set_var` with `set_no_onboarding()` |
| 2 | `nika-engine/src/provider/rig/mod.rs` | Add `from_name_with_key()` constructor |
| 2 | `nika-cli/src/onboarding.rs` | Use `from_name_with_key()` for connection test |
| 2 | `nika-cli/src/provider.rs` | Use `from_name_with_key()` for connection test |

### Tests

```bash
cargo test --workspace --lib  # All 2154+ tests still pass
cargo clippy --workspace      # No new warnings
```

### Verify

After Phase 1+2, grep for remaining unsafe set_var:
```bash
grep -rn "unsafe.*set_var" tools/nika/src/ tools/nika-cli/src/ tools/nika-engine/src/
# Should only show test code (verbs.rs tests, vault.rs tests)
```

---

## Priority & Timing

| Fix | Effort | When | Blocker? |
|-----|--------|------|----------|
| M8 Phase 1 (AtomicBool) | 10 min | v0.58.2 | No |
| M8 Phase 2 (from_name_with_key) | 1-2 hr | v0.59.0 | No |
| M1 Option A (pin DNS at startup) | 30 min | v0.59.0 | No |
| M1 Option C (per-request validation) | 2 hr | v0.60+ | No |

Neither is a VPS deployment blocker — the current mitigations (IP validation + no redirects) cover all practical attacks. DNS rebinding requires the attacker to control both the webhook URL environment variable AND a DNS server, which implies they already have server access.
