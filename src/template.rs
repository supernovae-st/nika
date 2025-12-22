//! Single-pass template resolver with caching
//!
//! This module provides an optimized template resolution system that:
//! - Tokenizes templates once and caches the result
//! - Performs single-pass resolution instead of 3 regex passes
//! - Uses Arc for zero-copy sharing of tokenized templates

use crate::runner::ExecutionContext;
use anyhow::Result;
use dashmap::DashMap;
use once_cell::sync::Lazy;
use std::ops::Range;
use std::sync::Arc;

/// Token representing a parsed template fragment
#[derive(Debug, Clone)]
pub enum Token {
    /// Literal text (stores range in original string)
    Literal(Range<usize>),
    /// Task reference: {{task_id}} or {{task_id.field}}
    TaskRef {
        task_id: String,
        field: Option<String>
    },
    /// Environment variable: ${env.NAME}
    EnvVar(String),
    /// Input parameter: ${input.name}
    Input(String),
}

/// Template resolver with caching
pub struct TemplateResolver {
    /// Cache of parsed templates
    cache: DashMap<String, Arc<Vec<Token>>>,
}

impl Default for TemplateResolver {
    fn default() -> Self {
        Self::new()
    }
}

impl TemplateResolver {
    /// Create a new template resolver
    pub fn new() -> Self {
        Self {
            cache: DashMap::new(),
        }
    }

    /// Parse template into tokens (with caching)
    pub fn tokenize(&self, template: &str) -> Arc<Vec<Token>> {
        // Check cache first
        if let Some(cached) = self.cache.get(template) {
            return Arc::clone(&cached);
        }

        // Parse template
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
                    if let Some((task_id, field, end_pos)) = self.parse_task_ref(&mut chars, i + 2, template) {
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
                    if let Some((token, end_pos)) = self.parse_dollar_ref(&mut chars, i + 2, template) {
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

        // Cache and return
        let tokens = Arc::new(tokens);
        self.cache.insert(template.to_string(), tokens.clone());
        tokens
    }

    /// Parse {{task_id}} or {{task_id.field}}
    fn parse_task_ref(
        &self,
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

    /// Parse ${env.NAME} or ${input.name}
    fn parse_dollar_ref(
        &self,
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
            }
        }

        None
    }

    /// Resolve template using pre-parsed tokens
    pub fn resolve(&self, template: &str, ctx: &ExecutionContext) -> Result<String> {
        let tokens = self.tokenize(template);

        // Pre-allocate with estimated size (template usually grows during resolution)
        let mut result = String::with_capacity(template.len() * 2);

        for token in tokens.iter() {
            match token {
                Token::Literal(range) => {
                    result.push_str(&template[range.clone()]);
                }
                Token::TaskRef { task_id, field } => {
                    if let Some(field_name) = field {
                        if let Some(value) = ctx.get_field(task_id, field_name) {
                            result.push_str(&value);
                        } else {
                            // Keep original template if not found
                            result.push_str(&format!("{{{{{}:{}}}}}", task_id, field_name));
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
}

/// Global template resolver instance
pub static TEMPLATE_RESOLVER: Lazy<TemplateResolver> = Lazy::new(TemplateResolver::new);

/// Convenience function for resolving templates
pub fn resolve_templates(template: &str, ctx: &ExecutionContext) -> Result<String> {
    TEMPLATE_RESOLVER.resolve(template, ctx)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tokenize_simple_literal() {
        let resolver = TemplateResolver::new();
        let tokens = resolver.tokenize("simple text");
        assert_eq!(tokens.len(), 1);
        matches!(&tokens[0], Token::Literal(range) if range.start == 0 && range.end == 11);
    }

    #[test]
    fn test_tokenize_task_ref() {
        let resolver = TemplateResolver::new();
        let tokens = resolver.tokenize("{{task1}}");
        assert_eq!(tokens.len(), 1);
        matches!(&tokens[0], Token::TaskRef { task_id, field }
            if task_id == "task1" && field.is_none());
    }

    #[test]
    fn test_tokenize_task_ref_with_field() {
        let resolver = TemplateResolver::new();
        let tokens = resolver.tokenize("{{task1.result}}");
        assert_eq!(tokens.len(), 1);
        matches!(&tokens[0], Token::TaskRef { task_id, field }
            if task_id == "task1" && field.as_deref() == Some("result"));
    }

    #[test]
    fn test_tokenize_env_var() {
        let resolver = TemplateResolver::new();
        let tokens = resolver.tokenize("${env.HOME}");
        assert_eq!(tokens.len(), 1);
        matches!(&tokens[0], Token::EnvVar(name) if name == "HOME");
    }

    #[test]
    fn test_tokenize_input() {
        let resolver = TemplateResolver::new();
        let tokens = resolver.tokenize("${input.filename}");
        assert_eq!(tokens.len(), 1);
        matches!(&tokens[0], Token::Input(name) if name == "filename");
    }

    #[test]
    fn test_tokenize_mixed() {
        let resolver = TemplateResolver::new();
        let tokens = resolver.tokenize("Process {{task1}} with ${input.file} in ${env.HOME}");
        assert_eq!(tokens.len(), 6);
        // Should have: Literal, TaskRef, Literal, Input, Literal, EnvVar
    }

    #[test]
    fn test_cache_reuse() {
        let resolver = TemplateResolver::new();
        let template = "{{task1}} ${env.HOME}}";

        let tokens1 = resolver.tokenize(template);
        let tokens2 = resolver.tokenize(template);

        // Should be the same Arc
        assert!(Arc::ptr_eq(&tokens1, &tokens2));
    }
}