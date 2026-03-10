# MP1: StructuredOutput Engine Fix

**Parent**: `2026-03-10-v0.24.0-bugfix-masterplan.md`
**Priority**: 💀 CRITICAL
**Files**: `src/runtime/structured_output.rs`, `src/runtime/executor.rs`
**Estimated**: 4-6 hours

---

## Problem Statement

The StructuredOutputEngine's **Layer 3 (Retry with Feedback)** and **Layer 4 (LLM Repair)** are completely non-functional. They claim to retry the LLM but actually just re-validate the **same failed output**.

### Evidence

```rust
// structured_output.rs:245-259 (Layer 3)
async fn try_layer_3(..., raw_output: &str, ...) -> Result<Value, NikaError> {
    // In a full implementation, this would:
    // 1. Generate error feedback from previous validation
    // 2. Re-call the LLM with the feedback     <-- NEVER HAPPENS
    // 3. Validate the new output
    //
    // For now, we just re-validate (since we can't call the LLM from here)
    // The executor will handle the actual retry loop   <-- FALSE

    let json_value = match extract_json(raw_output) {  // SAME raw_output!
```

```rust
// structured_output.rs:304-322 (Layer 4)
async fn try_layer_4(..., raw_output: &str, ...) -> Result<Value, NikaError> {
    // In a full implementation, this would:
    // 1. Call a repair LLM (self.spec.repair_model or default)
    // 2. Pass the invalid output and schema       <-- NEVER HAPPENS
    // 3. Get repaired JSON back
    // 4. Validate the repair
    //
    // For now, we make one final validation attempt  <-- JUST RE-VALIDATES SAME OUTPUT
```

### Impact

- `max_retries` field in StructuredOutputSpec is **completely ignored**
- `enable_retry: true` has **no effect**
- `enable_repair: true` has **no effect**
- Users think they have retry protection but they don't

---

## Solution Architecture

### Option A: Provider Callback (Recommended)

Pass an async callback to the engine that can call the LLM:

```rust
pub type InferCallback = Box<dyn Fn(&str) -> BoxFuture<'static, Result<String, NikaError>> + Send + Sync>;

pub struct StructuredOutputEngine {
    spec: StructuredOutputSpec,
    log: Arc<EventLog>,
    compiled_schema: Option<Value>,
    infer_fn: Option<InferCallback>,  // NEW: Callback to call LLM
}

impl StructuredOutputEngine {
    pub fn with_infer_callback(mut self, callback: InferCallback) -> Self {
        self.infer_fn = Some(callback);
        self
    }
}
```

### Option B: Provider Injection

Pass the RigProvider directly:

```rust
pub struct StructuredOutputEngine {
    spec: StructuredOutputSpec,
    log: Arc<EventLog>,
    compiled_schema: Option<Value>,
    provider: Option<Arc<RigProvider>>,  // NEW: Direct provider access
}
```

**Recommendation**: Option A (callback) is more flexible and doesn't couple the engine to RigProvider.

---

## Implementation Steps

### Step 1: Add Callback Type and Field

**File**: `src/runtime/structured_output.rs`

```rust
use std::future::Future;
use std::pin::Pin;

/// Callback type for LLM inference during retry/repair
pub type InferCallback = Arc<dyn Fn(String) -> Pin<Box<dyn Future<Output = Result<String, NikaError>> + Send>> + Send + Sync>;

pub struct StructuredOutputEngine {
    spec: StructuredOutputSpec,
    log: Arc<EventLog>,
    compiled_schema: Option<Value>,
    infer_fn: Option<InferCallback>,  // NEW
}

impl StructuredOutputEngine {
    pub fn new(spec: StructuredOutputSpec, log: Arc<EventLog>) -> Self {
        Self {
            spec,
            log,
            compiled_schema: None,
            infer_fn: None,
        }
    }

    /// Set the inference callback for Layer 3 & 4
    pub fn with_infer_callback(mut self, callback: InferCallback) -> Self {
        self.infer_fn = Some(callback);
        self
    }
}
```

### Step 2: Implement Real Layer 3 (Retry with Feedback)

**File**: `src/runtime/structured_output.rs`

