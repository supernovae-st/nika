// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Transform expression parsing — pipe chains and parametric transforms.

use serde_json::Value;
use smallvec::SmallVec;

use super::helpers::{parse_default_value, split_parametric_args, strip_quotes};
use super::{TransformError, TransformExpr, TransformOp, TransformParseError, KNOWN_TRANSFORM_NAMES};


// ═══════════════════════════════════════════════════════════════
// TransformExpr
// ═══════════════════════════════════════════════════════════════

/// Split a pipe-separated transform chain while respecting parentheses and quotes.
/// e.g. `join(" | ")` is one segment, not split on the inner `|`.
pub(crate) fn split_pipe_respecting_parens(input: &str) -> Vec<&str> {
    let mut result = Vec::new();
    let mut depth: u32 = 0;
    let mut quote_char: Option<char> = None;
    let mut start = 0;
    for (i, c) in input.char_indices() {
        match c {
            // Track quotes only inside parenthesized arguments.
            // This handles `join(", ")` and `join(' | ')` correctly
            // while ignoring top-level apostrophes (`it's a test | upper`).
            '"' | '\'' if depth > 0 => {
                if quote_char == Some(c) {
                    quote_char = None; // Close matching quote
                } else if quote_char.is_none() {
                    quote_char = Some(c); // Open new quote
                }
                // Mismatched quote (e.g. ' inside "...") → ignored
            }
            '(' if quote_char.is_none() => depth += 1,
            ')' if depth > 0 => {
                // Auto-close any unclosed quote at paren boundary.
                // Handles `filter(it's)` where the apostrophe is not a delimiter.
                quote_char = None;
                depth -= 1;
            }
            '|' if depth == 0 => {
                result.push(&input[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    result.push(&input[start..]);
    result
}

impl TransformExpr {
    /// Parse a pipe-separated transform expression.
    ///
    /// Examples: `"sort | unique | first(3)"`, `"upper"`, `""`
    pub fn parse(input: &str) -> Result<Self, TransformParseError> {
        let trimmed = input.trim();
        if trimmed.is_empty() {
            return Ok(TransformExpr {
                ops: SmallVec::new(),
            });
        }

        let ops: SmallVec<[TransformOp; 2]> = split_pipe_respecting_parens(trimmed)
            .iter()
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .map(|s| parse_single_op(s, input))
            .collect::<Result<_, _>>()?;

        Ok(TransformExpr { ops })
    }

    /// Apply all transforms in sequence to a value.
    pub fn apply(&self, value: &Value) -> Result<Value, TransformError> {
        let mut current = value.clone();
        for op in &self.ops {
            current = op.apply(&current)?;
        }
        Ok(current)
    }

    /// Returns true if this expression is empty (no-op).
    pub fn is_empty(&self) -> bool {
        self.ops.is_empty()
    }

    /// Returns true if the chain contains a `default()` transform.
    ///
    /// Used by binding resolution to allow `$env.MISSING | default("x")`
    /// even when the source returns `None` (missing, not null).
    pub fn has_default(&self) -> bool {
        self.ops
            .iter()
            .any(|op| matches!(op, TransformOp::Default(_)))
    }
}


// ═══════════════════════════════════════════════════════════════
// Pipe Parser
// ═══════════════════════════════════════════════════════════════

/// Parse a single transform op from string.
///
/// Examples: `"upper"`, `"first(3)"`, `"join(', ')"`, `"default('N/A')"`, `"round(2)"`
fn parse_single_op(input: &str, full_input: &str) -> Result<TransformOp, TransformParseError> {
    let trimmed = input.trim();

    // Check for parameterized form: name(arg)
    if let Some(paren_pos) = trimmed.find('(') {
        let name = trimmed[..paren_pos].trim();
        let rest = &trimmed[paren_pos + 1..];
        let arg = rest
            .strip_suffix(')')
            .ok_or_else(|| TransformParseError {
                input: full_input.to_string(),
                reason: format!("unclosed parenthesis in '{}'", trimmed),
            })?
            .trim();

        match name {
            "first" => {
                let n: usize = arg.parse().map_err(|_| TransformParseError {
                    input: full_input.to_string(),
                    reason: format!("invalid argument for first(): '{}'", arg),
                })?;
                Ok(TransformOp::FirstN(n))
            }
            "last" => {
                let n: usize = arg.parse().map_err(|_| TransformParseError {
                    input: full_input.to_string(),
                    reason: format!("invalid argument for last(): '{}'", arg),
                })?;
                Ok(TransformOp::LastN(n))
            }
            "round" => {
                let d: u32 = arg.parse().map_err(|_| TransformParseError {
                    input: full_input.to_string(),
                    reason: format!("invalid argument for round(): '{}'", arg),
                })?;
                Ok(TransformOp::Round(Some(d)))
            }
            "join" => {
                let sep = strip_quotes(arg);
                Ok(TransformOp::Join(sep.to_string()))
            }
            "split" => {
                let sep = strip_quotes(arg);
                Ok(TransformOp::Split(sep.to_string()))
            }
            "default" => {
                let val = parse_default_value(arg).map_err(|reason| TransformParseError {
                    input: full_input.to_string(),
                    reason,
                })?;
                Ok(TransformOp::Default(val))
            }
            "slice" => {
                let parts: Vec<&str> = arg.split(',').map(|s| s.trim()).collect();
                if parts.len() != 2 {
                    return Err(TransformParseError {
                        input: full_input.to_string(),
                        reason: format!(
                            "slice() requires 2 arguments (start, end), got {}",
                            parts.len()
                        ),
                    });
                }
                let start: usize = parts[0].parse().map_err(|_| TransformParseError {
                    input: full_input.to_string(),
                    reason: format!("invalid start for slice(): '{}'", parts[0]),
                })?;
                let end: usize = parts[1].parse().map_err(|_| TransformParseError {
                    input: full_input.to_string(),
                    reason: format!("invalid end for slice(): '{}'", parts[1]),
                })?;
                Ok(TransformOp::Slice(start, end))
            }
            "pluck" => {
                let field = strip_quotes(arg);
                Ok(TransformOp::Pluck(field.to_string()))
            }
            "where" => {
                // where("field", "value") — eq (default)
                // where("field", "op", value) — explicit operator
                let parts = split_parametric_args(arg);
                match parts.len() {
                    2 => {
                        let field = strip_quotes(parts[0].trim()).to_string();
                        let val_str = parts[1].trim();
                        let val =
                            parse_default_value(val_str).map_err(|reason| TransformParseError {
                                input: full_input.to_string(),
                                reason,
                            })?;
                        Ok(TransformOp::Where(field, "eq".to_string(), val))
                    }
                    3 => {
                        let field = strip_quotes(parts[0].trim()).to_string();
                        let op = strip_quotes(parts[1].trim()).to_string();
                        let valid_ops = [
                            "eq",
                            "ne",
                            "gt",
                            "lt",
                            "gte",
                            "lte",
                            "contains",
                            "starts_with",
                            "ends_with",
                        ];
                        if !valid_ops.contains(&op.as_str()) {
                            return Err(TransformParseError {
                                input: full_input.to_string(),
                                reason: format!(
                                    "unknown where() operator '{}', expected one of: {}",
                                    op,
                                    valid_ops.join(", ")
                                ),
                            });
                        }
                        let val_str = parts[2].trim();
                        let val =
                            parse_default_value(val_str).map_err(|reason| TransformParseError {
                                input: full_input.to_string(),
                                reason,
                            })?;
                        Ok(TransformOp::Where(field, op, val))
                    }
                    _ => Err(TransformParseError {
                        input: full_input.to_string(),
                        reason: format!(
                            "where() requires 2 or 3 arguments (field, [op], value), got {}",
                            parts.len()
                        ),
                    }),
                }
            }
            "pick" => {
                let fields: Vec<String> = split_parametric_args(arg)
                    .iter()
                    .map(|s| strip_quotes(s.trim()).to_string())
                    .collect();
                if fields.is_empty() {
                    return Err(TransformParseError {
                        input: full_input.to_string(),
                        reason: "pick() requires at least 1 field".to_string(),
                    });
                }
                Ok(TransformOp::Pick(fields))
            }
            "omit" => {
                let fields: Vec<String> = split_parametric_args(arg)
                    .iter()
                    .map(|s| strip_quotes(s.trim()).to_string())
                    .collect();
                if fields.is_empty() {
                    return Err(TransformParseError {
                        input: full_input.to_string(),
                        reason: "omit() requires at least 1 field".to_string(),
                    });
                }
                Ok(TransformOp::Omit(fields))
            }
            "sort_by" => {
                let field = strip_quotes(arg);
                Ok(TransformOp::SortBy(field.to_string()))
            }
            "group_by" => {
                let field = strip_quotes(arg);
                Ok(TransformOp::GroupBy(field.to_string()))
            }
            "merge" => {
                let val = parse_default_value(arg).map_err(|reason| TransformParseError {
                    input: full_input.to_string(),
                    reason,
                })?;
                Ok(TransformOp::Merge(Some(val)))
            }
            "regex" => {
                let pattern = strip_quotes(arg);
                Ok(TransformOp::Regex(pattern.to_string()))
            }
            "starts_with" => {
                let prefix = strip_quotes(arg);
                Ok(TransformOp::StartsWith(prefix.to_string()))
            }
            "ends_with" => {
                let suffix = strip_quotes(arg);
                Ok(TransformOp::EndsWith(suffix.to_string()))
            }
            "contains" => {
                let text = strip_quotes(arg);
                Ok(TransformOp::Contains(text.to_string()))
            }
            "min_by" => {
                let field = strip_quotes(arg);
                Ok(TransformOp::MinBy(field.to_string()))
            }
            "max_by" => {
                let field = strip_quotes(arg);
                Ok(TransformOp::MaxBy(field.to_string()))
            }
            "has" => {
                let key = strip_quotes(arg);
                Ok(TransformOp::Has(key.to_string()))
            }
            "replace" => {
                let parts = split_parametric_args(arg);
                if parts.len() != 2 {
                    return Err(TransformParseError {
                        input: full_input.to_string(),
                        reason: format!(
                            "replace() requires 2 arguments (from, to), got {}",
                            parts.len()
                        ),
                    });
                }
                let from = strip_quotes(parts[0].trim()).to_string();
                let to = strip_quotes(parts[1].trim()).to_string();
                Ok(TransformOp::Replace(from, to))
            }
            "truncate" => {
                let n: usize = arg.parse().map_err(|_| TransformParseError {
                    input: full_input.to_string(),
                    reason: format!("invalid argument for truncate(): '{}'", arg),
                })?;
                Ok(TransformOp::Truncate(n))
            }
            "jq" => {
                let expr = strip_quotes(arg);
                Ok(TransformOp::Jq(expr.to_string()))
            }
            _ => {
                let hint = crate::ast::analyzer::suggestions::find_similar(
                    name,
                    KNOWN_TRANSFORM_NAMES,
                    0.7,
                );
                let reason = match hint {
                    Some(ref s) => format!("unknown transform: '{}'. Did you mean '{}'?", name, s),
                    None => format!("unknown transform: '{}'", name),
                };
                Err(TransformParseError {
                    input: full_input.to_string(),
                    reason,
                })
            }
        }
    } else {
        // Simple name (no args)
        match trimmed {
            "upper" => Ok(TransformOp::Upper),
            "lower" => Ok(TransformOp::Lower),
            "trim" => Ok(TransformOp::Trim),
            "trim_start" => Ok(TransformOp::TrimStart),
            "trim_end" => Ok(TransformOp::TrimEnd),
            "length" => Ok(TransformOp::Length),
            "first" => Ok(TransformOp::First),
            "last" => Ok(TransformOp::Last),
            "keys" => Ok(TransformOp::Keys),
            "values" => Ok(TransformOp::Values),
            "flatten" => Ok(TransformOp::Flatten),
            "reverse" => Ok(TransformOp::Reverse),
            "sort" => Ok(TransformOp::Sort),
            "unique" => Ok(TransformOp::Unique),
            "compact" => Ok(TransformOp::Compact),
            "to_string" => Ok(TransformOp::ToString),
            "to_number" => Ok(TransformOp::ToNumber),
            "to_bool" => Ok(TransformOp::ToBool),
            "to_json" => Ok(TransformOp::ToJson),
            "parse_json" => Ok(TransformOp::ParseJson),
            "parse_yaml" => Ok(TransformOp::ParseYaml),
            "round" => Ok(TransformOp::Round(None)),
            "abs" => Ok(TransformOp::Abs),
            "ceil" => Ok(TransformOp::Ceil),
            "floor" => Ok(TransformOp::Floor),
            "type_of" => Ok(TransformOp::TypeOf),
            "shell" => Ok(TransformOp::Shell),
            "url_host" => Ok(TransformOp::UrlHost),
            "url_path" => Ok(TransformOp::UrlPath),
            "url_without_query" => Ok(TransformOp::UrlWithoutQuery),
            "url_normalize" => Ok(TransformOp::UrlNormalize),
            "merge" => Ok(TransformOp::Merge(None)),
            "base64_encode" => Ok(TransformOp::Base64Encode),
            "base64_decode" => Ok(TransformOp::Base64Decode),
            "content_hash" => Ok(TransformOp::ContentHash),
            "unique_urls" => Ok(TransformOp::UniqueUrls),
            "add" => Ok(TransformOp::Add),
            "min" => Ok(TransformOp::Min),
            "max" => Ok(TransformOp::Max),
            "sum" => Ok(TransformOp::Sum),
            "avg" => Ok(TransformOp::Avg),
            "not" => Ok(TransformOp::Not),
            "html_escape" => Ok(TransformOp::HtmlEscape),
            "md_escape" => Ok(TransformOp::MdEscape),
            "sanitize" => Ok(TransformOp::Sanitize),
            _ => {
                let hint = crate::ast::analyzer::suggestions::find_similar(
                    trimmed,
                    KNOWN_TRANSFORM_NAMES,
                    0.7,
                );
                let reason = match hint {
                    Some(ref s) => {
                        format!("unknown transform: '{}'. Did you mean '{}'?", trimmed, s)
                    }
                    None => format!("unknown transform: '{}'", trimmed),
                };
                Err(TransformParseError {
                    input: full_input.to_string(),
                    reason,
                })
            }
        }
    }
}

