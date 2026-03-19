//! End-to-end vision workflow tests
//!
//! Tests vision (multimodal) workflow parsing, validation, mock provider execution,
//! InferParams edge cases, and event emission for the vision pipeline.

use super::*;
use crate::ast::content::{ContentPart, ImageDetail};
use crate::ast::{InferParams, TaskAction};
use crate::event::EventKind;
use crate::store::RunContext;

// ═══════════════════════════════════════════════════════════════
// 1. FULL WORKFLOW PARSE + VALIDATE (YAML → Workflow struct)
// ═══════════════════════════════════════════════════════════════

fn parse_ok(yaml: &str) -> crate::ast::Workflow {
    crate::ast::parse_workflow(yaml).expect("Workflow YAML should parse without errors")
}

fn parse_err(yaml: &str) -> crate::error::NikaError {
    crate::ast::parse_workflow(yaml).expect_err("Workflow YAML should fail to parse")
}

#[test]
fn parse_vision_workflow_with_cas_image() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: mock
model: test
tasks:
  - id: describe
    infer:
      content:
        - type: image
          source: "blake3:deadbeef1234567890"
          detail: high
        - type: text
          text: "Describe this image"
"#;
    let wf = parse_ok(yaml);
    assert_eq!(wf.provider, "mock");
    assert_eq!(wf.tasks.len(), 1);
    assert_eq!(wf.tasks[0].id, "describe");
    match &wf.tasks[0].action {
        TaskAction::Infer { infer } => {
            let content = infer.content.as_ref().expect("content should be present");
            assert_eq!(content.len(), 2);
            match &content[0] {
                ContentPart::Image { source, detail } => {
                    assert_eq!(source, "blake3:deadbeef1234567890");
                    assert_eq!(*detail, ImageDetail::High);
                }
                other => panic!("Expected Image, got: {:?}", other),
            }
            match &content[1] {
                ContentPart::Text { text } => assert_eq!(text, "Describe this image"),
                other => panic!("Expected Text, got: {:?}", other),
            }
        }
        other => panic!("Expected Infer action, got: {:?}", other),
    }
}

#[test]
fn parse_mixed_vision_and_text_workflow() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: mock
tasks:
  - id: step1
    infer: "Plain text prompt"
  - id: step2
    infer:
      content:
        - type: text
          text: "Vision task"
    depends_on: [step1]
"#;
    let wf = parse_ok(yaml);
    assert_eq!(wf.tasks.len(), 2);
    match &wf.tasks[0].action {
        TaskAction::Infer { infer } => {
            assert_eq!(infer.prompt, "Plain text prompt");
            assert!(infer.content.is_none());
        }
        other => panic!("Expected Infer, got: {:?}", other),
    }
    match &wf.tasks[1].action {
        TaskAction::Infer { infer } => {
            let content = infer.content.as_ref().unwrap();
            assert_eq!(content.len(), 1);
            assert!(matches!(&content[0], ContentPart::Text { text } if text == "Vision task"));
        }
        other => panic!("Expected Infer, got: {:?}", other),
    }
}

#[test]
fn parse_vision_with_image_url() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: mock
tasks:
  - id: analyze
    infer:
      content:
        - type: image_url
          url: "https://example.com/photo.jpg"
          detail: low
        - type: text
          text: "What is in this image?"
"#;
    let wf = parse_ok(yaml);
    match &wf.tasks[0].action {
        TaskAction::Infer { infer } => {
            let content = infer.content.as_ref().unwrap();
            match &content[0] {
                ContentPart::ImageUrl { url, detail } => {
                    assert_eq!(url, "https://example.com/photo.jpg");
                    assert_eq!(*detail, ImageDetail::Low);
                }
                other => panic!("Expected ImageUrl, got: {:?}", other),
            }
        }
        other => panic!("Expected Infer, got: {:?}", other),
    }
}

