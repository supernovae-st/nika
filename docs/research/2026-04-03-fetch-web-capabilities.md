# Research Report: HTTP Caching, Cookie Jars, robots.txt, and Rate Limiting in Rust

**Date**: 2026-04-03
**Scope**: Implementation plan for web scraping capabilities in Nika's `fetch:` verb
**Crates investigated**: http-cache-reqwest, http-cache-semantics, cookie_store, reqwest_cookie_store, texting_robots, governor

---

## Summary

All four features (HTTP caching, cookie jars, robots.txt, per-domain rate limiting) have mature, production-tested Rust crates available. The key architectural decision for Nika is whether to use `http-cache-reqwest` as a reqwest-middleware layer (transparent, automatic) or build a lighter custom layer directly on `http-cache-semantics` (more control, less dependency). This report recommends a hybrid approach: custom cache layer with `http-cache-semantics` for logic, `cookie_store` + `reqwest_cookie_store` for cookies, `texting_robots` for robots.txt, and `governor` for rate limiting.

---

## 1. HTTP Caching (ETag / If-Modified-Since / 304)

### Crate Landscape

| Crate | Version | Role | Maturity |
|-------|---------|------|----------|
| `http-cache-reqwest` | 1.0.0-alpha.5 | reqwest-middleware with cache | Active, alpha |
| `http-cache` | 1.0.0-alpha.5 | Core caching logic + storage backends | Active, alpha |
| `http-cache-semantics` | 3.0.0 | RFC 7234 cache policy engine (port of npm http-cache-semantics) | Stable |
| `cacache` | (via http-cache) | Content-addressable disk cache | Mature |

### Option A: Full http-cache-reqwest middleware (REJECTED)

```rust
// Cargo.toml
http-cache-reqwest = { version = "1.0.0-alpha.5", features = ["manager-cacache"] }
reqwest-middleware = "0.4"

// Usage
use http_cache_reqwest::{Cache, CacheMode, CACacheManager, HttpCache, HttpCacheOptions};
use reqwest_middleware::ClientBuilder;

let client = ClientBuilder::new(reqwest::Client::new())
    .with(Cache(HttpCache {
        mode: CacheMode::Default,
        manager: CACacheManager::new(".nika/cache/http".into(), true),
        options: HttpCacheOptions::default(),
    }))
    .build();
```

**Why rejected:**
- Alpha status (1.0.0-alpha.5) -- API may change
- Replaces `reqwest::Client` with `reqwest_middleware::ClientWithMiddleware` -- affects all callsites
- Nika already builds custom per-request clients for SSRF pinning, redirect tracking, etc.
- `CACacheManager` uses cacache which has its own content-addressable structure -- conflicts with `.nika/cache/` conventions
- Brings in heavy dependency tree: cacache, postcard or bincode serialization, http-body-util

### Option B: Custom cache layer with http-cache-semantics (RECOMMENDED)

Use `http-cache-semantics` directly for RFC 7234 logic, with a simple `DashMap` + disk store.

#### Cargo.toml additions (nika-engine)

```toml
http-cache-semantics = { version = "3.0", features = ["serde"] }
# serde feature enables CachePolicy serialization for disk persistence
```

#### Core API of http-cache-semantics

```rust
use http_cache_semantics::{CachePolicy, CacheOptions, BeforeRequest, AfterResponse};

// 1. Create policy from request + response
let policy = CachePolicy::new(&request_parts, &response_parts);
// or with options:
let policy = CachePolicy::new_options(&req, &res, SystemTime::now(), CacheOptions {
    shared: false,       // single-user cache (private responses cacheable)
    cache_heuristic: 0.1,
    ..Default::default()
});

// 2. Check if response is storable
if policy.is_storable() {
    cache.put(url, response_body, policy).await;
}

// 3. Before a new request, check if cache can satisfy it
match policy.before_request(&new_request_parts, SystemTime::now()) {
    BeforeRequest::Fresh(parts) => {
        // Cache HIT -- return cached body, update headers from `parts`
    }
    BeforeRequest::Stale { request: conditional_parts, matches } => {
        // Need revalidation -- conditional_parts has If-None-Match / If-Modified-Since
        // Send request with these headers added
    }
}

// 4. After getting response to conditional request
match policy.after_response(&request_parts, &response_parts, SystemTime::now()) {
    AfterResponse::Modified(new_policy, parts) => {
        // Server sent new content (200), cache it
    }
    AfterResponse::NotModified(new_policy, parts) => {
        // 304 -- use cached body, update policy
    }
}
```

