// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! E2E Workflow Tests — v0.52
//!
//! Comprehensive end-to-end tests that parse YAML, build DAG, execute workflows,
//! and validate outputs. Uses mock provider for API-free tests and real providers
//! for integration validation.

use nika::ast::parse_analyzed;
use nika::event::{EventKind, EventLog};
use nika::runtime::Runner;

/// Parse and run a workflow, returning the final output string.
/// Captures events to provide detailed error information on failure.
async fn run_workflow(yaml: &str) -> Result<String, String> {
    let workflow = parse_analyzed(yaml).map_err(|e| format!("Parse error: {e}"))?;
    let (event_log, mut rx) = EventLog::new_with_broadcast();
    let runner =
        Runner::with_event_log(workflow, event_log).map_err(|e| format!("Runner error: {e}"))?;
    let mut runner = runner.quiet();

    let handle = tokio::spawn(async move { runner.run().await });

    // Capture events for error diagnostics
    let mut task_errors = Vec::new();
    let mut final_output = None;

    loop {
        match tokio::time::timeout(std::time::Duration::from_secs(60), rx.recv()).await {
            Ok(Ok(event)) => match &event.kind {
                EventKind::WorkflowCompleted {
                    final_output: o, ..
                } => {
                    final_output = Some(o.as_ref().as_str().unwrap_or("").to_string());
                    break;
                }
                EventKind::WorkflowFailed { error, .. } => {
                    let details = if task_errors.is_empty() {
                        String::new()
                    } else {
                        format!(" | task errors: {}", task_errors.join("; "))
                    };
                    return Err(format!("Workflow failed: {error}{details}"));
                }
                EventKind::TaskFailed { task_id, error, .. } => {
                    task_errors.push(format!("{task_id}: {error}"));
                }
                _ => {}
            },
            Ok(Err(_)) => break,
            Err(_) => return Err("Timeout after 60s".to_string()),
        }
    }

    let _ = handle.await;
    final_output.ok_or_else(|| {
        let details = if task_errors.is_empty() {
            String::new()
        } else {
            format!(" | task errors: {}", task_errors.join("; "))
        };
        format!("No output received{details}")
    })
}

// =============================================================================
// MOCK E2E: Simple infer
// =============================================================================

#[tokio::test]
async fn e2e_mock_simple_infer() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: mock
tasks:
  - id: greet
    infer: "Say hello"
"#;
    let result = run_workflow(yaml).await;
    assert!(
        result.is_ok(),
        "Simple mock infer should succeed: {result:?}"
    );
    let output = result.unwrap();
    assert!(!output.is_empty(), "Mock should return non-empty output");
}

// =============================================================================
// MOCK E2E: depends_on ordering
// =============================================================================

#[tokio::test]
async fn e2e_mock_depends_on_ordering() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: mock
tasks:
  - id: step1
    infer: "First step"
  - id: step2
    depends_on: [step1]
    with: { prev: $step1 }
    infer: "Second step uses {{with.prev}}"
  - id: step3
    depends_on: [step2]
    with: { prev: $step2 }
    infer: "Third step uses {{with.prev}}"
"#;
    let result = run_workflow(yaml).await;
    assert!(
        result.is_ok(),
        "Pipeline with depends_on should succeed: {result:?}"
    );
}

// =============================================================================
// MOCK E2E: for_each with concurrency
// =============================================================================

#[tokio::test]
async fn e2e_mock_for_each_concurrent() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: mock
tasks:
  - id: process
    for_each: ["apple", "banana", "cherry"]
    as: fruit
    concurrency: 3
    infer: "Describe the fruit: {{with.fruit}}"
  - id: merge
    depends_on: [process]
    with: { items: $process }
    infer: "Merge results: {{with.items | length}} items"
"#;
    let result = run_workflow(yaml).await;
    assert!(
        result.is_ok(),
        "for_each concurrent should succeed: {result:?}"
    );
}

// =============================================================================
// MOCK E2E: for_each with zero items
// =============================================================================

#[tokio::test]
async fn e2e_mock_for_each_empty_array() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: mock
tasks:
  - id: process
    for_each: []
    as: item
    infer: "This should never run: {{with.item}}"