#[test]
fn parse_vision_with_template_source() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: mock
tasks:
  - id: import_photo
    exec: "echo done"
  - id: describe
    infer:
      content:
        - type: image
          source: "{{with.photo.media[0].hash}}"
          detail: high
        - type: text
          text: "Describe this image"
    with:
      photo: $import_photo
    depends_on: [import_photo]
"#;
    let wf = parse_ok(yaml);
    assert_eq!(wf.tasks[1].id, "describe");
    match &wf.tasks[1].action {
        TaskAction::Infer { infer } => {
            let content = infer.content.as_ref().unwrap();
            match &content[0] {
                ContentPart::Image { source, .. } => {
                    assert_eq!(source, "{{with.photo.media[0].hash}}");
                }
                other => panic!("Expected Image, got: {:?}", other),
            }
        }
        other => panic!("Expected Infer, got: {:?}", other),
    }
}

#[test]
fn parse_vision_multiple_images() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: mock
tasks:
  - id: compare
    infer:
      content:
        - type: image
          source: "blake3:aaa111"
        - type: image
          source: "blake3:bbb222"
        - type: text
          text: "Compare these two images"
"#;
    let wf = parse_ok(yaml);
    let content = match &wf.tasks[0].action {
        TaskAction::Infer { infer } => infer.content.as_ref().unwrap(),
        other => panic!("Expected Infer, got: {:?}", other),
    };
    assert_eq!(content.len(), 3);
    assert_eq!(content.iter().filter(|p| matches!(p, ContentPart::Image { .. })).count(), 2);
}

#[test]
fn parse_vision_default_detail_is_auto() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: mock
tasks:
  - id: auto_detail
    infer:
      content:
        - type: image
          source: "blake3:abc123"
        - type: text
          text: "Describe"
"#;
    let wf = parse_ok(yaml);
    match &wf.tasks[0].action {
        TaskAction::Infer { infer } => match &infer.content.as_ref().unwrap()[0] {
            ContentPart::Image { detail, .. } => assert_eq!(*detail, ImageDetail::Auto),
            other => panic!("Expected Image, got: {:?}", other),
        },
        other => panic!("Expected Infer, got: {:?}", other),
    }
}

#[test]
fn parse_vision_with_prompt_and_content() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: mock
tasks:
  - id: hybrid
    infer:
      prompt: "Additional context for the model"
      content:
        - type: image
          source: "blake3:abc"
          detail: high
        - type: text
          text: "What is this?"
"#;
    let wf = parse_ok(yaml);
    match &wf.tasks[0].action {
        TaskAction::Infer { infer } => {
            assert_eq!(infer.prompt, "Additional context for the model");
            assert_eq!(infer.content.as_ref().unwrap().len(), 2);
        }
        other => panic!("Expected Infer, got: {:?}", other),
    }
}

#[test]
fn parse_vision_with_provider_and_model_overrides() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: mock
model: default-model
tasks:
  - id: vision_with_override
    provider: openai
    model: gpt-4o
    infer:
      prompt: "describe"
      content:
        - type: image_url
          url: "https://example.com/photo.png"
        - type: text
          text: "What is this?"
"#;
    let wf = parse_ok(yaml);
    match &wf.tasks[0].action {
        TaskAction::Infer { infer } => {
            assert_eq!(infer.provider.as_deref(), Some("openai"));
            assert_eq!(infer.model.as_deref(), Some("gpt-4o"));
            assert!(infer.content.is_some());
        }
        other => panic!("Expected Infer, got: {:?}", other),
    }
}

#[test]
fn parse_vision_image_url_with_auto_detail() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: mock
tasks:
  - id: url_auto
    infer:
      content:
        - type: image_url
          url: "https://example.com/img.webp"
          detail: auto
        - type: text
          text: "Describe"
"#;
    let wf = parse_ok(yaml);
    match &wf.tasks[0].action {
        TaskAction::Infer { infer } => match &infer.content.as_ref().unwrap()[0] {
            ContentPart::ImageUrl { url, detail } => {
                assert_eq!(url, "https://example.com/img.webp");
                assert_eq!(*detail, ImageDetail::Auto);
            }
            other => panic!("Expected ImageUrl, got: {:?}", other),
        },
        other => panic!("Expected Infer, got: {:?}", other),
    }
}

