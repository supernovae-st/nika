//! SSE (Server-Sent Events) wire format parser.
//!
//! Parses the `event:\ndata:\n\n` format from nika-serve into typed
//! `Event` values. Used by `RemoteTransport` to consume the
//! `/v1/events/{id}` SSE stream.

use crate::error::SdkError;
use crate::types::Event;

/// A single parsed SSE frame.
#[derive(Debug, PartialEq)]
pub(crate) enum SseFrame {
    /// A typed event with data payload.
    Event { event_type: String, data: String },
    /// A keepalive comment (`: ping`).
    Comment(String),
}

/// Parse a complete SSE text block (lines terminated by `\n\n`) into frames.
///
/// Each frame consists of `event:` and `data:` lines. Lines starting with `:`
/// are comments (keepalive). Empty lines delimit frames.
pub(crate) fn parse_sse_block(block: &str) -> Vec<SseFrame> {
    let mut frames = Vec::new();
    let mut event_type: Option<String> = None;
    let mut data_lines: Vec<String> = Vec::new();

    for line in block.lines() {
        if line.is_empty() {
            // Empty line = end of frame
            if !data_lines.is_empty() {
                let data = data_lines.join("\n");
                let etype = event_type.take().unwrap_or_else(|| "message".into());
                frames.push(SseFrame::Event {
                    event_type: etype,
                    data,
                });
                data_lines.clear();
            }
            event_type = None;
        } else if let Some(comment) = line.strip_prefix(':') {
            frames.push(SseFrame::Comment(comment.trim().to_string()));
        } else if let Some(value) = line.strip_prefix("event:") {
            event_type = Some(value.trim().to_string());
        } else if let Some(value) = line.strip_prefix("data:") {
            // SSE spec: strip exactly ONE leading space after "data:"
            let trimmed = value.strip_prefix(' ').unwrap_or(value);
            data_lines.push(trimmed.to_string());
        }
        // Ignore unknown field names per SSE spec
    }

    // Flush remaining frame if block doesn't end with empty line
    if !data_lines.is_empty() {
        let data = data_lines.join("\n");
        let etype = event_type.take().unwrap_or_else(|| "message".into());
        frames.push(SseFrame::Event {
            event_type: etype,
            data,
        });
    }

    frames
}

/// Deserialize an SSE data payload into a typed `Event`.
pub(crate) fn parse_event(data: &str) -> Result<Event, SdkError> {
    serde_json::from_str(data).map_err(|e| SdkError::EventParse(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_single_event() {
        let block = "event: started\ndata: {\"type\":\"started\",\"job_id\":\"abc\"}\n\n";
        let frames = parse_sse_block(block);
        assert_eq!(frames.len(), 1);
        assert_eq!(
            frames[0],
            SseFrame::Event {
                event_type: "started".into(),
                data: r#"{"type":"started","job_id":"abc"}"#.into(),
            }
        );
    }

    #[test]
    fn parse_keepalive_comment() {
        let block = ": ping\n\n";
        let frames = parse_sse_block(block);
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0], SseFrame::Comment("ping".into()));
    }

    #[test]
    fn parse_multiple_frames() {
        let block = concat!(
            "event: started\n",
            "data: {\"type\":\"started\",\"job_id\":\"j1\"}\n",
            "\n",
            "event: completed\n",
            "data: {\"type\":\"completed\",\"job_id\":\"j1\",\"output\":\"done\"}\n",
            "\n",
        );
        let frames = parse_sse_block(block);
        assert_eq!(frames.len(), 2);
    }

    #[test]
    fn parse_mixed_comments_and_events() {
        let block = concat!(
            ": ping\n",
            "\n",
            "event: started\n",
            "data: {\"type\":\"started\",\"job_id\":\"j1\"}\n",
            "\n",
            ": ping\n",
            "\n",
        );
        let frames = parse_sse_block(block);
        assert_eq!(frames.len(), 3);
        assert!(matches!(&frames[0], SseFrame::Comment(c) if c == "ping"));
        assert!(matches!(&frames[1], SseFrame::Event { event_type, .. } if event_type == "started"));
        assert!(matches!(&frames[2], SseFrame::Comment(c) if c == "ping"));
    }

    #[test]
    fn parse_event_deserialize() {
        let data = r#"{"type":"task_complete","job_id":"j1","task_id":"step1","duration_ms":1200}"#;
        let event = parse_event(data).unwrap();
        assert!(matches!(
            event,
            Event::TaskComplete {
                duration_ms: 1200,
                ..
            }
        ));
    }

    #[test]
    fn parse_event_invalid_json() {
        let result = parse_event("not json");
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), SdkError::EventParse(_)));
    }

    #[test]
    fn parse_multiline_data() {
        let block = concat!(
            "event: completed\n",
            "data: {\"type\":\"completed\",\n",
            "data: \"job_id\":\"j1\",\"output\":null}\n",
            "\n",
        );
        let frames = parse_sse_block(block);
        assert_eq!(frames.len(), 1);
        if let SseFrame::Event { data, .. } = &frames[0] {
            // Multiline data is joined with newlines
            assert!(data.contains('\n'));
        } else {
            panic!("expected Event frame");
        }
    }

    #[test]
    fn preserves_leading_spaces_after_first() {
        // SSE spec: only ONE leading space is stripped, not all
        let block = "event: task_complete\ndata:   indented content\n\n";
        let frames = parse_sse_block(block);
        assert_eq!(frames.len(), 1);
        if let SseFrame::Event { data, .. } = &frames[0] {
            assert_eq!(data, "  indented content"); // two spaces preserved
        } else {
            panic!("expected Event frame");
        }
    }

    #[test]
    fn no_space_after_colon() {
        // SSE spec: if no space after "data:", no stripping
        let block = "event: started\ndata:{\"type\":\"started\"}\n\n";
        let frames = parse_sse_block(block);
        if let SseFrame::Event { data, .. } = &frames[0] {
            assert!(data.starts_with('{'));
        }
    }

    #[test]
    fn parse_no_trailing_newline() {
        let block = "event: started\ndata: {\"type\":\"started\",\"job_id\":\"j1\"}";
        let frames = parse_sse_block(block);
        assert_eq!(frames.len(), 1);
    }

    #[test]
    fn data_space_handling() {
        // SSE spec: "data: value" — single space after colon is stripped
        let block = "event: started\ndata: {\"type\":\"started\",\"job_id\":\"j1\"}\n\n";
        let frames = parse_sse_block(block);
        if let SseFrame::Event { data, .. } = &frames[0] {
            assert!(data.starts_with('{'));
        }
    }

    #[test]
    fn all_event_types_deserialize() {
        let events = vec![
            r#"{"type":"started","job_id":"j"}"#,
            r#"{"type":"task_start","job_id":"j","task_id":"t","verb":"infer"}"#,
            r#"{"type":"task_complete","job_id":"j","task_id":"t","duration_ms":100}"#,
            r#"{"type":"task_failed","job_id":"j","task_id":"t","error":"oops","duration_ms":50}"#,
            r#"{"type":"artifact_written","job_id":"j","task_id":"t","path":"out.md","size":256}"#,
            r#"{"type":"completed","job_id":"j","output":"done"}"#,
            r#"{"type":"failed","job_id":"j","error":"boom"}"#,
            r#"{"type":"cancelled","job_id":"j"}"#,
        ];
        for data in events {
            let event = parse_event(data);
            assert!(event.is_ok(), "failed to parse: {data}");
        }
    }
}