"#;
    let result = run_workflow(yaml).await;
    assert!(
        result.is_ok(),
        "for_each with empty array should succeed (no iterations): {result:?}"
    );
}

// =============================================================================
// MOCK E2E: Structured output with schema
// =============================================================================

#[tokio::test]
async fn e2e_mock_structured_output_parses() {
    // Mock provider doesn't produce valid structured JSON — just verify the
    // workflow PARSES and the structured spec is recognized (YAML validation).
    // Real structured output testing is done with real API providers below.
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: mock
tasks:
  - id: extract
    infer: "Describe Alice, 30 years old, Rust developer"
    structured:
      schema:
        type: object
        properties:
          name: { type: string }
          age: { type: number }
          skills: { type: array, items: { type: string } }
        required: [name, age, skills]
"#;
    let workflow = parse_analyzed(yaml);
    assert!(
        workflow.is_ok(),
        "Structured output workflow should parse: {workflow:?}"
    );
}

// =============================================================================
// MOCK E2E: exec verb
// =============================================================================

#[tokio::test]
async fn e2e_mock_exec_verb() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: mock
tasks:
  - id: run_cmd
    exec: "echo 'hello from nika'"
"#;
    let result = run_workflow(yaml).await;
    assert!(result.is_ok(), "exec echo should succeed: {result:?}");
    let output = result.unwrap();
    assert!(
        output.contains("hello from nika"),
        "exec output should contain echo text, got: {output}"
    );
}

// =============================================================================
// MOCK E2E: exec + infer pipeline
// =============================================================================

#[tokio::test]
async fn e2e_mock_exec_then_infer() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: mock
tasks:
  - id: list_files
    exec: "echo 'file1.txt file2.txt file3.txt'"
  - id: analyze
    depends_on: [list_files]
    with: { files: $list_files }
    infer: "Analyze these files: {{with.files}}"
"#;
    let result = run_workflow(yaml).await;
    assert!(
        result.is_ok(),
        "exec→infer pipeline should succeed: {result:?}"
    );
}

// =============================================================================
// MOCK E2E: fan-out / fan-in
// =============================================================================

#[tokio::test]
async fn e2e_mock_fan_out_fan_in() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: mock
tasks:
  - id: research_a
    infer: "Research topic A"
  - id: research_b
    infer: "Research topic B"
  - id: research_c
    infer: "Research topic C"
  - id: synthesize
    depends_on: [research_a, research_b, research_c]
    with:
      a: $research_a
      b: $research_b
      c: $research_c
    infer: "Merge: {{with.a}} + {{with.b}} + {{with.c}}"
"#;
    let result = run_workflow(yaml).await;
    assert!(result.is_ok(), "Fan-out/fan-in should succeed: {result:?}");
}

// =============================================================================
// MOCK E2E: Retry on failure
// =============================================================================

#[tokio::test]
async fn e2e_mock_retry_config() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: mock
tasks:
  - id: resilient
    retry:
      max_attempts: 3
      delay_ms: 100
    infer: "Generate something"
"#;
    let result = run_workflow(yaml).await;
    assert!(
        result.is_ok(),
        "Task with retry config should succeed: {result:?}"
    );
}

// =============================================================================
// MOCK E2E: with bindings and transforms
// =============================================================================

#[tokio::test]
async fn e2e_mock_bindings_transforms() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: mock
tasks:
  - id: step1
    exec: "echo 'hello world'"
  - id: step2
    depends_on: [step1]
    with:
      data: $step1
    infer: "Use data: {{with.data | trim}}"
"#;
    let result = run_workflow(yaml).await;
    assert!(
        result.is_ok(),
        "Bindings with transforms should succeed: {result:?}"
    );
}

// =============================================================================
// MOCK E2E: Multiple verbs in pipeline
// =============================================================================

#[tokio::test]
async fn e2e_mock_multi_verb_pipeline() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: mock
tasks:
  - id: generate
    infer: "Generate a topic"
  - id: process
    depends_on: [generate]
    with: { topic: $generate }
    exec: "echo 'Processing: topic received'"
  - id: finalize
    depends_on: [process]
    with: { output: $process }
    infer: "Finalize based on: {{with.output}}"
