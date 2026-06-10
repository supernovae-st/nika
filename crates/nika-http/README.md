# nika-http

**Production `ReqwestHttp` — L1 implementation of the `nika-kernel` HTTP traits.**

The only production site touching `reqwest`. SSRF-defended by default
(three layers · see below), with response size caps and a manual
redirect loop. Pure crates (L0) and the kernel (L0.5) stay
network-free; tests inject a mock.

```rust,no_run
use nika_http::ReqwestHttp;
use nika_kernel::{FsRead, HttpGet, HttpRequest};

# async fn example() -> Result<(), nika_kernel::HttpError> {
let client = ReqwestHttp::new()?;          // SSRF enforced · 30s · 64 MiB cap
let resp = client.get(HttpRequest::get("https://api.anthropic.com/")).await?;
println!("{} · {} bytes", resp.status, resp.body.len());
# Ok(())
# }
```

## SSRF defense (on by default · three layers)

1. **Static** — scheme allow-list (http/https) · blocked hostnames
   (localhost · cloud metadata · trailing-dot-normalized) · literal-IP
   range checks (loopback · RFC1918 · link-local/metadata · CGN ·
   IPv6 local · v4-mapped v6).
2. **DNS-resolve** — non-literal hosts resolved via
   `tokio::net::lookup_host`; every resolved address range-checked
   (kills decimal-IP tricks + public-names-resolving-private).
3. **Per-hop re-check** — redirects followed manually (reqwest's own
   policy is `none`); layers 1+2 re-run on every hop, so a public host
   can not bounce the client into private space.

`SsrfMode::Disabled` is an explicit, auditable opt-out for tests and
trusted internal networks. The mechanism is policy-free; capability
gating (allow-lists, sandbox) is `nika-policy` (L1.5).

## Surface

| trait | methods | notes |
|---|---|---|
| `HttpGet` | `get` | cancel-safe (idempotent) |
| `HttpPost` | `post` · `send_streaming` | NOT cancel-safe (caller owns idempotency) · streaming body capped mid-stream |

`HttpConfig` knobs: `timeout` (30s) · `max_redirects` (5) ·
`max_response_bytes` (64 MiB) · `ssrf` (`Enforce`). Errors speak the
kernel `HttpError` (`SsrfBlocked` · `TooLarge` · `Timeout` ·
`Connection` · `Unsupported` · `Other`).

---

AGPL-3.0-or-later · SuperNovae Studio · 🦋