#[test]
fn parse_vision_with_system_prompt() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: mock
tasks:
  - id: with_system
    infer:
      system: "You are a professional art critic."
      content:
        - type: image
          source: "blake3:deadbeef"
        - type: text
          text: "Critique this artwork"
"#;
    let wf = parse_ok(yaml);
    match &wf.tasks[0].action {
        TaskAction::Infer { infer } => {
            assert_eq!(infer.system.as_deref(), Some("You are a professional art critic."));
            assert!(infer.content.is_some());
        }
        other => panic!("Expected Infer, got: {:?}", other),
    }
}

#[test]
fn parse_vision_content_only_no_prompt() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: mock
tasks:
  - id: content_only
    infer:
      content:
        - type: text
          text: "This is the only text"
"#;
    let wf = parse_ok(yaml);
    match &wf.tasks[0].action {
        TaskAction::Infer { infer } => {
            assert!(infer.prompt.is_empty());
            assert!(infer.content.is_some());
        }
        other => panic!("Expected Infer, got: {:?}", other),
    }
}

#[test]
fn parse_vision_empty_content_fails() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: mock
tasks:
  - id: empty_content
    infer:
      content: []
"#;
    parse_err(yaml);
}

#[test]
fn parse_vision_mixed_image_types_in_one_task() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: mock
tasks:
  - id: mixed_images
    infer:
      content:
        - type: image
          source: "blake3:cas_image"
          detail: high
        - type: image_url
          url: "https://example.com/remote.jpg"
          detail: low
        - type: text
          text: "Compare CAS image with remote URL image"
"#;
    let wf = parse_ok(yaml);
    let content = match &wf.tasks[0].action {
        TaskAction::Infer { infer } => infer.content.as_ref().unwrap(),
        other => panic!("Expected Infer, got: {:?}", other),
    };
    assert_eq!(content.len(), 3);
    assert!(matches!(&content[0], ContentPart::Image { .. }));
    assert!(matches!(&content[1], ContentPart::ImageUrl { .. }));
    assert!(matches!(&content[2], ContentPart::Text { .. }));
}

// ═══════════════════════════════════════════════════════════════
// 2. MOCK PROVIDER VISION EXECUTION
// ═══════════════════════════════════════════════════════════════

#[tokio::test]
async fn mock_infer_with_content_text_returns_response() {
    let executor = TaskExecutor::new("mock", None, None, EventLog::new());
    let task_id: Arc<str> = Arc::from("vision-mock-text");
    let bindings = ResolvedBindings::default();
    let datastore = RunContext::default();
    let infer = InferParams {
        prompt: String::new(),
        content: Some(vec![ContentPart::Text { text: "Describe this scene".to_string() }]),
        ..Default::default()
    };
    let result = executor.run_infer(&task_id, &infer, &bindings, &datastore, None).await;
    assert!(result.is_ok(), "Mock infer with content should succeed: {:?}", result.err());
    let parsed: serde_json::Value = serde_json::from_str(&result.unwrap()).unwrap();
    assert_eq!(parsed["mock"], true);
    assert_eq!(parsed["task_id"], "vision-mock-text");
}

#[tokio::test]
async fn mock_infer_with_content_image_returns_response() {
    let executor = TaskExecutor::new("mock", None, None, EventLog::new());
    let task_id: Arc<str> = Arc::from("vision-mock-image");
    let bindings = ResolvedBindings::default();
    let datastore = RunContext::default();
    let infer = InferParams {
        prompt: "Describe this".to_string(),
        content: Some(vec![
            ContentPart::Image { source: "blake3:deadbeef".to_string(), detail: ImageDetail::High },
            ContentPart::Text { text: "What is in this image?".to_string() },
        ]),
        ..Default::default()
    };
    let result = executor.run_infer(&task_id, &infer, &bindings, &datastore, None).await;
    assert!(result.is_ok(), "Mock infer with image content should succeed: {:?}", result.err());
}

