//! The Debug Adapter Protocol wire layer — hand-rolled, like the LSP.
//!
//! DAP framing IS LSP framing (`Content-Length: N\r\n\r\n{json}`), but
//! the envelopes differ: every message carries `seq` + `type`
//! (request/response/event); responses echo `request_seq` + `command`
//! and carry `success`. The `lsp-server` crate is JSON-RPC-specific, so
//! this module owns the ~100 lines instead of forcing a square peg.
//!
//! Generic over `Read`/`Write` — the unit tests drive the exact bytes a
//! VS Code client would send, no process spawn needed.

use std::io::{BufRead, Read as _, Write};

use serde::{Deserialize, Serialize};

/// One incoming client message (the adapter only ever RECEIVES requests).
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct Request {
    #[serde(default)]
    pub(crate) seq: i64,
    /// Always "request" from a conforming client — kept for the honest
    /// decode (a stray response/event is skipped, never a crash).
    #[serde(rename = "type")]
    pub(crate) kind: String,
    /// Defaulted: events/responses carry no `command`, and they must
    /// deserialize so the skip arm can drop them (a real stray event
    /// killing the session was the M2 finding).
    #[serde(default)]
    pub(crate) command: String,
    #[serde(default)]
    pub(crate) arguments: serde_json::Value,
}

/// The adapter's reply envelope.
#[derive(Debug, Serialize)]
pub(crate) struct Response {
    pub(crate) seq: i64,
    #[serde(rename = "type")]
    pub(crate) kind: &'static str,
    pub(crate) request_seq: i64,
    pub(crate) success: bool,
    pub(crate) command: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) message: Option<String>,
    #[serde(skip_serializing_if = "serde_json::Value::is_null")]
    pub(crate) body: serde_json::Value,
}

/// A server-pushed event (initialized · stopped · output · terminated).
#[derive(Debug, Serialize)]
pub(crate) struct EventMsg {
    pub(crate) seq: i64,
    #[serde(rename = "type")]
    pub(crate) kind: &'static str,
    pub(crate) event: &'static str,
    #[serde(skip_serializing_if = "serde_json::Value::is_null")]
    pub(crate) body: serde_json::Value,
}

/// Frame-size ceiling — a malformed/hostile `Content-Length` ends the
/// session instead of becoming an allocation.
const MAX_FRAME_BYTES: usize = 16 * 1024 * 1024;

/// Header-line ceiling — a peer streaming bytes with no newline must
/// not grow the line buffer unboundedly (`read_line` reads to \n).
const MAX_HEADER_LINE: u64 = 8 * 1024;

/// The wire: reads framed requests, writes framed responses/events,
/// owns the outgoing `seq` counter.
pub(crate) struct Wire<R, W> {
    reader: R,
    writer: W,
    next_seq: i64,
}

impl<R: BufRead, W: Write> Wire<R, W> {
    #[must_use]
    pub(crate) fn new(reader: R, writer: W) -> Self {
        Self {
            reader,
            writer,
            next_seq: 1,
        }
    }

    /// Next client REQUEST — `None` on clean EOF (client hung up).
    /// Non-request frames (a misbehaving client's responses/events) are
    /// skipped; malformed JSON ends the session (`None`) rather than
    /// guessing at a broken stream.
    pub(crate) fn read_request(&mut self) -> Option<Request> {
        loop {
            let body = self.read_frame()?;
            match serde_json::from_slice::<Request>(&body) {
                Ok(req) if req.kind == "request" => return Some(req),
                Ok(_) => {}
                Err(_) => return None,
            }
        }
    }

    /// One `Content-Length`-framed body.
    fn read_frame(&mut self) -> Option<Vec<u8>> {
        let mut content_length: Option<usize> = None;
        loop {
            let mut line = String::new();
            // `take` bounds ONE line: a newline-less stream ends the
            // session at the cap instead of growing the buffer.
            let n = self
                .reader
                .by_ref()
                .take(MAX_HEADER_LINE)
                .read_line(&mut line)
                .ok()?;
            if n == 0 {
                return None; // EOF
            }
            if n as u64 >= MAX_HEADER_LINE && !line.ends_with('\n') {
                return None; // hostile: an unterminated header line
            }
            let line = line.trim_end();
            if line.is_empty() {
                break; // header/body separator
            }
            if let Some(v) = line.strip_prefix("Content-Length:") {
                content_length = v.trim().parse().ok();
            }
        }
        let len = content_length?;
        // A hostile header (`Content-Length: 9e15`) must not become an
        // allocation. Real DAP messages are KBs; 16 MiB is generous.
        if len > MAX_FRAME_BYTES {
            return None;
        }
        let mut body = vec![0u8; len];
        self.reader.read_exact(&mut body).ok()?;
        Some(body)
    }

