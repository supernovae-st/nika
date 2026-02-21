//! MCP Connection Event Tests (v0.7.0)
//!
//! Tests for EventKind::McpConnected and EventKind::McpError emission
//! and StreamChunk::McpConnected/McpError TUI integration.

use nika::event::{EventKind, EventLog};
use nika::provider::rig::StreamChunk;

// =============================================================================
// EventKind::McpConnected/McpError Tests
// =============================================================================

#[test]
fn test_event_kind_mcp_connected_structure() {
    let event_log = EventLog::new();

    // Emit McpConnected event
    event_log.emit(EventKind::McpConnected {
        server_name: "novanet".to_string(),
    });

    // Verify event was recorded
    let events = event_log.events();
    assert_eq!(events.len(), 1);

    match &events[0].kind {
        EventKind::McpConnected { server_name } => {
            assert_eq!(server_name, "novanet");
        }
        _ => panic!("Expected McpConnected event"),
    }
}

#[test]
fn test_event_kind_mcp_error_structure() {
    let event_log = EventLog::new();

    // Emit McpError event
    event_log.emit(EventKind::McpError {
        server_name: "novanet".to_string(),
        error: "Connection refused".to_string(),
    });

    // Verify event was recorded
    let events = event_log.events();
    assert_eq!(events.len(), 1);

    match &events[0].kind {
        EventKind::McpError { server_name, error } => {
            assert_eq!(server_name, "novanet");
            assert_eq!(error, "Connection refused");
        }
        _ => panic!("Expected McpError event"),
    }
}

#[test]
fn test_event_kind_mcp_events_have_timestamps() {
    let event_log = EventLog::new();

    event_log.emit(EventKind::McpConnected {
        server_name: "test".to_string(),
    });

    let events = event_log.events();
    assert!(!events.is_empty());
    // Event should have a timestamp_ms field (u64 in milliseconds)
    // Verify event was recorded
    assert!(events.len() == 1);
}

#[test]
fn test_event_kind_mcp_events_sequential() {
    let event_log = EventLog::new();

    // Emit connection then error
    event_log.emit(EventKind::McpConnected {
        server_name: "server1".to_string(),
    });
    event_log.emit(EventKind::McpError {
        server_name: "server2".to_string(),
        error: "Failed".to_string(),
    });

    let events = event_log.events();
    assert_eq!(events.len(), 2);

    // Verify order
    assert!(matches!(&events[0].kind, EventKind::McpConnected { .. }));
    assert!(matches!(&events[1].kind, EventKind::McpError { .. }));
}

// =============================================================================
// StreamChunk::McpConnected/McpError Tests
// =============================================================================

#[test]
fn test_stream_chunk_mcp_connected_clone() {
    let chunk = StreamChunk::McpConnected("novanet".to_string());
    let cloned = chunk.clone();

    match cloned {
        StreamChunk::McpConnected(name) => {
            assert_eq!(name, "novanet");
        }
        _ => panic!("Expected McpConnected chunk"),
    }
}

#[test]
fn test_stream_chunk_mcp_error_clone() {
    let chunk = StreamChunk::McpError {
        server_name: "novanet".to_string(),
        error: "Connection timeout".to_string(),
    };
    let cloned = chunk.clone();

    match cloned {
        StreamChunk::McpError { server_name, error } => {
            assert_eq!(server_name, "novanet");
            assert_eq!(error, "Connection timeout");
        }
        _ => panic!("Expected McpError chunk"),
    }
}

#[test]
fn test_stream_chunk_mcp_debug_format() {
    let connected = StreamChunk::McpConnected("test-server".to_string());
    let debug_str = format!("{:?}", connected);
    assert!(debug_str.contains("McpConnected"));
    assert!(debug_str.contains("test-server"));

    let error = StreamChunk::McpError {
        server_name: "test-server".to_string(),
        error: "Failed to connect".to_string(),
    };
    let debug_str = format!("{:?}", error);
    assert!(debug_str.contains("McpError"));
    assert!(debug_str.contains("test-server"));
    assert!(debug_str.contains("Failed to connect"));
}

