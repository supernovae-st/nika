//! Fetch verb implementation for TaskExecutor
//!
//! Contains `run_fetch` for HTTP request execution.

use std::sync::Arc;
use std::time::Instant;

use futures::StreamExt;
use tracing::instrument;

use super::verbs::coerce_json_types;
use crate::ast::FetchParams;
use crate::binding::{template_resolve, ResolvedBindings};
use crate::error::NikaError;
use crate::event::EventKind;
use crate::runtime::policy::PolicyDecision;
use crate::store::RunContext;

use super::verbs::redact_for_event;
use super::TaskExecutor;
use crate::error_domains::ExecutionError;

/// Maximum response size for text responses (50 MB).
const MAX_TEXT_RESPONSE_SIZE: u64 = 50 * 1024 * 1024;

/// Maximum response size for binary CAS storage (100 MB).
const MAX_BINARY_RESPONSE_SIZE: u64 = 100 * 1024 * 1024;

/// Maximum backoff delay: 5 minutes (300,000 ms).
const MAX_BACKOFF_MS: u64 = 300_000;

/// Check if a content-type string indicates HTML (soft 404 detection for llm_txt).
fn is_html_content_type(ct: &str) -> bool {
    ct.to_ascii_lowercase().contains("text/html")
}

/// Safe exponential backoff that handles Infinity/NaN/overflow.
///
/// Returns a delay in milliseconds, guaranteed to be in `[1, MAX_BACKOFF_MS]`.
fn safe_backoff_delay(base_ms: u64, multiplier: f64, exp: u32) -> u64 {
    let factor = multiplier.powi(exp.min(30) as i32);
    if factor.is_infinite() || factor.is_nan() || factor > MAX_BACKOFF_MS as f64 {
        return MAX_BACKOFF_MS;
    }
    let raw = base_ms.saturating_mul(factor as u64);
    raw.clamp(1, MAX_BACKOFF_MS)
}

/// Read an HTTP response body with a streaming size limit.
///
/// Replaces `response.text().await` which buffers the entire body in memory.
/// Aborts early when accumulated size exceeds `max_bytes`, preventing OOM
/// from chunked transfer responses that bypass Content-Length pre-checks.
async fn read_body_with_limit(
    response: reqwest::Response,
    max_bytes: u64,
) -> Result<String, NikaError> {
    let mut stream = response.bytes_stream();
    let capacity = max_bytes.min(1_048_576) as usize;
    let mut buffer = Vec::with_capacity(capacity);
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| NikaError::FetchError {
            reason: format!("Failed to read response stream: {e}"),
        })?;
        if buffer.len() as u64 + chunk.len() as u64 > max_bytes {
            return Err(ExecutionError::FetchFailed {
                reason: format!(
                    "Response body exceeded {} byte limit during streaming",
                    max_bytes
                ),
            }
            .into());
        }
        buffer.extend_from_slice(&chunk);
    }
    // Use lossy conversion: non-UTF8 bytes (e.g., ISO-8859-1 pages) become
    // replacement characters rather than failing the entire fetch.
    Ok(String::from_utf8_lossy(&buffer).into_owned())
}

/// Read binary response body with a streaming size limit.
///
/// Same as `read_body_with_limit` but returns raw bytes instead of String.
/// Used for `response: binary` path where content is stored in CAS.
async fn read_bytes_with_limit(
    response: reqwest::Response,
    max_bytes: u64,
) -> Result<Vec<u8>, NikaError> {
    let mut stream = response.bytes_stream();
    let capacity = max_bytes.min(1_048_576) as usize;
    let mut buffer = Vec::with_capacity(capacity);
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| NikaError::FetchError {
            reason: format!("Failed to read binary response stream: {e}"),
        })?;
        if buffer.len() as u64 + chunk.len() as u64 > max_bytes {
            return Err(ExecutionError::FetchFailed {
                reason: format!(
                    "Binary response exceeded {} byte limit during streaming",
                    max_bytes
                ),
            }
            .into());
        }
        buffer.extend_from_slice(&chunk);
    }
    Ok(buffer)
}

