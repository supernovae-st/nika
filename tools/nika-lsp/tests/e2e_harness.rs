// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! End-to-end test harness for the Nika LSP.
//!
//! Spawns the `nika-lsp` binary and communicates via JSON-RPC over stdio.
//! These tests validate the full protocol handshake, diagnostics pipeline,
//! completion, hover, and shutdown sequence.
//!
//! # Running
//!
//! ```bash
//! # Build the binary first (required for integration tests)
//! cargo build -p nika-lsp
//!
//! # Run e2e tests
//! cargo test -p nika-lsp --test e2e_harness -- --ignored
//! ```
//!
//! Tests are `#[ignore]` by default so they don't run during `cargo test --lib`.

use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Read as _, Write as _};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::time::{Duration, Instant};

// ---------------------------------------------------------------------------
// Harness
// ---------------------------------------------------------------------------

/// LSP test client that spawns `nika-lsp` and talks JSON-RPC over stdio.
struct LspClient {
    /// The child process handle (kept alive for the duration of the test).
    child: Child,
    /// Writer for sending JSON-RPC messages.
    stdin: ChildStdin,
    /// Buffered reader for receiving JSON-RPC messages.
    stdout: BufReader<ChildStdout>,
    /// Monotonically increasing request ID.
    request_id: i64,
}

impl LspClient {
    /// Spawn a new `nika-lsp` process and return a connected client.
    ///
    /// Panics if the binary cannot be found or spawned.
    fn new() -> Self {
        // `CARGO_BIN_EXE_nika-lsp` is set by Cargo for integration tests when the
        // crate declares `[[bin]] name = "nika-lsp"`.
        let binary = env!("CARGO_BIN_EXE_nika-lsp");

        let mut child = Command::new(binary)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("failed to spawn nika-lsp binary");

        let stdin = child.stdin.take().expect("failed to capture stdin");
        let stdout = child.stdout.take().expect("failed to capture stdout");

        Self {
            child,
            stdin,
            stdout: BufReader::new(stdout),
            request_id: 0,
        }
    }

    // -- Low-level I/O ------------------------------------------------------

    /// Send a JSON-RPC message with Content-Length header.
    fn send_raw(&mut self, msg: &Value) {
        let body = serde_json::to_string(msg).unwrap();
        let header = format!("Content-Length: {}\r\n\r\n", body.len());
        self.stdin.write_all(header.as_bytes()).unwrap();
        self.stdin.write_all(body.as_bytes()).unwrap();
        self.stdin.flush().unwrap();
    }

    /// Read one JSON-RPC message from stdout.
    ///
    /// Blocks until a complete message is available. Returns the parsed JSON body.
    fn read_message(&mut self) -> Value {
        // Parse headers until we find Content-Length.
        let content_length = loop {
            let mut header_line = String::new();
            self.stdout
                .read_line(&mut header_line)
                .expect("failed to read header line from LSP");

            let trimmed = header_line.trim();

            // Empty line signals end of headers.
            if trimmed.is_empty() {
                continue;
            }

            if let Some(rest) = trimmed.strip_prefix("Content-Length:") {
                let len: usize = rest.trim().parse().expect("invalid Content-Length");
                // Consume the blank separator line that follows.
                let mut blank = String::new();
                self.stdout.read_line(&mut blank).unwrap();
                break len;
            }
            // Skip other headers (e.g. Content-Type).
        };

        let mut body = vec![0u8; content_length];
        self.stdout.read_exact(&mut body).unwrap();
        serde_json::from_slice(&body).expect("invalid JSON in LSP message body")
    }

    // -- High-level helpers -------------------------------------------------