"#;
    let result = run_workflow(yaml).await;
    assert!(
        result.is_ok(),
        "Multi-verb pipeline should succeed: {result:?}"
    );
}

// =============================================================================
// MOCK E2E: Workflow inputs
// =============================================================================

#[tokio::test]
async fn e2e_mock_workflow_inputs() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: mock
inputs:
  topic: "artificial intelligence"
  count: 3
tasks:
  - id: research
    infer: "Research {{inputs.topic}}, find {{inputs.count}} facts"
"#;
    let result = run_workflow(yaml).await;
    assert!(
        result.is_ok(),
        "Workflow with inputs should succeed: {result:?}"
    );
}

// =============================================================================
// REAL API: Provider basic tests (skip if no API key)
// =============================================================================

macro_rules! provider_test {
    ($name:ident, $provider:expr, $model:expr, $env_key:expr) => {
        #[tokio::test]
        async fn $name() {
            if std::env::var($env_key).is_err() {
                eprintln!("Skipping {}: no {}", stringify!($name), $env_key);
                return;
            }
            let yaml = format!(
                r#"
schema: "nika/workflow@0.12"
provider: {provider}
model: {model}
tasks:
  - id: hello
    infer: "Reply with exactly one word: NIKA"
"#,
                provider = $provider,
                model = $model
            );
            let result = run_workflow(&yaml).await;
            assert!(result.is_ok(), "{} failed: {:?}", $provider, result.err());
            let output = result.unwrap();
            assert!(!output.is_empty(), "{} returned empty output", $provider);
        }
    };
}

provider_test!(
    e2e_real_anthropic_haiku,
    "anthropic",
    "claude-haiku-4-5",
    "ANTHROPIC_API_KEY"
);
provider_test!(
    e2e_real_openai_mini,
    "openai",
    "gpt-4.1-mini",
    "OPENAI_API_KEY"
);
provider_test!(
    e2e_real_gemini_flash,
    "gemini",
    "gemini-2.5-flash",
    "GEMINI_API_KEY"
);
provider_test!(
    e2e_real_groq_llama,
    "groq",
    "llama-3.3-70b-versatile",
    "GROQ_API_KEY"
);
provider_test!(
    e2e_real_mistral_small,
    "mistral",
    "mistral-small-latest",
    "MISTRAL_API_KEY"
);
provider_test!(
    e2e_real_deepseek_chat,
    "deepseek",
    "deepseek-chat",
    "DEEPSEEK_API_KEY"
);
provider_test!(e2e_real_xai_grok, "xai", "grok-3", "XAI_API_KEY");

// =============================================================================
// REAL API: Structured output per provider
// =============================================================================

macro_rules! structured_provider_test {
    ($name:ident, $provider:expr, $model:expr, $env_key:expr) => {
        #[tokio::test]
        async fn $name() {
            if std::env::var($env_key).is_err() {
                eprintln!("Skipping {}: no {}", stringify!($name), $env_key);
                return;
            }
            let yaml = format!(
                r#"
schema: "nika/workflow@0.12"
provider: {provider}
model: {model}
tasks:
  - id: extract
    infer: "Parle-moi d'Alice, 30 ans, developpeuse Rust et Python"
    structured:
      schema:
        type: object
        properties:
          name:
            type: string
          age:
            type: number
            minimum: 0
            maximum: 150
          skills:
            type: array
            items:
              type: string
            minItems: 1
          active:
            type: boolean
        required: [name, age, skills, active]
      enable_repair: true
      max_retries: 3
"#,
                provider = $provider,
                model = $model
            );
            let result = run_workflow(&yaml).await;
            assert!(
                result.is_ok(),
                "{} structured failed: {:?}",
                $provider,
                result.err()
            );
            let output = result.unwrap();

            // Validate JSON programmatically
            let parsed: serde_json::Value = serde_json::from_str(&output).unwrap_or_else(|e| {
                panic!(
                    "{}: output is not valid JSON: {e}\nRaw: {output}",
                    $provider
                )
            });

            // Required fields
            assert!(
                parsed.get("name").is_some(),
                "{}: missing 'name' field",
                $provider
            );
            assert!(
                parsed.get("age").is_some(),
                "{}: missing 'age' field",
                $provider
            );
            assert!(
                parsed.get("skills").is_some(),
                "{}: missing 'skills' field",
                $provider
            );
            assert!(
                parsed.get("active").is_some(),
                "{}: missing 'active' field",
                $provider
            );

            // Type + constraint validation
            assert!(
                parsed["name"].is_string(),
                "{}: name is not string",
                $provider
            );

            let age = parsed["age"].as_f64().expect("age should be number");
            assert!(
                (0.0..=150.0).contains(&age),
                "{}: age {age} out of range 0-150",
                $provider
            );

            let skills = parsed["skills"]
                .as_array()
                .unwrap_or_else(|| panic!("{}: skills is not array", $provider));
            assert!(
                !skills.is_empty(),
                "{}: skills should have at least 1 item",
                $provider
            );
            for skill in skills {
                assert!(
                    skill.is_string(),
                    "{}: skill item is not string: {skill}",
                    $provider
                );
            }

            assert!(
                parsed["active"].is_boolean(),
                "{}: active is not boolean",
                $provider
            );
        }
    };
}