#### Storage Design for .nika/cache/

```rust
use dashmap::DashMap;
use std::path::PathBuf;

/// Per-URL cache entry stored on disk
#[derive(Serialize, Deserialize)]
struct CacheEntry {
    policy: CachePolicy,   // Serializable with serde feature
    body: Vec<u8>,         // Response body
    url: String,
    stored_at: SystemTime,
}

/// HTTP cache store using .nika/cache/http/ directory
pub struct NikaHttpCache {
    /// In-memory LRU of hot entries (policy only, not body)
    policies: DashMap<String, CachePolicy>,
    /// Base directory: .nika/cache/http/
    cache_dir: PathBuf,
    /// Max cache size in bytes (default: 100 MB)
    max_size: u64,
}

impl NikaHttpCache {
    pub fn new(project_root: &Path) -> Self {
        let cache_dir = project_root.join(".nika/cache/http");
        std::fs::create_dir_all(&cache_dir).ok();
        Self {
            policies: DashMap::new(),
            cache_dir,
            max_size: 100 * 1024 * 1024,
        }
    }

    /// Cache key: blake3 hash of "METHOD:URL"
    fn cache_key(method: &str, url: &str) -> String {
        let hash = blake3::hash(format!("{}:{}", method, url).as_bytes());
        hash.to_hex()[..16].to_string()
    }

    pub async fn get(&self, method: &str, url: &str) -> Option<CacheEntry> {
        let key = Self::cache_key(method, url);
        let path = self.cache_dir.join(&key);
        let bytes = tokio::fs::read(&path).await.ok()?;
        postcard::from_bytes(&bytes).ok()
    }

    pub async fn put(&self, method: &str, url: &str, entry: CacheEntry) {
        let key = Self::cache_key(method, url);
        self.policies.insert(key.clone(), entry.policy.clone());
        if let Ok(bytes) = postcard::to_allocvec(&entry) {
            let path = self.cache_dir.join(&key);
            tokio::fs::write(&path, bytes).await.ok();
        }
    }
}
```

#### Integration in fetch.rs

```rust
// In run_fetch(), after building the request but before sending:

// --- CACHE LOOKUP ---
if method.eq_ignore_ascii_case("GET") || method.eq_ignore_ascii_case("HEAD") {
    if let Some(entry) = self.http_cache.get(&method, &url).await {
        let req_parts = build_request_parts(&method, &url, &resolved_headers);
        match entry.policy.before_request(&req_parts, SystemTime::now()) {
            BeforeRequest::Fresh(response_parts) => {
                // CACHE HIT -- emit event, return cached body
                self.event_log.emit(EventKind::FetchCacheHit {
                    task_id: Arc::clone(task_id),
                    url: url.clone(),
                });
                return process_cached_response(entry.body, &fetch);
            }
            BeforeRequest::Stale { request: conditional_parts, .. } => {
                // Add If-None-Match / If-Modified-Since to request
                for (name, value) in conditional_parts.headers.iter() {
                    request = request.header(name.clone(), value.clone());
                }
            }
        }
    }
}

// --- SEND REQUEST (existing code) ---
let response = request.send().await?;

// --- CACHE STORE ---
if response.status() == 304 {
    // Reuse cached body
    if let Some(entry) = self.http_cache.get(&method, &url).await {
        return process_cached_response(entry.body, &fetch);
    }
}

if method.eq_ignore_ascii_case("GET") || method.eq_ignore_ascii_case("HEAD") {
    let req_parts = build_request_parts(&method, &url, &resolved_headers);
    let res_parts = response_to_parts(&response);
    let policy = CachePolicy::new_options(&req_parts, &res_parts, SystemTime::now(),
        CacheOptions { shared: false, ..Default::default() });
    if policy.is_storable() {
        let body = response.bytes().await?;
        self.http_cache.put(&method, &url, CacheEntry {
            policy,
            body: body.to_vec(),
            url: url.clone(),
            stored_at: SystemTime::now(),
        }).await;
        return process_body(body, &fetch);
    }
}
```