```rust
async fn try_layer_3(
    &self,
    task_id: &Arc<str>,
    raw_output: &str,
    schema: &Value,
    retry_num: u8,
    attempt: u32,
) -> Result<Value, NikaError> {
    // Check if we have an inference callback
    let infer_fn = match &self.infer_fn {
        Some(f) => f,
        None => {
            // No callback - fall back to re-validation only
            // (emit warning event)
            self.log.emit(EventKind::StructuredOutputAttempt {
                task_id: Arc::clone(task_id),
                layer: 3,
                layer_name: LAYER_3_NAME.to_string(),
                attempt,
                success: false,
                error: Some("No infer callback - Layer 3 disabled".to_string()),
            });
            return Err(NikaError::StructuredOutputValidationFailed {
                task_id: task_id.to_string(),
                layer: LAYER_3_NAME.to_string(),
                attempt,
                errors: vec!["Layer 3 requires infer callback".to_string()],
            });
        }
    };

    // Get validation errors from raw output
    let validation_errors = self.collect_validation_errors(raw_output, schema).join("\n");

    // Generate retry prompt with feedback
    let original_prompt = ""; // TODO: Store original prompt in spec
    let retry_prompt = self.generate_retry_prompt(original_prompt, raw_output, &validation_errors);

    // Actually call the LLM!
    let new_output = infer_fn(retry_prompt).await.map_err(|e| {
        self.emit_attempt(task_id, 3, LAYER_3_NAME, attempt, false, Some(e.to_string()));
        e
    })?;

    // Validate the new output
    let json_value = match extract_json(&new_output) {
        Ok(v) => v,
        Err(e) => {
            self.emit_attempt(
                task_id,
                3,
                LAYER_3_NAME,
                attempt,
                false,
                Some(format!("retry {}: extraction failed: {}", retry_num, e)),
            );
            return Err(NikaError::StructuredOutputExtractionFailed {
                task_id: task_id.to_string(),
                layer: LAYER_3_NAME.to_string(),
                reason: e,
            });
        }
    };

    match validate_schema_ref(&json_value, &SchemaRef::Inline(schema.clone())).await {
        Ok(()) => {
            self.emit_attempt(task_id, 3, LAYER_3_NAME, attempt, true, None);
            Ok(json_value)
        }
        Err(e) => {
            self.emit_attempt(
                task_id,
                3,
                LAYER_3_NAME,
                attempt,
                false,
                Some(format!("retry {}: validation failed: {}", retry_num, e)),
            );
            Err(NikaError::StructuredOutputValidationFailed {
                task_id: task_id.to_string(),
                layer: LAYER_3_NAME.to_string(),
                attempt,
                errors: vec![e.to_string()],
            })
        }
    }
}
```

### Step 3: Implement Real Layer 4 (LLM Repair)

**File**: `src/runtime/structured_output.rs`

```rust
async fn try_layer_4(
    &self,
    task_id: &Arc<str>,
    raw_output: &str,
    schema: &Value,
    attempt: u32,
) -> Result<Value, NikaError> {
    // Check if we have an inference callback
    let infer_fn = match &self.infer_fn {
        Some(f) => f,
        None => {
            self.log.emit(EventKind::StructuredOutputAttempt {
                task_id: Arc::clone(task_id),
                layer: 4,
                layer_name: LAYER_4_NAME.to_string(),
                attempt,
                success: false,
                error: Some("No infer callback - Layer 4 disabled".to_string()),
            });
            return Err(NikaError::StructuredOutputValidationFailed {
                task_id: task_id.to_string(),
                layer: LAYER_4_NAME.to_string(),
                attempt,
                errors: vec!["Layer 4 requires infer callback".to_string()],
            });
        }
    };

    // Generate repair prompt
    let repair_prompt = self.generate_repair_prompt(raw_output, schema);

    // Call the LLM to repair the JSON
    let repaired_output = infer_fn(repair_prompt).await.map_err(|e| {
        self.emit_attempt(task_id, 4, LAYER_4_NAME, attempt, false, Some(e.to_string()));
        e
    })?;

    // Validate the repaired output
    let json_value = match extract_json(&repaired_output) {
        Ok(v) => v,
        Err(e) => {
            self.emit_attempt(task_id, 4, LAYER_4_NAME, attempt, false, Some(e.clone()));
            return Err(NikaError::StructuredOutputExtractionFailed {
                task_id: task_id.to_string(),
                layer: LAYER_4_NAME.to_string(),
                reason: e,
            });
        }
    };

    match validate_schema_ref(&json_value, &SchemaRef::Inline(schema.clone())).await {
        Ok(()) => {
            self.emit_attempt(task_id, 4, LAYER_4_NAME, attempt, true, None);
            Ok(json_value)
        }
        Err(e) => {
            self.emit_attempt(task_id, 4, LAYER_4_NAME, attempt, false, Some(e.to_string()));
            Err(NikaError::StructuredOutputValidationFailed {
                task_id: task_id.to_string(),
                layer: LAYER_4_NAME.to_string(),
                attempt,
                errors: vec![e.to_string()],
            })
        }
    }
}
```

### Step 4: Store Original Prompt for Retry Context

**File**: `src/ast/output.rs`

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StructuredOutputSpec {
    pub schema: SchemaRef,
    pub enable_tool_use: Option<bool>,
    pub enable_retry: Option<bool>,
    pub enable_repair: Option<bool>,
    pub max_retries: Option<u8>,
    pub repair_model: Option<String>,
    #[serde(skip)]  // NEW: Runtime-only field
    pub original_prompt: Option<String>,
}
```

### Step 5: Update Executor to Pass Callback

**File**: `src/runtime/executor.rs`

```rust
// In execute_infer_task() or wherever structured output is used:

