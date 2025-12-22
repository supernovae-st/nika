# Nika CLI v4.7 Safety Features

## Overview

Nika CLI v4.7 introduces comprehensive safety features to prevent runaway workflows, protect against resource exhaustion, and handle external service failures gracefully.

## Resource Limits

### Configuration

Resource limits are configured through the `ResourceLimits` struct:

```rust
use nika::limits::ResourceLimits;
use nika::Runner;

// Use predefined profiles
let runner = Runner::new("claude")?
    .with_limits(ResourceLimits::testing());   // Strict limits for testing
    // or
    .with_limits(ResourceLimits::production()); // Balanced for production
    // or
    .with_limits(ResourceLimits::unlimited());  // Relaxed limits
```

### Available Limits

| Limit | Default | Testing | Description |
|-------|---------|---------|-------------|
| `max_workflow_duration` | 1 hour | 1 minute | Total workflow execution time |
| `max_task_duration` | 5 minutes | 10 seconds | Individual task timeout |
| `max_retries` | 3 | 1 | Retry attempts per task |
| `max_recursion_depth` | 10 | 3 | Maximum task nesting |
| `max_concurrent_tasks` | 10 | 2 | Parallel task limit |
| `max_output_size` | 10 MB | 1 MB | Task output size limit |

### Custom Limits

```rust
use std::time::Duration;
use nika::limits::ResourceLimits;

let limits = ResourceLimits {
    max_workflow_duration: Duration::from_secs(600), // 10 minutes
    max_task_duration: Duration::from_secs(60),      // 1 minute
    max_retries: 5,
    max_recursion_depth: 20,
    max_concurrent_tasks: 5,
    max_output_size: 5 * 1024 * 1024, // 5 MB
    rate_limiter: None,
};

let runner = Runner::new("claude")?
    .with_limits(limits);
```

## Rate Limiting

### Token Bucket Implementation

Prevent API abuse with configurable rate limits:

```rust
use nika::limits::RateLimiter;
use std::sync::Arc;

// Allow 10 requests per minute
let rate_limiter = Arc::new(
    RateLimiter::new(10, Duration::from_secs(60))
);

let limits = ResourceLimits {
    rate_limiter: Some(rate_limiter),
    ..ResourceLimits::default()
};
```

### Usage in Workflows

```yaml
tasks:
  - id: api-call
    http: "https://api.example.com/data"
    config:
      rateLimit: true  # Respects global rate limiter
```

## Circuit Breaker

### Preventing Cascade Failures

The circuit breaker pattern prevents repeated calls to failing services:

```rust
use nika::limits::CircuitBreaker;
use std::sync::Arc;

// Open circuit after 3 failures, reset after 30 seconds
let breaker = Arc::new(
    CircuitBreaker::new(3, Duration::from_secs(30))
);

let runner = Runner::new("claude")?
    .with_circuit_breaker(breaker);
```

### Circuit States

1. **Closed**: Normal operation, requests proceed
2. **Open**: Too many failures, requests rejected immediately
3. **Half-Open**: Testing recovery, limited requests allowed

### Automatic Recovery

- After the reset timeout, the circuit enters half-open state
- A successful request in half-open closes the circuit
- A failure in half-open immediately re-opens the circuit

## Timeout Configuration

### Task-Level Timeouts

Configure timeouts in workflow YAML:

```yaml
tasks:
  - id: slow-task
    shell: "long-running-command"
    config:
      timeout: "30s"  # Supports: ms, s, m, h
```

### Global Timeout Enforcement

The runner enforces global timeouts:

```rust
// Workflow will fail if it runs longer than limits.max_workflow_duration
let result = runner.run(&workflow)?;
```

## Memory Management

### Context Pool

Reuse execution contexts to reduce allocations:

```rust
use nika::context_pool::{warm_pool, get_context, return_context};

// Pre-warm the pool with 10 contexts
warm_pool(10);

// Get a context from the pool
let mut ctx = get_context();
// ... use context ...

// Return to pool for reuse (automatically cleared)
return_context(ctx);
```

### Smart String Optimization

Task IDs use `SmartString` for efficient storage:
- IDs ≤31 chars: Stack allocated (inline)
- IDs >31 chars: Heap allocated
- 95% of typical task IDs avoid heap allocation

## Error Categories

Enhanced error handling with categories:

```rust
pub enum ErrorCategory {
    Timeout,        // Task exceeded timeout
    Network,        // Network/connectivity issue
    Provider,       // LLM provider error
    Validation,     // Input validation failure
    Permission,     // Access denied
    ResourceLimit,  // Resource limit exceeded
    Unknown,        // Unclassified error
}
```