#[tokio::test]
async fn mock_infer_vision_emits_template_resolved() {
    let event_log = EventLog::new();
    let executor = TaskExecutor::new("mock", None, None, event_log.clone());
    let task_id: Arc<str> = Arc::from("vision-tpl-event");
    let bindings = ResolvedBindings::default();
    let datastore = RunContext::default();
    let infer = InferParams {
        prompt: "Describe the image".to_string(),
        content: Some(vec![ContentPart::Text { text: "Image description task".to_string() }]),
        ..Default::default()
    };
    executor.run_infer(&task_id, &infer, &bindings, &datastore, None).await.unwrap();
    let events = event_log.filter_task("vision-tpl-event");
    let tpl: Vec<_> = events.iter().filter(|e| matches!(e.kind, EventKind::TemplateResolved { .. })).collect();
    assert_eq!(tpl.len(), 1, "Should emit exactly one TemplateResolved event");
}

#[tokio::test]
async fn mock_infer_vision_emits_provider_responded() {
    let event_log = EventLog::new();
    let executor = TaskExecutor::new("mock", None, None, event_log.clone());
    let task_id: Arc<str> = Arc::from("vision-pr-event");
    let bindings = ResolvedBindings::default();
    let datastore = RunContext::default();
    let infer = InferParams {
        prompt: "Analyze".to_string(),
        content: Some(vec![ContentPart::Image { source: "blake3:abc123".to_string(), detail: ImageDetail::Auto }]),
        ..Default::default()
    };
    executor.run_infer(&task_id, &infer, &bindings, &datastore, None).await.unwrap();
    let events = event_log.filter_task("vision-pr-event");
    let pr: Vec<_> = events.iter().filter(|e| matches!(e.kind, EventKind::ProviderResponded { .. })).collect();
    assert_eq!(pr.len(), 1, "Should emit exactly one ProviderResponded event");
    if let EventKind::ProviderResponded { task_id: ref tid, .. } = &pr[0].kind {
        assert_eq!(tid.as_ref(), "vision-pr-event");
    }
}

#[tokio::test]
async fn mock_infer_vision_with_task_level_provider_override() {
    let executor = TaskExecutor::new("claude", None, None, EventLog::new());
    let task_id: Arc<str> = Arc::from("vision-override");
    let bindings = ResolvedBindings::default();
    let datastore = RunContext::default();
    let infer = InferParams {
        prompt: "Vision with override".to_string(),
        provider: Some("mock".to_string()),
        content: Some(vec![ContentPart::Text { text: "Override test".to_string() }]),
        ..Default::default()
    };
    let result = executor.run_infer(&task_id, &infer, &bindings, &datastore, None).await;
    assert!(result.is_ok(), "Task-level mock override should work: {:?}", result.err());
}

// ═══════════════════════════════════════════════════════════════
// 3. INFER PARAMS EDGE CASES
// ═══════════════════════════════════════════════════════════════

#[tokio::test]
async fn infer_no_content_text_prompt_normal_path() {
    let executor = TaskExecutor::new("mock", None, None, EventLog::new());
    let task_id: Arc<str> = Arc::from("text-only");
    let bindings = ResolvedBindings::default();
    let datastore = RunContext::default();
    let infer = InferParams { prompt: "Generate something".to_string(), content: None, ..Default::default() };
    let result = executor.run_infer(&task_id, &infer, &bindings, &datastore, None).await;
    assert!(result.is_ok(), "Normal text-only path should work");
}

#[tokio::test]
async fn infer_content_text_empty_prompt_vision_path() {
    let executor = TaskExecutor::new("mock", None, None, EventLog::new());
    let task_id: Arc<str> = Arc::from("vision-empty-prompt");
    let bindings = ResolvedBindings::default();
    let datastore = RunContext::default();
    let infer = InferParams {
        prompt: String::new(),
        content: Some(vec![ContentPart::Text { text: "Content-only vision task".to_string() }]),
        ..Default::default()
    };
    let result = executor.run_infer(&task_id, &infer, &bindings, &datastore, None).await;
    assert!(result.is_ok(), "Vision path with empty prompt should succeed: {:?}", result.err());
}