impl TaskExecutor {
    #[instrument(skip(self, bindings, datastore), fields(url = %fetch.url))]
    pub(super) async fn run_fetch(
        &self,
        task_id: &Arc<str>,
        fetch: &FetchParams,
        bindings: &ResolvedBindings,
        datastore: &RunContext,
    ) -> Result<String, NikaError> {
        // Validate fetch params (empty URL, invalid response mode)
        fetch.validate()?;

        // Resolve {{with.alias}} templates
        let url = template_resolve(&fetch.url, bindings, datastore)?;

        // Bug 4: SSRF protection — only allow http(s) schemes
        if !url.starts_with("https://") && !url.starts_with("http://") {
            return Err(NikaError::ValidationError {
                reason: format!(
                    "fetch: URL must use http:// or https:// scheme, got: {}",
                    url.chars().take(50).collect::<String>()
                ),
            });
        }

        // POLICY CHECK: fetch verb
        // Two-phase: (1) string-level check with lock held, (2) async DNS check + pinning
        // The pinned addresses prevent TOCTOU DNS rebinding — reqwest uses these IPs
        // instead of re-resolving, so the validated IPs are the same ones used for connection.
        let mut pinned_host: Option<String> = None;
        let mut pinned_addrs: Vec<std::net::SocketAddr> = Vec::new();
        let policy_decision = {
            let string_decision = self.policy_enforcer.read().check_fetch(&url);
            if !string_decision.is_allowed() {
                string_decision
            } else {
                use crate::runtime::policy::resolve_and_pin_ssrf;
                if let Ok(parsed) = url::Url::parse(&url) {
                    if let Some(host) = parsed.host_str() {
                        let h = host.to_lowercase();
                        let h = h.trim_start_matches('[').trim_end_matches(']');
                        match resolve_and_pin_ssrf(h).await {
                            Err(reason) => PolicyDecision::Block(reason),
                            Ok(addrs) => {
                                if !addrs.is_empty() {
                                    pinned_host = Some(host.to_string());
                                    pinned_addrs = addrs;
                                }
                                PolicyDecision::Allow
                            }
                        }
                    } else {
                        PolicyDecision::Allow
                    }
                } else {
                    PolicyDecision::Allow
                }
            }
        };
        if let PolicyDecision::Block(reason) = policy_decision {
            // EMIT: PolicyBlocked
            self.event_log.emit(EventKind::PolicyBlocked {
                task_id: Arc::clone(task_id),
                verb: "fetch".to_string(),
                policy_type: "host_blocklist".to_string(),
                reason: reason.clone(),
            });
            tracing::warn!(
                task_id = %task_id,
                url = %url,
                reason = %reason,
                "fetch: blocked by policy"
            );
            return Err(NikaError::PolicyViolation { reason });
        }

        // ── robots.txt compliance ──────────────────────────────────────────
        if let Some(ref robots) = self.robots_cache {
            if let Ok(parsed) = url::Url::parse(&url) {
                if !robots.is_allowed(&parsed, &self.http_client).await {
                    self.event_log.emit(EventKind::PolicyBlocked {
                        task_id: Arc::clone(task_id),
                        verb: "fetch".to_string(),
                        policy_type: "robots_txt".to_string(),
                        reason: format!("robots.txt disallows: {}", url),
                    });
                    tracing::info!(
                        task_id = %task_id,
                        url = %url,
                        "fetch: blocked by robots.txt"
                    );
                    return Err(NikaError::PolicyViolation {
                        reason: format!("robots.txt disallows: {}", url),
                    });
                }
            }
        }

        // ── Per-domain rate limiting ───────────────────────────────────────
        if let Some(ref limiter) = self.domain_rate_limiter {
            if let Ok(parsed) = url::Url::parse(&url) {
                if let Some(domain) = parsed.host_str() {
                    limiter.acquire(domain).await;
                }
            }
        }

        // EMIT: TemplateResolved (redacted to avoid leaking secrets)
        self.event_log.emit(EventKind::TemplateResolved {
            task_id: Arc::clone(task_id),
            template: fetch.url.clone(),
            result: redact_for_event(&url),
        });

        // Select HTTP client based on follow_redirects + DNS pinning + redirect tracking
        // When we have pinned DNS addresses, build a one-off client with .resolve()
        // to prevent TOCTOU rebinding. Otherwise use the shared client.
        // CRAWL-003: Also build custom client for response:full to track redirect chain.
        let is_response_full = fetch.response == Some(nika_core::ast::extract::ResponseMode::Full);
        let redirect_chain: std::sync::Arc<parking_lot::Mutex<Vec<(u16, String)>>> =
            std::sync::Arc::new(parking_lot::Mutex::new(Vec::new()));
        let needs_custom_client =
            fetch.follow_redirects == Some(false) || !pinned_addrs.is_empty() || is_response_full;
        let http_client: std::borrow::Cow<'_, reqwest::Client> = if needs_custom_client {
            let mut builder = reqwest::Client::builder()
                .timeout(crate::util::FETCH_TIMEOUT)
                .connect_timeout(crate::util::CONNECT_TIMEOUT)
                .user_agent(format!("nika/{}", env!("CARGO_PKG_VERSION")));
            if fetch.follow_redirects == Some(false) {
                tracing::debug!(
                    task_id = %task_id,
                    "fetch: using no-redirect client (follow_redirects=false)"
                );
                builder = builder.redirect(reqwest::redirect::Policy::none());
            } else {
                // Apply SSRF redirect policy to DNS-pinned clients.
                // Without this, redirects bypass the DNS pinning (the pinned
                // addresses only bind to the initial host, not redirect targets).
                // CRAWL-003: Also capture redirect chain for response:full.
                let chain_capture = std::sync::Arc::clone(&redirect_chain);
                // Pass allowed_hosts to closure so SSRF check respects policy overrides
                let allowed: Vec<String> = self.policy_enforcer.read().allowed_hosts().to_vec();
                builder = builder.redirect(reqwest::redirect::Policy::custom(move |attempt| {
                    use crate::runtime::policy::is_ssrf_blocked;
                    // Capture redirect info for response:full
                    chain_capture
                        .lock()
                        .push((attempt.status().as_u16(), attempt.url().to_string()));
                    if attempt.previous().len() >= super::REDIRECT_LIMIT {
                        attempt.stop()
                    } else {
                        let blocked = attempt.url().host_str().and_then(|host| {
                            let h = host.to_lowercase();
                            let h_normalized = h.trim_start_matches('[').trim_end_matches(']');
                            // Skip SSRF block for explicitly allowed hosts
                            let explicitly_allowed = allowed
                                .iter()
                                .any(|a: &String| h_normalized == a.to_lowercase());
                            if !explicitly_allowed && is_ssrf_blocked(h_normalized) {
                                Some(h)
                            } else {
                                None
                            }
                        });
                        if let Some(host) = blocked {
                            attempt.error(std::io::Error::new(
                                std::io::ErrorKind::PermissionDenied,
                                format!("SSRF protection: redirect to '{}' blocked", host),
                            ))
                        } else {
                            attempt.follow()
                        }
                    }
                }));
            }
            // DNS pinning: force reqwest to use our pre-validated IPs
            if let Some(ref host) = pinned_host {
                for addr in &pinned_addrs {
                    builder = builder.resolve(host, *addr);
                }
                tracing::debug!(
                    task_id = %task_id,
                    host = %host,
                    addrs = %pinned_addrs.len(),
                    "fetch: DNS pinned to pre-validated addresses"
                );
            }
            std::borrow::Cow::Owned(builder.build().map_err(|e| NikaError::FetchError {
                reason: format!("HTTP client build failed: {e}"),
            })?)
        } else {
            std::borrow::Cow::Borrowed(&self.http_client)
        };

