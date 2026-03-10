# MP3: Control Flow Fix

**Parent**: `2026-03-10-v0.24.0-bugfix-masterplan.md`
**Priority**: 🔴 HIGH
**Files**: `src/runtime/runner.rs`
**Estimated**: 2-3 hours

---

## Problem Statement

Two bugs in the control flow system cause incorrect behavior:

### Bug 1: fail_fast Doesn't Abort In-Flight Tasks

**File**: `src/runtime/runner.rs` (lines 1112-1133)

```rust
// CURRENT (WRONG ORDER):
let _permit = match semaphore.acquire().await {  // 1. Acquire permit (BLOCKS)
    Ok(permit) => permit,
    Err(_) => { ... }
};

if cancelled.load(Ordering::Relaxed) {  // 2. Check cancelled (TOO LATE!)
    return Err(...);
}
```

**Problem**: The semaphore is acquired BEFORE checking the cancelled flag. This means:
- If `concurrency=3` and task 1 fails with `fail_fast=true`
- Tasks 2 and 3 are already waiting on semaphore
- They will acquire permits and execute even after fail_fast triggers
- Only task 4+ will see the cancelled flag

### Bug 2: Failed Dependency Shows "Deadlock" Error

**File**: `src/runtime/runner.rs` (line 246)

```rust
// In get_ready_tasks():
deps.iter().all(|dep| self.datastore.is_success(dep))
```

**Problem**: If a dependency FAILED (not success), `is_success()` returns false, so the dependent task is never ready. The runner then sees "no tasks running, no tasks ready" and reports a **deadlock** error.

**Expected**: Should report "Task X cannot run because dependency Y failed"

---

## Solution: Bug 1 (fail_fast)

### Analysis

The fix requires checking cancellation BEFORE acquiring the semaphore permit. Additionally, we should use `JoinSet::abort_all()` to actually cancel in-flight tasks.

### Implementation

**File**: `src/runtime/runner.rs`

```rust
// In for_each execution (around line 1087):
for (index, item) in items.iter().enumerate() {
    // 1. Check cancellation BEFORE spawning
    if fail_fast && cancelled.load(Ordering::Relaxed) {
        tracing::debug!(
            task_id = %task_id,
            index = index,
            "Skipping iteration due to fail_fast cancellation"
        );
        continue;
    }

    let task_id = task_id.clone();
    let item = item.clone();
    let var_name = var_name.clone();
    let task = task.clone();
    let executor = executor.clone();
    let datastore = datastore.clone();
    let semaphore = Arc::clone(&semaphore);
    let cancelled = Arc::clone(&cancelled);

    join_set.spawn(async move {
        // 2. Check cancellation BEFORE acquiring semaphore
        if cancelled.load(Ordering::Relaxed) {
            return Err(NikaError::TaskCancelled {
                task_id: task_id.to_string(),
                reason: "Cancelled due to fail_fast before semaphore acquire".to_string(),
            });
        }

        // 3. Try to acquire semaphore with cancellation check
        let _permit = tokio::select! {
            permit = semaphore.acquire() => {
                match permit {
                    Ok(p) => p,
                    Err(_) => return Err(NikaError::TaskCancelled {
                        task_id: task_id.to_string(),
                        reason: "Semaphore closed".to_string(),
                    }),
                }
            }
            // If cancelled while waiting for semaphore, abort immediately
            _ = async {
                while !cancelled.load(Ordering::Relaxed) {
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            } => {
                return Err(NikaError::TaskCancelled {
                    task_id: task_id.to_string(),
                    reason: "Cancelled while waiting for semaphore".to_string(),
                });
            }
        };

        // 4. Final check after acquiring permit
        if cancelled.load(Ordering::Relaxed) {
            return Err(NikaError::TaskCancelled {
                task_id: task_id.to_string(),
                reason: "Cancelled due to fail_fast after semaphore acquire".to_string(),
            });
        }

        // Execute task...
        let result = executor.execute_task(&task, Some((var_name, item))).await;

        // 5. If failed and fail_fast, trigger cancellation AND abort others
        if !result.result.is_success() && fail_fast {
            cancelled.store(true, Ordering::SeqCst);  // Use SeqCst for visibility
        }

        Ok(result)
    });
}

// 6. Collect results, but abort remaining on first failure
let mut results = Vec::new();
while let Some(result) = join_set.join_next().await {
    match result {
        Ok(Ok(task_result)) => {
            if !task_result.result.is_success() && fail_fast {
                // Abort all remaining tasks immediately
                join_set.abort_all();
                results.push(task_result);

                // Drain remaining (they'll return JoinError::Cancelled)
                while let Some(_) = join_set.join_next().await {}
                break;
            }
            results.push(task_result);
        }
        Ok(Err(e)) => {
            // Task returned error (cancelled)
            tracing::debug!(error = %e, "Task cancelled or errored");
        }
        Err(join_error) => {
            // Task was aborted or panicked
            if join_error.is_cancelled() {
                tracing::debug!("Task was aborted due to fail_fast");
            }
        }
    }
}
```