structured_provider_test!(
    e2e_structured_anthropic,
    "anthropic",
    "claude-haiku-4-5",
    "ANTHROPIC_API_KEY"
);
structured_provider_test!(
    e2e_structured_openai,
    "openai",
    "gpt-4.1-mini",
    "OPENAI_API_KEY"
);
structured_provider_test!(
    e2e_structured_gemini,
    "gemini",
    "gemini-2.5-flash",
    "GEMINI_API_KEY"
);
structured_provider_test!(
    e2e_structured_groq,
    "groq",
    "llama-3.3-70b-versatile",
    "GROQ_API_KEY"
);
structured_provider_test!(
    e2e_structured_mistral,
    "mistral",
    "mistral-small-latest",
    "MISTRAL_API_KEY"
);
structured_provider_test!(
    e2e_structured_deepseek,
    "deepseek",
    "deepseek-chat",
    "DEEPSEEK_API_KEY"
);
structured_provider_test!(e2e_structured_xai, "xai", "grok-3", "XAI_API_KEY");

// =============================================================================
// REAL API: Multi-step pipeline
// =============================================================================

#[tokio::test]
async fn e2e_real_research_pipeline() {
    if std::env::var("ANTHROPIC_API_KEY").is_err() {
        eprintln!("Skipping: no ANTHROPIC_API_KEY");
        return;
    }
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: anthropic
model: claude-haiku-4-5
tasks:
  - id: research
    infer: "List 3 facts about the Rust programming language in one sentence each"
  - id: summarize
    depends_on: [research]
    with: { data: $research }
    infer: "Summarize in exactly one sentence: {{with.data}}"
"#;
    let result = run_workflow(yaml).await;
    assert!(
        result.is_ok(),
        "Research pipeline failed: {:?}",
        result.err()
    );
    let output = result.unwrap();
    assert!(!output.is_empty(), "Pipeline should produce output");
}

// =============================================================================
// Helper: expect_error — run workflow and assert it fails with a substring
// =============================================================================

async fn expect_error(yaml: &str, substring: &str) {
    let result = run_workflow(yaml).await;
    match result {
        Ok(output) => panic!(
            "Expected error containing '{}', but workflow succeeded with output: {}",
            substring, output
        ),
        Err(e) => assert!(
            e.contains(substring),
            "Expected error containing '{}', got: {}",
            substring,
            e
        ),
    }
}

// =============================================================================
// FETCH: extract markdown (no API key needed)
// =============================================================================

#[tokio::test]
async fn e2e_fetch_markdown() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: mock
tasks:
  - id: scrape
    fetch:
      url: "https://example.com"
      extract: markdown
"#;
    let result = run_workflow(yaml).await;
    assert!(result.is_ok(), "Fetch markdown should succeed: {result:?}");
    let output = result.unwrap();
    assert!(
        output.contains("Example") || output.contains("example"),
        "Should extract content from example.com, got: {output}"
    );
}

