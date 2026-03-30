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