### Alternative: Use CancellationToken

Better approach using tokio_util's CancellationToken:

```rust
use tokio_util::sync::CancellationToken;

// In for_each execution:
let cancel_token = CancellationToken::new();

for (index, item) in items.iter().enumerate() {
    if cancel_token.is_cancelled() {
        continue;
    }

    let child_token = cancel_token.child_token();
    let semaphore = Arc::clone(&semaphore);

    join_set.spawn(async move {
        tokio::select! {
            biased;  // Check cancellation first

            _ = child_token.cancelled() => {
                Err(NikaError::TaskCancelled {
                    task_id: task_id.to_string(),
                    reason: "Cancelled by fail_fast".to_string(),
                })
            }

            result = async {
                let _permit = semaphore.acquire().await?;

                if child_token.is_cancelled() {
                    return Err(...);
                }

                executor.execute_task(&task, Some((var_name, item))).await
            } => {
                result
            }
        }
    });
}

// On first failure:
if !result.is_success() && fail_fast {
    cancel_token.cancel();  // Cancels all child tokens
    join_set.abort_all();   // Aborts spawned tasks
}
```

---

## Solution: Bug 2 (Deadlock Error)

### Analysis

The issue is in `get_ready_tasks()` which checks `is_success(dep)`. If a dependency FAILED, it returns false forever, and the dependent task is never returned as "ready".

The workflow then hits line 860 which reports a generic "deadlock" error.

### Implementation

**File**: `src/runtime/runner.rs`

#### Step 1: Track Failed Dependencies

```rust
fn get_ready_tasks(&self) -> Vec<Arc<Task>> {
    self.workflow
        .tasks
        .iter()
        .filter(|task| {
            let task_id = &task.id;

            // Skip if already completed
            if self.datastore.is_completed(task_id) {
                return false;
            }

            // Skip if already running
            if self.running_tasks.contains(task_id) {
                return false;
            }

            // Check dependencies
            let deps = self.get_task_dependencies(task);
            for dep in &deps {
                // Check if dependency exists
                if !self.datastore.has_result(dep) {
                    // Dependency hasn't run yet - task not ready
                    return false;
                }

                // Check if dependency succeeded
                if !self.datastore.is_success(dep) {
                    // Dependency FAILED - task cannot run
                    // Mark this task as failed due to dependency
                    self.mark_task_dependency_failed(task_id, dep);
                    return false;
                }
            }

            true
        })
        .cloned()
        .collect()
}

fn mark_task_dependency_failed(&self, task_id: &str, failed_dep: &str) {
    // Store that this task cannot run due to failed dependency
    let result = TaskResult {
        task_id: task_id.to_string(),
        result: TaskResultVariant::DependencyFailed {
            dependency: failed_dep.to_string(),
        },
        output: None,
        duration: Duration::ZERO,
    };
    self.datastore.store(task_id, result);

    // Emit event
    self.log.emit(EventKind::TaskFailed {
        task_id: Arc::from(task_id),
        error: format!("Cannot run: dependency '{}' failed", failed_dep),
        duration_ms: 0,
    });
}
```

#### Step 2: Add TaskResultVariant for Dependency Failure

**File**: `src/runtime/result.rs` (or wherever TaskResultVariant is defined)

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TaskResultVariant {
    Success(Value),
    Failed {
        error: String,
        code: Option<String>,
    },
    DependencyFailed {  // NEW
        dependency: String,
    },
    Skipped {
        reason: String,
    },
    Cancelled {
        reason: String,
    },
}

impl TaskResultVariant {
    pub fn is_success(&self) -> bool {
        matches!(self, TaskResultVariant::Success(_))
    }

    pub fn is_dependency_failed(&self) -> bool {
        matches!(self, TaskResultVariant::DependencyFailed { .. })
    }
}
```

#### Step 3: Update Deadlock Detection

```rust
// In run_dag() around line 860:
if ready.is_empty() {
    // No tasks ready - check why

    // 1. Check for failed dependencies
    let failed_deps: Vec<_> = self.workflow.tasks.iter()
        .filter(|t| self.datastore.is_dependency_failed(&t.id))
        .map(|t| t.id.clone())
        .collect();

    if !failed_deps.is_empty() {
        // Not a deadlock - tasks blocked by failed dependencies
        return Err(NikaError::DependencyChainFailed {
            blocked_tasks: failed_deps,
            message: "One or more tasks cannot run because their dependencies failed".to_string(),
        });
    }

    // 2. Check for actual cycle (true deadlock)
    if self.detect_cycle() {
        return Err(NikaError::CyclicDependency {
            cycle: self.find_cycle_path(),
        });
    }

    // 3. Unknown deadlock (shouldn't happen)
    return Err(NikaError::WorkflowDeadlock {
        running_tasks: self.running_tasks.iter().cloned().collect(),
        pending_tasks: self.get_pending_tasks(),
    });
}
```

#### Step 4: Add New Error Variants

**File**: `src/error.rs`

```rust
#[derive(Debug, Error)]
pub enum NikaError {
    // ... existing variants ...

