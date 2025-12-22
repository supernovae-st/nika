//! Single-pass template resolver
//!
//! This module provides an optimized template resolution system that:
//! - Performs single-pass tokenization instead of 3 regex passes
//! - Keeps logic simple for CLI single-shot execution (no caching overhead)

use crate::runner::context::ContextReader;
use crate::runner::ExecutionContext;
use anyhow::Result;
use std::ops::Range;

/// Token representing a parsed template fragment
#[derive(Debug, Clone)]
pub enum Token {
    /// Literal text (stores range in original string)
    Literal(Range<usize>),
    /// Task reference: {{task_id}} or {{task_id.field}}
    TaskRef {
        task_id: String,
        field: Option<String>,
    },
    /// Environment variable: ${env.NAME}
    EnvVar(String),
    /// Input parameter: ${input.name}
    Input(String),
}

/// Tokenize a template string into tokens (single-pass)
///
/// For CLI single-shot execution, no caching is needed - each template
/// is typically resolved only once during workflow execution.
pub fn tokenize(template: &str) -> Vec<Token> {
    let mut tokens = Vec::new();
    let mut chars = template.char_indices().peekable();
    let mut current_literal_start = 0;

    while let Some((i, ch)) = chars.next() {
        match ch {
            '{' if chars.peek().map(|(_, c)| *c) == Some('{') => {
                // Flush literal if any
                if i > current_literal_start {
                    tokens.push(Token::Literal(current_literal_start..i));
                }
                chars.next(); // consume second '{'

                // Parse {{task_id}} or {{task_id.field}}
                if let Some((task_id, field, end_pos)) = parse_task_ref(&mut chars, i + 2, template)
                {
                    tokens.push(Token::TaskRef { task_id, field });
                    current_literal_start = end_pos;
                }
            }
            '$' if chars.peek().map(|(_, c)| *c) == Some('{') => {
                // Flush literal if any
                if i > current_literal_start {
                    tokens.push(Token::Literal(current_literal_start..i));
                }
                chars.next(); // consume '{'

                // Parse ${env.NAME} or ${input.name}
                if let Some((token, end_pos)) = parse_dollar_ref(&mut chars, i + 2, template) {
                    tokens.push(token);
                    current_literal_start = end_pos;
                }
            }
            _ => {} // Part of literal
        }
    }

    // Flush remaining literal
    if current_literal_start < template.len() {
        tokens.push(Token::Literal(current_literal_start..template.len()));
    }

    tokens
}

/// Parse {{task_id}} or {{task_id.field}}
fn parse_task_ref(
    chars: &mut std::iter::Peekable<std::str::CharIndices>,
    start_pos: usize,
    template: &str,
) -> Option<(String, Option<String>, usize)> {
    // Find the closing }}
    while let Some((i, ch)) = chars.peek() {
        if *ch == '}' {
            let i = *i;
            chars.next();
            if chars.peek().map(|(_, c)| *c) == Some('}') {
                chars.next(); // consume second '}'

                // Extract task_id and field
                let content = &template[start_pos..i];
                let parts: Vec<&str> = content.split('.').collect();

                if !parts.is_empty() && !parts[0].is_empty() {
                    let task_id = parts[0].to_string();
                    let field = if parts.len() > 1 {
                        Some(parts[1].to_string())
                    } else {
                        None
                    };
                    return Some((task_id, field, i + 2));
                }
            }
            break;
        } else {
            chars.next();
        }
    }

    None
}

/// Parse ${env.NAME}, ${input.name}, or ${task-id.output}
fn parse_dollar_ref(
    chars: &mut std::iter::Peekable<std::str::CharIndices>,
    start_pos: usize,
    template: &str,
) -> Option<(Token, usize)> {
    // Find the closing }
    let mut end_pos = start_pos;
    while let Some((i, ch)) = chars.peek() {
        if *ch == '}' {
            end_pos = *i;
            chars.next();
            break;
        }
        chars.next();
    }

    if end_pos > start_pos {
        let content = &template[start_pos..end_pos];
        if let Some(env_name) = content.strip_prefix("env.") {
            return Some((Token::EnvVar(env_name.to_string()), end_pos + 1));
        } else if let Some(input_name) = content.strip_prefix("input.") {
            return Some((Token::Input(input_name.to_string()), end_pos + 1));
        } else if content.contains('.') {
            // Support ${task-id.output} syntax (v4.7.1)
            let parts: Vec<&str> = content.splitn(2, '.').collect();
            if parts.len() == 2 {
                let task_id = parts[0].to_string();
                let field = parts[1].to_string();
                return Some((
                    Token::TaskRef {
                        task_id,
                        field: Some(field),
                    },
                    end_pos + 1,
                ));
            }
        }
    }

    None
}

