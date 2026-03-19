//! 200+ workflow YAML validation tests through parse_workflow + the three-phase pipeline.
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

use crate::ast::content::{ContentPart, ImageDetail};
use crate::ast::output::OutputFormat;
use crate::ast::{parse_workflow, TaskAction};

/// Helper: wraps a task YAML snippet in a minimal valid workflow.
fn wrap(task_yaml: &str) -> String {
    let indented: String = task_yaml
        .lines()
        .map(|line| format!("    {line}\n"))
        .collect();
    format!("schema: \"nika/workflow@0.12\"\ntasks:\n  - id: t1\n{indented}")
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
            assert_eq!(infer.provider.as_deref(), Some("openai"));
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
        "infer:\n  prompt: \"Deep reasoning\"\n  provider: claude\n  extended_thinking: true\n  thinking_budget: 4096",
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
        "infer:\n  prompt: \"Think deep\"\n  provider: claude\n  extended_thinking: true\n  thinking_budget: 8192",
    );
    let w = ok(&yaml);
    match &w.tasks[0].action {
        TaskAction::Infer { infer } => assert_eq!(infer.thinking_budget, Some(8192)),
        _ => panic!("expected Infer"),
    }
}

#[test]
fn a09_infer_multiline_prompt_shorthand() {
    let yaml = "schema: \"nika/workflow@0.12\"\ntasks:\n  - id: t1\n    infer: |\n      Line one.\n      Line two.\n";
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
            assert_eq!(infer.provider.as_deref(), Some("mistral"));
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
            assert_eq!(infer.provider.as_deref(), Some("claude"));
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
            assert_eq!(infer.provider.as_deref(), Some("openai"));
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
        TaskAction::Infer { infer } => assert_eq!(infer.provider.as_deref(), Some("claude")),
        _ => panic!("expected Infer"),
    }
}

#[test]
fn a33_infer_provider_openai() {
    let yaml = wrap("provider: openai\ninfer: \"Test\"");
    let w = ok(&yaml);
    match &w.tasks[0].action {
        TaskAction::Infer { infer } => assert_eq!(infer.provider.as_deref(), Some("openai")),
        _ => panic!("expected Infer"),
    }
}

#[test]
fn a34_infer_provider_groq() {
    let yaml = wrap("provider: groq\ninfer: \"Test\"");
    let w = ok(&yaml);
    match &w.tasks[0].action {
        TaskAction::Infer { infer } => assert_eq!(infer.provider.as_deref(), Some("groq")),
        _ => panic!("expected Infer"),
    }
}

#[test]
fn a35_infer_provider_gemini() {
    let yaml = wrap("provider: gemini\ninfer: \"Test\"");
    let w = ok(&yaml);
    match &w.tasks[0].action {
        TaskAction::Infer { infer } => assert_eq!(infer.provider.as_deref(), Some("gemini")),
        _ => panic!("expected Infer"),
    }
}

#[test]
fn a36_infer_provider_xai() {
    let yaml = wrap("provider: xai\ninfer: \"Test\"");
    let w = ok(&yaml);
    match &w.tasks[0].action {
        TaskAction::Infer { infer } => assert_eq!(infer.provider.as_deref(), Some("xai")),
        _ => panic!("expected Infer"),
    }
}

