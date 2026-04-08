// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! End-to-end integration tests for semantic token generation.
//!
//! Exercises `semantic_tokens()` and `encode_tokens()` against realistic
//! Nika workflow YAML, covering every token type, edge cases, delta
//! encoding correctness, and scale.

use nika_lsp_core::handlers::semantic_tokens::{
    encode_tokens, semantic_tokens, RawToken, TokenType,
};

// ===========================================================================
// Helpers
// ===========================================================================

/// Collect all tokens of a given type from the output.
fn tokens_of(tokens: &[RawToken], tt: TokenType) -> Vec<&RawToken> {
    tokens.iter().filter(|t| t.token_type == tt).collect()
}

/// Extract the matched text slice for a token from the source.
fn token_text<'a>(src: &'a str, tok: &RawToken) -> &'a str {
    let line_str = src.lines().nth(tok.line as usize).unwrap();
    let start = tok.start_char as usize;
    let end = start + tok.length as usize;
    &line_str[start..end]
}

/// Assert that exactly one token on `line` (0-based) has the given type
/// and that its text equals `expected`.
fn assert_token_on_line(tokens: &[RawToken], src: &str, line: u32, tt: TokenType, expected: &str) {
    let hits: Vec<_> = tokens
        .iter()
        .filter(|t| t.line == line && t.token_type == tt)
        .collect();
    assert!(
        !hits.is_empty(),
        "no {tt:?} token on line {line}; tokens on that line: {:?}",
        tokens.iter().filter(|t| t.line == line).collect::<Vec<_>>(),
    );
    let text = token_text(src, hits[0]);
    assert_eq!(
        text, expected,
        "expected {tt:?} text {expected:?} on line {line}, got {text:?}",
    );
}

// ===========================================================================
// 1. All 5 verbs get TokenType::Verb
// ===========================================================================

#[test]
fn verb_infer() {
    let src = "    infer: gpt-4\n";
    let toks = semantic_tokens(src);
    let verbs = tokens_of(&toks, TokenType::Verb);
    assert_eq!(verbs.len(), 1);
    assert_eq!(token_text(src, verbs[0]), "infer");
}

#[test]
fn verb_run_command() {
    let src = "    exec: echo hello\n";
    let toks = semantic_tokens(src);
    let verbs = tokens_of(&toks, TokenType::Verb);
    assert_eq!(verbs.len(), 1);
    assert_eq!(token_text(src, verbs[0]), "exec");
}

#[test]
fn verb_fetch() {
    let src = "    fetch: https://api.example.com\n";
    let toks = semantic_tokens(src);
    let verbs = tokens_of(&toks, TokenType::Verb);
    assert_eq!(verbs.len(), 1);
    assert_eq!(token_text(src, verbs[0]), "fetch");
}

#[test]
fn verb_invoke() {
    let src = "    invoke: novanet.search\n";
    let toks = semantic_tokens(src);
    let verbs = tokens_of(&toks, TokenType::Verb);
    assert_eq!(verbs.len(), 1);
    assert_eq!(token_text(src, verbs[0]), "invoke");
}

#[test]
fn verb_agent() {
    let src = "    agent: multi-turn\n";
    let toks = semantic_tokens(src);
    let verbs = tokens_of(&toks, TokenType::Verb);
    assert_eq!(verbs.len(), 1);
    assert_eq!(token_text(src, verbs[0]), "agent");
}

#[test]
fn all_five_verbs_in_one_workflow() {
    let src = "\
schema: '@0.12'
workflow: verb-test
tasks:
  - id: t1
    infer: gpt-4
  - id: t2
    exec: echo
  - id: t3
    fetch: https://x
  - id: t4
    invoke: tool
  - id: t5
    agent: loop
";
    let toks = semantic_tokens(src);
    let verbs = tokens_of(&toks, TokenType::Verb);
    let verb_texts: Vec<&str> = verbs.iter().map(|t| token_text(src, t)).collect();
    assert_eq!(verb_texts, &["infer", "exec", "fetch", "invoke", "agent"]);
}

#[test]
fn verb_at_different_indent_levels() {
    let src = "  infer: a\n      infer: b\n";
    let toks = semantic_tokens(src);
    let verbs = tokens_of(&toks, TokenType::Verb);
    assert_eq!(verbs.len(), 2);
    // Both should point at the correct start_char
    assert_eq!(verbs[0].start_char, 2);
    assert_eq!(verbs[1].start_char, 6);
}

#[test]
fn verb_after_list_dash() {
    // In a tasks list, a verb can appear as "- infer:" at the sequence item level.
    let src = "  - infer: something\n";
    let toks = semantic_tokens(src);
    let verbs = tokens_of(&toks, TokenType::Verb);
    assert_eq!(verbs.len(), 1);
    assert_eq!(token_text(src, verbs[0]), "infer");
}

// ===========================================================================
// 2. Top-level keys get TokenType::Keyword
// ===========================================================================

