//! Fetch verb implementation for TaskExecutor
//!
//! Contains `run_fetch` for HTTP request execution.

use std::sync::Arc;
use std::time::Instant;

use tracing::instrument;

use crate::ast::FetchParams;
use crate::binding::{template_resolve, ResolvedBindings};
use crate::error::NikaError;
use crate::event::EventKind;
use crate::runtime::policy::PolicyDecision;
use crate::store::RunContext;

use super::verbs::redact_for_event;
use super::TaskExecutor;
use crate::error_domains::ExecutionError;

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
        let policy_decision = self.policy_enforcer.read().check_fetch(&url);
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

        // EMIT: TemplateResolved (redacted to avoid leaking secrets)
        self.event_log.emit(EventKind::TemplateResolved {
            task_id: Arc::clone(task_id),
            template: fetch.url.clone(),
            result: redact_for_event(&url),
        });

        // Select HTTP client based on follow_redirects setting
        // Default behavior (None or Some(true)) uses the shared client with redirects enabled
        // When follow_redirects = false, create a one-off client without redirect following
        let http_client: std::borrow::Cow<'_, reqwest::Client> =
            if fetch.follow_redirects == Some(false) {
                tracing::debug!(
                    task_id = %task_id,
                    "fetch: using no-redirect client (follow_redirects=false)"
                );
                std::borrow::Cow::Owned(
                    reqwest::Client::builder()
                        .timeout(crate::util::FETCH_TIMEOUT)
                        .connect_timeout(crate::util::CONNECT_TIMEOUT)
                        .redirect(reqwest::redirect::Policy::none())
                        .user_agent(format!("nika/{}", env!("CARGO_PKG_VERSION")))
                        .build()
                        .map_err(|e| NikaError::FetchError {
                            reason: format!("HTTP client build failed: {e}"),
                        })?,
                )
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

        // Add headers
        for (key, value) in &fetch.headers {
            let resolved_value = template_resolve(value, bindings, datastore)?;
            request = request.header(key, resolved_value.as_ref());
        }

        // Handle json field - takes precedence over body
        // Auto-serializes to JSON string and sets Content-Type: application/json
        if let Some(ref json_value) = fetch.json {
            // Serialize JSON value to string
            let json_body =
                serde_json::to_string(json_value).map_err(|e| NikaError::InvalidJson {
                    details: format!("Failed to serialize json body: {e}"),
                })?;

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
        let multiplier = fetch.retry.as_ref().map_or(2.0, |r| r.multiplier);

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

            match req.send().await {
                Ok(response) => {
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

                    // Check for server errors that should be retried
                    if response.status().is_server_error() && attempt < effective_max_attempts {
                        let status = response.status();

                        // Exponential backoff calculation
                        let exp = (attempt - 1).min(30) as i32;
                        let delay_ms = backoff_ms
                            .saturating_mul(multiplier.powi(exp).min(u64::MAX as f64) as u64);

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
                            return Err(ExecutionError::FetchFailed {
                                reason: format!(
                                    "Overall fetch deadline exceeded during backoff after {} of {} attempts",
                                    attempt, effective_max_attempts,
                                ),
                            }
                            .into());
                        }
                        tokio::time::sleep(bounded_delay).await;
                        continue;
                    }

                    // Success or non-retryable error status

                    // Check response mode BEFORE consuming the body
                    if fetch.response.as_deref() == Some("full") {
                        let status = response.status().as_u16();
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
                        // Size limit for full response too
                        const FULL_MAX_RESPONSE_SIZE: u64 = 50 * 1024 * 1024;
                        if let Some(len) = response.content_length() {
                            if len > FULL_MAX_RESPONSE_SIZE {
                                return Err(ExecutionError::FetchFailed {
                                    reason: format!(
                                        "Response too large ({} bytes, max {} bytes)",
                                        len, FULL_MAX_RESPONSE_SIZE
                                    ),
                                }
                                .into());
                            }
                        }
                        let body = response.text().await.map_err(|e| NikaError::FetchError {
                            reason: format!("Failed to read response: {}", e),
                        })?;
                        if body.len() as u64 > FULL_MAX_RESPONSE_SIZE {
                            return Err(ExecutionError::FetchFailed {
                                reason: format!(
                                    "Response body too large ({} bytes, max {} bytes)",
                                    body.len(),
                                    FULL_MAX_RESPONSE_SIZE
                                ),
                            }
                            .into());
                        }
                        return Ok(serde_json::json!({
                            "status": status,
                            "headers": headers,
                            "body": body,
                            "url": final_url,
                        })
                        .to_string());
                    }

                    if fetch.response.as_deref() == Some("binary") {
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
                        const BINARY_MAX_RESPONSE_SIZE: u64 = 100 * 1024 * 1024; // 100 MB (CAS limit)
                        if let Some(len) = response.content_length() {
                            if len > BINARY_MAX_RESPONSE_SIZE {
                                return Err(ExecutionError::FetchFailed {
                                    reason: format!(
                                        "Binary response too large ({} bytes, max {} bytes)",
                                        len, BINARY_MAX_RESPONSE_SIZE
                                    ),
                                }
                                .into());
                            }
                        }
                        let bytes = response.bytes().await.map_err(|e| NikaError::FetchError {
                            reason: format!("Failed to read binary response: {}", e),
                        })?;
                        // Bug 3: Post-read size check (catches chunked encoding bypass)
                        if bytes.len() as u64 > BINARY_MAX_RESPONSE_SIZE {
                            return Err(ExecutionError::FetchFailed {
                                reason: format!(
                                    "Binary response too large ({} bytes, max {} bytes)",
                                    bytes.len(),
                                    BINARY_MAX_RESPONSE_SIZE
                                ),
                            }
                            .into());
                        }
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

                    const MAX_RESPONSE_SIZE: u64 = 50 * 1024 * 1024;
                    if let Some(len) = response.content_length() {
                        if len > MAX_RESPONSE_SIZE {
                            return Err(ExecutionError::FetchFailed {
                                reason: format!(
                                    "Response too large ({} bytes, max {} bytes)",
                                    len, MAX_RESPONSE_SIZE
                                ),
                            }
                            .into());
                        }
                    }
                    // Special case: llm_txt requires sub-requests, handled here not in extract.rs
                    if fetch.extract.as_deref() == Some("llm_txt") {
                        let parsed =
                            url::Url::parse(url.as_ref()).map_err(|e| NikaError::FetchError {
                                reason: format!("Invalid URL for llm_txt: {e}"),
                            })?;
                        let origin = parsed.origin().unicode_serialization();
                        for path in &[
                            "/.well-known/llm.txt",
                            "/llm.txt",
                            "/llms.txt",
                            "/llms-full.txt",
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
                                    if let Ok(body) = resp.text().await {
                                        // Bug 10: Post-read size check (chunked bypass)
                                        if body.len() > 1_048_576 {
                                            continue;
                                        }
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

                    let raw_body = response.text().await.map_err(|e| NikaError::FetchError {
                        reason: format!("Failed to read response: {}", e),
                    })?;
                    if raw_body.len() as u64 > MAX_RESPONSE_SIZE {
                        return Err(ExecutionError::FetchFailed {
                            reason: format!(
                                "Response body too large ({} bytes, max {} bytes)",
                                raw_body.len(),
                                MAX_RESPONSE_SIZE
                            ),
                        }
                        .into());
                    }
                    let extract_result = super::extract::apply_extract(
                        &raw_body,
                        fetch.extract.as_deref(),
                        fetch.selector.as_deref(),
                    );
                    // EMIT: ExtractApplied (if an extract mode was specified)
                    if let Some(mode) = fetch.extract.as_deref() {
                        let output_len = extract_result.as_ref().map(|s| s.len()).unwrap_or(0);
                        self.event_log.emit(EventKind::ExtractApplied {
                            task_id: Arc::clone(task_id),
                            mode: mode.to_string(),
                            selector: fetch.selector.clone(),
                            input_len: raw_body.len(),
                            output_len,
                        });
                    }
                    return extract_result;
                }
                Err(e) => {
                    // Network errors are retryable
                    if attempt < effective_max_attempts {
                        // Exponential backoff calculation
                        let exp = (attempt - 1).min(30) as i32;
                        let delay_ms = backoff_ms
                            .saturating_mul(multiplier.powi(exp).min(u64::MAX as f64) as u64);

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
                            return Err(ExecutionError::FetchFailed {
                                reason: format!(
                                    "Overall fetch deadline exceeded during backoff after {} of {} attempts",
                                    attempt, effective_max_attempts,
                                ),
                            }
                            .into());
                        }
                        tokio::time::sleep(bounded_delay).await;
                        continue;
                    }

                    return Err(ExecutionError::FetchFailed {
                        reason: format!(
                            "HTTP request failed after {} attempts: {}",
                            effective_max_attempts, e
                        ),
                    }
                    .into());
                }
            }
        }

        // Should not reach here, but just in case
        Err(last_error.unwrap_or_else(|| NikaError::FetchError {
            reason: "HTTP request failed: unknown error".to_string(),
        }))
    }
}