#[tokio::test]
async fn infer_empty_content_empty_prompt_fails() {
    let executor = TaskExecutor::new("mock", None, None, EventLog::new());
    let task_id: Arc<str> = Arc::from("both-empty");
    let bindings = ResolvedBindings::default();
    let datastore = RunContext::default();
    let infer = InferParams { prompt: "   ".to_string(), content: Some(Vec::new()), ..Default::default() };
    let result = executor.run_infer(&task_id, &infer, &bindings, &datastore, None).await;
    assert!(result.is_err(), "Empty prompt + empty content should fail validation");
}

#[tokio::test]
async fn infer_content_image_with_nonempty_prompt() {
    let executor = TaskExecutor::new("mock", None, None, EventLog::new());
    let task_id: Arc<str> = Arc::from("prompt-plus-content");
    let bindings = ResolvedBindings::default();
    let datastore = RunContext::default();
    let infer = InferParams {
        prompt: "Additional context".to_string(),
        content: Some(vec![ContentPart::Image { source: "blake3:abc".to_string(), detail: ImageDetail::High }]),
        ..Default::default()
    };
    let result = executor.run_infer(&task_id, &infer, &bindings, &datastore, None).await;
    assert!(result.is_ok(), "Prompt + content should work: {:?}", result.err());
    let response: serde_json::Value = serde_json::from_str(&result.unwrap()).unwrap();
    assert!(response["prompt_len"].as_u64().unwrap() > 0);
}

#[tokio::test]
async fn infer_content_with_extended_thinking_non_claude_fails() {
    let executor = TaskExecutor::new("mock", None, None, EventLog::new());
    let task_id: Arc<str> = Arc::from("vision-thinking");
    let bindings = ResolvedBindings::default();
    let datastore = RunContext::default();
    let infer = InferParams {
        prompt: "Analyze carefully".to_string(),
        content: Some(vec![ContentPart::Text { text: "test".to_string() }]),
        extended_thinking: Some(true),
        provider: Some("mock".to_string()),
        ..Default::default()
    };
    let result = executor.run_infer(&task_id, &infer, &bindings, &datastore, None).await;
    assert!(result.is_err(), "extended_thinking with mock provider should fail");
    assert!(result.unwrap_err().to_string().contains("claude"));
}

#[tokio::test]
async fn infer_content_with_temperature_and_max_tokens() {
    let executor = TaskExecutor::new("mock", None, None, EventLog::new());
    let task_id: Arc<str> = Arc::from("vision-llm-opts");
    let bindings = ResolvedBindings::default();
    let datastore = RunContext::default();
    let infer = InferParams {
        prompt: "Describe briefly".to_string(),
        content: Some(vec![ContentPart::Image { source: "blake3:img_hash".to_string(), detail: ImageDetail::Low }]),
        temperature: Some(0.7), max_tokens: Some(256),
        ..Default::default()
    };
    let result = executor.run_infer(&task_id, &infer, &bindings, &datastore, None).await;
    assert!(result.is_ok(), "Vision with temperature + max_tokens should work: {:?}", result.err());
}

#[tokio::test]
async fn infer_content_invalid_temperature_fails() {
    let executor = TaskExecutor::new("mock", None, None, EventLog::new());
    let task_id: Arc<str> = Arc::from("bad-temp");
    let bindings = ResolvedBindings::default();
    let datastore = RunContext::default();
    let infer = InferParams {
        prompt: "test".to_string(),
        content: Some(vec![ContentPart::Text { text: "test".to_string() }]),
        temperature: Some(5.0),
        ..Default::default()
    };
    let result = executor.run_infer(&task_id, &infer, &bindings, &datastore, None).await;
    assert!(result.is_err(), "Invalid temperature should fail validation");
}