#[test]
fn top_level_schema_keyword() {
    let src = "schema: '@0.12'\n";
    let toks = semantic_tokens(src);
    assert_token_on_line(&toks, src, 0, TokenType::Keyword, "schema");
}

#[test]
fn top_level_workflow_keyword() {
    let src = "workflow: my-pipeline\n";
    let toks = semantic_tokens(src);
    assert_token_on_line(&toks, src, 0, TokenType::Keyword, "workflow");
}

#[test]
fn top_level_tasks_keyword() {
    let src = "tasks:\n  - id: t1\n";
    let toks = semantic_tokens(src);
    assert_token_on_line(&toks, src, 0, TokenType::Keyword, "tasks");
}

#[test]
fn top_level_mcp_keyword() {
    let src = "mcp:\n  server: novanet\n";
    let toks = semantic_tokens(src);
    assert_token_on_line(&toks, src, 0, TokenType::Keyword, "mcp");
}

#[test]
fn top_level_context_keyword() {
    let src = "context:\n  files:\n    readme: ./README.md\n";
    let toks = semantic_tokens(src);
    assert_token_on_line(&toks, src, 0, TokenType::Keyword, "context");
}

#[test]
fn top_level_inputs_keyword() {
    let src = "inputs:\n  name: string\n";
    let toks = semantic_tokens(src);
    assert_token_on_line(&toks, src, 0, TokenType::Keyword, "inputs");
}

#[test]
fn top_level_include_keyword() {
    let src = "include:\n  - ./base.nika.yaml\n";
    let toks = semantic_tokens(src);
    assert_token_on_line(&toks, src, 0, TokenType::Keyword, "include");
}

#[test]
fn top_level_include_keyword_from_yaml() {
    let src = "include:\n  - ./utils.nika.yaml\n";
    let toks = semantic_tokens(src);
    assert_token_on_line(&toks, src, 0, TokenType::Keyword, "include");
}

#[test]
fn top_level_edges_keyword() {
    let src = "edges:\n  - from: a\n    to: b\n";
    let toks = semantic_tokens(src);
    assert_token_on_line(&toks, src, 0, TokenType::Keyword, "edges");
}

#[test]
fn all_top_level_keywords() {
    let src = "\
schema: '@0.12'
workflow: test
tasks:
mcp:
context:
inputs:
include:
edges:
";
    let toks = semantic_tokens(src);
    let kw = tokens_of(&toks, TokenType::Keyword);
    let kw_texts: Vec<&str> = kw.iter().map(|t| token_text(src, t)).collect();
    for expected in &[
        "schema", "workflow", "tasks", "mcp", "context", "inputs", "include", "edges",
    ] {
        assert!(
            kw_texts.contains(expected),
            "missing keyword {expected:?}; got {kw_texts:?}",
        );
    }
}

#[test]
fn indented_key_not_keyword() {
    // "schema:" indented should NOT be tagged as Keyword (indent != 0).
    let src = "  schema: nested\n";
    let toks = semantic_tokens(src);
    let kw = tokens_of(&toks, TokenType::Keyword);
    assert!(kw.is_empty(), "indented 'schema' should not be a keyword");
}

// ===========================================================================
// 3. Task properties (id, with, depends_on, etc.) get TokenType::Property
// ===========================================================================

#[test]
fn property_id() {
    let src = "    id: my-task\n";
    let toks = semantic_tokens(src);
    assert_token_on_line(&toks, src, 0, TokenType::Property, "id");
}

#[test]
fn property_with() {
    let src = "    with:\n      doc: file.txt\n";
    let toks = semantic_tokens(src);
    assert_token_on_line(&toks, src, 0, TokenType::Property, "with");
}

#[test]
fn property_depends_on() {
    let src = "    depends_on: [step1, step2]\n";
    let toks = semantic_tokens(src);
    assert_token_on_line(&toks, src, 0, TokenType::Property, "depends_on");
}

#[test]
fn property_content() {
    let src = "    content: true\n";
    let toks = semantic_tokens(src);
    assert_token_on_line(&toks, src, 0, TokenType::Property, "content");
}

#[test]
fn property_for_each() {
    let src = "    for_each: items\n";
    let toks = semantic_tokens(src);
    assert_token_on_line(&toks, src, 0, TokenType::Property, "for_each");
}

#[test]
fn property_retry() {
    let src = "    retry:\n      max_attempts: 3\n";
    let toks = semantic_tokens(src);
    assert_token_on_line(&toks, src, 0, TokenType::Property, "retry");
}

#[test]
fn property_timeout() {
    let src = "    timeout: 30s\n";
    let toks = semantic_tokens(src);
    assert_token_on_line(&toks, src, 0, TokenType::Property, "timeout");
}

#[test]
fn property_guardrails() {
    let src = "    guardrails:\n      - max_tokens: 100\n";
    let toks = semantic_tokens(src);
    assert_token_on_line(&toks, src, 0, TokenType::Property, "guardrails");
}

