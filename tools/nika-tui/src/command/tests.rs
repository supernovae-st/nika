// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

use super::*;

// ═══════════════════════════════════════════════════════════════════════
// /infer tests
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn test_parse_infer_command() {
    let input = "/infer explain this code";
    let cmd = Command::parse(input);
    assert!(matches!(cmd, Command::Infer { prompt } if prompt == "explain this code"));
}

#[test]
fn test_parse_infer_empty_prompt() {
    let input = "/infer";
    let cmd = Command::parse(input);
    assert!(matches!(cmd, Command::Infer { prompt } if prompt.is_empty()));
}

#[test]
fn test_parse_infer_with_extra_spaces() {
    let input = "/infer   explain this code  ";
    let cmd = Command::parse(input);
    assert!(matches!(cmd, Command::Infer { prompt } if prompt == "explain this code"));
}

// ═══════════════════════════════════════════════════════════════════════
// /exec tests
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn test_parse_exec_command() {
    let input = "/exec cargo test";
    let cmd = Command::parse(input);
    assert!(matches!(cmd, Command::Exec { command } if command == "cargo test"));
}

#[test]
fn test_parse_exec_with_pipes() {
    let input = "/exec ls -la | grep .rs";
    let cmd = Command::parse(input);
    assert!(matches!(cmd, Command::Exec { command } if command == "ls -la | grep .rs"));
}

// ═══════════════════════════════════════════════════════════════════════
// /fetch tests
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn test_parse_fetch_get() {
    let input = "/fetch https://api.example.com";
    let cmd = Command::parse(input);
    assert!(matches!(
        cmd,
        Command::Fetch { url, method }
        if url == "https://api.example.com" && method == "GET"
    ));
}

#[test]
fn test_parse_fetch_post() {
    let input = "/fetch https://api.example.com POST";
    let cmd = Command::parse(input);
    assert!(matches!(
        cmd,
        Command::Fetch { url, method }
        if url == "https://api.example.com" && method == "POST"
    ));
}

#[test]
fn test_parse_fetch_lowercase_method() {
    let input = "/fetch https://api.example.com post";
    let cmd = Command::parse(input);
    assert!(matches!(
        cmd,
        Command::Fetch { url, method }
        if url == "https://api.example.com" && method == "POST"
    ));
}

// FetchError tests for smart error detection
#[test]
fn test_parse_fetch_error_curl() {
    let input = "/fetch curl https://api.example.com";
    let cmd = Command::parse(input);
    assert!(matches!(cmd, Command::FetchError { error, .. } if error.contains("curl")));
}

#[test]
fn test_parse_fetch_error_no_scheme() {
    let input = "/fetch api.example.com";
    let cmd = Command::parse(input);
    assert!(matches!(cmd, Command::FetchError { error, .. } if error.contains("http")));
}

#[test]
fn test_parse_fetch_error_empty() {
    let input = "/fetch";
    let cmd = Command::parse(input);
    assert!(matches!(cmd, Command::FetchError { error, .. } if error.contains("manquante")));
}

#[test]
fn test_parse_fetch_error_invalid_method() {
    let input = "/fetch https://api.example.com POAST";
    let cmd = Command::parse(input);
    assert!(matches!(cmd, Command::FetchError { error, .. } if error.contains("POAST")));
}

#[test]
fn test_parse_fetch_method_first_swaps() {
    // User typed method before URL - we handle it!
    let input = "/fetch GET https://api.example.com";
    let cmd = Command::parse(input);
    assert!(matches!(
        cmd,
        Command::Fetch { url, method }
        if url == "HTTPS://API.EXAMPLE.COM" && method == "GET"
    ));
}

// ═══════════════════════════════════════════════════════════════════════
// /invoke tests
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn test_parse_invoke_simple() {
    let input = "/invoke describe";
    let cmd = Command::parse(input);
    assert!(matches!(
        cmd,
        Command::Invoke { tool, server, params }
        if tool == "describe" && server.is_none() && params.is_object()
    ));
}

