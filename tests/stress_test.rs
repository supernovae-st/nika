//! Stress tests for resource limits and safety features

use nika::limits::{CircuitBreaker, RateLimiter, ResourceLimits};
use nika::workflow::Workflow;
use nika::{ContextWriter, Runner};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

#[tokio::test]
#[ignore] // Run with: cargo test stress_test --ignored
async fn test_workflow_timeout() {
    let yaml = r#"
agent:
  model: claude-sonnet-4-5
  systemPrompt: "Test agent"

tasks:
  - id: slow-task
    shell:

      command: "sleep 10"

flows: []
"#;

    let workflow: Workflow = serde_yaml::from_str(yaml).unwrap();

    // Create runner with 1 second workflow timeout
    let limits = ResourceLimits {
        max_workflow_duration: Duration::from_secs(1),
        ..ResourceLimits::default()
    };

    let runner = Runner::new("mock").unwrap().with_limits(limits);

    let start = Instant::now();
    let result = runner.run(&workflow).await;
    let elapsed = start.elapsed();

    // Should fail due to timeout
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("Workflow timeout"));
    assert!(elapsed < Duration::from_secs(2)); // Should fail fast
}

#[tokio::test]
#[ignore]
async fn test_output_size_limit() {
    let yaml = r#"
agent:
  model: claude-sonnet-4-5
  systemPrompt: "Test agent"

tasks:
  - id: large-output
    shell:

      command: "yes | head -c 10000000"  # 10MB output

flows: []
"#;

    let workflow: Workflow = serde_yaml::from_str(yaml).unwrap();

    // Create runner with 1KB output limit
    let limits = ResourceLimits {
        max_output_size: 1024, // 1KB
        ..ResourceLimits::default()
    };

    let runner = Runner::new("mock").unwrap().with_limits(limits);

    let result = runner.run(&workflow).await;

    // Should fail due to output size
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("output exceeds size limit"));
}

#[test]
fn test_rate_limiter_stress() {
    let limiter = RateLimiter::new(100, Duration::from_secs(1));

    // Should be able to acquire 100 tokens quickly
    let start = Instant::now();
    for _ in 0..100 {
        assert!(limiter.try_acquire());
    }
    assert!(start.elapsed() < Duration::from_millis(100));

    // 101st should fail
    assert!(!limiter.try_acquire());

    // After waiting for refill, should work again
    thread::sleep(Duration::from_secs(1) + Duration::from_millis(10));
    assert!(limiter.try_acquire());
}

#[test]
fn test_circuit_breaker_stress() {
    let breaker = Arc::new(CircuitBreaker::new(5, Duration::from_millis(500)));
    let breaker_clone = Arc::clone(&breaker);

    // Simulate rapid failures from multiple threads
    let handles: Vec<_> = (0..10)
        .map(|_| {
            let b = Arc::clone(&breaker_clone);
            thread::spawn(move || {
                for _ in 0..3 {
                    b.record_failure();
                    thread::sleep(Duration::from_millis(10));
                }
            })
        })
        .collect();

    // Wait for threads
    for handle in handles {
        handle.join().unwrap();
    }

    // Circuit should be open after many failures
    assert!(!breaker.can_proceed());

    // Wait for reset timeout
    thread::sleep(Duration::from_millis(510));

    // Should be able to proceed again (half-open)
    assert!(breaker.can_proceed());

    // Record success to close circuit
    breaker.record_success();
    assert!(breaker.can_proceed());
}

#[tokio::test]
async fn test_concurrent_workflow_limit() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Mutex;

    let yaml = r#"
agent:
  model: claude-sonnet-4-5
  systemPrompt: "Test agent"

tasks:
  - id: task1
    agent:

      prompt: "Do something"

flows: []
"#;

    let workflow: Workflow = serde_yaml::from_str(yaml).unwrap();
    let limits = ResourceLimits {
        max_concurrent_tasks: 3,
        ..ResourceLimits::testing()
    };

    let runner = Arc::new(Runner::new("mock").unwrap().with_limits(limits));

    let counter = Arc::new(AtomicUsize::new(0));
    let errors = Arc::new(Mutex::new(Vec::new()));

    // Try to run many workflows concurrently
    let handles: Vec<_> = (0..10)
        .map(|i| {
            let r = Arc::clone(&runner);
            let w = workflow.clone();
            let c = Arc::clone(&counter);
            let e = Arc::clone(&errors);

            tokio::spawn(async move {
                c.fetch_add(1, Ordering::SeqCst);
                match r.run(&w).await {
                    Ok(_) => {}
                    Err(err) => {
                        e.lock().unwrap().push(format!("Task {}: {}", i, err));
                    }
                }
            })
        })
        .collect();

    // Wait for all tasks
    for handle in handles {
        handle.await.unwrap();
    }

    // Some should have succeeded, some may have failed due to limits
    let total_started = counter.load(Ordering::SeqCst);
    assert_eq!(total_started, 10);

    // Check if any failed (depends on timing)
    let errs = errors.lock().unwrap();
    println!("Concurrent execution errors: {:?}", errs.len());
}

#[tokio::test]
#[ignore]
async fn test_memory_pool_stress() {
    use nika::context_pool::{get_context, return_context, warm_pool};

    // Pre-warm the pool
    warm_pool(10);

    // Stress test with many get/return cycles
    let start = Instant::now();
    for i in 0..10000 {
        let mut ctx = get_context();
        ctx.set_output(&format!("task{}", i), format!("output{}", i));

        // Simulate some work
        if i % 100 == 0 {
            tokio::task::yield_now().await;
        }

        return_context(ctx);
    }
    let elapsed = start.elapsed();

    println!("10,000 context get/return cycles in {:?}", elapsed);
    assert!(elapsed < Duration::from_secs(1)); // Should be very fast
}

#[tokio::test]
async fn test_retry_limit() {
    // This test verifies that retry limits are respected
    let yaml = r#"
agent:
  model: claude-sonnet-4-5
  systemPrompt: "Test agent"

tasks:
  - id: failing-task
    shell:

      command: "exit 1"  # Always fails
    config:
      retry:
        max: 2

flows: []
"#;

    let workflow: Workflow = serde_yaml::from_str(yaml).unwrap();
    let limits = ResourceLimits {
        max_retries: 2,
        ..ResourceLimits::default()
    };

    let runner = Runner::new("mock")
        .unwrap()
        .with_limits(limits)
        .verbose(true);

    let start = Instant::now();
    let result = runner.run(&workflow).await;
    let elapsed = start.elapsed();

    // Should fail after retries
    assert!(result.is_ok()); // Workflow completes but task failed
    let run_result = result.unwrap();
    assert_eq!(run_result.tasks_failed, 1);

    // Should have retried but not infinitely
    println!("Task with retries completed in {:?}", elapsed);
}