#[test]
fn property_limits() {
    let src = "    limits:\n      max_tokens: 4096\n";
    let toks = semantic_tokens(src);
    assert_token_on_line(&toks, src, 0, TokenType::Property, "limits");
}

#[test]
fn property_provider() {
    let src = "    provider: openai\n";
    let toks = semantic_tokens(src);
    assert_token_on_line(&toks, src, 0, TokenType::Property, "provider");
}

#[test]
fn property_model() {
    let src = "    model: gpt-4o\n";
    let toks = semantic_tokens(src);
    assert_token_on_line(&toks, src, 0, TokenType::Property, "model");
}

#[test]
fn property_description() {
    let src = "    description: Summarize the document\n";
    let toks = semantic_tokens(src);
    assert_token_on_line(&toks, src, 0, TokenType::Property, "description");
}

#[test]
fn property_structured() {
    let src = "    structured: true\n";
    let toks = semantic_tokens(src);
    assert_token_on_line(&toks, src, 0, TokenType::Property, "structured");
}

#[test]
fn all_task_properties_in_one_task() {
    let src = "\
  - id: full-task
    description: A complete task
    infer: gpt-4
    content: true
    with:
      doc: file.txt
    depends_on: [setup]
    for_each: items
    retry:
      max_attempts: 3
    timeout: 30s
    guardrails:
      - check: length
    limits:
      max_tokens: 4096
    provider: openai
    model: gpt-4o
    structured: true
";
    let toks = semantic_tokens(src);
    let props = tokens_of(&toks, TokenType::Property);
    let prop_texts: Vec<&str> = props.iter().map(|t| token_text(src, t)).collect();
    for expected in &[
        "id",
        "description",
        "content",
        "with",
        "depends_on",
        "for_each",
        "retry",
        "timeout",
        "guardrails",
        "limits",
        "provider",
        "model",
        "structured",
    ] {
        assert!(
            prop_texts.contains(expected),
            "missing property {expected:?}; got {prop_texts:?}",
        );
    }
}

// ===========================================================================
// 4. Template {{...}} get TokenType::Variable
// ===========================================================================

#[test]
fn simple_template_variable() {
    let src = "  prompt: \"{{with.doc}}\"\n";
    let toks = semantic_tokens(src);
    let vars = tokens_of(&toks, TokenType::Variable);
    assert_eq!(vars.len(), 1);
    assert_eq!(token_text(src, vars[0]), "{{with.doc}}");
}

#[test]
fn multiple_templates_on_one_line() {
    let src = "  prompt: \"{{with.a}} and {{with.b}}\"\n";
    let toks = semantic_tokens(src);
    let vars = tokens_of(&toks, TokenType::Variable);
    assert_eq!(vars.len(), 2);
    assert_eq!(token_text(src, vars[0]), "{{with.a}}");
    assert_eq!(token_text(src, vars[1]), "{{with.b}}");
}

#[test]
fn template_on_multiple_lines() {
    let src = "\
  line1: \"{{with.x}}\"
  line2: \"{{with.y}}\"
";
    let toks = semantic_tokens(src);
    let vars = tokens_of(&toks, TokenType::Variable);
    assert_eq!(vars.len(), 2);
    assert_eq!(vars[0].line, 0);
    assert_eq!(vars[1].line, 1);
}

#[test]
fn template_without_with_prefix() {
    let src = "  x: \"{{inputs.name}}\"\n";
    let toks = semantic_tokens(src);
    let vars = tokens_of(&toks, TokenType::Variable);
    assert_eq!(vars.len(), 1);
    assert_eq!(token_text(src, vars[0]), "{{inputs.name}}");
}

#[test]
fn template_at_start_of_line() {
    let src = "{{global}}: value\n";
    let toks = semantic_tokens(src);
    let vars = tokens_of(&toks, TokenType::Variable);
    assert_eq!(vars.len(), 1);
    assert_eq!(vars[0].start_char, 0);
}

#[test]
fn unclosed_template_produces_no_variable() {
    let src = "  x: \"{{with.broken\"\n";
    let toks = semantic_tokens(src);
    let vars = tokens_of(&toks, TokenType::Variable);
    assert!(
        vars.is_empty(),
        "unclosed template should not produce a Variable token"
    );
}

#[test]
fn nested_braces_template() {
    // Only the first {{ to first }} should be matched.
    let src = "  x: \"{{with.a}} {{ }}\"\n";
    let toks = semantic_tokens(src);
    let vars = tokens_of(&toks, TokenType::Variable);
    assert_eq!(vars.len(), 2);
    assert_eq!(token_text(src, vars[0]), "{{with.a}}");
}

#[test]
fn empty_template_braces() {
    let src = "  x: \"{{}}\"\n";
    let toks = semantic_tokens(src);
    let vars = tokens_of(&toks, TokenType::Variable);
    assert_eq!(vars.len(), 1);
    assert_eq!(token_text(src, vars[0]), "{{}}");
}