let infer_callback: InferCallback = {
    let provider = self.provider.clone();
    let model = options.model.clone();
    Arc::new(move |prompt: String| {
        let provider = provider.clone();
        let model = model.clone();
        Box::pin(async move {
            provider.infer(&prompt, model.as_deref()).await
                .map_err(|e| NikaError::ProviderError {
                    provider: "rig".to_string(),
                    details: e.to_string(),
                })
        })
    })
};

let mut engine = StructuredOutputEngine::new(spec.clone(), self.log.clone())
    .with_infer_callback(infer_callback);

let result = engine.validate(task_id, &raw_output).await?;
```

---

## Test Plan

### Unit Tests

```rust
#[tokio::test]
async fn layer3_actually_retries_llm() {
    let call_count = Arc::new(AtomicU32::new(0));
    let call_count_clone = call_count.clone();

    // Mock callback that returns valid JSON on second call
    let callback: InferCallback = Arc::new(move |_prompt: String| {
        let count = call_count_clone.clone();
        Box::pin(async move {
            let n = count.fetch_add(1, Ordering::SeqCst);
            if n == 0 {
                // First call: return invalid JSON
                Ok(r#"{"invalid": true}"#.to_string())
            } else {
                // Second call: return valid JSON
                Ok(r#"{"name": "Alice", "age": 30}"#.to_string())
            }
        })
    });

    let spec = StructuredOutputSpec::with_inline_schema(create_user_schema());
    let log = Arc::new(EventLog::new());
    let mut engine = StructuredOutputEngine::new(spec, log)
        .with_infer_callback(callback);

    // First validation with invalid output
    let result = engine.validate("test", r#"{"invalid": true}"#).await;

    assert!(result.is_ok(), "Should succeed after retry");
    assert!(call_count.load(Ordering::SeqCst) >= 2, "Should have called LLM at least twice");
}

#[tokio::test]
async fn layer4_actually_repairs_json() {
    let call_count = Arc::new(AtomicU32::new(0));
    let call_count_clone = call_count.clone();

    // Mock callback that returns repaired JSON
    let callback: InferCallback = Arc::new(move |prompt: String| {
        let count = call_count_clone.clone();
        Box::pin(async move {
            count.fetch_add(1, Ordering::SeqCst);
            // Return valid JSON
            Ok(r#"{"name": "Repaired", "age": 25}"#.to_string())
        })
    });

    let mut spec = StructuredOutputSpec::with_inline_schema(create_user_schema());
    spec.enable_retry = Some(false);  // Skip Layer 3
    spec.enable_repair = Some(true);

    let log = Arc::new(EventLog::new());
    let mut engine = StructuredOutputEngine::new(spec, log)
        .with_infer_callback(callback);

    let result = engine.validate("test", "totally broken json").await;

    assert!(result.is_ok(), "Should succeed after repair");
    assert!(call_count.load(Ordering::SeqCst) >= 1, "Should have called repair LLM");
}

#[tokio::test]
async fn max_retries_is_respected() {
    let call_count = Arc::new(AtomicU32::new(0));
    let call_count_clone = call_count.clone();

    // Mock callback that always returns invalid JSON
    let callback: InferCallback = Arc::new(move |_prompt: String| {
        let count = call_count_clone.clone();
        Box::pin(async move {
            count.fetch_add(1, Ordering::SeqCst);
            Ok(r#"{"still_invalid": true}"#.to_string())
        })
    });

    let mut spec = StructuredOutputSpec::with_inline_schema(create_user_schema());
    spec.max_retries = Some(3);
    spec.enable_retry = Some(true);
    spec.enable_repair = Some(false);  // Skip Layer 4

    let log = Arc::new(EventLog::new());
    let mut engine = StructuredOutputEngine::new(spec, log)
        .with_infer_callback(callback);

    let result = engine.validate("test", r#"{"invalid": true}"#).await;

    assert!(result.is_err(), "Should fail after max retries");
    assert_eq!(call_count.load(Ordering::SeqCst), 3, "Should have retried exactly max_retries times");
}
```

### Integration Tests

- [ ] Test with real Claude API (extended thinking enabled)
- [ ] Test with OpenAI API (tool_use response_format)
- [ ] Test retry + repair chain (Layer 3 fails, Layer 4 succeeds)
- [ ] Test event emission (StructuredOutputAttempt events)

---

## Success Criteria

- [ ] Layer 3 actually calls the LLM with retry prompt
- [ ] Layer 4 actually calls the LLM with repair prompt
- [ ] `max_retries` field is respected
- [ ] Events correctly track which layer succeeded
- [ ] Existing Layer 2 tests still pass
- [ ] No callback = graceful degradation (Layer 2 only)
- [ ] 3+ new tests for Layer 3 & 4

---

## Rollback Plan

If issues are discovered:
1. Revert to Option A: Keep callback optional (None = old behavior)
2. Add feature flag: `enable_real_retry` in StructuredOutputSpec
3. Default to false for backwards compatibility

---

## Related Issues

- Audit finding: "StructuredOutput Layers 3 & 4 are FAKE"
- Comments in code admit incompleteness (lines 252-259, 313-322)