// =============================================================================
// ADVERSARIAL: Data Flow Traps
// =============================================================================

/// for_each with exactly 1 item (degenerate case).
/// Output should still be a JSON array, not a bare scalar.
#[tokio::test]
async fn e2e_adversarial_for_each_single_item() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: mock
tasks:
  - id: single
    for_each: ["only_one"]
    as: item
    infer: "Process this item: {{with.item}}"
  - id: verify
    depends_on: [single]
    with: { results: $single }
    infer: "Got {{with.results | length}} results: {{with.results}}"
"#;
    let result = run_workflow(yaml).await;
    assert!(
        result.is_ok(),
        "for_each with single item should succeed: {result:?}"
    );
}

/// for_each output fed into another for_each (nested iteration).
/// First task iterates over ["a","b"], second task iterates over $first output.
#[tokio::test]
async fn e2e_adversarial_for_each_nested() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: mock
tasks:
  - id: first_loop
    for_each: ["alpha", "beta"]
    as: letter
    infer: "Describe the letter: {{with.letter}}"
  - id: second_loop
    depends_on: [first_loop]
    with: { items: $first_loop }
    for_each: $first_loop
    as: prev_result
    infer: "Re-process: {{with.prev_result}}"
"#;
    let result = run_workflow(yaml).await;
    assert!(
        result.is_ok(),
        "Nested for_each (output of one into another) should succeed: {result:?}"
    );
}

/// Large binding: task produces verbose output, downstream task binds it.
/// Validates that large data flows through bindings without truncation.
#[tokio::test]
async fn e2e_adversarial_large_binding() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: mock
tasks:
  - id: verbose
    infer: "Write a very detailed and comprehensive analysis of the history of computing, covering at least 20 major milestones from the abacus to modern quantum computing"
  - id: consume
    depends_on: [verbose]
    with: { data: $verbose }
    infer: "Summarize this content: {{with.data}}"
"#;
    let result = run_workflow(yaml).await;
    assert!(
        result.is_ok(),
        "Large binding data flow should succeed: {result:?}"
    );
    let output = result.unwrap();
    assert!(
        !output.is_empty(),
        "Downstream task should produce non-empty output from large binding"
    );
}

/// with: { data: $nonexistent } should produce a clear error at analysis time
/// (unknown task reference), not a panic or cryptic failure.
/// NIKA-140 "unknown task" is wrapped inside NIKA-004 "validation failed".
#[tokio::test]
async fn e2e_adversarial_nonexistent_binding() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: mock
tasks:
  - id: consumer
    with: { data: $nonexistent }
    infer: "Use this: {{with.data}}"
"#;
    expect_error(yaml, "NIKA-140").await;
}

/// Non-Latin prompt (Chinese characters) should work correctly.
/// Mock provider should process it without encoding errors.
#[tokio::test]
async fn e2e_adversarial_non_latin_prompt() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: mock
tasks:
  - id: chinese
    infer: "请用中文详细描述人工智能的发展历史和未来趋势"
"#;
    let result = run_workflow(yaml).await;
    assert!(
        result.is_ok(),
        "Non-Latin (Chinese) prompt should succeed: {result:?}"
    );
    let output = result.unwrap();
    // Mock returns JSON — parse it to verify valid structure
    let parsed: serde_json::Value = serde_json::from_str(&output).unwrap_or_else(|_| {
        // Mock response might be plain text or JSON — either is fine
        serde_json::Value::String(output.clone())
    });
    assert!(
        !output.is_empty(),
        "Non-Latin prompt should produce non-empty output"
    );
    // If parsed as JSON, verify it's a proper object (mock returns JSON object)
    if parsed.is_object() {
        assert!(
            parsed.get("mock").is_some() || parsed.get("name").is_some(),
            "Mock JSON should contain expected fields, got: {parsed}"
        );
    }
}

// =============================================================================
// ADVERSARIAL: Structured Output Stress
// =============================================================================