// ===========================================================================
// 5. Transform pipes get TokenType::Function
// ===========================================================================

#[test]
fn pipe_transform_basic() {
    let src = "  x: \"{{with.doc | upper}}\"\n";
    let toks = semantic_tokens(src);
    let fns = tokens_of(&toks, TokenType::Function);
    assert_eq!(fns.len(), 1);
    assert_eq!(token_text(src, fns[0]), "upper");
}

#[test]
fn pipe_transform_trim_whitespace() {
    let src = "  x: \"{{with.doc |   lower  }}\"\n";
    let toks = semantic_tokens(src);
    let fns = tokens_of(&toks, TokenType::Function);
    assert_eq!(fns.len(), 1);
    assert_eq!(token_text(src, fns[0]), "lower");
}

#[test]
fn pipe_transform_also_has_variable() {
    let src = "  x: \"{{with.data | json}}\"\n";
    let toks = semantic_tokens(src);
    let vars = tokens_of(&toks, TokenType::Variable);
    let fns = tokens_of(&toks, TokenType::Function);
    assert_eq!(vars.len(), 1, "should have Variable for the template");
    assert_eq!(fns.len(), 1, "should have Function for the transform");
}

#[test]
fn no_pipe_no_function() {
    let src = "  x: \"{{with.doc}}\"\n";
    let toks = semantic_tokens(src);
    let fns = tokens_of(&toks, TokenType::Function);
    assert!(fns.is_empty(), "no pipe means no Function token");
}

#[test]
fn pipe_with_empty_transform_no_function() {
    // Edge case: pipe exists but nothing after it (just whitespace before }}).
    let src = "  x: \"{{with.doc | }}\"\n";
    let toks = semantic_tokens(src);
    let fns = tokens_of(&toks, TokenType::Function);
    assert!(
        fns.is_empty(),
        "empty transform after pipe should not emit Function"
    );
}

#[test]
fn multiple_templates_with_transforms() {
    let src = "  x: \"{{with.a | upper}} and {{with.b | lower}}\"\n";
    let toks = semantic_tokens(src);
    let fns = tokens_of(&toks, TokenType::Function);
    assert_eq!(fns.len(), 2);
    assert_eq!(token_text(src, fns[0]), "upper");
    assert_eq!(token_text(src, fns[1]), "lower");
}

// ===========================================================================
// 6. Comments get TokenType::Comment
// ===========================================================================

#[test]
fn single_comment_line() {
    let src = "# This is a comment\n";
    let toks = semantic_tokens(src);
    let comments = tokens_of(&toks, TokenType::Comment);
    assert_eq!(comments.len(), 1);
    assert_eq!(comments[0].line, 0);
    assert_eq!(token_text(src, comments[0]), "# This is a comment");
}

#[test]
fn indented_comment() {
    let src = "    # indented comment\n";
    let toks = semantic_tokens(src);
    let comments = tokens_of(&toks, TokenType::Comment);
    assert_eq!(comments.len(), 1);
    assert_eq!(comments[0].start_char, 4);
    assert_eq!(token_text(src, comments[0]), "# indented comment");
}

#[test]
fn multiple_comments() {
    let src = "\
# Comment 1
schema: '@0.12'
# Comment 2
tasks:
  # Comment 3
";
    let toks = semantic_tokens(src);
    let comments = tokens_of(&toks, TokenType::Comment);
    assert_eq!(comments.len(), 3);
    assert_eq!(comments[0].line, 0);
    assert_eq!(comments[1].line, 2);
    assert_eq!(comments[2].line, 4);
}

#[test]
fn comment_skips_key_extraction() {
    // A comment line that looks like "# tasks:" should only get Comment, not Keyword.
    let src = "# tasks: this is a comment\n";
    let toks = semantic_tokens(src);
    assert_eq!(toks.len(), 1);
    assert_eq!(toks[0].token_type, TokenType::Comment);
}

#[test]
fn hash_only_comment() {
    let src = "#\n";
    let toks = semantic_tokens(src);
    let comments = tokens_of(&toks, TokenType::Comment);
    assert_eq!(comments.len(), 1);
    assert_eq!(comments[0].length, 1);
}

// ===========================================================================
// 7. content type values get TokenType::Type
// ===========================================================================

#[test]
fn type_key_produces_type_token() {
    let src = "      type: text\n";
    let toks = semantic_tokens(src);
    let types = tokens_of(&toks, TokenType::Type);
    assert_eq!(types.len(), 1);
    assert_eq!(token_text(src, types[0]), "type");
}

#[test]
fn type_key_at_various_indents() {
    let src = "\
  type: image
        type: audio
type: text
";
    let toks = semantic_tokens(src);
    let types = tokens_of(&toks, TokenType::Type);
    // "type" at indent 0 is still Type (not Keyword, since "type" is not in TOP_KEYS).
    assert_eq!(types.len(), 3);
}