    pub(crate) fn respond(&mut self, req: &Request, body: serde_json::Value) {
        let msg = Response {
            seq: self.take_seq(),
            kind: "response",
            request_seq: req.seq,
            success: true,
            command: req.command.clone(),
            message: None,
            body,
        };
        self.write_frame(&serde_json::to_vec(&msg).unwrap_or_default());
    }

    pub(crate) fn reject(&mut self, req: &Request, message: &str) {
        let msg = Response {
            seq: self.take_seq(),
            kind: "response",
            request_seq: req.seq,
            success: false,
            command: req.command.clone(),
            message: Some(message.to_owned()),
            body: serde_json::Value::Null,
        };
        self.write_frame(&serde_json::to_vec(&msg).unwrap_or_default());
    }

    pub(crate) fn emit(&mut self, event: &'static str, body: serde_json::Value) {
        let msg = EventMsg {
            seq: self.take_seq(),
            kind: "event",
            event,
            body,
        };
        self.write_frame(&serde_json::to_vec(&msg).unwrap_or_default());
    }

    fn take_seq(&mut self) -> i64 {
        let seq = self.next_seq;
        self.next_seq += 1;
        seq
    }

    fn write_frame(&mut self, body: &[u8]) {
        // A dead client pipe ends the session on the next read — writes
        // never panic the adapter.
        let _ = write!(self.writer, "Content-Length: {}\r\n\r\n", body.len());
        let _ = self.writer.write_all(body);
        let _ = self.writer.flush();
    }
}