## Best Practices

### 1. Choose Appropriate Profiles

- **Development**: Use `ResourceLimits::testing()` for quick feedback
- **Production**: Use `ResourceLimits::production()` for balanced safety
- **Batch Jobs**: Consider `ResourceLimits::unlimited()` with monitoring

### 2. Handle Timeouts Gracefully

```rust
match runner.run(&workflow) {
    Ok(result) => {
        // Check for task timeouts
        for task in &result.results {
            if task.is_timeout() {
                log::warn!("Task {} timed out", task.task_id);
            }
        }
    }
    Err(e) if e.to_string().contains("timeout") => {
        log::error!("Workflow timeout: {}", e);
    }
    Err(e) => {
        log::error!("Workflow failed: {}", e);
    }
}
```

### 3. Monitor Resource Usage

```rust
let result = runner.run(&workflow)?;

println!("Workflow: {}", result.workflow_name);
println!("Tasks completed: {}", result.tasks_completed);
println!("Tasks failed: {}", result.tasks_failed);
println!("Total tokens: {}", result.total_tokens);

// Check output sizes
for task_result in &result.results {
    if task_result.output.len() > 1_000_000 {
        log::warn!("Large output from {}: {} bytes",
                  task_result.task_id,
                  task_result.output.len());
    }
}
```

### 4. Implement Retry Strategies

```yaml
tasks:
  - id: flaky-api
    http: "https://unstable-api.example.com"
    config:
      retries: 3
      retryBackoff: "exponential"  # or "linear", "constant"
      retryDelay: "1s"
```

### 5. Use Circuit Breakers for External Services

```rust
// Share circuit breaker across multiple runners
let breaker = Arc::new(CircuitBreaker::new(5, Duration::from_secs(60)));

let runner1 = Runner::new("claude")?
    .with_circuit_breaker(breaker.clone());

let runner2 = Runner::new("claude")?
    .with_circuit_breaker(breaker.clone());
```

## Monitoring & Observability

### Metrics to Track

1. **Workflow Duration**: Compare against `max_workflow_duration`
2. **Task Success Rate**: `tasks_completed / (tasks_completed + tasks_failed)`
3. **Retry Rate**: Track how often retries are needed
4. **Circuit Breaker State**: Monitor open/closed transitions
5. **Rate Limiter Tokens**: Track available tokens over time
6. **Output Sizes**: Monitor for unexpectedly large outputs
7. **Context Pool Hit Rate**: Track pool efficiency

### Example Monitoring Integration

```rust
use prometheus::{Counter, Histogram, register_counter, register_histogram};

lazy_static! {
    static ref WORKFLOW_DURATION: Histogram = register_histogram!(
        "nika_workflow_duration_seconds",
        "Workflow execution duration"
    ).unwrap();

    static ref TASK_TIMEOUTS: Counter = register_counter!(
        "nika_task_timeouts_total",
        "Total number of task timeouts"
    ).unwrap();
}

// In your code
let timer = WORKFLOW_DURATION.start_timer();
let result = runner.run(&workflow)?;
timer.observe_duration();

for task in &result.results {
    if task.is_timeout() {
        TASK_TIMEOUTS.inc();
    }
}
```

## Migration Guide

### From v4.6 to v4.7

1. **Update Runner Creation**:
```rust
// Before (v4.6)
let runner = Runner::new("claude")?;

// After (v4.7)
let runner = Runner::new("claude")?
    .with_limits(ResourceLimits::production());
```

2. **Add Timeout Configuration**:
```yaml
# Add to existing tasks
tasks:
  - id: existing-task
    shell: "command"
    config:
      timeout: "30s"  # New in v4.7
```

3. **Handle New Error Types**:
```rust
// Check for specific error categories
if task_result.is_timeout() {
    // Handle timeout specifically
}
```

## Performance Impact

The safety features have minimal performance overhead:

- **Template Resolution**: 3x faster with caching
- **Context Pool**: ~10% reduction in allocations
- **SmartString**: 95% of IDs avoid heap allocation
- **Rate Limiting**: <1μs per check
- **Circuit Breaker**: <100ns per check

## Troubleshooting

### Common Issues

1. **"Workflow timeout exceeded"**
   - Increase `max_workflow_duration`
   - Optimize slow tasks
   - Add task-level timeouts

2. **"Output exceeds size limit"**
   - Increase `max_output_size`
   - Stream large outputs instead
   - Compress output data

3. **"Circuit breaker open"**
   - Check external service health
   - Adjust failure threshold
   - Increase reset timeout

4. **"Rate limit exceeded"**
   - Reduce request frequency
   - Increase rate limit capacity
   - Implement request batching