#[tokio::test]
async fn infer_content_none_prompt_whitespace_fails() {
    let executor = TaskExecutor::new("mock", None, None, EventLog::new());
    let task_id: Arc<str> = Arc::from("whitespace-only");
    let bindings = ResolvedBindings::default();
    let datastore = RunContext::default();
    let infer = InferParams { prompt: "   \t\n  ".to_string(), content: None, ..Default::default() };
    let result = executor.run_infer(&task_id, &infer, &bindings, &datastore, None).await;
    assert!(result.is_err(), "Whitespace-only prompt with no content should fail");
}

// ═══════════════════════════════════════════════════════════════
// 4. EVENT EMISSION VERIFICATION
// ═══════════════════════════════════════════════════════════════

#[tokio::test]
async fn vision_mock_emits_context_assembled_event() {
    let event_log = EventLog::new();
    let executor = TaskExecutor::new("mock", None, None, event_log.clone());
    let task_id: Arc<str> = Arc::from("vision-ctx-event");
    let bindings = ResolvedBindings::default();
    let datastore = RunContext::default();
    let infer = InferParams {
        prompt: "Analyze image".to_string(),
        content: Some(vec![ContentPart::Text { text: "Vision content".to_string() }]),
        ..Default::default()
    };
    executor.run_infer(&task_id, &infer, &bindings, &datastore, None).await.unwrap();
    let events = event_log.filter_task("vision-ctx-event");
    let ctx: Vec<_> = events.iter().filter(|e| matches!(e.kind, EventKind::ContextAssembled { .. })).collect();
    assert_eq!(ctx.len(), 1, "Should emit ContextAssembled event");
}

#[tokio::test]
async fn vision_mock_full_event_sequence() {
    let event_log = EventLog::new();
    let executor = TaskExecutor::new("mock", None, None, event_log.clone());
    let task_id: Arc<str> = Arc::from("vision-full-seq");
    let bindings = ResolvedBindings::default();
    let datastore = RunContext::default();
    let infer = InferParams {
        prompt: "Describe this".to_string(),
        content: Some(vec![
            ContentPart::Image { source: "blake3:abc".to_string(), detail: ImageDetail::High },
            ContentPart::Text { text: "What is it?".to_string() },
        ]),
        ..Default::default()
    };
    executor.run_infer(&task_id, &infer, &bindings, &datastore, None).await.unwrap();
    let events = event_log.filter_task("vision-full-seq");
    assert!(events.iter().any(|e| matches!(e.kind, EventKind::TemplateResolved { .. })), "Should emit TemplateResolved");
    assert!(events.iter().any(|e| matches!(e.kind, EventKind::ContextAssembled { .. })), "Should emit ContextAssembled");
    assert!(events.iter().any(|e| matches!(e.kind, EventKind::ProviderResponded { .. })), "Should emit ProviderResponded");
    assert!(!events.iter().any(|e| matches!(e.kind, EventKind::VisionContentResolved { .. })), "Mock should NOT emit VisionContentResolved");
}

#[tokio::test]
async fn vision_mock_provider_responded_has_tokens() {
    let event_log = EventLog::new();
    let executor = TaskExecutor::new("mock", None, None, event_log.clone());
    let task_id: Arc<str> = Arc::from("vision-tokens");
    let bindings = ResolvedBindings::default();
    let datastore = RunContext::default();
    let infer = InferParams {
        prompt: "Count the objects in this image".to_string(),
        content: Some(vec![ContentPart::Image { source: "blake3:some_hash".to_string(), detail: ImageDetail::Auto }]),
        ..Default::default()
    };
    executor.run_infer(&task_id, &infer, &bindings, &datastore, None).await.unwrap();
    let events = event_log.filter_task("vision-tokens");
    let pr: Vec<_> = events.iter().filter(|e| matches!(e.kind, EventKind::ProviderResponded { .. })).collect();
    assert_eq!(pr.len(), 1);
    if let EventKind::ProviderResponded { input_tokens, output_tokens, finish_reason, .. } = &pr[0].kind {
        assert!(*input_tokens > 0, "Input tokens should be non-zero");
        assert!(*output_tokens > 0, "Output tokens should be non-zero");
        assert_eq!(finish_reason, "mock");
    }
}