#[test]
fn test_parse_invoke_with_server() {
    let input = "/invoke novanet:describe";
    let cmd = Command::parse(input);
    assert!(matches!(
        cmd,
        Command::Invoke { tool, server, .. }
        if tool == "describe" && server == Some("novanet".to_string())
    ));
}

#[test]
fn test_parse_invoke_with_json_params() {
    let input = r#"/invoke novanet:describe {"entity":"qr-code"}"#;
    let cmd = Command::parse(input);
    if let Command::Invoke {
        tool,
        server,
        params,
    } = cmd
    {
        assert_eq!(tool, "describe");
        assert_eq!(server, Some("novanet".to_string()));
        assert_eq!(params["entity"], "qr-code");
    } else {
        panic!("Expected Command::Invoke");
    }
}

#[test]
fn test_parse_invoke_empty() {
    let input = "/invoke";
    let cmd = Command::parse(input);
    assert!(
        matches!(cmd, Command::InvokeError { ref error, .. } if error.contains("Missing tool")),
        "Expected InvokeError for empty /invoke, got {:?}",
        cmd
    );
}

// ═══════════════════════════════════════════════════════════════════════
// /agent tests
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn test_parse_agent_simple() {
    let input = "/agent generate a landing page";
    let cmd = Command::parse(input);
    if let Command::Agent {
        goal,
        max_turns,
        mcp_servers,
    } = cmd
    {
        assert_eq!(goal, "generate a landing page");
        assert_eq!(max_turns, None);
        assert!(mcp_servers.is_empty());
    } else {
        panic!("Expected Command::Agent");
    }
}

#[test]
fn test_parse_agent_with_max_turns() {
    let input = "/agent generate a landing page --max-turns 5";
    let cmd = Command::parse(input);
    if let Command::Agent {
        goal,
        max_turns,
        mcp_servers,
    } = cmd
    {
        assert_eq!(goal, "generate a landing page");
        assert_eq!(max_turns, Some(5));
        assert!(mcp_servers.is_empty());
    } else {
        panic!("Expected Command::Agent");
    }
}

#[test]
fn test_parse_agent_max_turns_at_start() {
    let input = "/agent --max-turns 3 do something";
    let cmd = Command::parse(input);
    // The goal should be empty (before --max-turns)
    // "3 do something" parses as 3 using split_whitespace().next()
    if let Command::Agent {
        goal,
        max_turns,
        mcp_servers,
    } = cmd
    {
        assert!(goal.is_empty());
        assert_eq!(max_turns, Some(3));
        assert!(mcp_servers.is_empty());
    } else {
        panic!("Expected Command::Agent");
    }
}

#[test]
fn test_parse_agent_max_turns_only() {
    // When --max-turns is followed by only a number, it should parse correctly
    let input = "/agent --max-turns 10";
    let cmd = Command::parse(input);
    if let Command::Agent {
        goal,
        max_turns,
        mcp_servers,
    } = cmd
    {
        assert!(goal.is_empty());
        assert_eq!(max_turns, Some(10));
        assert!(mcp_servers.is_empty());
    } else {
        panic!("Expected Command::Agent");
    }
}

// ═══════════════════════════════════════════════════════════════════════
// /help tests
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn test_parse_help() {
    let input = "/help";
    let cmd = Command::parse(input);
    assert!(matches!(cmd, Command::Help));
}

#[test]
fn test_parse_question_mark_help() {
    let input = "/?";
    let cmd = Command::parse(input);
    assert!(matches!(cmd, Command::Help));
}

// ═══════════════════════════════════════════════════════════════════════
// Chat message tests
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn test_parse_plain_message() {
    let input = "hello world";
    let cmd = Command::parse(input);
    assert!(matches!(cmd, Command::Chat { message } if message == "hello world"));
}

#[test]
fn test_parse_empty_message() {
    let input = "";
    let cmd = Command::parse(input);
    assert!(matches!(cmd, Command::Chat { message } if message.is_empty()));
}