    #[error("[NIKA-025] Dependency chain failed: {message}")]
    DependencyChainFailed {
        blocked_tasks: Vec<String>,
        message: String,
    },

    #[error("[NIKA-026] Task '{task_id}' cannot run: dependency '{dependency}' failed")]
    TaskDependencyFailed {
        task_id: String,
        dependency: String,
    },
}
```

---

## Test Plan

### Bug 1 Tests (fail_fast)

```rust
#[tokio::test]
async fn fail_fast_aborts_in_flight_tasks() {
    // Create workflow with 5 parallel tasks, task 2 fails
    let workflow = create_parallel_workflow(5, 2); // 5 tasks, #2 fails
    let runner = Runner::new(workflow, log);

    let result = runner.run().await;

    // Should fail (task 2 failed)
    assert!(result.is_err());

    // Check that tasks 3-5 were cancelled, not executed
    let events = runner.log.events();
    let cancelled_count = events.iter()
        .filter(|e| matches!(&e.kind, EventKind::TaskCancelled { .. }))
        .count();

    // At least some tasks should have been cancelled
    assert!(cancelled_count > 0, "fail_fast should have cancelled some tasks");
}

#[tokio::test]
async fn fail_fast_with_high_concurrency() {
    // Stress test: 100 tasks, concurrency=50, task 10 fails
    let workflow = create_parallel_workflow(100, 10);
    let runner = Runner::new(workflow, log);

    let start = Instant::now();
    let result = runner.run().await;
    let duration = start.elapsed();

    assert!(result.is_err());

    // Should complete quickly (not wait for all 100 tasks)
    assert!(duration < Duration::from_secs(5), "fail_fast should abort quickly");
}
```

### Bug 2 Tests (Deadlock Error)

```rust
#[tokio::test]
async fn failed_dependency_shows_clear_error() {
    // Create workflow: A -> B -> C, where A fails
    let workflow = create_chain_workflow(vec!["A", "B", "C"], "A"); // A fails
    let runner = Runner::new(workflow, log);

    let result = runner.run().await;

    assert!(result.is_err());
    let error = result.unwrap_err();

    // Should NOT say "deadlock"
    assert!(!error.to_string().contains("deadlock"),
        "Error should not mention deadlock");

    // Should mention failed dependency
    assert!(error.to_string().contains("dependency") ||
            error.to_string().contains("failed"),
        "Error should explain dependency failure: {}", error);
}

#[tokio::test]
async fn dependency_failed_events_emitted() {
    let workflow = create_chain_workflow(vec!["A", "B"], "A");
    let runner = Runner::new(workflow, log);

    let _ = runner.run().await;

    let events = runner.log.events();

    // B should have DependencyFailed event
    let dep_failed = events.iter()
        .filter(|e| matches!(&e.kind, EventKind::TaskFailed { task_id, error, .. }
            if task_id.as_ref() == "B" && error.contains("dependency")))
        .count();

    assert_eq!(dep_failed, 1, "Should emit dependency failed event for B");
}
```

---

## Success Criteria

### Bug 1 (fail_fast)

- [ ] Cancellation checked BEFORE semaphore acquire
- [ ] `join_set.abort_all()` called on first failure
- [ ] In-flight tasks actually stop (not just skipped)
- [ ] Test: 100 tasks with early failure completes quickly

### Bug 2 (Deadlock Error)

- [ ] New `TaskResultVariant::DependencyFailed` variant
- [ ] Clear error message: "dependency X failed"
- [ ] NOT "deadlock detected" for simple dependency failures
- [ ] Events track dependency failures

### Combined

- [ ] All existing DAG tests pass
- [ ] New regression tests added
- [ ] for_each stress tests pass

---

## Migration Notes

### Breaking Changes

- New error variant `NikaError::DependencyChainFailed`
- New `TaskResultVariant::DependencyFailed`

### Behavioral Changes

- fail_fast now actually cancels running tasks
- Dependency failures show clear error messages

---

## Related Issues

- Audit finding: "fail_fast doesn't abort in-flight tasks"
- Audit finding: "Failed dependency shows 'deadlock' error"
- Line 1114-1126: semaphore acquired before cancel check
- Line 246: `is_success()` returns false for failed deps