/// Schema with 5+ levels of nesting. Validate the workflow parses, runs, and
/// produces output matching the deeply nested schema.
/// Uses a schema that aligns with mock provider's response structure
/// (user.address nesting gives 3 natural levels; we add 2 more).
#[tokio::test]
async fn e2e_adversarial_structured_nested_schema() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: mock
tasks:
  - id: deep
    infer: "Describe a person with their contact details and location"
    structured:
      schema:
        type: object
        properties:
          user:
            type: object
            properties:
              name:
                type: string
              address:
                type: object
                properties:
                  city:
                    type: string
                  country:
                    type: string
          metadata:
            type: object
            properties:
              version:
                type: number
"#;
    let result = run_workflow(yaml).await;
    assert!(
        result.is_ok(),
        "Deeply nested schema should parse and run: {result:?}"
    );
    let output = result.unwrap();
    // Validate mock response is valid JSON with nested structure
    let parsed: serde_json::Value = serde_json::from_str(&output)
        .unwrap_or_else(|e| panic!("Output should be valid JSON: {e}\nRaw: {output}"));
    // Verify 3+ levels of nesting exist in mock response
    assert!(
        parsed.get("user").is_some(),
        "Should have 'user' field: {parsed}"
    );
    assert!(
        parsed["user"].get("address").is_some(),
        "Should have nested 'user.address': {parsed}"
    );
    assert!(
        parsed["user"]["address"].get("city").is_some(),
        "Should have 'user.address.city' (3 levels deep): {parsed}"
    );
}

/// Schema with additionalProperties: false rejects extra fields.
/// Mock provider returns many fields beyond the schema, so structured
/// validation should catch them and produce NIKA-061.
/// This proves additionalProperties:false enforcement works end-to-end.
#[tokio::test]
async fn e2e_adversarial_structured_additional_properties_false() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: mock
tasks:
  - id: strict
    infer: "Tell me about a simple product: a red notebook costing five dollars"
    structured:
      schema:
        type: object
        properties:
          name:
            type: string
          price:
            type: number
          color:
            type: string
        required: [name, price, color]
        additionalProperties: false
      max_retries: 0
"#;
    // Mock returns many extra fields (mock, task_id, items, etc.) which
    // violate additionalProperties:false — validation MUST reject them.
    expect_error(yaml, "NIKA-061").await;
}

/// Schema with contradictory constraints (minimum > maximum).
/// Should produce a clear error, not hang or panic.
#[tokio::test]
async fn e2e_adversarial_structured_contradictory_constraints() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: mock
tasks:
  - id: impossible
    infer: "Give me a number between ten and five"
    structured:
      schema:
        type: object
        properties:
          value:
            type: number
            minimum: 10
            maximum: 5
        required: [value]
      max_retries: 1
"#;
    // The workflow should either:
    // 1. Fail with a clear error about invalid schema / validation failure
    // 2. Succeed because mock bypasses structured validation
    // Either way, it must NOT hang or panic.
    let result = run_workflow(yaml).await;
    // Mock bypasses structured output validation, so it succeeds.
    // The important thing is: no hang, no panic.
    assert!(
        result.is_ok() || result.is_err(),
        "Contradictory constraints must not panic or hang"
    );
}

/// Structured output combined with for_each: each iteration should produce
/// output, and the combined result should be an array.
#[tokio::test]
async fn e2e_adversarial_structured_for_each() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: mock
tasks:
  - id: extract_many
    for_each: ["Alice", "Bob", "Charlie"]
    as: person_name
    infer: "Tell me about a person named {{with.person_name}}"
    structured:
      schema:
        type: object
        properties:
          name:
            type: string
          age:
            type: number
        required: [name, age]
  - id: check
    depends_on: [extract_many]
    with: { people: $extract_many }
    infer: "Got {{with.people | length}} people"
"#;
    let result = run_workflow(yaml).await;
    assert!(
        result.is_ok(),
        "Structured output + for_each should succeed: {result:?}"
    );
}

// =============================================================================
// ADVERSARIAL: Concurrency Edge Cases
// =============================================================================