#### nika.toml configuration

```toml
[fetch]
cache = true              # Enable HTTP caching (default: false)
cache_max_size = 104857600  # 100 MB
cache_ttl_override = 3600   # Optional: max TTL in seconds regardless of headers
```

---

## 2. Cookie Jar Management

### Crate Landscape

| Crate | Version | Role |
|-------|---------|------|
| `cookie_store` | 0.22.1 | RFC 6265 cookie store implementation |
| `reqwest_cookie_store` | 0.10.0 | Bridge between cookie_store and reqwest's CookieStore trait |
| reqwest `cookies` feature | built-in | reqwest's own Jar type (not persistent) |

### Key Findings

1. **reqwest's built-in `Jar`** persists cookies across requests within the same `Client` but has no serialization support -- cookies are lost when the Client is dropped.

2. **`reqwest_cookie_store::CookieStoreMutex`** implements `reqwest::cookie::CookieStore` and wraps `cookie_store::CookieStore` with a `Mutex`. This is the standard way to get persistent cookies with reqwest.

3. **Serialization**: `cookie_store::serde::json::save()` and `load()` provide JSON persistence. Also supports RON format.

4. **Thread safety**: `CookieStoreMutex` (Mutex-based) and `CookieStoreRwLock` (RwLock-based) are both available. RwLock is better for concurrent reads.

### Cargo.toml additions

```toml
cookie_store = { version = "0.22", features = ["serde_json"] }
reqwest_cookie_store = "0.10"
```

### Implementation Sketch

```rust
use cookie_store::CookieStore;
use reqwest_cookie_store::CookieStoreRwLock;
use std::sync::Arc;

/// Workflow-scoped cookie jar that persists to .nika/cache/cookies.json
pub struct NikaCookieJar {
    store: Arc<CookieStoreRwLock>,
    persist_path: PathBuf,
}

impl NikaCookieJar {
    /// Load or create cookie jar for this workflow run
    pub fn new(project_root: &Path) -> Self {
        let persist_path = project_root.join(".nika/cache/cookies.json");
        let store = if persist_path.exists() {
            let file = std::fs::File::open(&persist_path)
                .map(std::io::BufReader::new)
                .ok();
            file.and_then(|f| cookie_store::serde::json::load(f).ok())
                .unwrap_or_default()
        } else {
            CookieStore::default()
        };
        Self {
            store: Arc::new(CookieStoreRwLock::new(store)),
            persist_path,
        }
    }

    /// Get an Arc<CookieStoreRwLock> to pass to reqwest::Client::builder().cookie_provider()
    pub fn provider(&self) -> Arc<CookieStoreRwLock> {
        Arc::clone(&self.store)
    }

    /// Persist cookies to disk (call at workflow end)
    pub fn save(&self) -> Result<(), NikaError> {
        if let Some(parent) = self.persist_path.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        let mut writer = std::fs::File::create(&self.persist_path)
            .map(std::io::BufWriter::new)
            .map_err(|e| NikaError::ArtifactWriteFailed {
                reason: format!("Cookie jar save failed: {e}"),
            })?;
        let store = self.store.read().unwrap();
        cookie_store::serde::json::save(&store, &mut writer)
            .map_err(|e| NikaError::ArtifactWriteFailed {
                reason: format!("Cookie jar serialization failed: {e}"),
            })
    }
}
```

### Integration in TaskExecutor

```rust
// In TaskExecutor struct:
cookie_jar: Option<Arc<NikaCookieJar>>,

// When building the shared http_client:
let mut builder = reqwest::Client::builder()
    .timeout(FETCH_TIMEOUT)
    .connect_timeout(CONNECT_TIMEOUT)
    .user_agent(format!("nika/{}", env!("CARGO_PKG_VERSION")));

if let Some(ref jar) = self.cookie_jar {
    builder = builder.cookie_provider(jar.provider());
}
let http_client = builder.build()?;

// IMPORTANT: Per-request custom clients (SSRF pinned) also need cookie_provider:
if needs_custom_client {
    let mut builder = reqwest::Client::builder()/* ... */;
    if let Some(ref jar) = self.cookie_jar {
        builder = builder.cookie_provider(jar.provider());
    }
    // ...
}
```