#[test]
fn test_parse_whitespace_message() {
    let input = "   ";
    let cmd = Command::parse(input);
    assert!(matches!(cmd, Command::Chat { message } if message.is_empty()));
}

#[test]
fn test_parse_unknown_command_as_chat() {
    let input = "/unknown some text";
    let cmd = Command::parse(input);
    assert!(matches!(cmd, Command::Chat { message } if message == "/unknown some text"));
}

// ═══════════════════════════════════════════════════════════════════════
// /model tests
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn test_parse_model_openai() {
    let input = "/model openai";
    let cmd = Command::parse(input);
    assert!(matches!(
        cmd,
        Command::Model {
            provider: ModelProvider::OpenAI
        }
    ));
}

#[test]
fn test_parse_model_claude() {
    let input = "/model claude";
    let cmd = Command::parse(input);
    assert!(matches!(
        cmd,
        Command::Model {
            provider: ModelProvider::Claude
        }
    ));
}

#[test]
fn test_parse_model_gpt_alias() {
    let input = "/model gpt";
    let cmd = Command::parse(input);
    assert!(matches!(
        cmd,
        Command::Model {
            provider: ModelProvider::OpenAI
        }
    ));
}

#[test]
fn test_parse_model_anthropic_alias() {
    let input = "/model anthropic";
    let cmd = Command::parse(input);
    assert!(matches!(
        cmd,
        Command::Model {
            provider: ModelProvider::Claude
        }
    ));
}

#[test]
fn test_parse_model_list() {
    let input = "/model list";
    let cmd = Command::parse(input);
    assert!(matches!(
        cmd,
        Command::Model {
            provider: ModelProvider::List
        }
    ));
}

#[test]
fn test_parse_model_empty() {
    let input = "/model";
    let cmd = Command::parse(input);
    assert!(matches!(
        cmd,
        Command::Model {
            provider: ModelProvider::List
        }
    ));
}

#[test]
fn test_model_provider_name() {
    assert_eq!(ModelProvider::OpenAI.name(), "OpenAI (gpt-4o)");
    assert_eq!(
        ModelProvider::Claude.name(),
        "Anthropic Claude (claude-sonnet-4)"
    );
    assert_eq!(ModelProvider::Mistral.name(), "Mistral AI (mistral-large)");
    assert_eq!(ModelProvider::Groq.name(), "Groq (llama-3.3-70b)");
    assert_eq!(ModelProvider::DeepSeek.name(), "DeepSeek (deepseek-chat)");
    assert_eq!(ModelProvider::Native.name(), "Native (mistral.rs)");
}

#[test]
fn test_model_provider_env_var() {
    assert_eq!(ModelProvider::OpenAI.env_var(), "OPENAI_API_KEY");
    assert_eq!(ModelProvider::Claude.env_var(), "ANTHROPIC_API_KEY");
    assert_eq!(ModelProvider::Mistral.env_var(), "MISTRAL_API_KEY");
    assert_eq!(ModelProvider::Groq.env_var(), "GROQ_API_KEY");
    assert_eq!(ModelProvider::DeepSeek.env_var(), "DEEPSEEK_API_KEY");
    assert_eq!(ModelProvider::Native.env_var(), "NIKA_NATIVE_MODEL_PATH");
}

// ═══════════════════════════════════════════════════════════════════════
// Provider tests (new providers)
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn test_parse_model_mistral() {
    let input = "/model mistral";
    let cmd = Command::parse(input);
    assert!(matches!(
        cmd,
        Command::Model {
            provider: ModelProvider::Mistral
        }
    ));
}

#[test]
fn test_parse_model_groq() {
    let input = "/model groq";
    let cmd = Command::parse(input);
    assert!(matches!(
        cmd,
        Command::Model {
            provider: ModelProvider::Groq
        }
    ));
}

#[test]
fn test_parse_model_deepseek() {
    let input = "/model deepseek";
    let cmd = Command::parse(input);
    assert!(matches!(
        cmd,
        Command::Model {
            provider: ModelProvider::DeepSeek
        }
    ));
}