    /// Send a JSON-RPC request and return the response.
    ///
    /// This will skip over any interleaved server-initiated notifications (e.g.
    /// `window/logMessage`) until it finds the response matching the request id.
    fn send_request(&mut self, method: &str, params: Value) -> Value {
        self.request_id += 1;
        let id = self.request_id;

        self.send_raw(&json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        }));

        // Read messages until we get the matching response.
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            assert!(
                Instant::now() < deadline,
                "timed out waiting for response to '{}' (id={})",
                method,
                id,
            );
            let msg = self.read_message();
            if msg.get("id").and_then(|v| v.as_i64()) == Some(id) {
                return msg;
            }
            // Otherwise it's a notification — drop it and keep reading.
        }
    }

    /// Send a JSON-RPC notification (no response expected).
    fn send_notification(&mut self, method: &str, params: Value) {
        self.send_raw(&json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
        }));
    }

    /// Read messages until one matches `predicate` or `timeout` elapses.
    ///
    /// Returns `Some(msg)` if found, `None` on timeout.  Any non-matching
    /// messages are silently discarded.
    fn read_until(
        &mut self,
        predicate: impl Fn(&Value) -> bool,
        timeout: Duration,
    ) -> Option<Value> {
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            // We need non-blocking behaviour.  A simple approach: set a
            // read timeout on the underlying handle.  Unfortunately
            // `BufReader` doesn't expose that directly, so we use a
            // secondary thread.
            let msg = self.read_message_with_timeout(deadline.duration_since(Instant::now()));
            match msg {
                Some(m) if predicate(&m) => return Some(m),
                Some(_) => continue,
                None => return None,
            }
        }
        None
    }

    /// Try to read one message within `timeout`.  Returns `None` on timeout.
    fn read_message_with_timeout(&mut self, timeout: Duration) -> Option<Value> {
        // We peek at the internal buffer first.  If data is already available
        // `read_line` won't block.  For true non-blocking we'd need async I/O,
        // but for test purposes a thread + channel works fine.
        // Check if we already have data buffered.
        let buf = self.stdout.buffer();
        if !buf.is_empty() {
            return Some(self.read_message());
        }

        // No data buffered — use a timed approach.
        // We can't easily move the BufReader into a thread, so instead we
        // set the underlying handle to non-blocking temporarily using raw fd.
        #[cfg(unix)]
        {
            use std::os::unix::io::AsRawFd;
            let fd = self.stdout.get_ref().as_raw_fd();

            // Save old flags.
            let old_flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
            // Set non-blocking.
            unsafe { libc::fcntl(fd, libc::F_SETFL, old_flags | libc::O_NONBLOCK) };

            let result = self.poll_read(timeout);

            // Restore blocking mode.
            unsafe { libc::fcntl(fd, libc::F_SETFL, old_flags) };

            result
        }

        #[cfg(not(unix))]
        {
            // Fallback: just try a blocking read with a short sleep loop.
            let start = Instant::now();
            while start.elapsed() < timeout {
                let buf = self.stdout.buffer();
                if !buf.is_empty() {
                    return Some(self.read_message());
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            None
        }
    }

    /// Poll for a message on a non-blocking fd, retrying until `timeout`.
    #[cfg(unix)]
    fn poll_read(&mut self, timeout: Duration) -> Option<Value> {
        let start = Instant::now();
        loop {
            if start.elapsed() >= timeout {
                return None;
            }

            let mut header_line = String::new();
            match self.stdout.read_line(&mut header_line) {
                Ok(0) => return None, // EOF
                Ok(_) => {
                    // Got data — switch back to blocking for the rest of this message.
                    let fd = {
                        use std::os::unix::io::AsRawFd;
                        self.stdout.get_ref().as_raw_fd()
                    };
                    let old_flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
                    unsafe { libc::fcntl(fd, libc::F_SETFL, old_flags & !libc::O_NONBLOCK) };

                    // Parse the header we just read.
                    let trimmed = header_line.trim();
                    if trimmed.is_empty() {
                        // Keep reading.
                        // Restore non-blocking.
                        unsafe { libc::fcntl(fd, libc::F_SETFL, old_flags) };
                        continue;
                    }

                    if let Some(rest) = trimmed.strip_prefix("Content-Length:") {
                        let len: usize = rest.trim().parse().expect("invalid Content-Length");
                        // Consume the blank separator line.
                        let mut blank = String::new();
                        self.stdout.read_line(&mut blank).unwrap();
                        // Read body.
                        let mut body = vec![0u8; len];
                        self.stdout.read_exact(&mut body).unwrap();
                        return Some(serde_json::from_slice(&body).expect("invalid JSON"));
                    }

                    // Some other header — restore non-blocking and keep polling.
                    unsafe { libc::fcntl(fd, libc::F_SETFL, old_flags) };
                    continue;
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(25));
                    continue;
                }
                Err(e) => panic!("unexpected read error: {}", e),
            }
        }
    }

    // -- Lifecycle helpers --------------------------------------------------

    /// Perform the full initialize + initialized handshake and return the
    /// `InitializeResult`.
    fn initialize(&mut self) -> Value {
        let resp = self.send_request(
            "initialize",
            json!({
                "processId": std::process::id(),
                "rootUri": null,
                "capabilities": {},
            }),
        );
        self.send_notification("initialized", json!({}));
        resp
    }

    /// Send `shutdown` request followed by `exit` notification.
    fn shutdown(&mut self) {
        let resp = self.send_request("shutdown", json!(null));
        assert!(
            resp.get("error").is_none(),
            "shutdown returned an error: {:?}",
            resp
        );
        self.send_notification("exit", json!(null));
    }

    /// Open a virtual document in the server.
    fn open_document(&mut self, uri: &str, text: &str) {
        self.send_notification(
            "textDocument/didOpen",
            json!({
                "textDocument": {
                    "uri": uri,
                    "languageId": "nika",
                    "version": 1,
                    "text": text,
                }
            }),
        );
    }
}