### Workflow-level control

```yaml
# In workflow header:
fetch:
  cookies: true  # Enable cookie jar for this workflow (default: false)
```

### nika.toml control

```toml
[fetch]
cookies = false  # Global default
```

---

## 3. robots.txt Handling

### Crate Comparison

| Crate | Version | RFC | Crawl-Delay | Sitemaps | Test Coverage |
|-------|---------|-----|-------------|----------|---------------|
| `texting_robots` | 0.2.2 | Google spec | Yes (f32) | Yes | 34M+ real-world files tested |
| `robotstxt` | 0.3 | Google C++ port | No | No | Lower |

**Winner: `texting_robots`** -- tested against 34 million real-world robots.txt files, supports Crawl-Delay and sitemaps, clean API.

### API Surface

```rust
use texting_robots::{Robot, get_robots_url};

// 1. Get robots.txt URL for any page URL
let robots_url = get_robots_url("https://example.com/page")?;
// Returns: "https://example.com/robots.txt"

// 2. Parse robots.txt (you fetch the content yourself)
let robot = Robot::new("nika/0.63", robots_txt_bytes)?;

// 3. Check if URL is allowed
robot.allowed("https://example.com/api/data")  // -> bool

// 4. Read crawl delay
robot.delay  // -> Option<f32> (seconds)

// 5. Read sitemaps
robot.sitemaps  // -> Vec<String>
```

### Cargo.toml addition

```toml
texting_robots = "0.2"
```

### Implementation Sketch

```rust
use texting_robots::{Robot, get_robots_url};
use dashmap::DashMap;
use std::time::{Duration, Instant};

/// Per-domain robots.txt cache
pub struct RobotsCache {
    /// domain -> (Robot, fetched_at)
    cache: DashMap<String, (Robot, Instant)>,
    /// TTL for cached robots.txt (default: 1 hour)
    ttl: Duration,
    /// User agent for robots.txt matching
    user_agent: String,
}

impl RobotsCache {
    pub fn new(user_agent: &str) -> Self {
        Self {
            cache: DashMap::new(),
            ttl: Duration::from_secs(3600),
            user_agent: user_agent.to_string(),
        }
    }

    /// Extract domain from URL for cache key
    fn domain_key(url: &str) -> Option<String> {
        url::Url::parse(url).ok().and_then(|u| {
            u.host_str().map(|h| format!("{}://{}", u.scheme(), h))
        })
    }

    /// Check if URL is allowed by robots.txt. Fetches and caches if needed.
    pub async fn is_allowed(
        &self,
        url: &str,
        http_client: &reqwest::Client,
    ) -> Result<bool, NikaError> {
        let domain = Self::domain_key(url)
            .ok_or_else(|| NikaError::ValidationError {
                reason: format!("Cannot extract domain from URL: {}", url),
            })?;

        // Check cache (with TTL)
        if let Some(entry) = self.cache.get(&domain) {
            let (robot, fetched_at) = entry.value();
            if fetched_at.elapsed() < self.ttl {
                return Ok(robot.allowed(url));
            }
        }

        // Fetch robots.txt
        let robots_url = get_robots_url(url).map_err(|e| NikaError::FetchError {
            reason: format!("Invalid URL for robots.txt lookup: {e}"),
        })?;

        let robot = match http_client
            .get(&robots_url)
            .timeout(Duration::from_secs(10))
            .send()
            .await
        {
            Ok(resp) => {
                let status = resp.status().as_u16();
                match status {
                    200..=299 => {
                        let body = resp.bytes().await.map_err(|e| NikaError::FetchError {
                            reason: format!("Failed to read robots.txt: {e}"),
                        })?;
                        // Limit to 500 KiB per Google's recommendation
                        let truncated = &body[..body.len().min(512_000)];
                        Robot::new(&self.user_agent, truncated).unwrap_or_else(|_| {
                            // Parse error -> assume allowed (lenient)
                            Robot::new(&self.user_agent, b"").unwrap()
                        })
                    }
                    // 4xx (except 429): no restrictions
                    400..=499 if status != 429 => {
                        Robot::new(&self.user_agent, b"").unwrap()
                    }
                    // 429: too many requests -- be conservative, disallow
                    429 => {
                        Robot::new(&self.user_agent, b"User-agent: *\nDisallow: /").unwrap()
                    }
                    // 5xx: server error -- be conservative, disallow for now
                    _ => {
                        Robot::new(&self.user_agent, b"User-agent: *\nDisallow: /").unwrap()
                    }
                }
            }
            Err(_) => {
                // Network error fetching robots.txt -- assume allowed (lenient)
                Robot::new(&self.user_agent, b"").unwrap()
            }
        };

        let allowed = robot.allowed(url);
        self.cache.insert(domain, (robot, Instant::now()));
        Ok(allowed)
    }

    /// Get crawl delay for a domain (if set in robots.txt)
    pub fn crawl_delay(&self, url: &str) -> Option<f32> {
        Self::domain_key(url)
            .and_then(|domain| self.cache.get(&domain))
            .and_then(|entry| entry.value().0.delay)
    }
}
```

