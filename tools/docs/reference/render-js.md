# JavaScript rendering (`render: js`) — `nika-render-js`

> **Status** · v0 (0.83.0-dev) · verb-crate seam shipped · engine-bridge wire
> deferred (see [Wiring status](#wiring-status)).

Some pages return an empty HTML shell and build their real content with
client-side JavaScript (React / Vue / Next / Nuxt SPAs). A static `fetch:`
sees only the shell. `nika-render-js` renders such pages in a headless Chrome
and returns the **post-JS** HTML, behind the same `HttpClient` trait the rest
of the engine already uses.

## Architecture

```
nika_kernel::http::HttpClient  (trait · already exists)
   ├── nika-http::ReqwestClient      static HTML fetch        (default)
   └── nika-render-js::ChromiumClient headless JS render      (render: js)
```

`ChromiumClient` is a drop-in `HttpClient`. The fetch verb picks the transport
via `nika_verb_fetch::run_with_render(.., render_client, RenderBackend)`:

| `RenderBackend` | render client | transport used |
|---|---|---|
| `Default` | (any) | `caps.http` (static) |
| `ChromiumJs` | `Some(client)` | the render client |
| `ChromiumJs` | `None` | `caps.http` (graceful degrade) |

Because the render client implements the same trait, cancellation,
event-emission, and `nika-extract` post-processing are identical to a static
fetch — only the transport differs.

## v0 capabilities & limits

| Capability | v0 | Notes |
|---|---|---|
| HTTP method | `GET` only | non-GET → `HttpError::Unsupported` (`NIKA-CHRM-008`) |
| Headless | yes | `BrowserConfig::builder().build()` default |
| Concurrency | 1 page | `MAX_CONCURRENT_PAGES = 1` · semaphore-gated |
| Nav timeout | 30s | honors `HttpRequest.timeout` when set |
| Settle wait | 10s | `wait_for_navigation` · non-fatal on timeout |
| User-Agent | overridable | `RenderOptions.user_agent` → `Page::set_user_agent` |
| Chrome binary | **system-installed** | chromiumoxide auto-detects via PATH |
| Auto-download Chromium | ✗ (Round 2) | needs the `fetcher` + zip + TLS feature set |
| Cookies / POST / JS injection | ✗ (Round 2) | LOCK-031 trigger-gated |

## Lifecycle & cancellation

- Every render races a `CancellationToken`; a fired token returns
  `RenderError::Cancelled` (`NIKA-CHRM-007`) before any further `.await`.
- The browser tab is closed on **every** exit path (success / error / cancel)
  within a bounded 2s timeout — chromiumoxide 0.9.1 does not auto-close tabs on
  `Page` drop, so an un-closed tab leaks renderer memory.
- Tear the client down with the async `ChromiumClient::close()` (bounded
  cancel → `Browser::close` 5s → pump-task join 5s). `Drop` is a best-effort
  safety net only (it cannot `.await`); long-lived daemons MUST call `close()`.

## Error codes (`NIKA-CHRM-NNN`)

| Code | Variant | Transient |
|---|---|---|
| 001 | `Launch` | yes |
| 002 | `Config` | no |
| 003 | `NewPage` | yes |
| 004 | `Navigation` | yes |
| 005 | `NavTimeout` | yes |
| 006 | `Extract` | no |
| 007 | `Cancelled` | no |
| 008 | `UnsupportedMethod` | no |
| 009 | `SemaphoreClosed` | no |

`RenderError` maps onto `HttpError` (`NavTimeout`→`Timeout` ·
`UnsupportedMethod`→`Unsupported` · transient→`Connection` · else→`Other`),
embedding the `NIKA-CHRM-NNN` code so it stays grep-able through `dyn HttpClient`.

## Wiring status

| Layer | Status |
|---|---|
| `nika-render-js` crate (`ChromiumClient` + lifecycle) | ✅ shipped (0.83.0-dev) |
| `nika-verb-fetch` render dispatch seam (`run_with_render`) | ✅ shipped + tested (mock transport) |
| Engine bridge · parse `render: js` from YAML + construct `ChromiumClient` | ⏳ follow-up (needs `nika-engine` edits) |

Until the engine bridge wires it, the dispatch seam exists at the verb-crate
layer and is exercised by unit tests; a workflow `fetch: { render: js }` is not
yet honored end-to-end.

## Sovereignty

Rendering is 100% local (per `supernovae-alignment.md` Rule 1). The headless
Chrome runs on the operator's machine; no page content or rendering state leaves
the host. No cloud rendering service is contacted.

## Provenance

`chromiumoxide` is pinned `=0.9.1` (MIT/Apache-2.0). See
`tools/nika-render-js/NOTICE.md` for the full attribution + the phantom-feature
note (`tokio-runtime` does not exist in 0.9.1 · the runtime comes from the
caller).
