# Quick Wins Implementation Plan

> TDD Approach: RED → GREEN → REFACTOR

## Overview

Implement 3 quick wins to bring nika-cli from 2/7 to 5/7 functional keywords.

**STATUS: ✅ COMPLETED (2025-12-22)**

## Tasks

### 1. Fix execute_shell blocking (P0 - Critical) ✅

**Problem:** `execute_shell` in runner.rs uses blocking operations in async context.

**Solution:** Wrap subprocess operations in `tokio::task::spawn_blocking`.

**Implementation:**
- [x] Made execute_shell async
- [x] Wrapped Command::spawn and wait in spawn_blocking
- [x] All 6 shell tests pass

### 2. Implement llm: keyword (P1 - High) ✅

**Problem:** Currently returns stub `"[llm] Would execute..."`.

**Solution:** Actually call provider with one-shot request (no history, isolated).

**Implementation:**
- [x] Implemented real provider call with `PromptRequest::new().isolated()`
- [x] Uses haiku as default model (cheap, fast)
- [x] Returns actual LLM response

### 3. Implement http: keyword (P1 - High) ✅

**Problem:** Currently returns stub `"[http] Would GET url"`.

**Solution:** Use reqwest to make actual HTTP calls.

**Implementation:**
- [x] Full reqwest HTTP client integration
- [x] Support for all common HTTP methods (GET, POST, PUT, DELETE, PATCH, HEAD)
- [x] Template resolution for URL and headers
- [x] JSON body support

## Security Considerations ✅

- [x] HTTP: Block localhost/internal IPs (SSRF) - `validate_http_url()`
- [x] HTTP: Validate URL scheme (http/https only)
- [x] HTTP: Set reasonable timeout (30s request, 10s connect)
- [x] HTTP: Limit response size (10MB max, truncated display)
- [x] HTTP: Block cloud metadata endpoints (169.254.169.254, *.internal)
- [x] HTTP: Block private IP ranges (10.x, 172.16-31.x, 192.168.x, 127.x, 169.254.x)
- [x] Shell: Already has timeout (30s default)

## Verification Checklist ✅

- [x] `cargo test` - 215 tests pass
- [x] `cargo clippy -- -D warnings` - No warnings
- [x] `cargo fmt --check` - Properly formatted
- [x] 5 SSRF security tests added and passing