#[test]
fn type_key_in_content_block() {
    let src = "\
    content:
      - type: text
        text: hello
      - type: image
        url: https://example.com/img.png
";
    let toks = semantic_tokens(src);
    let types = tokens_of(&toks, TokenType::Type);
    assert_eq!(types.len(), 2);
    // Both should report "type" as the text.
    for t in &types {
        assert_eq!(token_text(src, t), "type");
    }
}

// ===========================================================================
// 8. encode_tokens produces valid delta encoding
// ===========================================================================

#[test]
fn encode_single_token() {
    let tokens = vec![RawToken {
        line: 0,
        start_char: 0,
        length: 6,
        token_type: TokenType::Keyword,
        modifiers: 0,
    }];
    let data = encode_tokens(&tokens);
    assert_eq!(data, vec![0, 0, 6, 0, 0]); // deltaLine=0, deltaStart=0, len=6, type=0, mod=0
}

#[test]
fn encode_two_tokens_same_line() {
    let tokens = vec![
        RawToken {
            line: 0,
            start_char: 0,
            length: 6,
            token_type: TokenType::Keyword,
            modifiers: 0,
        },
        RawToken {
            line: 0,
            start_char: 8,
            length: 5,
            token_type: TokenType::Variable,
            modifiers: 0,
        },
    ];
    let data = encode_tokens(&tokens);
    // Token 1: dL=0, dS=0, len=6, type=0(Keyword), mod=0
    // Token 2: dL=0, dS=8, len=5, type=3(Variable), mod=0
    assert_eq!(data, vec![0, 0, 6, 0, 0, 0, 8, 5, 3, 0]);
}

#[test]
fn encode_tokens_across_lines() {
    let tokens = vec![
        RawToken {
            line: 0,
            start_char: 0,
            length: 6,
            token_type: TokenType::Keyword,
            modifiers: 0,
        },
        RawToken {
            line: 2,
            start_char: 4,
            length: 5,
            token_type: TokenType::Verb,
            modifiers: 0,
        },
    ];
    let data = encode_tokens(&tokens);
    // Token 1: dL=0, dS=0, len=6, type=0, mod=0
    // Token 2: dL=2, dS=4 (new line so absolute startChar), len=5, type=1, mod=0
    assert_eq!(data, vec![0, 0, 6, 0, 0, 2, 4, 5, 1, 0]);
}

#[test]
fn encode_empty_tokens() {
    let data = encode_tokens(&[]);
    assert!(data.is_empty());
}

#[test]
fn encode_preserves_modifiers() {
    let tokens = vec![RawToken {
        line: 0,
        start_char: 0,
        length: 3,
        token_type: TokenType::Comment,
        modifiers: 42,
    }];
    let data = encode_tokens(&tokens);
    assert_eq!(data[4], 42); // modifiers preserved
}

#[test]
fn encode_all_token_type_indices() {
    // Verify the index mapping for every TokenType variant.
    assert_eq!(TokenType::Keyword.index(), 0);
    assert_eq!(TokenType::Verb.index(), 1);
    assert_eq!(TokenType::Property.index(), 2);
    assert_eq!(TokenType::Variable.index(), 3);
    assert_eq!(TokenType::Function.index(), 4);
    assert_eq!(TokenType::Type.index(), 5);
    assert_eq!(TokenType::Comment.index(), 6);
}

#[test]
fn encode_tokens_output_length_is_multiple_of_five() {
    let src = "\
schema: '@0.12'
# comment
tasks:
  - id: t
    infer: gpt-4
    with:
      doc: \"{{with.file | upper}}\"
";
    let toks = semantic_tokens(src);
    let data = encode_tokens(&toks);
    assert_eq!(
        data.len() % 5,
        0,
        "encoded data length must be a multiple of 5, got {}",
        data.len()
    );
}

#[test]
fn encode_delta_line_resets_start_char() {
    // When moving to a new line, start_char should be absolute (not delta from prev).
    let tokens = vec![
        RawToken {
            line: 0,
            start_char: 10,
            length: 3,
            token_type: TokenType::Keyword,
            modifiers: 0,
        },
        RawToken {
            line: 1,
            start_char: 2,
            length: 4,
            token_type: TokenType::Property,
            modifiers: 0,
        },
    ];
    let data = encode_tokens(&tokens);
    // Token 2: dL=1 (new line), dS=2 (absolute, not 2-10=-8)
    assert_eq!(data[5], 1); // deltaLine
    assert_eq!(data[6], 2); // deltaStartChar = absolute start_char on new line
}

#[test]
fn encode_three_tokens_same_line_deltas() {
    let tokens = vec![
        RawToken {
            line: 5,
            start_char: 0,
            length: 2,
            token_type: TokenType::Variable,
            modifiers: 0,
        },
        RawToken {
            line: 5,
            start_char: 5,
            length: 3,
            token_type: TokenType::Function,
            modifiers: 0,
        },
        RawToken {
            line: 5,
            start_char: 12,
            length: 4,
            token_type: TokenType::Variable,
            modifiers: 0,
        },
    ];
    let data = encode_tokens(&tokens);
    // Token 1: dL=5, dS=0
    assert_eq!(data[0], 5);
    assert_eq!(data[1], 0);
    // Token 2: dL=0, dS=5 (5-0)
    assert_eq!(data[5], 0);
    assert_eq!(data[6], 5);
    // Token 3: dL=0, dS=7 (12-5)
    assert_eq!(data[10], 0);
    assert_eq!(data[11], 7);
}

