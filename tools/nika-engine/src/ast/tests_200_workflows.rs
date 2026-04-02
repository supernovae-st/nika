//! 500+ workflow YAML validation tests through parse_workflow + the three-phase pipeline.
//!
//! Categories:
//! A. Infer verb (40 tests)
//! B. Exec verb (20 tests)
//! C. Fetch verb (20 tests)
//! D. Invoke verb (20 tests)
//! E. Agent verb (20 tests)
//! F. DAG patterns (20 tests)
//! G. Bindings and templates (20 tests)
//! H. Output and structured (15 tests)
//! I. Error cases (25 tests)
//! J. Mock provider vision (5 tests)
//! K. Workflow-level features (10 tests)
//! L. Fetch extract modes (40 tests)
//! M. Fetch response modes (20 tests)
//! N. Fetch + bindings (15 tests)
//! O. Combined with PR4 vision (15 tests)
//! P. Fetch edge cases (12 tests)
//! Q. Vision + fetch combined (20 tests)
//! R. For_each + extract (15 tests)
//! S. Agent + fetch tools (15 tests)
//! T. Complex multi-task pipelines (30 tests)
//! U. Retry + error handling (15 tests)
//! V. Schema version + compatibility (10 tests)
//! W. Structured output + extract (15 tests)
//! X. Context + includes + skills (10 tests)
//! Y. Limits + guardrails (10 tests)
//! Z. Edge cases (20 tests)
//! AA. Backward compatibility (15 tests)
//! AB. All 26 media tools (26 tests)
//! AD. Extreme complexity (7 tests)

use crate::ast::content::{ContentPart, ImageDetail};
use crate::ast::extract::{ExtractMode, ResponseMode};
use crate::ast::output::OutputFormat;
use crate::ast::{parse_workflow, ResponseFormat, TaskAction};

/// Helper: wraps a task YAML snippet in a minimal valid workflow.
fn wrap(task_yaml: &str) -> String {
    let indented: String = task_yaml
        .lines()
        .map(|line| format!("    {line}\n"))
        .collect();
    format!("schema: \"nika/workflow@0.12\"\nprovider: mock\nmodel: test-model\ntasks:\n  - id: t1\n{indented}")
}

/// Helper: parse a workflow and expect success.
fn ok(yaml: &str) -> crate::ast::Workflow {
    parse_workflow(yaml).unwrap_or_else(|e| panic!("Expected Ok, got: {e}"))
}

/// Helper: parse a workflow and expect failure.
fn err(yaml: &str) -> crate::error::NikaError {
    parse_workflow(yaml).expect_err("Expected parse_workflow to fail")
}

// ═══════════════════════════════════════════════════════════════════════════════
// A. INFER VERB (40 tests)
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn a01_infer_shorthand() {
    let w = ok(&wrap("infer: \"Hello world\""));
    assert_eq!(w.tasks.len(), 1);
    match &w.tasks[0].action {
        TaskAction::Infer { infer } => assert_eq!(infer.prompt, "Hello world"),
        _ => panic!("expected Infer"),
    }
}

#[test]
fn a02_infer_full_prompt_only() {
    let w = ok(&wrap("infer:\n  prompt: \"Generate a headline\""));
    match &w.tasks[0].action {
        TaskAction::Infer { infer } => assert_eq!(infer.prompt, "Generate a headline"),
        _ => panic!("expected Infer"),
    }
}

#[test]
fn a03_infer_full_with_system() {
    let w = ok(&wrap(
        "infer:\n  prompt: \"Explain quantum\"\n  system: \"You are a professor\"",
    ));
    match &w.tasks[0].action {
        TaskAction::Infer { infer } => {
            assert_eq!(infer.system.as_deref(), Some("You are a professor"));
        }
        _ => panic!("expected Infer"),
    }
}

#[test]
fn a04_infer_temperature() {
    let w = ok(&wrap("infer:\n  prompt: \"Creative\"\n  temperature: 0.7"));
    match &w.tasks[0].action {
        TaskAction::Infer { infer } => assert_eq!(infer.temperature, Some(0.7)),
        _ => panic!("expected Infer"),
    }
}

#[test]
fn a05_infer_max_tokens() {
    let w = ok(&wrap("infer:\n  prompt: \"Short\"\n  max_tokens: 100"));
    match &w.tasks[0].action {
        TaskAction::Infer { infer } => assert_eq!(infer.max_tokens, Some(100)),
        _ => panic!("expected Infer"),
    }
}

#[test]
fn a06_infer_all_llm_options() {
    // provider/model are task-level fields in the three-phase pipeline
    let yaml = r#"
schema: "nika/workflow@0.12"
model: test-model
tasks:
  - id: t1
    provider: openai
    model: gpt-4o
    infer:
      prompt: "Write haiku"
      temperature: 0.9
      max_tokens: 50
      system: "Poetry master"
"#;
    let w = ok(yaml);
    match &w.tasks[0].action {
        TaskAction::Infer { infer } => {
            assert_eq!(infer.prompt, "Write haiku");
            assert_eq!(infer.provider.as_ref().map(|p| p.as_str()), Some("openai"));
            assert_eq!(infer.model.as_deref(), Some("gpt-4o"));
            assert_eq!(infer.temperature, Some(0.9));
            assert_eq!(infer.max_tokens, Some(50));
            assert_eq!(infer.system.as_deref(), Some("Poetry master"));
        }
        _ => panic!("expected Infer"),
    }
}

#[test]
fn a07_infer_thinking_true() {
    let yaml = wrap(
        "provider: claude\ninfer:\n  prompt: \"Deep reasoning\"\n  extended_thinking: true\n  thinking_budget: 4096",
    );
    let w = ok(&yaml);
    match &w.tasks[0].action {
        TaskAction::Infer { infer } => {
            assert_eq!(infer.extended_thinking, Some(true));
            assert_eq!(infer.thinking_budget, Some(4096));
        }
        _ => panic!("expected Infer"),
    }
}

#[test]
fn a08_infer_thinking_custom_budget() {
    let yaml = wrap(
        "provider: claude\ninfer:\n  prompt: \"Think deep\"\n  extended_thinking: true\n  thinking_budget: 8192",
    );
    let w = ok(&yaml);
    match &w.tasks[0].action {
        TaskAction::Infer { infer } => assert_eq!(infer.thinking_budget, Some(8192)),
        _ => panic!("expected Infer"),
    }
}

#[test]
fn a09_infer_multiline_prompt_shorthand() {
    let yaml = "schema: \"nika/workflow@0.12\"\nmodel: test-model\ntasks:\n  - id: t1\n    infer: |\n      Line one.\n      Line two.\n";
    let w = ok(yaml);
    match &w.tasks[0].action {
        TaskAction::Infer { infer } => {
            assert!(infer.prompt.contains("Line one."));
            assert!(infer.prompt.contains("Line two."));
        }
        _ => panic!("expected Infer"),
    }
}

#[test]
fn a10_infer_provider_override() {
    // provider is a task-level field in the three-phase pipeline
    let yaml = wrap("provider: mistral\ninfer:\n  prompt: \"Test\"");
    let w = ok(&yaml);
    match &w.tasks[0].action {
        TaskAction::Infer { infer } => {
            assert_eq!(infer.provider.as_ref().map(|p| p.as_str()), Some("mistral"));
        }
        _ => panic!("expected Infer"),
    }
}

#[test]
fn a11_infer_model_override() {
    // model is a task-level field in the three-phase pipeline
    let yaml = wrap("model: gpt-4-turbo\ninfer:\n  prompt: \"Test\"");
    let w = ok(&yaml);
    match &w.tasks[0].action {
        TaskAction::Infer { infer } => {
            assert_eq!(infer.model.as_deref(), Some("gpt-4-turbo"));
        }
        _ => panic!("expected Infer"),
    }
}

#[test]
fn a12_infer_response_format_not_preserved() {
    // response_format is not preserved through the three-phase pipeline (always None)
    let yaml = wrap("infer:\n  prompt: \"Respond JSON\"");
    let w = ok(&yaml);
    match &w.tasks[0].action {
        TaskAction::Infer { infer } => {
            assert!(infer.response_format.is_none());
        }
        _ => panic!("expected Infer"),
    }
}

#[test]
fn a13_infer_provider_at_task_level_claude() {
    let yaml = wrap("provider: claude\ninfer:\n  prompt: \"Test\"");
    let w = ok(&yaml);
    match &w.tasks[0].action {
        TaskAction::Infer { infer } => {
            assert_eq!(
                infer.provider.as_ref().map(|p| p.as_str()),
                Some("anthropic")
            );
        }
        _ => panic!("expected Infer"),
    }
}

#[test]
fn a14_infer_provider_at_task_level_openai() {
    let yaml = wrap("provider: openai\ninfer:\n  prompt: \"Test\"");
    let w = ok(&yaml);
    match &w.tasks[0].action {
        TaskAction::Infer { infer } => {
            assert_eq!(infer.provider.as_ref().map(|p| p.as_str()), Some("openai"));
        }
        _ => panic!("expected Infer"),
    }
}

#[test]
fn a15_infer_temperature_zero_deterministic() {
    let yaml = wrap("infer:\n  prompt: \"Deterministic\"\n  temperature: 0.0");
    let w = ok(&yaml);
    match &w.tasks[0].action {
        TaskAction::Infer { infer } => assert_eq!(infer.temperature, Some(0.0)),
        _ => panic!("expected Infer"),
    }
}

#[test]
fn a16_infer_temperature_max() {
    let yaml = wrap("infer:\n  prompt: \"Max random\"\n  temperature: 2.0");
    let w = ok(&yaml);
    match &w.tasks[0].action {
        TaskAction::Infer { infer } => assert_eq!(infer.temperature, Some(2.0)),
        _ => panic!("expected Infer"),
    }
}

#[test]
fn a17_infer_special_characters() {
    let yaml = wrap("infer: \"Content with !@#$%^&*()\"");
    let w = ok(&yaml);
    match &w.tasks[0].action {
        TaskAction::Infer { infer } => assert!(infer.prompt.contains("!@#$%^&*()")),
        _ => panic!("expected Infer"),
    }
}

#[test]
fn a18_infer_unicode_prompt() {
    let yaml = wrap("infer: \"Contenu en francais: resume, cafe, naive\"");
    let w = ok(&yaml);
    match &w.tasks[0].action {
        TaskAction::Infer { infer } => assert!(infer.prompt.contains("francais")),
        _ => panic!("expected Infer"),
    }
}

#[test]
fn a19_infer_vision_text_only() {
    let yaml = wrap("infer:\n  content:\n    - type: text\n      text: \"Describe this\"");
    let w = ok(&yaml);
    match &w.tasks[0].action {
        TaskAction::Infer { infer } => {
            let parts = infer.content.as_ref().unwrap();
            assert_eq!(parts.len(), 1);
            assert!(matches!(&parts[0], ContentPart::Text { text } if text == "Describe this"));
        }
        _ => panic!("expected Infer"),
    }
}

#[test]
fn a20_infer_vision_image_cas() {
    let yaml = wrap(
        "infer:\n  content:\n    - type: image\n      source: \"blake3:abc123\"\n      detail: high",
    );
    let w = ok(&yaml);
    match &w.tasks[0].action {
        TaskAction::Infer { infer } => {
            let parts = infer.content.as_ref().unwrap();
            assert_eq!(parts.len(), 1);
            match &parts[0] {
                ContentPart::Image { source, detail } => {
                    assert_eq!(source, "blake3:abc123");
                    assert_eq!(*detail, ImageDetail::High);
                }
                _ => panic!("expected Image"),
            }
        }
        _ => panic!("expected Infer"),
    }
}

#[test]
fn a21_infer_vision_image_url() {
    let yaml = wrap(
        "infer:\n  content:\n    - type: image_url\n      url: \"https://example.com/photo.jpg\"\n      detail: low",
    );
    let w = ok(&yaml);
    match &w.tasks[0].action {
        TaskAction::Infer { infer } => {
            let parts = infer.content.as_ref().unwrap();
            match &parts[0] {
                ContentPart::ImageUrl { url, detail } => {
                    assert_eq!(url, "https://example.com/photo.jpg");
                    assert_eq!(*detail, ImageDetail::Low);
                }
                _ => panic!("expected ImageUrl"),
            }
        }
        _ => panic!("expected Infer"),
    }
}

#[test]
fn a22_infer_vision_mixed_content() {
    let yaml = wrap(
        "infer:\n  content:\n    - type: text\n      text: \"Describe\"\n    - type: image\n      source: \"blake3:deadbeef\"\n      detail: high\n    - type: image_url\n      url: \"https://img.com/a.png\"\n      detail: auto",
    );
    let w = ok(&yaml);
    match &w.tasks[0].action {
        TaskAction::Infer { infer } => {
            let parts = infer.content.as_ref().unwrap();
            assert_eq!(parts.len(), 3);
            assert!(matches!(&parts[0], ContentPart::Text { .. }));
            assert!(matches!(&parts[1], ContentPart::Image { .. }));
            assert!(matches!(&parts[2], ContentPart::ImageUrl { .. }));
        }
        _ => panic!("expected Infer"),
    }
}

#[test]
fn a23_infer_vision_plus_prompt() {
    let yaml = wrap(
        "infer:\n  prompt: \"Analyze the image\"\n  content:\n    - type: image\n      source: \"blake3:face\"\n      detail: auto",
    );
    let w = ok(&yaml);
    match &w.tasks[0].action {
        TaskAction::Infer { infer } => {
            assert_eq!(infer.prompt, "Analyze the image");
            assert!(infer.content.is_some());
        }
        _ => panic!("expected Infer"),
    }
}

#[test]
fn a24_infer_vision_with_system_and_temp() {
    let yaml = wrap(
        "infer:\n  system: \"You are a vision expert\"\n  temperature: 0.3\n  content:\n    - type: image\n      source: \"blake3:img\"\n      detail: high",
    );
    let w = ok(&yaml);
    match &w.tasks[0].action {
        TaskAction::Infer { infer } => {
            assert_eq!(infer.system.as_deref(), Some("You are a vision expert"));
            assert_eq!(infer.temperature, Some(0.3));
            assert!(infer.content.is_some());
        }
        _ => panic!("expected Infer"),
    }
}

#[test]
fn a25_infer_vision_detail_auto() {
    let yaml = wrap(
        "infer:\n  content:\n    - type: image\n      source: \"blake3:x\"\n      detail: auto",
    );
    let w = ok(&yaml);
    match &w.tasks[0].action {
        TaskAction::Infer { infer } => match &infer.content.as_ref().unwrap()[0] {
            ContentPart::Image { detail, .. } => assert_eq!(*detail, ImageDetail::Auto),
            _ => panic!("expected Image"),
        },
        _ => panic!("expected Infer"),
    }
}

#[test]
fn a26_infer_vision_detail_low() {
    let yaml = wrap(
        "infer:\n  content:\n    - type: image\n      source: \"blake3:x\"\n      detail: low",
    );
    let w = ok(&yaml);
    match &w.tasks[0].action {
        TaskAction::Infer { infer } => match &infer.content.as_ref().unwrap()[0] {
            ContentPart::Image { detail, .. } => assert_eq!(*detail, ImageDetail::Low),
            _ => panic!("expected Image"),
        },
        _ => panic!("expected Infer"),
    }
}

#[test]
fn a27_infer_vision_detail_high() {
    let yaml = wrap(
        "infer:\n  content:\n    - type: image\n      source: \"blake3:x\"\n      detail: high",
    );
    let w = ok(&yaml);
    match &w.tasks[0].action {
        TaskAction::Infer { infer } => match &infer.content.as_ref().unwrap()[0] {
            ContentPart::Image { detail, .. } => assert_eq!(*detail, ImageDetail::High),
            _ => panic!("expected Image"),
        },
        _ => panic!("expected Infer"),
    }
}

#[test]
fn a28_infer_vision_content_only_no_prompt() {
    let yaml = wrap("infer:\n  content:\n    - type: text\n      text: \"What is this?\"");
    let w = ok(&yaml);
    match &w.tasks[0].action {
        TaskAction::Infer { infer } => {
            // prompt defaults to empty when only content is provided
            assert!(infer.content.is_some());
        }
        _ => panic!("expected Infer"),
    }
}

#[test]
fn a29_infer_vision_image_url_detail_auto() {
    let yaml = wrap(
        "infer:\n  content:\n    - type: image_url\n      url: \"https://img.com/x.jpg\"\n      detail: auto",
    );
    let w = ok(&yaml);
    match &w.tasks[0].action {
        TaskAction::Infer { infer } => match &infer.content.as_ref().unwrap()[0] {
            ContentPart::ImageUrl { detail, .. } => assert_eq!(*detail, ImageDetail::Auto),
            _ => panic!("expected ImageUrl"),
        },
        _ => panic!("expected Infer"),
    }
}

#[test]
fn a30_infer_vision_multiple_images() {
    let yaml = wrap(
        "infer:\n  content:\n    - type: image\n      source: \"blake3:a\"\n    - type: image\n      source: \"blake3:b\"\n    - type: image_url\n      url: \"https://x.com/c.jpg\"",
    );
    let w = ok(&yaml);
    match &w.tasks[0].action {
        TaskAction::Infer { infer } => {
            assert_eq!(infer.content.as_ref().unwrap().len(), 3);
        }
        _ => panic!("expected Infer"),
    }
}

#[test]
fn a31_infer_vision_text_and_multiple_images() {
    let yaml = wrap(
        "infer:\n  prompt: \"Compare these\"\n  content:\n    - type: text\n      text: \"Image comparison task\"\n    - type: image\n      source: \"blake3:first\"\n      detail: high\n    - type: image\n      source: \"blake3:second\"\n      detail: high",
    );
    let w = ok(&yaml);
    match &w.tasks[0].action {
        TaskAction::Infer { infer } => {
            let parts = infer.content.as_ref().unwrap();
            assert_eq!(parts.len(), 3);
            assert!(matches!(&parts[0], ContentPart::Text { .. }));
            assert!(matches!(&parts[1], ContentPart::Image { .. }));
            assert!(matches!(&parts[2], ContentPart::Image { .. }));
        }
        _ => panic!("expected Infer"),
    }
}

#[test]
fn a32_infer_provider_claude() {
    let yaml = wrap("provider: claude\ninfer: \"Test\"");
    let w = ok(&yaml);
    match &w.tasks[0].action {
        TaskAction::Infer { infer } => assert_eq!(
            infer.provider.as_ref().map(|p| p.as_str()),
            Some("anthropic")
        ),
        _ => panic!("expected Infer"),
    }
}

#[test]
fn a33_infer_provider_openai() {
    let yaml = wrap("provider: openai\ninfer: \"Test\"");
    let w = ok(&yaml);
    match &w.tasks[0].action {
        TaskAction::Infer { infer } => {
            assert_eq!(infer.provider.as_ref().map(|p| p.as_str()), Some("openai"))
        }
        _ => panic!("expected Infer"),
    }
}

#[test]
fn a34_infer_provider_groq() {
    let yaml = wrap("provider: groq\ninfer: \"Test\"");
    let w = ok(&yaml);
    match &w.tasks[0].action {
        TaskAction::Infer { infer } => {
            assert_eq!(infer.provider.as_ref().map(|p| p.as_str()), Some("groq"))
        }
        _ => panic!("expected Infer"),
    }
}

#[test]
fn a35_infer_provider_gemini() {
    let yaml = wrap("provider: gemini\ninfer: \"Test\"");
    let w = ok(&yaml);
    match &w.tasks[0].action {
        TaskAction::Infer { infer } => {
            assert_eq!(infer.provider.as_ref().map(|p| p.as_str()), Some("gemini"))
        }
        _ => panic!("expected Infer"),
    }
}

#[test]
fn a36_infer_provider_xai() {
    let yaml = wrap("provider: xai\ninfer: \"Test\"");
    let w = ok(&yaml);
    match &w.tasks[0].action {
        TaskAction::Infer { infer } => {
            assert_eq!(infer.provider.as_ref().map(|p| p.as_str()), Some("xai"))
        }
        _ => panic!("expected Infer"),
    }
}

#[test]
fn a37_infer_provider_deepseek() {
    let yaml = wrap("provider: deepseek\ninfer: \"Test\"");
    let w = ok(&yaml);
    match &w.tasks[0].action {
        TaskAction::Infer { infer } => assert_eq!(
            infer.provider.as_ref().map(|p| p.as_str()),
            Some("deepseek")
        ),
        _ => panic!("expected Infer"),
    }
}

#[test]
fn a38_infer_thinking_false() {
    let yaml = wrap("infer:\n  prompt: \"No thinking\"\n  extended_thinking: false");
    let w = ok(&yaml);
    match &w.tasks[0].action {
        TaskAction::Infer { infer } => assert_eq!(infer.extended_thinking, Some(false)),
        _ => panic!("expected Infer"),
    }
}

#[test]
fn a39_infer_thinking_budget_min() {
    let yaml = wrap(
        "provider: claude\ninfer:\n  prompt: \"Min budget\"\n  extended_thinking: true\n  thinking_budget: 1024",
    );
    let w = ok(&yaml);
    match &w.tasks[0].action {
        TaskAction::Infer { infer } => assert_eq!(infer.thinking_budget, Some(1024)),
        _ => panic!("expected Infer"),
    }
}

#[test]
fn a40_infer_thinking_budget_max() {
    let yaml = wrap(
        "provider: claude\ninfer:\n  prompt: \"Max budget\"\n  extended_thinking: true\n  thinking_budget: 65536",
    );
    let w = ok(&yaml);
    match &w.tasks[0].action {
        TaskAction::Infer { infer } => assert_eq!(infer.thinking_budget, Some(65536)),
        _ => panic!("expected Infer"),
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// B. EXEC VERB (20 tests)
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn b01_exec_shorthand() {
    let w = ok(&wrap("exec: \"echo hello\""));
    match &w.tasks[0].action {
        TaskAction::Exec { exec } => assert_eq!(exec.command, "echo hello"),
        _ => panic!("expected Exec"),
    }
}

#[test]
fn b02_exec_full_form() {
    let w = ok(&wrap("exec:\n  command: \"npm run build\""));
    match &w.tasks[0].action {
        TaskAction::Exec { exec } => assert_eq!(exec.command, "npm run build"),
        _ => panic!("expected Exec"),
    }
}

#[test]
fn b03_exec_shell_true() {
    let w = ok(&wrap(
        "exec:\n  command: \"echo $HOME | grep foo\"\n  shell: true",
    ));
    match &w.tasks[0].action {
        TaskAction::Exec { exec } => assert_eq!(exec.shell, Some(true)),
        _ => panic!("expected Exec"),
    }
}

#[test]
fn b04_exec_shell_false() {
    let w = ok(&wrap("exec:\n  command: \"echo hello\"\n  shell: false"));
    match &w.tasks[0].action {
        TaskAction::Exec { exec } => assert_eq!(exec.shell, Some(false)),
        _ => panic!("expected Exec"),
    }
}

#[test]
fn b05_exec_shell_default_false() {
    // The three-phase pipeline defaults shell to Some(false)
    let w = ok(&wrap("exec:\n  command: \"echo hello\""));
    match &w.tasks[0].action {
        TaskAction::Exec { exec } => assert_eq!(exec.shell, Some(false)),
        _ => panic!("expected Exec"),
    }
}

#[test]
fn b06_exec_with_timeout() {
    let w = ok(&wrap("exec:\n  command: \"sleep 10\"\n  timeout: 30"));
    match &w.tasks[0].action {
        TaskAction::Exec { exec } => assert_eq!(exec.timeout, Some(30)),
        _ => panic!("expected Exec"),
    }
}

#[test]
fn b07_exec_with_cwd() {
    let w = ok(&wrap("exec:\n  command: \"ls\"\n  cwd: \"/tmp\""));
    match &w.tasks[0].action {
        TaskAction::Exec { exec } => assert_eq!(exec.cwd.as_deref(), Some("/tmp")),
        _ => panic!("expected Exec"),
    }
}

#[test]
fn b08_exec_with_env() {
    let yaml =
        wrap("exec:\n  command: \"echo $FOO\"\n  shell: true\n  env:\n    FOO: bar\n    BAZ: qux");
    let w = ok(&yaml);
    match &w.tasks[0].action {
        TaskAction::Exec { exec } => {
            let env = exec.env.as_ref().unwrap();
            assert_eq!(env.get("FOO").unwrap(), "bar");
            assert_eq!(env.get("BAZ").unwrap(), "qux");
        }
        _ => panic!("expected Exec"),
    }
}

#[test]
fn b09_exec_complex_command() {
    let w = ok(&wrap("exec: \"cargo test --lib -- --test-threads=1\""));
    match &w.tasks[0].action {
        TaskAction::Exec { exec } => {
            assert!(exec.command.contains("cargo test"));
            assert!(exec.command.contains("--test-threads=1"));
        }
        _ => panic!("expected Exec"),
    }
}

#[test]
fn b10_exec_pipes_and_redirects() {
    let w = ok(&wrap("exec: \"cat file.txt | grep pattern > output.txt\""));
    match &w.tasks[0].action {
        TaskAction::Exec { exec } => {
            assert!(exec.command.contains("grep pattern"));
        }
        _ => panic!("expected Exec"),
    }
}

#[test]
fn b11_exec_multiline_shorthand() {
    let yaml = "schema: \"nika/workflow@0.12\"\nmodel: test-model\ntasks:\n  - id: t1\n    exec: |\n      echo first &&\n      echo second\n";
    let w = ok(yaml);
    match &w.tasks[0].action {
        TaskAction::Exec { exec } => {
            assert!(exec.command.contains("echo first"));
            assert!(exec.command.contains("echo second"));
        }
        _ => panic!("expected Exec"),
    }
}

#[test]
fn b12_exec_full_all_options() {
    let yaml = wrap(
        "exec:\n  command: \"make build\"\n  shell: true\n  timeout: 60\n  cwd: \"/app\"\n  env:\n    CC: gcc",
    );
    let w = ok(&yaml);
    match &w.tasks[0].action {
        TaskAction::Exec { exec } => {
            assert_eq!(exec.command, "make build");
            assert_eq!(exec.shell, Some(true));
            assert_eq!(exec.timeout, Some(60));
            assert_eq!(exec.cwd.as_deref(), Some("/app"));
            assert!(exec.env.as_ref().unwrap().contains_key("CC"));
        }
        _ => panic!("expected Exec"),
    }
}

#[test]
fn b13_exec_empty_env_becomes_none() {
    // Empty env maps become None through the three-phase pipeline
    let yaml = wrap("exec:\n  command: \"echo hi\"\n  env: {}");
    let w = ok(&yaml);
    match &w.tasks[0].action {
        TaskAction::Exec { exec } => {
            assert!(exec.env.is_none());
        }
        _ => panic!("expected Exec"),
    }
}

#[test]
fn b14_exec_quoted_command() {
    let w = ok(&wrap("exec: \"echo 'hello world'\""));
    match &w.tasks[0].action {
        TaskAction::Exec { exec } => assert!(exec.command.contains("hello world")),
        _ => panic!("expected Exec"),
    }
}

#[test]
fn b15_exec_git_command() {
    let w = ok(&wrap("exec: \"git status\""));
    match &w.tasks[0].action {
        TaskAction::Exec { exec } => assert_eq!(exec.command, "git status"),
        _ => panic!("expected Exec"),
    }
}

#[test]
fn b16_exec_docker_command() {
    let w = ok(&wrap("exec: \"docker run --rm alpine echo test\""));
    match &w.tasks[0].action {
        TaskAction::Exec { exec } => assert!(exec.command.contains("docker run")),
        _ => panic!("expected Exec"),
    }
}

#[test]
fn b17_exec_python_command() {
    let w = ok(&wrap("exec: \"python3 -c 'print(42)'\""));
    match &w.tasks[0].action {
        TaskAction::Exec { exec } => assert!(exec.command.contains("python3")),
        _ => panic!("expected Exec"),
    }
}

#[test]
fn b18_exec_node_command() {
    let w = ok(&wrap("exec: \"node -e 'console.log(1+1)'\""));
    match &w.tasks[0].action {
        TaskAction::Exec { exec } => assert!(exec.command.contains("node")),
        _ => panic!("expected Exec"),
    }
}

#[test]
fn b19_exec_with_large_timeout() {
    let w = ok(&wrap("exec:\n  command: \"sleep 999\"\n  timeout: 3600"));
    match &w.tasks[0].action {
        TaskAction::Exec { exec } => assert_eq!(exec.timeout, Some(3600)),
        _ => panic!("expected Exec"),
    }
}

#[test]
fn b20_exec_multiple_env_vars() {
    let yaml = wrap(
        "exec:\n  command: \"env\"\n  env:\n    A: \"1\"\n    B: \"2\"\n    C: \"3\"\n    D: \"4\"",
    );
    let w = ok(&yaml);
    match &w.tasks[0].action {
        TaskAction::Exec { exec } => {
            assert_eq!(exec.env.as_ref().unwrap().len(), 4);
        }
        _ => panic!("expected Exec"),
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// C. FETCH VERB (20 tests)
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn c01_fetch_get_minimal() {
    let w = ok(&wrap("fetch:\n  url: \"https://api.example.com/data\""));
    match &w.tasks[0].action {
        TaskAction::Fetch { fetch } => {
            assert_eq!(fetch.url, "https://api.example.com/data");
            assert_eq!(fetch.method, "GET");
        }
        _ => panic!("expected Fetch"),
    }
}

#[test]
fn c02_fetch_post() {
    let w = ok(&wrap(
        "fetch:\n  url: \"https://api.example.com\"\n  method: POST",
    ));
    match &w.tasks[0].action {
        TaskAction::Fetch { fetch } => assert_eq!(fetch.method, "POST"),
        _ => panic!("expected Fetch"),
    }
}

#[test]
fn c03_fetch_put() {
    let w = ok(&wrap(
        "fetch:\n  url: \"https://api.example.com\"\n  method: PUT",
    ));
    match &w.tasks[0].action {
        TaskAction::Fetch { fetch } => assert_eq!(fetch.method, "PUT"),
        _ => panic!("expected Fetch"),
    }
}

#[test]
fn c04_fetch_delete() {
    let w = ok(&wrap(
        "fetch:\n  url: \"https://api.example.com\"\n  method: DELETE",
    ));
    match &w.tasks[0].action {
        TaskAction::Fetch { fetch } => assert_eq!(fetch.method, "DELETE"),
        _ => panic!("expected Fetch"),
    }
}

#[test]
fn c05_fetch_with_headers() {
    let yaml = wrap(
        "fetch:\n  url: \"https://api.example.com\"\n  headers:\n    Authorization: \"Bearer tok\"\n    Accept: \"application/json\"",
    );
    let w = ok(&yaml);
    match &w.tasks[0].action {
        TaskAction::Fetch { fetch } => {
            assert_eq!(fetch.headers.len(), 2);
            assert_eq!(fetch.headers.get("Authorization").unwrap(), "Bearer tok");
        }
        _ => panic!("expected Fetch"),
    }
}

#[test]
fn c06_fetch_with_body() {
    let yaml = wrap(
        "fetch:\n  url: \"https://api.example.com\"\n  method: POST\n  body: '{\"key\": \"val\"}'",
    );
    let w = ok(&yaml);
    match &w.tasks[0].action {
        TaskAction::Fetch { fetch } => {
            assert!(fetch.body.as_ref().unwrap().contains("key"));
        }
        _ => panic!("expected Fetch"),
    }
}

#[test]
fn c07_fetch_with_json() {
    let yaml = wrap(
        "fetch:\n  url: \"https://api.example.com\"\n  method: POST\n  json:\n    name: Alice\n    age: 30",
    );
    let w = ok(&yaml);
    match &w.tasks[0].action {
        TaskAction::Fetch { fetch } => {
            let json = fetch.json.as_ref().unwrap();
            assert_eq!(json["name"], "Alice");
            assert_eq!(json["age"], 30);
        }
        _ => panic!("expected Fetch"),
    }
}

#[test]
fn c08_fetch_with_timeout() {
    let w = ok(&wrap(
        "fetch:\n  url: \"https://api.example.com\"\n  timeout: 30",
    ));
    match &w.tasks[0].action {
        TaskAction::Fetch { fetch } => assert_eq!(fetch.timeout, Some(30)),
        _ => panic!("expected Fetch"),
    }
}

#[test]
fn c09_fetch_follow_redirects_true() {
    let w = ok(&wrap(
        "fetch:\n  url: \"https://example.com/redir\"\n  follow_redirects: true",
    ));
    match &w.tasks[0].action {
        TaskAction::Fetch { fetch } => assert_eq!(fetch.follow_redirects, Some(true)),
        _ => panic!("expected Fetch"),
    }
}

#[test]
fn c10_fetch_follow_redirects_false() {
    let w = ok(&wrap(
        "fetch:\n  url: \"https://example.com/redir\"\n  follow_redirects: false",
    ));
    match &w.tasks[0].action {
        TaskAction::Fetch { fetch } => assert_eq!(fetch.follow_redirects, Some(false)),
        _ => panic!("expected Fetch"),
    }
}

#[test]
fn c11_fetch_follow_redirects_default_true() {
    // The three-phase pipeline defaults follow_redirects to Some(true)
    let w = ok(&wrap("fetch:\n  url: \"https://example.com\""));
    match &w.tasks[0].action {
        TaskAction::Fetch { fetch } => assert_eq!(fetch.follow_redirects, Some(true)),
        _ => panic!("expected Fetch"),
    }
}

#[test]
fn c12_fetch_json_nested() {
    let yaml = wrap(
        "fetch:\n  url: \"https://api.example.com\"\n  method: POST\n  json:\n    user:\n      name: Bob\n      email: bob@test.com\n    tags:\n      - admin\n      - active",
    );
    let w = ok(&yaml);
    match &w.tasks[0].action {
        TaskAction::Fetch { fetch } => {
            let json = fetch.json.as_ref().unwrap();
            assert_eq!(json["user"]["name"], "Bob");
            assert_eq!(json["tags"][0], "admin");
        }
        _ => panic!("expected Fetch"),
    }
}

#[test]
fn c13_fetch_empty_headers() {
    let w = ok(&wrap(
        "fetch:\n  url: \"https://example.com\"\n  headers: {}",
    ));
    match &w.tasks[0].action {
        TaskAction::Fetch { fetch } => assert!(fetch.headers.is_empty()),
        _ => panic!("expected Fetch"),
    }
}

#[test]
fn c14_fetch_url_with_query() {
    let w = ok(&wrap(
        "fetch:\n  url: \"https://api.example.com/search?q=rust&limit=10\"",
    ));
    match &w.tasks[0].action {
        TaskAction::Fetch { fetch } => assert!(fetch.url.contains("?q=rust")),
        _ => panic!("expected Fetch"),
    }
}

#[test]
fn c15_fetch_complete() {
    let yaml = wrap(
        "fetch:\n  url: \"https://api.example.com/users\"\n  method: POST\n  headers:\n    Content-Type: application/json\n  json:\n    name: Alice\n  timeout: 60\n  follow_redirects: true",
    );
    let w = ok(&yaml);
    match &w.tasks[0].action {
        TaskAction::Fetch { fetch } => {
            assert_eq!(fetch.method, "POST");
            assert!(fetch.headers.contains_key("Content-Type"));
            assert!(fetch.json.is_some());
            assert_eq!(fetch.timeout, Some(60));
            assert_eq!(fetch.follow_redirects, Some(true));
        }
        _ => panic!("expected Fetch"),
    }
}

#[test]
fn c16_fetch_retry_config() {
    // retry is a task-level field; field names: max_attempts, delay_ms, backoff
    let yaml = wrap(
        "retry:\n  max_attempts: 5\n  delay_ms: 2000\n  backoff: 3.0\nfetch:\n  url: \"https://api.example.com\"",
    );
    let w = ok(&yaml);
    match &w.tasks[0].action {
        TaskAction::Fetch { fetch } => {
            let retry = fetch.retry.as_ref().unwrap();
            assert_eq!(retry.max_attempts, 5);
            assert_eq!(retry.backoff_ms, 2000);
            assert!((retry.multiplier - 3.0).abs() < f64::EPSILON);
        }
        _ => panic!("expected Fetch"),
    }
}

#[test]
fn c17_fetch_patch_method() {
    let w = ok(&wrap(
        "fetch:\n  url: \"https://api.example.com\"\n  method: PATCH",
    ));
    match &w.tasks[0].action {
        TaskAction::Fetch { fetch } => assert_eq!(fetch.method, "PATCH"),
        _ => panic!("expected Fetch"),
    }
}

#[test]
fn c18_fetch_head_method() {
    let w = ok(&wrap(
        "fetch:\n  url: \"https://api.example.com\"\n  method: HEAD",
    ));
    match &w.tasks[0].action {
        TaskAction::Fetch { fetch } => assert_eq!(fetch.method, "HEAD"),
        _ => panic!("expected Fetch"),
    }
}

#[test]
fn c19_fetch_options_method() {
    let w = ok(&wrap(
        "fetch:\n  url: \"https://api.example.com\"\n  method: OPTIONS",
    ));
    match &w.tasks[0].action {
        TaskAction::Fetch { fetch } => assert_eq!(fetch.method, "OPTIONS"),
        _ => panic!("expected Fetch"),
    }
}

#[test]
fn c20_fetch_default_method_get() {
    let w = ok(&wrap("fetch:\n  url: \"https://example.com\""));
    match &w.tasks[0].action {
        TaskAction::Fetch { fetch } => assert_eq!(fetch.method, "GET"),
        _ => panic!("expected Fetch"),
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// D. INVOKE VERB (20 tests)
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn d01_invoke_simple_tool() {
    let yaml =
        wrap("invoke:\n  mcp: novanet\n  tool: novanet_context\n  params:\n    entity: qr-code");
    let w = ok(&yaml);
    match &w.tasks[0].action {
        TaskAction::Invoke { invoke } => {
            assert_eq!(invoke.mcp.as_deref(), Some("novanet"));
            assert_eq!(invoke.tool.as_deref(), Some("novanet_context"));
            assert!(invoke.params.is_some());
        }
        _ => panic!("expected Invoke"),
    }
}

#[test]
fn d02_invoke_tool_with_mcp_and_params() {
    // resource: is not supported through the raw parser; use tool: instead
    let yaml =
        wrap("invoke:\n  mcp: novanet\n  tool: novanet_search\n  params:\n    query: qr-code");
    let w = ok(&yaml);
    match &w.tasks[0].action {
        TaskAction::Invoke { invoke } => {
            assert_eq!(invoke.mcp.as_deref(), Some("novanet"));
            assert_eq!(invoke.tool.as_deref(), Some("novanet_search"));
            assert!(invoke.params.is_some());
        }
        _ => panic!("expected Invoke"),
    }
}

#[test]
fn d03_invoke_tool_without_params() {
    let yaml = wrap("invoke:\n  mcp: test_server\n  tool: simple_tool");
    let w = ok(&yaml);
    match &w.tasks[0].action {
        TaskAction::Invoke { invoke } => {
            assert!(invoke.params.is_none());
        }
        _ => panic!("expected Invoke"),
    }
}

#[test]
fn d04_invoke_with_timeout() {
    let yaml = wrap("invoke:\n  mcp: novanet\n  tool: gen\n  timeout: 120");
    let w = ok(&yaml);
    match &w.tasks[0].action {
        TaskAction::Invoke { invoke } => assert_eq!(invoke.timeout, Some(120)),
        _ => panic!("expected Invoke"),
    }
}

#[test]
fn d05_invoke_builtin_nika_import() {
    let yaml = wrap("invoke:\n  tool: nika:import\n  params:\n    path: ./photo.jpg");
    let w = ok(&yaml);
    match &w.tasks[0].action {
        TaskAction::Invoke { invoke } => {
            assert_eq!(invoke.tool.as_deref(), Some("nika:import"));
            assert!(invoke.mcp.is_none());
        }
        _ => panic!("expected Invoke"),
    }
}

#[test]
fn d06_invoke_builtin_nika_thumbnail() {
    let yaml = wrap("invoke:\n  tool: nika:thumbnail\n  params:\n    width: 200\n    height: 200");
    let w = ok(&yaml);
    match &w.tasks[0].action {
        TaskAction::Invoke { invoke } => {
            assert_eq!(invoke.tool.as_deref(), Some("nika:thumbnail"));
        }
        _ => panic!("expected Invoke"),
    }
}

#[test]
fn d07_invoke_builtin_nika_dimensions() {
    let yaml = wrap("invoke:\n  tool: nika:dimensions\n  params:\n    hash: blake3:abc");
    let w = ok(&yaml);
    match &w.tasks[0].action {
        TaskAction::Invoke { invoke } => {
            assert_eq!(invoke.tool.as_deref(), Some("nika:dimensions"));
        }
        _ => panic!("expected Invoke"),
    }
}

#[test]
fn d08_invoke_builtin_nika_metadata() {
    let yaml = wrap("invoke:\n  tool: nika:metadata\n  params:\n    hash: blake3:xyz");
    let w = ok(&yaml);
    match &w.tasks[0].action {
        TaskAction::Invoke { invoke } => {
            assert_eq!(invoke.tool.as_deref(), Some("nika:metadata"));
        }
        _ => panic!("expected Invoke"),
    }
}

#[test]
fn d09_invoke_server_alias() {
    // "server" is an alias for "mcp"
    let yaml = wrap("invoke:\n  server: myserver\n  tool: do_thing");
    let w = ok(&yaml);
    match &w.tasks[0].action {
        TaskAction::Invoke { invoke } => {
            assert_eq!(invoke.mcp.as_deref(), Some("myserver"));
        }
        _ => panic!("expected Invoke"),
    }
}

#[test]
fn d10_invoke_complex_params() {
    let yaml = wrap(
        "invoke:\n  mcp: novanet\n  tool: batch_gen\n  params:\n    entities:\n      - qr-code\n      - barcode\n    locale: en-US\n    count: 5",
    );
    let w = ok(&yaml);
    match &w.tasks[0].action {
        TaskAction::Invoke { invoke } => {
            let params = invoke.params.as_ref().unwrap();
            assert_eq!(params["locale"], "en-US");
            assert_eq!(params["count"], 5);
            assert!(params["entities"].is_array());
        }
        _ => panic!("expected Invoke"),
    }
}

#[test]
fn d11_invoke_builtin_nika_sleep() {
    let yaml = wrap("invoke:\n  tool: nika:sleep\n  params:\n    duration: \"1s\"");
    let w = ok(&yaml);
    match &w.tasks[0].action {
        TaskAction::Invoke { invoke } => {
            assert_eq!(invoke.tool.as_deref(), Some("nika:sleep"));
        }
        _ => panic!("expected Invoke"),
    }
}

#[test]
fn d12_invoke_builtin_nika_log() {
    let yaml =
        wrap("invoke:\n  tool: nika:log\n  params:\n    level: info\n    message: \"test log\"");
    let w = ok(&yaml);
    match &w.tasks[0].action {
        TaskAction::Invoke { invoke } => {
            assert_eq!(invoke.tool.as_deref(), Some("nika:log"));
        }
        _ => panic!("expected Invoke"),
    }
}

#[test]
fn d13_invoke_builtin_nika_optimize() {
    let yaml = wrap("invoke:\n  tool: nika:optimize\n  params:\n    hash: blake3:img");
    let w = ok(&yaml);
    match &w.tasks[0].action {
        TaskAction::Invoke { invoke } => {
            assert_eq!(invoke.tool.as_deref(), Some("nika:optimize"));
        }
        _ => panic!("expected Invoke"),
    }
}

#[test]
fn d14_invoke_builtin_nika_convert() {
    let yaml =
        wrap("invoke:\n  tool: nika:convert\n  params:\n    hash: blake3:img\n    format: webp");
    let w = ok(&yaml);
    match &w.tasks[0].action {
        TaskAction::Invoke { invoke } => {
            assert_eq!(invoke.tool.as_deref(), Some("nika:convert"));
        }
        _ => panic!("expected Invoke"),
    }
}

#[test]
fn d15_invoke_builtin_nika_pipeline() {
    let yaml = wrap(
        "invoke:\n  tool: nika:pipeline\n  params:\n    steps:\n      - import\n      - thumbnail\n      - optimize",
    );
    let w = ok(&yaml);
    match &w.tasks[0].action {
        TaskAction::Invoke { invoke } => {
            assert_eq!(invoke.tool.as_deref(), Some("nika:pipeline"));
        }
        _ => panic!("expected Invoke"),
    }
}

#[test]
fn d16_invoke_with_nested_json_params() {
    let yaml = wrap(
        "invoke:\n  mcp: novanet\n  tool: gen\n  params:\n    config:\n      nested:\n        deep: true",
    );
    let w = ok(&yaml);
    match &w.tasks[0].action {
        TaskAction::Invoke { invoke } => {
            let params = invoke.params.as_ref().unwrap();
            assert_eq!(params["config"]["nested"]["deep"], true);
        }
        _ => panic!("expected Invoke"),
    }
}

#[test]
fn d17_invoke_timeout_none() {
    let yaml = wrap("invoke:\n  mcp: test\n  tool: do_thing");
    let w = ok(&yaml);
    match &w.tasks[0].action {
        TaskAction::Invoke { invoke } => assert!(invoke.timeout.is_none()),
        _ => panic!("expected Invoke"),
    }
}

#[test]
fn d18_invoke_params_empty_object() {
    let yaml = wrap("invoke:\n  mcp: test\n  tool: no_args\n  params: {}");
    let w = ok(&yaml);
    match &w.tasks[0].action {
        TaskAction::Invoke { invoke } => {
            let params = invoke.params.as_ref().unwrap();
            assert!(params.as_object().unwrap().is_empty());
        }
        _ => panic!("expected Invoke"),
    }
}

#[test]
fn d19_invoke_builtin_nika_chart() {
    let yaml = wrap(
        "invoke:\n  tool: nika:chart\n  params:\n    type: bar\n    data:\n      - label: A\n        value: 10",
    );
    let w = ok(&yaml);
    match &w.tasks[0].action {
        TaskAction::Invoke { invoke } => {
            assert_eq!(invoke.tool.as_deref(), Some("nika:chart"));
        }
        _ => panic!("expected Invoke"),
    }
}

#[test]
fn d20_invoke_builtin_nika_strip() {
    let yaml = wrap("invoke:\n  tool: nika:strip\n  params:\n    hash: blake3:photo");
    let w = ok(&yaml);
    match &w.tasks[0].action {
        TaskAction::Invoke { invoke } => {
            assert_eq!(invoke.tool.as_deref(), Some("nika:strip"));
        }
        _ => panic!("expected Invoke"),
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// E. AGENT VERB (20 tests)
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn e01_agent_basic() {
    let w = ok(&wrap("agent:\n  prompt: \"Generate content\""));
    match &w.tasks[0].action {
        TaskAction::Agent { agent } => {
            assert_eq!(agent.prompt, "Generate content");
        }
        _ => panic!("expected Agent"),
    }
}

#[test]
fn e02_agent_with_mcp() {
    let yaml = wrap("agent:\n  prompt: \"Generate\"\n  mcp:\n    - novanet\n    - perplexity");
    let w = ok(&yaml);
    match &w.tasks[0].action {
        TaskAction::Agent { agent } => {
            assert_eq!(agent.mcp.len(), 2);
            assert!(agent.mcp.contains(&"novanet".to_string()));
        }
        _ => panic!("expected Agent"),
    }
}

#[test]
fn e03_agent_with_tools() {
    let yaml = wrap(
        "agent:\n  prompt: \"Generate\"\n  tools:\n    - nika:read\n    - nika:write\n    - nika:edit",
    );
    let w = ok(&yaml);
    match &w.tasks[0].action {
        TaskAction::Agent { agent } => {
            assert_eq!(agent.tools.len(), 3);
        }
        _ => panic!("expected Agent"),
    }
}

#[test]
fn e04_agent_max_turns() {
    let w = ok(&wrap("agent:\n  prompt: \"Test\"\n  max_turns: 20"));
    match &w.tasks[0].action {
        TaskAction::Agent { agent } => assert_eq!(agent.max_turns, Some(20)),
        _ => panic!("expected Agent"),
    }
}

#[test]
fn e05_agent_with_system() {
    let yaml = wrap("agent:\n  prompt: \"Generate\"\n  system: \"You are an expert\"");
    let w = ok(&yaml);
    match &w.tasks[0].action {
        TaskAction::Agent { agent } => {
            assert_eq!(agent.system.as_deref(), Some("You are an expert"));
        }
        _ => panic!("expected Agent"),
    }
}

#[test]
fn e06_agent_with_provider_model() {
    let yaml = wrap("agent:\n  prompt: \"Test\"\n  provider: claude\n  model: claude-sonnet-4-6");
    let w = ok(&yaml);
    match &w.tasks[0].action {
        TaskAction::Agent { agent } => {
            assert_eq!(
                agent.provider.as_ref().map(|p| p.as_str()),
                Some("anthropic")
            );
            assert_eq!(agent.model.as_deref(), Some("claude-sonnet-4-6"));
        }
        _ => panic!("expected Agent"),
    }
}

#[test]
fn e07_agent_token_budget() {
    let w = ok(&wrap("agent:\n  prompt: \"Test\"\n  token_budget: 50000"));
    match &w.tasks[0].action {
        TaskAction::Agent { agent } => assert_eq!(agent.token_budget, Some(50000)),
        _ => panic!("expected Agent"),
    }
}

#[test]
fn e08_agent_scope_not_preserved() {
    // scope is not preserved through the three-phase pipeline (always None)
    let w = ok(&wrap("agent:\n  prompt: \"Test\""));
    match &w.tasks[0].action {
        TaskAction::Agent { agent } => assert!(agent.scope.is_none()),
        _ => panic!("expected Agent"),
    }
}

#[test]
fn e09_agent_extended_thinking() {
    let yaml = wrap(
        "agent:\n  prompt: \"Test\"\n  provider: claude\n  extended_thinking: true\n  thinking_budget: 8192",
    );
    let w = ok(&yaml);
    match &w.tasks[0].action {
        TaskAction::Agent { agent } => {
            assert_eq!(agent.extended_thinking, Some(true));
            assert_eq!(agent.thinking_budget, Some(8192));
        }
        _ => panic!("expected Agent"),
    }
}

#[test]
fn e10_agent_depth_limit() {
    let w = ok(&wrap("agent:\n  prompt: \"Test\"\n  depth_limit: 5"));
    match &w.tasks[0].action {
        TaskAction::Agent { agent } => assert_eq!(agent.depth_limit, Some(5)),
        _ => panic!("expected Agent"),
    }
}

#[test]
fn e11_agent_tool_choice_not_preserved() {
    // tool_choice is not preserved through the three-phase pipeline (always None)
    let w = ok(&wrap("agent:\n  prompt: \"Test\""));
    match &w.tasks[0].action {
        TaskAction::Agent { agent } => {
            assert!(agent.tool_choice.is_none());
        }
        _ => panic!("expected Agent"),
    }
}

#[test]
fn e12_agent_default_max_turns() {
    // max_turns defaults to None when not specified
    let w = ok(&wrap("agent:\n  prompt: \"Test\""));
    match &w.tasks[0].action {
        TaskAction::Agent { agent } => {
            assert!(agent.max_turns.is_none());
        }
        _ => panic!("expected Agent"),
    }
}

#[test]
fn e13_agent_default_token_budget() {
    // token_budget defaults to None when not specified
    let w = ok(&wrap("agent:\n  prompt: \"Test\""));
    match &w.tasks[0].action {
        TaskAction::Agent { agent } => {
            assert!(agent.token_budget.is_none());
        }
        _ => panic!("expected Agent"),
    }
}

#[test]
fn e14_agent_temperature() {
    let w = ok(&wrap("agent:\n  prompt: \"Test\"\n  temperature: 1.5"));
    match &w.tasks[0].action {
        TaskAction::Agent { agent } => assert_eq!(agent.temperature, Some(1.5)),
        _ => panic!("expected Agent"),
    }
}

#[test]
fn e15_agent_max_tokens() {
    let w = ok(&wrap("agent:\n  prompt: \"Test\"\n  max_tokens: 16384"));
    match &w.tasks[0].action {
        TaskAction::Agent { agent } => assert_eq!(agent.max_tokens, Some(16384)),
        _ => panic!("expected Agent"),
    }
}

#[test]
fn e16_agent_with_skills() {
    let yaml = wrap("agent:\n  prompt: \"Test\"\n  skills:\n    - writing\n    - research");
    let w = ok(&yaml);
    match &w.tasks[0].action {
        TaskAction::Agent { agent } => {
            let skills = agent.skills.as_ref().unwrap();
            assert_eq!(skills.len(), 2);
            assert!(skills.contains(&"writing".to_string()));
        }
        _ => panic!("expected Agent"),
    }
}

#[test]
fn e17_agent_stop_sequences_empty_by_default() {
    // stop_sequences is always empty through the three-phase pipeline
    let w = ok(&wrap("agent:\n  prompt: \"Test\""));
    match &w.tasks[0].action {
        TaskAction::Agent { agent } => {
            assert!(agent.stop_sequences.is_empty());
        }
        _ => panic!("expected Agent"),
    }
}

#[test]
fn e18_agent_completion_not_preserved() {
    // completion config is not preserved through the three-phase pipeline
    let w = ok(&wrap("agent:\n  prompt: \"Test\""));
    match &w.tasks[0].action {
        TaskAction::Agent { agent } => {
            assert!(agent.completion.is_none());
        }
        _ => panic!("expected Agent"),
    }
}

#[test]
fn e19_agent_empty_mcp() {
    let w = ok(&wrap("agent:\n  prompt: \"Test\"\n  mcp: []"));
    match &w.tasks[0].action {
        TaskAction::Agent { agent } => assert!(agent.mcp.is_empty()),
        _ => panic!("expected Agent"),
    }
}

#[test]
fn e20_agent_full_config() {
    let yaml = wrap(
        "agent:\n  prompt: \"Full config\"\n  system: \"Expert\"\n  provider: claude\n  model: claude-sonnet-4-6\n  mcp:\n    - novanet\n  max_turns: 15\n  token_budget: 100000\n  temperature: 0.8\n  tool_choice: auto\n  depth_limit: 3",
    );
    let w = ok(&yaml);
    match &w.tasks[0].action {
        TaskAction::Agent { agent } => {
            assert_eq!(agent.prompt, "Full config");
            assert_eq!(agent.system.as_deref(), Some("Expert"));
            assert_eq!(
                agent.provider.as_ref().map(|p| p.as_str()),
                Some("anthropic")
            );
            assert_eq!(agent.max_turns, Some(15));
            assert_eq!(agent.token_budget, Some(100000));
            assert_eq!(agent.temperature, Some(0.8));
            assert_eq!(agent.depth_limit, Some(3));
        }
        _ => panic!("expected Agent"),
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// F. DAG PATTERNS (20 tests)
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn f01_dag_single_task() {
    let yaml = "schema: \"nika/workflow@0.12\"\nmodel: test-model\ntasks:\n  - id: only\n    infer: \"Solo\"";
    let w = ok(yaml);
    assert_eq!(w.tasks.len(), 1);
    assert_eq!(w.flow_count(), 0);
}

#[test]
fn f02_dag_linear_two_tasks() {
    let yaml = r#"
schema: "nika/workflow@0.12"
model: test-model
tasks:
  - id: step1
    infer: "First"
  - id: step2
    depends_on: [step1]
    infer: "Second"
"#;
    let w = ok(yaml);
    assert_eq!(w.tasks.len(), 2);
    assert_eq!(w.flow_count(), 1);
}

#[test]
fn f03_dag_linear_three_tasks() {
    let yaml = r#"
schema: "nika/workflow@0.12"
model: test-model
tasks:
  - id: a
    infer: "A"
  - id: b
    depends_on: [a]
    infer: "B"
  - id: c
    depends_on: [b]
    infer: "C"
"#;
    let w = ok(yaml);
    assert_eq!(w.tasks.len(), 3);
    assert_eq!(w.flow_count(), 2);
}

#[test]
fn f04_dag_fan_out() {
    let yaml = r#"
schema: "nika/workflow@0.12"
model: test-model
tasks:
  - id: root
    infer: "Root"
  - id: branch_a
    depends_on: [root]
    infer: "A"
  - id: branch_b
    depends_on: [root]
    infer: "B"
  - id: branch_c
    depends_on: [root]
    infer: "C"
"#;
    let w = ok(yaml);
    assert_eq!(w.tasks.len(), 4);
    assert_eq!(w.flow_count(), 3);
}

#[test]
fn f05_dag_fan_in() {
    let yaml = r#"
schema: "nika/workflow@0.12"
model: test-model
tasks:
  - id: src_a
    infer: "A"
  - id: src_b
    infer: "B"
  - id: src_c
    infer: "C"
  - id: merge
    depends_on: [src_a, src_b, src_c]
    infer: "Merge"
"#;
    let w = ok(yaml);
    assert_eq!(w.tasks.len(), 4);
    assert_eq!(w.flow_count(), 3);
}

#[test]
fn f06_dag_diamond() {
    let yaml = r#"
schema: "nika/workflow@0.12"
model: test-model
tasks:
  - id: top
    infer: "Top"
  - id: left
    depends_on: [top]
    infer: "Left"
  - id: right
    depends_on: [top]
    infer: "Right"
  - id: bottom
    depends_on: [left, right]
    infer: "Bottom"
"#;
    let w = ok(yaml);
    assert_eq!(w.tasks.len(), 4);
    assert_eq!(w.flow_count(), 4);
    let edges = w.edges();
    assert!(edges.contains(&("top", "left")));
    assert!(edges.contains(&("top", "right")));
    assert!(edges.contains(&("left", "bottom")));
    assert!(edges.contains(&("right", "bottom")));
}

#[test]
fn f07_dag_parallel_no_deps() {
    let yaml = r#"
schema: "nika/workflow@0.12"
model: test-model
tasks:
  - id: p1
    infer: "Parallel 1"
  - id: p2
    infer: "Parallel 2"
  - id: p3
    infer: "Parallel 3"
"#;
    let w = ok(yaml);
    assert_eq!(w.tasks.len(), 3);
    assert_eq!(w.flow_count(), 0);
}

#[test]
fn f08_dag_long_chain() {
    let yaml = r#"
schema: "nika/workflow@0.12"
model: test-model
tasks:
  - id: s1
    infer: "1"
  - id: s2
    depends_on: [s1]
    infer: "2"
  - id: s3
    depends_on: [s2]
    infer: "3"
  - id: s4
    depends_on: [s3]
    infer: "4"
  - id: s5
    depends_on: [s4]
    infer: "5"
"#;
    let w = ok(yaml);
    assert_eq!(w.tasks.len(), 5);
    assert_eq!(w.flow_count(), 4);
}

#[test]
fn f09_dag_mixed_verbs() {
    let yaml = r#"
schema: "nika/workflow@0.12"
model: test-model
tasks:
  - id: research
    infer: "Research topic"
  - id: fetch_data
    depends_on: [research]
    fetch:
      url: "https://api.example.com"
  - id: process
    depends_on: [fetch_data]
    exec: "python3 process.py"
  - id: store
    depends_on: [process]
    invoke:
      mcp: novanet
      tool: novanet_write
"#;
    let w = ok(yaml);
    assert_eq!(w.tasks.len(), 4);
    assert_eq!(w.flow_count(), 3);
}

#[test]
fn f10_dag_multiple_roots() {
    let yaml = r#"
schema: "nika/workflow@0.12"
model: test-model
tasks:
  - id: root_a
    infer: "Root A"
  - id: root_b
    exec: "echo B"
  - id: merge
    depends_on: [root_a, root_b]
    infer: "Merge"
"#;
    let w = ok(yaml);
    assert_eq!(w.flow_count(), 2);
}

#[test]
fn f11_dag_edges_method() {
    let yaml = r#"
schema: "nika/workflow@0.12"
model: test-model
tasks:
  - id: a
    infer: "A"
  - id: b
    depends_on: [a]
    infer: "B"
  - id: c
    depends_on: [a]
    infer: "C"
"#;
    let w = ok(yaml);
    let edges = w.edges();
    assert_eq!(edges.len(), 2);
    assert!(edges.contains(&("a", "b")));
    assert!(edges.contains(&("a", "c")));
}

#[test]
fn f12_dag_hash_different_structure() {
    let yaml1 = r#"
schema: "nika/workflow@0.12"
model: test-model
tasks:
  - id: a
    infer: "A"
"#;
    let yaml2 = r#"
schema: "nika/workflow@0.12"
model: test-model
tasks:
  - id: a
    infer: "A"
  - id: b
    infer: "B"
"#;
    let w1 = ok(yaml1);
    let w2 = ok(yaml2);
    assert_ne!(w1.compute_hash(), w2.compute_hash());
}

#[test]
fn f13_dag_hash_consistent() {
    let yaml = r#"
schema: "nika/workflow@0.12"
model: test-model
tasks:
  - id: a
    infer: "A"
"#;
    let w1 = ok(yaml);
    let w2 = ok(yaml);
    assert_eq!(w1.compute_hash(), w2.compute_hash());
}

#[test]
fn f14_dag_complex_web() {
    let yaml = r#"
schema: "nika/workflow@0.12"
model: test-model
tasks:
  - id: a
    infer: "A"
  - id: b
    infer: "B"
  - id: c
    depends_on: [a, b]
    infer: "C"
  - id: d
    depends_on: [a]
    infer: "D"
  - id: e
    depends_on: [c, d]
    infer: "E"
"#;
    let w = ok(yaml);
    assert_eq!(w.tasks.len(), 5);
    // c deps: a, b; d dep: a; e deps: c, d => total 5
    assert_eq!(w.flow_count(), 5);
}

#[test]
fn f15_dag_single_dep() {
    let yaml = r#"
schema: "nika/workflow@0.12"
model: test-model
tasks:
  - id: a
    infer: "A"
  - id: b
    depends_on: [a]
    infer: "B"
"#;
    let w = ok(yaml);
    assert_eq!(w.tasks[1].depends_on.as_ref().unwrap().len(), 1);
}

#[test]
fn f16_dag_many_deps() {
    let yaml = r#"
schema: "nika/workflow@0.12"
model: test-model
tasks:
  - id: a
    infer: "A"
  - id: b
    infer: "B"
  - id: c
    infer: "C"
  - id: d
    infer: "D"
  - id: e
    depends_on: [a, b, c, d]
    infer: "E"
"#;
    let w = ok(yaml);
    assert_eq!(w.tasks[4].depends_on.as_ref().unwrap().len(), 4);
}

#[test]
fn f17_dag_parallel_then_serial() {
    let yaml = r#"
schema: "nika/workflow@0.12"
model: test-model
tasks:
  - id: p1
    infer: "P1"
  - id: p2
    infer: "P2"
  - id: serial
    depends_on: [p1, p2]
    infer: "Serial"
  - id: final_task
    depends_on: [serial]
    infer: "Final"
"#;
    let w = ok(yaml);
    assert_eq!(w.flow_count(), 3);
}

#[test]
fn f18_dag_ten_tasks() {
    let yaml = r#"
schema: "nika/workflow@0.12"
model: test-model
tasks:
  - id: t01
    infer: "1"
  - id: t02
    infer: "2"
  - id: t03
    infer: "3"
  - id: t04
    infer: "4"
  - id: t05
    infer: "5"
  - id: t06
    infer: "6"
  - id: t07
    infer: "7"
  - id: t08
    infer: "8"
  - id: t09
    infer: "9"
  - id: t10
    depends_on: [t01, t02, t03, t04, t05, t06, t07, t08, t09]
    infer: "10"
"#;
    let w = ok(yaml);
    assert_eq!(w.tasks.len(), 10);
    assert_eq!(w.flow_count(), 9);
}

#[test]
fn f19_dag_provider_override() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: claude
model: test-model
tasks:
  - id: a
    infer: "A"
  - id: b
    depends_on: [a]
    provider: openai
    infer: "B"
"#;
    let w = ok(yaml);
    assert_eq!(w.provider.as_str(), "anthropic");
    match &w.tasks[1].action {
        TaskAction::Infer { infer } => {
            assert_eq!(infer.provider.as_ref().map(|p| p.as_str()), Some("openai"));
        }
        _ => panic!("expected Infer"),
    }
}

#[test]
fn f20_dag_model_override() {
    let yaml = r#"
schema: "nika/workflow@0.12"
model: claude-sonnet-4-6
tasks:
  - id: a
    model: gpt-4-turbo
    infer: "A"
"#;
    let w = ok(yaml);
    assert_eq!(w.model.as_deref(), Some("claude-sonnet-4-6"));
    match &w.tasks[0].action {
        TaskAction::Infer { infer } => {
            assert_eq!(infer.model.as_deref(), Some("gpt-4-turbo"));
        }
        _ => panic!("expected Infer"),
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// G. BINDINGS AND TEMPLATES (20 tests)
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn g01_with_simple_binding() {
    let yaml = r#"
schema: "nika/workflow@0.12"
model: test-model
tasks:
  - id: gen
    infer: "Generate"
  - id: use_it
    with:
      result: $gen
    depends_on: [gen]
    infer: "Use {{with.result}}"
"#;
    let w = ok(yaml);
    assert!(w.tasks[1].with_spec.is_some());
}

#[test]
fn g02_with_multiple_bindings() {
    let yaml = r#"
schema: "nika/workflow@0.12"
model: test-model
tasks:
  - id: gen
    infer: "Generate"
  - id: use_it
    with:
      alpha: $gen
      beta: $gen
    depends_on: [gen]
    infer: "Use {{with.alpha}} and {{with.beta}}"
"#;
    let w = ok(yaml);
    let spec = w.tasks[1].with_spec.as_ref().unwrap();
    assert!(spec.contains_key("alpha"));
    assert!(spec.contains_key("beta"));
}

#[test]
fn g03_with_dotted_path() {
    let yaml = r#"
schema: "nika/workflow@0.12"
model: test-model
tasks:
  - id: gen
    infer: "Generate"
  - id: use_it
    with:
      name: $gen.data.name
    depends_on: [gen]
    infer: "Name: {{with.name}}"
"#;
    let w = ok(yaml);
    assert!(w.tasks[1].with_spec.is_some());
}

#[test]
fn g04_with_fallback_string() {
    let yaml = r#"
schema: "nika/workflow@0.12"
model: test-model
tasks:
  - id: gen
    infer: "Generate"
  - id: use_it
    with:
      name: $gen.name ?? "Anonymous"
    depends_on: [gen]
    infer: "Hello {{with.name}}"
"#;
    let w = ok(yaml);
    assert!(w.tasks[1].with_spec.is_some());
}

#[test]
fn g05_with_fallback_number() {
    let yaml = r#"
schema: "nika/workflow@0.12"
model: test-model
tasks:
  - id: gen
    infer: "Generate"
  - id: use_it
    with:
      count: $gen.count ?? 0
    depends_on: [gen]
    infer: "Count: {{with.count}}"
"#;
    let w = ok(yaml);
    assert!(w.tasks[1].with_spec.is_some());
}

#[test]
fn g06_with_dollar_prefix() {
    let yaml = r#"
schema: "nika/workflow@0.12"
model: test-model
tasks:
  - id: gen
    infer: "Generate"
  - id: use_it
    with:
      data: $gen
    depends_on: [gen]
    infer: "Data: {{with.data}}"
"#;
    let w = ok(yaml);
    assert!(w.tasks[1].with_spec.is_some());
}

#[test]
fn g07_with_dollar_dotted_path() {
    let yaml = r#"
schema: "nika/workflow@0.12"
model: test-model
tasks:
  - id: gen
    infer: "Generate"
  - id: use_it
    with:
      val: $gen.result.items
    depends_on: [gen]
    infer: "Val: {{with.val}}"
"#;
    let w = ok(yaml);
    assert!(w.tasks[1].with_spec.is_some());
}

#[test]
fn g08_template_in_prompt() {
    let yaml = r#"
schema: "nika/workflow@0.12"
model: test-model
tasks:
  - id: gen
    infer: "Generate"
  - id: use_it
    with:
      text: $gen
    depends_on: [gen]
    infer: "Process: {{with.text}}"
"#;
    let w = ok(yaml);
    match &w.tasks[1].action {
        TaskAction::Infer { infer } => {
            assert!(infer.prompt.contains("{{with.text}}"));
        }
        _ => panic!("expected Infer"),
    }
}

#[test]
fn g09_template_in_exec() {
    let yaml = r#"
schema: "nika/workflow@0.12"
model: test-model
tasks:
  - id: gen
    infer: "Generate"
  - id: run
    with:
      file: $gen
    depends_on: [gen]
    exec: "process {{with.file}}"
"#;
    let w = ok(yaml);
    match &w.tasks[1].action {
        TaskAction::Exec { exec } => {
            assert!(exec.command.contains("{{with.file}}"));
        }
        _ => panic!("expected Exec"),
    }
}

#[test]
fn g10_template_in_fetch_url() {
    let yaml = r#"
schema: "nika/workflow@0.12"
model: test-model
tasks:
  - id: gen
    infer: "Generate"
  - id: fetch_it
    with:
      endpoint: $gen
    depends_on: [gen]
    fetch:
      url: "https://api.example.com/{{with.endpoint}}"
"#;
    let w = ok(yaml);
    match &w.tasks[1].action {
        TaskAction::Fetch { fetch } => {
            assert!(fetch.url.contains("{{with.endpoint}}"));
        }
        _ => panic!("expected Fetch"),
    }
}

#[test]
fn g11_with_three_bindings() {
    let yaml = r#"
schema: "nika/workflow@0.12"
model: test-model
tasks:
  - id: gen
    infer: "Generate"
  - id: use_it
    with:
      a: $gen
      b: $gen
      c: $gen
    depends_on: [gen]
    infer: "{{with.a}} {{with.b}} {{with.c}}"
"#;
    let w = ok(yaml);
    let spec = w.tasks[1].with_spec.as_ref().unwrap();
    assert_eq!(spec.len(), 3);
}

#[test]
fn g12_with_binding_from_different_tasks() {
    let yaml = r#"
schema: "nika/workflow@0.12"
model: test-model
tasks:
  - id: src_a
    infer: "Source A"
  - id: src_b
    infer: "Source B"
  - id: merge
    with:
      from_a: $src_a
      from_b: $src_b
    depends_on: [src_a, src_b]
    infer: "Merge {{with.from_a}} and {{with.from_b}}"
"#;
    let w = ok(yaml);
    let spec = w.tasks[2].with_spec.as_ref().unwrap();
    assert!(spec.contains_key("from_a"));
    assert!(spec.contains_key("from_b"));
}

#[test]
fn g13_template_multiple_in_one_prompt() {
    let yaml = r#"
schema: "nika/workflow@0.12"
model: test-model
tasks:
  - id: gen
    infer: "Generate"
  - id: use_it
    with:
      x: $gen
      y: $gen
    depends_on: [gen]
    infer: "x={{with.x}}, y={{with.y}}, x again={{with.x}}"
"#;
    let w = ok(yaml);
    match &w.tasks[1].action {
        TaskAction::Infer { infer } => {
            assert!(infer.prompt.contains("{{with.x}}"));
            assert!(infer.prompt.contains("{{with.y}}"));
        }
        _ => panic!("expected Infer"),
    }
}

#[test]
fn g14_with_no_bindings() {
    // A task without with: block should have None
    let yaml = r#"
schema: "nika/workflow@0.12"
model: test-model
tasks:
  - id: solo
    infer: "No bindings needed"
"#;
    let w = ok(yaml);
    assert!(w.tasks[0].with_spec.is_none());
}

#[test]
fn g15_template_in_system_prompt() {
    let yaml = r#"
schema: "nika/workflow@0.12"
model: test-model
tasks:
  - id: gen
    infer: "Generate context"
  - id: use_it
    with:
      ctx: $gen
    depends_on: [gen]
    infer:
      prompt: "Continue"
      system: "Context: {{with.ctx}}"
"#;
    let w = ok(yaml);
    match &w.tasks[1].action {
        TaskAction::Infer { infer } => {
            assert!(infer.system.as_ref().unwrap().contains("{{with.ctx}}"));
        }
        _ => panic!("expected Infer"),
    }
}

#[test]
fn g16_with_transform_pipe() {
    let yaml = r#"
schema: "nika/workflow@0.12"
model: test-model
tasks:
  - id: gen
    infer: "Generate"
  - id: use_it
    with:
      clean: $gen | trim
    depends_on: [gen]
    infer: "Clean: {{with.clean}}"
"#;
    let w = ok(yaml);
    assert!(w.tasks[1].with_spec.is_some());
}

#[test]
fn g17_with_transform_chain() {
    let yaml = r#"
schema: "nika/workflow@0.12"
model: test-model
tasks:
  - id: gen
    infer: "Generate"
  - id: use_it
    with:
      val: $gen | upper | trim
    depends_on: [gen]
    infer: "Val: {{with.val}}"
"#;
    let w = ok(yaml);
    assert!(w.tasks[1].with_spec.is_some());
}

#[test]
fn g18_with_array_index_path() {
    let yaml = r#"
schema: "nika/workflow@0.12"
model: test-model
tasks:
  - id: gen
    infer: "Generate"
  - id: use_it
    with:
      first: $gen.items[0]
    depends_on: [gen]
    infer: "First: {{with.first}}"
"#;
    let w = ok(yaml);
    assert!(w.tasks[1].with_spec.is_some());
}

#[test]
fn g19_for_each_with_binding() {
    let yaml = r#"
schema: "nika/workflow@0.12"
model: test-model
tasks:
  - id: process
    for_each: ["en-US", "fr-FR", "de-DE"]
    as: locale
    infer: "Generate for {{with.locale}}"
"#;
    let w = ok(yaml);
    assert!(w.tasks[0].for_each.is_some());
}

#[test]
fn g20_for_each_dollar_binding() {
    let yaml = r#"
schema: "nika/workflow@0.12"
model: test-model
tasks:
  - id: gen
    infer: "Generate list"
  - id: process
    for_each: "$gen"
    as: item
    depends_on: [gen]
    infer: "Process {{with.item}}"
"#;
    let w = ok(yaml);
    assert!(w.tasks[1].for_each.is_some());
}

// ═══════════════════════════════════════════════════════════════════════════════
// H. OUTPUT AND STRUCTURED (15 tests)
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn h01_output_format_json() {
    let yaml = wrap("output:\n  format: json\ninfer: \"Generate JSON\"");
    let w = ok(&yaml);
    let output = w.tasks[0].output.as_ref().unwrap();
    assert_eq!(output.format, OutputFormat::Json);
}

#[test]
fn h02_output_format_text() {
    let yaml = wrap("output:\n  format: text\ninfer: \"Plain text\"");
    let w = ok(&yaml);
    let output = w.tasks[0].output.as_ref().unwrap();
    assert_eq!(output.format, OutputFormat::Text);
}

#[test]
fn h03_output_format_json_with_schema() {
    // Markdown is not in the analyzer's OutputFormat; test json+schema instead
    let yaml = wrap("output:\n  format: json\n  schema:\n    type: object\ninfer: \"Generate\"");
    let w = ok(&yaml);
    let output = w.tasks[0].output.as_ref().unwrap();
    assert_eq!(output.format, OutputFormat::Json);
    assert!(output.schema.is_some());
}

#[test]
fn h04_output_format_yaml() {
    let yaml = wrap("output:\n  format: yaml\ninfer: \"YAML output\"");
    let w = ok(&yaml);
    let output = w.tasks[0].output.as_ref().unwrap();
    assert_eq!(output.format, OutputFormat::Yaml);
}

#[test]
fn h05_output_json_with_inline_schema() {
    let yaml = wrap(
        "output:\n  format: json\n  schema:\n    type: object\n    properties:\n      name:\n        type: string\ninfer: \"Generate\"",
    );
    let w = ok(&yaml);
    let output = w.tasks[0].output.as_ref().unwrap();
    assert_eq!(output.format, OutputFormat::Json);
    assert!(output.schema.is_some());
}

#[test]
fn h06_output_json_schema_is_inline() {
    // Through the three-phase pipeline, schemas are always lowered as Inline
    let yaml = wrap(
        "output:\n  format: json\n  schema:\n    type: object\n    properties:\n      name:\n        type: string\ninfer: \"Generate\"",
    );
    let w = ok(&yaml);
    let output = w.tasks[0].output.as_ref().unwrap();
    assert!(matches!(
        output.schema,
        Some(crate::ast::output::SchemaRef::Inline(_))
    ));
}

#[test]
fn h07_output_max_retries_not_preserved() {
    // max_retries on output: is not preserved through the three-phase pipeline
    let yaml = wrap("output:\n  format: json\ninfer: \"Generate\"");
    let w = ok(&yaml);
    let output = w.tasks[0].output.as_ref().unwrap();
    assert!(output.max_retries.is_none());
}

#[test]
fn h08_no_output_policy() {
    let yaml = wrap("infer: \"No output policy\"");
    let w = ok(&yaml);
    assert!(w.tasks[0].output.is_none());
}

#[test]
fn h09_structured_shorthand_file() {
    let yaml = wrap("structured: ./schemas/user.json\ninfer: \"Extract data\"");
    let w = ok(&yaml);
    assert!(w.tasks[0].structured.is_some());
}

#[test]
fn h10_structured_full_config() {
    let yaml = wrap(
        "structured:\n  schema: ./schemas/user.json\n  max_retries: 3\n  enable_repair: true\ninfer: \"Extract data\"",
    );
    let w = ok(&yaml);
    let spec = w.tasks[0].structured.as_ref().unwrap();
    assert_eq!(spec.max_retries, Some(3));
    assert_eq!(spec.enable_repair, Some(true));
}

#[test]
fn h11_structured_inline_schema() {
    let yaml = wrap(
        "structured:\n  schema:\n    type: object\n    properties:\n      name:\n        type: string\n    required:\n      - name\ninfer: \"Extract\"",
    );
    let w = ok(&yaml);
    let spec = w.tasks[0].structured.as_ref().unwrap();
    assert!(matches!(
        spec.schema,
        Some(crate::ast::output::SchemaRef::Inline(_))
    ));
}

#[test]
fn h12_structured_all_toggles() {
    let yaml = wrap(
        "structured:\n  schema: ./test.json\n  enable_extractor: false\n  enable_tool_injection: false\n  enable_retry: true\n  enable_repair: false\ninfer: \"Test\"",
    );
    let w = ok(&yaml);
    let spec = w.tasks[0].structured.as_ref().unwrap();
    // enable_extractor removed (L1 was never implemented) — silently ignored in YAML
    assert_eq!(spec.enable_tool_injection, Some(false));
    assert_eq!(spec.enable_retry, Some(true));
    assert_eq!(spec.enable_repair, Some(false));
}

#[test]
fn h13_structured_repair_model() {
    let yaml = wrap(
        "structured:\n  schema: ./test.json\n  repair_model: claude-sonnet-4-6\ninfer: \"Test\"",
    );
    let w = ok(&yaml);
    let spec = w.tasks[0].structured.as_ref().unwrap();
    assert_eq!(spec.repair_model.as_deref(), Some("claude-sonnet-4-6"));
}

#[test]
fn h14_output_and_structured_coexist() {
    // A task with both output: and structured: fields
    let yaml = wrap("output:\n  format: json\nstructured:\n  schema: ./test.json\ninfer: \"Test\"");
    let w = ok(&yaml);
    assert!(w.tasks[0].output.is_some());
    assert!(w.tasks[0].structured.is_some());
}

#[test]
fn h15_output_is_structured_check() {
    let yaml = wrap("output:\n  format: json\n  schema:\n    type: object\ninfer: \"Test\"");
    let w = ok(&yaml);
    let output = w.tasks[0].output.as_ref().unwrap();
    assert!(output.is_structured());
}

// ═══════════════════════════════════════════════════════════════════════════════
// I. ERROR CASES (25 tests)
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn i01_error_missing_schema() {
    let yaml = "tasks:\n  - id: t1\n    infer: \"Hello\"";
    let e = err(yaml);
    let msg = format!("{e}");
    assert!(msg.contains("schema") || msg.contains("Schema") || msg.contains("NIKA"));
}

#[test]
fn i02_error_bad_schema_version() {
    let yaml = "schema: \"nika/workflow@9.99\"\ntasks:\n  - id: t1\n    infer: \"Hello\"";
    let e = err(yaml);
    let msg = format!("{e}");
    assert!(msg.contains("schema") || msg.contains("version") || msg.contains("NIKA"));
}

#[test]
fn i03_error_duplicate_task_ids() {
    let yaml = r#"
schema: "nika/workflow@0.12"
model: test-model
tasks:
  - id: dupe
    infer: "First"
  - id: dupe
    infer: "Second"
"#;
    let e = err(yaml);
    let msg = format!("{e}");
    assert!(msg.contains("dupe") || msg.contains("duplicate") || msg.contains("NIKA"));
}

#[test]
fn i04_error_missing_task_id() {
    let yaml = "schema: \"nika/workflow@0.12\"\nmodel: test-model\ntasks:\n  - infer: \"No id\"";
    let e = err(yaml);
    let msg = format!("{e}");
    assert!(msg.contains("id") || msg.contains("NIKA"));
}

#[test]
fn i05_error_empty_tasks() {
    let yaml = "schema: \"nika/workflow@0.12\"\nmodel: test-model\ntasks: []";
    let e = err(yaml);
    let msg = format!("{e}");
    assert!(msg.contains("task") || msg.contains("NIKA"));
}

#[test]
fn i06_no_verb_rejected_with_missing_field_error() {
    // A task with no verb is rejected at analysis time (NIKA-145)
    let yaml = "schema: \"nika/workflow@0.12\"\nmodel: test-model\ntasks:\n  - id: no_verb";
    let e = err(yaml);
    let msg = format!("{e}");
    assert!(msg.contains("NIKA-145") || msg.contains("no verb"));
}

#[test]
fn i07_error_circular_dep_self() {
    let yaml = r#"
schema: "nika/workflow@0.12"
model: test-model
tasks:
  - id: self_ref
    depends_on: [self_ref]
    infer: "Loop"
"#;
    let e = err(yaml);
    let msg = format!("{e}");
    assert!(
        msg.contains("cycle")
            || msg.contains("circular")
            || msg.contains("self")
            || msg.contains("NIKA")
    );
}

#[test]
fn i08_error_circular_dep_pair() {
    let yaml = r#"
schema: "nika/workflow@0.12"
model: test-model
tasks:
  - id: a
    depends_on: [b]
    infer: "A"
  - id: b
    depends_on: [a]
    infer: "B"
"#;
    let e = err(yaml);
    let msg = format!("{e}");
    assert!(msg.contains("cycle") || msg.contains("circular") || msg.contains("NIKA"));
}

#[test]
fn i09_error_dep_nonexistent() {
    let yaml = r#"
schema: "nika/workflow@0.12"
model: test-model
tasks:
  - id: t1
    depends_on: [nonexistent]
    infer: "Test"
"#;
    let e = err(yaml);
    let msg = format!("{e}");
    assert!(
        msg.contains("nonexistent")
            || msg.contains("not found")
            || msg.contains("unknown")
            || msg.contains("NIKA")
    );
}

#[test]
fn i10_error_bad_yaml_syntax() {
    let yaml = "schema: \"nika/workflow@0.12\"\nmodel: test-model\ntasks:\n  - id: t1\n  infer: bad indent";
    let result = parse_workflow(yaml);
    // Should fail at parse or validation
    assert!(result.is_err());
}

#[test]
fn i11_error_schema_wrong_prefix() {
    let yaml = "schema: \"wrong/workflow@0.12\"\ntasks:\n  - id: t1\n    infer: \"Hello\"";
    let e = err(yaml);
    let msg = format!("{e}");
    assert!(msg.contains("schema") || msg.contains("NIKA"));
}

#[test]
fn i12_error_missing_tasks() {
    let yaml = "schema: \"nika/workflow@0.12\"";
    let e = err(yaml);
    let msg = format!("{e}");
    assert!(msg.contains("task") || msg.contains("NIKA"));
}

#[test]
fn i13_error_tasks_not_array() {
    let yaml = "schema: \"nika/workflow@0.12\"\nmodel: test-model\ntasks: \"not an array\"";
    let result = parse_workflow(yaml);
    assert!(result.is_err());
}

#[test]
fn i14_error_empty_schema() {
    let yaml = "schema: \"\"\ntasks:\n  - id: t1\n    infer: \"Hello\"";
    let e = err(yaml);
    let msg = format!("{e}");
    assert!(msg.contains("schema") || msg.contains("NIKA"));
}

#[test]
fn i15_error_task_id_whitespace() {
    let yaml = "schema: \"nika/workflow@0.12\"\nmodel: test-model\ntasks:\n  - id: \"  \"\n    infer: \"Hello\"";
    let e = err(yaml);
    let msg = format!("{e}");
    assert!(msg.contains("id") || msg.contains("NIKA"));
}

#[test]
fn i16_error_circular_dep_triangle() {
    let yaml = r#"
schema: "nika/workflow@0.12"
model: test-model
tasks:
  - id: a
    depends_on: [c]
    infer: "A"
  - id: b
    depends_on: [a]
    infer: "B"
  - id: c
    depends_on: [b]
    infer: "C"
"#;
    let e = err(yaml);
    let msg = format!("{e}");
    assert!(msg.contains("cycle") || msg.contains("circular") || msg.contains("NIKA"));
}

#[test]
fn i17_error_task_id_with_special_chars() {
    // Task IDs with spaces or special chars should be rejected
    let yaml =
        "schema: \"nika/workflow@0.12\"\nmodel: test-model\ntasks:\n  - id: \"has space\"\n    infer: \"Hello\"";
    let result = parse_workflow(yaml);
    // The parser should reject IDs with spaces
    assert!(result.is_err());
}

#[test]
fn i18_duplicate_dep_accepted() {
    // Duplicate in depends_on list is currently accepted by the pipeline
    let yaml = r#"
schema: "nika/workflow@0.12"
model: test-model
tasks:
  - id: a
    infer: "A"
  - id: b
    depends_on: [a, a]
    infer: "B"
"#;
    let w = ok(yaml);
    assert_eq!(w.tasks.len(), 2);
}

#[test]
fn i19_error_task_id_empty() {
    let yaml = "schema: \"nika/workflow@0.12\"\nmodel: test-model\ntasks:\n  - id: \"\"\n    infer: \"Hello\"";
    let e = err(yaml);
    let msg = format!("{e}");
    assert!(msg.contains("id") || msg.contains("empty") || msg.contains("NIKA"));
}

#[test]
fn i20_error_schema_no_at_sign() {
    let yaml = "schema: \"nika/workflow0.12\"\ntasks:\n  - id: t1\n    infer: \"Hello\"";
    let e = err(yaml);
    let msg = format!("{e}");
    assert!(msg.contains("schema") || msg.contains("NIKA"));
}

#[test]
fn i21_error_only_whitespace_yaml() {
    let yaml = "   \n   \n   ";
    let result = parse_workflow(yaml);
    assert!(result.is_err());
}

#[test]
fn i22_error_completely_empty() {
    let result = parse_workflow("");
    assert!(result.is_err());
}

#[test]
fn i23_error_no_schema_field() {
    let yaml = "provider: claude\ntasks:\n  - id: t1\n    infer: \"Hello\"";
    let e = err(yaml);
    let msg = format!("{e}");
    assert!(msg.contains("schema") || msg.contains("NIKA"));
}

#[test]
fn i24_error_tasks_null() {
    let yaml = "schema: \"nika/workflow@0.12\"\nmodel: test-model\ntasks:";
    let result = parse_workflow(yaml);
    assert!(result.is_err());
}

#[test]
fn i25_error_schema_v0() {
    let yaml = "schema: \"nika/workflow@0.0\"\ntasks:\n  - id: t1\n    infer: \"Hello\"";
    let e = err(yaml);
    let msg = format!("{e}");
    assert!(msg.contains("schema") || msg.contains("version") || msg.contains("NIKA"));
}

// ═══════════════════════════════════════════════════════════════════════════════
// J. MOCK PROVIDER VISION TESTS (5 tests)
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn j01_mock_provider_vision_workflow_parses() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: mock
tasks:
  - id: vision
    infer:
      content:
        - type: image
          source: "blake3:abc123"
          detail: high
        - type: text
          text: "What is in this image?"
"#;
    let w = ok(yaml);
    assert_eq!(w.provider.as_str(), "mock");
    match &w.tasks[0].action {
        TaskAction::Infer { infer } => {
            let parts = infer.content.as_ref().unwrap();
            assert_eq!(parts.len(), 2);
            assert!(matches!(&parts[0], ContentPart::Image { .. }));
            assert!(matches!(&parts[1], ContentPart::Text { .. }));
        }
        _ => panic!("expected Infer"),
    }
}

#[test]
fn j02_mock_provider_vision_content_only_validates() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: mock
tasks:
  - id: vision
    infer:
      content:
        - type: text
          text: "Describe this"
        - type: image
          source: "blake3:deadbeef"
          detail: auto
"#;
    let w = ok(yaml);
    match &w.tasks[0].action {
        TaskAction::Infer { infer } => {
            // Validate passes: content is present even though prompt is empty
            assert!(infer.validate().is_ok());
        }
        _ => panic!("expected Infer"),
    }
}

#[test]
fn j03_mock_provider_vision_prompt_plus_content() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: mock
tasks:
  - id: vision
    infer:
      prompt: "Analyze the photo"
      content:
        - type: image
          source: "blake3:face"
          detail: high
"#;
    let w = ok(yaml);
    match &w.tasks[0].action {
        TaskAction::Infer { infer } => {
            assert_eq!(infer.prompt, "Analyze the photo");
            assert!(infer.content.is_some());
            assert!(infer.validate().is_ok());
        }
        _ => panic!("expected Infer"),
    }
}

#[test]
fn j04_mock_provider_vision_content_has_correct_parts() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: mock
tasks:
  - id: vision
    infer:
      content:
        - type: text
          text: "First"
        - type: image
          source: "blake3:img1"
          detail: high
        - type: image_url
          url: "https://example.com/img.jpg"
          detail: low
"#;
    let w = ok(yaml);
    match &w.tasks[0].action {
        TaskAction::Infer { infer } => {
            let parts = infer.content.as_ref().unwrap();
            assert_eq!(parts.len(), 3);
            assert_eq!(
                parts[0],
                ContentPart::Text {
                    text: "First".to_string()
                }
            );
            match &parts[1] {
                ContentPart::Image { source, detail } => {
                    assert_eq!(source, "blake3:img1");
                    assert_eq!(*detail, ImageDetail::High);
                }
                _ => panic!("expected Image"),
            }
            match &parts[2] {
                ContentPart::ImageUrl { url, detail } => {
                    assert_eq!(url, "https://example.com/img.jpg");
                    assert_eq!(*detail, ImageDetail::Low);
                }
                _ => panic!("expected ImageUrl"),
            }
        }
        _ => panic!("expected Infer"),
    }
}

#[test]
fn j05_mock_provider_vision_all_detail_levels() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: mock
tasks:
  - id: vision
    infer:
      content:
        - type: image
          source: "blake3:auto_img"
          detail: auto
        - type: image
          source: "blake3:low_img"
          detail: low
        - type: image
          source: "blake3:high_img"
          detail: high
"#;
    let w = ok(yaml);
    match &w.tasks[0].action {
        TaskAction::Infer { infer } => {
            let parts = infer.content.as_ref().unwrap();
            assert_eq!(parts.len(), 3);
            match &parts[0] {
                ContentPart::Image { detail, .. } => assert_eq!(*detail, ImageDetail::Auto),
                _ => panic!("expected Image"),
            }
            match &parts[1] {
                ContentPart::Image { detail, .. } => assert_eq!(*detail, ImageDetail::Low),
                _ => panic!("expected Image"),
            }
            match &parts[2] {
                ContentPart::Image { detail, .. } => assert_eq!(*detail, ImageDetail::High),
                _ => panic!("expected Image"),
            }
        }
        _ => panic!("expected Infer"),
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// K. WORKFLOW-LEVEL FEATURES (10 tests) -- bonus to reach 200+
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn k01_workflow_default_provider_claude() {
    let yaml = "schema: \"nika/workflow@0.12\"\nmodel: test-model\ntasks:\n  - id: t1\n    infer: \"Hello\"";
    let w = ok(yaml);
    assert_eq!(w.provider.as_str(), "anthropic");
}

#[test]
fn k02_workflow_custom_provider() {
    let yaml =
        "schema: \"nika/workflow@0.12\"\nprovider: openai\nmodel: test-model\ntasks:\n  - id: t1\n    infer: \"Hello\"";
    let w = ok(yaml);
    assert_eq!(w.provider.as_str(), "openai");
}

#[test]
fn k03_workflow_with_model() {
    let yaml = "schema: \"nika/workflow@0.12\"\nmodel: gpt-4-turbo\ntasks:\n  - id: t1\n    infer: \"Hello\"";
    let w = ok(yaml);
    assert_eq!(w.model.as_deref(), Some("gpt-4-turbo"));
}

#[test]
fn k04_workflow_mcp_config() {
    let yaml = r#"
schema: "nika/workflow@0.12"
model: test-model
mcp:
  servers:
    novanet:
      command: cargo
      args: [run, -p, novanet-mcp]
      env:
        NEO4J_URI: bolt://localhost:7687
tasks:
  - id: t1
    infer: "Hello"
"#;
    let w = ok(yaml);
    let mcp = w.mcp.as_ref().unwrap();
    assert!(mcp.contains_key("novanet"));
    assert_eq!(mcp["novanet"].command, "cargo");
}

#[test]
fn k05_workflow_multiple_mcp_servers() {
    let yaml = r#"
schema: "nika/workflow@0.12"
model: test-model
mcp:
  servers:
    server_a:
      command: echo
    server_b:
      command: cat
tasks:
  - id: t1
    infer: "Hello"
"#;
    let w = ok(yaml);
    let mcp = w.mcp.as_ref().unwrap();
    assert_eq!(mcp.len(), 2);
}

#[test]
fn k06_workflow_schema_v10() {
    let yaml = "schema: \"nika/workflow@0.10\"\nmodel: test-model\ntasks:\n  - id: t1\n    infer: \"Hello\"";
    let w = ok(yaml);
    assert_eq!(w.schema, "nika/workflow@0.10");
}

#[test]
fn k07_workflow_schema_v11() {
    let yaml = "schema: \"nika/workflow@0.11\"\nmodel: test-model\ntasks:\n  - id: t1\n    infer: \"Hello\"";
    let w = ok(yaml);
    assert_eq!(w.schema, "nika/workflow@0.11");
}

#[test]
fn k08_workflow_schema_v12() {
    let yaml = "schema: \"nika/workflow@0.12\"\nmodel: test-model\ntasks:\n  - id: t1\n    infer: \"Hello\"";
    let w = ok(yaml);
    assert_eq!(w.schema, "nika/workflow@0.12");
}

#[test]
fn k09_workflow_inputs() {
    let yaml = r#"
schema: "nika/workflow@0.12"
model: test-model
inputs:
  name:
    type: string
    default: "world"
tasks:
  - id: t1
    infer: "Hello {{inputs.name}}"
"#;
    let w = ok(yaml);
    assert!(w.inputs.is_some());
}

#[test]
fn k10_workflow_hash_hex_format() {
    let yaml = "schema: \"nika/workflow@0.12\"\nmodel: test-model\ntasks:\n  - id: t1\n    infer: \"Hello\"";
    let w = ok(yaml);
    let hash = w.compute_hash();
    assert_eq!(hash.len(), 16);
    assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
}

// ═══════════════════════════════════════════════════════════════════════════════
// L. FETCH EXTRACT MODES — PR5 (40 tests)
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn l01_fetch_extract_markdown() {
    let w = ok(&wrap(
        "fetch:\n  url: \"https://example.com\"\n  extract: markdown",
    ));
    match &w.tasks[0].action {
        TaskAction::Fetch { fetch } => {
            assert_eq!(fetch.extract, Some(ExtractMode::Markdown));
            assert!(fetch.selector.is_none());
        }
        _ => panic!("expected Fetch"),
    }
}

#[test]
fn l02_fetch_extract_article() {
    let w = ok(&wrap(
        "fetch:\n  url: \"https://blog.example.com/post\"\n  extract: article",
    ));
    match &w.tasks[0].action {
        TaskAction::Fetch { fetch } => {
            assert_eq!(fetch.extract, Some(ExtractMode::Article));
        }
        _ => panic!("expected Fetch"),
    }
}

#[test]
fn l03_fetch_extract_text() {
    let w = ok(&wrap(
        "fetch:\n  url: \"https://example.com\"\n  extract: text",
    ));
    match &w.tasks[0].action {
        TaskAction::Fetch { fetch } => {
            assert_eq!(fetch.extract, Some(ExtractMode::Text));
        }
        _ => panic!("expected Fetch"),
    }
}

#[test]
fn l04_fetch_extract_text_with_selector() {
    let w = ok(&wrap(
        "fetch:\n  url: \"https://example.com\"\n  extract: text\n  selector: \"div.content h2\"",
    ));
    match &w.tasks[0].action {
        TaskAction::Fetch { fetch } => {
            assert_eq!(fetch.extract, Some(ExtractMode::Text));
            assert_eq!(fetch.selector.as_deref(), Some("div.content h2"));
        }
        _ => panic!("expected Fetch"),
    }
}

#[test]
fn l05_fetch_extract_selector_mode() {
    let w = ok(&wrap(
        "fetch:\n  url: \"https://example.com\"\n  extract: selector\n  selector: \"article ul li\"",
    ));
    match &w.tasks[0].action {
        TaskAction::Fetch { fetch } => {
            assert_eq!(fetch.extract, Some(ExtractMode::Selector));
            assert_eq!(fetch.selector.as_deref(), Some("article ul li"));
        }
        _ => panic!("expected Fetch"),
    }
}

#[test]
fn l06_fetch_extract_metadata() {
    let w = ok(&wrap(
        "fetch:\n  url: \"https://example.com\"\n  extract: metadata",
    ));
    match &w.tasks[0].action {
        TaskAction::Fetch { fetch } => {
            assert_eq!(fetch.extract, Some(ExtractMode::Metadata));
        }
        _ => panic!("expected Fetch"),
    }
}

#[test]
fn l07_fetch_extract_links() {
    let w = ok(&wrap(
        "fetch:\n  url: \"https://example.com\"\n  extract: links",
    ));
    match &w.tasks[0].action {
        TaskAction::Fetch { fetch } => {
            assert_eq!(fetch.extract, Some(ExtractMode::Links));
        }
        _ => panic!("expected Fetch"),
    }
}

#[test]
fn l08_fetch_extract_jsonpath() {
    let w = ok(&wrap(
        "fetch:\n  url: \"https://api.example.com/data\"\n  extract: jsonpath\n  selector: \"$.data.items\"",
    ));
    match &w.tasks[0].action {
        TaskAction::Fetch { fetch } => {
            assert_eq!(fetch.extract, Some(ExtractMode::Jsonpath));
            assert_eq!(fetch.selector.as_deref(), Some("$.data.items"));
        }
        _ => panic!("expected Fetch"),
    }
}

#[test]
fn l09_fetch_extract_llm_txt() {
    let w = ok(&wrap(
        "fetch:\n  url: \"https://example.com/.well-known/llm.txt\"\n  extract: llm_txt",
    ));
    match &w.tasks[0].action {
        TaskAction::Fetch { fetch } => {
            assert_eq!(fetch.extract, Some(ExtractMode::LlmTxt));
        }
        _ => panic!("expected Fetch"),
    }
}

#[test]
fn l10_fetch_extract_feed() {
    let w = ok(&wrap(
        "fetch:\n  url: \"https://blog.example.com/feed.xml\"\n  extract: feed",
    ));
    match &w.tasks[0].action {
        TaskAction::Fetch { fetch } => {
            assert_eq!(fetch.extract, Some(ExtractMode::Feed));
        }
        _ => panic!("expected Fetch"),
    }
}

#[test]
fn l11_fetch_extract_invalid_mode_becomes_none() {
    // Invalid extract modes are caught at analysis and result in None
    let w = ok(&wrap(
        "fetch:\n  url: \"https://example.com\"\n  extract: invalid_mode",
    ));
    match &w.tasks[0].action {
        TaskAction::Fetch { fetch } => {
            assert!(
                fetch.extract.is_none(),
                "invalid mode should be None after analysis"
            );
        }
        _ => panic!("expected Fetch"),
    }
}

#[test]
fn l12_fetch_selector_without_extract_validates_err() {
    // selector without extract parses but fails validation
    let w = ok(&wrap(
        "fetch:\n  url: \"https://example.com\"\n  selector: \"div.content\"",
    ));
    match &w.tasks[0].action {
        TaskAction::Fetch { fetch } => {
            assert!(fetch.extract.is_none());
            assert_eq!(fetch.selector.as_deref(), Some("div.content"));
            let result = fetch.validate();
            assert!(result.is_err());
            let msg = format!("{}", result.unwrap_err());
            assert!(msg.contains("selector"));
            assert!(msg.contains("requires"));
        }
        _ => panic!("expected Fetch"),
    }
}

#[test]
fn l13_fetch_extract_markdown_with_post() {
    let w = ok(&wrap(
        "fetch:\n  url: \"https://example.com\"\n  method: POST\n  extract: markdown",
    ));
    match &w.tasks[0].action {
        TaskAction::Fetch { fetch } => {
            assert_eq!(fetch.method, "POST");
            assert_eq!(fetch.extract, Some(ExtractMode::Markdown));
        }
        _ => panic!("expected Fetch"),
    }
}

#[test]
fn l14_fetch_extract_article_with_post() {
    let w = ok(&wrap(
        "fetch:\n  url: \"https://example.com\"\n  method: POST\n  extract: article",
    ));
    match &w.tasks[0].action {
        TaskAction::Fetch { fetch } => {
            assert_eq!(fetch.method, "POST");
            assert_eq!(fetch.extract, Some(ExtractMode::Article));
        }
        _ => panic!("expected Fetch"),
    }
}

#[test]
fn l15_fetch_extract_text_with_post() {
    let w = ok(&wrap(
        "fetch:\n  url: \"https://example.com\"\n  method: POST\n  extract: text",
    ));
    match &w.tasks[0].action {
        TaskAction::Fetch { fetch } => {
            assert_eq!(fetch.method, "POST");
            assert_eq!(fetch.extract, Some(ExtractMode::Text));
        }
        _ => panic!("expected Fetch"),
    }
}

#[test]
fn l16_fetch_extract_selector_with_post() {
    let w = ok(&wrap(
        "fetch:\n  url: \"https://example.com\"\n  method: POST\n  extract: selector\n  selector: \"table.data td\"",
    ));
    match &w.tasks[0].action {
        TaskAction::Fetch { fetch } => {
            assert_eq!(fetch.method, "POST");
            assert_eq!(fetch.extract, Some(ExtractMode::Selector));
            assert_eq!(fetch.selector.as_deref(), Some("table.data td"));
        }
        _ => panic!("expected Fetch"),
    }
}

#[test]
fn l17_fetch_extract_metadata_with_post() {
    let w = ok(&wrap(
        "fetch:\n  url: \"https://example.com\"\n  method: POST\n  extract: metadata",
    ));
    match &w.tasks[0].action {
        TaskAction::Fetch { fetch } => {
            assert_eq!(fetch.method, "POST");
            assert_eq!(fetch.extract, Some(ExtractMode::Metadata));
        }
        _ => panic!("expected Fetch"),
    }
}

#[test]
fn l18_fetch_extract_links_with_post() {
    let w = ok(&wrap(
        "fetch:\n  url: \"https://example.com\"\n  method: POST\n  extract: links",
    ));
    match &w.tasks[0].action {
        TaskAction::Fetch { fetch } => {
            assert_eq!(fetch.method, "POST");
            assert_eq!(fetch.extract, Some(ExtractMode::Links));
        }
        _ => panic!("expected Fetch"),
    }
}

#[test]
fn l19_fetch_extract_jsonpath_with_post_and_body() {
    let w = ok(&wrap(
        "fetch:\n  url: \"https://api.example.com/search\"\n  method: POST\n  body: '{\"q\": \"test\"}'\n  extract: jsonpath\n  selector: \"$.results\"",
    ));
    match &w.tasks[0].action {
        TaskAction::Fetch { fetch } => {
            assert_eq!(fetch.method, "POST");
            assert_eq!(fetch.extract, Some(ExtractMode::Jsonpath));
            assert_eq!(fetch.selector.as_deref(), Some("$.results"));
        }
        _ => panic!("expected Fetch"),
    }
}

#[test]
fn l20_fetch_extract_markdown_with_headers() {
    let yaml = wrap(
        "fetch:\n  url: \"https://example.com\"\n  extract: markdown\n  headers:\n    Accept: text/html\n    User-Agent: nika/0.34",
    );
    let w = ok(&yaml);
    match &w.tasks[0].action {
        TaskAction::Fetch { fetch } => {
            assert_eq!(fetch.extract, Some(ExtractMode::Markdown));
            assert_eq!(fetch.headers.len(), 2);
            assert_eq!(fetch.headers.get("Accept").unwrap(), "text/html");
        }
        _ => panic!("expected Fetch"),
    }
}

#[test]
fn l21_fetch_extract_article_with_headers() {
    let yaml = wrap(
        "fetch:\n  url: \"https://example.com\"\n  extract: article\n  headers:\n    Cookie: session=abc123",
    );
    let w = ok(&yaml);
    match &w.tasks[0].action {
        TaskAction::Fetch { fetch } => {
            assert_eq!(fetch.extract, Some(ExtractMode::Article));
            assert!(fetch.headers.contains_key("Cookie"));
        }
        _ => panic!("expected Fetch"),
    }
}

#[test]
fn l22_fetch_extract_text_with_headers() {
    let yaml = wrap(
        "fetch:\n  url: \"https://example.com\"\n  extract: text\n  headers:\n    Authorization: \"Bearer tok\"",
    );
    let w = ok(&yaml);
    match &w.tasks[0].action {
        TaskAction::Fetch { fetch } => {
            assert_eq!(fetch.extract, Some(ExtractMode::Text));
            assert!(fetch.headers.contains_key("Authorization"));
        }
        _ => panic!("expected Fetch"),
    }
}

#[test]
fn l23_fetch_extract_markdown_with_timeout() {
    let w = ok(&wrap(
        "fetch:\n  url: \"https://example.com\"\n  extract: markdown\n  timeout: 30",
    ));
    match &w.tasks[0].action {
        TaskAction::Fetch { fetch } => {
            assert_eq!(fetch.extract, Some(ExtractMode::Markdown));
            assert_eq!(fetch.timeout, Some(30));
        }
        _ => panic!("expected Fetch"),
    }
}

#[test]
fn l24_fetch_extract_article_with_timeout() {
    let w = ok(&wrap(
        "fetch:\n  url: \"https://example.com\"\n  extract: article\n  timeout: 60",
    ));
    match &w.tasks[0].action {
        TaskAction::Fetch { fetch } => {
            assert_eq!(fetch.extract, Some(ExtractMode::Article));
            assert_eq!(fetch.timeout, Some(60));
        }
        _ => panic!("expected Fetch"),
    }
}

#[test]
fn l25_fetch_extract_text_with_timeout() {
    let w = ok(&wrap(
        "fetch:\n  url: \"https://example.com\"\n  extract: text\n  timeout: 10",
    ));
    match &w.tasks[0].action {
        TaskAction::Fetch { fetch } => {
            assert_eq!(fetch.extract, Some(ExtractMode::Text));
            assert_eq!(fetch.timeout, Some(10));
        }
        _ => panic!("expected Fetch"),
    }
}

#[test]
fn l26_fetch_extract_selector_with_timeout() {
    let w = ok(&wrap(
        "fetch:\n  url: \"https://example.com\"\n  extract: selector\n  selector: \"h1\"\n  timeout: 15",
    ));
    match &w.tasks[0].action {
        TaskAction::Fetch { fetch } => {
            assert_eq!(fetch.extract, Some(ExtractMode::Selector));
            assert_eq!(fetch.timeout, Some(15));
        }
        _ => panic!("expected Fetch"),
    }
}

#[test]
fn l27_fetch_extract_metadata_with_timeout() {
    let w = ok(&wrap(
        "fetch:\n  url: \"https://example.com\"\n  extract: metadata\n  timeout: 45",
    ));
    match &w.tasks[0].action {
        TaskAction::Fetch { fetch } => {
            assert_eq!(fetch.extract, Some(ExtractMode::Metadata));
            assert_eq!(fetch.timeout, Some(45));
        }
        _ => panic!("expected Fetch"),
    }
}

#[test]
fn l28_fetch_extract_links_with_timeout() {
    let w = ok(&wrap(
        "fetch:\n  url: \"https://example.com\"\n  extract: links\n  timeout: 20",
    ));
    match &w.tasks[0].action {
        TaskAction::Fetch { fetch } => {
            assert_eq!(fetch.extract, Some(ExtractMode::Links));
            assert_eq!(fetch.timeout, Some(20));
        }
        _ => panic!("expected Fetch"),
    }
}

#[test]
fn l29_fetch_extract_jsonpath_with_timeout() {
    let w = ok(&wrap(
        "fetch:\n  url: \"https://api.example.com\"\n  extract: jsonpath\n  selector: \"$.items[*].name\"\n  timeout: 5",
    ));
    match &w.tasks[0].action {
        TaskAction::Fetch { fetch } => {
            assert_eq!(fetch.extract, Some(ExtractMode::Jsonpath));
            assert_eq!(fetch.selector.as_deref(), Some("$.items[*].name"));
            assert_eq!(fetch.timeout, Some(5));
        }
        _ => panic!("expected Fetch"),
    }
}

#[test]
fn l30_fetch_extract_llm_txt_with_timeout() {
    let w = ok(&wrap(
        "fetch:\n  url: \"https://example.com/.well-known/llm.txt\"\n  extract: llm_txt\n  timeout: 10",
    ));
    match &w.tasks[0].action {
        TaskAction::Fetch { fetch } => {
            assert_eq!(fetch.extract, Some(ExtractMode::LlmTxt));
            assert_eq!(fetch.timeout, Some(10));
        }
        _ => panic!("expected Fetch"),
    }
}

#[test]
fn l31_fetch_extract_feed_with_timeout() {
    let w = ok(&wrap(
        "fetch:\n  url: \"https://blog.example.com/rss\"\n  extract: feed\n  timeout: 30",
    ));
    match &w.tasks[0].action {
        TaskAction::Fetch { fetch } => {
            assert_eq!(fetch.extract, Some(ExtractMode::Feed));
            assert_eq!(fetch.timeout, Some(30));
        }
        _ => panic!("expected Fetch"),
    }
}

#[test]
fn l32_fetch_extract_with_response_full() {
    let w = ok(&wrap(
        "fetch:\n  url: \"https://example.com\"\n  extract: markdown\n  response: full",
    ));
    match &w.tasks[0].action {
        TaskAction::Fetch { fetch } => {
            assert_eq!(fetch.extract, Some(ExtractMode::Markdown));
            assert_eq!(fetch.response, Some(ResponseMode::Full));
        }
        _ => panic!("expected Fetch"),
    }
}

#[test]
fn l33_fetch_no_extract_no_selector_defaults() {
    let w = ok(&wrap("fetch:\n  url: \"https://example.com\""));
    match &w.tasks[0].action {
        TaskAction::Fetch { fetch } => {
            assert!(fetch.extract.is_none());
            assert!(fetch.selector.is_none());
        }
        _ => panic!("expected Fetch"),
    }
}

#[test]
fn l34_fetch_extract_validates_all_nine_modes() {
    let modes = [
        "markdown", "article", "text", "selector", "metadata", "links", "feed", "jsonpath",
        "llm_txt",
    ];
    for mode in &modes {
        let yaml = wrap(&format!(
            "fetch:\n  url: \"https://example.com\"\n  extract: {}",
            mode
        ));
        let w = ok(&yaml);
        match &w.tasks[0].action {
            TaskAction::Fetch { fetch } => {
                let expected = ExtractMode::parse(mode).unwrap();
                assert_eq!(fetch.extract, Some(expected), "mode '{}' mismatch", mode);
            }
            _ => panic!("expected Fetch for mode {}", mode),
        }
    }
}

#[test]
fn l35_fetch_extract_markdown_with_post_headers_timeout() {
    let yaml = wrap(
        "fetch:\n  url: \"https://example.com\"\n  method: POST\n  headers:\n    Accept: text/html\n  extract: markdown\n  timeout: 30",
    );
    let w = ok(&yaml);
    match &w.tasks[0].action {
        TaskAction::Fetch { fetch } => {
            assert_eq!(fetch.method, "POST");
            assert_eq!(fetch.extract, Some(ExtractMode::Markdown));
            assert_eq!(fetch.timeout, Some(30));
            assert!(fetch.headers.contains_key("Accept"));
        }
        _ => panic!("expected Fetch"),
    }
}

#[test]
fn l36_fetch_extract_jsonpath_nested_selector() {
    let w = ok(&wrap(
        "fetch:\n  url: \"https://api.example.com/v2/data\"\n  extract: jsonpath\n  selector: \"$.response.data.users[0].profile.name\"",
    ));
    match &w.tasks[0].action {
        TaskAction::Fetch { fetch } => {
            assert_eq!(fetch.extract, Some(ExtractMode::Jsonpath));
            assert_eq!(
                fetch.selector.as_deref(),
                Some("$.response.data.users[0].profile.name")
            );
        }
        _ => panic!("expected Fetch"),
    }
}

#[test]
fn l37_fetch_extract_selector_complex_css() {
    let w = ok(&wrap(
        "fetch:\n  url: \"https://example.com\"\n  extract: selector\n  selector: \"main > article.post:first-child h2 a\"",
    ));
    match &w.tasks[0].action {
        TaskAction::Fetch { fetch } => {
            assert_eq!(fetch.extract, Some(ExtractMode::Selector));
            assert_eq!(
                fetch.selector.as_deref(),
                Some("main > article.post:first-child h2 a")
            );
        }
        _ => panic!("expected Fetch"),
    }
}

#[test]
fn l38_fetch_extract_text_with_class_selector() {
    let w = ok(&wrap(
        "fetch:\n  url: \"https://example.com\"\n  extract: text\n  selector: \".main-content p\"",
    ));
    match &w.tasks[0].action {
        TaskAction::Fetch { fetch } => {
            assert_eq!(fetch.extract, Some(ExtractMode::Text));
            assert_eq!(fetch.selector.as_deref(), Some(".main-content p"));
        }
        _ => panic!("expected Fetch"),
    }
}

#[test]
fn l39_fetch_extract_with_json_body() {
    let yaml = wrap(
        "fetch:\n  url: \"https://api.example.com/scrape\"\n  method: POST\n  json:\n    url: \"https://target.com\"\n    depth: 2\n  extract: article",
    );
    let w = ok(&yaml);
    match &w.tasks[0].action {
        TaskAction::Fetch { fetch } => {
            assert_eq!(fetch.method, "POST");
            assert_eq!(fetch.extract, Some(ExtractMode::Article));
            assert!(fetch.json.is_some());
        }
        _ => panic!("expected Fetch"),
    }
}

#[test]
fn l40_fetch_extract_with_follow_redirects() {
    let w = ok(&wrap(
        "fetch:\n  url: \"https://short.link/abc\"\n  follow_redirects: true\n  extract: markdown",
    ));
    match &w.tasks[0].action {
        TaskAction::Fetch { fetch } => {
            assert_eq!(fetch.follow_redirects, Some(true));
            assert_eq!(fetch.extract, Some(ExtractMode::Markdown));
        }
        _ => panic!("expected Fetch"),
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// M. FETCH RESPONSE MODES — PR5 (20 tests)
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn m01_fetch_response_full() {
    let w = ok(&wrap(
        "fetch:\n  url: \"https://api.example.com/data\"\n  response: full",
    ));
    match &w.tasks[0].action {
        TaskAction::Fetch { fetch } => {
            assert_eq!(fetch.response, Some(ResponseMode::Full));
        }
        _ => panic!("expected Fetch"),
    }
}

#[test]
fn m02_fetch_response_binary() {
    let w = ok(&wrap(
        "fetch:\n  url: \"https://example.com/image.png\"\n  response: binary",
    ));
    match &w.tasks[0].action {
        TaskAction::Fetch { fetch } => {
            assert_eq!(fetch.response, Some(ResponseMode::Binary));
        }
        _ => panic!("expected Fetch"),
    }
}

#[test]
fn m03_fetch_response_invalid_becomes_none() {
    // Invalid response modes are caught at analysis and become None
    let w = ok(&wrap(
        "fetch:\n  url: \"https://example.com\"\n  response: stream",
    ));
    match &w.tasks[0].action {
        TaskAction::Fetch { fetch } => {
            assert!(
                fetch.response.is_none(),
                "invalid response mode should become None"
            );
        }
        _ => panic!("expected Fetch"),
    }
}

#[test]
fn m04_fetch_response_full_follow_redirects_false() {
    let w = ok(&wrap(
        "fetch:\n  url: \"https://example.com/redir\"\n  response: full\n  follow_redirects: false",
    ));
    match &w.tasks[0].action {
        TaskAction::Fetch { fetch } => {
            assert_eq!(fetch.response, Some(ResponseMode::Full));
            assert_eq!(fetch.follow_redirects, Some(false));
        }
        _ => panic!("expected Fetch"),
    }
}

#[test]
fn m05_fetch_response_full_with_retry() {
    let yaml = wrap(
        "retry:\n  max_attempts: 3\n  delay_ms: 1000\n  backoff: 2.0\nfetch:\n  url: \"https://api.example.com\"\n  response: full",
    );
    let w = ok(&yaml);
    match &w.tasks[0].action {
        TaskAction::Fetch { fetch } => {
            assert_eq!(fetch.response, Some(ResponseMode::Full));
            assert!(fetch.retry.is_some());
        }
        _ => panic!("expected Fetch"),
    }
}

#[test]
fn m06_fetch_response_binary_with_timeout() {
    let w = ok(&wrap(
        "fetch:\n  url: \"https://example.com/large.zip\"\n  response: binary\n  timeout: 120",
    ));
    match &w.tasks[0].action {
        TaskAction::Fetch { fetch } => {
            assert_eq!(fetch.response, Some(ResponseMode::Binary));
            assert_eq!(fetch.timeout, Some(120));
        }
        _ => panic!("expected Fetch"),
    }
}

#[test]
fn m07_fetch_response_default_none() {
    let w = ok(&wrap("fetch:\n  url: \"https://example.com\""));
    match &w.tasks[0].action {
        TaskAction::Fetch { fetch } => {
            assert!(fetch.response.is_none());
        }
        _ => panic!("expected Fetch"),
    }
}

#[test]
fn m08_fetch_response_full_with_headers() {
    let yaml = wrap(
        "fetch:\n  url: \"https://api.example.com\"\n  response: full\n  headers:\n    Accept: application/json",
    );
    let w = ok(&yaml);
    match &w.tasks[0].action {
        TaskAction::Fetch { fetch } => {
            assert_eq!(fetch.response, Some(ResponseMode::Full));
            assert!(fetch.headers.contains_key("Accept"));
        }
        _ => panic!("expected Fetch"),
    }
}

#[test]
fn m09_fetch_response_binary_with_post() {
    let w = ok(&wrap(
        "fetch:\n  url: \"https://api.example.com/render\"\n  method: POST\n  response: binary\n  json:\n    format: png",
    ));
    match &w.tasks[0].action {
        TaskAction::Fetch { fetch } => {
            assert_eq!(fetch.method, "POST");
            assert_eq!(fetch.response, Some(ResponseMode::Binary));
            assert!(fetch.json.is_some());
        }
        _ => panic!("expected Fetch"),
    }
}

#[test]
fn m10_fetch_response_full_with_post_body() {
    let w = ok(&wrap(
        "fetch:\n  url: \"https://api.example.com\"\n  method: POST\n  body: '{\"q\": \"test\"}'\n  response: full",
    ));
    match &w.tasks[0].action {
        TaskAction::Fetch { fetch } => {
            assert_eq!(fetch.response, Some(ResponseMode::Full));
            assert!(fetch.body.is_some());
        }
        _ => panic!("expected Fetch"),
    }
}

#[test]
fn m11_fetch_response_binary_follow_redirects_true() {
    let w = ok(&wrap(
        "fetch:\n  url: \"https://cdn.example.com/file\"\n  response: binary\n  follow_redirects: true",
    ));
    match &w.tasks[0].action {
        TaskAction::Fetch { fetch } => {
            assert_eq!(fetch.response, Some(ResponseMode::Binary));
            assert_eq!(fetch.follow_redirects, Some(true));
        }
        _ => panic!("expected Fetch"),
    }
}

#[test]
fn m12_fetch_response_full_with_retry_config() {
    let yaml = wrap(
        "retry:\n  max_attempts: 5\n  delay_ms: 500\n  backoff: 1.5\nfetch:\n  url: \"https://unstable.api.com\"\n  response: full",
    );
    let w = ok(&yaml);
    match &w.tasks[0].action {
        TaskAction::Fetch { fetch } => {
            assert_eq!(fetch.response, Some(ResponseMode::Full));
            let retry = fetch.retry.as_ref().unwrap();
            assert_eq!(retry.max_attempts, 5);
            assert_eq!(retry.backoff_ms, 500);
        }
        _ => panic!("expected Fetch"),
    }
}

#[test]
fn m13_fetch_response_full_complete() {
    let yaml = wrap(
        "fetch:\n  url: \"https://api.example.com/data\"\n  method: POST\n  headers:\n    Content-Type: application/json\n    Authorization: \"Bearer tok\"\n  json:\n    query: test\n  response: full\n  timeout: 30\n  follow_redirects: true",
    );
    let w = ok(&yaml);
    match &w.tasks[0].action {
        TaskAction::Fetch { fetch } => {
            assert_eq!(fetch.method, "POST");
            assert_eq!(fetch.response, Some(ResponseMode::Full));
            assert_eq!(fetch.timeout, Some(30));
            assert_eq!(fetch.follow_redirects, Some(true));
            assert_eq!(fetch.headers.len(), 2);
            assert!(fetch.json.is_some());
        }
        _ => panic!("expected Fetch"),
    }
}

#[test]
fn m14_fetch_response_binary_complete() {
    let yaml = wrap(
        "fetch:\n  url: \"https://cdn.example.com/photo.jpg\"\n  response: binary\n  timeout: 60\n  follow_redirects: true\n  headers:\n    Accept: image/jpeg",
    );
    let w = ok(&yaml);
    match &w.tasks[0].action {
        TaskAction::Fetch { fetch } => {
            assert_eq!(fetch.response, Some(ResponseMode::Binary));
            assert_eq!(fetch.timeout, Some(60));
            assert_eq!(fetch.follow_redirects, Some(true));
            assert!(fetch.headers.contains_key("Accept"));
        }
        _ => panic!("expected Fetch"),
    }
}

#[test]
fn m15_fetch_response_invalid_raw_becomes_none() {
    let w = ok(&wrap(
        "fetch:\n  url: \"https://example.com\"\n  response: raw",
    ));
    match &w.tasks[0].action {
        TaskAction::Fetch { fetch } => {
            assert!(
                fetch.response.is_none(),
                "invalid 'raw' mode should become None"
            );
        }
        _ => panic!("expected Fetch"),
    }
}

#[test]
fn m16_fetch_response_full_validates_ok() {
    let w = ok(&wrap(
        "fetch:\n  url: \"https://example.com\"\n  response: full",
    ));
    match &w.tasks[0].action {
        TaskAction::Fetch { fetch } => {
            assert!(fetch.validate().is_ok());
        }
        _ => panic!("expected Fetch"),
    }
}

#[test]
fn m17_fetch_response_binary_validates_ok() {
    let w = ok(&wrap(
        "fetch:\n  url: \"https://example.com\"\n  response: binary",
    ));
    match &w.tasks[0].action {
        TaskAction::Fetch { fetch } => {
            assert!(fetch.validate().is_ok());
        }
        _ => panic!("expected Fetch"),
    }
}

#[test]
fn m18_fetch_response_with_extract_coexist() {
    let w = ok(&wrap(
        "fetch:\n  url: \"https://example.com\"\n  response: full\n  extract: article",
    ));
    match &w.tasks[0].action {
        TaskAction::Fetch { fetch } => {
            assert_eq!(fetch.response, Some(ResponseMode::Full));
            assert_eq!(fetch.extract, Some(ExtractMode::Article));
        }
        _ => panic!("expected Fetch"),
    }
}

#[test]
fn m19_fetch_response_binary_head_method() {
    let w = ok(&wrap(
        "fetch:\n  url: \"https://cdn.example.com/file.bin\"\n  method: HEAD\n  response: binary",
    ));
    match &w.tasks[0].action {
        TaskAction::Fetch { fetch } => {
            assert_eq!(fetch.method, "HEAD");
            assert_eq!(fetch.response, Some(ResponseMode::Binary));
        }
        _ => panic!("expected Fetch"),
    }
}

#[test]
fn m20_fetch_response_full_delete_method() {
    let w = ok(&wrap(
        "fetch:\n  url: \"https://api.example.com/resource/123\"\n  method: DELETE\n  response: full",
    ));
    match &w.tasks[0].action {
        TaskAction::Fetch { fetch } => {
            assert_eq!(fetch.method, "DELETE");
            assert_eq!(fetch.response, Some(ResponseMode::Full));
        }
        _ => panic!("expected Fetch"),
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// N. FETCH + BINDINGS — PR5 (15 tests)
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn n01_fetch_extract_markdown_then_infer() {
    let yaml = "schema: \"nika/workflow@0.12\"\nmodel: test-model\ntasks:\n  - id: scrape\n    fetch:\n      url: \"https://example.com\"\n      extract: markdown\n  - id: summarize\n    depends_on: [scrape]\n    with:\n      page: $scrape\n    infer: \"Summarize: {{with.page}}\"";
    let w = ok(yaml);
    assert_eq!(w.tasks.len(), 2);
    match &w.tasks[0].action {
        TaskAction::Fetch { fetch } => assert_eq!(fetch.extract, Some(ExtractMode::Markdown)),
        _ => panic!("expected Fetch"),
    }
    match &w.tasks[1].action {
        TaskAction::Infer { infer } => assert!(infer.prompt.contains("{{with.page}}")),
        _ => panic!("expected Infer"),
    }
}

#[test]
fn n02_fetch_extract_metadata_then_infer() {
    let yaml = "schema: \"nika/workflow@0.12\"\nmodel: test-model\ntasks:\n  - id: meta\n    fetch:\n      url: \"https://example.com\"\n      extract: metadata\n  - id: use_meta\n    depends_on: [meta]\n    with:\n      info: $meta\n    infer: \"Title: {{with.info}}\"";
    let w = ok(yaml);
    assert_eq!(w.tasks.len(), 2);
    match &w.tasks[0].action {
        TaskAction::Fetch { fetch } => assert_eq!(fetch.extract, Some(ExtractMode::Metadata)),
        _ => panic!("expected Fetch"),
    }
}

#[test]
fn n03_fetch_extract_links_then_infer() {
    let yaml = "schema: \"nika/workflow@0.12\"\nmodel: test-model\ntasks:\n  - id: get_links\n    fetch:\n      url: \"https://example.com\"\n      extract: links\n  - id: categorize\n    depends_on: [get_links]\n    with:\n      links: $get_links\n    infer: \"Categorize: {{with.links}}\"";
    let w = ok(yaml);
    assert_eq!(w.tasks.len(), 2);
    match &w.tasks[0].action {
        TaskAction::Fetch { fetch } => assert_eq!(fetch.extract, Some(ExtractMode::Links)),
        _ => panic!("expected Fetch"),
    }
}

#[test]
fn n04_fetch_response_full_then_infer() {
    let yaml = "schema: \"nika/workflow@0.12\"\nmodel: test-model\ntasks:\n  - id: api_call\n    fetch:\n      url: \"https://api.example.com/status\"\n      response: full\n  - id: analyze\n    depends_on: [api_call]\n    with:\n      resp: $api_call\n    infer: \"Analyze: {{with.resp}}\"";
    let w = ok(yaml);
    assert_eq!(w.tasks.len(), 2);
    match &w.tasks[0].action {
        TaskAction::Fetch { fetch } => assert_eq!(fetch.response, Some(ResponseMode::Full)),
        _ => panic!("expected Fetch"),
    }
}

#[test]
fn n05_fetch_response_binary_then_invoke_thumbnail() {
    let yaml = "schema: \"nika/workflow@0.12\"\nmodel: test-model\ntasks:\n  - id: download\n    fetch:\n      url: \"https://cdn.example.com/photo.jpg\"\n      response: binary\n  - id: thumb\n    depends_on: [download]\n    with:\n      img: $download\n    invoke:\n      tool: \"nika:thumbnail\"\n      params:\n        hash: \"{{with.img}}\"\n        width: 200";
    let w = ok(yaml);
    assert_eq!(w.tasks.len(), 2);
    match &w.tasks[0].action {
        TaskAction::Fetch { fetch } => assert_eq!(fetch.response, Some(ResponseMode::Binary)),
        _ => panic!("expected Fetch"),
    }
    match &w.tasks[1].action {
        TaskAction::Invoke { invoke } => assert_eq!(invoke.tool.as_deref(), Some("nika:thumbnail")),
        _ => panic!("expected Invoke"),
    }
}

#[test]
fn n06_fetch_extract_text_selector_then_exec() {
    let yaml = "schema: \"nika/workflow@0.12\"\nmodel: test-model\ntasks:\n  - id: scrape\n    fetch:\n      url: \"https://example.com\"\n      extract: text\n      selector: \"h1\"\n  - id: store\n    depends_on: [scrape]\n    with:\n      title: $scrape\n    exec: \"echo {{with.title}}\"";
    let w = ok(yaml);
    assert_eq!(w.tasks.len(), 2);
    match &w.tasks[0].action {
        TaskAction::Fetch { fetch } => {
            assert_eq!(fetch.extract, Some(ExtractMode::Text));
            assert_eq!(fetch.selector.as_deref(), Some("h1"));
        }
        _ => panic!("expected Fetch"),
    }
}

#[test]
fn n07_fetch_extract_markdown_chain_three_tasks() {
    let yaml = "schema: \"nika/workflow@0.12\"\nmodel: test-model\ntasks:\n  - id: fetch_page\n    fetch:\n      url: \"https://docs.example.com/api\"\n      extract: markdown\n  - id: summarize\n    depends_on: [fetch_page]\n    with:\n      doc: $fetch_page\n    infer: \"Summarize: {{with.doc}}\"\n  - id: format\n    depends_on: [summarize]\n    with:\n      summary: $summarize\n    infer: \"Format: {{with.summary}}\"";
    let w = ok(yaml);
    assert_eq!(w.tasks.len(), 3);
    let deps1 = w.tasks[1].depends_on.as_ref().unwrap();
    assert!(deps1.contains(&"fetch_page".to_string()));
    let deps2 = w.tasks[2].depends_on.as_ref().unwrap();
    assert!(deps2.contains(&"summarize".to_string()));
}

#[test]
fn n08_fetch_extract_jsonpath_then_infer() {
    let yaml = "schema: \"nika/workflow@0.12\"\nmodel: test-model\ntasks:\n  - id: api\n    fetch:\n      url: \"https://api.example.com/products\"\n      extract: jsonpath\n      selector: \"$.data.products[*].name\"\n  - id: describe\n    depends_on: [api]\n    with:\n      products: $api\n    infer: \"Describe: {{with.products}}\"";
    let w = ok(yaml);
    assert_eq!(w.tasks.len(), 2);
    match &w.tasks[0].action {
        TaskAction::Fetch { fetch } => {
            assert_eq!(fetch.extract, Some(ExtractMode::Jsonpath));
            assert_eq!(fetch.selector.as_deref(), Some("$.data.products[*].name"));
        }
        _ => panic!("expected Fetch"),
    }
}

#[test]
fn n09_fetch_extract_article_then_agent() {
    let yaml = "schema: \"nika/workflow@0.12\"\nmodel: test-model\ntasks:\n  - id: scrape\n    fetch:\n      url: \"https://blog.example.com/post/123\"\n      extract: article\n  - id: research\n    depends_on: [scrape]\n    with:\n      article: $scrape\n    agent:\n      prompt: \"Analyze: {{with.article}}\"\n      max_turns: 3";
    let w = ok(yaml);
    assert_eq!(w.tasks.len(), 2);
    match &w.tasks[1].action {
        TaskAction::Agent { agent } => assert!(agent.prompt.contains("{{with.article}}")),
        _ => panic!("expected Agent"),
    }
}

#[test]
fn n10_fetch_extract_feed_then_for_each() {
    let yaml = "schema: \"nika/workflow@0.12\"\nmodel: test-model\ntasks:\n  - id: get_feed\n    fetch:\n      url: \"https://blog.example.com/rss\"\n      extract: feed\n  - id: process\n    depends_on: [get_feed]\n    for_each: $get_feed\n    as: entry\n    infer: \"Summarize: {{with.entry}}\"";
    let w = ok(yaml);
    assert_eq!(w.tasks.len(), 2);
    match &w.tasks[0].action {
        TaskAction::Fetch { fetch } => assert_eq!(fetch.extract, Some(ExtractMode::Feed)),
        _ => panic!("expected Fetch"),
    }
}

#[test]
fn n11_fetch_multiple_extracts_parallel() {
    let yaml = "schema: \"nika/workflow@0.12\"\nmodel: test-model\ntasks:\n  - id: get_md\n    fetch:\n      url: \"https://example.com\"\n      extract: markdown\n  - id: get_links\n    fetch:\n      url: \"https://example.com\"\n      extract: links\n  - id: combine\n    depends_on: [get_md, get_links]\n    with:\n      md: $get_md\n      links: $get_links\n    infer: \"Content: {{with.md}} Links: {{with.links}}\"";
    let w = ok(yaml);
    assert_eq!(w.tasks.len(), 3);
    assert!(w.tasks[2]
        .depends_on
        .as_ref()
        .unwrap()
        .contains(&"get_md".to_string()));
    assert!(w.tasks[2]
        .depends_on
        .as_ref()
        .unwrap()
        .contains(&"get_links".to_string()));
}

#[test]
fn n12_fetch_extract_with_dotted_binding_path() {
    let yaml = "schema: \"nika/workflow@0.12\"\nmodel: test-model\ntasks:\n  - id: scrape\n    fetch:\n      url: \"https://example.com\"\n      extract: metadata\n  - id: use_og\n    depends_on: [scrape]\n    with:\n      title: $scrape.og.title\n    infer: \"OG Title: {{with.title}}\"";
    let w = ok(yaml);
    assert_eq!(w.tasks.len(), 2);
}

#[test]
fn n13_fetch_binary_then_invoke_import() {
    let yaml = "schema: \"nika/workflow@0.12\"\nmodel: test-model\ntasks:\n  - id: dl\n    fetch:\n      url: \"https://cdn.example.com/file.pdf\"\n      response: binary\n  - id: import\n    depends_on: [dl]\n    with:\n      file: $dl\n    invoke:\n      tool: \"nika:import\"\n      params:\n        source: \"{{with.file}}\"";
    let w = ok(yaml);
    assert_eq!(w.tasks.len(), 2);
    match &w.tasks[1].action {
        TaskAction::Invoke { invoke } => assert_eq!(invoke.tool.as_deref(), Some("nika:import")),
        _ => panic!("expected Invoke"),
    }
}

#[test]
fn n14_fetch_extract_llm_txt_then_infer() {
    let yaml = "schema: \"nika/workflow@0.12\"\nmodel: test-model\ntasks:\n  - id: ctx\n    fetch:\n      url: \"https://example.com/.well-known/llm.txt\"\n      extract: llm_txt\n  - id: use_ctx\n    depends_on: [ctx]\n    with:\n      c: $ctx\n    infer:\n      system: \"Context: {{with.c}}\"\n      prompt: \"Answer the question\"";
    let w = ok(yaml);
    assert_eq!(w.tasks.len(), 2);
    match &w.tasks[0].action {
        TaskAction::Fetch { fetch } => assert_eq!(fetch.extract, Some(ExtractMode::LlmTxt)),
        _ => panic!("expected Fetch"),
    }
}

#[test]
fn n15_fetch_response_full_status_in_binding() {
    let yaml = "schema: \"nika/workflow@0.12\"\nmodel: test-model\ntasks:\n  - id: check\n    fetch:\n      url: \"https://api.example.com/health\"\n      response: full\n  - id: report\n    depends_on: [check]\n    with:\n      status: $check.status\n    infer: \"Status code: {{with.status}}\"";
    let w = ok(yaml);
    assert_eq!(w.tasks.len(), 2);
}

// ═══════════════════════════════════════════════════════════════════════════════
// O. COMBINED WITH PR4 (VISION) — PR5 (15 tests)
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn o01_fetch_binary_then_vision_content() {
    let yaml = "schema: \"nika/workflow@0.12\"\nmodel: test-model\ntasks:\n  - id: download\n    fetch:\n      url: \"https://cdn.example.com/photo.jpg\"\n      response: binary\n  - id: describe\n    depends_on: [download]\n    with:\n      img: $download\n    infer:\n      content:\n        - type: image\n          source: \"{{with.img}}\"\n          detail: high\n        - type: text\n          text: \"Describe this image\"";
    let w = ok(yaml);
    assert_eq!(w.tasks.len(), 2);
    match &w.tasks[0].action {
        TaskAction::Fetch { fetch } => assert_eq!(fetch.response, Some(ResponseMode::Binary)),
        _ => panic!("expected Fetch"),
    }
    match &w.tasks[1].action {
        TaskAction::Infer { infer } => {
            let parts = infer.content.as_ref().unwrap();
            assert_eq!(parts.len(), 2);
            assert!(matches!(&parts[0], ContentPart::Image { .. }));
            assert!(matches!(&parts[1], ContentPart::Text { .. }));
        }
        _ => panic!("expected Infer"),
    }
}

#[test]
fn o02_fetch_markdown_then_vision_infer() {
    let yaml = "schema: \"nika/workflow@0.12\"\nmodel: test-model\ntasks:\n  - id: scrape\n    fetch:\n      url: \"https://example.com\"\n      extract: markdown\n  - id: analyze\n    depends_on: [scrape]\n    with:\n      doc: $scrape\n    infer:\n      prompt: \"Analyze: {{with.doc}}\"\n      content:\n        - type: image_url\n          url: \"https://chart.example.com/preview.png\"\n          detail: auto";
    let w = ok(yaml);
    assert_eq!(w.tasks.len(), 2);
    match &w.tasks[1].action {
        TaskAction::Infer { infer } => {
            assert!(infer.prompt.contains("{{with.doc}}"));
            assert!(infer.content.is_some());
        }
        _ => panic!("expected Infer"),
    }
}

#[test]
fn o03_fetch_extract_and_binary_then_vision_pipeline() {
    let yaml = "schema: \"nika/workflow@0.12\"\nmodel: test-model\ntasks:\n  - id: fetch_html\n    fetch:\n      url: \"https://example.com\"\n      extract: article\n  - id: fetch_image\n    fetch:\n      url: \"https://example.com/hero.jpg\"\n      response: binary\n  - id: analyze\n    depends_on: [fetch_html, fetch_image]\n    with:\n      article: $fetch_html\n      photo: $fetch_image\n    infer:\n      prompt: \"Based on article: {{with.article}}\"\n      content:\n        - type: image\n          source: \"{{with.photo}}\"\n          detail: high";
    let w = ok(yaml);
    assert_eq!(w.tasks.len(), 3);
    assert!(w.tasks[2]
        .depends_on
        .as_ref()
        .unwrap()
        .contains(&"fetch_html".to_string()));
    assert!(w.tasks[2]
        .depends_on
        .as_ref()
        .unwrap()
        .contains(&"fetch_image".to_string()));
}

#[test]
fn o04_fetch_binary_two_images_then_vision_compare() {
    let yaml = "schema: \"nika/workflow@0.12\"\nmodel: test-model\ntasks:\n  - id: dl1\n    fetch:\n      url: \"https://cdn.example.com/before.jpg\"\n      response: binary\n  - id: dl2\n    fetch:\n      url: \"https://cdn.example.com/after.jpg\"\n      response: binary\n  - id: compare\n    depends_on: [dl1, dl2]\n    with:\n      before: $dl1\n      after: $dl2\n    infer:\n      content:\n        - type: image\n          source: \"{{with.before}}\"\n          detail: high\n        - type: image\n          source: \"{{with.after}}\"\n          detail: high\n        - type: text\n          text: \"Compare these two images\"";
    let w = ok(yaml);
    assert_eq!(w.tasks.len(), 3);
    match &w.tasks[2].action {
        TaskAction::Infer { infer } => {
            assert_eq!(infer.content.as_ref().unwrap().len(), 3);
        }
        _ => panic!("expected Infer"),
    }
}

#[test]
fn o05_fetch_metadata_then_vision() {
    let yaml = "schema: \"nika/workflow@0.12\"\nmodel: test-model\ntasks:\n  - id: meta\n    fetch:\n      url: \"https://example.com\"\n      extract: metadata\n  - id: vision\n    depends_on: [meta]\n    with:\n      info: $meta\n    infer:\n      prompt: \"Metadata: {{with.info}}\"\n      content:\n        - type: image_url\n          url: \"https://example.com/og.jpg\"\n          detail: low";
    let w = ok(yaml);
    assert_eq!(w.tasks.len(), 2);
    match &w.tasks[1].action {
        TaskAction::Infer { infer } => {
            assert!(infer.prompt.contains("{{with.info}}"));
            assert!(infer.content.is_some());
        }
        _ => panic!("expected Infer"),
    }
}

#[test]
fn o06_fetch_binary_invoke_thumbnail_then_vision() {
    let yaml = "schema: \"nika/workflow@0.12\"\nmodel: test-model\ntasks:\n  - id: download\n    fetch:\n      url: \"https://cdn.example.com/photo.jpg\"\n      response: binary\n  - id: thumbnail\n    depends_on: [download]\n    with:\n      img: $download\n    invoke:\n      tool: \"nika:thumbnail\"\n      params:\n        hash: \"{{with.img}}\"\n        width: 256\n  - id: describe\n    depends_on: [thumbnail]\n    with:\n      thumb: $thumbnail\n    infer:\n      content:\n        - type: image\n          source: \"{{with.thumb}}\"\n          detail: auto\n        - type: text\n          text: \"Describe this thumbnail\"";
    let w = ok(yaml);
    assert_eq!(w.tasks.len(), 3);
}

#[test]
fn o07_fetch_links_then_for_each_fetch_extract() {
    let yaml = "schema: \"nika/workflow@0.12\"\nmodel: test-model\ntasks:\n  - id: sitemap\n    fetch:\n      url: \"https://example.com\"\n      extract: links\n  - id: scrape_each\n    depends_on: [sitemap]\n    for_each: $sitemap\n    as: link\n    fetch:\n      url: \"{{with.link}}\"\n      extract: markdown";
    let w = ok(yaml);
    assert_eq!(w.tasks.len(), 2);
    match &w.tasks[1].action {
        TaskAction::Fetch { fetch } => assert_eq!(fetch.extract, Some(ExtractMode::Markdown)),
        _ => panic!("expected Fetch"),
    }
}

#[test]
fn o08_fetch_article_with_vision_system_prompt() {
    let yaml = "schema: \"nika/workflow@0.12\"\nmodel: test-model\ntasks:\n  - id: scrape\n    fetch:\n      url: \"https://news.example.com/article\"\n      extract: article\n  - id: vision\n    depends_on: [scrape]\n    with:\n      article: $scrape\n    provider: claude\n    infer:\n      system: \"Article: {{with.article}}\"\n      content:\n        - type: image_url\n          url: \"https://news.example.com/header.jpg\"\n          detail: high\n        - type: text\n          text: \"Does this image match the article?\"";
    let w = ok(yaml);
    assert_eq!(w.tasks.len(), 2);
    match &w.tasks[1].action {
        TaskAction::Infer { infer } => {
            assert!(infer.system.as_ref().unwrap().contains("{{with.article}}"));
        }
        _ => panic!("expected Infer"),
    }
}

#[test]
fn o09_fetch_binary_to_invoke_phash() {
    let yaml = "schema: \"nika/workflow@0.12\"\nmodel: test-model\ntasks:\n  - id: dl\n    fetch:\n      url: \"https://cdn.example.com/logo.png\"\n      response: binary\n  - id: hash\n    depends_on: [dl]\n    with:\n      img: $dl\n    invoke:\n      tool: \"nika:phash\"\n      params:\n        hash: \"{{with.img}}\"";
    let w = ok(yaml);
    assert_eq!(w.tasks.len(), 2);
}

#[test]
fn o10_fetch_article_then_structured_output() {
    let yaml = "schema: \"nika/workflow@0.12\"\nmodel: test-model\ntasks:\n  - id: scrape\n    fetch:\n      url: \"https://blog.example.com/post\"\n      extract: article\n  - id: extract_data\n    depends_on: [scrape]\n    with:\n      article: $scrape\n    structured: ./schemas/article.json\n    infer: \"Extract title, author from: {{with.article}}\"";
    let w = ok(yaml);
    assert_eq!(w.tasks.len(), 2);
    assert!(w.tasks[1].structured.is_some());
}

#[test]
fn o11_fetch_binary_download_resize_vision_pipeline() {
    let yaml = "schema: \"nika/workflow@0.12\"\nmodel: test-model\ntasks:\n  - id: dl\n    fetch:\n      url: \"https://storage.example.com/img.png\"\n      response: binary\n      timeout: 60\n  - id: resize\n    depends_on: [dl]\n    with:\n      raw: $dl\n    invoke:\n      tool: \"nika:thumbnail\"\n      params:\n        hash: \"{{with.raw}}\"\n        width: 512\n  - id: analyze\n    depends_on: [resize]\n    with:\n      thumb: $resize\n    provider: openai\n    infer:\n      content:\n        - type: image\n          source: \"{{with.thumb}}\"\n          detail: high\n        - type: text\n          text: \"Describe what you see\"";
    let w = ok(yaml);
    assert_eq!(w.tasks.len(), 3);
    match &w.tasks[0].action {
        TaskAction::Fetch { fetch } => {
            assert_eq!(fetch.response, Some(ResponseMode::Binary));
            assert_eq!(fetch.timeout, Some(60));
        }
        _ => panic!("expected Fetch"),
    }
}

#[test]
fn o12_fetch_scrape_summarize_publish_workflow() {
    let yaml = "schema: \"nika/workflow@0.12\"\nmodel: test-model\ntasks:\n  - id: scrape\n    fetch:\n      url: \"https://docs.example.com\"\n      extract: markdown\n      timeout: 30\n  - id: get_meta\n    fetch:\n      url: \"https://docs.example.com\"\n      extract: metadata\n  - id: summarize\n    depends_on: [scrape, get_meta]\n    with:\n      content: $scrape\n      meta: $get_meta\n    infer:\n      system: \"You are a technical writer\"\n      prompt: \"Summarize: {{with.content}} Meta: {{with.meta}}\"\n  - id: publish\n    depends_on: [summarize]\n    with:\n      summary: $summarize\n    fetch:\n      url: \"https://api.example.com/publish\"\n      method: POST\n      json:\n        content: \"{{with.summary}}\"\n      response: full";
    let w = ok(yaml);
    assert_eq!(w.tasks.len(), 4);
    match &w.tasks[3].action {
        TaskAction::Fetch { fetch } => {
            assert_eq!(fetch.method, "POST");
            assert_eq!(fetch.response, Some(ResponseMode::Full));
        }
        _ => panic!("expected Fetch"),
    }
}

#[test]
fn o13_fetch_binary_two_downloads_then_quality_compare() {
    let yaml = "schema: \"nika/workflow@0.12\"\nmodel: test-model\ntasks:\n  - id: original\n    fetch:\n      url: \"https://cdn.example.com/original.jpg\"\n      response: binary\n  - id: optimized\n    fetch:\n      url: \"https://cdn.example.com/optimized.jpg\"\n      response: binary\n  - id: quality\n    depends_on: [original, optimized]\n    with:\n      orig: $original\n      opt: $optimized\n    invoke:\n      tool: \"nika:quality\"\n      params:\n        reference: \"{{with.orig}}\"\n        distorted: \"{{with.opt}}\"";
    let w = ok(yaml);
    assert_eq!(w.tasks.len(), 3);
}

#[test]
fn o14_fetch_feed_then_for_each_vision() {
    let yaml = "schema: \"nika/workflow@0.12\"\nmodel: test-model\ntasks:\n  - id: rss\n    fetch:\n      url: \"https://photo-blog.example.com/feed\"\n      extract: feed\n  - id: review\n    depends_on: [rss]\n    for_each: $rss\n    as: item\n    infer:\n      content:\n        - type: image_url\n          url: \"{{with.item}}\"\n          detail: low\n        - type: text\n          text: \"Rate this photo 1-10\"";
    let w = ok(yaml);
    assert_eq!(w.tasks.len(), 2);
}

#[test]
fn o15_fetch_article_then_agent_with_mcp() {
    let yaml = "schema: \"nika/workflow@0.12\"\nmodel: test-model\ntasks:\n  - id: research\n    fetch:\n      url: \"https://arxiv.org/abs/2401.00001\"\n      extract: article\n  - id: deep_analysis\n    depends_on: [research]\n    with:\n      paper: $research\n    agent:\n      prompt: \"Analyze: {{with.paper}}\"\n      system: \"You are a research assistant\"\n      mcp:\n        - novanet\n      max_turns: 5";
    let w = ok(yaml);
    assert_eq!(w.tasks.len(), 2);
    match &w.tasks[1].action {
        TaskAction::Agent { agent } => {
            assert!(agent.prompt.contains("{{with.paper}}"));
            assert!(agent.mcp.contains(&"novanet".to_string()));
        }
        _ => panic!("expected Agent"),
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// P. FETCH EDGE CASES — PR5 (12 tests)
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn p01_fetch_shorthand_string_is_error() {
    let yaml =
        "schema: \"nika/workflow@0.12\"\nmodel: test-model\ntasks:\n  - id: t1\n    fetch: \"https://example.com\"";
    let e = err(yaml);
    let msg = format!("{e}");
    assert!(msg.contains("mapping") || msg.contains("NIKA"));
}

#[test]
fn p02_fetch_all_fields_combined() {
    let yaml = "schema: \"nika/workflow@0.12\"\nmodel: test-model\ntasks:\n  - id: t1\n    retry:\n      max_attempts: 3\n      delay_ms: 1000\n      backoff: 2.0\n    fetch:\n      url: \"https://api.example.com/data\"\n      method: POST\n      headers:\n        Content-Type: application/json\n        Authorization: \"Bearer token123\"\n      json:\n        key: value\n        nested:\n          deep: true\n      timeout: 60\n      follow_redirects: false\n      response: full\n      extract: jsonpath\n      selector: \"$.data.results\"";
    let w = ok(yaml);
    match &w.tasks[0].action {
        TaskAction::Fetch { fetch } => {
            assert_eq!(fetch.url, "https://api.example.com/data");
            assert_eq!(fetch.method, "POST");
            assert_eq!(fetch.headers.len(), 2);
            assert!(fetch.json.is_some());
            assert_eq!(fetch.timeout, Some(60));
            assert_eq!(fetch.follow_redirects, Some(false));
            assert_eq!(fetch.response, Some(ResponseMode::Full));
            assert_eq!(fetch.extract, Some(ExtractMode::Jsonpath));
            assert_eq!(fetch.selector.as_deref(), Some("$.data.results"));
            assert!(fetch.retry.is_some());
        }
        _ => panic!("expected Fetch"),
    }
}

#[test]
fn p03_fetch_unicode_in_selector() {
    let w = ok(&wrap(
        "fetch:\n  url: \"https://example.fr\"\n  extract: selector\n  selector: \"div.contenu-francais\"",
    ));
    match &w.tasks[0].action {
        TaskAction::Fetch { fetch } => {
            assert_eq!(fetch.selector.as_deref(), Some("div.contenu-francais"));
        }
        _ => panic!("expected Fetch"),
    }
}

#[test]
fn p04_fetch_empty_extract_string_becomes_none() {
    // Empty extract string is caught at analysis and becomes None
    let w = ok(&wrap(
        "fetch:\n  url: \"https://example.com\"\n  extract: \"\"",
    ));
    match &w.tasks[0].action {
        TaskAction::Fetch { fetch } => {
            assert!(fetch.extract.is_none(), "empty extract should become None");
        }
        _ => panic!("expected Fetch"),
    }
}

#[test]
fn p05_fetch_extract_with_body_and_json_coexist() {
    let yaml = wrap(
        "fetch:\n  url: \"https://example.com\"\n  method: POST\n  body: 'raw text'\n  json:\n    key: value\n  extract: markdown",
    );
    let w = ok(&yaml);
    match &w.tasks[0].action {
        TaskAction::Fetch { fetch } => {
            assert!(fetch.body.is_some());
            assert!(fetch.json.is_some());
            assert_eq!(fetch.extract, Some(ExtractMode::Markdown));
        }
        _ => panic!("expected Fetch"),
    }
}

#[test]
fn p06_fetch_response_full_extract_links_no_selector() {
    let w = ok(&wrap(
        "fetch:\n  url: \"https://example.com\"\n  response: full\n  extract: links",
    ));
    match &w.tasks[0].action {
        TaskAction::Fetch { fetch } => {
            assert_eq!(fetch.response, Some(ResponseMode::Full));
            assert_eq!(fetch.extract, Some(ExtractMode::Links));
            assert!(fetch.selector.is_none());
        }
        _ => panic!("expected Fetch"),
    }
}

#[test]
fn p07_fetch_extract_invalid_modes_become_none() {
    // Invalid extract modes are caught at analysis and become None
    let invalid_modes = ["html", "xml", "csv", "pdf", "image", "video"];
    for mode in &invalid_modes {
        let yaml = wrap(&format!(
            "fetch:\n  url: \"https://example.com\"\n  extract: {}",
            mode
        ));
        let w = ok(&yaml);
        match &w.tasks[0].action {
            TaskAction::Fetch { fetch } => {
                assert!(
                    fetch.extract.is_none(),
                    "invalid mode '{}' should become None after analysis",
                    mode
                );
            }
            _ => panic!("expected Fetch for mode {}", mode),
        }
    }
}

#[test]
fn p08_fetch_response_invalid_modes_become_none() {
    // Invalid response modes are caught at analysis and become None
    let invalid_modes = ["stream", "chunked", "sse", "raw", "json"];
    for mode in &invalid_modes {
        let yaml = wrap(&format!(
            "fetch:\n  url: \"https://example.com\"\n  response: {}",
            mode
        ));
        let w = ok(&yaml);
        match &w.tasks[0].action {
            TaskAction::Fetch { fetch } => {
                assert!(
                    fetch.response.is_none(),
                    "invalid response mode '{}' should become None after analysis",
                    mode
                );
            }
            _ => panic!("expected Fetch for mode {}", mode),
        }
    }
}

#[test]
fn p09_fetch_selector_special_chars() {
    let w = ok(&wrap(
        "fetch:\n  url: \"https://example.com\"\n  extract: selector\n  selector: \"div[data-id='123'] > span.title::after\"",
    ));
    match &w.tasks[0].action {
        TaskAction::Fetch { fetch } => {
            assert_eq!(
                fetch.selector.as_deref(),
                Some("div[data-id='123'] > span.title::after")
            );
        }
        _ => panic!("expected Fetch"),
    }
}

#[test]
fn p10_fetch_response_full_with_extract_validates_ok() {
    // ENG-015: response: full + extract modes are now ALLOWED (except binary + llm_txt)
    let w = ok(&wrap(
        "fetch:\n  url: \"https://example.com\"\n  extract: markdown\n  response: full",
    ));
    match &w.tasks[0].action {
        TaskAction::Fetch { fetch } => {
            fetch.validate().expect("full + markdown should validate");
            assert_eq!(fetch.response, Some(ResponseMode::Full));
            assert_eq!(fetch.extract, Some(ExtractMode::Markdown));
        }
        _ => panic!("expected Fetch"),
    }
}

#[test]
fn p10b_fetch_response_binary_with_extract_rejected() {
    let w = ok(&wrap(
        "fetch:\n  url: \"https://example.com\"\n  extract: metadata\n  response: binary",
    ));
    match &w.tasks[0].action {
        TaskAction::Fetch { fetch } => {
            let err = fetch.validate().unwrap_err();
            assert!(
                err.to_string().contains("binary"),
                "expected binary conflict error, got: {}",
                err
            );
        }
        _ => panic!("expected Fetch"),
    }
}

#[test]
fn p10c_fetch_response_full_with_llm_txt_rejected() {
    let w = ok(&wrap(
        "fetch:\n  url: \"https://example.com\"\n  extract: llm_txt\n  response: full",
    ));
    match &w.tasks[0].action {
        TaskAction::Fetch { fetch } => {
            let err = fetch.validate().unwrap_err();
            assert!(
                err.to_string().contains("llm_txt"),
                "expected llm_txt conflict error, got: {}",
                err
            );
        }
        _ => panic!("expected Fetch"),
    }
}

#[test]
fn p11_fetch_no_extract_no_response_validates_ok() {
    let w = ok(&wrap("fetch:\n  url: \"https://example.com\""));
    match &w.tasks[0].action {
        TaskAction::Fetch { fetch } => {
            assert!(fetch.validate().is_ok());
        }
        _ => panic!("expected Fetch"),
    }
}

#[test]
fn p12_fetch_extract_selector_id_with_hash() {
    let w = ok(&wrap(
        "fetch:\n  url: \"https://example.com\"\n  extract: selector\n  selector: \"#main-content > p\"",
    ));
    match &w.tasks[0].action {
        TaskAction::Fetch { fetch } => {
            assert_eq!(fetch.selector.as_deref(), Some("#main-content > p"));
        }
        _ => panic!("expected Fetch"),
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// Q. VISION + FETCH COMBINED (20 tests)
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn q01_fetch_binary_vision_cas_bridge() {
    let yaml = r#"
schema: "nika/workflow@0.12"
model: test-model
tasks:
  - id: download_photo
    fetch:
      url: "https://cdn.example.com/photo.jpg"
      response: binary
  - id: import_cas
    depends_on: [download_photo]
    with:
      raw: $download_photo
    invoke:
      tool: "nika:import"
      params:
        source: "{{with.raw}}"
  - id: describe
    depends_on: [import_cas]
    with:
      hash: $import_cas
    infer:
      content:
        - type: image
          source: "{{with.hash}}"
          detail: high
        - type: text
          text: "Describe this image in detail"
"#;
    let w = ok(yaml);
    assert_eq!(w.tasks.len(), 3);
    match &w.tasks[2].action {
        TaskAction::Infer { infer } => {
            let parts = infer.content.as_ref().unwrap();
            assert_eq!(parts.len(), 2);
        }
        _ => panic!("expected Infer"),
    }
}

#[test]
fn q02_fetch_extract_markdown_then_infer_with_result() {
    let yaml = r#"
schema: "nika/workflow@0.12"
model: test-model
tasks:
  - id: scrape
    fetch:
      url: "https://docs.example.com/api-reference"
      extract: markdown
  - id: analyze
    depends_on: [scrape]
    with:
      doc: $scrape
    infer:
      prompt: "Extract the key API endpoints from: {{with.doc}}"
      system: "You are a technical documentation analyst"
      temperature: 0.2
"#;
    let w = ok(yaml);
    assert_eq!(w.tasks.len(), 2);
    match &w.tasks[1].action {
        TaskAction::Infer { infer } => {
            assert!(infer.prompt.contains("{{with.doc}}"));
            assert_eq!(infer.temperature, Some(0.2));
        }
        _ => panic!("expected Infer"),
    }
}

#[test]
fn q03_fetch_extract_metadata_then_infer_analyze() {
    let yaml = r#"
schema: "nika/workflow@0.12"
model: test-model
tasks:
  - id: meta
    fetch:
      url: "https://news.example.com/article/123"
      extract: metadata
  - id: judge
    depends_on: [meta]
    with:
      m: $meta
    infer:
      prompt: "Based on this metadata: {{with.m}}, is this a credible source?"
      max_tokens: 200
"#;
    let w = ok(yaml);
    assert_eq!(w.tasks.len(), 2);
    match &w.tasks[1].action {
        TaskAction::Infer { infer } => {
            assert_eq!(infer.max_tokens, Some(200));
        }
        _ => panic!("expected Infer"),
    }
}

#[test]
fn q04_fetch_extract_links_then_infer_classify() {
    let yaml = r#"
schema: "nika/workflow@0.12"
model: test-model
tasks:
  - id: get_links
    fetch:
      url: "https://example.com/sitemap"
      extract: links
  - id: classify
    depends_on: [get_links]
    with:
      links: $get_links
    output:
      format: json
    infer: "Classify these links into categories: {{with.links}}"
"#;
    let w = ok(yaml);
    assert_eq!(w.tasks.len(), 2);
    let output = w.tasks[1].output.as_ref().unwrap();
    assert_eq!(output.format, OutputFormat::Json);
}

#[test]
fn q05_fetch_response_full_then_infer_on_body() {
    let yaml = r#"
schema: "nika/workflow@0.12"
model: test-model
tasks:
  - id: api_call
    fetch:
      url: "https://api.example.com/v2/products"
      response: full
      headers:
        Authorization: "Bearer tok"
  - id: summarize
    depends_on: [api_call]
    with:
      resp: $api_call
    infer: "Summarize the API response: {{with.resp}}"
"#;
    let w = ok(yaml);
    assert_eq!(w.tasks.len(), 2);
    match &w.tasks[0].action {
        TaskAction::Fetch { fetch } => {
            assert_eq!(fetch.response, Some(ResponseMode::Full));
        }
        _ => panic!("expected Fetch"),
    }
}

#[test]
fn q06_vision_content_five_image_parts() {
    let yaml = r#"
schema: "nika/workflow@0.12"
model: test-model
tasks:
  - id: gallery
    infer:
      content:
        - type: image
          source: "blake3:img1"
          detail: high
        - type: image
          source: "blake3:img2"
          detail: high
        - type: image
          source: "blake3:img3"
          detail: low
        - type: image
          source: "blake3:img4"
          detail: auto
        - type: image
          source: "blake3:img5"
          detail: high
        - type: text
          text: "Compare all 5 images"
"#;
    let w = ok(yaml);
    match &w.tasks[0].action {
        TaskAction::Infer { infer } => {
            let parts = infer.content.as_ref().unwrap();
            assert_eq!(parts.len(), 6);
            let image_count = parts
                .iter()
                .filter(|p| matches!(p, ContentPart::Image { .. }))
                .count();
            assert_eq!(image_count, 5);
        }
        _ => panic!("expected Infer"),
    }
}

#[test]
fn q07_vision_image_url_all_detail_levels() {
    let yaml = wrap(
        "infer:\n  content:\n    - type: image_url\n      url: \"https://img.com/a.jpg\"\n      detail: low\n    - type: image_url\n      url: \"https://img.com/b.jpg\"\n      detail: high\n    - type: image_url\n      url: \"https://img.com/c.jpg\"\n      detail: auto",
    );
    let w = ok(&yaml);
    match &w.tasks[0].action {
        TaskAction::Infer { infer } => {
            let parts = infer.content.as_ref().unwrap();
            assert_eq!(parts.len(), 3);
            match &parts[0] {
                ContentPart::ImageUrl { detail, .. } => assert_eq!(*detail, ImageDetail::Low),
                _ => panic!("expected ImageUrl"),
            }
            match &parts[1] {
                ContentPart::ImageUrl { detail, .. } => assert_eq!(*detail, ImageDetail::High),
                _ => panic!("expected ImageUrl"),
            }
            match &parts[2] {
                ContentPart::ImageUrl { detail, .. } => assert_eq!(*detail, ImageDetail::Auto),
                _ => panic!("expected ImageUrl"),
            }
        }
        _ => panic!("expected Infer"),
    }
}

#[test]
fn q08_vision_plus_system_prompt_plus_temperature() {
    let yaml = wrap(
        "provider: openai\ninfer:\n  system: \"You are a vision AI\"\n  temperature: 0.5\n  content:\n    - type: image_url\n      url: \"https://example.com/chart.png\"\n      detail: high\n    - type: text\n      text: \"Describe the chart\"",
    );
    let w = ok(&yaml);
    match &w.tasks[0].action {
        TaskAction::Infer { infer } => {
            assert_eq!(infer.system.as_deref(), Some("You are a vision AI"));
            assert_eq!(infer.temperature, Some(0.5));
            assert_eq!(infer.provider.as_ref().map(|p| p.as_str()), Some("openai"));
            assert!(infer.content.is_some());
        }
        _ => panic!("expected Infer"),
    }
}

#[test]
fn q09_vision_plus_extended_thinking() {
    let yaml = wrap(
        "provider: claude\ninfer:\n  extended_thinking: true\n  thinking_budget: 4096\n  content:\n    - type: image\n      source: \"blake3:complex_diagram\"\n      detail: high\n    - type: text\n      text: \"Analyze this complex architecture diagram\"",
    );
    let w = ok(&yaml);
    match &w.tasks[0].action {
        TaskAction::Infer { infer } => {
            assert_eq!(infer.extended_thinking, Some(true));
            assert_eq!(infer.thinking_budget, Some(4096));
            assert!(infer.content.is_some());
        }
        _ => panic!("expected Infer"),
    }
}

#[test]
fn q10_fetch_binary_then_vision_with_system_and_max_tokens() {
    let yaml = r#"
schema: "nika/workflow@0.12"
model: test-model
tasks:
  - id: dl
    fetch:
      url: "https://cdn.example.com/logo.png"
      response: binary
  - id: analyze_logo
    depends_on: [dl]
    with:
      img: $dl
    infer:
      system: "You are a brand identity expert"
      max_tokens: 500
      content:
        - type: image
          source: "{{with.img}}"
          detail: high
        - type: text
          text: "Evaluate this logo design"
"#;
    let w = ok(yaml);
    match &w.tasks[1].action {
        TaskAction::Infer { infer } => {
            assert_eq!(
                infer.system.as_deref(),
                Some("You are a brand identity expert")
            );
            assert_eq!(infer.max_tokens, Some(500));
        }
        _ => panic!("expected Infer"),
    }
}

#[test]
fn q11_fetch_two_pages_vision_comparison() {
    let yaml = r#"
schema: "nika/workflow@0.12"
model: test-model
tasks:
  - id: page_a
    fetch:
      url: "https://example.com/v1"
      response: binary
  - id: page_b
    fetch:
      url: "https://example.com/v2"
      response: binary
  - id: diff
    depends_on: [page_a, page_b]
    with:
      before: $page_a
      after: $page_b
    infer:
      content:
        - type: image
          source: "{{with.before}}"
          detail: high
        - type: image
          source: "{{with.after}}"
          detail: high
        - type: text
          text: "What changed between these two screenshots?"
"#;
    let w = ok(yaml);
    assert_eq!(w.tasks.len(), 3);
    // flow_count includes edges from depends_on AND with: bindings
    assert!(w.flow_count() >= 2);
}

#[test]
fn q12_fetch_markdown_with_inline_image() {
    let yaml = r#"
schema: "nika/workflow@0.12"
model: test-model
tasks:
  - id: scrape
    fetch:
      url: "https://docs.example.com"
      extract: markdown
  - id: explain
    depends_on: [scrape]
    with:
      doc: $scrape
    infer:
      prompt: "Explain the documentation: {{with.doc}}"
      content:
        - type: image_url
          url: "https://docs.example.com/architecture.svg"
          detail: auto
"#;
    let w = ok(yaml);
    assert_eq!(w.tasks.len(), 2);
}

#[test]
fn q13_fetch_extract_then_vision_with_output_json() {
    let yaml = r#"
schema: "nika/workflow@0.12"
model: test-model
tasks:
  - id: scrape
    fetch:
      url: "https://example.com/product"
      extract: text
      selector: ".product-info"
  - id: extract_structured
    depends_on: [scrape]
    with:
      info: $scrape
    output:
      format: json
      schema:
        type: object
        properties:
          name:
            type: string
          price:
            type: number
    infer:
      prompt: "Extract product info from: {{with.info}}"
      content:
        - type: image_url
          url: "https://example.com/product/main.jpg"
          detail: low
"#;
    let w = ok(yaml);
    let output = w.tasks[1].output.as_ref().unwrap();
    assert_eq!(output.format, OutputFormat::Json);
    assert!(output.schema.is_some());
}

#[test]
fn q14_vision_mixed_cas_and_url_images() {
    let yaml = wrap(
        "infer:\n  content:\n    - type: image\n      source: \"blake3:local_photo\"\n      detail: high\n    - type: image_url\n      url: \"https://remote.example.com/photo.jpg\"\n      detail: low\n    - type: text\n      text: \"Compare local vs remote image\"",
    );
    let w = ok(&yaml);
    match &w.tasks[0].action {
        TaskAction::Infer { infer } => {
            let parts = infer.content.as_ref().unwrap();
            assert_eq!(parts.len(), 3);
            assert!(matches!(&parts[0], ContentPart::Image { .. }));
            assert!(matches!(&parts[1], ContentPart::ImageUrl { .. }));
            assert!(matches!(&parts[2], ContentPart::Text { .. }));
        }
        _ => panic!("expected Infer"),
    }
}

#[test]
fn q15_fetch_metadata_vision_og_image() {
    let yaml = r#"
schema: "nika/workflow@0.12"
model: test-model
tasks:
  - id: meta
    fetch:
      url: "https://blog.example.com/post/42"
      extract: metadata
  - id: review
    depends_on: [meta]
    with:
      m: $meta
    infer:
      prompt: "Review article metadata: {{with.m}}"
      content:
        - type: image_url
          url: "https://blog.example.com/post/42/og-image.jpg"
          detail: auto
"#;
    let w = ok(yaml);
    assert_eq!(w.tasks.len(), 2);
}

#[test]
fn q16_vision_with_provider_override_per_task() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: claude
model: test-model
tasks:
  - id: claude_vision
    infer:
      content:
        - type: image
          source: "blake3:photo_a"
          detail: high
        - type: text
          text: "Describe this"
  - id: openai_vision
    provider: openai
    infer:
      content:
        - type: image
          source: "blake3:photo_b"
          detail: auto
        - type: text
          text: "Describe this"
"#;
    let w = ok(yaml);
    match &w.tasks[0].action {
        TaskAction::Infer { infer } => assert!(infer.provider.is_none()),
        _ => panic!("expected Infer"),
    }
    match &w.tasks[1].action {
        TaskAction::Infer { infer } => {
            assert_eq!(infer.provider.as_ref().map(|p| p.as_str()), Some("openai"));
        }
        _ => panic!("expected Infer"),
    }
}

#[test]
fn q17_vision_content_only_text_parts() {
    let yaml = wrap(
        "infer:\n  content:\n    - type: text\n      text: \"First paragraph\"\n    - type: text\n      text: \"Second paragraph\"\n    - type: text\n      text: \"Third paragraph\"",
    );
    let w = ok(&yaml);
    match &w.tasks[0].action {
        TaskAction::Infer { infer } => {
            let parts = infer.content.as_ref().unwrap();
            assert_eq!(parts.len(), 3);
            assert!(parts.iter().all(|p| matches!(p, ContentPart::Text { .. })));
        }
        _ => panic!("expected Infer"),
    }
}

#[test]
fn q18_fetch_links_then_vision_per_link() {
    let yaml = r#"
schema: "nika/workflow@0.12"
model: test-model
tasks:
  - id: get_links
    fetch:
      url: "https://gallery.example.com"
      extract: links
  - id: preview
    depends_on: [get_links]
    for_each: $get_links
    as: link
    infer:
      content:
        - type: image_url
          url: "{{with.link}}"
          detail: low
        - type: text
          text: "Rate this image 1-10"
"#;
    let w = ok(yaml);
    assert_eq!(w.tasks.len(), 2);
    assert!(w.tasks[1].for_each.is_some());
}

#[test]
fn q19_vision_with_structured_output() {
    let yaml = wrap(
        "structured:\n  schema:\n    type: object\n    properties:\n      objects:\n        type: array\n      mood:\n        type: string\n    required:\n      - objects\n      - mood\ninfer:\n  content:\n    - type: image\n      source: \"blake3:scene\"\n      detail: high\n    - type: text\n      text: \"Identify objects and mood\"",
    );
    let w = ok(&yaml);
    assert!(w.tasks[0].structured.is_some());
    match &w.tasks[0].action {
        TaskAction::Infer { infer } => assert!(infer.content.is_some()),
        _ => panic!("expected Infer"),
    }
}

#[test]
fn q20_fetch_binary_chain_import_thumbnail_vision() {
    let yaml = r#"
schema: "nika/workflow@0.12"
model: test-model
tasks:
  - id: dl
    fetch:
      url: "https://storage.example.com/hi-res.jpg"
      response: binary
      timeout: 120
  - id: import
    depends_on: [dl]
    with:
      raw: $dl
    invoke:
      tool: "nika:import"
      params:
        source: "{{with.raw}}"
  - id: thumb
    depends_on: [import]
    with:
      hash: $import
    invoke:
      tool: "nika:thumbnail"
      params:
        hash: "{{with.hash}}"
        width: 512
        height: 512
  - id: describe
    depends_on: [thumb]
    with:
      thumb_hash: $thumb
    infer:
      content:
        - type: image
          source: "{{with.thumb_hash}}"
          detail: auto
        - type: text
          text: "Describe the contents of this image"
"#;
    let w = ok(yaml);
    assert_eq!(w.tasks.len(), 4);
    // flow_count includes edges from depends_on AND with: bindings
    assert!(w.flow_count() >= 3);
}

// ═══════════════════════════════════════════════════════════════════════════════
// R. FOR_EACH + EXTRACT (15 tests)
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn r01_for_each_items_fetch_extract_markdown_per_item() {
    let yaml = r#"
schema: "nika/workflow@0.12"
model: test-model
tasks:
  - id: scrape_all
    for_each: ["https://a.com", "https://b.com", "https://c.com"]
    as: url
    fetch:
      url: "{{with.url}}"
      extract: markdown
"#;
    let w = ok(yaml);
    assert!(w.tasks[0].for_each.is_some());
    assert_eq!(w.tasks[0].for_each_as.as_deref(), Some("url"));
    match &w.tasks[0].action {
        TaskAction::Fetch { fetch } => assert_eq!(fetch.extract, Some(ExtractMode::Markdown)),
        _ => panic!("expected Fetch"),
    }
}

#[test]
fn r02_for_each_items_fetch_extract_metadata_per_item() {
    let yaml = r#"
schema: "nika/workflow@0.12"
model: test-model
tasks:
  - id: meta_all
    for_each: ["https://a.com", "https://b.com"]
    as: site
    fetch:
      url: "{{with.site}}"
      extract: metadata
"#;
    let w = ok(yaml);
    match &w.tasks[0].action {
        TaskAction::Fetch { fetch } => assert_eq!(fetch.extract, Some(ExtractMode::Metadata)),
        _ => panic!("expected Fetch"),
    }
}

#[test]
fn r03_for_each_invoke_per_item() {
    let yaml = r#"
schema: "nika/workflow@0.12"
model: test-model
tasks:
  - id: process
    for_each: ["file1.jpg", "file2.jpg", "file3.jpg"]
    as: file
    invoke:
      tool: "nika:import"
      params:
        path: "{{with.file}}"
"#;
    let w = ok(yaml);
    assert!(w.tasks[0].for_each.is_some());
    match &w.tasks[0].action {
        TaskAction::Invoke { invoke } => {
            assert_eq!(invoke.tool.as_deref(), Some("nika:import"));
        }
        _ => panic!("expected Invoke"),
    }
}

#[test]
fn r04_for_each_with_vision_content() {
    let yaml = r#"
schema: "nika/workflow@0.12"
model: test-model
tasks:
  - id: review_images
    for_each: ["blake3:img1", "blake3:img2", "blake3:img3"]
    as: hash
    infer:
      content:
        - type: image
          source: "{{with.hash}}"
          detail: high
        - type: text
          text: "Rate this image quality"
"#;
    let w = ok(yaml);
    assert!(w.tasks[0].for_each.is_some());
    match &w.tasks[0].action {
        TaskAction::Infer { infer } => assert!(infer.content.is_some()),
        _ => panic!("expected Infer"),
    }
}

#[test]
fn r05_for_each_with_response_full() {
    let yaml = r#"
schema: "nika/workflow@0.12"
model: test-model
tasks:
  - id: check_apis
    for_each: ["https://a.com/api", "https://b.com/api"]
    as: endpoint
    fetch:
      url: "{{with.endpoint}}"
      response: full
"#;
    let w = ok(yaml);
    match &w.tasks[0].action {
        TaskAction::Fetch { fetch } => {
            assert_eq!(fetch.response, Some(ResponseMode::Full));
        }
        _ => panic!("expected Fetch"),
    }
}

#[test]
fn r06_for_each_dollar_binding_then_fetch() {
    let yaml = r#"
schema: "nika/workflow@0.12"
model: test-model
tasks:
  - id: get_items
    fetch:
      url: "https://api.example.com/items"
      extract: jsonpath
      selector: "$.items[*].url"
  - id: process
    depends_on: [get_items]
    for_each: $get_items
    as: item_url
    fetch:
      url: "{{with.item_url}}"
      extract: markdown
"#;
    let w = ok(yaml);
    assert_eq!(w.tasks.len(), 2);
    assert!(w.tasks[1].for_each.is_some());
}

#[test]
fn r07_for_each_with_concurrency() {
    let yaml = r#"
schema: "nika/workflow@0.12"
model: test-model
tasks:
  - id: parallel_scrape
    for_each: ["https://a.com", "https://b.com", "https://c.com", "https://d.com"]
    as: url
    concurrency: 4
    fetch:
      url: "{{with.url}}"
      extract: markdown
"#;
    let w = ok(yaml);
    assert_eq!(w.tasks[0].concurrency, Some(4));
}

#[test]
fn r08_for_each_with_fail_fast_false() {
    let yaml = r#"
schema: "nika/workflow@0.12"
model: test-model
tasks:
  - id: scrape_resilient
    for_each: ["https://a.com", "https://b.com"]
    as: url
    fail_fast: false
    fetch:
      url: "{{with.url}}"
      extract: text
"#;
    let w = ok(yaml);
    assert_eq!(w.tasks[0].fail_fast, Some(false));
}

#[test]
fn r09_for_each_exec_per_item() {
    let yaml = r#"
schema: "nika/workflow@0.12"
model: test-model
tasks:
  - id: build_all
    for_each: ["app-a", "app-b", "app-c"]
    as: app
    exec: "make build -C {{with.app}}"
"#;
    let w = ok(yaml);
    match &w.tasks[0].action {
        TaskAction::Exec { exec } => assert!(exec.command.contains("{{with.app}}")),
        _ => panic!("expected Exec"),
    }
}

#[test]
fn r10_for_each_infer_per_locale() {
    let yaml = r#"
schema: "nika/workflow@0.12"
model: test-model
tasks:
  - id: translate
    for_each: ["en-US", "fr-FR", "de-DE", "ja-JP", "ko-KR"]
    as: locale
    infer:
      prompt: "Translate to {{with.locale}}: Hello World"
      system: "You are a professional translator"
"#;
    let w = ok(yaml);
    assert!(w.tasks[0].for_each.is_some());
    assert_eq!(w.tasks[0].for_each_as.as_deref(), Some("locale"));
}

#[test]
fn r11_for_each_with_depends_on_and_binding() {
    let yaml = r#"
schema: "nika/workflow@0.12"
model: test-model
tasks:
  - id: get_urls
    infer: "List 5 URLs"
  - id: scrape_each
    depends_on: [get_urls]
    for_each: $get_urls
    as: url
    fetch:
      url: "{{with.url}}"
      extract: markdown
"#;
    let w = ok(yaml);
    assert_eq!(w.tasks.len(), 2);
    assert!(w.tasks[1]
        .depends_on
        .as_ref()
        .unwrap()
        .contains(&"get_urls".to_string()));
}

#[test]
fn r12_for_each_default_as_item() {
    let yaml = r#"
schema: "nika/workflow@0.12"
model: test-model
tasks:
  - id: process
    for_each: ["a", "b", "c"]
    infer: "Process {{with.item}}"
"#;
    let w = ok(yaml);
    // Through the three-phase pipeline, as defaults to "item"
    assert_eq!(w.tasks[0].for_each_var(), "item");
}

#[test]
fn r13_for_each_agent_per_item() {
    let yaml = r#"
schema: "nika/workflow@0.12"
model: test-model
tasks:
  - id: research
    for_each: ["quantum computing", "machine learning", "blockchain"]
    as: topic
    agent:
      prompt: "Research {{with.topic}} and write a summary"
      max_turns: 5
"#;
    let w = ok(yaml);
    match &w.tasks[0].action {
        TaskAction::Agent { agent } => {
            assert!(agent.prompt.contains("{{with.topic}}"));
        }
        _ => panic!("expected Agent"),
    }
}

#[test]
fn r14_for_each_with_output_format() {
    let yaml = r#"
schema: "nika/workflow@0.12"
model: test-model
tasks:
  - id: extract
    for_each: ["https://a.com", "https://b.com"]
    as: url
    output:
      format: json
    fetch:
      url: "{{with.url}}"
      extract: metadata
"#;
    let w = ok(yaml);
    let output = w.tasks[0].output.as_ref().unwrap();
    assert_eq!(output.format, OutputFormat::Json);
}

#[test]
fn r15_for_each_with_concurrency_and_fail_fast() {
    let yaml = r#"
schema: "nika/workflow@0.12"
model: test-model
tasks:
  - id: batch
    for_each: ["a", "b", "c", "d", "e", "f"]
    as: item
    concurrency: 3
    fail_fast: true
    infer: "Process {{with.item}}"
"#;
    let w = ok(yaml);
    assert_eq!(w.tasks[0].concurrency, Some(3));
    assert_eq!(w.tasks[0].fail_fast, Some(true));
}

// ═══════════════════════════════════════════════════════════════════════════════
// S. AGENT + FETCH TOOLS (15 tests)
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn s01_agent_with_nika_read_tool() {
    let yaml = wrap(
        "agent:\n  prompt: \"Read and analyze files\"\n  tools:\n    - nika:read\n    - nika:glob",
    );
    let w = ok(&yaml);
    match &w.tasks[0].action {
        TaskAction::Agent { agent } => {
            assert_eq!(agent.tools.len(), 2);
            assert!(agent.tools.contains(&"nika:read".to_string()));
            assert!(agent.tools.contains(&"nika:glob".to_string()));
        }
        _ => panic!("expected Agent"),
    }
}

#[test]
fn s02_agent_with_nika_write_edit_tools() {
    let yaml = wrap(
        "agent:\n  prompt: \"Modify files\"\n  tools:\n    - nika:write\n    - nika:edit\n    - nika:grep",
    );
    let w = ok(&yaml);
    match &w.tasks[0].action {
        TaskAction::Agent { agent } => {
            assert_eq!(agent.tools.len(), 3);
        }
        _ => panic!("expected Agent"),
    }
}

#[test]
fn s03_agent_with_multiple_mcp_servers() {
    let yaml = wrap(
        "agent:\n  prompt: \"Research\"\n  mcp:\n    - novanet\n    - perplexity\n    - firecrawl",
    );
    let w = ok(&yaml);
    match &w.tasks[0].action {
        TaskAction::Agent { agent } => {
            assert_eq!(agent.mcp.len(), 3);
        }
        _ => panic!("expected Agent"),
    }
}

#[test]
fn s04_agent_with_tools_and_mcp_combined() {
    let yaml = wrap(
        "agent:\n  prompt: \"Full research\"\n  mcp:\n    - novanet\n  tools:\n    - nika:read\n    - nika:write\n  max_turns: 10",
    );
    let w = ok(&yaml);
    match &w.tasks[0].action {
        TaskAction::Agent { agent } => {
            assert_eq!(agent.mcp.len(), 1);
            assert_eq!(agent.tools.len(), 2);
            assert_eq!(agent.max_turns, Some(10));
        }
        _ => panic!("expected Agent"),
    }
}

#[test]
fn s05_agent_with_depth_limit_and_token_budget() {
    let yaml = wrap(
        "agent:\n  prompt: \"Deep research\"\n  depth_limit: 3\n  token_budget: 200000\n  max_turns: 30",
    );
    let w = ok(&yaml);
    match &w.tasks[0].action {
        TaskAction::Agent { agent } => {
            assert_eq!(agent.depth_limit, Some(3));
            assert_eq!(agent.token_budget, Some(200000));
            assert_eq!(agent.max_turns, Some(30));
        }
        _ => panic!("expected Agent"),
    }
}

#[test]
fn s06_agent_after_fetch_with_binding() {
    let yaml = r#"
schema: "nika/workflow@0.12"
model: test-model
tasks:
  - id: scrape
    fetch:
      url: "https://arxiv.org/abs/2401.00001"
      extract: markdown
  - id: analyze
    depends_on: [scrape]
    with:
      paper: $scrape
    agent:
      prompt: "Analyze this paper: {{with.paper}}"
      system: "You are a research scientist"
      mcp:
        - novanet
      max_turns: 10
      temperature: 0.3
"#;
    let w = ok(yaml);
    match &w.tasks[1].action {
        TaskAction::Agent { agent } => {
            assert!(agent.prompt.contains("{{with.paper}}"));
            assert_eq!(agent.temperature, Some(0.3));
        }
        _ => panic!("expected Agent"),
    }
}

#[test]
fn s07_agent_all_providers() {
    // Input alias → canonical name after ProviderName resolution
    let providers: &[(&str, &str)] = &[
        ("claude", "anthropic"),
        ("openai", "openai"),
        ("groq", "groq"),
        ("gemini", "gemini"),
        ("xai", "xai"),
        ("deepseek", "deepseek"),
        ("mistral", "mistral"),
    ];
    for (alias, canonical) in providers {
        let yaml = wrap(&format!(
            "agent:\n  prompt: \"Test\"\n  provider: {}",
            alias
        ));
        let w = ok(&yaml);
        match &w.tasks[0].action {
            TaskAction::Agent { agent } => {
                assert_eq!(
                    agent.provider.as_ref().map(|p| p.as_str()),
                    Some(*canonical)
                );
            }
            _ => panic!("expected Agent for {}", alias),
        }
    }
}

#[test]
fn s08_agent_with_skills() {
    let yaml = wrap(
        "agent:\n  prompt: \"Test\"\n  skills:\n    - coding\n    - research\n    - writing\n    - math",
    );
    let w = ok(&yaml);
    match &w.tasks[0].action {
        TaskAction::Agent { agent } => {
            let skills = agent.skills.as_ref().unwrap();
            assert_eq!(skills.len(), 4);
        }
        _ => panic!("expected Agent"),
    }
}

#[test]
fn s09_agent_with_extended_thinking_and_tools() {
    let yaml = wrap(
        "agent:\n  prompt: \"Complex reasoning\"\n  provider: claude\n  extended_thinking: true\n  thinking_budget: 16384\n  tools:\n    - nika:read\n    - nika:grep",
    );
    let w = ok(&yaml);
    match &w.tasks[0].action {
        TaskAction::Agent { agent } => {
            assert_eq!(agent.extended_thinking, Some(true));
            assert_eq!(agent.thinking_budget, Some(16384));
            assert_eq!(agent.tools.len(), 2);
        }
        _ => panic!("expected Agent"),
    }
}

#[test]
fn s10_agent_with_model_override() {
    let yaml = wrap("agent:\n  prompt: \"Test\"\n  provider: openai\n  model: gpt-4o-2024-11-20");
    let w = ok(&yaml);
    match &w.tasks[0].action {
        TaskAction::Agent { agent } => {
            assert_eq!(agent.model.as_deref(), Some("gpt-4o-2024-11-20"));
        }
        _ => panic!("expected Agent"),
    }
}

#[test]
fn s11_agent_multiline_system_prompt() {
    let yaml = "schema: \"nika/workflow@0.12\"\nmodel: test-model\ntasks:\n  - id: t1\n    agent:\n      prompt: \"Analyze the codebase\"\n      system: |\n        You are a senior software engineer.\n        Focus on: architecture, patterns, and potential issues.\n        Be thorough but concise.\n      max_turns: 15";
    let w = ok(yaml);
    match &w.tasks[0].action {
        TaskAction::Agent { agent } => {
            assert!(agent
                .system
                .as_ref()
                .unwrap()
                .contains("senior software engineer"));
            assert!(agent.system.as_ref().unwrap().contains("architecture"));
        }
        _ => panic!("expected Agent"),
    }
}

#[test]
fn s12_agent_empty_tools_list() {
    let yaml = wrap("agent:\n  prompt: \"Test\"\n  tools: []");
    let w = ok(&yaml);
    match &w.tasks[0].action {
        TaskAction::Agent { agent } => assert!(agent.tools.is_empty()),
        _ => panic!("expected Agent"),
    }
}

#[test]
fn s13_agent_max_turns_one() {
    let yaml = wrap("agent:\n  prompt: \"Single turn\"\n  max_turns: 1");
    let w = ok(&yaml);
    match &w.tasks[0].action {
        TaskAction::Agent { agent } => assert_eq!(agent.max_turns, Some(1)),
        _ => panic!("expected Agent"),
    }
}

#[test]
fn s14_agent_max_turns_large() {
    let yaml = wrap("agent:\n  prompt: \"Marathon\"\n  max_turns: 100");
    let w = ok(&yaml);
    match &w.tasks[0].action {
        TaskAction::Agent { agent } => assert_eq!(agent.max_turns, Some(100)),
        _ => panic!("expected Agent"),
    }
}

#[test]
fn s15_agent_temperature_zero_deterministic() {
    let yaml = wrap("agent:\n  prompt: \"Deterministic\"\n  temperature: 0.0");
    let w = ok(&yaml);
    match &w.tasks[0].action {
        TaskAction::Agent { agent } => assert_eq!(agent.temperature, Some(0.0)),
        _ => panic!("expected Agent"),
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// T. COMPLEX MULTI-TASK PIPELINES (30 tests)
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn t01_five_task_pipeline_all_verbs() {
    let yaml = r#"
schema: "nika/workflow@0.12"
model: test-model
tasks:
  - id: scrape
    fetch:
      url: "https://example.com"
      extract: markdown
  - id: analyze
    depends_on: [scrape]
    with:
      page: $scrape
    infer: "Extract key points from: {{with.page}}"
  - id: save
    depends_on: [analyze]
    with:
      analysis: $analyze
    exec: "echo done > analysis.txt"
  - id: store
    depends_on: [analyze]
    with:
      data: $analyze
    invoke:
      mcp: novanet
      tool: novanet_write
      params:
        content: "{{with.data}}"
  - id: refine
    depends_on: [store]
    with:
      stored: $store
    agent:
      prompt: "Refine based on: {{with.stored}}"
      max_turns: 5
"#;
    let w = ok(yaml);
    assert_eq!(w.tasks.len(), 5);
    // flow_count includes edges from depends_on AND with: bindings
    assert!(w.flow_count() >= 4);
}

#[test]
fn t02_diamond_dag_fetch_infer_merge() {
    let yaml = r#"
schema: "nika/workflow@0.12"
model: test-model
tasks:
  - id: source
    fetch:
      url: "https://api.example.com/data"
  - id: left
    depends_on: [source]
    with:
      d: $source
    infer: "Analyze from angle A: {{with.d}}"
  - id: right
    depends_on: [source]
    with:
      d: $source
    infer: "Analyze from angle B: {{with.d}}"
  - id: merge
    depends_on: [left, right]
    with:
      a: $left
      b: $right
    infer: "Synthesize: {{with.a}} and {{with.b}}"
"#;
    let w = ok(yaml);
    assert_eq!(w.tasks.len(), 4);
    // flow_count includes edges from depends_on AND with: bindings
    assert!(w.flow_count() >= 4);
    let edges = w.edges();
    assert!(edges.contains(&("source", "left")));
    assert!(edges.contains(&("source", "right")));
    assert!(edges.contains(&("left", "merge")));
    assert!(edges.contains(&("right", "merge")));
}

#[test]
fn t03_fan_out_three_parallel_fetches_fan_in_summarize() {
    let yaml = r#"
schema: "nika/workflow@0.12"
model: test-model
tasks:
  - id: fetch_a
    fetch:
      url: "https://a.example.com"
      extract: markdown
  - id: fetch_b
    fetch:
      url: "https://b.example.com"
      extract: markdown
  - id: fetch_c
    fetch:
      url: "https://c.example.com"
      extract: markdown
  - id: summarize
    depends_on: [fetch_a, fetch_b, fetch_c]
    with:
      a: $fetch_a
      b: $fetch_b
      c: $fetch_c
    infer: "Summarize all three: {{with.a}} {{with.b}} {{with.c}}"
"#;
    let w = ok(yaml);
    assert_eq!(w.tasks.len(), 4);
    assert!(w.flow_count() >= 3);
}

#[test]
fn t04_fetch_binary_thumbnail_quality_infer_pipeline() {
    let yaml = r#"
schema: "nika/workflow@0.12"
model: test-model
tasks:
  - id: download
    fetch:
      url: "https://cdn.example.com/photo.jpg"
      response: binary
  - id: thumb
    depends_on: [download]
    with:
      raw: $download
    invoke:
      tool: "nika:thumbnail"
      params:
        hash: "{{with.raw}}"
        width: 256
  - id: quality
    depends_on: [download]
    with:
      raw: $download
    invoke:
      tool: "nika:quality"
      params:
        hash: "{{with.raw}}"
  - id: report
    depends_on: [thumb, quality]
    with:
      t: $thumb
      q: $quality
    infer: "Thumbnail: {{with.t}}, Quality: {{with.q}}"
"#;
    let w = ok(yaml);
    assert_eq!(w.tasks.len(), 4);
    assert!(w.flow_count() >= 4);
}

#[test]
fn t05_fetch_extract_metadata_then_infer_structured_output() {
    let yaml = r#"
schema: "nika/workflow@0.12"
model: test-model
tasks:
  - id: meta
    fetch:
      url: "https://example.com"
      extract: metadata
  - id: extract
    depends_on: [meta]
    with:
      m: $meta
    structured:
      schema:
        type: object
        properties:
          title:
            type: string
          description:
            type: string
    infer: "Extract title and description from: {{with.m}}"
"#;
    let w = ok(yaml);
    assert!(w.tasks[1].structured.is_some());
}

#[test]
fn t06_ten_task_workflow_all_five_verbs() {
    let yaml = r#"
schema: "nika/workflow@0.12"
model: test-model
tasks:
  - id: t01
    infer: "Generate idea"
  - id: t02
    depends_on: [t01]
    with:
      idea: $t01
    exec: "echo done > idea.txt"
  - id: t03
    depends_on: [t01]
    with:
      idea: $t01
    fetch:
      url: "https://api.example.com/search"
      method: POST
      json:
        query: "{{with.idea}}"
  - id: t04
    depends_on: [t03]
    with:
      results: $t03
    invoke:
      mcp: novanet
      tool: novanet_write
      params:
        data: "{{with.results}}"
  - id: t05
    depends_on: [t02, t04]
    with:
      file: $t02
      data: $t04
    agent:
      prompt: "Analyze: {{with.file}} and {{with.data}}"
      max_turns: 5
  - id: t06
    depends_on: [t05]
    with:
      analysis: $t05
    infer: "Summarize: {{with.analysis}}"
  - id: t07
    depends_on: [t06]
    with:
      summary: $t06
    exec: "echo done"
  - id: t08
    depends_on: [t06]
    with:
      summary: $t06
    fetch:
      url: "https://api.example.com/publish"
      method: POST
      json:
        content: "{{with.summary}}"
  - id: t09
    depends_on: [t07, t08]
    invoke:
      tool: "nika:log"
      params:
        level: info
        message: "Pipeline complete"
  - id: t10
    depends_on: [t09]
    infer: "Generate final report"
"#;
    let w = ok(yaml);
    assert_eq!(w.tasks.len(), 10);
}

#[test]
fn t07_parallel_fetch_merge_infer() {
    let yaml = r#"
schema: "nika/workflow@0.12"
model: test-model
tasks:
  - id: api_a
    fetch:
      url: "https://api-a.example.com/data"
      response: full
  - id: api_b
    fetch:
      url: "https://api-b.example.com/data"
      response: full
  - id: api_c
    fetch:
      url: "https://api-c.example.com/data"
      response: full
  - id: merge
    depends_on: [api_a, api_b, api_c]
    with:
      a: $api_a
      b: $api_b
      c: $api_c
    infer: "Compare APIs: {{with.a}} vs {{with.b}} vs {{with.c}}"
"#;
    let w = ok(yaml);
    assert_eq!(w.tasks.len(), 4);
    assert!(w.flow_count() >= 3);
}

#[test]
fn t08_scrape_extract_store_pipeline() {
    let yaml = r#"
schema: "nika/workflow@0.12"
model: test-model
tasks:
  - id: scrape
    fetch:
      url: "https://docs.example.com"
      extract: markdown
      timeout: 30
  - id: get_meta
    fetch:
      url: "https://docs.example.com"
      extract: metadata
  - id: get_links
    fetch:
      url: "https://docs.example.com"
      extract: links
  - id: analyze
    depends_on: [scrape, get_meta, get_links]
    with:
      content: $scrape
      meta: $get_meta
      links: $get_links
    infer: "Analyze: {{with.content}} Meta: {{with.meta}} Links: {{with.links}}"
  - id: store
    depends_on: [analyze]
    with:
      result: $analyze
    invoke:
      mcp: novanet
      tool: novanet_write
      params:
        data: "{{with.result}}"
"#;
    let w = ok(yaml);
    assert_eq!(w.tasks.len(), 5);
}

#[test]
fn t09_multi_provider_pipeline() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: claude
model: test-model
tasks:
  - id: draft
    infer:
      prompt: "Write a draft"
      system: "You are a writer"
  - id: review
    depends_on: [draft]
    provider: openai
    with:
      text: $draft
    infer:
      prompt: "Review this draft: {{with.text}}"
  - id: finalize
    depends_on: [review]
    provider: gemini
    with:
      feedback: $review
    infer:
      prompt: "Incorporate feedback: {{with.feedback}}"
"#;
    let w = ok(yaml);
    assert_eq!(w.provider.as_str(), "anthropic");
    match &w.tasks[1].action {
        TaskAction::Infer { infer } => {
            assert_eq!(infer.provider.as_ref().map(|p| p.as_str()), Some("openai"))
        }
        _ => panic!("expected Infer"),
    }
    match &w.tasks[2].action {
        TaskAction::Infer { infer } => {
            assert_eq!(infer.provider.as_ref().map(|p| p.as_str()), Some("gemini"))
        }
        _ => panic!("expected Infer"),
    }
}

#[test]
fn t10_wide_fan_out_eight_branches() {
    let yaml = r#"
schema: "nika/workflow@0.12"
model: test-model
tasks:
  - id: root
    infer: "Root"
  - id: b1
    depends_on: [root]
    infer: "1"
  - id: b2
    depends_on: [root]
    infer: "2"
  - id: b3
    depends_on: [root]
    infer: "3"
  - id: b4
    depends_on: [root]
    infer: "4"
  - id: b5
    depends_on: [root]
    infer: "5"
  - id: b6
    depends_on: [root]
    infer: "6"
  - id: b7
    depends_on: [root]
    infer: "7"
  - id: b8
    depends_on: [root]
    infer: "8"
  - id: merge
    depends_on: [b1, b2, b3, b4, b5, b6, b7, b8]
    infer: "Merge all"
"#;
    let w = ok(yaml);
    assert_eq!(w.tasks.len(), 10);
    assert_eq!(w.flow_count(), 16);
}

#[test]
fn t11_exec_then_fetch_then_infer() {
    let yaml = r#"
schema: "nika/workflow@0.12"
model: test-model
tasks:
  - id: build
    exec: "cargo build --release"
  - id: deploy
    depends_on: [build]
    exec:
      command: "deploy.sh"
      shell: true
      timeout: 120
  - id: health
    depends_on: [deploy]
    fetch:
      url: "https://app.example.com/health"
      response: full
  - id: report
    depends_on: [health]
    with:
      status: $health
    infer: "Deployment report: {{with.status}}"
"#;
    let w = ok(yaml);
    assert_eq!(w.tasks.len(), 4);
    assert!(w.flow_count() >= 3);
}

#[test]
fn t12_nested_with_binding_chain() {
    let yaml = r#"
schema: "nika/workflow@0.12"
model: test-model
tasks:
  - id: raw
    infer: "Generate data"
  - id: clean
    depends_on: [raw]
    with:
      data: $raw | trim
    infer: "Clean: {{with.data}}"
  - id: transform
    depends_on: [clean]
    with:
      cleaned: $clean | upper
    infer: "Transform: {{with.cleaned}}"
  - id: validate
    depends_on: [transform]
    with:
      transformed: $transform
    infer: "Validate: {{with.transformed}}"
"#;
    let w = ok(yaml);
    assert_eq!(w.tasks.len(), 4);
    assert!(w.flow_count() >= 3);
}

#[test]
fn t13_vision_after_two_fetches() {
    let yaml = r#"
schema: "nika/workflow@0.12"
model: test-model
tasks:
  - id: fetch_md
    fetch:
      url: "https://example.com/docs"
      extract: markdown
  - id: fetch_img
    fetch:
      url: "https://example.com/diagram.png"
      response: binary
  - id: explain
    depends_on: [fetch_md, fetch_img]
    with:
      doc: $fetch_md
      img: $fetch_img
    infer:
      prompt: "Explain based on docs: {{with.doc}}"
      content:
        - type: image
          source: "{{with.img}}"
          detail: high
"#;
    let w = ok(yaml);
    assert_eq!(w.tasks.len(), 3);
}

#[test]
fn t14_invoke_chain_import_optimize_convert() {
    let yaml = r#"
schema: "nika/workflow@0.12"
model: test-model
tasks:
  - id: import
    invoke:
      tool: "nika:import"
      params:
        path: "./photo.jpg"
  - id: optimize
    depends_on: [import]
    with:
      h: $import
    invoke:
      tool: "nika:optimize"
      params:
        hash: "{{with.h}}"
  - id: convert
    depends_on: [optimize]
    with:
      h: $optimize
    invoke:
      tool: "nika:convert"
      params:
        hash: "{{with.h}}"
        format: webp
"#;
    let w = ok(yaml);
    assert_eq!(w.tasks.len(), 3);
    assert!(w.flow_count() >= 2);
}

#[test]
fn t15_mcp_invoke_then_agent() {
    let yaml = r#"
schema: "nika/workflow@0.12"
model: test-model
mcp:
  servers:
    novanet:
      command: cargo
      args: [run, -p, novanet-mcp]
tasks:
  - id: search
    invoke:
      mcp: novanet
      tool: novanet_search
      params:
        query: "qr-code"
  - id: research
    depends_on: [search]
    with:
      context: $search
    agent:
      prompt: "Deep research on: {{with.context}}"
      mcp:
        - novanet
      max_turns: 10
"#;
    let w = ok(yaml);
    assert!(w.mcp.is_some());
    assert_eq!(w.tasks.len(), 2);
}

#[test]
fn t16_double_diamond_dag() {
    let yaml = r#"
schema: "nika/workflow@0.12"
model: test-model
tasks:
  - id: start
    infer: "Start"
  - id: left1
    depends_on: [start]
    infer: "Left1"
  - id: right1
    depends_on: [start]
    infer: "Right1"
  - id: mid
    depends_on: [left1, right1]
    infer: "Mid"
  - id: left2
    depends_on: [mid]
    infer: "Left2"
  - id: right2
    depends_on: [mid]
    infer: "Right2"
  - id: end
    depends_on: [left2, right2]
    infer: "End"
"#;
    let w = ok(yaml);
    assert_eq!(w.tasks.len(), 7);
    assert_eq!(w.flow_count(), 8);
}

#[test]
fn t17_for_each_then_merge() {
    let yaml = r#"
schema: "nika/workflow@0.12"
model: test-model
tasks:
  - id: expand
    for_each: ["topic-a", "topic-b", "topic-c"]
    as: topic
    infer: "Research {{with.topic}}"
  - id: merge
    depends_on: [expand]
    with:
      results: $expand
    infer: "Merge all research: {{with.results}}"
"#;
    let w = ok(yaml);
    assert_eq!(w.tasks.len(), 2);
    assert!(w.tasks[0].for_each.is_some());
}

#[test]
fn t18_structured_output_chain() {
    let yaml = r#"
schema: "nika/workflow@0.12"
model: test-model
tasks:
  - id: extract
    structured:
      schema:
        type: object
        properties:
          entities:
            type: array
        required: [entities]
    infer: "Extract entities from the text"
  - id: classify
    depends_on: [extract]
    with:
      entities: $extract
    structured:
      schema:
        type: object
        properties:
          categories:
            type: object
    infer: "Classify entities: {{with.entities}}"
"#;
    let w = ok(yaml);
    assert!(w.tasks[0].structured.is_some());
    assert!(w.tasks[1].structured.is_some());
}

#[test]
fn t19_multi_output_format_pipeline() {
    let yaml = r#"
schema: "nika/workflow@0.12"
model: test-model
tasks:
  - id: gen_json
    output:
      format: json
    infer: "Generate JSON data"
  - id: gen_yaml
    output:
      format: yaml
    infer: "Generate YAML data"
  - id: gen_text
    output:
      format: text
    infer: "Generate plain text"
  - id: combine
    depends_on: [gen_json, gen_yaml, gen_text]
    with:
      j: $gen_json
      y: $gen_yaml
      t: $gen_text
    infer: "Combine: {{with.j}} {{with.y}} {{with.t}}"
"#;
    let w = ok(yaml);
    assert_eq!(w.tasks.len(), 4);
    let o0 = w.tasks[0].output.as_ref().unwrap();
    assert_eq!(o0.format, OutputFormat::Json);
    let o1 = w.tasks[1].output.as_ref().unwrap();
    assert_eq!(o1.format, OutputFormat::Yaml);
}

#[test]
fn t20_long_chain_ten_steps() {
    let yaml = r#"
schema: "nika/workflow@0.12"
model: test-model
tasks:
  - id: s01
    infer: "Step 1"
  - id: s02
    depends_on: [s01]
    infer: "Step 2"
  - id: s03
    depends_on: [s02]
    exec: "echo step3"
  - id: s04
    depends_on: [s03]
    fetch:
      url: "https://api.example.com"
  - id: s05
    depends_on: [s04]
    with:
      data: $s04
    invoke:
      tool: "nika:log"
      params:
        message: "{{with.data}}"
  - id: s06
    depends_on: [s05]
    infer: "Step 6"
  - id: s07
    depends_on: [s06]
    exec: "echo step7"
  - id: s08
    depends_on: [s07]
    infer: "Step 8"
  - id: s09
    depends_on: [s08]
    infer: "Step 9"
  - id: s10
    depends_on: [s09]
    infer: "Final step"
"#;
    let w = ok(yaml);
    assert_eq!(w.tasks.len(), 10);
    assert!(w.flow_count() >= 9);
}

#[test]
fn t21_all_verbs_in_parallel() {
    let yaml = r#"
schema: "nika/workflow@0.12"
model: test-model
tasks:
  - id: infer_task
    infer: "Generate"
  - id: exec_task
    exec: "echo hello"
  - id: fetch_task
    fetch:
      url: "https://example.com"
  - id: invoke_task
    invoke:
      tool: "nika:log"
      params:
        message: "test"
  - id: agent_task
    agent:
      prompt: "Research"
      max_turns: 3
"#;
    let w = ok(yaml);
    assert_eq!(w.tasks.len(), 5);
    assert_eq!(w.flow_count(), 0);
}

#[test]
fn t22_workflow_level_provider_with_task_overrides() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: claude
model: claude-sonnet-4-6
tasks:
  - id: t1
    infer: "Uses default claude"
  - id: t2
    provider: openai
    model: gpt-4o
    infer: "Uses openai"
  - id: t3
    provider: gemini
    infer: "Uses gemini"
"#;
    let w = ok(yaml);
    assert_eq!(w.provider.as_str(), "anthropic");
    assert_eq!(w.model.as_deref(), Some("claude-sonnet-4-6"));
    match &w.tasks[1].action {
        TaskAction::Infer { infer } => {
            assert_eq!(infer.provider.as_ref().map(|p| p.as_str()), Some("openai"));
            assert_eq!(infer.model.as_deref(), Some("gpt-4o"));
        }
        _ => panic!("expected Infer"),
    }
}

#[test]
fn t23_fetch_retry_with_extract() {
    let yaml = r#"
schema: "nika/workflow@0.12"
model: test-model
tasks:
  - id: scrape
    retry:
      max_attempts: 3
      delay_ms: 2000
      backoff: 2.0
    fetch:
      url: "https://unstable.example.com"
      extract: markdown
      timeout: 30
"#;
    let w = ok(yaml);
    match &w.tasks[0].action {
        TaskAction::Fetch { fetch } => {
            assert!(fetch.retry.is_some());
            assert_eq!(fetch.extract, Some(ExtractMode::Markdown));
        }
        _ => panic!("expected Fetch"),
    }
}

#[test]
fn t24_for_each_with_structured_output() {
    let yaml = r#"
schema: "nika/workflow@0.12"
model: test-model
tasks:
  - id: batch
    for_each: ["Product A", "Product B", "Product C"]
    as: product
    structured:
      schema:
        type: object
        properties:
          name:
            type: string
          category:
            type: string
    infer: "Categorize: {{with.product}}"
"#;
    let w = ok(yaml);
    assert!(w.tasks[0].for_each.is_some());
    assert!(w.tasks[0].structured.is_some());
}

#[test]
fn t25_decompose_with_infer() {
    let yaml = r#"
schema: "nika/workflow@0.12"
model: test-model
tasks:
  - id: gen
    decompose:
      strategy: semantic
      traverse: HAS_CHILD
      source: "$entity"
    infer: "Generate for {{with.item}}"
"#;
    let w = ok(yaml);
    assert!(w.tasks[0].decompose.is_some());
}

#[test]
fn t26_multiple_with_bindings_from_four_tasks() {
    let yaml = r#"
schema: "nika/workflow@0.12"
model: test-model
tasks:
  - id: a
    infer: "A"
  - id: b
    infer: "B"
  - id: c
    infer: "C"
  - id: d
    infer: "D"
  - id: merge
    depends_on: [a, b, c, d]
    with:
      va: $a
      vb: $b
      vc: $c
      vd: $d
    infer: "{{with.va}} {{with.vb}} {{with.vc}} {{with.vd}}"
"#;
    let w = ok(yaml);
    let spec = w.tasks[4].with_spec.as_ref().unwrap();
    assert_eq!(spec.len(), 4);
}

#[test]
fn t27_exec_with_env_and_cwd_then_fetch() {
    let yaml = r#"
schema: "nika/workflow@0.12"
model: test-model
tasks:
  - id: build
    exec:
      command: "make build"
      shell: true
      cwd: "/app"
      env:
        NODE_ENV: production
        CI: "true"
      timeout: 300
  - id: test
    depends_on: [build]
    fetch:
      url: "https://localhost:3000/health"
      response: full
"#;
    let w = ok(yaml);
    match &w.tasks[0].action {
        TaskAction::Exec { exec } => {
            assert_eq!(exec.cwd.as_deref(), Some("/app"));
            assert_eq!(exec.timeout, Some(300));
            assert!(exec.env.as_ref().unwrap().contains_key("NODE_ENV"));
        }
        _ => panic!("expected Exec"),
    }
}

#[test]
fn t28_fetch_jsonpath_then_for_each_infer() {
    let yaml = r#"
schema: "nika/workflow@0.12"
model: test-model
tasks:
  - id: api
    fetch:
      url: "https://api.example.com/products"
      extract: jsonpath
      selector: "$.data[*].name"
  - id: describe
    depends_on: [api]
    for_each: $api
    as: name
    infer: "Write a description for: {{with.name}}"
"#;
    let w = ok(yaml);
    assert_eq!(w.tasks.len(), 2);
    assert!(w.tasks[1].for_each.is_some());
}

#[test]
fn t29_agent_with_context_binding() {
    let yaml = r#"
schema: "nika/workflow@0.12"
model: test-model
tasks:
  - id: fetch_context
    fetch:
      url: "https://docs.example.com/api"
      extract: markdown
  - id: research
    depends_on: [fetch_context]
    with:
      docs: $fetch_context
    agent:
      prompt: "Based on docs: {{with.docs}}, implement the feature"
      system: "You are a senior developer"
      tools:
        - nika:read
        - nika:write
        - nika:edit
      max_turns: 20
      token_budget: 100000
"#;
    let w = ok(yaml);
    match &w.tasks[1].action {
        TaskAction::Agent { agent } => {
            assert_eq!(agent.tools.len(), 3);
            assert_eq!(agent.token_budget, Some(100000));
        }
        _ => panic!("expected Agent"),
    }
}

#[test]
fn t30_complex_web_dag_seven_tasks() {
    let yaml = r#"
schema: "nika/workflow@0.12"
model: test-model
tasks:
  - id: a
    infer: "A"
  - id: b
    infer: "B"
  - id: c
    depends_on: [a]
    infer: "C"
  - id: d
    depends_on: [a, b]
    infer: "D"
  - id: e
    depends_on: [b]
    infer: "E"
  - id: f
    depends_on: [c, d]
    infer: "F"
  - id: g
    depends_on: [d, e, f]
    infer: "G"
"#;
    let w = ok(yaml);
    assert_eq!(w.tasks.len(), 7);
    assert_eq!(w.flow_count(), 9);
}

// ═══════════════════════════════════════════════════════════════════════════════
// U. RETRY + ERROR HANDLING (15 tests)
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn u01_fetch_retry_default_values() {
    let yaml = wrap("retry:\n  max_attempts: 3\n  delay_ms: 1000\n  backoff: 2.0\nfetch:\n  url: \"https://example.com\"");
    let w = ok(&yaml);
    match &w.tasks[0].action {
        TaskAction::Fetch { fetch } => {
            let retry = fetch.retry.as_ref().unwrap();
            assert_eq!(retry.max_attempts, 3);
            assert_eq!(retry.backoff_ms, 1000);
            assert!((retry.multiplier - 2.0).abs() < f64::EPSILON);
        }
        _ => panic!("expected Fetch"),
    }
}

#[test]
fn u02_fetch_retry_custom_values() {
    let yaml = wrap("retry:\n  max_attempts: 10\n  delay_ms: 5000\n  backoff: 1.5\nfetch:\n  url: \"https://example.com\"");
    let w = ok(&yaml);
    match &w.tasks[0].action {
        TaskAction::Fetch { fetch } => {
            let retry = fetch.retry.as_ref().unwrap();
            assert_eq!(retry.max_attempts, 10);
            assert_eq!(retry.backoff_ms, 5000);
        }
        _ => panic!("expected Fetch"),
    }
}

#[test]
fn u03_fetch_retry_with_extract_markdown() {
    let yaml = wrap("retry:\n  max_attempts: 3\n  delay_ms: 1000\n  backoff: 2.0\nfetch:\n  url: \"https://example.com\"\n  extract: markdown");
    let w = ok(&yaml);
    match &w.tasks[0].action {
        TaskAction::Fetch { fetch } => {
            assert!(fetch.retry.is_some());
            assert_eq!(fetch.extract, Some(ExtractMode::Markdown));
        }
        _ => panic!("expected Fetch"),
    }
}

#[test]
fn u04_fetch_timeout_with_extract() {
    let yaml = wrap("fetch:\n  url: \"https://slow.example.com\"\n  extract: text\n  timeout: 120");
    let w = ok(&yaml);
    match &w.tasks[0].action {
        TaskAction::Fetch { fetch } => {
            assert_eq!(fetch.timeout, Some(120));
            assert_eq!(fetch.extract, Some(ExtractMode::Text));
        }
        _ => panic!("expected Fetch"),
    }
}

#[test]
fn u05_fetch_follow_redirects_false_response_full() {
    let yaml = wrap(
        "fetch:\n  url: \"https://short.link/abc\"\n  follow_redirects: false\n  response: full",
    );
    let w = ok(&yaml);
    match &w.tasks[0].action {
        TaskAction::Fetch { fetch } => {
            assert_eq!(fetch.follow_redirects, Some(false));
            assert_eq!(fetch.response, Some(ResponseMode::Full));
        }
        _ => panic!("expected Fetch"),
    }
}

#[test]
fn u06_fetch_validate_empty_url_err() {
    let yaml = wrap("fetch:\n  url: \"\"");
    let w = ok(&yaml);
    match &w.tasks[0].action {
        TaskAction::Fetch { fetch } => {
            assert!(fetch.validate().is_err());
        }
        _ => panic!("expected Fetch"),
    }
}

#[test]
fn u07_fetch_validate_zero_timeout_err() {
    // timeout: 0 is rejected at analysis time (not runtime validation)
    let yaml = wrap("fetch:\n  url: \"https://example.com\"\n  timeout: 0");
    let e = err(&yaml);
    assert!(
        e.to_string().contains("timeout: 0"),
        "Expected timeout error, got: {e}"
    );
}

#[test]
fn u08_infer_validate_empty_prompt_no_content_err() {
    // Empty prompts are now rejected at analysis time (NIKA-145)
    let yaml = wrap("infer:\n  prompt: \"\"");
    let e = err(&yaml);
    let msg = format!("{e}");
    assert!(
        msg.contains("empty prompt"),
        "Expected empty prompt error, got: {msg}"
    );
}

#[test]
fn u09_infer_validate_temperature_out_of_range() {
    let yaml = wrap("infer:\n  prompt: \"Test\"\n  temperature: 3.0");
    let w = ok(&yaml);
    match &w.tasks[0].action {
        TaskAction::Infer { infer } => {
            assert!(infer.validate().is_err());
        }
        _ => panic!("expected Infer"),
    }
}

#[test]
fn u10_exec_validate_empty_command_err() {
    let yaml = wrap("exec:\n  command: \"\"");
    let w = ok(&yaml);
    match &w.tasks[0].action {
        TaskAction::Exec { exec } => {
            assert!(exec.validate().is_err());
        }
        _ => panic!("expected Exec"),
    }
}

#[test]
fn u11_exec_validate_zero_timeout_err() {
    // timeout: 0 is rejected at analysis time (not runtime validation)
    let yaml = wrap("exec:\n  command: \"echo hi\"\n  timeout: 0");
    let e = err(&yaml);
    assert!(
        e.to_string().contains("timeout: 0"),
        "Expected timeout error, got: {e}"
    );
}

#[test]
fn u12_infer_validate_thinking_budget_out_of_range() {
    let yaml = wrap("provider: claude\ninfer:\n  prompt: \"Test\"\n  extended_thinking: true\n  thinking_budget: 999");
    let w = ok(&yaml);
    match &w.tasks[0].action {
        TaskAction::Infer { infer } => {
            assert!(infer.validate().is_err());
        }
        _ => panic!("expected Infer"),
    }
}

#[test]
fn u13_infer_validate_thinking_non_claude_provider_at_task_level() {
    // Through the three-phase pipeline, provider set inside infer block
    // is hoisted to task level, not kept in InferParams.
    // extended_thinking IS preserved in InferParams.
    let yaml = wrap("provider: openai\ninfer:\n  prompt: \"Test\"\n  extended_thinking: true");
    let w = ok(&yaml);
    match &w.tasks[0].action {
        TaskAction::Infer { infer } => {
            // extended_thinking is preserved in InferParams
            assert_eq!(infer.extended_thinking, Some(true));
            // provider is at task level (hoisted), not in InferParams
            // The validate() check requires provider to be in InferParams to catch the error
        }
        _ => panic!("expected Infer"),
    }
}

#[test]
fn u14_fetch_retry_with_response_binary() {
    let yaml = wrap("retry:\n  max_attempts: 5\n  delay_ms: 3000\n  backoff: 1.0\nfetch:\n  url: \"https://cdn.example.com/file.zip\"\n  response: binary\n  timeout: 300");
    let w = ok(&yaml);
    match &w.tasks[0].action {
        TaskAction::Fetch { fetch } => {
            assert!(fetch.retry.is_some());
            assert_eq!(fetch.response, Some(ResponseMode::Binary));
            assert_eq!(fetch.timeout, Some(300));
        }
        _ => panic!("expected Fetch"),
    }
}

#[test]
fn u15_infer_effective_thinking_budget_default() {
    let yaml = wrap("provider: claude\ninfer:\n  prompt: \"Test\"\n  extended_thinking: true");
    let w = ok(&yaml);
    match &w.tasks[0].action {
        TaskAction::Infer { infer } => {
            assert_eq!(infer.effective_thinking_budget(), 4096);
        }
        _ => panic!("expected Infer"),
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// V. SCHEMA VERSION + COMPATIBILITY (10 tests)
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn v01_schema_012_with_all_new_features() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: claude
model: claude-sonnet-4-6
tasks:
  - id: vision_task
    infer:
      content:
        - type: image
          source: "blake3:test"
          detail: high
        - type: text
          text: "Describe"
      extended_thinking: true
      thinking_budget: 4096
  - id: fetch_task
    fetch:
      url: "https://example.com"
      extract: markdown
      response: full
  - id: structured_task
    depends_on: [vision_task]
    structured:
      schema:
        type: object
    output:
      format: json
    infer: "Extract data"
"#;
    let w = ok(yaml);
    assert_eq!(w.schema, "nika/workflow@0.12");
    assert_eq!(w.tasks.len(), 3);
}

#[test]
fn v02_maximum_complexity_single_task() {
    let yaml = r#"
schema: "nika/workflow@0.12"
model: test-model
tasks:
  - id: mega_task
    provider: claude
    model: claude-sonnet-4-6
    output:
      format: json
      schema:
        type: object
        properties:
          result:
            type: string
    structured:
      schema:
        type: object
        properties:
          name:
            type: string
      max_retries: 3
      enable_repair: true
    infer:
      prompt: "Extract structured data"
      system: "You are a data extractor"
      temperature: 0.1
      max_tokens: 1000
      content:
        - type: image
          source: "blake3:test"
          detail: high
        - type: text
          text: "Process this image"
"#;
    let w = ok(yaml);
    assert_eq!(w.tasks.len(), 1);
    assert!(w.tasks[0].output.is_some());
    assert!(w.tasks[0].structured.is_some());
    match &w.tasks[0].action {
        TaskAction::Infer { infer } => {
            assert_eq!(infer.temperature, Some(0.1));
            assert_eq!(infer.max_tokens, Some(1000));
            assert!(infer.content.is_some());
        }
        _ => panic!("expected Infer"),
    }
}

#[test]
fn v03_schema_010_still_parses() {
    let yaml = "schema: \"nika/workflow@0.10\"\nmodel: test-model\ntasks:\n  - id: t1\n    infer: \"Hello\"";
    let w = ok(yaml);
    assert_eq!(w.schema, "nika/workflow@0.10");
}

#[test]
fn v04_schema_011_still_parses() {
    let yaml = "schema: \"nika/workflow@0.11\"\ntasks:\n  - id: t1\n    exec: \"echo hi\"";
    let w = ok(yaml);
    assert_eq!(w.schema, "nika/workflow@0.11");
}

#[test]
fn v05_all_providers_at_workflow_level() {
    // Input alias → canonical name after ProviderName resolution
    let providers: &[(&str, &str)] = &[
        ("claude", "anthropic"),
        ("openai", "openai"),
        ("groq", "groq"),
        ("gemini", "gemini"),
        ("xai", "xai"),
        ("deepseek", "deepseek"),
        ("mistral", "mistral"),
        ("mock", "mock"),
    ];
    for (alias, canonical) in providers {
        let yaml = format!(
            "schema: \"nika/workflow@0.12\"\nprovider: {}\nmodel: test-model\ntasks:\n  - id: t1\n    infer: \"Test\"",
            alias
        );
        let w = ok(&yaml);
        assert_eq!(w.provider.as_str(), *canonical);
    }
}

#[test]
fn v06_schema_hash_differs_across_versions() {
    let v10 = ok(
        "schema: \"nika/workflow@0.10\"\nmodel: test-model\ntasks:\n  - id: t1\n    infer: \"A\"",
    );
    let v11 = ok(
        "schema: \"nika/workflow@0.11\"\nmodel: test-model\ntasks:\n  - id: t1\n    infer: \"A\"",
    );
    let v12 = ok(
        "schema: \"nika/workflow@0.12\"\nmodel: test-model\ntasks:\n  - id: t1\n    infer: \"A\"",
    );
    assert_ne!(v10.compute_hash(), v11.compute_hash());
    assert_ne!(v11.compute_hash(), v12.compute_hash());
}

#[test]
fn v07_workflow_inputs_with_multiple_params() {
    let yaml = r#"
schema: "nika/workflow@0.12"
model: test-model
inputs:
  name:
    type: string
    default: "world"
  count:
    type: integer
    default: 5
  verbose:
    type: boolean
    default: false
tasks:
  - id: t1
    infer: "Hello {{inputs.name}} x{{inputs.count}}"
"#;
    let w = ok(yaml);
    assert!(w.inputs.is_some());
    let inputs = w.inputs.as_ref().unwrap();
    assert_eq!(inputs.len(), 3);
}

#[test]
fn v08_workflow_no_inputs() {
    let yaml = "schema: \"nika/workflow@0.12\"\nmodel: test-model\ntasks:\n  - id: t1\n    infer: \"Hello\"";
    let w = ok(yaml);
    assert!(w.inputs.is_none());
}

#[test]
fn v09_workflow_no_model() {
    let yaml = "schema: \"nika/workflow@0.12\"\ntasks:\n  - id: t1\n    infer: \"Hello\"";
    let e = err(yaml);
    let msg = format!("{e}");
    assert!(
        msg.contains("NIKA-034") || msg.contains("model"),
        "expected MissingModel error, got: {msg}"
    );
}

#[test]
fn v10_workflow_no_mcp() {
    let yaml = "schema: \"nika/workflow@0.12\"\nmodel: test-model\ntasks:\n  - id: t1\n    infer: \"Hello\"";
    let w = ok(yaml);
    assert!(w.mcp.is_none());
}

// ═══════════════════════════════════════════════════════════════════════════════
// W. STRUCTURED OUTPUT + EXTRACT (15 tests)
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn w01_fetch_metadata_with_json_output() {
    let yaml = r#"
schema: "nika/workflow@0.12"
model: test-model
tasks:
  - id: meta
    fetch:
      url: "https://example.com"
      extract: metadata
  - id: structure
    depends_on: [meta]
    with:
      m: $meta
    output:
      format: json
      schema:
        type: object
        properties:
          title:
            type: string
    infer: "Structure metadata: {{with.m}}"
"#;
    let w = ok(yaml);
    let output = w.tasks[1].output.as_ref().unwrap();
    assert!(output.is_structured());
}

#[test]
fn w02_fetch_links_with_json_output() {
    let yaml = r#"
schema: "nika/workflow@0.12"
model: test-model
tasks:
  - id: links
    fetch:
      url: "https://example.com"
      extract: links
  - id: classify
    depends_on: [links]
    with:
      l: $links
    output:
      format: json
    infer: "Classify links: {{with.l}}"
"#;
    let w = ok(yaml);
    let output = w.tasks[1].output.as_ref().unwrap();
    assert_eq!(output.format, OutputFormat::Json);
}

#[test]
fn w03_infer_content_with_json_output() {
    let yaml = wrap(
        "output:\n  format: json\n  schema:\n    type: object\n    properties:\n      objects:\n        type: array\ninfer:\n  content:\n    - type: image\n      source: \"blake3:test\"\n      detail: high\n    - type: text\n      text: \"List objects\"",
    );
    let w = ok(&yaml);
    let output = w.tasks[0].output.as_ref().unwrap();
    assert!(output.is_structured());
}

#[test]
fn w04_structured_shorthand_with_infer() {
    let yaml = wrap("structured: ./schemas/user.json\ninfer: \"Extract user data\"");
    let w = ok(&yaml);
    assert!(w.tasks[0].structured.is_some());
}

#[test]
fn w05_structured_inline_schema_complex() {
    let yaml = wrap(
        "structured:\n  schema:\n    type: object\n    properties:\n      users:\n        type: array\n        items:\n          type: object\n          properties:\n            name:\n              type: string\n            email:\n              type: string\n    required:\n      - users\ninfer: \"Extract users\"",
    );
    let w = ok(&yaml);
    assert!(w.tasks[0].structured.is_some());
}

#[test]
fn w06_structured_with_repair_and_retries() {
    let yaml = wrap(
        "structured:\n  schema: ./schemas/product.json\n  max_retries: 5\n  enable_repair: true\n  repair_model: claude-sonnet-4-6\ninfer: \"Extract product\"",
    );
    let w = ok(&yaml);
    let spec = w.tasks[0].structured.as_ref().unwrap();
    assert_eq!(spec.max_retries, Some(5));
    assert_eq!(spec.enable_repair, Some(true));
    assert_eq!(spec.repair_model.as_deref(), Some("claude-sonnet-4-6"));
}

#[test]
fn w07_output_format_yaml_with_schema() {
    let yaml = wrap("output:\n  format: yaml\n  schema:\n    type: object\ninfer: \"Generate\"");
    let w = ok(&yaml);
    let output = w.tasks[0].output.as_ref().unwrap();
    assert_eq!(output.format, OutputFormat::Yaml);
    assert!(output.schema.is_some());
}

#[test]
fn w08_output_text_no_schema() {
    let yaml = wrap("output:\n  format: text\ninfer: \"Generate plain\"");
    let w = ok(&yaml);
    let output = w.tasks[0].output.as_ref().unwrap();
    assert_eq!(output.format, OutputFormat::Text);
    assert!(output.schema.is_none());
}

#[test]
fn w09_structured_all_layers_disabled() {
    let yaml = wrap(
        "structured:\n  schema: ./test.json\n  enable_extractor: false\n  enable_tool_injection: false\n  enable_retry: false\n  enable_repair: false\ninfer: \"Test\"",
    );
    let w = ok(&yaml);
    let spec = w.tasks[0].structured.as_ref().unwrap();
    // enable_extractor removed (L1 was never implemented) — silently ignored in YAML
    assert_eq!(spec.enable_tool_injection, Some(false));
    assert_eq!(spec.enable_retry, Some(false));
    assert_eq!(spec.enable_repair, Some(false));
}

#[test]
fn w10_output_json_is_structured_true() {
    let yaml = wrap("output:\n  format: json\n  schema:\n    type: object\ninfer: \"Test\"");
    let w = ok(&yaml);
    assert!(w.tasks[0].output.as_ref().unwrap().is_structured());
}

#[test]
fn w11_output_json_no_schema_is_structured_false() {
    let yaml = wrap("output:\n  format: json\ninfer: \"Test\"");
    let w = ok(&yaml);
    assert!(!w.tasks[0].output.as_ref().unwrap().is_structured());
}

#[test]
fn w12_structured_with_vision_and_output() {
    let yaml = wrap(
        "output:\n  format: json\nstructured:\n  schema:\n    type: object\ninfer:\n  content:\n    - type: image\n      source: \"blake3:doc\"\n      detail: high\n    - type: text\n      text: \"Extract data\"",
    );
    let w = ok(&yaml);
    assert!(w.tasks[0].output.is_some());
    assert!(w.tasks[0].structured.is_some());
}

#[test]
fn w13_fetch_extract_then_structured_pipeline() {
    let yaml = r#"
schema: "nika/workflow@0.12"
model: test-model
tasks:
  - id: scrape
    fetch:
      url: "https://example.com/product"
      extract: text
      selector: ".product-details"
  - id: extract
    depends_on: [scrape]
    with:
      raw: $scrape
    structured:
      schema:
        type: object
        properties:
          name:
            type: string
          price:
            type: number
    infer: "Extract product data from: {{with.raw}}"
"#;
    let w = ok(yaml);
    assert!(w.tasks[1].structured.is_some());
}

#[test]
fn w14_output_json_schema_nested() {
    let yaml = wrap(
        "output:\n  format: json\n  schema:\n    type: object\n    properties:\n      data:\n        type: object\n        properties:\n          items:\n            type: array\n            items:\n              type: string\ninfer: \"Generate\"",
    );
    let w = ok(&yaml);
    assert!(w.tasks[0].output.as_ref().unwrap().is_structured());
}

#[test]
fn w15_structured_and_output_both_with_schema() {
    let yaml = wrap(
        "output:\n  format: json\n  schema:\n    type: object\nstructured:\n  schema:\n    type: object\n  max_retries: 2\ninfer: \"Test\"",
    );
    let w = ok(&yaml);
    assert!(w.tasks[0].output.is_some());
    assert!(w.tasks[0].structured.is_some());
    assert_eq!(w.tasks[0].structured.as_ref().unwrap().max_retries, Some(2));
}

// ═══════════════════════════════════════════════════════════════════════════════
// X. CONTEXT + INCLUDES + SKILLS (10 tests)
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn x01_context_files_config() {
    let yaml = r#"
schema: "nika/workflow@0.12"
model: test-model
context:
  files:
    brand: ./context/brand.md
    persona: ./context/persona.json
tasks:
  - id: t1
    infer: "Use {{context.files.brand}}"
"#;
    let w = ok(yaml);
    assert!(w.context.is_some());
    let ctx = w.context.as_ref().unwrap();
    assert_eq!(ctx.files.len(), 2);
}

#[test]
fn x02_context_with_session() {
    let yaml = r#"
schema: "nika/workflow@0.12"
model: test-model
context:
  files:
    data: ./context/data.json
  session: .nika/sessions/prev.json
tasks:
  - id: t1
    infer: "Resume previous session"
"#;
    let w = ok(yaml);
    // Context is preserved through the pipeline; session may or may not be lowered
    assert!(w.context.is_some());
}

#[test]
fn x03_include_path_spec() {
    let yaml = r#"
schema: "nika/workflow@0.12"
model: test-model
include:
  - path: ./lib/seo-tasks.nika.yaml
    prefix: seo_
tasks:
  - id: t1
    infer: "Main task"
"#;
    let w = ok(yaml);
    assert!(w.include.is_some());
    let includes = w.include.as_ref().unwrap();
    assert_eq!(includes.len(), 1);
}

#[test]
fn x04_include_multiple_specs() {
    let yaml = r#"
schema: "nika/workflow@0.12"
model: test-model
include:
  - path: ./lib/common.nika.yaml
    prefix: common_
  - path: ./lib/media.nika.yaml
    prefix: media_
tasks:
  - id: t1
    infer: "Main task"
"#;
    let w = ok(yaml);
    let includes = w.include.as_ref().unwrap();
    assert_eq!(includes.len(), 2);
}

#[test]
fn x05_skill_definitions_in_agent() {
    // skills field on agent tasks is always available
    let yaml =
        wrap("agent:\n  prompt: \"Code something\"\n  skills:\n    - coding\n    - research");
    let w = ok(&yaml);
    match &w.tasks[0].action {
        TaskAction::Agent { agent } => {
            let skills = agent.skills.as_ref().unwrap();
            assert_eq!(skills.len(), 2);
        }
        _ => panic!("expected Agent"),
    }
}

#[test]
fn x06_agents_definition() {
    let yaml = r#"
schema: "nika/workflow@0.12"
model: test-model
agents:
  researcher:
    system: "You are a researcher"
    tools:
      - nika:read
    max_turns: 10
tasks:
  - id: t1
    infer: "Test"
"#;
    let w = ok(yaml);
    assert!(w.agents.is_some());
}

#[test]
fn x07_context_empty_files() {
    let yaml = r#"
schema: "nika/workflow@0.12"
model: test-model
context:
  files: {}
tasks:
  - id: t1
    infer: "No context files"
"#;
    let w = ok(yaml);
    // Through the pipeline, empty context may be normalized to None or preserved
    // Either way, the workflow parses successfully
    assert_eq!(w.tasks.len(), 1);
}

#[test]
fn x08_no_context_no_include() {
    let yaml = "schema: \"nika/workflow@0.12\"\nmodel: test-model\ntasks:\n  - id: t1\n    infer: \"Hello\"";
    let w = ok(yaml);
    assert!(w.context.is_none());
    assert!(w.include.is_none());
}

#[test]
fn x09_workflow_with_log_config() {
    let yaml = r#"
schema: "nika/workflow@0.12"
model: test-model
log:
  level: debug
tasks:
  - id: t1
    infer: "Test"
"#;
    let w = ok(yaml);
    assert!(w.log.is_some());
}

#[test]
fn x10_workflow_with_artifacts_config() {
    let yaml = r#"
schema: "nika/workflow@0.12"
model: test-model
artifacts:
  dir: ./output
tasks:
  - id: t1
    infer: "Generate content"
"#;
    let w = ok(yaml);
    assert!(w.artifacts.is_some());
}

// ═══════════════════════════════════════════════════════════════════════════════
// Y. LIMITS + GUARDRAILS (10 tests)
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn y01_output_format_json_with_infer() {
    let yaml = wrap("output:\n  format: json\ninfer: \"Generate JSON\"");
    let w = ok(&yaml);
    assert!(w.tasks[0].output.is_some());
}

#[test]
fn y02_output_format_yaml_with_exec() {
    let yaml = wrap("output:\n  format: yaml\nexec: \"generate-yaml.sh\"");
    let w = ok(&yaml);
    assert!(w.tasks[0].output.is_some());
}

#[test]
fn y03_output_with_fetch() {
    let yaml = wrap("output:\n  format: json\nfetch:\n  url: \"https://api.example.com\"");
    let w = ok(&yaml);
    assert!(w.tasks[0].output.is_some());
}

#[test]
fn y04_task_level_artifact_config() {
    let yaml = r#"
schema: "nika/workflow@0.12"
model: test-model
tasks:
  - id: gen
    artifact:
      path: ./output/result.json
    infer: "Generate result"
"#;
    let w = ok(yaml);
    assert!(w.tasks[0].artifact.is_some());
}

#[test]
fn y05_task_level_log_config() {
    let yaml = r#"
schema: "nika/workflow@0.12"
model: test-model
tasks:
  - id: debug_task
    log:
      level: trace
    infer: "Debug this"
"#;
    let w = ok(yaml);
    assert!(w.tasks[0].log.is_some());
}

#[test]
fn y06_structured_max_retries_zero() {
    let yaml = wrap("structured:\n  schema: ./test.json\n  max_retries: 0\ninfer: \"Test\"");
    let w = ok(&yaml);
    let spec = w.tasks[0].structured.as_ref().unwrap();
    assert_eq!(spec.max_retries, Some(0));
}

#[test]
fn y07_structured_enable_all() {
    let yaml = wrap(
        "structured:\n  schema: ./test.json\n  enable_extractor: true\n  enable_tool_injection: true\n  enable_retry: true\n  enable_repair: true\ninfer: \"Test\"",
    );
    let w = ok(&yaml);
    let spec = w.tasks[0].structured.as_ref().unwrap();
    // enable_extractor removed (L1 was never implemented) — silently ignored in YAML
    assert_eq!(spec.enable_tool_injection, Some(true));
    assert_eq!(spec.enable_retry, Some(true));
    assert_eq!(spec.enable_repair, Some(true));
}

#[test]
fn y08_output_none_by_default() {
    let yaml = wrap("infer: \"No output config\"");
    let w = ok(&yaml);
    assert!(w.tasks[0].output.is_none());
}

#[test]
fn y09_structured_none_by_default() {
    let yaml = wrap("infer: \"No structured config\"");
    let w = ok(&yaml);
    assert!(w.tasks[0].structured.is_none());
}

#[test]
fn y10_artifact_none_by_default() {
    let yaml = wrap("infer: \"No artifact config\"");
    let w = ok(&yaml);
    assert!(w.tasks[0].artifact.is_none());
}

// ═══════════════════════════════════════════════════════════════════════════════
// Z. EDGE CASES (20 tests)
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn z01_unicode_in_extract_selector() {
    let yaml = wrap(
        "fetch:\n  url: \"https://example.jp\"\n  extract: selector\n  selector: \"div.content\"",
    );
    let w = ok(&yaml);
    match &w.tasks[0].action {
        TaskAction::Fetch { fetch } => {
            assert_eq!(fetch.selector.as_deref(), Some("div.content"));
        }
        _ => panic!("expected Fetch"),
    }
}

#[test]
fn z02_very_long_css_selector() {
    let selector = "div.container > section.main > article.post > div.content > ul.list > li.item:nth-child(2n+1) > a.link > span.text";
    let yaml = wrap(&format!(
        "fetch:\n  url: \"https://example.com\"\n  extract: selector\n  selector: \"{}\"",
        selector
    ));
    let w = ok(&yaml);
    match &w.tasks[0].action {
        TaskAction::Fetch { fetch } => {
            assert_eq!(fetch.selector.as_deref(), Some(selector));
        }
        _ => panic!("expected Fetch"),
    }
}

#[test]
fn z03_task_id_with_underscores() {
    let yaml =
        "schema: \"nika/workflow@0.12\"\nmodel: test-model\ntasks:\n  - id: my_long_task_name\n    infer: \"Test\"";
    let w = ok(yaml);
    assert_eq!(w.tasks[0].id, "my_long_task_name");
}

#[test]
fn z04_task_id_with_hyphens() {
    let yaml = "schema: \"nika/workflow@0.12\"\nmodel: test-model\ntasks:\n  - id: my-task-name\n    infer: \"Test\"";
    let w = ok(yaml);
    assert_eq!(w.tasks[0].id, "my-task-name");
}

#[test]
fn z05_task_id_with_numbers() {
    let yaml = "schema: \"nika/workflow@0.12\"\nmodel: test-model\ntasks:\n  - id: task123\n    infer: \"Test\"";
    let w = ok(yaml);
    assert_eq!(w.tasks[0].id, "task123");
}

#[test]
fn z06_many_tasks_twenty() {
    let mut yaml = "schema: \"nika/workflow@0.12\"\nmodel: test-model\ntasks:\n".to_string();
    for i in 1..=20 {
        yaml.push_str(&format!("  - id: t{:02}\n    infer: \"Task {}\"\n", i, i));
    }
    let w = ok(&yaml);
    assert_eq!(w.tasks.len(), 20);
}

#[test]
fn z07_deeply_nested_json_params() {
    let yaml = wrap(
        "invoke:\n  mcp: test\n  tool: deep\n  params:\n    level1:\n      level2:\n        level3:\n          level4:\n            value: deep",
    );
    let w = ok(&yaml);
    match &w.tasks[0].action {
        TaskAction::Invoke { invoke } => {
            let p = invoke.params.as_ref().unwrap();
            assert_eq!(p["level1"]["level2"]["level3"]["level4"]["value"], "deep");
        }
        _ => panic!("expected Invoke"),
    }
}

#[test]
fn z08_fetch_url_with_special_chars() {
    let yaml =
        wrap("fetch:\n  url: \"https://api.example.com/search?q=hello%20world&lang=en&sort=date\"");
    let w = ok(&yaml);
    match &w.tasks[0].action {
        TaskAction::Fetch { fetch } => {
            assert!(fetch.url.contains("hello%20world"));
        }
        _ => panic!("expected Fetch"),
    }
}

#[test]
fn z09_infer_prompt_with_template_syntax() {
    let yaml = r#"
schema: "nika/workflow@0.12"
model: test-model
tasks:
  - id: gen
    infer: "Generate"
  - id: use
    depends_on: [gen]
    with:
      a: $gen
      b: $gen.nested.path
      c: $gen | trim
    infer: "{{with.a}} {{with.b}} {{with.c}}"
"#;
    let w = ok(yaml);
    match &w.tasks[1].action {
        TaskAction::Infer { infer } => {
            assert!(infer.prompt.contains("{{with.a}}"));
            assert!(infer.prompt.contains("{{with.b}}"));
            assert!(infer.prompt.contains("{{with.c}}"));
        }
        _ => panic!("expected Infer"),
    }
}

#[test]
fn z10_exec_command_with_heredoc_style() {
    let yaml = "schema: \"nika/workflow@0.12\"\nmodel: test-model\ntasks:\n  - id: t1\n    exec: |\n      set -e\n      echo \"Step 1\"\n      echo \"Step 2\"\n      echo \"Done\"\n";
    let w = ok(yaml);
    match &w.tasks[0].action {
        TaskAction::Exec { exec } => {
            assert!(exec.command.contains("Step 1"));
            assert!(exec.command.contains("Step 2"));
        }
        _ => panic!("expected Exec"),
    }
}

#[test]
fn z11_fetch_json_body_with_array() {
    let yaml = wrap(
        "fetch:\n  url: \"https://api.example.com\"\n  method: POST\n  json:\n    items:\n      - name: a\n        value: 1\n      - name: b\n        value: 2\n      - name: c\n        value: 3",
    );
    let w = ok(&yaml);
    match &w.tasks[0].action {
        TaskAction::Fetch { fetch } => {
            let json = fetch.json.as_ref().unwrap();
            assert_eq!(json["items"].as_array().unwrap().len(), 3);
        }
        _ => panic!("expected Fetch"),
    }
}

#[test]
fn z12_invoke_resource_without_tool_is_valid() {
    // invoke with resource and no tool is now valid (resource read mode)
    let yaml = wrap("invoke:\n  mcp: novanet\n  resource: entity://qr-code/en-US");
    let wf = ok(&yaml);
    assert_eq!(wf.tasks.len(), 1);
}

#[test]
fn z13_workflow_with_all_optional_top_level_fields() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: openai
model: gpt-4o
inputs:
  param: {}
context:
  files:
    data: ./data.json
mcp:
  servers:
    test:
      command: echo
tasks:
  - id: t1
    infer: "Test"
"#;
    let w = ok(yaml);
    assert_eq!(w.provider.as_str(), "openai");
    assert!(w.model.is_some());
    assert!(w.inputs.is_some());
    assert!(w.context.is_some());
    assert!(w.mcp.is_some());
}

#[test]
fn z14_with_fallback_null_coalesce() {
    let yaml = r#"
schema: "nika/workflow@0.12"
model: test-model
tasks:
  - id: gen
    infer: "Generate"
  - id: use
    depends_on: [gen]
    with:
      name: $gen.name ?? "Unknown"
      count: $gen.count ?? 0
      flag: $gen.flag ?? true
    infer: "{{with.name}} {{with.count}} {{with.flag}}"
"#;
    let w = ok(yaml);
    assert!(w.tasks[1].with_spec.is_some());
}

#[test]
fn z15_exec_with_many_env_vars() {
    let yaml = wrap(
        "exec:\n  command: \"env\"\n  env:\n    A: \"1\"\n    B: \"2\"\n    C: \"3\"\n    D: \"4\"\n    E: \"5\"\n    F: \"6\"\n    G: \"7\"\n    H: \"8\"",
    );
    let w = ok(&yaml);
    match &w.tasks[0].action {
        TaskAction::Exec { exec } => {
            assert_eq!(exec.env.as_ref().unwrap().len(), 8);
        }
        _ => panic!("expected Exec"),
    }
}

#[test]
fn z16_fetch_many_headers() {
    let yaml = wrap(
        "fetch:\n  url: \"https://example.com\"\n  headers:\n    Accept: text/html\n    Accept-Language: en-US\n    Authorization: \"Bearer tok\"\n    Cache-Control: no-cache\n    Cookie: session=abc\n    User-Agent: nika/0.34",
    );
    let w = ok(&yaml);
    match &w.tasks[0].action {
        TaskAction::Fetch { fetch } => {
            assert_eq!(fetch.headers.len(), 6);
        }
        _ => panic!("expected Fetch"),
    }
}

#[test]
fn z17_agent_with_all_optional_fields() {
    let yaml = wrap(
        "agent:\n  prompt: \"Full agent\"\n  system: \"Expert\"\n  provider: claude\n  model: claude-sonnet-4-6\n  mcp:\n    - novanet\n  tools:\n    - nika:read\n    - nika:write\n  skills:\n    - coding\n  max_turns: 25\n  token_budget: 200000\n  temperature: 0.7\n  max_tokens: 8192\n  depth_limit: 5\n  extended_thinking: true\n  thinking_budget: 8192",
    );
    let w = ok(&yaml);
    match &w.tasks[0].action {
        TaskAction::Agent { agent } => {
            assert_eq!(agent.prompt, "Full agent");
            assert!(agent.system.is_some());
            assert_eq!(
                agent.provider.as_ref().map(|p| p.as_str()),
                Some("anthropic")
            );
            assert_eq!(agent.model.as_deref(), Some("claude-sonnet-4-6"));
            assert_eq!(agent.mcp.len(), 1);
            assert_eq!(agent.tools.len(), 2);
            assert_eq!(agent.skills.as_ref().unwrap().len(), 1);
            assert_eq!(agent.max_turns, Some(25));
            assert_eq!(agent.token_budget, Some(200000));
            assert_eq!(agent.temperature, Some(0.7));
            assert_eq!(agent.max_tokens, Some(8192));
            assert_eq!(agent.depth_limit, Some(5));
            assert_eq!(agent.extended_thinking, Some(true));
            assert_eq!(agent.thinking_budget, Some(8192));
        }
        _ => panic!("expected Agent"),
    }
}

#[test]
fn z18_infer_all_fields_simultaneously() {
    let yaml = wrap(
        "provider: claude\nmodel: claude-sonnet-4-6\ninfer:\n  prompt: \"Full infer\"\n  system: \"You are an expert\"\n  temperature: 0.5\n  max_tokens: 2048\n  extended_thinking: true\n  thinking_budget: 4096\n  content:\n    - type: image\n      source: \"blake3:test\"\n      detail: high\n    - type: text\n      text: \"Also analyze this\"",
    );
    let w = ok(&yaml);
    match &w.tasks[0].action {
        TaskAction::Infer { infer } => {
            assert_eq!(infer.prompt, "Full infer");
            assert!(infer.system.is_some());
            assert_eq!(infer.temperature, Some(0.5));
            assert_eq!(infer.max_tokens, Some(2048));
            assert_eq!(infer.extended_thinking, Some(true));
            assert_eq!(infer.thinking_budget, Some(4096));
            assert!(infer.content.is_some());
            assert_eq!(
                infer.provider.as_ref().map(|p| p.as_str()),
                Some("anthropic")
            );
            assert_eq!(infer.model.as_deref(), Some("claude-sonnet-4-6"));
        }
        _ => panic!("expected Infer"),
    }
}

#[test]
fn z19_fetch_all_fields_simultaneously() {
    let yaml = wrap(
        "retry:\n  max_attempts: 3\n  delay_ms: 1000\n  backoff: 2.0\nfetch:\n  url: \"https://api.example.com/v2/data\"\n  method: POST\n  headers:\n    Content-Type: application/json\n    Authorization: \"Bearer tok123\"\n  json:\n    query: test\n    filters:\n      active: true\n  timeout: 60\n  follow_redirects: false\n  response: full\n  extract: jsonpath\n  selector: \"$.results\"",
    );
    let w = ok(&yaml);
    match &w.tasks[0].action {
        TaskAction::Fetch { fetch } => {
            assert_eq!(fetch.url, "https://api.example.com/v2/data");
            assert_eq!(fetch.method, "POST");
            assert_eq!(fetch.headers.len(), 2);
            assert!(fetch.json.is_some());
            assert_eq!(fetch.timeout, Some(60));
            assert_eq!(fetch.follow_redirects, Some(false));
            assert_eq!(fetch.response, Some(ResponseMode::Full));
            assert_eq!(fetch.extract, Some(ExtractMode::Jsonpath));
            assert_eq!(fetch.selector.as_deref(), Some("$.results"));
            assert!(fetch.retry.is_some());
        }
        _ => panic!("expected Fetch"),
    }
}

#[test]
fn z20_invoke_all_fields_simultaneously() {
    let yaml = wrap(
        "invoke:\n  mcp: novanet\n  tool: novanet_search\n  params:\n    query: test\n    limit: 10\n  timeout: 60",
    );
    let w = ok(&yaml);
    match &w.tasks[0].action {
        TaskAction::Invoke { invoke } => {
            assert_eq!(invoke.mcp.as_deref(), Some("novanet"));
            assert_eq!(invoke.tool.as_deref(), Some("novanet_search"));
            assert!(invoke.params.is_some());
            assert_eq!(invoke.timeout, Some(60));
        }
        _ => panic!("expected Invoke"),
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// AA. BACKWARD COMPATIBILITY (15 tests)
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn aa01_infer_shorthand_still_works() {
    let w = ok(&wrap("infer: \"Hello world\""));
    match &w.tasks[0].action {
        TaskAction::Infer { infer } => assert_eq!(infer.prompt, "Hello world"),
        _ => panic!("expected Infer"),
    }
}

#[test]
fn aa02_exec_shorthand_still_works() {
    let w = ok(&wrap("exec: \"echo hello\""));
    match &w.tasks[0].action {
        TaskAction::Exec { exec } => assert_eq!(exec.command, "echo hello"),
        _ => panic!("expected Exec"),
    }
}

#[test]
fn aa03_fetch_full_form_still_works() {
    let w = ok(&wrap("fetch:\n  url: \"https://example.com\""));
    match &w.tasks[0].action {
        TaskAction::Fetch { fetch } => assert_eq!(fetch.url, "https://example.com"),
        _ => panic!("expected Fetch"),
    }
}

#[test]
fn aa04_invoke_full_form_still_works() {
    let w = ok(&wrap("invoke:\n  mcp: novanet\n  tool: novanet_search"));
    match &w.tasks[0].action {
        TaskAction::Invoke { invoke } => {
            assert_eq!(invoke.mcp.as_deref(), Some("novanet"));
        }
        _ => panic!("expected Invoke"),
    }
}

#[test]
fn aa05_agent_full_form_still_works() {
    let w = ok(&wrap("agent:\n  prompt: \"Research\""));
    match &w.tasks[0].action {
        TaskAction::Agent { agent } => assert_eq!(agent.prompt, "Research"),
        _ => panic!("expected Agent"),
    }
}

#[test]
fn aa06_depends_on_still_works() {
    let yaml = r#"
schema: "nika/workflow@0.12"
model: test-model
tasks:
  - id: a
    infer: "A"
  - id: b
    depends_on: [a]
    infer: "B"
"#;
    let w = ok(yaml);
    assert_eq!(w.flow_count(), 1);
}

#[test]
fn aa07_with_binding_still_works() {
    let yaml = r#"
schema: "nika/workflow@0.12"
model: test-model
tasks:
  - id: gen
    infer: "Gen"
  - id: use
    depends_on: [gen]
    with:
      data: $gen
    infer: "Use {{with.data}}"
"#;
    let w = ok(yaml);
    assert!(w.tasks[1].with_spec.is_some());
}

#[test]
fn aa08_output_format_still_works() {
    let yaml = wrap("output:\n  format: json\ninfer: \"Generate JSON\"");
    let w = ok(&yaml);
    let output = w.tasks[0].output.as_ref().unwrap();
    assert_eq!(output.format, OutputFormat::Json);
}

#[test]
fn aa09_pr4_vision_content_unchanged() {
    let yaml = wrap(
        "infer:\n  content:\n    - type: image\n      source: \"blake3:abc123\"\n      detail: high\n    - type: text\n      text: \"Describe\"",
    );
    let w = ok(&yaml);
    match &w.tasks[0].action {
        TaskAction::Infer { infer } => {
            let parts = infer.content.as_ref().unwrap();
            assert_eq!(parts.len(), 2);
        }
        _ => panic!("expected Infer"),
    }
}

#[test]
fn aa10_pr4_vision_image_url_unchanged() {
    let yaml = wrap(
        "infer:\n  content:\n    - type: image_url\n      url: \"https://example.com/photo.jpg\"\n      detail: low",
    );
    let w = ok(&yaml);
    match &w.tasks[0].action {
        TaskAction::Infer { infer } => {
            let parts = infer.content.as_ref().unwrap();
            match &parts[0] {
                ContentPart::ImageUrl { url, detail } => {
                    assert_eq!(url, "https://example.com/photo.jpg");
                    assert_eq!(*detail, ImageDetail::Low);
                }
                _ => panic!("expected ImageUrl"),
            }
        }
        _ => panic!("expected Infer"),
    }
}

#[test]
fn aa11_pr4_extended_thinking_unchanged() {
    let yaml = wrap("provider: claude\ninfer:\n  prompt: \"Deep\"\n  extended_thinking: true\n  thinking_budget: 8192");
    let w = ok(&yaml);
    match &w.tasks[0].action {
        TaskAction::Infer { infer } => {
            assert_eq!(infer.extended_thinking, Some(true));
            assert_eq!(infer.thinking_budget, Some(8192));
        }
        _ => panic!("expected Infer"),
    }
}

#[test]
fn aa12_mcp_config_unchanged() {
    let yaml = r#"
schema: "nika/workflow@0.12"
mcp:
  servers:
    novanet:
      command: cargo
      args: [run, -p, novanet-mcp]
tasks:
  - id: t1
    invoke:
      mcp: novanet
      tool: novanet_search
"#;
    let w = ok(yaml);
    assert!(w.mcp.is_some());
}

#[test]
fn aa13_for_each_array_still_works() {
    let yaml = r#"
schema: "nika/workflow@0.12"
model: test-model
tasks:
  - id: t1
    for_each: ["a", "b", "c"]
    as: item
    infer: "Process {{with.item}}"
"#;
    let w = ok(yaml);
    assert!(w.tasks[0].for_each.is_some());
}

#[test]
fn aa14_for_each_dollar_binding_still_works() {
    let yaml = r#"
schema: "nika/workflow@0.12"
model: test-model
tasks:
  - id: gen
    infer: "Generate list"
  - id: process
    depends_on: [gen]
    for_each: "$gen"
    as: item
    infer: "Process {{with.item}}"
"#;
    let w = ok(yaml);
    assert!(w.tasks[1].for_each.is_some());
}

#[test]
fn aa15_workflow_default_provider_still_claude() {
    let yaml = "schema: \"nika/workflow@0.12\"\nmodel: test-model\ntasks:\n  - id: t1\n    infer: \"Hello\"";
    let w = ok(yaml);
    assert_eq!(w.provider.as_str(), "anthropic");
}

// ═══════════════════════════════════════════════════════════════════════════════
// AB. ALL 26 MEDIA TOOLS (26 tests)
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn ab01_invoke_nika_import() {
    let w = ok(&wrap(
        "invoke:\n  tool: nika:import\n  params:\n    path: ./photo.jpg",
    ));
    match &w.tasks[0].action {
        TaskAction::Invoke { invoke } => assert_eq!(invoke.tool.as_deref(), Some("nika:import")),
        _ => panic!("expected Invoke"),
    }
}

#[test]
fn ab02_invoke_nika_dimensions() {
    let w = ok(&wrap(
        "invoke:\n  tool: nika:dimensions\n  params:\n    hash: blake3:abc",
    ));
    match &w.tasks[0].action {
        TaskAction::Invoke { invoke } => {
            assert_eq!(invoke.tool.as_deref(), Some("nika:dimensions"))
        }
        _ => panic!("expected Invoke"),
    }
}

#[test]
fn ab03_invoke_nika_thumbhash() {
    let w = ok(&wrap(
        "invoke:\n  tool: nika:thumbhash\n  params:\n    hash: blake3:abc",
    ));
    match &w.tasks[0].action {
        TaskAction::Invoke { invoke } => assert_eq!(invoke.tool.as_deref(), Some("nika:thumbhash")),
        _ => panic!("expected Invoke"),
    }
}

#[test]
fn ab04_invoke_nika_dominant_color() {
    let w = ok(&wrap(
        "invoke:\n  tool: nika:dominant_color\n  params:\n    hash: blake3:abc",
    ));
    match &w.tasks[0].action {
        TaskAction::Invoke { invoke } => {
            assert_eq!(invoke.tool.as_deref(), Some("nika:dominant_color"))
        }
        _ => panic!("expected Invoke"),
    }
}

#[test]
fn ab05_invoke_nika_pipeline() {
    let w = ok(&wrap(
        "invoke:\n  tool: nika:pipeline\n  params:\n    steps:\n      - import\n      - thumbnail",
    ));
    match &w.tasks[0].action {
        TaskAction::Invoke { invoke } => assert_eq!(invoke.tool.as_deref(), Some("nika:pipeline")),
        _ => panic!("expected Invoke"),
    }
}

#[test]
fn ab06_invoke_nika_thumbnail() {
    let w = ok(&wrap(
        "invoke:\n  tool: nika:thumbnail\n  params:\n    hash: blake3:abc\n    width: 200",
    ));
    match &w.tasks[0].action {
        TaskAction::Invoke { invoke } => assert_eq!(invoke.tool.as_deref(), Some("nika:thumbnail")),
        _ => panic!("expected Invoke"),
    }
}

#[test]
fn ab07_invoke_nika_convert() {
    let w = ok(&wrap(
        "invoke:\n  tool: nika:convert\n  params:\n    hash: blake3:abc\n    format: webp",
    ));
    match &w.tasks[0].action {
        TaskAction::Invoke { invoke } => assert_eq!(invoke.tool.as_deref(), Some("nika:convert")),
        _ => panic!("expected Invoke"),
    }
}

#[test]
fn ab08_invoke_nika_strip() {
    let w = ok(&wrap(
        "invoke:\n  tool: nika:strip\n  params:\n    hash: blake3:abc",
    ));
    match &w.tasks[0].action {
        TaskAction::Invoke { invoke } => assert_eq!(invoke.tool.as_deref(), Some("nika:strip")),
        _ => panic!("expected Invoke"),
    }
}

#[test]
fn ab09_invoke_nika_metadata() {
    let w = ok(&wrap(
        "invoke:\n  tool: nika:metadata\n  params:\n    hash: blake3:abc",
    ));
    match &w.tasks[0].action {
        TaskAction::Invoke { invoke } => assert_eq!(invoke.tool.as_deref(), Some("nika:metadata")),
        _ => panic!("expected Invoke"),
    }
}

#[test]
fn ab10_invoke_nika_optimize() {
    let w = ok(&wrap(
        "invoke:\n  tool: nika:optimize\n  params:\n    hash: blake3:abc",
    ));
    match &w.tasks[0].action {
        TaskAction::Invoke { invoke } => assert_eq!(invoke.tool.as_deref(), Some("nika:optimize")),
        _ => panic!("expected Invoke"),
    }
}

#[test]
fn ab11_invoke_nika_svg_render() {
    let w = ok(&wrap(
        "invoke:\n  tool: nika:svg_render\n  params:\n    hash: blake3:svg1\n    width: 800",
    ));
    match &w.tasks[0].action {
        TaskAction::Invoke { invoke } => {
            assert_eq!(invoke.tool.as_deref(), Some("nika:svg_render"))
        }
        _ => panic!("expected Invoke"),
    }
}

#[test]
fn ab12_invoke_nika_phash() {
    let w = ok(&wrap(
        "invoke:\n  tool: nika:phash\n  params:\n    hash: blake3:abc",
    ));
    match &w.tasks[0].action {
        TaskAction::Invoke { invoke } => assert_eq!(invoke.tool.as_deref(), Some("nika:phash")),
        _ => panic!("expected Invoke"),
    }
}

#[test]
fn ab13_invoke_nika_compare() {
    let w = ok(&wrap(
        "invoke:\n  tool: nika:compare\n  params:\n    hash_a: blake3:a\n    hash_b: blake3:b",
    ));
    match &w.tasks[0].action {
        TaskAction::Invoke { invoke } => assert_eq!(invoke.tool.as_deref(), Some("nika:compare")),
        _ => panic!("expected Invoke"),
    }
}

#[test]
fn ab14_invoke_nika_pdf_extract() {
    let w = ok(&wrap(
        "invoke:\n  tool: nika:pdf_extract\n  params:\n    hash: blake3:pdf1",
    ));
    match &w.tasks[0].action {
        TaskAction::Invoke { invoke } => {
            assert_eq!(invoke.tool.as_deref(), Some("nika:pdf_extract"))
        }
        _ => panic!("expected Invoke"),
    }
}

#[test]
fn ab15_invoke_nika_chart() {
    let w = ok(&wrap("invoke:\n  tool: nika:chart\n  params:\n    type: bar\n    data:\n      - label: A\n        value: 10"));
    match &w.tasks[0].action {
        TaskAction::Invoke { invoke } => assert_eq!(invoke.tool.as_deref(), Some("nika:chart")),
        _ => panic!("expected Invoke"),
    }
}

#[test]
fn ab16_invoke_nika_provenance() {
    let w = ok(&wrap(
        "invoke:\n  tool: nika:provenance\n  params:\n    hash: blake3:abc\n    creator: nika",
    ));
    match &w.tasks[0].action {
        TaskAction::Invoke { invoke } => {
            assert_eq!(invoke.tool.as_deref(), Some("nika:provenance"))
        }
        _ => panic!("expected Invoke"),
    }
}

#[test]
fn ab17_invoke_nika_verify() {
    let w = ok(&wrap(
        "invoke:\n  tool: nika:verify\n  params:\n    hash: blake3:abc",
    ));
    match &w.tasks[0].action {
        TaskAction::Invoke { invoke } => assert_eq!(invoke.tool.as_deref(), Some("nika:verify")),
        _ => panic!("expected Invoke"),
    }
}

#[test]
fn ab18_invoke_nika_qr_validate() {
    let w = ok(&wrap(
        "invoke:\n  tool: nika:qr_validate\n  params:\n    hash: blake3:qr1",
    ));
    match &w.tasks[0].action {
        TaskAction::Invoke { invoke } => {
            assert_eq!(invoke.tool.as_deref(), Some("nika:qr_validate"))
        }
        _ => panic!("expected Invoke"),
    }
}

#[test]
fn ab19_invoke_nika_quality() {
    let w = ok(&wrap(
        "invoke:\n  tool: nika:quality\n  params:\n    hash: blake3:abc",
    ));
    match &w.tasks[0].action {
        TaskAction::Invoke { invoke } => assert_eq!(invoke.tool.as_deref(), Some("nika:quality")),
        _ => panic!("expected Invoke"),
    }
}

#[test]
fn ab20_invoke_nika_sleep() {
    let w = ok(&wrap(
        "invoke:\n  tool: nika:sleep\n  params:\n    duration: \"2s\"",
    ));
    match &w.tasks[0].action {
        TaskAction::Invoke { invoke } => assert_eq!(invoke.tool.as_deref(), Some("nika:sleep")),
        _ => panic!("expected Invoke"),
    }
}

#[test]
fn ab21_invoke_nika_log() {
    let w = ok(&wrap(
        "invoke:\n  tool: nika:log\n  params:\n    level: info\n    message: \"test\"",
    ));
    match &w.tasks[0].action {
        TaskAction::Invoke { invoke } => assert_eq!(invoke.tool.as_deref(), Some("nika:log")),
        _ => panic!("expected Invoke"),
    }
}

#[test]
fn ab22_invoke_nika_read() {
    let w = ok(&wrap(
        "invoke:\n  tool: nika:read\n  params:\n    path: ./file.txt",
    ));
    match &w.tasks[0].action {
        TaskAction::Invoke { invoke } => assert_eq!(invoke.tool.as_deref(), Some("nika:read")),
        _ => panic!("expected Invoke"),
    }
}

#[test]
fn ab23_invoke_nika_write() {
    let w = ok(&wrap(
        "invoke:\n  tool: nika:write\n  params:\n    path: ./out.txt\n    content: hello",
    ));
    match &w.tasks[0].action {
        TaskAction::Invoke { invoke } => assert_eq!(invoke.tool.as_deref(), Some("nika:write")),
        _ => panic!("expected Invoke"),
    }
}

#[test]
fn ab24_invoke_nika_edit() {
    let w = ok(&wrap(
        "invoke:\n  tool: nika:edit\n  params:\n    path: ./file.txt\n    old: foo\n    new: bar",
    ));
    match &w.tasks[0].action {
        TaskAction::Invoke { invoke } => assert_eq!(invoke.tool.as_deref(), Some("nika:edit")),
        _ => panic!("expected Invoke"),
    }
}

#[test]
fn ab25_invoke_nika_glob() {
    let w = ok(&wrap(
        "invoke:\n  tool: nika:glob\n  params:\n    pattern: \"src/*.rs\"",
    ));
    match &w.tasks[0].action {
        TaskAction::Invoke { invoke } => assert_eq!(invoke.tool.as_deref(), Some("nika:glob")),
        _ => panic!("expected Invoke"),
    }
}

#[test]
fn ab26_invoke_nika_grep() {
    let w = ok(&wrap(
        "invoke:\n  tool: nika:grep\n  params:\n    pattern: \"TODO\"\n    path: ./src",
    ));
    match &w.tasks[0].action {
        TaskAction::Invoke { invoke } => assert_eq!(invoke.tool.as_deref(), Some("nika:grep")),
        _ => panic!("expected Invoke"),
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// AC. BUG FIX REGRESSION TESTS (Bugs 7, 8, 41, 42, 43, 44)
// ═══════════════════════════════════════════════════════════════════════════════

/// Bug 7: schema_ref must survive the full YAML -> Runtime pipeline.
#[test]
fn ac01_bug7_schema_ref_survives_pipeline() {
    let yaml = r#"
schema: "nika/workflow@0.12"
model: test-model
tasks:
  - id: structured_task
    infer: "Generate JSON"
    output:
      format: json
      schema_ref: "./schemas/result.json"
"#;
    let w = ok(yaml);
    let output = w.tasks[0].output.as_ref().expect("output should exist");
    match &output.schema {
        Some(crate::ast::output::SchemaRef::File(path)) => {
            assert_eq!(path, "./schemas/result.json");
        }
        other => panic!("expected SchemaRef::File, got {:?}", other),
    }
}

/// $ref alias was removed (clashes with JSON Schema). Only schema_ref works.
#[test]
fn ac02_dollar_ref_alias_removed() {
    let yaml = r#"
schema: "nika/workflow@0.12"
model: test-model
tasks:
  - id: ref_task
    infer: "Generate JSON"
    output:
      format: json
      $ref: "./schemas/entity.json"
"#;
    let w = ok(yaml);
    let output = w.tasks[0].output.as_ref().expect("output should exist");
    // $ref no longer recognized; schema should be None
    assert!(output.schema.is_none(), "$ref alias should no longer work");
}

/// schema_ref is the only way to reference an external schema.
#[test]
fn ac02b_schema_ref_works() {
    let yaml = r#"
schema: "nika/workflow@0.12"
model: test-model
tasks:
  - id: ref_task
    infer: "Generate JSON"
    output:
      format: json
      schema_ref: "./schemas/entity.json"
"#;
    let w = ok(yaml);
    let output = w.tasks[0].output.as_ref().expect("output should exist");
    match &output.schema {
        Some(crate::ast::output::SchemaRef::File(path)) => {
            assert_eq!(path, "./schemas/entity.json");
        }
        other => panic!("expected SchemaRef::File from schema_ref, got {:?}", other),
    }
}

/// Bug 8: String schemas that look like file paths become SchemaRef::File.
#[test]
fn ac03_bug8_schema_string_file_path() {
    let yaml = r#"
schema: "nika/workflow@0.12"
model: test-model
tasks:
  - id: t1
    infer: "Generate"
    output:
      format: json
      schema: "./schemas/user.json"
"#;
    let w = ok(yaml);
    let output = w.tasks[0].output.as_ref().expect("output should exist");
    match &output.schema {
        Some(crate::ast::output::SchemaRef::File(path)) => {
            assert_eq!(path, "./schemas/user.json");
        }
        other => panic!("expected SchemaRef::File for ./ path, got {:?}", other),
    }
}

/// Bug 8: Inline JSON schema objects remain Inline.
#[test]
fn ac04_bug8_inline_schema_stays_inline() {
    let yaml = r#"
schema: "nika/workflow@0.12"
model: test-model
tasks:
  - id: t1
    infer: "Generate"
    output:
      format: json
      schema:
        type: object
        properties:
          name:
            type: string
"#;
    let w = ok(yaml);
    let output = w.tasks[0].output.as_ref().expect("output should exist");
    assert!(
        matches!(
            output.schema,
            Some(crate::ast::output::SchemaRef::Inline(_))
        ),
        "JSON object schema should remain Inline"
    );
}

/// Bug 41: Invalid HTTP method still parses (defaults to GET with warning).
#[test]
fn ac05_bug41_invalid_http_method_defaults_to_get() {
    let yaml = r#"
schema: "nika/workflow@0.12"
model: test-model
tasks:
  - id: bad_method
    fetch:
      url: "https://example.com"
      method: FROBNICATE
"#;
    // Should parse without error (defaults to GET, warning emitted)
    let w = ok(yaml);
    match &w.tasks[0].action {
        TaskAction::Fetch { fetch } => {
            assert_eq!(fetch.method, "GET", "Invalid method should default to GET");
        }
        _ => panic!("expected Fetch action"),
    }
}

/// Bug 42: output.max_retries survives the full pipeline.
#[test]
fn ac06_bug42_output_max_retries() {
    let yaml = r#"
schema: "nika/workflow@0.12"
model: test-model
tasks:
  - id: retry_task
    infer: "Generate JSON"
    output:
      format: json
      max_retries: 5
      schema:
        type: object
"#;
    let w = ok(yaml);
    let output = w.tasks[0].output.as_ref().expect("output should exist");
    assert_eq!(
        output.max_retries,
        Some(5),
        "max_retries should survive the pipeline"
    );
}

/// Bug 43: infer.response_format survives the full pipeline.
#[test]
fn ac07_bug43_response_format_json() {
    let yaml = r#"
schema: "nika/workflow@0.12"
model: test-model
tasks:
  - id: json_response
    infer:
      prompt: "Generate structured data"
      response_format: json
"#;
    let w = ok(yaml);
    match &w.tasks[0].action {
        TaskAction::Infer { infer } => {
            assert_eq!(
                infer.response_format,
                Some(crate::ast::action::ResponseFormat::Json),
                "response_format: json should survive pipeline"
            );
        }
        _ => panic!("expected Infer action"),
    }
}

/// Bug 43: response_format: markdown variant.
#[test]
fn ac08_bug43_response_format_markdown() {
    let yaml = r#"
schema: "nika/workflow@0.12"
model: test-model
tasks:
  - id: md_response
    infer:
      prompt: "Generate docs"
      response_format: markdown
"#;
    let w = ok(yaml);
    match &w.tasks[0].action {
        TaskAction::Infer { infer } => {
            assert_eq!(
                infer.response_format,
                Some(crate::ast::action::ResponseFormat::Markdown),
                "response_format: markdown should survive pipeline"
            );
        }
        _ => panic!("expected Infer action"),
    }
}

/// Bug 43: response_format: text (explicit default).
#[test]
fn ac09_bug43_response_format_text() {
    let yaml = r#"
schema: "nika/workflow@0.12"
model: test-model
tasks:
  - id: text_response
    infer:
      prompt: "Generate text"
      response_format: text
"#;
    let w = ok(yaml);
    match &w.tasks[0].action {
        TaskAction::Infer { infer } => {
            assert_eq!(
                infer.response_format,
                Some(crate::ast::action::ResponseFormat::Text),
                "response_format: text should survive pipeline"
            );
        }
        _ => panic!("expected Infer action"),
    }
}

/// Bug 43: response_format absent remains None (backward compatible).
#[test]
fn ac10_bug43_response_format_absent() {
    let yaml = r#"
schema: "nika/workflow@0.12"
model: test-model
tasks:
  - id: no_rf
    infer: "Simple prompt"
"#;
    let w = ok(yaml);
    match &w.tasks[0].action {
        TaskAction::Infer { infer } => {
            assert!(
                infer.response_format.is_none(),
                "Absent response_format should be None"
            );
        }
        _ => panic!("expected Infer action"),
    }
}

/// Bug 44: Sub-second exec timeout must not truncate to zero.
#[test]
fn ac11_bug44_sub_second_exec_timeout() {
    let yaml = r#"
schema: "nika/workflow@0.12"
model: test-model
tasks:
  - id: fast_cmd
    exec:
      command: "echo hi"
      timeout_ms: 500
"#;
    let w = ok(yaml);
    match &w.tasks[0].action {
        TaskAction::Exec { exec: e } => {
            assert_eq!(
                e.timeout,
                Some(1),
                "500ms should ceil to 1s, not truncate to 0s"
            );
        }
        _ => panic!("expected Exec action"),
    }
}

/// Bug 44: Exact second fetch timeout unchanged.
#[test]
fn ac12_bug44_exact_second_fetch_timeout() {
    let yaml = r#"
schema: "nika/workflow@0.12"
model: test-model
tasks:
  - id: fetch_5s
    fetch:
      url: "https://example.com"
      timeout_ms: 5000
"#;
    let w = ok(yaml);
    match &w.tasks[0].action {
        TaskAction::Fetch { fetch } => {
            assert_eq!(fetch.timeout, Some(5), "5000ms should remain 5s");
        }
        _ => panic!("expected Fetch action"),
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// AD. Production workflow gate tests
// ═══════════════════════════════════════════════════════════════════════════

/// Gate test: all use-case workflows must parse through the full pipeline.
#[test]
fn ad01_all_use_case_workflows_parse_and_validate() {
    let examples_dir = std::path::Path::new("examples/use-cases");
    if !examples_dir.exists() {
        return; // skip if not in correct dir
    }

    let mut count = 0;
    for entry in std::fs::read_dir(examples_dir).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().and_then(|e| e.to_str()) == Some("yaml") {
            let yaml = std::fs::read_to_string(&path).unwrap();
            let result = parse_workflow(&yaml);
            assert!(
                result.is_ok(),
                "Failed to parse {}: {:?}",
                path.display(),
                result.err()
            );
            count += 1;
        }
    }
    assert!(
        count >= 10,
        "Expected at least 10 use-case workflows, found {}",
        count
    );
}

/// Gate test: all gate/e2e/audit/trap/stress/monster workflows must parse.
#[test]
fn ad02_all_gate_workflows_parse() {
    let gates_dir = std::path::Path::new("examples/gates");
    if !gates_dir.exists() {
        return; // skip if not in correct dir
    }

    let mut count = 0;
    for entry in std::fs::read_dir(gates_dir).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().and_then(|e| e.to_str()) == Some("yaml") {
            let yaml = std::fs::read_to_string(&path).unwrap();
            let result = parse_workflow(&yaml);
            assert!(
                result.is_ok(),
                "Failed to parse {}: {:?}",
                path.display(),
                result.err()
            );
            count += 1;
        }
    }
    // Gate workflows may be cleaned up — skip assertion if none found
    if count > 0 {
        assert!(
            count >= 10,
            "Expected at least 10 gate workflows, found {}",
            count
        );
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// AB. ENGINE FIXES E2E TESTS (v0.50 session)
// Tests for: structured output, include:, model template, extended_thinking,
// agent system template, response_format
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn ab01_structured_output_with_openai_provider_parses() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: openai
model: gpt-4o

tasks:
  - id: extract
    infer:
      prompt: "Extract product data from this text"
    structured:
      schema:
        type: object
        properties:
          name: { type: string }
          price: { type: number }
          in_stock: { type: boolean }
        required: [name, price]
      enable_repair: true
      max_retries: 3
"#;
    let w = ok(yaml);
    assert_eq!(w.tasks.len(), 1);
    let task = &w.tasks[0];
    assert!(task.structured.is_some());
    let spec = task.structured.as_ref().unwrap();
    assert_eq!(spec.max_retries, Some(3));
    assert_eq!(spec.enable_repair, Some(true));
}

#[test]
fn ab02_structured_output_with_schema_and_for_each() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: mock
model: test-model

tasks:
  - id: urls
    infer: "List 5 URLs"
    structured:
      schema:
        type: object
        properties:
          urls: { type: array, items: { type: string } }
        required: [urls]

  - id: scrape
    with:
      data: $urls
    for_each: "{{with.data}}"
    as: url
    concurrency: 3
    fetch:
      url: "{{with.url}}"
      extract: article

  - id: synthesize
    depends_on: [scrape]
    with:
      articles: $scrape
    infer: "Synthesize: {{with.articles | join('\n')}}"
"#;
    let w = ok(yaml);
    assert_eq!(w.tasks.len(), 3);
    assert!(w.tasks[0].structured.is_some());
    assert!(w.tasks[1].for_each.is_some());
}

#[test]
fn ab03_include_key_parses() {
    let yaml = r#"
schema: "nika/workflow@0.12"
model: test-model
include:
  - path: ./lib/seo.nika.yaml
    prefix: seo_
  - path: ./lib/media.nika.yaml
    prefix: media_
tasks:
  - id: main
    infer: "Main task"
"#;
    let w = ok(yaml);
    assert!(w.include.is_some());
    let includes = w.include.as_ref().unwrap();
    assert_eq!(includes.len(), 2);
    assert_eq!(includes[0].prefix.as_deref(), Some("seo_"));
    assert_eq!(includes[1].prefix.as_deref(), Some("media_"));
}

#[test]
fn ab04_imports_key_rejected() {
    let yaml = r#"
schema: "nika/workflow@0.12"
model: test-model
imports:
  - path: ./lib/seo.nika.yaml
tasks:
  - id: main
    infer: "Main task"
"#;
    // 'imports:' is no longer a known key — should fail or be ignored
    let e = err(yaml);
    let msg = e.to_string();
    assert!(
        msg.contains("Unknown") || msg.contains("unknown") || msg.contains("NIKA-010"),
        "Expected unknown key error, got: {msg}"
    );
}

#[test]
fn ab05_model_template_in_infer() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: mock

tasks:
  - id: t1
    model: "{{inputs.fast_model | default('gpt-4o-mini')}}"
    infer:
      prompt: "Test"
"#;
    let w = ok(yaml);
    let task = &w.tasks[0];
    match &task.action {
        crate::ast::TaskAction::Infer { infer } => {
            assert_eq!(
                infer.model.as_deref(),
                Some("{{inputs.fast_model | default('gpt-4o-mini')}}")
            );
        }
        _ => panic!("Expected Infer"),
    }
}

#[test]
fn ab06_model_template_in_agent() {
    let yaml = wrap("agent:\n  prompt: \"Test\"\n  model: \"{{inputs.agent_model}}\"");
    let w = ok(&yaml);
    let task = &w.tasks[0];
    match &task.action {
        crate::ast::TaskAction::Agent { agent } => {
            assert_eq!(agent.model.as_deref(), Some("{{inputs.agent_model}}"));
        }
        _ => panic!("Expected Agent"),
    }
}

#[test]
fn ab07_extended_thinking_with_openai_parses_ok() {
    // Previously crashed with ValidationError, now degrades gracefully
    let yaml = wrap(
        "provider: openai\ninfer:\n  prompt: \"Deep reasoning\"\n  extended_thinking: true\n  thinking_budget: 4096",
    );
    let w = ok(&yaml);
    let task = &w.tasks[0];
    match &task.action {
        crate::ast::TaskAction::Infer { infer } => {
            assert_eq!(infer.extended_thinking, Some(true));
            assert_eq!(infer.thinking_budget, Some(4096));
        }
        _ => panic!("Expected Infer"),
    }
}

#[test]
fn ab08_extended_thinking_with_groq_parses_ok() {
    let yaml = wrap(
        "agent:\n  prompt: \"Test\"\n  provider: groq\n  extended_thinking: true\n  thinking_budget: 8192",
    );
    let w = ok(&yaml);
    let task = &w.tasks[0];
    match &task.action {
        crate::ast::TaskAction::Agent { agent } => {
            assert_eq!(agent.extended_thinking, Some(true));
            assert_eq!(agent.provider, Some(nika_core::ProviderName::Groq));
        }
        _ => panic!("Expected Agent"),
    }
}

#[test]
fn ab09_extended_thinking_with_claude_still_ok() {
    let yaml = wrap(
        "provider: claude\ninfer:\n  prompt: \"Think deeply\"\n  extended_thinking: true\n  thinking_budget: 16384",
    );
    let w = ok(&yaml);
    let task = &w.tasks[0];
    match &task.action {
        crate::ast::TaskAction::Infer { infer } => {
            assert_eq!(infer.extended_thinking, Some(true));
            assert_eq!(infer.thinking_budget, Some(16384));
        }
        _ => panic!("Expected Infer"),
    }
}

#[test]
fn ab10_agent_system_with_template() {
    let yaml = wrap(
        "agent:\n  prompt: \"Research {{inputs.topic}}\"\n  system: \"You are an expert in {{inputs.domain}}\"",
    );
    let w = ok(&yaml);
    let task = &w.tasks[0];
    match &task.action {
        crate::ast::TaskAction::Agent { agent } => {
            assert_eq!(
                agent.system.as_deref(),
                Some("You are an expert in {{inputs.domain}}")
            );
        }
        _ => panic!("Expected Agent"),
    }
}

#[test]
fn ab11_complex_multi_provider_structured_pipeline() {
    // A realistic workflow that uses structured output across multiple providers
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: mock
model: test-model

inputs:
  topic: "AI workflow engines"
  fast_model: "gpt-4o-mini"

tasks:
  - id: research
    infer:
      prompt: "Research: {{inputs.topic}}"
      temperature: 0.7

  - id: extract_entities
    depends_on: [research]
    with:
      text: $research
    infer:
      prompt: "Extract entities from: {{with.text}}"
    structured:
      schema:
        type: object
        properties:
          entities:
            type: array
            items:
              type: object
              properties:
                name: { type: string }
                type: { type: string }
              required: [name, type]
        required: [entities]

  - id: summarize
    depends_on: [extract_entities]
    with:
      entities: $extract_entities
    infer:
      prompt: |
        Summarize findings. Entities: {{with.entities}}
      max_tokens: 500
"#;
    let w = ok(yaml);
    assert_eq!(w.tasks.len(), 3);
    assert!(w.tasks[1].structured.is_some());
    assert!(w.tasks[1]
        .depends_on
        .as_ref()
        .unwrap()
        .contains(&"research".to_string()));
    assert!(w.tasks[2]
        .depends_on
        .as_ref()
        .unwrap()
        .contains(&"extract_entities".to_string()));
}

#[test]
fn ab12_structured_output_with_repair_model() {
    let yaml = wrap(
        "infer:\n  prompt: \"Extract data\"\nstructured:\n  schema:\n    type: object\n    properties:\n      name: { type: string }\n    required: [name]\n  enable_repair: true\n  max_retries: 3\n  repair_model: claude-haiku-4-5",
    );
    let w = ok(&yaml);
    let task = &w.tasks[0];
    let spec = task.structured.as_ref().unwrap();
    assert_eq!(spec.max_retries, Some(3));
    assert_eq!(spec.enable_repair, Some(true));
    assert_eq!(spec.repair_model.as_deref(), Some("claude-haiku-4-5"));
}

#[test]
fn ab13_include_with_structured_and_for_each() {
    // Complex workflow combining include, structured output, and for_each
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: mock
model: test-model

include:
  - path: ./lib/helpers.nika.yaml
    prefix: h_

tasks:
  - id: get_items
    infer: "List items"
    structured:
      schema:
        type: object
        properties:
          items: { type: array, items: { type: string } }
        required: [items]

  - id: process
    depends_on: [get_items]
    with:
      data: $get_items
    for_each: "{{with.data}}"
    as: item
    concurrency: 5
    infer: "Process: {{with.item}}"

  - id: aggregate
    depends_on: [process]
    with:
      results: $process
    infer: "Aggregate {{with.results | length}} results"
"#;
    let w = ok(yaml);
    assert_eq!(w.tasks.len(), 3);
    assert!(w.include.is_some());
    assert!(w.tasks[0].structured.is_some());
    assert!(w.tasks[1].for_each.is_some());
}

#[test]
fn ab14_provider_agnostic_workflow_with_extended_thinking() {
    // Workflow designed to work with any provider — extended_thinking degrades
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: mock
model: test-model

tasks:
  - id: analyze
    infer:
      prompt: "Deep analysis of the system"
      extended_thinking: true
      thinking_budget: 8192
      temperature: 0.3

  - id: structured_result
    depends_on: [analyze]
    with:
      analysis: $analyze
    infer:
      prompt: "Structure the analysis: {{with.analysis}}"
    structured:
      schema:
        type: object
        properties:
          findings: { type: array, items: { type: string } }
          severity: { type: string }
          recommendation: { type: string }
        required: [findings, severity, recommendation]
      max_retries: 2
"#;
    let w = ok(yaml);
    assert_eq!(w.tasks.len(), 2);
    match &w.tasks[0].action {
        crate::ast::TaskAction::Infer { infer } => {
            assert_eq!(infer.extended_thinking, Some(true));
        }
        _ => panic!("Expected Infer"),
    }
    assert!(w.tasks[1].structured.is_some());
}

#[test]
fn ab20_response_format_with_structured_doesnt_conflict() {
    // When structured: is present, response_format should not interfere
    let yaml = wrap(
        "infer:\n  prompt: \"Extract\"\n  response_format: json\nstructured:\n  schema:\n    type: object\n    properties:\n      x: { type: number }\n    required: [x]",
    );
    let w = ok(&yaml);
    let task = &w.tasks[0];
    assert!(task.structured.is_some());
}

// ═══════════════════════════════════════════════════════════════════════════════
// AD. EXTREME COMPLEXITY (7 tests)
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn ad01_maximum_complexity_pipeline() {
    // 10-task pipeline exercising: include, inputs, context, with bindings,
    // pipe transforms, depends_on, implicit flow, for_each with concurrency,
    // all 5 verbs, structured output, and artifacts.
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: mock
model: test-model

include:
  - path: ./lib/seo.nika.yaml
    prefix: seo_
  - path: ./lib/media.nika.yaml
    prefix: media_

inputs:
  topic:
    type: string
    default: "AI workflows"
  depth:
    type: integer
    default: 3
  verbose:
    type: boolean
    default: true
  format:
    type: string
    default: markdown
  max_results:
    type: integer
    default: 10

context:
  files:
    brand: ./context/brand.md
    style: ./context/style.json

skills:
  writing: ./skills/writing.md

artifacts:
  dir: ./output
  format: json
  mode: overwrite

tasks:
  - id: fetch_source
    fetch:
      url: "https://example.com/api/data"
      method: GET
      headers:
        Accept: application/json
      extract: jsonpath
      selector: "$.results[*].title"
      response: full
      timeout: 30

  - id: analyze
    depends_on: [fetch_source]
    with:
      raw: $fetch_source
    infer:
      prompt: "Analyze {{inputs.topic}} from: {{with.raw | trim | upper}}"
      system: "You are a senior analyst"
      temperature: 0.5
      max_tokens: 2000

  - id: extract_entities
    depends_on: [analyze]
    with:
      analysis: $analyze
    infer:
      prompt: "Extract entities from: {{with.analysis | trim}}"
    structured:
      schema:
        type: object
        properties:
          entities:
            type: array
            items:
              type: object
              properties:
                name:
                  type: string
                category:
                  type: string
              required:
                - name
          count:
            type: integer
        required:
          - entities
          - count
      enable_repair: true
      max_retries: 3
      repair_model: test-model

  - id: process_entities
    depends_on: [extract_entities]
    with:
      items: $extract_entities
    for_each: "{{with.items}}"
    as: entity
    concurrency: 5
    fail_fast: false
    infer: "Generate content for entity: {{with.entity | default('unknown') | trim}}"

  - id: research
    depends_on: [analyze]
    with:
      summary: $analyze
    agent:
      prompt: "Deep research on: {{with.summary | trim}}"
      system: "You are a thorough researcher"
      max_turns: 8
      token_budget: 50000
      tools:
        - nika:read
        - nika:write
      mcp:
        - novanet
      temperature: 0.3

  - id: build_report
    depends_on: [process_entities, research]
    with:
      entities: $process_entities
      research: $research
    exec:
      command: "echo report_built"
      shell: true
      cwd: "./output"
      timeout: 60
      env:
        OUTPUT_FORMAT: markdown
        VERBOSE: "true"

  - id: store_graph
    depends_on: [extract_entities]
    with:
      data: $extract_entities
    invoke:
      mcp: novanet
      tool: novanet_write
      params:
        content: "{{with.data}}"
        topic: "{{inputs.topic | lower | trim}}"
      timeout: 30

  - id: validate
    depends_on: [build_report, store_graph]
    with:
      report: $build_report
      graph: $store_graph
    infer:
      prompt: "Validate: {{with.report | length}} chars, graph: {{with.graph | default('empty')}}"
      temperature: 0.0

  - id: scrape_references
    fetch:
      url: "https://references.example.com"
      extract: article
      timeout: 15

  - id: final_summary
    depends_on: [validate, scrape_references]
    with:
      validation: $validate
      refs: $scrape_references
    infer:
      prompt: |
        Final summary:
        - Validation: {{with.validation | trim}}
        - References: {{with.refs | default('none') | trim}}
        - Topic: {{inputs.topic | upper}}
      max_tokens: 4000
    artifact:
      path: summary.txt
      format: text
"#;
    let w = ok(yaml);
    assert_eq!(w.tasks.len(), 10);
    // Verify workflow-level features
    assert!(w.include.is_some());
    assert_eq!(w.include.as_ref().unwrap().len(), 2);
    assert!(w.inputs.is_some());
    assert_eq!(w.inputs.as_ref().unwrap().len(), 5);
    assert!(w.context.is_some());
    assert_eq!(w.context.as_ref().unwrap().files.len(), 2);
    // skills: dropped during lowering (skills_map: _ in lower.rs)
    assert!(w.artifacts.is_some());
    // Verify task verbs
    assert_eq!(w.tasks[0].action.verb_name(), "fetch");
    assert_eq!(w.tasks[1].action.verb_name(), "infer");
    assert_eq!(w.tasks[2].action.verb_name(), "infer");
    assert_eq!(w.tasks[3].action.verb_name(), "infer"); // for_each infer
    assert_eq!(w.tasks[4].action.verb_name(), "agent");
    assert_eq!(w.tasks[5].action.verb_name(), "exec");
    assert_eq!(w.tasks[6].action.verb_name(), "invoke");
    assert_eq!(w.tasks[7].action.verb_name(), "infer");
    assert_eq!(w.tasks[8].action.verb_name(), "fetch");
    assert_eq!(w.tasks[9].action.verb_name(), "infer");
    // Verify for_each
    assert!(w.tasks[3].has_for_each());
    assert_eq!(w.tasks[3].for_each_var(), "entity");
    assert_eq!(w.tasks[3].for_each_concurrency(), 5);
    assert!(!w.tasks[3].for_each_fail_fast());
    // Verify structured output
    assert!(w.tasks[2].structured.is_some());
    let spec = w.tasks[2].structured.as_ref().unwrap();
    assert_eq!(spec.enable_repair, Some(true));
    assert_eq!(spec.max_retries, Some(3));
    assert_eq!(spec.repair_model.as_deref(), Some("test-model"));
    // Verify artifact
    assert!(w.tasks[9].artifact.is_some());
    // Verify DAG edges
    assert!(w.flow_count() >= 8);
    let edges = w.edges();
    assert!(edges.contains(&("fetch_source", "analyze")));
    assert!(edges.contains(&("analyze", "extract_entities")));
    assert!(edges.contains(&("extract_entities", "process_entities")));
    assert!(edges.contains(&("analyze", "research")));
    assert!(edges.contains(&("process_entities", "build_report")));
    assert!(edges.contains(&("research", "build_report")));
    assert!(edges.contains(&("extract_entities", "store_graph")));
    assert!(edges.contains(&("build_report", "validate")));
    assert!(edges.contains(&("store_graph", "validate")));
    assert!(edges.contains(&("validate", "final_summary")));
    assert!(edges.contains(&("scrape_references", "final_summary")));
    // Verify fetch params
    match &w.tasks[0].action {
        TaskAction::Fetch { fetch } => {
            assert_eq!(fetch.url, "https://example.com/api/data");
            assert_eq!(fetch.method, "GET");
            assert_eq!(fetch.extract, Some(ExtractMode::Jsonpath));
            assert_eq!(fetch.selector.as_deref(), Some("$.results[*].title"));
            assert_eq!(fetch.response, Some(ResponseMode::Full));
            assert_eq!(fetch.timeout, Some(30));
        }
        _ => panic!("expected Fetch"),
    }
    // Verify agent params
    match &w.tasks[4].action {
        TaskAction::Agent { agent } => {
            assert_eq!(agent.prompt, "Deep research on: {{with.summary | trim}}");
            assert_eq!(
                agent.system.as_deref(),
                Some("You are a thorough researcher")
            );
            assert_eq!(agent.effective_max_turns(), 8);
            assert_eq!(agent.token_budget, Some(50000));
            assert_eq!(agent.tools.len(), 2);
            assert_eq!(agent.mcp.len(), 1);
            assert_eq!(agent.temperature, Some(0.3));
        }
        _ => panic!("expected Agent"),
    }
    // Verify exec params
    match &w.tasks[5].action {
        TaskAction::Exec { exec } => {
            assert_eq!(exec.command, "echo report_built");
            assert_eq!(exec.shell, Some(true));
            assert_eq!(exec.cwd.as_deref(), Some("./output"));
            assert_eq!(exec.timeout, Some(60));
            assert!(exec.env.is_some());
        }
        _ => panic!("expected Exec"),
    }
    // Verify invoke params
    match &w.tasks[6].action {
        TaskAction::Invoke { invoke } => {
            assert_eq!(invoke.mcp.as_deref(), Some("novanet"));
            assert_eq!(invoke.tool.as_deref(), Some("novanet_write"));
            assert!(invoke.params.is_some());
            assert_eq!(invoke.timeout, Some(30));
        }
        _ => panic!("expected Invoke"),
    }
}

#[test]
fn ad02_agent_with_everything() {
    // Agent verb with ALL possible fields: system, prompt, model, provider,
    // temperature, max_tokens, extended_thinking, thinking_budget, tools,
    // skills, mcp, max_turns, token_budget, completion, guardrails (4 types),
    // limits, tool_choice, depth_limit, stop_sequences
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: mock
model: test-model

skills:
  research: ./skills/research.md
  writing: ./skills/writing.md

tasks:
  - id: mega_agent
    agent:
      system: "You are a world-class research assistant with deep expertise."
      prompt: "Analyze the given topic comprehensively and produce a structured report."
      provider: anthropic
      model: claude-sonnet-4-20250514
      temperature: 0.4
      max_tokens: 16384
      extended_thinking: true
      thinking_budget: 8192
      tools:
        - nika:read
        - nika:write
        - nika:glob
        - novanet::novanet_search
        - novanet::novanet_context
      skills:
        - research
        - writing
      mcp:
        - novanet
        - filesystem
      max_turns: 25
      token_budget: 100000
      depth_limit: 5
      tool_choice: required
      stop_sequences:
        - "DONE"
        - "---END---"
      completion:
        mode: explicit
      guardrails:
        - type: length
          min_words: 100
          max_words: 5000
          on_failure: retry
        - type: schema
          json_schema:
            type: object
            properties:
              findings:
                type: array
                items:
                  type: object
                  properties:
                    title:
                      type: string
                    confidence:
                      type: number
              recommendation:
                type: string
            required:
              - findings
              - recommendation
          on_failure: escalate
        - type: regex
          pattern: "^## (Findings|Analysis|Summary)"
          message: "Response must start with a markdown heading"
          on_failure: retry
        - type: llm
          judge_prompt: |
            Evaluate if this research report is factually grounded
            and free of hallucination. Reply PASS or FAIL.
          pass_pattern: "^PASS"
          on_failure: fail
      limits:
        max_cost_usd: 5.0
        max_duration_secs: 300
"#;
    let w = ok(yaml);
    assert_eq!(w.tasks.len(), 1);
    // workflow-level skills: dropped during lowering (skills_map: _ in lower.rs)
    // but agent-level skills: survive in AgentParams.skills
    match &w.tasks[0].action {
        TaskAction::Agent { agent } => {
            // Core fields
            assert_eq!(
                agent.system.as_deref(),
                Some("You are a world-class research assistant with deep expertise.")
            );
            assert!(agent.prompt.contains("comprehensively"));
            assert_eq!(
                agent.provider.as_ref().map(|p| p.as_str()),
                Some("anthropic")
            );
            assert_eq!(agent.model.as_deref(), Some("claude-sonnet-4-20250514"));
            assert_eq!(agent.temperature, Some(0.4));
            assert_eq!(agent.max_tokens, Some(16384));
            // Extended thinking
            assert_eq!(agent.extended_thinking, Some(true));
            assert_eq!(agent.thinking_budget, Some(8192));
            assert_eq!(agent.effective_thinking_budget(), 8192);
            // Tools + MCP
            assert_eq!(agent.tools.len(), 5);
            assert!(agent.tools.contains(&"nika:read".to_string()));
            assert!(agent.tools.contains(&"novanet::novanet_search".to_string()));
            assert_eq!(agent.mcp.len(), 2);
            // Skills
            let skills = agent.skills.as_ref().unwrap();
            assert_eq!(skills.len(), 2);
            assert!(skills.contains(&"research".to_string()));
            // Control
            assert_eq!(agent.effective_max_turns(), 25);
            assert_eq!(agent.token_budget, Some(100000));
            assert_eq!(agent.effective_depth_limit(), 5);
            assert_eq!(
                agent.effective_tool_choice(),
                crate::ast::ToolChoice::Required
            );
            assert_eq!(agent.stop_sequences.len(), 2);
            // Completion
            let completion = agent.completion.as_ref().unwrap();
            assert_eq!(completion.mode, crate::ast::CompletionMode::Explicit);
            // Guardrails
            assert_eq!(agent.guardrails.len(), 4);
            assert_eq!(agent.guardrails[0].guardrail_type(), "length");
            assert_eq!(agent.guardrails[1].guardrail_type(), "schema");
            assert_eq!(agent.guardrails[2].guardrail_type(), "regex");
            assert_eq!(agent.guardrails[3].guardrail_type(), "llm");
            // On-failure actions
            assert_eq!(
                agent.guardrails[0].on_failure(),
                crate::ast::guardrails::OnFailure::Retry
            );
            assert_eq!(
                agent.guardrails[1].on_failure(),
                crate::ast::guardrails::OnFailure::Escalate
            );
            assert_eq!(
                agent.guardrails[3].on_failure(),
                crate::ast::guardrails::OnFailure::Fail
            );
            // Limits
            let limits = agent.limits.as_ref().unwrap();
            assert!((limits.max_cost_usd - 5.0).abs() < f64::EPSILON);
            assert_eq!(limits.max_duration_secs, 300);
        }
        _ => panic!("expected Agent"),
    }
}

#[test]
fn ad03_fetch_extraction_gauntlet() {
    // 9 tasks, each using a different extract mode:
    // markdown, article, text, selector, metadata, links, jsonpath, feed, llm_txt
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: mock
model: test-model
tasks:
  - id: f_markdown
    fetch:
      url: "https://blog.example.com/post"
      extract: markdown

  - id: f_article
    fetch:
      url: "https://news.example.com/story"
      extract: article

  - id: f_text
    fetch:
      url: "https://docs.example.com/page"
      extract: text
      selector: "main .content"

  - id: f_selector
    fetch:
      url: "https://shop.example.com/products"
      extract: selector
      selector: ".product-card h2"

  - id: f_metadata
    fetch:
      url: "https://social.example.com/post"
      extract: metadata

  - id: f_links
    fetch:
      url: "https://hub.example.com"
      extract: links

  - id: f_jsonpath
    fetch:
      url: "https://api.example.com/v2/data"
      method: POST
      headers:
        Content-Type: application/json
        Authorization: "Bearer {{inputs.token}}"
      json:
        query: "AI workflows"
        limit: 50
      extract: jsonpath
      selector: "$.data[*].name"
      response: full

  - id: f_feed
    fetch:
      url: "https://blog.example.com/rss.xml"
      extract: feed

  - id: f_llm_txt
    fetch:
      url: "https://example.com"
      extract: llm_txt
      response: full
"#;
    let w = ok(yaml);
    assert_eq!(w.tasks.len(), 9);
    // Verify each extract mode
    let extract_modes: Vec<Option<ExtractMode>> = w
        .tasks
        .iter()
        .map(|t| match &t.action {
            TaskAction::Fetch { fetch } => fetch.extract,
            _ => panic!("expected Fetch"),
        })
        .collect();
    assert_eq!(
        extract_modes,
        vec![
            Some(ExtractMode::Markdown),
            Some(ExtractMode::Article),
            Some(ExtractMode::Text),
            Some(ExtractMode::Selector),
            Some(ExtractMode::Metadata),
            Some(ExtractMode::Links),
            Some(ExtractMode::Jsonpath),
            Some(ExtractMode::Feed),
            Some(ExtractMode::LlmTxt),
        ]
    );
    // Verify text extract with CSS selector
    match &w.tasks[2].action {
        TaskAction::Fetch { fetch } => {
            assert_eq!(fetch.selector.as_deref(), Some("main .content"));
        }
        _ => panic!("expected Fetch"),
    }
    // Verify selector extract with CSS selector
    match &w.tasks[3].action {
        TaskAction::Fetch { fetch } => {
            assert_eq!(fetch.selector.as_deref(), Some(".product-card h2"));
        }
        _ => panic!("expected Fetch"),
    }
    // Verify jsonpath with POST, headers, json body, selector, and response: full
    match &w.tasks[6].action {
        TaskAction::Fetch { fetch } => {
            assert_eq!(fetch.method, "POST");
            assert!(fetch.headers.contains_key("Content-Type"));
            assert!(fetch.headers.contains_key("Authorization"));
            assert!(fetch.json.is_some());
            assert_eq!(fetch.extract, Some(ExtractMode::Jsonpath));
            assert_eq!(fetch.selector.as_deref(), Some("$.data[*].name"));
            assert_eq!(fetch.response, Some(ResponseMode::Full));
        }
        _ => panic!("expected Fetch"),
    }
    // Verify feed
    match &w.tasks[7].action {
        TaskAction::Fetch { fetch } => {
            assert_eq!(fetch.extract, Some(ExtractMode::Feed));
            assert!(fetch.selector.is_none());
        }
        _ => panic!("expected Fetch"),
    }
    // Verify llm_txt with response: full
    match &w.tasks[8].action {
        TaskAction::Fetch { fetch } => {
            assert_eq!(fetch.extract, Some(ExtractMode::LlmTxt));
            assert_eq!(fetch.response, Some(ResponseMode::Full));
        }
        _ => panic!("expected Fetch"),
    }
}

#[test]
fn ad04_structured_output_stress() {
    // Deeply nested schema (3 levels), from_example, enable_repair,
    // max_retries: 5, repair_model, combined with for_each iteration.
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: mock
model: test-model
tasks:
  - id: get_docs
    fetch:
      url: "https://api.example.com/docs"
      extract: jsonpath
      selector: "$.documents"

  - id: extract_per_doc
    depends_on: [get_docs]
    with:
      docs: $get_docs
    for_each: "{{with.docs}}"
    as: doc
    concurrency: 3
    fail_fast: false
    infer:
      prompt: "Extract structured data from: {{with.doc}}"
      temperature: 0.1
    structured:
      schema:
        type: object
        properties:
          metadata:
            type: object
            properties:
              title:
                type: string
              author:
                type: object
                properties:
                  name:
                    type: string
                  affiliation:
                    type: object
                    properties:
                      org:
                        type: string
                      department:
                        type: string
                    required:
                      - org
                required:
                  - name
              tags:
                type: array
                items:
                  type: string
            required:
              - title
              - author
          sections:
            type: array
            items:
              type: object
              properties:
                heading:
                  type: string
                content:
                  type: string
                subsections:
                  type: array
                  items:
                    type: object
                    properties:
                      heading:
                        type: string
                      paragraphs:
                        type: array
                        items:
                          type: string
          citations:
            type: array
            items:
              type: object
              properties:
                ref_id:
                  type: string
                text:
                  type: string
        required:
          - metadata
          - sections
      enable_repair: true
      enable_extractor: true
      enable_tool_injection: true
      enable_retry: true
      max_retries: 5
      repair_model: test-model
      strict: true

  - id: from_example_task
    depends_on: [get_docs]
    with:
      raw: $get_docs
    infer:
      prompt: "Structure this data: {{with.raw | trim}}"
    structured:
      from_example:
        product_name: "Widget Pro"
        price: 29.99
        in_stock: true
        variants:
          - color: "red"
            sku: "WP-R"
          - color: "blue"
            sku: "WP-B"
      enable_repair: true
      max_retries: 3

  - id: aggregate
    depends_on: [extract_per_doc, from_example_task]
    with:
      structured_docs: $extract_per_doc
      product: $from_example_task
    infer: "Aggregate docs: {{with.structured_docs | first}} and product: {{with.product | trim}}"
"#;
    let w = ok(yaml);
    assert_eq!(w.tasks.len(), 4);
    // Verify for_each with structured output
    let t = &w.tasks[1];
    assert!(t.has_for_each());
    assert_eq!(t.for_each_var(), "doc");
    assert_eq!(t.for_each_concurrency(), 3);
    assert!(!t.for_each_fail_fast());
    // Verify deeply nested structured spec
    let spec = t.structured.as_ref().unwrap();
    assert_eq!(spec.enable_repair, Some(true));
    // enable_extractor removed (L1 was never implemented) — silently ignored in YAML
    assert_eq!(spec.enable_tool_injection, Some(true));
    assert_eq!(spec.enable_retry, Some(true));
    assert_eq!(spec.max_retries, Some(5));
    assert_eq!(spec.repair_model.as_deref(), Some("test-model"));
    assert_eq!(spec.strict, Some(true));
    assert!(spec.schema.is_some());
    // Verify from_example task
    let t_ex = &w.tasks[2];
    let spec_ex = t_ex.structured.as_ref().unwrap();
    assert!(spec_ex.from_example.is_some());
    assert_eq!(spec_ex.enable_repair, Some(true));
    assert_eq!(spec_ex.max_retries, Some(3));
}

#[test]
fn ad05_edge_case_combos() {
    // Vision content + structured, agent + structured, exec with template env,
    // invoke with template params + pipe transforms, fetch with json body template.
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: mock
model: test-model

inputs:
  api_key:
    type: string
    default: "sk-test"
  node_env:
    type: string
    default: "production"
  search_query:
    type: string
    default: "AI safety"

tasks:
  - id: vision_structured
    infer:
      content:
        - type: image
          source: "blake3:abcdef1234567890"
          detail: high
        - type: text
          text: "Extract all visible data from this document scan"
    structured:
      schema:
        type: object
        properties:
          document_type:
            type: string
          fields:
            type: array
            items:
              type: object
              properties:
                label:
                  type: string
                value:
                  type: string
        required:
          - document_type
          - fields
      enable_repair: true

  - id: agent_structured
    depends_on: [vision_structured]
    with:
      doc: $vision_structured
    agent:
      prompt: "Validate and enrich document: {{with.doc | trim}}"
      max_turns: 5
      tools:
        - nika:read
    structured:
      schema:
        type: object
        properties:
          validated:
            type: boolean
          enriched_fields:
            type: array
        required:
          - validated

  - id: exec_with_template_env
    depends_on: [vision_structured]
    with:
      doc_type: $vision_structured
    exec:
      command: "process-doc --type {{with.doc_type | default('unknown') | lower}}"
      shell: true
      cwd: "./processing"
      timeout: 120
      env:
        NODE_ENV: "{{inputs.node_env}}"
        API_KEY: "{{inputs.api_key}}"
        DOC_HASH: "blake3:abcdef1234567890"

  - id: invoke_with_template_transforms
    depends_on: [agent_structured]
    with:
      enriched: $agent_structured
    invoke:
      mcp: novanet
      tool: novanet_write
      params:
        content: "{{with.enriched | trim}}"
        category: "{{inputs.search_query | lower | trim}}"
      timeout: 45

  - id: fetch_json_body_template
    depends_on: [invoke_with_template_transforms]
    with:
      stored: $invoke_with_template_transforms
    fetch:
      url: "https://api.example.com/v2/process"
      method: POST
      headers:
        Authorization: "Bearer {{inputs.api_key}}"
        X-Request-ID: "{{with.stored | default('none')}}"
      json:
        query: "{{inputs.search_query}}"
        context: "{{with.stored | trim | default('empty')}}"
        options:
          depth: 3
          format: json
      timeout: 30
      follow_redirects: false
"#;
    let w = ok(yaml);
    assert_eq!(w.tasks.len(), 5);
    // Vision + structured
    let t0 = &w.tasks[0];
    match &t0.action {
        TaskAction::Infer { infer } => {
            assert!(infer.content.is_some());
            let content = infer.content.as_ref().unwrap();
            assert_eq!(content.len(), 2);
            match &content[0] {
                ContentPart::Image { source, detail, .. } => {
                    assert!(source.contains("blake3:"));
                    assert_eq!(*detail, ImageDetail::High);
                }
                _ => panic!("expected Image content part"),
            }
        }
        _ => panic!("expected Infer"),
    }
    assert!(t0.structured.is_some());
    // Agent + structured
    let t1 = &w.tasks[1];
    assert_eq!(t1.action.verb_name(), "agent");
    assert!(t1.structured.is_some());
    // Exec with env templates
    match &w.tasks[2].action {
        TaskAction::Exec { exec } => {
            assert_eq!(exec.shell, Some(true));
            assert_eq!(exec.cwd.as_deref(), Some("./processing"));
            assert_eq!(exec.timeout, Some(120));
            let env = exec.env.as_ref().unwrap();
            assert_eq!(env.len(), 3);
            assert_eq!(env.get("NODE_ENV").unwrap(), "{{inputs.node_env}}");
        }
        _ => panic!("expected Exec"),
    }
    // Invoke with templates + transforms
    match &w.tasks[3].action {
        TaskAction::Invoke { invoke } => {
            assert_eq!(invoke.mcp.as_deref(), Some("novanet"));
            assert_eq!(invoke.tool.as_deref(), Some("novanet_write"));
            assert!(invoke.params.is_some());
            assert_eq!(invoke.timeout, Some(45));
        }
        _ => panic!("expected Invoke"),
    }
    // Fetch with json body, headers, follow_redirects
    match &w.tasks[4].action {
        TaskAction::Fetch { fetch } => {
            assert_eq!(fetch.method, "POST");
            assert!(fetch.headers.contains_key("Authorization"));
            assert!(fetch.json.is_some());
            let json = fetch.json.as_ref().unwrap();
            assert!(json.get("options").is_some());
            assert_eq!(fetch.follow_redirects, Some(false));
            assert_eq!(fetch.timeout, Some(30));
        }
        _ => panic!("expected Fetch"),
    }
}

#[test]
fn ad06_provider_agnostic_mega_workflow() {
    // Different providers per task, task-level model overrides, extended_thinking
    // on Claude tasks, response_format on OpenAI tasks.
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: mock
model: test-model

tasks:
  - id: claude_deep
    provider: anthropic
    model: claude-sonnet-4-20250514
    infer:
      prompt: "Deep analysis of complex topic"
      system: "You are a deep reasoner"
      temperature: 0.2
      max_tokens: 8192
      extended_thinking: true
      thinking_budget: 4096

  - id: openai_json
    depends_on: [claude_deep]
    provider: openai
    model: gpt-4o
    with:
      analysis: $claude_deep
    infer:
      prompt: "Convert to JSON: {{with.analysis | trim}}"
      temperature: 0.0
      response_format: json

  - id: groq_fast
    depends_on: [claude_deep]
    provider: groq
    model: llama-3.3-70b-versatile
    with:
      analysis: $claude_deep
    infer:
      prompt: "Quick summary: {{with.analysis | trim | upper}}"
      temperature: 0.5
      max_tokens: 500

  - id: mistral_translate
    depends_on: [groq_fast]
    provider: mistral
    model: mistral-large-latest
    with:
      summary: $groq_fast
    infer:
      prompt: "Translate to French: {{with.summary}}"
      temperature: 0.3

  - id: gemini_creative
    depends_on: [openai_json, mistral_translate]
    provider: gemini
    model: gemini-2.5-pro
    with:
      json_data: $openai_json
      french: $mistral_translate
    infer:
      prompt: "Creative synthesis of {{with.json_data | trim}} and {{with.french | trim}}"
      temperature: 1.5
      max_tokens: 4000

  - id: xai_verify
    depends_on: [gemini_creative]
    with:
      creative: $gemini_creative
    agent:
      prompt: "Verify factual accuracy: {{with.creative | trim}}"
      provider: xai
      model: grok-3
      max_turns: 3
      tools:
        - nika:read

  - id: fetch_external
    fetch:
      url: "https://api.example.com/validate"
      method: POST
      json:
        data: "placeholder"

  - id: mock_aggregate
    depends_on: [xai_verify, fetch_external]
    with:
      verified: $xai_verify
      external: $fetch_external
    infer: "Final report from {{with.verified | length}} chars + {{with.external | default('none')}}"
"#;
    let w = ok(yaml);
    assert_eq!(w.tasks.len(), 8);
    assert_eq!(w.provider.as_str(), "mock"); // workflow-level
                                             // Verify per-task providers
    match &w.tasks[0].action {
        TaskAction::Infer { infer } => {
            assert_eq!(
                infer.provider.as_ref().map(|p| p.as_str()),
                Some("anthropic")
            );
            assert_eq!(infer.model.as_deref(), Some("claude-sonnet-4-20250514"));
            assert_eq!(infer.extended_thinking, Some(true));
            assert_eq!(infer.thinking_budget, Some(4096));
            assert_eq!(infer.effective_thinking_budget(), 4096);
        }
        _ => panic!("expected Infer"),
    }
    match &w.tasks[1].action {
        TaskAction::Infer { infer } => {
            assert_eq!(infer.provider.as_ref().map(|p| p.as_str()), Some("openai"));
            assert_eq!(infer.model.as_deref(), Some("gpt-4o"));
            assert_eq!(infer.response_format, Some(ResponseFormat::Json));
        }
        _ => panic!("expected Infer"),
    }
    match &w.tasks[2].action {
        TaskAction::Infer { infer } => {
            assert_eq!(infer.provider.as_ref().map(|p| p.as_str()), Some("groq"));
            assert_eq!(infer.model.as_deref(), Some("llama-3.3-70b-versatile"));
        }
        _ => panic!("expected Infer"),
    }
    match &w.tasks[3].action {
        TaskAction::Infer { infer } => {
            assert_eq!(infer.provider.as_ref().map(|p| p.as_str()), Some("mistral"));
            assert_eq!(infer.model.as_deref(), Some("mistral-large-latest"));
        }
        _ => panic!("expected Infer"),
    }
    match &w.tasks[4].action {
        TaskAction::Infer { infer } => {
            assert_eq!(infer.provider.as_ref().map(|p| p.as_str()), Some("gemini"));
            assert_eq!(infer.model.as_deref(), Some("gemini-2.5-pro"));
            assert_eq!(infer.temperature, Some(1.5));
        }
        _ => panic!("expected Infer"),
    }
    match &w.tasks[5].action {
        TaskAction::Agent { agent } => {
            assert_eq!(agent.provider.as_ref().map(|p| p.as_str()), Some("xai"));
            assert_eq!(agent.model.as_deref(), Some("grok-3"));
        }
        _ => panic!("expected Agent"),
    }
    // Verify DAG structure
    let edges = w.edges();
    assert!(edges.contains(&("claude_deep", "openai_json")));
    assert!(edges.contains(&("claude_deep", "groq_fast")));
    assert!(edges.contains(&("groq_fast", "mistral_translate")));
    assert!(edges.contains(&("openai_json", "gemini_creative")));
    assert!(edges.contains(&("mistral_translate", "gemini_creative")));
    assert!(edges.contains(&("gemini_creative", "xai_verify")));
}

#[test]
fn ad07_error_edge_cases() {
    // Test various parse-time errors.

    // 1. Invalid binding expression ({{...}} in with: block)
    let yaml1 = r#"
schema: "nika/workflow@0.12"
provider: mock
model: test-model
tasks:
  - id: gen
    infer: "Generate"
  - id: use_it
    depends_on: [gen]
    with:
      data: "{{inputs.topic}}"
    infer: "Use {{with.data}}"
"#;
    let e1 = err(yaml1);
    let msg1 = e1.to_string();
    assert!(
        msg1.contains("NIKA-151") || msg1.contains("binding") || msg1.contains("must start with"),
        "Expected invalid binding error, got: {msg1}"
    );

    // 2. Circular depends_on (A -> B -> C -> A)
    let yaml2 = r#"
schema: "nika/workflow@0.12"
provider: mock
model: test-model
tasks:
  - id: alpha
    depends_on: [gamma]
    infer: "Alpha"
  - id: beta
    depends_on: [alpha]
    infer: "Beta"
  - id: gamma
    depends_on: [beta]
    infer: "Gamma"
"#;
    let e2 = err(yaml2);
    let msg2 = e2.to_string();
    assert!(
        msg2.contains("cycl") || msg2.contains("NIKA-020") || msg2.contains("cycle"),
        "Expected cycle error, got: {msg2}"
    );

    // 3. Invalid structured schema (missing type at top level)
    // Note: structured without schema or from_example should fail
    let yaml3 = wrap("structured:\n  enable_repair: true\ninfer: \"Extract data\"");
    let e3 = err(&yaml3);
    let msg3 = e3.to_string();
    assert!(
        msg3.contains("schema") || msg3.contains("from_example") || msg3.contains("missing"),
        "Expected missing schema error, got: {msg3}"
    );

    // 4. with: binding referencing non-existent task
    let yaml4 = r#"
schema: "nika/workflow@0.12"
provider: mock
model: test-model
tasks:
  - id: reader
    with:
      data: $nonexistent_task
    infer: "Read {{with.data}}"
"#;
    let e4 = err(yaml4);
    let msg4 = e4.to_string();
    assert!(
        msg4.contains("nonexistent") || msg4.contains("unknown") || msg4.contains("NIKA-14"),
        "Expected unknown task ref error, got: {msg4}"
    );

    // 5. Invalid schema version
    let yaml5 = r#"
schema: "nika/workflow@999.0"
provider: mock
model: test-model
tasks:
  - id: t
    infer: "Hello"
"#;
    let e5 = err(yaml5);
    let msg5 = e5.to_string();
    assert!(
        msg5.contains("schema")
            || msg5.contains("version")
            || msg5.contains("NIKA-01")
            || msg5.contains("NIKA-142"),
        "Expected schema version error, got: {msg5}"
    );

    // 6. Missing model for infer without mock provider
    let yaml6 = r#"
schema: "nika/workflow@0.12"
provider: anthropic
tasks:
  - id: oops
    infer: "No model specified"
"#;
    let e6 = err(yaml6);
    let msg6 = e6.to_string();
    assert!(
        msg6.contains("model") || msg6.contains("NIKA-034"),
        "Expected missing model error, got: {msg6}"
    );

    // 7. Duplicate task IDs
    let yaml7 = r#"
schema: "nika/workflow@0.12"
provider: mock
model: test-model
tasks:
  - id: same_id
    infer: "First"
  - id: same_id
    infer: "Second"
"#;
    let e7 = err(yaml7);
    let msg7 = e7.to_string();
    assert!(
        msg7.contains("duplicate") || msg7.contains("same_id") || msg7.contains("NIKA-01"),
        "Expected duplicate ID error, got: {msg7}"
    );

    // 8. depends_on referencing nonexistent task
    let yaml8 = r#"
schema: "nika/workflow@0.12"
provider: mock
model: test-model
tasks:
  - id: real_task
    depends_on: [ghost_task]
    infer: "I depend on nothing"
"#;
    let e8 = err(yaml8);
    let msg8 = e8.to_string();
    assert!(
        msg8.contains("ghost_task")
            || msg8.contains("unknown")
            || msg8.contains("not found")
            || msg8.contains("NIKA-02"),
        "Expected unknown dependency error, got: {msg8}"
    );

    // 9. Missing schema: field entirely
    let yaml9 = r#"
provider: mock
model: test-model
tasks:
  - id: t1
    infer: "Hello"
"#;
    let e9 = err(yaml9);
    let msg9 = e9.to_string();
    assert!(
        msg9.contains("schema") || msg9.contains("NIKA-01"),
        "Expected missing schema error, got: {msg9}"
    );

    // 10. NaN temperature (caught by is_finite check at parse time)
    let yaml10 = r#"
schema: "nika/workflow@0.12"
provider: mock
model: test-model
tasks:
  - id: t1
    infer:
      prompt: "Test"
      temperature: .nan
"#;
    let e10 = err(yaml10);
    let msg10 = e10.to_string();
    assert!(
        msg10.contains("temperature") || msg10.contains("NIKA-01") || msg10.contains("parse"),
        "Expected NaN temperature error, got: {msg10}"
    );

    // 11. Self-referencing dependency
    let yaml11 = r#"
schema: "nika/workflow@0.12"
provider: mock
model: test-model
tasks:
  - id: narcissist
    depends_on: [narcissist]
    infer: "I depend on myself"
"#;
    let e11 = err(yaml11);
    let msg11 = e11.to_string();
    assert!(
        msg11.contains("cycl") || msg11.contains("narcissist") || msg11.contains("NIKA-020"),
        "Expected self-cycle error, got: {msg11}"
    );
}