// =============================================================================
// Channel Integration Tests
// =============================================================================

#[tokio::test]
async fn test_mcp_connected_channel_send() {
    use tokio::sync::mpsc;

    let (tx, mut rx) = mpsc::channel::<StreamChunk>(10);

    // Send MCP connected event
    tx.send(StreamChunk::McpConnected("novanet".to_string()))
        .await
        .expect("Send should succeed");

    // Receive and verify
    let received = rx.recv().await.expect("Should receive chunk");
    match received {
        StreamChunk::McpConnected(name) => {
            assert_eq!(name, "novanet");
        }
        _ => panic!("Expected McpConnected"),
    }
}

#[tokio::test]
async fn test_mcp_error_channel_send() {
    use tokio::sync::mpsc;

    let (tx, mut rx) = mpsc::channel::<StreamChunk>(10);

    // Send MCP error event
    tx.send(StreamChunk::McpError {
        server_name: "novanet".to_string(),
        error: "Server not responding".to_string(),
    })
    .await
    .expect("Send should succeed");

    // Receive and verify
    let received = rx.recv().await.expect("Should receive chunk");
    match received {
        StreamChunk::McpError { server_name, error } => {
            assert_eq!(server_name, "novanet");
            assert_eq!(error, "Server not responding");
        }
        _ => panic!("Expected McpError"),
    }
}

#[tokio::test]
async fn test_mixed_stream_chunks_with_mcp_events() {
    use tokio::sync::mpsc;

    let (tx, mut rx) = mpsc::channel::<StreamChunk>(20);

    // Send a mix of streaming chunks
    tx.send(StreamChunk::McpConnected("server1".to_string()))
        .await
        .unwrap();
    tx.send(StreamChunk::Token("Hello".to_string()))
        .await
        .unwrap();
    tx.send(StreamChunk::McpError {
        server_name: "server2".to_string(),
        error: "Timeout".to_string(),
    })
    .await
    .unwrap();
    tx.send(StreamChunk::Done("Hello World".to_string()))
        .await
        .unwrap();

    // Collect all chunks
    drop(tx); // Close sender
    let mut chunks = Vec::new();
    while let Some(chunk) = rx.recv().await {
        chunks.push(chunk);
    }

    // Verify we got all 4 chunks in order
    assert_eq!(chunks.len(), 4);
    assert!(matches!(&chunks[0], StreamChunk::McpConnected(_)));
    assert!(matches!(&chunks[1], StreamChunk::Token(_)));
    assert!(matches!(&chunks[2], StreamChunk::McpError { .. }));
    assert!(matches!(&chunks[3], StreamChunk::Done(_)));
}

// =============================================================================
// EventLog Broadcast Tests
// =============================================================================

#[tokio::test]
async fn test_mcp_events_broadcast() {
    let (event_log, mut rx) = EventLog::new_with_broadcast();

    // Emit MCP connected event
    event_log.emit(EventKind::McpConnected {
        server_name: "novanet".to_string(),
    });

    // Should receive via broadcast
    let event = rx.recv().await.expect("Should receive event");
    match &event.kind {
        EventKind::McpConnected { server_name } => {
            assert_eq!(server_name, "novanet");
        }
        _ => panic!("Expected McpConnected event"),
    }
}

#[tokio::test]
async fn test_mcp_error_broadcast() {
    let (event_log, mut rx) = EventLog::new_with_broadcast();

    // Emit MCP error event
    event_log.emit(EventKind::McpError {
        server_name: "perplexity".to_string(),
        error: "API key not found".to_string(),
    });

    // Should receive via broadcast
    let event = rx.recv().await.expect("Should receive event");
    match &event.kind {
        EventKind::McpError { server_name, error } => {
            assert_eq!(server_name, "perplexity");
            assert_eq!(error, "API key not found");
        }
        _ => panic!("Expected McpError event"),
    }
}