/// Resolve a template string using the provided context
///
/// Performs single-pass tokenization and resolution in one step.
pub fn resolve_templates(template: &str, ctx: &ExecutionContext) -> Result<String> {
    let tokens = tokenize(template);

    // Pre-allocate with estimated size (template usually grows during resolution)
    let mut result = String::with_capacity(template.len() * 2);

    for token in &tokens {
        match token {
            Token::Literal(range) => {
                result.push_str(&template[range.clone()]);
            }
            Token::TaskRef { task_id, field } => {
                if let Some(field_name) = field {
                    // Special case: "output" field means get the task output
                    if field_name == "output" {
                        if let Some(output) = ctx.get_output(task_id) {
                            result.push_str(output);
                        } else {
                            // Keep original template if not found
                            result.push_str(&format!("${{{}.output}}", task_id));
                        }
                    } else if let Some(value) = ctx.get_field(task_id, field_name) {
                        result.push_str(&value);
                    } else {
                        // Keep original template if not found
                        result.push_str(&format!("${{{}.{}}}", task_id, field_name));
                    }
                } else if let Some(output) = ctx.get_output(task_id) {
                    result.push_str(output);
                } else {
                    // Keep original template if not found
                    result.push_str(&format!("{{{{{}}}}}", task_id));
                }
            }
            Token::EnvVar(var) => {
                if let Some(value) = ctx.get_env(var) {
                    result.push_str(&value);
                } else {
                    // Keep original template if not found
                    result.push_str(&format!("${{env.{}}}", var));
                }
            }
            Token::Input(field) => {
                if let Some(value) = ctx.get_input(field) {
                    result.push_str(value);
                } else {
                    // Keep original template if not found
                    result.push_str(&format!("${{input.{}}}", field));
                }
            }
        }
    }

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tokenize_simple_literal() {
        let tokens = tokenize("simple text");
        assert_eq!(tokens.len(), 1);
        assert!(matches!(&tokens[0], Token::Literal(range) if range.start == 0 && range.end == 11));
    }

    #[test]
    fn test_tokenize_task_ref() {
        let tokens = tokenize("{{task1}}");
        assert_eq!(tokens.len(), 1);
        assert!(matches!(&tokens[0], Token::TaskRef { task_id, field }
            if task_id == "task1" && field.is_none()));
    }

    #[test]
    fn test_tokenize_task_ref_with_field() {
        let tokens = tokenize("{{task1.result}}");
        assert_eq!(tokens.len(), 1);
        assert!(matches!(&tokens[0], Token::TaskRef { task_id, field }
            if task_id == "task1" && field.as_deref() == Some("result")));
    }

    #[test]
    fn test_tokenize_env_var() {
        let tokens = tokenize("${env.HOME}");
        assert_eq!(tokens.len(), 1);
        assert!(matches!(&tokens[0], Token::EnvVar(name) if name == "HOME"));
    }

    #[test]
    fn test_tokenize_input() {
        let tokens = tokenize("${input.filename}");
        assert_eq!(tokens.len(), 1);
        assert!(matches!(&tokens[0], Token::Input(name) if name == "filename"));
    }

    #[test]
    fn test_tokenize_mixed() {
        let tokens = tokenize("Process {{task1}} with ${input.file} in ${env.HOME}");
        assert_eq!(tokens.len(), 6);
        // Should have: Literal, TaskRef, Literal, Input, Literal, EnvVar
    }

    #[test]
    fn test_tokenize_empty_template() {
        let tokens = tokenize("");
        assert_eq!(tokens.len(), 0);
    }

    #[test]
    fn test_tokenize_only_literal() {
        let tokens = tokenize("no templates here at all");
        assert_eq!(tokens.len(), 1);
        assert!(matches!(&tokens[0], Token::Literal(range) if range.start == 0 && range.end == 24));
    }

    #[test]
    fn test_tokenize_dollar_task_output() {
        // v4.7.1: Support ${task-id.output} syntax
        let tokens = tokenize("Result: ${get-time.output}");
        assert_eq!(tokens.len(), 2);
        assert!(matches!(&tokens[0], Token::Literal(_)));
        assert!(matches!(&tokens[1], Token::TaskRef { task_id, field }
            if task_id == "get-time" && field.as_deref() == Some("output")));
    }

    #[test]
    fn test_tokenize_dollar_task_field() {
        // v4.7.1: Support ${task-id.field} syntax
        let tokens = tokenize("Name: ${user.name}");
        assert_eq!(tokens.len(), 2);
        assert!(matches!(&tokens[1], Token::TaskRef { task_id, field }
            if task_id == "user" && field.as_deref() == Some("name")));
    }
}