### Integration in fetch.rs

```rust
// In run_fetch(), BEFORE the request is sent, after SSRF check:

if let Some(ref robots) = self.robots_cache {
    if !robots.is_allowed(&url, &self.http_client).await? {
        self.event_log.emit(EventKind::PolicyBlocked {
            task_id: Arc::clone(task_id),
            verb: "fetch".to_string(),
            policy_type: "robots_txt".to_string(),
            reason: format!("Blocked by robots.txt: {}", url),
        });
        return Err(NikaError::PolicyViolation {
            reason: format!("URL blocked by robots.txt: {}", url),
        });
    }
}
```

### Configuration

```toml
# nika.toml
[fetch]
respect_robots_txt = true   # Default: true for polite crawling
robots_ttl = 3600           # Cache robots.txt for 1 hour

# Override per workflow:
# fetch:
#   robots: false  # Disable for this workflow (e.g., internal APIs)
```

---

## 4. Per-Domain Rate Limiting

### Crate: governor

| Feature | Detail |
|---------|--------|
| Version | 0.10.4 |
| Algorithm | GCRA (Generic Cell Rate Algorithm) |
| Keyed support | Yes, via DashMap (default) or HashMap |
| Thread-safe | Yes, all state is AtomicU64 |
| Async support | `.until_ready().await` / `.until_key_ready(&key).await` |

### http-cache built-in rate limiting

The `http-cache` crate already has a `rate-limiting` feature with `DomainRateLimiter` that wraps governor. But since we're not using http-cache-reqwest, we'll use governor directly.

### Cargo.toml addition

```toml
governor = { version = "0.10", features = ["std"] }
nonzero_ext = "0.3"
```

### Implementation Sketch

```rust
use governor::{Quota, RateLimiter, DefaultKeyedRateLimiter};
use std::num::NonZeroU32;
use std::sync::Arc;

/// Per-domain rate limiter for fetch: verb
pub struct FetchRateLimiter {
    /// Keyed by domain -> separate token bucket per domain
    limiter: DefaultKeyedRateLimiter<String>,
    /// Per-domain crawl delay overrides (from robots.txt)
    crawl_delays: DashMap<String, Duration>,
}

impl FetchRateLimiter {
    /// Create with default quota (e.g., 2 requests per second per domain)
    pub fn new(requests_per_second: u32) -> Self {
        let quota = Quota::per_second(
            NonZeroU32::new(requests_per_second).unwrap_or(NonZeroU32::new(2).unwrap())
        );
        Self {
            limiter: RateLimiter::keyed(quota),
            crawl_delays: DashMap::new(),
        }
    }

    /// Create from nika.toml configuration
    pub fn from_config(config: &FetchConfig) -> Self {
        Self::new(config.rate_limit_per_domain.unwrap_or(2))
    }

    /// Wait until a request to this domain is allowed
    pub async fn wait_for_domain(&self, url: &str) {
        if let Some(domain) = Self::extract_domain(url) {
            // Check for robots.txt crawl delay override
            if let Some(delay) = self.crawl_delays.get(&domain) {
                tokio::time::sleep(*delay.value()).await;
            }

            self.limiter.until_key_ready(&domain).await;
        }
    }

    /// Non-blocking check
    pub fn check_domain(&self, url: &str) -> bool {
        Self::extract_domain(url)
            .map(|d| self.limiter.check_key(&d).is_ok())
            .unwrap_or(true)
    }

    /// Set crawl delay for a domain (from robots.txt Crawl-Delay directive)
    pub fn set_crawl_delay(&self, domain: &str, delay_secs: f32) {
        if delay_secs > 0.0 && delay_secs <= 300.0 {
            self.crawl_delays.insert(
                domain.to_string(),
                Duration::from_secs_f32(delay_secs),
            );
        }
    }

    fn extract_domain(url: &str) -> Option<String> {
        url::Url::parse(url).ok().and_then(|u| u.host_str().map(String::from))
    }
}
```

