//! `nika dap` — the Debug Adapter Protocol server (replay sessions).
//!
//! Session model (the research-locked blueprint): the MVP is a
//! READ-ONLY replay debugger over a recorded run journal — launch args
//! carry `{workflow, replay}`, breakpoints map to task lines, stepping
//! walks task settles, `stepBack` is free because the log is total.
//! Live sessions (breakpoint gates on the durable-pause substrate) come
//! after replay proves the wiring.
//!
//! Lifecycle (the spec's launch sequencing): initialize → capabilities
//! response → `initialized` event → setBreakpoints → configurationDone
//! → stopped/continue loop → disconnect.

mod protocol;

use protocol::Wire;

/// Serve one DAP session over stdio. Like `nika lsp`, stdout belongs to
/// the protocol — diagnostics go to stderr only.
#[must_use]
pub fn run_stdio() -> u8 {
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    let wire = Wire::new(stdin.lock(), stdout.lock());
    serve(wire)
}

/// The session loop — generic over the wire so tests drive exact bytes.
fn serve<R: std::io::BufRead, W: std::io::Write>(mut wire: Wire<R, W>) -> u8 {
    while let Some(req) = wire.read_request() {
        match req.command.as_str() {
            "initialize" => {
                // Capabilities: absence = unsupported (the spec's rule) —
                // claim ONLY what the replay session actually serves.
                wire.respond(
                    &req,
                    serde_json::json!({
                        "supportsConfigurationDoneRequest": true,
                        "supportsStepBack": true,
                    }),
                );
                wire.emit("initialized", serde_json::Value::Null);
            }
            "disconnect" => {
                wire.respond(&req, serde_json::Value::Null);
                return 0;
            }
            // The replay session lands next — until then every other
            // request is honestly refused (VS Code surfaces the message).
            _ => wire.reject(&req, "nika dap: replay sessions are not wired yet"),
        }
    }
    0
}

#[cfg(test)]
mod tests {
    use super::protocol::frame;
    use super::*;

    fn drive(messages: &[serde_json::Value]) -> String {
        let mut input: Vec<u8> = Vec::new();
        for m in messages {
            input.extend(frame(m));
        }
        let mut out: Vec<u8> = Vec::new();
        let code = serve(Wire::new(std::io::Cursor::new(input), &mut out));
        assert_eq!(code, 0);
        String::from_utf8(out).expect("utf8")
    }

    #[test]
    fn handshake_then_disconnect() {
        let out = drive(&[
            serde_json::json!({"seq": 1, "type": "request", "command": "initialize",
                               "arguments": {"adapterID": "nika"}}),
            serde_json::json!({"seq": 2, "type": "request", "command": "disconnect"}),
        ]);
        assert!(out.contains(r#""supportsStepBack":true"#));
        assert!(out.contains(r#""event":"initialized""#));
        // The disconnect response closes the session (loop returned).
        assert!(out.contains(r#""request_seq":2"#));
    }

    #[test]
    fn unwired_requests_are_refused_not_ignored() {
        let out = drive(&[
            serde_json::json!({"seq": 1, "type": "request", "command": "launch",
                               "arguments": {"replay": "x.ndjson"}}),
            serde_json::json!({"seq": 2, "type": "request", "command": "disconnect"}),
        ]);
        assert!(out.contains(r#""success":false"#));
        assert!(out.contains("not wired yet"));
    }
}