// ═══════════════════════════════════════════════════════════════
// 5. EXECUTE DISPATCH WITH VISION CONTENT
// ═══════════════════════════════════════════════════════════════

#[tokio::test]
async fn execute_dispatch_infer_with_content() {
    let executor = TaskExecutor::new("mock", None, None, EventLog::new());
    let task_id: Arc<str> = Arc::from("dispatch-vision");
    let bindings = ResolvedBindings::default();
    let datastore = RunContext::default();
    let action = TaskAction::Infer {
        infer: InferParams {
            prompt: "Describe".to_string(),
            content: Some(vec![
                ContentPart::Image { source: "blake3:dispatch_test".to_string(), detail: ImageDetail::High },
                ContentPart::Text { text: "What is this?".to_string() },
            ]),
            ..Default::default()
        },
    };
    let result = executor.execute(&task_id, &action, &bindings, &datastore, None).await;
    assert!(result.is_ok(), "Execute dispatch should work for vision infer: {:?}", result.err());
    let response: serde_json::Value = serde_json::from_str(&result.unwrap()).unwrap();
    assert_eq!(response["mock"], true);
}

#[tokio::test]
async fn execute_dispatch_vision_with_template_bindings() {
    let executor = TaskExecutor::new("mock", None, None, EventLog::new());
    let task_id: Arc<str> = Arc::from("dispatch-tpl");
    let mut bindings = ResolvedBindings::new();
    bindings.set("question", serde_json::json!("What colors do you see?"));
    let datastore = RunContext::default();
    let action = TaskAction::Infer {
        infer: InferParams {
            prompt: "{{with.question}}".to_string(),
            content: Some(vec![ContentPart::Image { source: "blake3:tpl_test".to_string(), detail: ImageDetail::Auto }]),
            ..Default::default()
        },
    };
    let result = executor.execute(&task_id, &action, &bindings, &datastore, None).await;
    assert!(result.is_ok(), "Vision with template bindings should succeed: {:?}", result.err());
}

// ═══════════════════════════════════════════════════════════════
// 6. VALIDATE() METHOD EDGE CASES FOR CONTENT
// ═══════════════════════════════════════════════════════════════

#[test]
fn validate_infer_prompt_only() {
    let infer = InferParams { prompt: "Hello".to_string(), ..Default::default() };
    assert!(infer.validate().is_ok());
}

#[test]
fn validate_infer_content_only() {
    let infer = InferParams {
        prompt: String::new(),
        content: Some(vec![ContentPart::Text { text: "Content only".to_string() }]),
        ..Default::default()
    };
    assert!(infer.validate().is_ok());
}

#[test]
fn validate_infer_both_prompt_and_content() {
    let infer = InferParams {
        prompt: "Also a prompt".to_string(),
        content: Some(vec![ContentPart::Image { source: "blake3:abc".to_string(), detail: ImageDetail::High }]),
        ..Default::default()
    };
    assert!(infer.validate().is_ok());
}

#[test]
fn validate_infer_neither_prompt_nor_content() {
    let infer = InferParams { prompt: "  ".to_string(), content: None, ..Default::default() };
    assert!(infer.validate().is_err());
}

#[test]
fn validate_infer_empty_content_empty_prompt() {
    let infer = InferParams { prompt: String::new(), content: Some(Vec::new()), ..Default::default() };
    assert!(infer.validate().is_err(), "Empty content vec with empty prompt should fail");
}

#[test]
fn validate_infer_content_with_image_empty_prompt() {
    let infer = InferParams {
        prompt: String::new(),
        content: Some(vec![ContentPart::Image { source: "blake3:test".to_string(), detail: ImageDetail::Auto }]),
        ..Default::default()
    };
    assert!(infer.validate().is_ok(), "Image content with empty prompt should pass");
}