        // Build request based on HTTP method
        let mut request = if fetch.method.eq_ignore_ascii_case("POST") {
            http_client.post(url.as_ref())
        } else if fetch.method.eq_ignore_ascii_case("PUT") {
            http_client.put(url.as_ref())
        } else if fetch.method.eq_ignore_ascii_case("DELETE") {
            http_client.delete(url.as_ref())
        } else if fetch.method.eq_ignore_ascii_case("PATCH") {
            http_client.patch(url.as_ref())
        } else if fetch.method.eq_ignore_ascii_case("HEAD") {
            http_client.head(url.as_ref())
        } else if fetch.method.eq_ignore_ascii_case("OPTIONS") {
            http_client.request(reqwest::Method::OPTIONS, url.as_ref())
        } else {
            http_client.get(url.as_ref()) // Default to GET
        };

        // Add headers (resolve templates in both keys and values)
        for (key, value) in &fetch.headers {
            let resolved_key = if key.contains("{{") {
                template_resolve(key, bindings, datastore)?.into_owned()
            } else {
                key.clone()
            };
            let resolved_value = template_resolve(value, bindings, datastore)?;
            // SECURITY: reject CRLF injection in header keys and values
            if resolved_key.contains('\r') || resolved_key.contains('\n') {
                return Err(NikaError::ValidationError {
                    reason: format!(
                        "fetch: header key '{}' contains illegal CRLF characters",
                        resolved_key.chars().take(30).collect::<String>()
                    ),
                });
            }
            if resolved_value.contains('\r') || resolved_value.contains('\n') {
                return Err(NikaError::ValidationError {
                    reason: format!(
                        "fetch: header '{}' value contains illegal CRLF characters",
                        resolved_key
                    ),
                });
            }
            request = request.header(resolved_key, resolved_value.as_ref());
        }

        // Handle json field - takes precedence over body
        // Auto-serializes to JSON string and sets Content-Type: application/json
        // Template expressions inside JSON values are resolved (e.g. "{{inputs.query}}")
        if let Some(ref json_value) = fetch.json {
            // Resolve templates in JSON string values (same pattern as invoke.rs)
            let json_str =
                serde_json::to_string(json_value).map_err(|e| NikaError::InvalidJson {
                    details: format!("Failed to serialize json body: {e}"),
                })?;
            let resolved_json_str = template_resolve(&json_str, bindings, datastore)?.into_owned();
            let json_body = if resolved_json_str != json_str {
                // Re-parse to validate the resolved JSON is still valid, then
                // coerce string values back to native types (same as invoke.rs)
                serde_json::from_str::<serde_json::Value>(&resolved_json_str)
                    .map(|mut v| {
                        coerce_json_types(&mut v);
                        serde_json::to_string(&v).unwrap_or(resolved_json_str.clone())
                    })
                    .unwrap_or(resolved_json_str)
            } else {
                json_str
            };

            // Set Content-Type header if not already set
            if !fetch
                .headers
                .keys()
                .any(|k| k.eq_ignore_ascii_case("content-type"))
            {
                request = request.header("Content-Type", "application/json");
            }

            request = request.body(json_body);
        } else if let Some(body) = &fetch.body {
            // Add body if present (only if json not set)
            let resolved_body = template_resolve(body, bindings, datastore)?;
            request = request.body(resolved_body.into_owned());
        }

        // Apply per-request timeout if specified (overrides client default)
        if let Some(timeout_secs) = fetch.timeout {
            request = request.timeout(std::time::Duration::from_secs(timeout_secs));
        }

        // Retry configuration
        let max_attempts = fetch.retry.as_ref().map_or(1, |r| r.max_attempts.max(1));
        let backoff_ms = fetch.retry.as_ref().map_or(1000, |r| r.backoff_ms);
        let multiplier = fetch.retry.as_ref().map_or(2.0, |r| {
            if r.multiplier.is_finite() && r.multiplier > 0.0 {
                r.multiplier
            } else {
                1.0
            }
        });

        // Check if request can be cloned (required for retry)
        let can_retry = request.try_clone().is_some();
        if !can_retry && max_attempts > 1 {
            tracing::debug!(
                task_id = %task_id,
                "fetch: retry disabled (request body cannot be cloned)"
            );
        }

        let effective_max_attempts = if can_retry { max_attempts } else { 1 };
        let mut last_error: Option<NikaError> = None;
        let mut current_request = Some(request);
        let fetch_start = Instant::now();

        // Overall deadline prevents retry+backoff from blocking indefinitely.
        // Calculated as: per_request_timeout * max_attempts * 3 (covers request + backoff + buffer)
        let per_request_secs = fetch
            .timeout
            .unwrap_or(crate::util::FETCH_TIMEOUT.as_secs());
        let overall_deadline = fetch_start
            + std::time::Duration::from_secs(
                per_request_secs
                    .saturating_mul(effective_max_attempts as u64)
                    .saturating_mul(3),
            );

        // Determine method and has_body for HttpRequest event
        let req_method = fetch.method.to_uppercase();
        let req_has_body = fetch.body.is_some() || fetch.json.is_some();