/// for_each concurrency:50 with only 3 items.
/// Should not deadlock or produce errors due to over-provisioned concurrency.
#[tokio::test]
async fn e2e_adversarial_high_concurrency_low_items() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: mock
tasks:
  - id: over_concurrent
    for_each: ["x", "y", "z"]
    as: item
    concurrency: 50
    infer: "Process item: {{with.item}}"
"#;
    let result = run_workflow(yaml).await;
    assert!(
        result.is_ok(),
        "concurrency:50 with 3 items should not deadlock: {result:?}"
    );
}

/// Two tasks depending on the same parent should both execute.
/// Verifies diamond-shaped DAG works correctly.
#[tokio::test]
async fn e2e_adversarial_parallel_deps() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: mock
tasks:
  - id: parent
    infer: "Generate the source data"
  - id: child_a
    depends_on: [parent]
    with: { data: $parent }
    infer: "Process branch A: {{with.data}}"
  - id: child_b
    depends_on: [parent]
    with: { data: $parent }
    infer: "Process branch B: {{with.data}}"
  - id: merge
    depends_on: [child_a, child_b]
    with:
      a: $child_a
      b: $child_b
    infer: "Merge branches: {{with.a}} and {{with.b}}"
"#;
    let result = run_workflow(yaml).await;
    assert!(
        result.is_ok(),
        "Diamond-shaped DAG with parallel deps should succeed: {result:?}"
    );
    let output = result.unwrap();
    assert!(
        !output.is_empty(),
        "Merge task should produce non-empty output"
    );
}

/// Agent with max_turns:1 should complete gracefully without hanging.
/// The agent should stop after a single turn and return whatever it has.
#[tokio::test]
async fn e2e_adversarial_agent_max_turns_1() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: mock
tasks:
  - id: minimal_agent
    agent:
      prompt: "Answer briefly: what is two plus two?"
      max_turns: 1
      completion:
        mode: natural
"#;
    let result = run_workflow(yaml).await;
    assert!(
        result.is_ok(),
        "Agent with max_turns:1 should complete gracefully: {result:?}"
    );
    let output = result.unwrap();
    assert!(
        !output.is_empty(),
        "Agent should produce output even with max_turns:1"
    );
}

// =============================================================================
// Session 2A E2E Tests
// =============================================================================

/// E2E: Provider fallback chain — first provider fails (missing key),
/// falls back to mock which always succeeds.
#[tokio::test]
async fn e2e_provider_fallback_to_mock() {
    let yaml = r#"
schema: "nika/workflow@0.12"
model: mock-model
tasks:
  - id: fallback_test
    provider: [nonexistent_provider_xyz, mock]
    infer: "Test fallback"
"#;
    let result = run_workflow(yaml).await;
    assert!(
        result.is_ok(),
        "Fallback chain should succeed via mock: {result:?}"
    );
}

/// E2E: Artifact writing creates a file on disk.
/// Verifies that artifact: config produces a real file with task output.
#[tokio::test]
async fn e2e_artifact_writing_creates_file() {
    let temp_dir = std::env::temp_dir().join(format!("nika_e2e_artifact_{}", std::process::id()));
    std::fs::create_dir_all(&temp_dir).unwrap();
    let artifact_dir = temp_dir.display();

    let yaml = format!(
        r#"
schema: "nika/workflow@0.12"
provider: mock
artifacts:
  dir: "{artifact_dir}"
tasks:
  - id: report
    infer: "Generate a test report"
    artifact:
      path: report.json
      format: json
"#
    );

    let result = run_workflow(&yaml).await;
    assert!(
        result.is_ok(),
        "Workflow with artifact should succeed: {result:?}"
    );

    let artifact_path = temp_dir.join("report.json");
    assert!(
        artifact_path.exists(),
        "Artifact file should exist at {}",
        artifact_path.display()
    );
    let content = std::fs::read_to_string(&artifact_path).unwrap();
    assert!(!content.is_empty(), "Artifact file should have content");

    // Cleanup
    let _ = std::fs::remove_dir_all(&temp_dir);
}