// ===========================================================================
// 9. Empty file produces no tokens
// ===========================================================================

#[test]
fn empty_string_no_tokens() {
    assert!(semantic_tokens("").is_empty());
}

#[test]
fn whitespace_only_no_tokens() {
    assert!(semantic_tokens("   \n\n  \n").is_empty());
}

#[test]
fn newlines_only_no_tokens() {
    assert!(semantic_tokens("\n\n\n").is_empty());
}

// ===========================================================================
// 10. Large workflow (20 tasks) - all tokens correct
// ===========================================================================

#[test]
fn twenty_task_workflow_all_verbs_and_ids() {
    let verbs = ["infer", "exec", "fetch", "invoke", "agent"];
    let mut yaml = String::from("schema: '@0.12'\nworkflow: big-pipeline\ntasks:\n");
    for i in 0..20 {
        let verb = verbs[i % 5];
        yaml.push_str(&format!("  - id: task_{i}\n    {verb}: value_{i}\n"));
    }

    let toks = semantic_tokens(&yaml);

    // Check top-level keywords
    let kw = tokens_of(&toks, TokenType::Keyword);
    let kw_texts: Vec<&str> = kw.iter().map(|t| token_text(&yaml, t)).collect();
    assert!(kw_texts.contains(&"schema"));
    assert!(kw_texts.contains(&"workflow"));
    assert!(kw_texts.contains(&"tasks"));

    // Check all 20 id properties
    let props = tokens_of(&toks, TokenType::Property);
    let id_props: Vec<_> = props
        .iter()
        .filter(|t| token_text(&yaml, t) == "id")
        .collect();
    assert_eq!(id_props.len(), 20, "should have 20 'id' property tokens");

    // Check all 20 verb tokens (4 of each verb type)
    let verb_toks = tokens_of(&toks, TokenType::Verb);
    assert_eq!(verb_toks.len(), 20, "should have 20 verb tokens");
    for v in &verbs {
        let count = verb_toks
            .iter()
            .filter(|t| token_text(&yaml, t) == *v)
            .count();
        assert_eq!(count, 4, "verb {v} should appear 4 times, got {count}");
    }
}

#[test]
fn twenty_task_workflow_with_templates_and_transforms() {
    let mut yaml = String::from("schema: '@0.12'\ntasks:\n");
    for i in 0..20 {
        yaml.push_str(&format!(
            "  - id: task_{i}\n    infer: \"{{{{with.data_{i} | transform_{i}}}}}\"\n"
        ));
    }

    let toks = semantic_tokens(&yaml);

    let vars = tokens_of(&toks, TokenType::Variable);
    assert_eq!(vars.len(), 20, "should have 20 Variable tokens");

    let fns = tokens_of(&toks, TokenType::Function);
    assert_eq!(fns.len(), 20, "should have 20 Function tokens");

    // Verify the transform names
    for i in 0..20 {
        let expected_fn = format!("transform_{i}");
        assert!(
            fns.iter().any(|t| token_text(&yaml, t) == expected_fn),
            "missing function {expected_fn}",
        );
    }
}

#[test]
fn twenty_task_encode_is_valid() {
    let mut yaml = String::from("schema: '@0.12'\ntasks:\n");
    for i in 0..20 {
        yaml.push_str(&format!("  - id: task_{i}\n    exec: echo {i}\n"));
    }

    let toks = semantic_tokens(&yaml);
    let data = encode_tokens(&toks);

    assert_eq!(data.len() % 5, 0);
    assert_eq!(data.len() / 5, toks.len());

    // Verify all delta lines are non-negative by checking reconstruction.
    let mut prev_line = 0u32;
    let mut prev_start = 0u32;
    for chunk in data.chunks(5) {
        let dl = chunk[0];
        let ds = chunk[1];
        let abs_line = prev_line + dl;
        let abs_start = if dl == 0 { prev_start + ds } else { ds };
        // Absolute line should be within the document.
        let line_count = yaml.lines().count() as u32;
        assert!(
            abs_line < line_count,
            "reconstructed line {abs_line} >= line count {line_count}",
        );
        prev_line = abs_line;
        prev_start = abs_start;
    }
}

// ===========================================================================
// Combined: realistic full workflow
// ===========================================================================