#[test]
fn test_parse_model_native() {
    let input = "/model native";
    let cmd = Command::parse(input);
    assert!(matches!(
        cmd,
        Command::Model {
            provider: ModelProvider::Native
        }
    ));
}

#[test]
fn test_parse_model_llama_alias() {
    // llama maps to Groq
    let input = "/model llama";
    let cmd = Command::parse(input);
    assert!(matches!(
        cmd,
        Command::Model {
            provider: ModelProvider::Groq
        }
    ));
}

#[test]
fn test_parse_model_local_alias() {
    // "local" maps to Native
    let input = "/model local";
    let cmd = Command::parse(input);
    assert!(matches!(
        cmd,
        Command::Model {
            provider: ModelProvider::Native
        }
    ));
}

#[test]
fn test_native_always_available() {
    // Native inference is always available when feature is enabled
    assert!(ModelProvider::Native.is_available());
}

// ═══════════════════════════════════════════════════════════════════════
// /clear tests
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn test_parse_clear() {
    let input = "/clear";
    let cmd = Command::parse(input);
    assert!(matches!(cmd, Command::Clear));
}

// ═══════════════════════════════════════════════════════════════════════
// /agent with --mcp tests
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn test_parse_agent_with_mcp_servers() {
    let input = "/agent generate a landing page --mcp novanet,perplexity";
    let cmd = Command::parse(input);
    if let Command::Agent {
        goal,
        max_turns,
        mcp_servers,
    } = cmd
    {
        assert_eq!(goal, "generate a landing page");
        assert_eq!(max_turns, None);
        assert_eq!(mcp_servers, vec!["novanet", "perplexity"]);
    } else {
        panic!("Expected Command::Agent");
    }
}

#[test]
fn test_parse_agent_with_mcp_and_max_turns() {
    let input = "/agent generate a landing page --mcp novanet --max-turns 5";
    let cmd = Command::parse(input);
    if let Command::Agent {
        goal,
        max_turns,
        mcp_servers,
    } = cmd
    {
        assert_eq!(goal, "generate a landing page");
        assert_eq!(max_turns, Some(5));
        assert_eq!(mcp_servers, vec!["novanet"]);
    } else {
        panic!("Expected Command::Agent");
    }
}

#[test]
fn test_parse_agent_with_single_mcp_server() {
    let input = "/agent do something --mcp novanet";
    let cmd = Command::parse(input);
    if let Command::Agent { mcp_servers, .. } = cmd {
        assert_eq!(mcp_servers, vec!["novanet"]);
    } else {
        panic!("Expected Command::Agent");
    }
}

#[test]
fn test_parse_agent_mcp_order_reversed() {
    // --max-turns before --mcp should also work
    let input = "/agent do something --max-turns 3 --mcp novanet,perplexity";
    let cmd = Command::parse(input);
    if let Command::Agent {
        max_turns,
        mcp_servers,
        ..
    } = cmd
    {
        assert_eq!(max_turns, Some(3));
        assert_eq!(mcp_servers, vec!["novanet", "perplexity"]);
    } else {
        panic!("Expected Command::Agent");
    }
}

// ═══════════════════════════════════════════════════════════════════════
// /mcp command tests
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn test_parse_mcp_list() {
    let input = "/mcp";
    let cmd = Command::parse(input);
    assert!(matches!(
        cmd,
        Command::Mcp {
            action: McpAction::List
        }
    ));
}

#[test]
fn test_parse_mcp_list_explicit() {
    let input = "/mcp list";
    let cmd = Command::parse(input);
    assert!(matches!(
        cmd,
        Command::Mcp {
            action: McpAction::List
        }
    ));
}

#[test]
fn test_parse_mcp_select() {
    let input = "/mcp select novanet,perplexity";
    let cmd = Command::parse(input);
    if let Command::Mcp {
        action: McpAction::Select(servers),
    } = cmd
    {
        assert_eq!(servers, vec!["novanet", "perplexity"]);
    } else {
        panic!("Expected Command::Mcp with Select action");
    }
}