### Integration in fetch.rs

```rust
// In run_fetch(), after SSRF + robots.txt checks, BEFORE request send:

if let Some(ref rate_limiter) = self.fetch_rate_limiter {
    // Apply crawl delay from robots.txt if available
    if let Some(ref robots) = self.robots_cache {
        if let Some(delay) = robots.crawl_delay(&url) {
            rate_limiter.set_crawl_delay(
                &url::Url::parse(&url).ok().and_then(|u| u.host_str().map(String::from))
                    .unwrap_or_default(),
                delay,
            );
        }
    }
    rate_limiter.wait_for_domain(&url).await;
}
```

### Configuration

```toml
# nika.toml
[fetch]
rate_limit = 2          # Requests per second per domain (default: unlimited)
respect_crawl_delay = true  # Honor robots.txt Crawl-Delay (default: true)
```

---

## 5. Combined Architecture

### New fields in TaskExecutor

```rust
pub struct TaskExecutor {
    // ... existing fields ...

    /// HTTP response cache (ETag / 304)
    http_cache: Option<Arc<NikaHttpCache>>,
    /// Cookie jar (workflow-scoped)
    cookie_jar: Option<Arc<NikaCookieJar>>,
    /// robots.txt cache (per-domain)
    robots_cache: Option<Arc<RobotsCache>>,
    /// Per-domain rate limiter
    fetch_rate_limiter: Option<Arc<FetchRateLimiter>>,
}
```

### New module structure

```
nika-engine/src/runtime/
  executor/
    fetch.rs            -- existing (add cache/robots/rate-limit hooks)
    fetch_cache.rs      -- NEW: NikaHttpCache (http-cache-semantics)
    fetch_cookies.rs    -- NEW: NikaCookieJar (cookie_store + reqwest_cookie_store)
    fetch_robots.rs     -- NEW: RobotsCache (texting_robots)
    fetch_rate_limit.rs -- NEW: FetchRateLimiter (governor)
```

### Request flow (ordered pipeline)

```
fetch: verb invoked
  |
  v
1. Template resolution (existing)
  |
  v
2. SSRF protection (existing)
  |
  v
3. robots.txt check (NEW) -- block if disallowed
  |
  v
4. HTTP cache lookup (NEW) -- return cached if fresh
  |
  v
5. Rate limiter wait (NEW) -- throttle per domain
  |
  v
6. Send HTTP request (existing, with cookies and conditional headers)
  |
  v
7. Handle 304 Not Modified (NEW) -- return cached body
  |
  v
8. Cache store (NEW) -- save response if storable
  |
  v
9. Extract / process response (existing)
```

### Dependency additions to Cargo.toml (nika-engine)

```toml
# HTTP caching (RFC 7234 policy engine)
http-cache-semantics = { version = "3.0", features = ["serde"] }

# Cookie management
cookie_store = { version = "0.22", features = ["serde_json"] }
reqwest_cookie_store = "0.10"

# robots.txt parsing (RFC 9309)
texting_robots = "0.2"

# Per-domain rate limiting (GCRA algorithm)
governor = { version = "0.10", features = ["std"] }
nonzero_ext = "0.3"
```

### AST additions (nika-core)

```rust
// In FetchParams or workflow header:
pub struct FetchConfig {
    /// Enable HTTP caching (ETag/304)
    pub cache: bool,
    /// Enable cookie jar persistence
    pub cookies: bool,
    /// Respect robots.txt
    pub robots: bool,
    /// Rate limit (requests/sec per domain), 0 = unlimited
    pub rate_limit: Option<u32>,
}
```

### Event kinds (nika-event)

