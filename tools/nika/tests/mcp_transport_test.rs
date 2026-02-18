//! Integration tests for MCP Transport
//!
//! Tests the process spawn and lifecycle management functionality.

use nika::mcp::McpTransport;

// ═══════════════════════════════════════════════════════════════
// CONSTRUCTION TESTS
// ═══════════════════════════════════════════════════════════════

#[test]
fn test_transport_new_creates_with_command_and_args() {
    let transport = McpTransport::new("echo", &["hello", "world"]);

    assert_eq!(transport.command(), "echo");
    assert_eq!(transport.args(), &["hello", "world"]);
}

#[test]
fn test_transport_new_with_empty_args() {
    let transport = McpTransport::new("ls", &[]);

    assert_eq!(transport.command(), "ls");
    assert!(transport.args().is_empty());
}

#[test]
fn test_transport_with_env_adds_environment_variables() {
    let transport = McpTransport::new("node", &["server.js"])
        .with_env("API_KEY", "secret123")
        .with_env("DEBUG", "true");

    let env = transport.env();
    assert_eq!(env.get("API_KEY"), Some(&"secret123".to_string()));
    assert_eq!(env.get("DEBUG"), Some(&"true".to_string()));
}

#[test]
fn test_transport_with_env_overwrites_existing() {
    let transport = McpTransport::new("node", &[])
        .with_env("KEY", "first")
        .with_env("KEY", "second");

    assert_eq!(transport.env().get("KEY"), Some(&"second".to_string()));
}

// ═══════════════════════════════════════════════════════════════
// SPAWN TESTS
// ═══════════════════════════════════════════════════════════════

#[tokio::test]
async fn test_transport_spawn_echo_succeeds() {
    let transport = McpTransport::new("echo", &["test"]);

    let result = transport.spawn().await;

    assert!(result.is_ok(), "spawn should succeed for valid command");
    let mut child = result.unwrap();

    // Clean up - wait for process to finish
    let status = child.wait().await.expect("wait should succeed");
    assert!(status.success());
}

#[tokio::test]
async fn test_transport_spawn_provides_stdio_pipes() {
    let transport = McpTransport::new("cat", &[]);

    let mut child = transport.spawn().await.expect("spawn should succeed");

    // Verify we have stdin/stdout handles
    assert!(child.stdin.is_some(), "stdin should be piped");
    assert!(child.stdout.is_some(), "stdout should be piped");

    // Clean up
    child.kill().await.ok();
}

#[tokio::test]
async fn test_transport_spawn_invalid_command_returns_error() {
    let transport = McpTransport::new("nonexistent_command_xyz_123", &[]);

    let result = transport.spawn().await;

    assert!(result.is_err(), "spawn should fail for invalid command");

    // Verify it's the right error type (McpStartError)
    let err = result.unwrap_err();
    assert_eq!(err.code(), "NIKA-101");
}

#[tokio::test]
async fn test_transport_spawn_passes_environment_variables() {
    // Use 'env' command to print environment, grep for our variable
    #[cfg(unix)]
    {
        let transport = McpTransport::new("sh", &["-c", "echo $TEST_VAR"])
            .with_env("TEST_VAR", "hello_from_nika");

        let mut child = transport.spawn().await.expect("spawn should succeed");

        // Read stdout
        use tokio::io::AsyncReadExt;
        let mut stdout = child.stdout.take().unwrap();
        let mut output = String::new();
        stdout.read_to_string(&mut output).await.unwrap();

        assert!(
            output.contains("hello_from_nika"),
            "environment variable should be passed to child process"
        );

        child.wait().await.ok();
    }
}

// ═══════════════════════════════════════════════════════════════
// DEBUG TRAIT TEST
// ═══════════════════════════════════════════════════════════════

#[test]
fn test_transport_implements_debug() {
    let transport = McpTransport::new("echo", &["test"]).with_env("KEY", "value");

    let debug_output = format!("{:?}", transport);

    assert!(debug_output.contains("echo"));
    assert!(debug_output.contains("McpTransport"));
}