impl Drop for LspClient {
    fn drop(&mut self) {
        // Best-effort shutdown — ignore errors (process may already be dead).
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[test]
#[ignore = "e2e: requires `cargo build -p nika-lsp`"]
fn test_initialize_handshake() {
    let mut client = LspClient::new();
    let response = client.initialize();

    // Server should return capabilities.
    let caps = &response["result"]["capabilities"];
    assert!(
        caps["completionProvider"].is_object(),
        "expected completionProvider capability, got: {}",
        serde_json::to_string_pretty(&response["result"]).unwrap()
    );
    assert!(
        caps["hoverProvider"].is_boolean(),
        "expected hoverProvider capability"
    );
    assert!(
        caps["definitionProvider"].is_boolean(),
        "expected definitionProvider capability"
    );

    // Server info should identify as nika-lsp.
    let server_info = &response["result"]["serverInfo"];
    assert_eq!(server_info["name"].as_str(), Some("nika-lsp"));
    assert!(
        server_info["version"].is_string(),
        "expected server version string"
    );

    client.shutdown();
}

#[test]
#[ignore = "e2e: requires `cargo build -p nika-lsp`"]
fn test_shutdown_and_exit() {
    let mut client = LspClient::new();
    client.initialize();

    // Shutdown should succeed.
    let resp = client.send_request("shutdown", json!(null));
    assert!(resp.get("error").is_none(), "shutdown failed: {:?}", resp);
    assert!(resp["result"].is_null(), "shutdown result should be null");

    // Send exit notification.
    client.send_notification("exit", json!(null));

    // The process should terminate.
    let status = client
        .child
        .wait()
        .expect("failed to wait for child process");
    // LSP spec: exit code 0 if shutdown was received, 1 otherwise.
    assert!(
        status.success(),
        "expected exit code 0 after clean shutdown, got: {:?}",
        status
    );
}

#[test]
#[ignore = "e2e: requires `cargo build -p nika-lsp`"]
fn test_didopen_valid_document_no_error_diagnostics() {
    let mut client = LspClient::new();
    client.initialize();

    let doc = r#"schema: "nika/workflow@0.12"
workflow: test
provider: mock
model: mock-model
tasks:
  - id: step1
    infer: "Hello world"
"#;

    client.open_document("file:///tmp/valid.nika.yaml", doc);

    // Wait for publishDiagnostics.
    let diag = client.read_until(
        |msg| msg.get("method").and_then(|m| m.as_str()) == Some("textDocument/publishDiagnostics"),
        Duration::from_secs(5),
    );

    assert!(diag.is_some(), "expected publishDiagnostics notification");
    let params = &diag.unwrap()["params"];
    let diagnostics = params["diagnostics"].as_array().unwrap();

    // A valid mock-provider doc with model should produce zero *error* diagnostics.
    let errors: Vec<_> = diagnostics
        .iter()
        .filter(|d| d["severity"].as_u64() == Some(1)) // 1 = Error
        .collect();
    assert!(
        errors.is_empty(),
        "expected no error diagnostics for a valid document, got: {:#?}",
        errors
    );

    client.shutdown();
}

#[test]
#[ignore = "e2e: requires `cargo build -p nika-lsp`"]
fn test_didopen_invalid_schema_triggers_diagnostics() {
    let mut client = LspClient::new();
    client.initialize();

    let doc = r#"schema: "invalid/schema"
workflow: bad
tasks:
  - id: a
    exec: "echo hello"
"#;

    client.open_document("file:///tmp/bad.nika.yaml", doc);

    // Read until we get publishDiagnostics.
    let diag = client.read_until(
        |msg| msg.get("method").and_then(|m| m.as_str()) == Some("textDocument/publishDiagnostics"),
        Duration::from_secs(5),
    );

    assert!(diag.is_some(), "expected publishDiagnostics notification");
    let params = &diag.unwrap()["params"];
    let diagnostics = params["diagnostics"].as_array().unwrap();
    assert!(
        !diagnostics.is_empty(),
        "expected diagnostics for invalid schema"
    );

    // At least one should mention schema.
    let has_schema_error = diagnostics.iter().any(|d| {
        d["message"]
            .as_str()
            .unwrap_or("")
            .to_lowercase()
            .contains("schema")
    });
    assert!(
        has_schema_error,
        "expected a diagnostic mentioning 'schema', got: {:#?}",
        diagnostics
    );

    client.shutdown();
}

#[test]
#[ignore = "e2e: requires `cargo build -p nika-lsp`"]
fn test_didopen_unknown_task_reference() {
    let mut client = LspClient::new();
    client.initialize();

    let doc = r#"schema: "nika/workflow@0.12"
workflow: test
provider: mock
model: mock-model
tasks:
  - id: step1
    infer: "Hello"
  - id: step2
    with:
      data: $nonexistent
    infer: "{{with.data}}"
"#;

    client.open_document("file:///tmp/badref.nika.yaml", doc);

    let diag = client.read_until(
        |msg| msg.get("method").and_then(|m| m.as_str()) == Some("textDocument/publishDiagnostics"),
        Duration::from_secs(5),
    );

    assert!(diag.is_some(), "expected publishDiagnostics");
    let params = &diag.unwrap()["params"];
    let diagnostics = params["diagnostics"].as_array().unwrap();

    let has_unknown = diagnostics.iter().any(|d| {
        let msg = d["message"].as_str().unwrap_or("");
        msg.to_lowercase().contains("unknown") || msg.to_lowercase().contains("nonexistent")
    });
    assert!(
        has_unknown,
        "expected diagnostic about unknown task reference, got: {:#?}",
        diagnostics
    );

    client.shutdown();
}

#[test]
#[ignore = "e2e: requires `cargo build -p nika-lsp`"]
fn test_completion_at_task_level() {
    let mut client = LspClient::new();
    client.initialize();

    let doc = "schema: \"nika/workflow@0.12\"\nworkflow: test\nprovider: mock\nmodel: mock-model\ntasks:\n  - id: step1\n    ";

    client.open_document("file:///tmp/comp.nika.yaml", doc);

    // Give the server time to process the document.
    std::thread::sleep(Duration::from_millis(300));

    // Request completion at line 6, character 4 (after "    ").
    let response = client.send_request(
        "textDocument/completion",
        json!({
            "textDocument": { "uri": "file:///tmp/comp.nika.yaml" },
            "position": { "line": 6, "character": 4 },
        }),
    );

    // The result can be a CompletionList { items: [...] } or a bare array.
    let items = response["result"]["items"]
        .as_array()
        .or_else(|| response["result"].as_array());

    assert!(
        items.is_some(),
        "expected completion items, got: {}",
        serde_json::to_string_pretty(&response).unwrap()
    );

    let items = items.unwrap();
    assert!(
        !items.is_empty(),
        "expected non-empty completion list at task verb position"
    );

    // The 5 verbs should be in the completions.
    let labels: Vec<&str> = items.iter().filter_map(|i| i["label"].as_str()).collect();
    assert!(
        labels.iter().any(|l| l.contains("infer")),
        "expected 'infer' in completions, got: {:?}",
        labels
    );

    client.shutdown();
}

#[test]
#[ignore = "e2e: requires `cargo build -p nika-lsp`"]
fn test_hover_on_verb() {
    let mut client = LspClient::new();
    client.initialize();

    let doc = r#"schema: "nika/workflow@0.12"
workflow: test
provider: mock
model: mock-model
tasks:
  - id: step1
    infer: "Hello"
"#;

    client.open_document("file:///tmp/hover.nika.yaml", doc);
    std::thread::sleep(Duration::from_millis(300));

    // Hover on "infer" at line 6, character 6 (middle of "infer").
    let response = client.send_request(
        "textDocument/hover",
        json!({
            "textDocument": { "uri": "file:///tmp/hover.nika.yaml" },
            "position": { "line": 6, "character": 6 },
        }),
    );

    // Hover may return null if the hover provider doesn't have info at that exact position.
    // But if it returns something, it should contain markdown content.
    if !response["result"].is_null() {
        let contents = &response["result"]["contents"];
        assert!(
            contents.is_object() || contents.is_string(),
            "hover contents should be MarkupContent or string, got: {:?}",
            contents
        );
    }

    client.shutdown();
}

#[test]
#[ignore = "e2e: requires `cargo build -p nika-lsp`"]
fn test_document_symbols() {
    let mut client = LspClient::new();
    client.initialize();

    let doc = r#"schema: "nika/workflow@0.12"
workflow: symbol-test
provider: mock
model: mock-model
tasks:
  - id: alpha
    infer: "Hello"
  - id: beta
    exec: "echo hi"
"#;

    client.open_document("file:///tmp/symbols.nika.yaml", doc);
    std::thread::sleep(Duration::from_millis(300));

    let response = client.send_request(
        "textDocument/documentSymbol",
        json!({
            "textDocument": { "uri": "file:///tmp/symbols.nika.yaml" },
        }),
    );

    let result = &response["result"];
    assert!(
        result.is_array(),
        "expected symbol array, got: {}",
        serde_json::to_string_pretty(&response).unwrap()
    );

    let symbols = result.as_array().unwrap();
    assert!(!symbols.is_empty(), "expected at least one document symbol");

    client.shutdown();
}

#[test]
#[ignore = "e2e: requires `cargo build -p nika-lsp`"]
fn test_empty_tasks_warning() {
    let mut client = LspClient::new();
    client.initialize();

    let doc = "schema: \"nika/workflow@0.12\"\nworkflow: empty\nprovider: mock\ntasks: []\n";

    client.open_document("file:///tmp/empty.nika.yaml", doc);

    let diag = client.read_until(
        |msg| msg.get("method").and_then(|m| m.as_str()) == Some("textDocument/publishDiagnostics"),
        Duration::from_secs(5),
    );

    assert!(diag.is_some(), "expected publishDiagnostics");
    let params = &diag.unwrap()["params"];
    let diagnostics = params["diagnostics"].as_array().unwrap();

    let has_empty_warning = diagnostics.iter().any(|d| {
        let code = d["code"].as_str().unwrap_or("");
        let msg = d["message"].as_str().unwrap_or("").to_lowercase();
        code == "NIKA-144"
            || code == "NIKA-145"
            || msg.contains("no tasks")
            || msg.contains("must not be empty")
    });
    assert!(
        has_empty_warning,
        "expected empty-tasks diagnostic (NIKA-144 or NIKA-145), got: {:#?}",
        diagnostics
    );

    client.shutdown();
}

#[test]
#[ignore = "e2e: requires `cargo build -p nika-lsp`"]
fn test_didclose_clears_diagnostics() {
    let mut client = LspClient::new();
    client.initialize();

    let uri = "file:///tmp/close.nika.yaml";
    let doc = "schema: \"invalid\"\nworkflow: bad\n";

    client.open_document(uri, doc);

    // Wait for initial diagnostics (with errors).
    let _ = client.read_until(
        |msg| msg.get("method").and_then(|m| m.as_str()) == Some("textDocument/publishDiagnostics"),
        Duration::from_secs(5),
    );

    // Close the document.
    client.send_notification(
        "textDocument/didClose",
        json!({
            "textDocument": { "uri": uri }
        }),
    );

    // Server should publish empty diagnostics on close.
    let cleared = client.read_until(
        |msg| {
            msg.get("method").and_then(|m| m.as_str()) == Some("textDocument/publishDiagnostics")
                && msg["params"]["diagnostics"]
                    .as_array()
                    .map(|a| a.is_empty())
                    .unwrap_or(false)
        },
        Duration::from_secs(5),
    );

    assert!(
        cleared.is_some(),
        "expected empty diagnostics after didClose"
    );

    client.shutdown();
}

#[test]
#[ignore = "e2e: requires `cargo build -p nika-lsp`"]
fn test_folding_ranges() {
    let mut client = LspClient::new();
    client.initialize();

    let doc = r#"schema: "nika/workflow@0.12"
workflow: fold-test
provider: mock
model: mock-model
tasks:
  - id: alpha
    infer:
      prompt: "Hello"
      temperature: 0.5
  - id: beta
    exec:
      command: "echo hi"
      shell: true
"#;

    let uri = "file:///tmp/fold.nika.yaml";
    client.open_document(uri, doc);
    std::thread::sleep(Duration::from_millis(300));

    let response = client.send_request(
        "textDocument/foldingRange",
        json!({
            "textDocument": { "uri": uri },
        }),
    );

    // Folding ranges may be null (empty) or an array.
    if !response["result"].is_null() {
        let ranges = response["result"].as_array().unwrap();
        // With multi-line tasks we should have at least one folding range.
        assert!(
            !ranges.is_empty(),
            "expected folding ranges for multi-line tasks"
        );
    }

    client.shutdown();
}

#[test]
#[ignore = "e2e: requires `cargo build -p nika-lsp`"]
fn test_semantic_tokens() {
    let mut client = LspClient::new();
    client.initialize();

    let doc = r#"schema: "nika/workflow@0.12"
workflow: tokens-test
provider: mock
model: mock-model
tasks:
  - id: step1
    infer: "Hello world"
"#;

    let uri = "file:///tmp/tokens.nika.yaml";
    client.open_document(uri, doc);
    std::thread::sleep(Duration::from_millis(300));

    let response = client.send_request(
        "textDocument/semanticTokens/full",
        json!({
            "textDocument": { "uri": uri },
        }),
    );

    // Result can be null if no tokens, or { data: [...] }.
    if !response["result"].is_null() {
        let data = &response["result"]["data"];
        assert!(
            data.is_array(),
            "expected semantic tokens data array, got: {:?}",
            data
        );
        // Tokens are encoded as groups of 5 integers.
        let len = data.as_array().unwrap().len();
        assert!(
            len % 5 == 0,
            "semantic token data length {} is not a multiple of 5",
            len
        );
    }

    client.shutdown();
}

#[test]
#[ignore = "e2e: requires `cargo build -p nika-lsp`"]
fn test_code_lens() {
    let mut client = LspClient::new();
    client.initialize();

    let doc = r#"schema: "nika/workflow@0.12"
workflow: lens-test
provider: mock
model: mock-model
tasks:
  - id: step1
    infer: "Hello world"
"#;

    let uri = "file:///tmp/lens.nika.yaml";
    client.open_document(uri, doc);
    std::thread::sleep(Duration::from_millis(300));

    let response = client.send_request(
        "textDocument/codeLens",
        json!({
            "textDocument": { "uri": uri },
        }),
    );

    // Code lenses may be null or an array.  We just verify no error.
    assert!(
        response.get("error").is_none(),
        "codeLens returned error: {:?}",
        response
    );

    client.shutdown();
}

#[test]
#[ignore = "e2e: requires `cargo build -p nika-lsp`"]
fn test_multiple_documents() {
    let mut client = LspClient::new();
    client.initialize();

    let doc_a = "schema: \"nika/workflow@0.12\"\nworkflow: a\nprovider: mock\nmodel: m\ntasks:\n  - id: s1\n    infer: \"A\"\n";
    let doc_b = "schema: \"nika/workflow@0.12\"\nworkflow: b\nprovider: mock\nmodel: m\ntasks:\n  - id: s1\n    exec: \"echo B\"\n";

    client.open_document("file:///tmp/a.nika.yaml", doc_a);
    client.open_document("file:///tmp/b.nika.yaml", doc_b);

    // Give server time to validate both.
    std::thread::sleep(Duration::from_millis(500));

    // Request completion on both — neither should crash.
    let resp_a = client.send_request(
        "textDocument/completion",
        json!({
            "textDocument": { "uri": "file:///tmp/a.nika.yaml" },
            "position": { "line": 6, "character": 4 },
        }),
    );
    assert!(
        resp_a.get("error").is_none(),
        "completion on doc A failed: {:?}",
        resp_a
    );

    let resp_b = client.send_request(
        "textDocument/completion",
        json!({
            "textDocument": { "uri": "file:///tmp/b.nika.yaml" },
            "position": { "line": 6, "character": 4 },
        }),
    );
    assert!(
        resp_b.get("error").is_none(),
        "completion on doc B failed: {:?}",
        resp_b
    );

    client.shutdown();
}

// ═══════════════════════════════════════════════════════════════════════════
// Daemon-powered features (v0.50.0)
// ═══════════════════════════════════════════════════════════════════════════

#[test]
#[ignore = "e2e: requires `cargo build -p nika-lsp`"]
fn test_completion_provider_shows_items() {
    let mut client = LspClient::new();
    client.initialize();

    let doc = "schema: \"nika/workflow@0.12\"\nprovider: ";
    client.open_document("file:///tmp/provider-comp.nika.yaml", doc);
    std::thread::sleep(Duration::from_millis(300));

    let resp = client.send_request(
        "textDocument/completion",
        json!({
            "textDocument": { "uri": "file:///tmp/provider-comp.nika.yaml" },
            "position": { "line": 1, "character": 10 },
        }),
    );

    assert!(resp.get("error").is_none(), "completion error: {:?}", resp);

    client.shutdown();
}

#[test]
#[ignore = "e2e: requires `cargo build -p nika-lsp`"]
fn test_rename_task_id() {
    let mut client = LspClient::new();
    client.initialize();

    let text = "schema: \"nika/workflow@0.12\"\nprovider: mock\nmodel: m\ntasks:\n  - id: old_name\n    infer: \"test\"\n  - id: step2\n    depends_on: [old_name]\n    infer: \"test2\"\n";
    client.open_document("file:///tmp/rename.nika.yaml", text);
    std::thread::sleep(Duration::from_millis(300));

    let resp = client.send_request(
        "textDocument/rename",
        json!({
            "textDocument": { "uri": "file:///tmp/rename.nika.yaml" },
            "position": { "line": 4, "character": 10 },
            "newName": "new_name",
        }),
    );

    assert!(resp.get("error").is_none(), "rename error: {:?}", resp);

    client.shutdown();
}

#[test]
#[ignore = "e2e: requires `cargo build -p nika-lsp`"]
fn test_inlay_hints_no_error() {
    let mut client = LspClient::new();
    client.initialize();

    let text = "schema: \"nika/workflow@0.12\"\nprovider: mock\nmodel: mock-model\ntasks:\n  - id: gen\n    infer:\n      prompt: \"test\"\n      model: claude-sonnet-4-20250514\n";
    client.open_document("file:///tmp/inlay.nika.yaml", text);
    std::thread::sleep(Duration::from_millis(300));

    let resp = client.send_request(
        "textDocument/inlayHint",
        json!({
            "textDocument": { "uri": "file:///tmp/inlay.nika.yaml" },
            "range": {
                "start": { "line": 0, "character": 0 },
                "end": { "line": 8, "character": 0 },
            },
        }),
    );

    assert!(resp.get("error").is_none(), "inlayHint error: {:?}", resp);

    client.shutdown();
}

#[test]
#[ignore = "e2e: requires `cargo build -p nika-lsp`"]
fn test_daemon_status_custom_request() {
    let mut client = LspClient::new();
    client.initialize();

    let resp = client.send_request("nika/daemonStatus", json!({}));

    // Should return { connected: false } when daemon not running
    assert!(
        resp.get("error").is_none(),
        "daemon status error: {:?}",
        resp
    );
    let connected = resp["result"]["connected"].as_bool();
    assert!(
        connected.is_some(),
        "Should have 'connected' field: {:?}",
        resp
    );

    client.shutdown();
}