```rust
// New events for observability:
FetchCacheHit { task_id, url }
FetchCacheMiss { task_id, url }
FetchCacheStore { task_id, url, ttl_secs }
FetchRobotsDenied { task_id, url }
FetchRateLimited { task_id, url, domain, waited_ms }
```

---

## 6. Dependency Weight Analysis

| Crate | Compile time impact | Binary size impact | Transitive deps |
|-------|--------------------|--------------------|-----------------|
| `http-cache-semantics` | Low (pure logic, no I/O) | ~30 KB | time, http-serde |
| `cookie_store` | Low | ~20 KB | cookie, url, time |
| `reqwest_cookie_store` | Minimal (thin wrapper) | ~5 KB | cookie_store |
| `texting_robots` | Low (pure parser) | ~25 KB | bstr, url, percent-encoding |
| `governor` | Low | ~15 KB | dashmap (already in deps) |

**Total estimated impact**: ~95 KB binary, minimal compile time. Most transitive deps (url, time, dashmap) are already in the workspace.

---

## 7. Risk Assessment

| Risk | Severity | Mitigation |
|------|----------|------------|
| http-cache-semantics 3.0 has breaking change | Low | Pin version, well-tested crate |
| Cookie jar leaks credentials across domains | Medium | cookie_store handles RFC 6265 domain scoping; keep jar workflow-scoped |
| robots.txt fetch creates infinite loop | Low | Use separate non-cached client for robots.txt, 10s timeout |
| Rate limiter blocks high-concurrency for_each | Medium | Rate limiter is per-domain, not per-workflow; concurrent tasks to different domains are fine |
| Cache fills .nika/ disk | Low | Size limit with eviction; `nika cache clear` command |
| Custom SSRF-pinned clients bypass cookie jar | Medium | Always pass cookie_provider to custom client builders |

---

## 8. Implementation Order

| Phase | Feature | Effort | Priority |
|-------|---------|--------|----------|
| 1 | robots.txt (texting_robots) | 1-2h | HIGH -- polite crawling baseline |
| 2 | Per-domain rate limiting (governor) | 1-2h | HIGH -- prevents 429 cascades |
| 3 | HTTP caching (http-cache-semantics) | 3-4h | MEDIUM -- performance optimization |
| 4 | Cookie jar (cookie_store) | 2-3h | LOW -- needed for authenticated scraping |

Phase 1+2 are essential for any scraping workflow and protect against BUG-005 (rate limit cascades). Phase 3 saves API calls and bandwidth. Phase 4 is needed for login-gated content.

---

## Sources

1. [http-cache-reqwest 1.0.0-alpha.5](https://crates.io/crates/http-cache-reqwest) -- Full source read of lib.rs (906 lines)
2. [http-cache 1.0.0-alpha.5](https://crates.io/crates/http-cache) -- Core lib.rs, CACacheManager, rate_limiting module
3. [http-cache-semantics 3.0.0](https://crates.io/crates/http-cache-semantics) -- CachePolicy API, CacheOptions, BeforeRequest/AfterResponse enums
4. [texting_robots 0.2.2](https://crates.io/crates/texting_robots) -- Robot struct, get_robots_url, full parser source
5. [cookie_store 0.22.1](https://crates.io/crates/cookie_store) -- serde::json::load/save, CookieStore API
6. [reqwest_cookie_store 0.10.0](https://crates.io/crates/reqwest_cookie_store) -- CookieStoreMutex, CookieStoreRwLock, reqwest integration
7. [governor 0.10.4](https://crates.io/crates/governor) -- DefaultKeyedRateLimiter, Quota, _guide module
8. Nika source: `nika-engine/src/runtime/executor/fetch.rs` (1146 lines) -- current fetch implementation
9. Nika source: `nika-engine/src/runtime/executor/mod.rs` -- TaskExecutor struct and http_client setup

## Methodology

- Tools used: cargo search, source code reading (15 crate source files)
- Pages analyzed: 12 Rust source files across 8 crates, plus 3 Nika engine files
- All APIs verified by reading actual implementations, not documentation alone

## Confidence Level

**High** -- All crate APIs verified from source. Integration points in Nika's fetch executor are well-understood from reading the 1146-line implementation. No assumptions made about undocumented behavior.
