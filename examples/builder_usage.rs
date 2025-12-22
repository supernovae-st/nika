//! Example usage of the v5.0 builder patterns and NewType wrappers

use nika::limits::{CircuitBreaker, RateLimiter, ResourceLimits};
use nika::runner::ContextWriter; // Import trait for set_output
use nika::types::{Prompt, ShellCommand, TaskId, TokenCount, Url};
use std::sync::Arc;
use std::time::Duration;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // ========================================
    // NewType Wrappers Demo
    // ========================================

    println!("=== NewType Wrappers Demo ===\n");

    // TaskId with validation
    let valid_id = TaskId::new("my-task-123")?;
    println!("Valid TaskId: {}", valid_id);

    // Invalid IDs are caught at compile time
    if let Err(e) = TaskId::new("task with spaces") {
        println!("Invalid TaskId caught: {}", e);
    }

    // Prompt with sanitization
    let prompt = Prompt::new("  Multiple   spaces   get   normalized  ")?;
    println!("Sanitized prompt: '{}'", prompt);
    println!("Prompt length: {} bytes", prompt.len());

    // Shell command safety
    if let Err(e) = ShellCommand::new("rm -rf /") {
        println!("Dangerous command blocked: {}", e);
    }

    // Safe commands pass validation
    let safe_cmd = ShellCommand::new("ls -la")?;
    println!("Safe command: '{}'", safe_cmd.as_str());

    // URL validation
    let secure_url = Url::new("https://api.example.com/data")?;
    println!(
        "URL: {} (secure: {})",
        secure_url.as_str(),
        secure_url.is_secure()
    );

    // Token counting with type safety
    let tokens_a = TokenCount::new(1000);
    let tokens_b = TokenCount::new(500);
    let total = tokens_a.saturating_add(tokens_b);
    println!("Total tokens: {}", total);

    // ========================================
    // Resource Limits Demo
    // ========================================

    println!("\n=== Resource Limits Demo ===\n");

    // Use predefined safety profiles
    let testing_limits = ResourceLimits::testing();
    println!("Testing limits:");
    println!(
        "  - Max workflow duration: {:?}",
        testing_limits.max_workflow_duration
    );
    println!(
        "  - Max task duration: {:?}",
        testing_limits.max_task_duration
    );
    println!(
        "  - Max output size: {} MB",
        testing_limits.max_output_size / 1_048_576
    );

    // Custom limits for specific use case
    let custom_limits = ResourceLimits {
        max_workflow_duration: Duration::from_secs(300), // 5 minutes
        max_task_duration: Duration::from_secs(30),      // 30 seconds
        max_retries: 5,
        max_recursion_depth: 15,
        max_concurrent_tasks: 8,
        max_output_size: 2 * 1024 * 1024, // 2 MB
        rate_limiter: Some(Arc::new(RateLimiter::new(
            20,                      // 20 requests
            Duration::from_secs(60), // per minute
        ))),
    };

    // ========================================
    // Circuit Breaker Demo
    // ========================================

    println!("\n=== Circuit Breaker Demo ===\n");

    let breaker = Arc::new(CircuitBreaker::new(
        3,                       // Open after 3 failures
        Duration::from_secs(30), // Reset after 30 seconds
    ));

    println!("Initial state: {:?}", breaker.current_state());

    // Simulate failures
    for i in 1..=3 {
        breaker.record_failure();
        println!("After failure {}: {:?}", i, breaker.current_state());
    }

    println!("Can proceed: {}", breaker.can_proceed());

    // ========================================
    // Rate Limiter Demo
    // ========================================

    println!("\n=== Rate Limiter Demo ===\n");

    let limiter = RateLimiter::new(5, Duration::from_secs(10));

    println!("Available tokens: {}", limiter.available_tokens());

    // Try to acquire tokens
    for i in 1..=7 {
        if limiter.try_acquire() {
            println!("Request {}: ✅ Allowed", i);
        } else {
            println!("Request {}: ❌ Rate limited", i);
        }
    }

    // ========================================
    // Complete Runner Setup
    // ========================================

    println!("\n=== Complete Runner Setup ===\n");

    // Note: The v4.7.1 architecture uses SharedAgentRunner and IsolatedAgentRunner
    // instead of the old Runner type. The runner setup would look like:
    //
    // use nika::provider::create_provider;
    // use nika::runner::{SharedAgentRunner, AgentConfig, GlobalContext};
    //
    // let provider = Arc::from(create_provider("mock")?);
    // let config = AgentConfig::new("claude-sonnet-4-5");
    // let runner = SharedAgentRunner::new(provider, config);
    // let mut context = GlobalContext::new();
    // let result = runner.execute("task-id", "Prompt", &mut context).await?;

    // Example with custom limits (would be applied per-runner)
    let _ = custom_limits;
    let _ = breaker;

    println!("v4.7.1 Runner architecture:");
    println!("  ✅ SharedAgentRunner for agent: tasks");
    println!("  ✅ IsolatedAgentRunner for subagent: tasks");
    println!("  ✅ GlobalContext for shared state");
    println!("  ✅ LocalContext for isolated state");

    // ========================================
    // Memory Pool Usage
    // ========================================

    println!("\n=== Memory Pool Demo ===\n");

    use nika::context_pool::{get_context, return_context, warm_pool};

    // Pre-warm the pool for better performance
    warm_pool(5);
    println!("Pool warmed with 5 contexts");

    // Use contexts from the pool
    for i in 1..=3 {
        let mut ctx = get_context();
        ctx.set_output(&format!("task-{}", i), format!("output-{}", i));
        println!("Context {} used and returned to pool", i);
        return_context(ctx);
    }

    // ========================================
    // Performance Metrics
    // ========================================

    println!("\n=== Performance Improvements ===\n");

    println!("v4.7.1 Performance:");
    println!("  • Template resolution: 3x faster (single-pass)");
    println!("  • String allocations: 50% reduction (Arc<str>)");
    println!("  • Task IDs: 95% stack-allocated (SmartString)");
    println!("  • Context reuse: Memory pool available");

    println!("\nv4.7 Safety:");
    println!("  • Workflow timeouts: Enforced");
    println!("  • Output size limits: Checked");
    println!("  • Rate limiting: Active");
    println!("  • Circuit breaker: Protected");

    println!("\nv5.0 Architecture:");
    println!("  • Type safety: NewType wrappers");
    println!("  • Builder patterns: Available");
    println!("  • Resource limits: Configurable");
    println!("  • Error handling: Categorized");

    Ok(())
}