#[test]
fn realistic_workflow_all_token_types() {
    let src = "\
# Nika workflow for document processing
schema: '@0.12'
workflow: doc-processor
inputs:
  document_path: string
context:
  files:
    readme: ./README.md
mcp:
  novanet:
    command: novanet serve
tasks:
  - id: load
    exec: cat document
    description: Load the document
  - id: summarize
    infer: gpt-4
    content:
      - type: text
        text: \"Summarize: {{with.doc | trim}}\"
    with:
      doc: \"{{tasks.load.output}}\"
    depends_on: [load]
    provider: openai
    model: gpt-4o
    timeout: 60s
    retry:
      max_attempts: 3
  # Final step: publish
  - id: publish
    invoke: novanet.write
    depends_on: [summarize]
";
    let toks = semantic_tokens(src);

    // Comments
    let comments = tokens_of(&toks, TokenType::Comment);
    assert!(
        comments.len() >= 2,
        "should have at least 2 comment tokens (header + inline)",
    );

    // Keywords
    let kw = tokens_of(&toks, TokenType::Keyword);
    let kw_texts: Vec<&str> = kw.iter().map(|t| token_text(src, t)).collect();
    for expected in &["schema", "workflow", "inputs", "context", "mcp", "tasks"] {
        assert!(kw_texts.contains(expected), "missing keyword {expected}");
    }

    // Verbs
    let verb_toks = tokens_of(&toks, TokenType::Verb);
    let verb_texts: Vec<&str> = verb_toks.iter().map(|t| token_text(src, t)).collect();
    assert!(verb_texts.contains(&"exec"));
    assert!(verb_texts.contains(&"infer"));
    assert!(verb_texts.contains(&"invoke"));

    // Properties
    let prop_toks = tokens_of(&toks, TokenType::Property);
    let prop_texts: Vec<&str> = prop_toks.iter().map(|t| token_text(src, t)).collect();
    for expected in &[
        "id",
        "description",
        "content",
        "with",
        "depends_on",
        "provider",
        "model",
        "timeout",
        "retry",
    ] {
        assert!(
            prop_texts.contains(expected),
            "missing property {expected}; got {prop_texts:?}",
        );
    }

    // Variables (templates)
    let var_toks = tokens_of(&toks, TokenType::Variable);
    assert!(
        var_toks.len() >= 2,
        "should have at least 2 template variables",
    );

    // Functions (transforms)
    let fn_toks = tokens_of(&toks, TokenType::Function);
    assert!(
        !fn_toks.is_empty(),
        "should have at least 1 transform function (trim)",
    );
    assert!(fn_toks.iter().any(|t| token_text(src, t) == "trim"));

    // Type tokens
    let type_toks = tokens_of(&toks, TokenType::Type);
    assert!(
        !type_toks.is_empty(),
        "should have at least one 'type' token from the content block",
    );

    // Encoding is valid
    let data = encode_tokens(&toks);
    assert_eq!(data.len() % 5, 0);
    assert_eq!(data.len() / 5, toks.len());
}

// ===========================================================================
// Edge cases and regression guards
// ===========================================================================

#[test]
fn key_with_no_value_after_colon() {
    let src = "tasks:\n";
    let toks = semantic_tokens(src);
    assert_token_on_line(&toks, src, 0, TokenType::Keyword, "tasks");
}

#[test]
fn unknown_key_produces_no_token() {
    let src = "    unknown_key: some value\n";
    let toks = semantic_tokens(src);
    assert!(toks.is_empty(), "unknown keys should not produce tokens");
}

#[test]
fn line_with_colon_but_empty_key() {
    let src = "    : value\n";
    let toks = semantic_tokens(src);
    assert!(
        toks.is_empty(),
        "empty key before colon should produce no token",
    );
}

#[test]
fn template_and_key_on_same_line() {
    // A line that has both a recognized key and a template.
    let src = "    id: \"{{with.name}}\"\n";
    let toks = semantic_tokens(src);
    let props = tokens_of(&toks, TokenType::Property);
    let vars = tokens_of(&toks, TokenType::Variable);
    assert_eq!(props.len(), 1);
    assert_eq!(vars.len(), 1);
    assert_eq!(token_text(src, props[0]), "id");
    assert_eq!(token_text(src, vars[0]), "{{with.name}}");
}

#[test]
fn verb_and_template_on_same_line() {
    let src = "    infer: \"{{with.prompt}}\"\n";
    let toks = semantic_tokens(src);
    let verbs = tokens_of(&toks, TokenType::Verb);
    let vars = tokens_of(&toks, TokenType::Variable);
    assert_eq!(verbs.len(), 1);
    assert_eq!(vars.len(), 1);
}

#[test]
fn comment_with_template_syntax_only_comment() {
    // A comment that contains {{ should only be Comment, not Variable.
    let src = "# {{this.is.not.a.variable}}\n";
    let toks = semantic_tokens(src);
    assert_eq!(toks.len(), 1);
    assert_eq!(toks[0].token_type, TokenType::Comment);
}

#[test]
fn consecutive_templates_no_space() {
    let src = "  x: \"{{a}}{{b}}\"\n";
    let toks = semantic_tokens(src);
    let vars = tokens_of(&toks, TokenType::Variable);
    assert_eq!(vars.len(), 2);
    assert_eq!(token_text(src, vars[0]), "{{a}}");
    assert_eq!(token_text(src, vars[1]), "{{b}}");
}