        for attempt in 1..=effective_max_attempts {
            // Check for workflow cancellation before each attempt
            if self.cancel_token.is_cancelled() {
                return Err(NikaError::TaskCancelled {
                    task_id: task_id.to_string(),
                    reason: "workflow cancelled during fetch".to_string(),
                });
            }

            // Check overall deadline before each attempt
            if Instant::now() >= overall_deadline {
                return Err(ExecutionError::FetchFailed {
                    reason: format!(
                        "Overall fetch deadline exceeded ({}s) after {} of {} attempts",
                        per_request_secs * effective_max_attempts as u64 * 3,
                        attempt - 1,
                        effective_max_attempts,
                    ),
                }
                .into());
            }

            // Get the request for this attempt
            let req = current_request
                .take()
                .ok_or_else(|| NikaError::FetchError {
                    reason: format!(
                        "request unavailable on attempt {} of {} (clone may have failed)",
                        attempt, effective_max_attempts,
                    ),
                })?;

            // Clone for potential next retry (before sending consumes the request)
            if attempt < effective_max_attempts {
                current_request = req.try_clone();
            }

            // EMIT: HttpRequest
            self.event_log.emit(EventKind::HttpRequest {
                task_id: Arc::clone(task_id),
                method: req_method.clone(),
                url: url.to_string(),
                has_body: req_has_body,
            });

            let send_result = tokio::select! {
                biased;
                _ = self.cancel_token.cancelled() => {
                    return Err(NikaError::TaskCancelled {
                        task_id: task_id.to_string(),
                        reason: "workflow cancelled during fetch".to_string(),
                    });
                }
                result = req.send() => result,
            };
            match send_result {
                Ok(response) => {
                    // SECURITY: Post-redirect SSRF check.
                    // The redirect policy only does string-level IP checks. A redirect
                    // to a hostname that DNS-resolves to a private IP bypasses it.
                    // Check the final URL's hostname after all redirects.
                    {
                        use crate::runtime::policy::resolve_and_check_ssrf;
                        let final_url = response.url();
                        if let Some(host) = final_url.host_str() {
                            let h = host.to_lowercase();
                            let h = h.trim_start_matches('[').trim_end_matches(']');
                            if resolve_and_check_ssrf(h).await {
                                return Err(NikaError::PolicyViolation {
                                    reason: format!(
                                        "SSRF protection: final URL '{}' resolved to blocked IP after redirect",
                                        final_url
                                    ),
                                });
                            }
                        }
                    }

                    // EMIT: HttpResponse
                    let elapsed_ms = fetch_start.elapsed().as_millis() as u64;
                    let status_code = response.status().as_u16();
                    let content_type = response
                        .headers()
                        .get("content-type")
                        .and_then(|v| v.to_str().ok())
                        .map(|s| s.to_string());
                    let content_length = response.content_length();
                    self.event_log.emit(EventKind::HttpResponse {
                        task_id: Arc::clone(task_id),
                        status_code,
                        content_type,
                        content_length,
                        elapsed_ms,
                    });

                    // Check for server errors or rate limits that should be retried
                    let is_retryable_status = response.status().is_server_error()
                        || response.status() == reqwest::StatusCode::TOO_MANY_REQUESTS;
                    if is_retryable_status && attempt < effective_max_attempts {
                        let status = response.status();

                        // Prefer server-mandated Retry-After delay on 429, else exponential backoff
                        let delay_ms = if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
                            parse_retry_after(response.headers())
                        } else {
                            None
                        }
                        .unwrap_or_else(|| safe_backoff_delay(backoff_ms, multiplier, attempt - 1));

                        // EMIT: FetchRetry
                        self.event_log.emit(EventKind::FetchRetry {
                            task_id: Arc::clone(task_id),
                            url: url.to_string(),
                            attempt,
                            max_attempts: effective_max_attempts,
                            status_code: Some(status.as_u16()),
                            backoff_ms: delay_ms,
                        });

                        tracing::warn!(
                            task_id = %task_id,
                            attempt = attempt,
                            status = %status,
                            "fetch: server error, retrying..."
                        );
                        last_error = Some(NikaError::FetchError {
                            reason: format!("HTTP server error: {}", status),
                        });

                        // Exponential backoff (bounded by overall deadline)
                        let remaining = overall_deadline.saturating_duration_since(Instant::now());
                        let bounded_delay =
                            std::time::Duration::from_millis(delay_ms).min(remaining);
                        if bounded_delay.is_zero() {
                            let reason = format!(
                                "Overall fetch deadline exceeded during backoff after {} of {} attempts",
                                attempt, effective_max_attempts,
                            );
                            // EMIT: FetchExhausted
                            self.event_log.emit(EventKind::FetchExhausted {
                                task_id: Arc::clone(task_id),
                                url: url.to_string(),
                                attempts: attempt,
                                last_status: Some(response.status().as_u16()),
                                reason: reason.clone(),
                            });
                            return Err(ExecutionError::FetchFailed { reason }.into());
                        }
                        tokio::time::sleep(bounded_delay).await;
                        continue;
                    }

                    // If we exhausted retries and the last status was retryable,
                    // return the error instead of treating the error body as success.
                    if is_retryable_status {
                        let status = response.status();
                        let reason = format!(
                            "HTTP {} after {} retry attempt(s) exhausted",
                            status, effective_max_attempts,
                        );
                        // EMIT: FetchExhausted
                        self.event_log.emit(EventKind::FetchExhausted {
                            task_id: Arc::clone(task_id),
                            url: url.to_string(),
                            attempts: effective_max_attempts,
                            last_status: Some(status.as_u16()),
                            reason: reason.clone(),
                        });
                        return Err(NikaError::FetchError { reason });
                    }

                    // Fail on client errors (4xx) unless response: full
                    // (response: full includes status in JSON — user can inspect it)
                    // (response: binary already fails on non-2xx at line ~597)
                    if !response.status().is_success()
                        && !response.status().is_redirection()
                        && fetch.response != Some(nika_core::ast::extract::ResponseMode::Full)
                        && fetch.response != Some(nika_core::ast::extract::ResponseMode::Slim)
                    {
                        let status = response.status();
                        let final_url = response.url().to_string();
                        return Err(NikaError::FetchError {
                            reason: format!(
                                "HTTP {} {} for URL: {}",
                                status.as_u16(),
                                status.canonical_reason().unwrap_or("Unknown"),
                                final_url
                            ),
                        });
                    }

                    // Check response mode BEFORE consuming the body
                    // IMP-028: Slim mode — metadata only (no body, no headers)
                    if fetch.response == Some(nika_core::ast::extract::ResponseMode::Slim) {
                        let status = response.status().as_u16();
                        let final_url = response.url().to_string();
                        let redirects = redirect_chain.lock().clone();
                        let redirect_count = redirects.len();
                        let redirects_json: Vec<serde_json::Value> = redirects
                            .iter()
                            .map(|(s, u)| serde_json::json!({"status": s, "url": u}))
                            .collect();

                        // If extract mode is also set, read body for extraction
                        if fetch.extract.is_some() {
                            let body =
                                read_body_with_limit(response, MAX_TEXT_RESPONSE_SIZE).await?;
                            let resolved_selector = match &fetch.selector {
                                Some(s) => {
                                    Some(template_resolve(s, bindings, datastore)?.into_owned())
                                }
                                None => None,
                            };
                            let extract_result = super::extract::apply_extract_with_base(
                                &body,
                                fetch.extract,
                                resolved_selector.as_deref(),
                                Some(&final_url),
                            );
                            let extracted = match extract_result {
                                Ok(s) => serde_json::from_str::<serde_json::Value>(&s)
                                    .unwrap_or(serde_json::Value::String(s)),
                                Err(e) => serde_json::json!({"error": e.to_string()}),
                            };
                            return Ok(serde_json::json!({
                                "status": status,
                                "url": final_url,
                                "elapsed_ms": elapsed_ms,
                                "redirects": redirects_json,
                                "redirect_count": redirect_count,
                                "extracted": extracted,
                            })
                            .to_string());
                        }

                        return Ok(serde_json::json!({
                            "status": status,
                            "url": final_url,
                            "elapsed_ms": elapsed_ms,
                            "redirects": redirects_json,
                            "redirect_count": redirect_count,
                        })
                        .to_string());
                    }

                    if fetch.response == Some(nika_core::ast::extract::ResponseMode::Full) {
                        let status = response.status().as_u16();
                        // HREFLANG-001: capture Link headers before collecting all headers
                        let link_headers: Vec<String> = response
                            .headers()
                            .get_all("link")
                            .iter()
                            .filter_map(|v| v.to_str().ok().map(String::from))
                            .collect();
                        let headers: serde_json::Map<String, serde_json::Value> = response
                            .headers()
                            .iter()
                            .map(|(k, v)| {
                                (
                                    k.to_string(),
                                    serde_json::Value::String(v.to_str().unwrap_or("").to_string()),
                                )
                            })
                            .collect();
                        let final_url = response.url().to_string();
                        if let Some(len) = response.content_length() {
                            if len > MAX_TEXT_RESPONSE_SIZE {
                                return Err(ExecutionError::FetchFailed {
                                    reason: format!(
                                        "Response too large ({} bytes, max {} bytes)",
                                        len, MAX_TEXT_RESPONSE_SIZE
                                    ),
                                }
                                .into());
                            }
                        }
                        let body = read_body_with_limit(response, MAX_TEXT_RESPONSE_SIZE).await?;

                        // ENG-015: When extract mode is also set, apply extraction and
                        // include the result in an "extracted" field alongside the full response.
                        if fetch.extract.is_some() {
                            let resolved_selector = match &fetch.selector {
                                Some(s) => {
                                    Some(template_resolve(s, bindings, datastore)?.into_owned())
                                }
                                None => None,
                            };
                            let extract_result = super::extract::apply_extract_with_base(
                                &body,
                                fetch.extract,
                                resolved_selector.as_deref(),
                                Some(&final_url),
                            );
                            if let Some(mode) = fetch.extract {
                                let output_len =
                                    extract_result.as_ref().map(|s| s.len()).unwrap_or(0);
                                self.event_log.emit(EventKind::ExtractApplied {
                                    task_id: Arc::clone(task_id),
                                    mode: mode.to_string(),
                                    selector: fetch.selector.clone(),
                                    input_len: body.len(),
                                    output_len,
                                });
                            }
                            let extracted = match extract_result {
                                Ok(extracted_str) => {
                                    // Try parsing as JSON; fall back to string
                                    serde_json::from_str::<serde_json::Value>(&extracted_str)
                                        .unwrap_or(serde_json::Value::String(extracted_str))
                                }
                                Err(e) => {
                                    // Extract failed — include the error, don't fail the task
                                    serde_json::json!({ "error": e.to_string() })
                                }
                            };
                            // HREFLANG-001: merge Link header hreflang into extracted metadata
                            let extracted =
                                merge_link_hreflang_value(extracted, fetch.extract, &link_headers);
                            // CRAWL-003: include redirect chain
                            let redirects = redirect_chain.lock().clone();
                            let redirect_count = redirects.len();
                            let redirects_json: Vec<serde_json::Value> = redirects
                                .iter()
                                .map(|(s, u)| serde_json::json!({"status": s, "url": u}))
                                .collect();
                            return Ok(serde_json::json!({
                                "status": status,
                                "headers": headers,
                                "body": body,
                                "url": final_url,
                                "elapsed_ms": elapsed_ms,
                                "redirects": redirects_json,
                                "redirect_count": redirect_count,
                                "extracted": extracted,
                            })
                            .to_string());
                        }

                        // CRAWL-003: include redirect chain
                        let redirects = redirect_chain.lock().clone();
                        let redirect_count = redirects.len();
                        let redirects_json: Vec<serde_json::Value> = redirects
                            .iter()
                            .map(|(s, u)| serde_json::json!({"status": s, "url": u}))
                            .collect();
                        return Ok(serde_json::json!({
                            "status": status,
                            "headers": headers,
                            "body": body,
                            "url": final_url,
                            "elapsed_ms": elapsed_ms,
                            "redirects": redirects_json,
                            "redirect_count": redirect_count,
                        })
                        .to_string());
                    }

                    if fetch.response == Some(nika_core::ast::extract::ResponseMode::Binary) {
                        // Reject non-success HTTP status before storing anything in CAS.
                        // Without this, 4xx/5xx error pages (HTML) get stored as binary artifacts.
                        if !response.status().is_success() {
                            return Err(ExecutionError::FetchFailed {
                                reason: format!(
                                    "HTTP {} for binary fetch: {}",
                                    response.status(),
                                    url
                                ),
                            }
                            .into());
                        }
                        // Strip Content-Type parameters (e.g. "image/png; charset=utf-8" -> "image/png")
                        // so that mime_to_extension() exact matching works correctly.
                        let content_type = response
                            .headers()
                            .get(reqwest::header::CONTENT_TYPE)
                            .and_then(|v| v.to_str().ok())
                            .unwrap_or("application/octet-stream");
                        let content_type = content_type
                            .split(';')
                            .next()
                            .unwrap_or(content_type)
                            .trim()
                            .to_string();
                        if let Some(len) = response.content_length() {
                            if len > MAX_BINARY_RESPONSE_SIZE {
                                return Err(ExecutionError::FetchFailed {
                                    reason: format!(
                                        "Binary response too large ({} bytes, max {} bytes)",
                                        len, MAX_BINARY_RESPONSE_SIZE
                                    ),
                                }
                                .into());
                            }
                        }
                        let bytes =
                            read_bytes_with_limit(response, MAX_BINARY_RESPONSE_SIZE).await?;
                        // Bug 11: Handle 0-byte responses gracefully
                        if bytes.is_empty() {
                            return Ok(serde_json::json!({
                                "hash": null,
                                "mime_type": content_type,
                                "size_bytes": 0,
                                "deduplicated": false,
                            })
                            .to_string());
                        }
                        let store_result =
                            self.cas
                                .store(&bytes)
                                .await
                                .map_err(|e| NikaError::FetchError {
                                    reason: format!("CAS store failed: {}", e),
                                })?;

                        // Stage MediaRef so artifact format: binary can find it.
                        // Without this, write_binary_artifact() gets empty media_refs -> NIKA-281.
                        let media_ref = crate::media::MediaRef {
                            hash: store_result.hash.clone(),
                            mime_type: content_type.clone(),
                            size_bytes: bytes.len() as u64,
                            path: store_result.path.clone(),
                            extension: crate::media::detect::mime_to_extension(&content_type),
                            created_by: task_id.to_string(),
                            metadata: serde_json::Map::new(),
                        };
                        datastore.set_media(task_id, vec![media_ref]);

                        return Ok(serde_json::json!({
                            "hash": store_result.hash,
                            "mime_type": content_type,
                            "size_bytes": bytes.len(),
                            "deduplicated": store_result.deduplicated,
                        })
                        .to_string());
                    }

                    if let Some(len) = response.content_length() {
                        if len > MAX_TEXT_RESPONSE_SIZE {
                            return Err(ExecutionError::FetchFailed {
                                reason: format!(
                                    "Response too large ({} bytes, max {} bytes)",
                                    len, MAX_TEXT_RESPONSE_SIZE
                                ),
                            }
                            .into());
                        }
                    }
                    // Special case: llm_txt requires sub-requests, handled here not in extract.rs
                    if fetch.extract == Some(nika_core::ast::extract::ExtractMode::LlmTxt) {
                        let parsed =
                            url::Url::parse(url.as_ref()).map_err(|e| NikaError::FetchError {
                                reason: format!("Invalid URL for llm_txt: {e}"),
                            })?;
                        let origin = parsed.origin().unicode_serialization();
                        // Prefer llms-full.txt (expanded) over llms.txt (summary)
                        for path in &[
                            "/llms-full.txt",
                            "/.well-known/llm.txt",
                            "/llms.txt",
                            "/llm.txt",
                        ] {
                            let llm_url = format!("{}{}", origin, path);
                            // SSRF policy check for llm_txt sub-requests
                            let sub_decision = self.policy_enforcer.read().check_fetch(&llm_url);
                            if let PolicyDecision::Block(reason) = sub_decision {
                                tracing::warn!(
                                    task_id = %task_id,
                                    url = %llm_url,
                                    reason = %reason,
                                    "fetch: llm_txt sub-request blocked by policy"
                                );
                                continue;
                            }
                            if let Ok(resp) = http_client
                                .get(&llm_url)
                                .timeout(std::time::Duration::from_secs(5))
                                .send()
                                .await
                            {
                                // Size limit on llm.txt response (1 MB max -- these are text files)
                                if resp.content_length().unwrap_or(0) > 1_048_576 {
                                    continue;
                                }
                                if resp.status().is_success() {
                                    // Skip HTML responses (soft 404)
                                    let ct = resp
                                        .headers()
                                        .get(reqwest::header::CONTENT_TYPE)
                                        .and_then(|v| v.to_str().ok())
                                        .unwrap_or("");
                                    if is_html_content_type(ct) {
                                        tracing::debug!(url = %llm_url, "llm_txt: skipping HTML response (soft 404)");
                                        continue;
                                    }
                                    if let Ok(body) = read_body_with_limit(resp, 1_048_576).await {
                                        if !body.trim().is_empty() {
                                            return Ok(serde_json::json!({
                                                "found": true,
                                                "url": llm_url,
                                                "content": body,
                                            })
                                            .to_string());
                                        }
                                    } else {
                                        tracing::debug!(url = %llm_url, "llm_txt: failed to read response body");
                                    }
                                }
                            } else {
                                tracing::debug!(url = %llm_url, "llm_txt: request failed");
                            }
                        }
                        return Ok(serde_json::json!({ "found": false }).to_string());
                    }

                    let response_url = response.url().to_string();
                    // HREFLANG-001: capture Link headers before consuming response body
                    let link_headers: Vec<String> = response
                        .headers()
                        .get_all("link")
                        .iter()
                        .filter_map(|v| v.to_str().ok().map(String::from))
                        .collect();
                    let raw_body = read_body_with_limit(response, MAX_TEXT_RESPONSE_SIZE).await?;
                    // Resolve templates in selector (e.g. {{with.css_query}})
                    let resolved_selector = match &fetch.selector {
                        Some(s) => Some(template_resolve(s, bindings, datastore)?.into_owned()),
                        None => None,
                    };
                    let extract_result = super::extract::apply_extract_with_base(
                        &raw_body,
                        fetch.extract,
                        resolved_selector.as_deref(),
                        Some(&response_url),
                    );
                    // EMIT: ExtractApplied (if an extract mode was specified)
                    if let Some(mode) = fetch.extract {
                        let output_len = extract_result.as_ref().map(|s| s.len()).unwrap_or(0);
                        self.event_log.emit(EventKind::ExtractApplied {
                            task_id: Arc::clone(task_id),
                            mode: mode.to_string(),
                            selector: fetch.selector.clone(),
                            input_len: raw_body.len(),
                            output_len,
                        });
                    }
                    // HREFLANG-001: merge Link header hreflang into metadata result
                    return merge_link_hreflang(extract_result, fetch.extract, &link_headers);
                }
                Err(e) => {
                    // Network errors are retryable
                    if attempt < effective_max_attempts {
                        // Exponential backoff calculation
                        let delay_ms = safe_backoff_delay(backoff_ms, multiplier, attempt - 1);

                        // EMIT: FetchRetry
                        self.event_log.emit(EventKind::FetchRetry {
                            task_id: Arc::clone(task_id),
                            url: url.to_string(),
                            attempt,
                            max_attempts: effective_max_attempts,
                            status_code: None,
                            backoff_ms: delay_ms,
                        });

                        tracing::warn!(
                            task_id = %task_id,
                            attempt = attempt,
                            error = %e,
                            "fetch: request failed, retrying..."
                        );
                        last_error = Some(NikaError::FetchError {
                            reason: format!("HTTP request failed: {}", e),
                        });

                        // Exponential backoff (bounded by overall deadline)
                        let remaining = overall_deadline.saturating_duration_since(Instant::now());
                        let bounded_delay =
                            std::time::Duration::from_millis(delay_ms).min(remaining);
                        if bounded_delay.is_zero() {
                            let reason = format!(
                                "Overall fetch deadline exceeded during backoff after {} of {} attempts",
                                attempt, effective_max_attempts,
                            );
                            // EMIT: FetchExhausted
                            self.event_log.emit(EventKind::FetchExhausted {
                                task_id: Arc::clone(task_id),
                                url: url.to_string(),
                                attempts: attempt,
                                last_status: None,
                                reason: reason.clone(),
                            });
                            return Err(ExecutionError::FetchFailed { reason }.into());
                        }
                        tokio::time::sleep(bounded_delay).await;
                        continue;
                    }

                    let reason = format!(
                        "HTTP request failed after {} attempts: {}",
                        effective_max_attempts, e
                    );
                    // EMIT: FetchExhausted
                    self.event_log.emit(EventKind::FetchExhausted {
                        task_id: Arc::clone(task_id),
                        url: url.to_string(),
                        attempts: effective_max_attempts,
                        last_status: None,
                        reason: reason.clone(),
                    });
                    return Err(ExecutionError::FetchFailed { reason }.into());
                }
            }
        }

        // Should not reach here, but just in case
        Err(last_error.unwrap_or_else(|| NikaError::FetchError {
            reason: "HTTP request failed: unknown error".to_string(),
        }))
    }
}