#[test]
fn test_parse_mcp_toggle() {
    let input = "/mcp toggle novanet";
    let cmd = Command::parse(input);
    if let Command::Mcp {
        action: McpAction::Toggle(server),
    } = cmd
    {
        assert_eq!(server, "novanet");
    } else {
        panic!("Expected Command::Mcp with Toggle action");
    }
}

// ═══════════════════════════════════════════════════════════════════════
// Case insensitivity tests
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn test_parse_uppercase_infer() {
    let input = "/INFER explain this";
    let cmd = Command::parse(input);
    assert!(matches!(cmd, Command::Infer { prompt } if prompt == "explain this"));
}

#[test]
fn test_parse_mixed_case_exec() {
    let input = "/ExEc cargo test";
    let cmd = Command::parse(input);
    assert!(matches!(cmd, Command::Exec { command } if command == "cargo test"));
}

// ═══════════════════════════════════════════════════════════════════════
// Helper method tests
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn test_verb_names() {
    assert_eq!(Command::Infer { prompt: "x".into() }.verb(), "infer");
    assert_eq!(
        Command::Exec {
            command: "x".into()
        }
        .verb(),
        "exec"
    );
    assert_eq!(
        Command::Fetch {
            url: "x".into(),
            method: "GET".into()
        }
        .verb(),
        "fetch"
    );
    assert_eq!(
        Command::Invoke {
            tool: "x".into(),
            server: None,
            params: serde_json::json!({})
        }
        .verb(),
        "invoke"
    );
    assert_eq!(
        Command::Agent {
            goal: "x".into(),
            max_turns: None,
            mcp_servers: vec![]
        }
        .verb(),
        "agent"
    );
    assert_eq!(
        Command::Chat {
            message: "x".into()
        }
        .verb(),
        "chat"
    );
    assert_eq!(Command::Help.verb(), "help");
    assert_eq!(
        Command::Model {
            provider: ModelProvider::OpenAI
        }
        .verb(),
        "model"
    );
    assert_eq!(Command::Clear.verb(), "clear");
    assert_eq!(
        Command::Mcp {
            action: McpAction::List
        }
        .verb(),
        "mcp"
    );
}

#[test]
fn test_is_empty() {
    assert!(Command::Chat { message: "".into() }.is_empty());
    assert!(!Command::Chat {
        message: "hi".into()
    }
    .is_empty());
    assert!(Command::Infer { prompt: "".into() }.is_empty());
    assert!(!Command::Help.is_empty());
    assert!(!Command::Model {
        provider: ModelProvider::OpenAI
    }
    .is_empty());
    assert!(!Command::Clear.is_empty());
}

// ═══════════════════════════════════════════════════════════════════════
// ModelProvider::from_name tests
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn test_model_provider_from_name_claude() {
    assert_eq!(
        ModelProvider::from_name("claude"),
        Some(ModelProvider::Claude)
    );
    assert_eq!(
        ModelProvider::from_name("anthropic"),
        Some(ModelProvider::Claude)
    );
    assert_eq!(
        ModelProvider::from_name("CLAUDE"),
        Some(ModelProvider::Claude)
    );
}

#[test]
fn test_model_provider_from_name_openai() {
    assert_eq!(
        ModelProvider::from_name("openai"),
        Some(ModelProvider::OpenAI)
    );
    assert_eq!(ModelProvider::from_name("gpt"), Some(ModelProvider::OpenAI));
}

#[test]
fn test_model_provider_from_name_all_providers() {
    assert_eq!(
        ModelProvider::from_name("mistral"),
        Some(ModelProvider::Mistral)
    );
    assert_eq!(ModelProvider::from_name("groq"), Some(ModelProvider::Groq));
    assert_eq!(
        ModelProvider::from_name("deepseek"),
        Some(ModelProvider::DeepSeek)
    );
    // Native inference
    assert_eq!(
        ModelProvider::from_name("native"),
        Some(ModelProvider::Native)
    );
    // "local" alias also maps to Native
    assert_eq!(
        ModelProvider::from_name("local"),
        Some(ModelProvider::Native)
    );
    assert_eq!(ModelProvider::from_name("ollama"), None);
}