#[test]
fn token_positions_are_byte_accurate() {
    let src = "schema: '@0.12'\n";
    let toks = semantic_tokens(src);
    let kw = tokens_of(&toks, TokenType::Keyword);
    assert_eq!(kw.len(), 1);
    assert_eq!(kw[0].line, 0);
    assert_eq!(kw[0].start_char, 0);
    assert_eq!(kw[0].length, 6); // "schema" is 6 chars
}

#[test]
fn multiline_workflow_token_ordering() {
    let src = "\
schema: '@0.12'
tasks:
  - id: t
    infer: gpt-4
";
    let toks = semantic_tokens(src);
    // Tokens should be ordered by line (and within a line, by start_char).
    for pair in toks.windows(2) {
        assert!(
            pair[0].line < pair[1].line
                || (pair[0].line == pair[1].line && pair[0].start_char <= pair[1].start_char),
            "tokens not ordered: {:?} should come before {:?}",
            pair[0],
            pair[1],
        );
    }
}

#[test]
fn encode_roundtrip_matches_raw_tokens() {
    // Use a workflow where no line has both a key token and a template,
    // so the natural emission order from semantic_tokens() is already sorted.
    let src = "\
schema: '@0.12'
# A comment
tasks:
  - id: gen
    infer: gpt-4
    with:
      doc: file.txt
    depends_on: [setup]
";
    let toks = semantic_tokens(src);
    let data = encode_tokens(&toks);

    // Reconstruct absolute positions from deltas and compare.
    let mut prev_line = 0u32;
    let mut prev_start = 0u32;
    for (i, chunk) in data.chunks(5).enumerate() {
        let dl = chunk[0];
        let ds = chunk[1];
        let length = chunk[2];
        let type_idx = chunk[3];
        let modifiers = chunk[4];

        let abs_line = prev_line + dl;
        let abs_start = if dl == 0 { prev_start + ds } else { ds };

        assert_eq!(abs_line, toks[i].line, "token {i}: line mismatch");
        assert_eq!(
            abs_start, toks[i].start_char,
            "token {i}: start_char mismatch"
        );
        assert_eq!(length, toks[i].length, "token {i}: length mismatch");
        assert_eq!(
            type_idx,
            toks[i].token_type.index(),
            "token {i}: type mismatch"
        );
        assert_eq!(
            modifiers, toks[i].modifiers,
            "token {i}: modifiers mismatch"
        );

        prev_line = abs_line;
        prev_start = abs_start;
    }
}

#[test]
fn encode_roundtrip_with_sorted_tokens() {
    // When a line has both a key and a template, semantic_tokens() emits
    // Variable/Function before the key token. Callers must sort before
    // encoding. This test verifies the sorted-then-encoded roundtrip.
    let src = "\
schema: '@0.12'
# A comment
tasks:
  - id: gen
    infer: \"{{with.doc | upper}}\"
    with:
      doc: file.txt
";
    let mut toks = semantic_tokens(src);
    toks.sort_by(|a, b| a.line.cmp(&b.line).then(a.start_char.cmp(&b.start_char)));

    let data = encode_tokens(&toks);

    let mut prev_line = 0u32;
    let mut prev_start = 0u32;
    for (i, chunk) in data.chunks(5).enumerate() {
        let dl = chunk[0];
        let ds = chunk[1];
        let length = chunk[2];
        let type_idx = chunk[3];
        let modifiers = chunk[4];

        let abs_line = prev_line + dl;
        let abs_start = if dl == 0 { prev_start + ds } else { ds };

        assert_eq!(abs_line, toks[i].line, "token {i}: line mismatch");
        assert_eq!(
            abs_start, toks[i].start_char,
            "token {i}: start_char mismatch"
        );
        assert_eq!(length, toks[i].length, "token {i}: length mismatch");
        assert_eq!(
            type_idx,
            toks[i].token_type.index(),
            "token {i}: type mismatch"
        );
        assert_eq!(
            modifiers, toks[i].modifiers,
            "token {i}: modifiers mismatch"
        );

        prev_line = abs_line;
        prev_start = abs_start;
    }
}

#[test]
fn template_before_key_on_same_line_ordering() {
    // semantic_tokens() emits template tokens before key tokens per line.
    // This documents the emission order so callers know to sort.
    let src = "    infer: \"{{with.prompt}}\"\n";
    let toks = semantic_tokens(src);
    assert!(toks.len() >= 2);
    // Variable is emitted before Verb because templates are scanned first.
    let first = &toks[0];
    let last = &toks[toks.len() - 1];
    assert_eq!(first.token_type, TokenType::Variable);
    assert_eq!(last.token_type, TokenType::Verb);
    // But the Verb's start_char is smaller (it's the key at the start).
    assert!(last.start_char < first.start_char);
}