/// HREFLANG-001: Merge Link header hreflang entries into a metadata extract result (String path).
/// For non-metadata modes or empty link_headers, returns the original result unchanged.
fn merge_link_hreflang(
    result: Result<String, NikaError>,
    extract_mode: Option<nika_core::ast::extract::ExtractMode>,
    link_headers: &[String],
) -> Result<String, NikaError> {
    use nika_core::ast::extract::ExtractMode;
    if !matches!(extract_mode, Some(ExtractMode::Metadata)) || link_headers.is_empty() {
        return result;
    }
    let result_str = result?;
    let mut parsed: serde_json::Value = serde_json::from_str(&result_str).unwrap_or_default();
    let link_hreflang = super::extract::parse_link_header_hreflang(link_headers);
    if !link_hreflang.is_empty() {
        let hreflang = parsed.as_object_mut().and_then(|obj| {
            obj.entry("hreflang")
                .or_insert(serde_json::json!([]))
                .as_array_mut()
        });
        if let Some(arr) = hreflang {
            arr.extend(link_hreflang);
            dedup_hreflang(arr);
        }
    }
    Ok(parsed.to_string())
}

/// HREFLANG-001: Merge Link header hreflang into an already-parsed Value (response:full path).
fn merge_link_hreflang_value(
    mut extracted: serde_json::Value,
    extract_mode: Option<nika_core::ast::extract::ExtractMode>,
    link_headers: &[String],
) -> serde_json::Value {
    use nika_core::ast::extract::ExtractMode;
    if !matches!(extract_mode, Some(ExtractMode::Metadata)) || link_headers.is_empty() {
        return extracted;
    }
    let link_hreflang = super::extract::parse_link_header_hreflang(link_headers);
    if !link_hreflang.is_empty() {
        if let Some(obj) = extracted.as_object_mut() {
            let arr = obj.entry("hreflang").or_insert(serde_json::json!([]));
            if let Some(vec) = arr.as_array_mut() {
                vec.extend(link_hreflang);
                dedup_hreflang(vec);
            }
        }
    }
    extracted
}

