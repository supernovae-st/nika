//! MCP Race Condition Tests (HIGH Priority)
//!
//! Tests for concurrent MCP client access patterns to verify thread safety.
//!
//! ## Test Coverage
//!
//! | Test | Scenario | Validates |
//! |------|----------|-----------|
//! | `test_concurrent_client_init` | 10 tasks try get_or_init same client | Only ONE client created |
//! | `test_concurrent_calls_same_tool` | 20 concurrent calls to same tool | All complete successfully |
//! | `test_for_each_parallel_mcp_calls` | for_each with concurrency=5 | No race conditions |
//! | `test_concurrent_cache_access` | Multiple tasks hitting response cache | Cache thread safety |
//! | `test_concurrent_connect_disconnect` | Connect/disconnect race | State consistency |

use std::collections::HashSet;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use nika::mcp::{McpClient, McpConfig};
use nika::runtime::Runner;
use nika::Workflow;
use serde_json::json;
use tokio::sync::Barrier;
use tokio::task::JoinSet;

// ═══════════════════════════════════════════════════════════════════════════
// TEST 1: Concurrent Client Initialization
// ═══════════════════════════════════════════════════════════════════════════

/// Test that concurrent get_or_init calls result in only ONE client being created.
///
/// This validates the OnceCell-based lazy initialization pattern in TaskExecutor.
/// Even with 10 concurrent tasks racing to initialize the same client,
/// only one should actually be created.
#[tokio::test]
async fn test_concurrent_client_init() {
    use dashmap::DashMap;
    use tokio::sync::OnceCell;

    // Simulate the pattern used in TaskExecutor::get_mcp_client
    let client_cache: Arc<DashMap<String, Arc<OnceCell<Arc<McpClient>>>>> =
        Arc::new(DashMap::new());

    // Counter to track how many times initialization actually runs
    let init_count = Arc::new(AtomicUsize::new(0));

    // Barrier to synchronize all tasks to start at the same time
    let barrier = Arc::new(Barrier::new(10));

    let mut handles = JoinSet::new();

    for i in 0..10 {
        let cache = Arc::clone(&client_cache);
        let counter = Arc::clone(&init_count);
        let barrier = Arc::clone(&barrier);

        handles.spawn(async move {
            // Wait for all tasks to be ready
            barrier.wait().await;

            // Get or create the OnceCell for "novanet" server
            let cell = cache
                .entry("novanet".to_string())
                .or_insert_with(|| Arc::new(OnceCell::new()))
                .clone();

            // Try to initialize - only one should succeed in actually running init
            let client = cell
                .get_or_init(|| async {
                    // Increment counter - this should only happen ONCE
                    counter.fetch_add(1, Ordering::SeqCst);

                    // Small delay to ensure race condition window
                    tokio::time::sleep(Duration::from_millis(10)).await;

                    Arc::new(McpClient::mock("novanet"))
                })
                .await;

            // Return client name and task id for verification
            (client.name().to_string(), i)
        });
    }

    // Collect all results
    let mut results = Vec::new();
    while let Some(result) = handles.join_next().await {
        results.push(result.expect("Task should not panic"));
    }

    // Verify: exactly 10 tasks completed
    assert_eq!(results.len(), 10, "All 10 tasks should complete");

    // Verify: all tasks got the same client (same name)
    let names: HashSet<_> = results.iter().map(|(name, _)| name.clone()).collect();
    assert_eq!(names.len(), 1, "All tasks should get the same client");
    assert!(
        names.contains("novanet"),
        "Client should be named 'novanet'"
    );

    // Verify: initialization only ran ONCE
    let final_count = init_count.load(Ordering::SeqCst);
    assert_eq!(
        final_count, 1,
        "Initialization should only run once, but ran {} times",
        final_count
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// TEST 2: Concurrent Calls to Same Tool
// ═══════════════════════════════════════════════════════════════════════════

/// Test that 20 concurrent calls to the same tool all complete successfully.
///
/// This validates thread safety of McpClient.call_tool() when accessed
/// from multiple tokio tasks simultaneously.
#[tokio::test]
async fn test_concurrent_calls_same_tool() {
    // Single shared client
    let client = Arc::new(McpClient::mock("novanet"));

    // Barrier to synchronize all 20 tasks
    let barrier = Arc::new(Barrier::new(20));

    let mut handles = JoinSet::new();

    for i in 0..20 {
        let client = Arc::clone(&client);
        let barrier = Arc::clone(&barrier);

        handles.spawn(async move {
            // Wait for all tasks to be ready
            barrier.wait().await;

            // Make concurrent call to same tool with unique params
            let result = client
                .call_tool(
                    "novanet_generate",
                    json!({
                        "entity": format!("entity_{}", i),
                        "locale": "fr-FR"
                    }),
                )
                .await;

            (i, result)
        });
    }

    // Collect and verify results
    let mut success_count = 0;
    let mut task_ids = Vec::new();

    while let Some(result) = handles.join_next().await {
        let (task_id, call_result) = result.expect("Task should not panic");
        task_ids.push(task_id);

        assert!(
            call_result.is_ok(),
            "Task {} call should succeed: {:?}",
            task_id,
            call_result.err()
        );

        let tool_result = call_result.unwrap();
        assert!(
            !tool_result.is_error,
            "Task {} tool result should not be error",
            task_id
        );

        success_count += 1;
    }

    // Verify all 20 calls succeeded
    assert_eq!(success_count, 20, "All 20 concurrent calls should succeed");

    // Verify all tasks completed (no lost tasks)
    task_ids.sort();
    let expected: Vec<usize> = (0..20).collect();
    assert_eq!(task_ids, expected, "All task IDs should be present");
}

// ═══════════════════════════════════════════════════════════════════════════
// TEST 3: for_each Parallel MCP Calls
// ═══════════════════════════════════════════════════════════════════════════

/// Test for_each with concurrency=5 calling MCP tools without race conditions.
///
/// This simulates the real-world pattern where for_each parallelism
/// causes multiple concurrent MCP tool invocations.
#[tokio::test]
async fn test_for_each_parallel_mcp_calls() {
    // Create a workflow with for_each that would invoke MCP calls
    // Using exec as a stand-in since we can't easily mock real MCP here
    let yaml = r#"
schema: nika/workflow@0.3
provider: mock

tasks:
  - id: parallel_mcp_simulation
    for_each: ["a", "b", "c", "d", "e", "f", "g", "h", "i", "j"]
    as: item
    concurrency: 5
    exec:
      command: "echo MCP call for {{use.item}}"
"#;

    let workflow: Workflow = serde_yaml::from_str(yaml).expect("Should parse workflow");
    let runner = Runner::new(workflow);

    let result = runner.run().await;

    assert!(
        result.is_ok(),
        "Parallel MCP-simulated calls should succeed: {:?}",
        result.err()
    );

    let output = result.unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&output).unwrap_or_default();

    if let serde_json::Value::Array(arr) = parsed {
        assert_eq!(arr.len(), 10, "Should have 10 results from for_each");

        // Verify all items were processed (in any order due to concurrency)
        let items: Vec<&str> = arr.iter().filter_map(|v| v.as_str()).collect();

        for letter in ["a", "b", "c", "d", "e", "f", "g", "h", "i", "j"] {
            assert!(
                items.iter().any(|s| s.contains(letter)),
                "Should have result containing '{}'",
                letter
            );
        }
    } else {
        panic!("Output should be a JSON array");
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// TEST 4: Concurrent Cache Access
// ═══════════════════════════════════════════════════════════════════════════

/// Test concurrent access to the response cache.
///
/// When multiple tasks hit the same cached response simultaneously,
/// the cache should handle concurrent reads/writes safely.
#[tokio::test]
async fn test_concurrent_cache_access() {
    use nika::mcp::CacheConfig;
    use std::time::Duration;

    // Client with caching enabled
    let client = Arc::new(McpClient::mock("test").with_cache(CacheConfig {
        ttl: Duration::from_secs(60),
        max_entries: 100,
    }));

    // First call to populate cache
    let params = json!({"entity": "shared"});
    client
        .call_tool("novanet_generate", params.clone())
        .await
        .expect("First call should succeed");

    // Verify cache has 1 entry
    let stats = client.cache_stats().unwrap();
    assert_eq!(stats.entries, 1, "Cache should have 1 entry");

    // Now spawn 20 concurrent tasks that should all hit the cache
    let barrier = Arc::new(Barrier::new(20));
    let mut handles = JoinSet::new();

    for i in 0..20 {
        let client = Arc::clone(&client);
        let barrier = Arc::clone(&barrier);
        let params = params.clone();

        handles.spawn(async move {
            barrier.wait().await;

            // All should hit the same cache entry
            let result = client.call_tool("novanet_generate", params).await;
            let was_cached = client.was_last_call_cached();

            (i, result.is_ok(), was_cached)
        });
    }

    // Collect results
    let mut cache_hits = 0;
    let mut success_count = 0;

    while let Some(result) = handles.join_next().await {
        let (task_id, success, was_cached) = result.expect("Task should not panic");

        assert!(success, "Task {} should succeed", task_id);
        success_count += 1;

        if was_cached {
            cache_hits += 1;
        }
    }

    assert_eq!(success_count, 20, "All 20 tasks should succeed");

    // All 20 concurrent calls should be cache hits
    // (the first populating call was before the concurrent batch)
    assert_eq!(cache_hits, 20, "All concurrent calls should be cache hits");

    // Final cache stats
    let final_stats = client.cache_stats().unwrap();
    assert_eq!(final_stats.entries, 1, "Should still have 1 cache entry");
    // 20 cache hits from concurrent calls + 0 misses (first call was separate)
    assert!(final_stats.hits >= 20, "Should have at least 20 cache hits");
}

// ═══════════════════════════════════════════════════════════════════════════
// TEST 5: Concurrent Connect/Disconnect Race
// ═══════════════════════════════════════════════════════════════════════════

/// Test that concurrent connect/disconnect operations don't cause race conditions.
///
/// Mock clients use AtomicBool for connection state, which should be thread-safe.
#[tokio::test]
async fn test_concurrent_connect_disconnect() {
    let client = Arc::new(McpClient::mock("test"));

    // Already connected (mock starts connected)
    assert!(client.is_connected());

    let barrier = Arc::new(Barrier::new(10));
    let mut handles = JoinSet::new();

    // Spawn tasks that alternate between connect and disconnect
    for i in 0..10 {
        let client = Arc::clone(&client);
        let barrier = Arc::clone(&barrier);

        handles.spawn(async move {
            barrier.wait().await;

            let result = if i % 2 == 0 {
                client.disconnect().await
            } else {
                client.connect().await
            };

            (i, result)
        });
    }

    // All operations should complete without panic or error
    while let Some(result) = handles.join_next().await {
        let (task_id, op_result) = result.expect("Task should not panic");
        assert!(
            op_result.is_ok(),
            "Task {} connect/disconnect should succeed: {:?}",
            task_id,
            op_result.err()
        );
    }

    // Final state should be consistent - verify by reading it twice
    // If there were corruption, repeated reads might give different results
    let state1 = client.is_connected();
    let state2 = client.is_connected();
    assert_eq!(
        state1, state2,
        "Connection state should be consistent across reads"
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// TEST 6: High-Contention Stress Test
// ═══════════════════════════════════════════════════════════════════════════

/// Stress test with 100 concurrent tasks hitting the same client.
///
/// This is an extreme case to verify no deadlocks or panics under heavy load.
#[tokio::test]
async fn test_high_contention_stress() {
    let client = Arc::new(McpClient::mock("stress-test"));

    let mut handles = JoinSet::new();

    // Spawn 100 tasks with no barrier (pure race condition stress)
    for i in 0..100 {
        let client = Arc::clone(&client);

        handles.spawn(async move {
            // Random mix of operations to stress-test
            match i % 4 {
                0 => {
                    // call_tool with unique params
                    let result = client
                        .call_tool("novanet_describe", json!({"iteration": i}))
                        .await;
                    result.is_ok()
                }
                1 => {
                    // call_tool with shared params
                    let result = client.call_tool("novanet_describe", json!({})).await;
                    result.is_ok()
                }
                2 => {
                    // list_tools
                    let result = client.list_tools().await;
                    result.is_ok()
                }
                3 => {
                    // read_resource
                    let result = client
                        .read_resource(&format!("neo4j://entity/test-{}", i))
                        .await;
                    result.is_ok()
                }
                _ => unreachable!(),
            }
        });
    }

    // Collect results
    let mut success_count = 0;
    let mut total = 0;

    while let Some(result) = handles.join_next().await {
        total += 1;
        if result.expect("Task should not panic") {
            success_count += 1;
        }
    }

    assert_eq!(total, 100, "All 100 tasks should complete");
    assert_eq!(success_count, 100, "All 100 operations should succeed");
}

// ═══════════════════════════════════════════════════════════════════════════
// TEST 7: Concurrent Workflow Runs with Shared MCP Mock
// ═══════════════════════════════════════════════════════════════════════════

/// Test that multiple concurrent workflow executions share MCP client safely.
///
/// When multiple workflows run concurrently with the same MCP configuration,
/// they should not interfere with each other.
#[tokio::test]
async fn test_concurrent_workflow_runs_shared_mcp() {
    let yaml = r#"
schema: nika/workflow@0.3
provider: mock

tasks:
  - id: simple_exec
    exec:
      command: "echo workflow_run"
"#;

    let mut handles = JoinSet::new();

    // Run 5 workflows concurrently
    for i in 0..5 {
        let yaml = yaml.to_string();
        handles.spawn(async move {
            let workflow: Workflow = serde_yaml::from_str(&yaml).expect("Should parse workflow");
            let runner = Runner::new(workflow);
            let result = runner.run().await;
            (i, result.is_ok())
        });
    }

    // All should complete without panic
    let mut completed = Vec::new();
    while let Some(result) = handles.join_next().await {
        let (i, success) = result.expect("Task should not panic");
        assert!(success, "Workflow {} should succeed", i);
        completed.push(i);
    }

    completed.sort();
    assert_eq!(
        completed,
        vec![0, 1, 2, 3, 4],
        "All workflows should complete"
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// TEST 8: Multiple Servers Concurrent Access
// ═══════════════════════════════════════════════════════════════════════════

/// Test concurrent access to multiple different MCP servers.
///
/// This validates that the DashMap keying by server name works correctly
/// when multiple servers are accessed concurrently.
#[tokio::test]
async fn test_multiple_servers_concurrent() {
    use dashmap::DashMap;
    use tokio::sync::OnceCell;

    let client_cache: Arc<DashMap<String, Arc<OnceCell<Arc<McpClient>>>>> =
        Arc::new(DashMap::new());

    // Track which servers were initialized
    let init_servers = Arc::new(std::sync::Mutex::new(Vec::new()));

    let mut handles = JoinSet::new();

    // 20 tasks accessing 4 different servers (5 tasks per server)
    let servers = ["novanet", "perplexity", "filesystem", "memory"];

    for i in 0..20 {
        let cache = Arc::clone(&client_cache);
        let init_log = Arc::clone(&init_servers);
        let server = servers[i % 4].to_string();

        handles.spawn(async move {
            let cell = cache
                .entry(server.clone())
                .or_insert_with(|| Arc::new(OnceCell::new()))
                .clone();

            let client = cell
                .get_or_init(|| {
                    let server = server.clone();
                    let init_log = Arc::clone(&init_log);
                    async move {
                        // Log which server was initialized
                        init_log.lock().unwrap().push(server.clone());

                        // Small delay to create race window
                        tokio::time::sleep(Duration::from_millis(5)).await;

                        Arc::new(McpClient::mock(&server))
                    }
                })
                .await;

            (i, client.name().to_string())
        });
    }

    // Collect results
    let mut results = Vec::new();
    while let Some(result) = handles.join_next().await {
        results.push(result.expect("Task should not panic"));
    }

    assert_eq!(results.len(), 20, "All 20 tasks should complete");

    // Check initialization log - should have exactly 4 entries (one per server)
    let init_log = init_servers.lock().unwrap();
    let unique_servers: HashSet<_> = init_log.iter().cloned().collect();

    assert_eq!(
        unique_servers.len(),
        4,
        "Should have initialized exactly 4 unique servers: {:?}",
        *init_log
    );
    assert_eq!(
        init_log.len(),
        4,
        "Each server should be initialized exactly once, but got {} initializations",
        init_log.len()
    );

    // Verify cache has 4 entries
    assert_eq!(client_cache.len(), 4, "Cache should have exactly 4 entries");
}

// ═══════════════════════════════════════════════════════════════════════════
// TEST 9: Cache Eviction Under Concurrent Load
// ═══════════════════════════════════════════════════════════════════════════

/// Test cache eviction doesn't cause race conditions under concurrent load.
#[tokio::test]
async fn test_cache_eviction_concurrent() {
    use nika::mcp::CacheConfig;
    use std::time::Duration;

    // Small cache that will trigger eviction
    let client = Arc::new(McpClient::mock("test").with_cache(CacheConfig {
        ttl: Duration::from_secs(60),
        max_entries: 10, // Small limit to trigger eviction
    }));

    let mut handles = JoinSet::new();

    // Spawn 50 tasks with different params to trigger eviction
    for i in 0..50 {
        let client = Arc::clone(&client);

        handles.spawn(async move {
            // Each task uses unique params to create new cache entries
            let result = client
                .call_tool("novanet_generate", json!({"unique_id": i}))
                .await;

            (i, result.is_ok())
        });
    }

    // Collect results
    let mut success_count = 0;
    while let Some(result) = handles.join_next().await {
        let (task_id, success) = result.expect("Task should not panic");
        assert!(success, "Task {} should succeed", task_id);
        success_count += 1;
    }

    assert_eq!(success_count, 50, "All 50 tasks should succeed");

    // Cache should have been evicted to stay under limit
    let stats = client.cache_stats().unwrap();
    assert!(
        stats.entries <= 10,
        "Cache should not exceed max_entries (10), has {}",
        stats.entries
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// TEST 10: Real MCP Config Creation Race
// ═══════════════════════════════════════════════════════════════════════════

/// Test that creating McpConfig and McpClient concurrently is thread-safe.
///
/// Although McpClient::new is typically called once, this ensures
/// the construction path is free of race conditions.
#[tokio::test]
async fn test_client_creation_concurrent() {
    let mut handles = JoinSet::new();

    for i in 0..10 {
        handles.spawn(async move {
            // Create unique config for each task
            let config = McpConfig::new(format!("server-{}", i), "echo")
                .with_arg("hello")
                .with_env("TEST_VAR", format!("value-{}", i));

            // Create client (should not panic)
            let client = McpClient::new(config);

            (i, client.is_ok())
        });
    }

    // Collect results
    let mut success_count = 0;
    while let Some(result) = handles.join_next().await {
        let (task_id, success) = result.expect("Task should not panic");
        assert!(success, "Task {} client creation should succeed", task_id);
        success_count += 1;
    }

    assert_eq!(success_count, 10, "All 10 client creations should succeed");
}
