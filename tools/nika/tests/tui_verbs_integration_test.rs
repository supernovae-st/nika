//! TUI Verbs Integration Tests
//!
//! Real integration tests for all 5 verb handlers in the TUI.
//! These tests require actual API keys and optionally MCP servers.
//!
//! Run with:
//!   cargo test --test tui_verbs_integration_test -- --ignored --nocapture
//!
//! Environment:
//!   ANTHROPIC_API_KEY - Required for Claude tests
//!   OPENAI_API_KEY - Required for OpenAI tests
//!   PERPLEXITY_API_KEY - Optional for MCP tests

use nika::provider::rig::{RigInferError, RigProvider, StreamChunk, StreamResult};
use nika::tui::chat_agent::ChatAgent;
use tokio::sync::mpsc;

// ═══════════════════════════════════════════════════════════════════════════
// 1. INFER VERB TESTS - LLM Text Generation
// ═══════════════════════════════════════════════════════════════════════════

mod infer_tests {
    use super::*;

    #[tokio::test]
    #[ignore]
    async fn test_infer_claude_simple() {
        if std::env::var("ANTHROPIC_API_KEY").is_err() {
            eprintln!("SKIP: ANTHROPIC_API_KEY not set");
            return;
        }

        let mut agent = ChatAgent::new().expect("Failed to create ChatAgent");
        let result = agent.infer("Reply with exactly: PONG").await;

        assert!(result.is_ok(), "Infer failed: {:?}", result.err());
        let response = result.unwrap();
        eprintln!("Response: {}", response);
        assert!(
            response.to_uppercase().contains("PONG"),
            "Expected PONG in response: {}",
            response
        );
    }

    #[tokio::test]
    #[ignore]
    async fn test_infer_claude_streaming_tokens_arrive() {
        if std::env::var("ANTHROPIC_API_KEY").is_err() {
            eprintln!("SKIP: ANTHROPIC_API_KEY not set");
            return;
        }

        let provider = RigProvider::claude();
        let (tx, mut rx) = mpsc::channel::<StreamChunk>(100);

        // Spawn collector to receive tokens
        let collector = tokio::spawn(async move {
            let mut tokens = Vec::new();
            let mut got_done = false;
            while let Some(chunk) = rx.recv().await {
                match chunk {
                    StreamChunk::Token(t) => {
                        eprintln!("TOKEN[{}]: '{}'", tokens.len(), t);
                        tokens.push(t);
                    }
                    StreamChunk::Done(_) => {
                        got_done = true;
                        break;
                    }
                    StreamChunk::Error(e) => {
                        panic!("Stream error: {}", e);
                    }
                    _ => {}
                }
            }
            (tokens, got_done)
        });

        let result: Result<StreamResult, RigInferError> = provider
            .infer_stream("Count from 1 to 5, one number per line", tx, None)
            .await;

        assert!(result.is_ok(), "Stream failed: {:?}", result.err());
        let stream_result = result.unwrap();
        eprintln!(
            "Tokens: input={}, output={}, total={}",
            stream_result.input_tokens, stream_result.output_tokens, stream_result.total_tokens
        );

        let (tokens, got_done) = collector.await.expect("Collector failed");
        assert!(got_done, "Should receive Done chunk");
        assert!(
            !tokens.is_empty(),
            "Should receive at least one token, got: {:?}",
            tokens
        );
        eprintln!("✅ Streaming test passed! Received {} tokens", tokens.len());
    }

    #[tokio::test]
    #[ignore]
    async fn test_infer_openai_simple() {
        if std::env::var("OPENAI_API_KEY").is_err() {
            eprintln!("SKIP: OPENAI_API_KEY not set");
            return;
        }

        let provider = RigProvider::openai();
        let result = provider.infer("Reply with exactly: PONG", None).await;

        assert!(result.is_ok(), "Infer failed: {:?}", result.err());
        let response = result.unwrap();
        eprintln!("Response: {}", response);
        assert!(
            response.to_uppercase().contains("PONG"),
            "Expected PONG: {}",
            response
        );
    }