/// Remove exact duplicate hreflang entries (same lang + same href).
/// Different URLs for the same lang are kept — that's a site-side SEO error the user should see.
fn dedup_hreflang(entries: &mut Vec<serde_json::Value>) {
    let mut seen = std::collections::HashSet::new();
    entries.retain(|entry| {
        let key = (
            entry["lang"].as_str().unwrap_or_default().to_string(),
            entry["href"].as_str().unwrap_or_default().to_string(),
        );
        seen.insert(key)
    });
}

/// Parse the `Retry-After` header from a 429 response.
///
/// Supports delay-seconds format per RFC 7231 §7.1.3:
/// - `Retry-After: 120` → 120_000ms
///
/// HTTP-date format is not supported (uncommon for LLM APIs).
/// Returns `None` if the header is missing, unparseable, or zero.
/// Caps at 5 minutes to prevent servers from stalling a workflow indefinitely.
fn parse_retry_after(headers: &reqwest::header::HeaderMap) -> Option<u64> {
    const MAX_RETRY_AFTER_MS: u64 = 300_000; // 5 minutes cap

    let value = headers.get(reqwest::header::RETRY_AFTER)?.to_str().ok()?;
    let secs = value.trim().parse::<u64>().ok()?;
    if secs == 0 {
        return None;
    }
    Some(secs.saturating_mul(1000).min(MAX_RETRY_AFTER_MS))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_retry_after_integer_seconds() {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(reqwest::header::RETRY_AFTER, "30".parse().unwrap());
        assert_eq!(parse_retry_after(&headers), Some(30_000));
    }

    #[test]
    fn parse_retry_after_missing_header() {
        let headers = reqwest::header::HeaderMap::new();
        assert_eq!(parse_retry_after(&headers), None);
    }

    #[test]
    fn parse_retry_after_zero_returns_none() {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(reqwest::header::RETRY_AFTER, "0".parse().unwrap());
        assert_eq!(parse_retry_after(&headers), None);
    }

    #[test]
    fn parse_retry_after_caps_at_5_minutes() {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(reqwest::header::RETRY_AFTER, "600".parse().unwrap());
        assert_eq!(parse_retry_after(&headers), Some(300_000)); // capped
    }

    #[test]
    fn parse_retry_after_non_numeric_returns_none() {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(
            reqwest::header::RETRY_AFTER,
            "Fri, 31 Dec 2099 23:59:59 GMT".parse().unwrap(),
        );
        assert_eq!(parse_retry_after(&headers), None); // HTTP-date not supported
    }

    #[test]
    fn parse_retry_after_whitespace_trimmed() {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(reqwest::header::RETRY_AFTER, " 5 ".parse().unwrap());
        assert_eq!(parse_retry_after(&headers), Some(5_000));
    }

    #[test]
    fn backoff_large_exponent_does_not_produce_zero() {
        for exp in 0..=30u32 {
            let delay = safe_backoff_delay(100, 2.5, exp);
            assert!(delay > 0, "delay must never be 0 at exp={exp}");
            assert!(
                delay <= MAX_BACKOFF_MS,
                "delay must be capped at {MAX_BACKOFF_MS}, got {delay} at exp={exp}"
            );
        }
    }

    #[test]
    fn backoff_infinity_capped_at_max() {
        // 10^30 overflows f64 → Infinity → (as u64) == 0 in Rust
        let delay = safe_backoff_delay(100, 10.0, 30);
        assert_eq!(delay, MAX_BACKOFF_MS, "Infinity should be capped");
    }

    #[test]
    fn backoff_normal_case() {
        // 100 * 2.0^2 = 400
        let delay = safe_backoff_delay(100, 2.0, 2);
        assert_eq!(delay, 400);
    }

    #[test]
    fn backoff_zero_exponent() {
        // 100 * 2.0^0 = 100
        let delay = safe_backoff_delay(100, 2.0, 0);
        assert_eq!(delay, 100);
    }

    #[test]
    fn is_html_content_type_detects_html() {
        assert!(is_html_content_type("text/html"));
        assert!(is_html_content_type("text/html; charset=utf-8"));
        assert!(is_html_content_type("TEXT/HTML"));
    }

    #[test]
    fn is_html_content_type_allows_plaintext() {
        assert!(!is_html_content_type("text/plain"));
        assert!(!is_html_content_type("text/markdown"));
        assert!(!is_html_content_type("application/json"));
    }
}