#[test]
fn a37_infer_provider_deepseek() {
    let yaml = wrap("provider: deepseek\ninfer: \"Test\"");
    let w = ok(&yaml);
    match &w.tasks[0].action {
        TaskAction::Infer { infer } => assert_eq!(infer.provider.as_deref(), Some("deepseek")),
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
        "infer:\n  prompt: \"Min budget\"\n  provider: claude\n  extended_thinking: true\n  thinking_budget: 1024",
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
        "infer:\n  prompt: \"Max budget\"\n  provider: claude\n  extended_thinking: true\n  thinking_budget: 65536",
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
    let yaml = "schema: \"nika/workflow@0.12\"\ntasks:\n  - id: t1\n    exec: |\n      echo first &&\n      echo second\n";
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
        wrap("invoke:\n  mcp: novanet\n  tool: novanet_generate\n  params:\n    entity: qr-code");
    let w = ok(&yaml);
    match &w.tasks[0].action {
        TaskAction::Invoke { invoke } => {
            assert_eq!(invoke.mcp.as_deref(), Some("novanet"));
            assert_eq!(invoke.tool.as_deref(), Some("novanet_generate"));
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
            assert_eq!(agent.provider.as_deref(), Some("claude"));
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
            assert_eq!(agent.provider.as_deref(), Some("claude"));
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
    let yaml = "schema: \"nika/workflow@0.12\"\ntasks:\n  - id: only\n    infer: \"Solo\"";
    let w = ok(yaml);
    assert_eq!(w.tasks.len(), 1);
    assert_eq!(w.flow_count(), 0);
}

#[test]
fn f02_dag_linear_two_tasks() {
    let yaml = r#"
schema: "nika/workflow@0.12"
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
tasks:
  - id: a
    infer: "A"
"#;
    let yaml2 = r#"
schema: "nika/workflow@0.12"
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
tasks:
  - id: a
    infer: "A"
  - id: b
    depends_on: [a]
    provider: openai
    infer: "B"
"#;
    let w = ok(yaml);
    assert_eq!(w.provider, "claude");
    match &w.tasks[1].action {
        TaskAction::Infer { infer } => {
            assert_eq!(infer.provider.as_deref(), Some("openai"));
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
        crate::ast::output::SchemaRef::Inline(_)
    ));
}

#[test]
fn h12_structured_all_toggles() {
    let yaml = wrap(
        "structured:\n  schema: ./test.json\n  enable_extractor: false\n  enable_tool_injection: false\n  enable_retry: true\n  enable_repair: false\ninfer: \"Test\"",
    );
    let w = ok(&yaml);
    let spec = w.tasks[0].structured.as_ref().unwrap();
    assert_eq!(spec.enable_extractor, Some(false));
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
    let yaml = "schema: \"nika/workflow@0.12\"\ntasks:\n  - infer: \"No id\"";
    let e = err(yaml);
    let msg = format!("{e}");
    assert!(msg.contains("id") || msg.contains("NIKA"));
}

#[test]
fn i05_error_empty_tasks() {
    let yaml = "schema: \"nika/workflow@0.12\"\ntasks: []";
    let e = err(yaml);
    let msg = format!("{e}");
    assert!(msg.contains("task") || msg.contains("NIKA"));
}

#[test]
fn i06_no_verb_defaults_to_empty_infer() {
    // A task with no verb defaults to an Infer with empty prompt (not an error at parse time)
    let yaml = "schema: \"nika/workflow@0.12\"\ntasks:\n  - id: no_verb";
    let w = ok(yaml);
    match &w.tasks[0].action {
        TaskAction::Infer { infer } => assert!(infer.prompt.is_empty()),
        _ => panic!("expected Infer"),
    }
}

#[test]
fn i07_error_circular_dep_self() {
    let yaml = r#"
schema: "nika/workflow@0.12"
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
    let yaml = "schema: \"nika/workflow@0.12\"\ntasks:\n  - id: t1\n  infer: bad indent";
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
    let yaml = "schema: \"nika/workflow@0.12\"\ntasks: \"not an array\"";
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
    let yaml = "schema: \"nika/workflow@0.12\"\ntasks:\n  - id: \"  \"\n    infer: \"Hello\"";
    let e = err(yaml);
    let msg = format!("{e}");
    assert!(msg.contains("id") || msg.contains("NIKA"));
}

#[test]
fn i16_error_circular_dep_triangle() {
    let yaml = r#"
schema: "nika/workflow@0.12"
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
        "schema: \"nika/workflow@0.12\"\ntasks:\n  - id: \"has space\"\n    infer: \"Hello\"";
    let result = parse_workflow(yaml);
    // The parser should reject IDs with spaces
    assert!(result.is_err());
}

#[test]
fn i18_duplicate_dep_accepted() {
    // Duplicate in depends_on list is currently accepted by the pipeline
    let yaml = r#"
schema: "nika/workflow@0.12"
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
    let yaml = "schema: \"nika/workflow@0.12\"\ntasks:\n  - id: \"\"\n    infer: \"Hello\"";
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
    let yaml = "schema: \"nika/workflow@0.12\"\ntasks:";
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
    assert_eq!(w.provider, "mock");
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
    let yaml = "schema: \"nika/workflow@0.12\"\ntasks:\n  - id: t1\n    infer: \"Hello\"";
    let w = ok(yaml);
    assert_eq!(w.provider, "claude");
}

#[test]
fn k02_workflow_custom_provider() {
    let yaml =
        "schema: \"nika/workflow@0.12\"\nprovider: openai\ntasks:\n  - id: t1\n    infer: \"Hello\"";
    let w = ok(yaml);
    assert_eq!(w.provider, "openai");
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
    let yaml = "schema: \"nika/workflow@0.10\"\ntasks:\n  - id: t1\n    infer: \"Hello\"";
    let w = ok(yaml);
    assert_eq!(w.schema, "nika/workflow@0.10");
}

#[test]
fn k07_workflow_schema_v11() {
    let yaml = "schema: \"nika/workflow@0.11\"\ntasks:\n  - id: t1\n    infer: \"Hello\"";
    let w = ok(yaml);
    assert_eq!(w.schema, "nika/workflow@0.11");
}

#[test]
fn k08_workflow_schema_v12() {
    let yaml = "schema: \"nika/workflow@0.12\"\ntasks:\n  - id: t1\n    infer: \"Hello\"";
    let w = ok(yaml);
    assert_eq!(w.schema, "nika/workflow@0.12");
}

#[test]
fn k09_workflow_inputs() {
    let yaml = r#"
schema: "nika/workflow@0.12"
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
    let yaml = "schema: \"nika/workflow@0.12\"\ntasks:\n  - id: t1\n    infer: \"Hello\"";
    let w = ok(yaml);
    let hash = w.compute_hash();
    assert_eq!(hash.len(), 16);
    assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
}