/// Encode one frame — the test-side helper for driving a session.
#[cfg(test)]
pub(crate) fn frame(json: &serde_json::Value) -> Vec<u8> {
    let body = serde_json::to_vec(json).expect("test json");
    let mut out = format!("Content-Length: {}\r\n\r\n", body.len()).into_bytes();
    out.extend(body);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_request_response_event() {
        let input = frame(&serde_json::json!({
            "seq": 1, "type": "request", "command": "initialize",
            "arguments": {"adapterID": "nika"}
        }));
        let mut out: Vec<u8> = Vec::new();
        {
            let mut wire = Wire::new(std::io::Cursor::new(input), &mut out);
            let req = wire.read_request().expect("one request");
            assert_eq!(req.command, "initialize");
            wire.respond(&req, serde_json::json!({"supportsStepBack": true}));
            wire.emit("initialized", serde_json::Value::Null);
        }
        let text = String::from_utf8(out).expect("utf8");
        // Two frames, each self-describing its length.
        assert_eq!(text.matches("Content-Length:").count(), 2);
        assert!(text.contains(r#""request_seq":1"#));
        assert!(text.contains(r#""success":true"#));
        assert!(text.contains(r#""supportsStepBack":true"#));
        assert!(text.contains(r#""event":"initialized""#));
        // The event body is Null → omitted entirely (skip_serializing_if).
        assert!(!text.contains(r#""body":null"#));
    }

    #[test]
    fn eof_and_garbage_end_the_session_cleanly() {
        let mut out: Vec<u8> = Vec::new();
        let mut wire = Wire::new(std::io::Cursor::new(Vec::<u8>::new()), &mut out);
        assert!(wire.read_request().is_none(), "EOF → None");

        let mut garbage = b"Content-Length: 9\r\n\r\nnot json!".to_vec();
        garbage.extend(frame(&serde_json::json!({
            "seq": 2, "type": "request", "command": "disconnect"
        })));
        let mut out2: Vec<u8> = Vec::new();
        let mut wire2 = Wire::new(std::io::Cursor::new(garbage), &mut out2);
        assert!(
            wire2.read_request().is_none(),
            "malformed JSON ends the session — never guess at a broken stream"
        );
    }

    #[test]
    fn a_hostile_content_length_ends_the_session_not_the_memory() {
        let hostile = b"Content-Length: 9999999999999

"
        .to_vec();
        let mut out: Vec<u8> = Vec::new();
        let mut wire = Wire::new(std::io::Cursor::new(hostile), &mut out);
        assert!(
            wire.read_request().is_none(),
            "a frame beyond MAX_FRAME_BYTES is refused, never allocated"
        );
    }

    #[test]
    fn the_wire_caps_sit_at_their_documented_boundaries() {
        // Both figures are contracts the hostile-input guards lean on —
        // arithmetic drift in a constant must not move them.
        assert_eq!(MAX_FRAME_BYTES, 16_777_216);
        assert_eq!(MAX_HEADER_LINE, 8192);
    }

    #[test]
    fn outgoing_seq_counts_one_then_two() {
        // The wire owns its OWN counter — a stubbed or regressing seq
        // would desync a real client's message correlation.
        let input = frame(&serde_json::json!({
            "seq": 7, "type": "request", "command": "threads"
        }));
        let mut out: Vec<u8> = Vec::new();
        {
            let mut wire = Wire::new(std::io::Cursor::new(input), &mut out);
            let req = wire.read_request().expect("one request");
            wire.respond(&req, serde_json::Value::Null);
            wire.emit("stopped", serde_json::Value::Null);
        }
        let text = String::from_utf8(out).expect("utf8");
        assert!(text.contains(r#""seq":1"#), "first frame: {text}");
        assert!(text.contains(r#""seq":2"#), "second frame: {text}");
    }

    #[test]
    fn a_header_line_ending_exactly_at_the_cap_is_legal() {
        // 8191 junk bytes + '\n': the capped read returns exactly
        // MAX_HEADER_LINE bytes but the line IS terminated — only the
        // UNTERMINATED case is hostile, and the guard needs both facts.
        let mut input = vec![b'X'; 8191];
        input.push(b'\n');
        input.extend(frame(&serde_json::json!({
            "seq": 1, "type": "request", "command": "threads"
        })));
        let mut out: Vec<u8> = Vec::new();
        let mut wire = Wire::new(std::io::Cursor::new(input), &mut out);
        assert_eq!(
            wire.read_request()
                .expect("the frame after the long-but-terminated line")
                .command,
            "threads"
        );
    }

    #[test]
    fn a_frame_at_the_cap_reads_and_one_byte_over_refuses() {
        let json = br#"{"seq":1,"type":"request","command":"threads"}"#;
        let mut body = json.to_vec();
        body.resize(MAX_FRAME_BYTES, b' '); // trailing whitespace is legal JSON
        let mut input = format!("Content-Length: {}\r\n\r\n", body.len()).into_bytes();
        input.extend(&body);
        let mut out: Vec<u8> = Vec::new();
        let mut wire = Wire::new(std::io::Cursor::new(input), &mut out);
        assert_eq!(
            wire.read_request().expect("at-cap frame reads").command,
            "threads"
        );

        let mut body = json.to_vec();
        body.resize(MAX_FRAME_BYTES + 1, b' ');
        let mut input = format!("Content-Length: {}\r\n\r\n", body.len()).into_bytes();
        input.extend(&body);
        let mut out: Vec<u8> = Vec::new();
        let mut wire = Wire::new(std::io::Cursor::new(input), &mut out);
        assert!(
            wire.read_request().is_none(),
            "one byte past the cap is refused before the allocation"
        );
    }

    #[test]
    fn non_request_frames_are_skipped() {
        // A REAL stray event — no command field (the doctored fixture
        // used to smuggle one in, masking the M2 session-kill).
        let mut input = frame(&serde_json::json!({
            "seq": 9, "type": "event", "event": "echo"
        }));
        input.extend(frame(&serde_json::json!({
            "seq": 3, "type": "request", "command": "threads"
        })));
        let mut out: Vec<u8> = Vec::new();
        let mut wire = Wire::new(std::io::Cursor::new(input), &mut out);
        let req = wire
            .read_request()
            .expect("the request after the stray event");
        assert_eq!(req.command, "threads");
    }
}