#[test]
fn test_model_provider_from_name_invalid() {
    assert_eq!(ModelProvider::from_name("invalid"), None);
    assert_eq!(ModelProvider::from_name(""), None);
    assert_eq!(ModelProvider::from_name("list"), None);
}

// ═══════════════════════════════════════════════════════════════════════════
// Task 2.4: parse_export_args tests
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_parse_export_args_no_args_defaults_to_json() {
    let cmd = Command::parse_export_args("");
    assert!(matches!(
        cmd,
        Command::Export {
            format: ExportFormat::Json,
            path: None
        }
    ));
}

#[test]
fn test_parse_export_args_yaml_keyword() {
    let cmd = Command::parse_export_args("yaml");
    assert!(matches!(
        cmd,
        Command::Export {
            format: ExportFormat::Yaml,
            path: None
        }
    ));
}

#[test]
fn test_parse_export_args_json_keyword() {
    let cmd = Command::parse_export_args("json");
    assert!(matches!(
        cmd,
        Command::Export {
            format: ExportFormat::Json,
            path: None
        }
    ));
}

#[test]
fn test_parse_export_args_yaml_with_path() {
    let cmd = Command::parse_export_args("yaml my-workflow.nika.yaml");
    if let Command::Export { format, path } = cmd {
        assert_eq!(format, ExportFormat::Yaml);
        assert_eq!(path, Some("my-workflow.nika.yaml".to_string()));
    } else {
        panic!("Expected Export command");
    }
}

#[test]
fn test_parse_export_args_json_with_path() {
    let cmd = Command::parse_export_args("json output.json");
    if let Command::Export { format, path } = cmd {
        assert_eq!(format, ExportFormat::Json);
        assert_eq!(path, Some("output.json".to_string()));
    } else {
        panic!("Expected Export command");
    }
}

#[test]
fn test_parse_export_args_path_infers_yaml_extension() {
    // .yaml extension should infer YAML format
    let cmd = Command::parse_export_args("output.yaml");
    if let Command::Export { format, path } = cmd {
        assert_eq!(format, ExportFormat::Yaml);
        assert_eq!(path, Some("output.yaml".to_string()));
    } else {
        panic!("Expected Export command");
    }
}

#[test]
fn test_parse_export_args_path_infers_yml_extension() {
    // .yml extension should infer YAML format
    let cmd = Command::parse_export_args("output.yml");
    if let Command::Export { format, path } = cmd {
        assert_eq!(format, ExportFormat::Yaml);
        assert_eq!(path, Some("output.yml".to_string()));
    } else {
        panic!("Expected Export command");
    }
}

#[test]
fn test_parse_export_args_path_infers_json_extension() {
    // .json extension should infer JSON format
    let cmd = Command::parse_export_args("output.json");
    if let Command::Export { format, path } = cmd {
        assert_eq!(format, ExportFormat::Json);
        assert_eq!(path, Some("output.json".to_string()));
    } else {
        panic!("Expected Export command");
    }
}

#[test]
fn test_parse_export_args_path_without_extension_defaults_json() {
    // No extension should default to JSON
    let cmd = Command::parse_export_args("myfile");
    if let Command::Export { format, path } = cmd {
        assert_eq!(format, ExportFormat::Json);
        assert_eq!(path, Some("myfile".to_string()));
    } else {
        panic!("Expected Export command");
    }
}

#[test]
fn test_export_format_default() {
    assert_eq!(ExportFormat::default(), ExportFormat::Json);
}

#[test]
fn test_export_format_equality() {
    assert_eq!(ExportFormat::Json, ExportFormat::Json);
    assert_eq!(ExportFormat::Yaml, ExportFormat::Yaml);
    assert_ne!(ExportFormat::Json, ExportFormat::Yaml);
}