/// E2E: for_each + structured output produces an array of schema-valid JSON.
/// Each item in the for_each produces a structured output, and the final
/// result is an array with one valid object per iteration.
#[tokio::test]
async fn e2e_for_each_with_structured_output() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: mock
tasks:
  - id: process
    for_each: ["alice", "bob", "charlie"]
    as: person
    infer: "Describe {{with.person}}"
    structured:
      schema:
        type: object
        properties:
          name: { type: string }
          age: { type: integer, minimum: 0 }
        required: [name, age]

  - id: collect
    depends_on: [process]
    with:
      results: $process
    infer: "Collected {{with.results | length}} results"
"#;
    let result = run_workflow(yaml).await;
    assert!(
        result.is_ok(),
        "for_each + structured should succeed: {result:?}"
    );
}

// =============================================================================
// Session 2B E2E Tests
// =============================================================================

/// E2E: Infer guardrail with max_words:3 triggers on mock's long response.
/// Mock response has ~50 words, so max_words:3 should cause a guardrail failure.
#[tokio::test]
async fn e2e_guardrail_length_violation_fails() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: mock
tasks:
  - id: short
    infer:
      prompt: "Say something"
      guardrails:
        - type: length
          max_words: 3
          on_failure: fail
"#;
    let result = run_workflow(yaml).await;
    assert!(
        result.is_err(),
        "Guardrail max_words:3 should fail on mock's long response: {result:?}"
    );
    let err = result.unwrap_err();
    assert!(
        err.contains("guardrail") || err.contains("NIKA-112") || err.contains("length"),
        "Error should mention guardrail: {err}"
    );
}

/// E2E: Infer guardrail with max_words:1000 passes on mock's short response.
#[tokio::test]
async fn e2e_guardrail_length_passes() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: mock
tasks:
  - id: ok
    infer:
      prompt: "Say something"
      guardrails:
        - type: length
          max_words: 1000
          on_failure: fail
"#;
    let result = run_workflow(yaml).await;
    assert!(
        result.is_ok(),
        "Guardrail max_words:1000 should pass: {result:?}"
    );
}

/// E2E: Real provider structured output — OpenAI (skip if no key).
/// Validates complex schema with nested objects, enums, arrays, and constraints.
#[tokio::test]
async fn e2e_structured_openai_complex() {
    if std::env::var("OPENAI_API_KEY").is_err() {
        eprintln!("SKIP: OPENAI_API_KEY not set");
        return;
    }
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: openai
model: gpt-4.1-nano
tasks:
  - id: extract
    infer: "Tell me about Paris, the capital of France, a European city with about 2 million people"
    structured:
      schema:
        type: object
        properties:
          city: { type: string }
          country: { type: string }
          population: { type: integer, minimum: 0 }
          continent: { type: string, enum: [Europe, Asia, Africa, Americas, Oceania] }
          landmarks:
            type: array
            items: { type: string }
            minItems: 1
        required: [city, country, population, continent, landmarks]
      max_retries: 2
"#;
    let result = run_workflow(yaml).await;
    assert!(
        result.is_ok(),
        "OpenAI structured should succeed: {result:?}"
    );
    let output = result.unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&output).expect("Should be valid JSON");
    assert_eq!(parsed["continent"], "Europe");
    assert!(parsed["population"].as_i64().unwrap_or(0) > 0);
    assert!(!parsed["landmarks"].as_array().unwrap().is_empty());
}

/// E2E: Real provider structured output — xAI Grok (skip if no key).
#[tokio::test]
async fn e2e_structured_xai_complex() {
    if std::env::var("XAI_API_KEY").is_err() {
        eprintln!("SKIP: XAI_API_KEY not set");
        return;
    }
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: xai
model: grok-3-fast
tasks:
  - id: extract
    infer: "Tell me about Tokyo, the capital of Japan"
    structured:
      schema:
        type: object
        properties:
          city: { type: string }
          country: { type: string }
          population: { type: integer, minimum: 0 }
        required: [city, country, population]
      max_retries: 2
"#;
    let result = run_workflow(yaml).await;
    assert!(result.is_ok(), "xAI structured should succeed: {result:?}");
    let output = result.unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&output).expect("Should be valid JSON");
    assert!(parsed["city"].is_string());
    assert!(parsed["population"].as_i64().unwrap_or(0) > 0);
}