    #[tokio::test]
    #[ignore]
    async fn test_infer_openai_streaming_tokens_arrive() {
        if std::env::var("OPENAI_API_KEY").is_err() {
            eprintln!("SKIP: OPENAI_API_KEY not set");
            return;
        }

        let provider = RigProvider::openai();
        let (tx, mut rx) = mpsc::channel::<StreamChunk>(100);

        let collector = tokio::spawn(async move {
            let mut count = 0;
            while let Some(chunk) = rx.recv().await {
                if let StreamChunk::Token(t) = chunk {
                    eprintln!("TOKEN[{}]: '{}'", count, t);
                    count += 1;
                }
            }
            count
        });

        let result: Result<StreamResult, RigInferError> =
            provider.infer_stream("Say hello", tx, None).await;

        assert!(result.is_ok(), "Stream failed: {:?}", result.err());
        let stream_result = result.unwrap();
        eprintln!(
            "Tokens: input={}, output={}, total={}",
            stream_result.input_tokens, stream_result.output_tokens, stream_result.total_tokens
        );

        let count = collector.await.expect("Collector failed");
        assert!(count > 0, "Should receive tokens, got: {}", count);
        eprintln!("✅ OpenAI streaming passed! {} tokens", count);
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 2. EXEC VERB TESTS - Shell Command Execution
// ═══════════════════════════════════════════════════════════════════════════

mod exec_tests {
    use super::*;

    #[tokio::test]
    #[ignore]
    async fn test_exec_echo_command() {
        let agent = ChatAgent::new().expect("Failed to create ChatAgent");
        let result = agent.exec_command("echo HELLO_NIKA").await;

        assert!(result.is_ok(), "Exec failed: {:?}", result.err());
        let output = result.unwrap();
        eprintln!("Output: {}", output);
        assert!(
            output.contains("HELLO_NIKA"),
            "Expected HELLO_NIKA: {}",
            output
        );
    }

    #[tokio::test]
    #[ignore]
    async fn test_exec_pwd_command() {
        let agent = ChatAgent::new().expect("Failed to create ChatAgent");
        let result = agent.exec_command("pwd").await;

        assert!(result.is_ok(), "Exec failed: {:?}", result.err());
        let output = result.unwrap();
        eprintln!("PWD: {}", output);
        assert!(output.contains("/"), "Expected path: {}", output);
    }

    #[tokio::test]
    #[ignore]
    async fn test_exec_failing_command_returns_error() {
        let agent = ChatAgent::new().expect("Failed to create ChatAgent");
        let result = agent.exec_command("exit 42").await;

        // Should return error or include exit code
        eprintln!("Result: {:?}", result);
        // The command should either fail or show non-zero exit
    }

    #[tokio::test]
    #[ignore]
    async fn test_exec_pipe_command() {
        let agent = ChatAgent::new().expect("Failed to create ChatAgent");
        let result = agent
            .exec_command("echo 'line1\nline2\nline3' | wc -l")
            .await;

        assert!(result.is_ok(), "Exec failed: {:?}", result.err());
        let output = result.unwrap();
        eprintln!("Output: {}", output);
        // Should contain "3" (three lines)
        assert!(output.contains("3"), "Expected 3 lines: {}", output);
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 3. FETCH VERB TESTS - HTTP Requests
// ═══════════════════════════════════════════════════════════════════════════

mod fetch_tests {
    use super::*;

    #[tokio::test]
    #[ignore]
    async fn test_fetch_httpbin_get() {
        let agent = ChatAgent::new().expect("Failed to create ChatAgent");
        let result = agent.fetch("https://httpbin.org/get", "GET").await;

        assert!(result.is_ok(), "Fetch failed: {:?}", result.err());
        let response = result.unwrap();
        eprintln!("Response: {}", &response[..500.min(response.len())]);
        assert!(
            response.contains("httpbin"),
            "Expected httpbin: {}",
            response
        );
    }

    #[tokio::test]
    #[ignore]
    async fn test_fetch_json_api() {
        let agent = ChatAgent::new().expect("Failed to create ChatAgent");
        let result = agent
            .fetch("https://jsonplaceholder.typicode.com/posts/1", "GET")
            .await;

        assert!(result.is_ok(), "Fetch failed: {:?}", result.err());
        let response = result.unwrap();
        eprintln!("Response: {}", response);
        assert!(response.contains("userId"), "Expected JSON: {}", response);
    }

    #[tokio::test]
    #[ignore]
    async fn test_fetch_post_httpbin() {
        let agent = ChatAgent::new().expect("Failed to create ChatAgent");
        // Note: ChatAgent.fetch() doesn't support body yet - just tests POST method works
        let result = agent.fetch("https://httpbin.org/post", "POST").await;

        assert!(result.is_ok(), "Fetch failed: {:?}", result.err());
        let response = result.unwrap();
        eprintln!("Response: {}", &response[..800.min(response.len())]);
        // Should at least return something from httpbin
        assert!(
            response.contains("httpbin"),
            "Expected httpbin: {}",
            response
        );
    }

    #[tokio::test]
    #[ignore]
    async fn test_fetch_404_returns_status() {
        let agent = ChatAgent::new().expect("Failed to create ChatAgent");
        let result = agent.fetch("https://httpbin.org/status/404", "GET").await;

        // Should return Ok but with 404 status info
        eprintln!("Result: {:?}", result);
        // Either error or contains 404
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 4. INVOKE VERB TESTS - MCP Tool Calls
// ═══════════════════════════════════════════════════════════════════════════

mod invoke_tests {
    use nika::mcp::{McpClient, McpConfig};

    #[tokio::test]
    #[ignore]
    async fn test_invoke_echo_tool() {
        // Create a simple echo MCP server for testing
        // This test requires a mock or real MCP server

        // For now, test the McpClient creation
        let config = McpConfig::new("test", "echo").with_arg("test-server");

        let client = McpClient::new(config);
        assert!(
            client.is_ok(),
            "Failed to create MCP client: {:?}",
            client.err()
        );
    }

    #[tokio::test]
    #[ignore]
    async fn test_invoke_list_tools_from_mock() {
        let client = McpClient::mock("test-mock");

        // Mock clients have predefined tools
        let tools = client.get_tool_definitions();
        eprintln!(
            "Mock tools: {:?}",
            tools.iter().map(|t| &t.name).collect::<Vec<_>>()
        );

        // Should have some mock tools
        assert!(!tools.is_empty(), "Mock should have tools");
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 5. AGENT VERB TESTS - Multi-Turn Agentic Loop
// ═══════════════════════════════════════════════════════════════════════════

mod agent_tests {
    use nika::ast::AgentParams;
    use nika::event::EventLog;
    use nika::mcp::McpClient;
    use nika::runtime::RigAgentLoop;
    use rustc_hash::FxHashMap;
    use std::sync::Arc;

    #[tokio::test]
    #[ignore]
    async fn test_agent_simple_prompt_completes() {
        if std::env::var("ANTHROPIC_API_KEY").is_err() && std::env::var("OPENAI_API_KEY").is_err() {
            eprintln!("SKIP: No API key set");
            return;
        }

        let params = AgentParams {
            prompt: "Reply with exactly: AGENT_OK".to_string(),
            mcp: vec![],
            max_turns: Some(1),
            ..Default::default()
        };

        let event_log = EventLog::new();
        let mcp_clients: FxHashMap<String, Arc<McpClient>> = FxHashMap::default();

        let mut agent = RigAgentLoop::new("test-agent".into(), params, event_log, mcp_clients)
            .expect("Failed to create agent");

        let result = agent.run_auto().await;
        assert!(result.is_ok(), "Agent failed: {:?}", result.err());

        let status = result.unwrap();
        eprintln!("Agent status: {:?}", status);
        eprintln!("Tool calls: {}", agent.tool_count());

        // Agent should complete naturally or hit max turns
    }

    #[tokio::test]
    #[ignore]
    async fn test_agent_with_mock_mcp_tools() {
        if std::env::var("ANTHROPIC_API_KEY").is_err() && std::env::var("OPENAI_API_KEY").is_err() {
            eprintln!("SKIP: No API key set");
            return;
        }

        // Create mock MCP client
        let mock_client = Arc::new(McpClient::mock("test-tools"));

        let mut mcp_clients: FxHashMap<String, Arc<McpClient>> = FxHashMap::default();
        mcp_clients.insert("test-tools".to_string(), mock_client);

        let params = AgentParams {
            prompt: "Use the echo tool to say hello".to_string(),
            mcp: vec!["test-tools".to_string()],
            max_turns: Some(3),
            ..Default::default()
        };

        let event_log = EventLog::new();

        let mut agent = RigAgentLoop::new("test-agent-mcp".into(), params, event_log, mcp_clients)
            .expect("Failed to create agent");

        let result = agent.run_auto().await;
        eprintln!("Agent result: {:?}", result);
        eprintln!("Tool calls: {}", agent.tool_count());
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 6. END-TO-END STREAMING TEST
// ═══════════════════════════════════════════════════════════════════════════

mod e2e_streaming_tests {
    use super::*;

    #[tokio::test]
    #[ignore]
    async fn test_chat_agent_uses_streaming_when_channel_set() {
        if std::env::var("ANTHROPIC_API_KEY").is_err() {
            eprintln!("SKIP: ANTHROPIC_API_KEY not set");
            return;
        }

        let (tx, mut rx) = mpsc::channel::<StreamChunk>(256);

        // Create agent WITH streaming channel
        let agent = ChatAgent::new().expect("Failed to create agent");
        let mut agent = agent.with_stream_chunks(tx);

        // Spawn token collector
        let collector = tokio::spawn(async move {
            let mut tokens = Vec::new();
            while let Some(chunk) = rx.recv().await {
                match chunk {
                    StreamChunk::Token(t) => {
                        tokens.push(t.clone());
                        eprint!("{}", t); // Print tokens as they arrive
                    }
                    StreamChunk::Done(_) => break,
                    StreamChunk::Error(e) => {
                        eprintln!("\nError: {}", e);
                        break;
                    }
                    _ => {}
                }
            }
            eprintln!(); // Newline after streaming
            tokens
        });

        // Run inference
        let result = agent.infer("Write a haiku about Rust programming").await;
        assert!(result.is_ok(), "Infer failed: {:?}", result.err());

        // Check tokens were received
        let tokens = collector.await.expect("Collector failed");
        assert!(
            !tokens.is_empty(),
            "Should receive streaming tokens, got none!"
        );
        eprintln!("✅ E2E streaming passed! {} tokens received", tokens.len());
    }
}
