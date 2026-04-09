// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! TransformOp::apply — the core transform execution engine.

use serde_json::Value;
use std::num::NonZeroUsize;
use std::sync::{Arc, LazyLock, Mutex};

use super::dispatch_macro::{strict_arr, strict_num, strict_obj, strict_str};
use super::helpers::{
    f64_to_json_number, shell_escape, strip_bom_and_control_chars, strip_markdown_code_block,
    truncate, type_mismatch, value_type_name,
};
use super::{TransformError, TransformOp};

// ═══════════════════════════════════════════════════════════════
// TransformOp::apply
// ═══════════════════════════════════════════════════════════════

impl TransformOp {
    /// Apply this single transform to a JSON value.
    ///
    /// # Null handling
    ///
    /// Two strategies exist:
    /// - **Propagating**: `length`, `keys`, `values`, `to_string`, `to_json`,
    ///   `type_of` return `Value::Null` on null input (marked `// propagating`).
    /// - **Strict**: `upper`, `trim`, `first`, `last`, etc. return `NIKA-153`
    ///   error on null input. Users should chain `| default("fallback")` before
    ///   strict transforms if null is possible.
    pub fn apply(&self, value: &Value) -> Result<Value, TransformError> {
        match self {
            // ── String ───────────────────────────────────────
            TransformOp::Upper => strict_str("upper", value, |s| Ok(Value::String(s.to_uppercase()))),
            TransformOp::Lower => strict_str("lower", value, |s| Ok(Value::String(s.to_lowercase()))),
            TransformOp::Trim => strict_str("trim", value, |s| Ok(Value::String(s.trim().to_string()))),
            TransformOp::TrimStart => strict_str("trim_start", value, |s| Ok(Value::String(s.trim_start().to_string()))),
            TransformOp::TrimEnd => strict_str("trim_end", value, |s| Ok(Value::String(s.trim_end().to_string()))),

            // ── Collection ───────────────────────────────────
            TransformOp::Length => match value {
                Value::Null => Ok(Value::Null), // propagating
                Value::Array(arr) => Ok(Value::Number(arr.len().into())),
                Value::String(s) => Ok(Value::Number(s.chars().count().into())),
                Value::Object(obj) => Ok(Value::Number(obj.len().into())),
                _ => Err(type_mismatch("length", "array, string, or object", value)),
            },
            TransformOp::First => strict_arr("first", value, |arr| Ok(arr.first().cloned().unwrap_or(Value::Null))),
            TransformOp::Last => strict_arr("last", value, |arr| Ok(arr.last().cloned().unwrap_or(Value::Null))),
            TransformOp::FirstN(n) => match value {
                Value::Null => Err(TransformError::NullInput { op: "first" }),
                Value::Array(arr) => {
                    let taken: Vec<Value> = arr.iter().take(*n).cloned().collect();
                    Ok(Value::Array(taken))
                }
                Value::String(s) => {
                    // Truncate string to N characters
                    let truncated: String = s.chars().take(*n).collect();
                    Ok(Value::String(truncated))
                }
                Value::Object(_) => {
                    // Serialize object to JSON string and truncate to N characters
                    // serde_json::Value is always serializable — unwrap is safe
                    let json = serde_json::to_string(value).expect("Value is serializable");
                    let truncated: String = json.chars().take(*n).collect();
                    Ok(Value::String(truncated))
                }
                _ => Err(type_mismatch("first", "array, string, or object", value)),
            },
            TransformOp::LastN(n) => match value {
                Value::Null => Err(TransformError::NullInput { op: "last" }),
                Value::Array(arr) => {
                    let skip = arr.len().saturating_sub(*n);
                    let taken: Vec<Value> = arr.iter().skip(skip).cloned().collect();
                    Ok(Value::Array(taken))
                }
                Value::String(s) => {
                    // Last N characters (Unicode-safe)
                    let chars: Vec<char> = s.chars().collect();
                    let skip = chars.len().saturating_sub(*n);
                    let truncated: String = chars[skip..].iter().collect();
                    Ok(Value::String(truncated))
                }
                Value::Object(_) => {
                    let json = serde_json::to_string(value).expect("Value is serializable");
                    let chars: Vec<char> = json.chars().collect();
                    let skip = chars.len().saturating_sub(*n);
                    let truncated: String = chars[skip..].iter().collect();
                    Ok(Value::String(truncated))
                }
                _ => Err(type_mismatch("last", "array, string, or object", value)),
            },
            TransformOp::Keys => match value {
                Value::Null => Ok(Value::Null), // propagating
                Value::Object(obj) => {
                    let keys: Vec<Value> = obj.keys().map(|k| Value::String(k.clone())).collect();
                    Ok(Value::Array(keys))
                }
                _ => Err(type_mismatch("keys", "object", value)),
            },
            TransformOp::Values => match value {
                Value::Null => Ok(Value::Null), // propagating (symmetric with keys)
                Value::Object(obj) => {
                    let vals: Vec<Value> = obj.values().cloned().collect();
                    Ok(Value::Array(vals))
                }
                _ => Err(type_mismatch("values", "object", value)),
            },
            TransformOp::Flatten => strict_arr("flatten", value, |arr| {
                let mut flat = Vec::new();
                for item in arr {
                    match item {
                        Value::Array(inner) => flat.extend(inner.iter().cloned()),
                        other => flat.push(other.clone()),
                    }
                }
                Ok(Value::Array(flat))
            }),
            TransformOp::Reverse => strict_arr("reverse", value, |arr| {
                let mut rev = arr.to_vec();
                rev.reverse();
                Ok(Value::Array(rev))
            }),
            TransformOp::Sort => strict_arr("sort", value, |arr| {
                let mut sorted = arr.to_vec();
                sorted.sort_by(|a, b| match (a.as_f64(), b.as_f64()) {
                    (Some(x), Some(y)) => {
                        x.partial_cmp(&y).unwrap_or(std::cmp::Ordering::Equal)
                    }
                    (Some(_), None) => std::cmp::Ordering::Less,
                    (None, Some(_)) => std::cmp::Ordering::Greater,
                    _ => a.to_string().cmp(&b.to_string()),
                });
                Ok(Value::Array(sorted))
            }),
            TransformOp::Unique => strict_arr("unique", value, |arr| {
                let mut seen = Vec::new();
                let mut unique = Vec::new();
                for item in arr {
                    let s = item.to_string();
                    if !seen.contains(&s) {
                        seen.push(s);
                        unique.push(item.clone());
                    }
                }
                Ok(Value::Array(unique))
            }),
            TransformOp::Compact => strict_arr("compact", value, |arr| {
                let compacted: Vec<Value> = arr
                    .iter()
                    .filter(|v| !v.is_null() && !matches!(v, Value::String(s) if s.is_empty()))
                    .cloned()
                    .collect();
                Ok(Value::Array(compacted))
            }),

            // ── Type conversion ──────────────────────────────
            TransformOp::ToString => match value {
                Value::Null => Ok(Value::Null), // propagating
                Value::String(_) => Ok(value.clone()),
                Value::Number(n) => Ok(Value::String(n.to_string())),
                Value::Bool(b) => Ok(Value::String(b.to_string())),
                _ => Ok(Value::String(value.to_string())),
            },
            TransformOp::ToNumber => match value {
                Value::Null => Err(TransformError::NullInput { op: "to_number" }),
                Value::Number(_) => Ok(value.clone()),
                Value::String(s) => {
                    if let Ok(n) = s.parse::<i64>() {
                        Ok(Value::Number(n.into()))
                    } else if let Ok(f) = s.parse::<f64>() {
                        Ok(serde_json::Number::from_f64(f)
                            .map(Value::Number)
                            .unwrap_or(Value::Null))
                    } else {
                        Err(TransformError::TypeMismatch {
                            op: "to_number",
                            expected: "numeric string",
                            got: format!("\"{}\"", s),
                        })
                    }
                }
                Value::Bool(b) => Ok(Value::Number(if *b { 1 } else { 0 }.into())),
                _ => Err(type_mismatch("to_number", "string, number, or bool", value)),
            },
            TransformOp::ToBool => match value {
                Value::Null => Err(TransformError::NullInput { op: "to_bool" }),
                Value::Bool(_) => Ok(value.clone()),
                Value::Number(n) => Ok(Value::Bool(n.as_f64().map(|f| f != 0.0).unwrap_or(false))),
                Value::String(s) => match s.as_str() {
                    "true" | "1" | "yes" => Ok(Value::Bool(true)),
                    "false" | "0" | "no" | "" => Ok(Value::Bool(false)),
                    _ => Err(TransformError::TypeMismatch {
                        op: "to_bool",
                        expected: "truthy/falsy value",
                        got: format!("\"{}\"", s),
                    }),
                },
                _ => Err(type_mismatch("to_bool", "string, number, or bool", value)),
            },
            TransformOp::ToJson => match value {
                Value::Null => Ok(Value::Null), // propagating
                _ => Ok(Value::String(
                    // serde_json::Value is always serializable — unwrap is safe
                    serde_json::to_string(value).expect("Value is serializable"),
                )),
            },
            TransformOp::ParseJson => match value {
                Value::Null => Err(TransformError::NullInput { op: "parse_json" }),
                Value::String(s) => {
                    // Strip markdown code blocks: ```json\n...\n``` or ```\n...\n```
                    let cleaned = strip_markdown_code_block(s);
                    // Strip UTF-8 BOM and NUL bytes that exec output may contain
                    let cleaned = strip_bom_and_control_chars(&cleaned);
                    serde_json::from_str(&cleaned).map_err(|e| TransformError::TypeMismatch {
                        op: "parse_json",
                        expected: "valid JSON string",
                        got: format!("{} (input: \"{}\")", e, truncate(s, 80)),
                    })
                }
                // Idempotent: already-parsed values pass through unchanged.
                // This handles auto-parsed exec outputs where Nika converts
                // JSON strings to values before transforms run.
                Value::Array(_) | Value::Object(_) | Value::Number(_) | Value::Bool(_) => {
                    Ok(value.clone())
                }
            },

            TransformOp::ParseYaml => match value {
                Value::Null => Err(TransformError::NullInput { op: "parse_yaml" }),
                Value::String(s) => {
                    // Strip markdown code blocks: ```yaml\n...\n``` or ```\n...\n```
                    let cleaned = strip_markdown_code_block(s);
                    let cleaned = strip_bom_and_control_chars(&cleaned);
                    crate::serde_yaml::from_str::<Value>(&cleaned).map_err(|e| {
                        TransformError::TypeMismatch {
                            op: "parse_yaml",
                            expected: "valid YAML string",
                            got: format!("{} (input: \"{}\")", e, truncate(s, 80)),
                        }
                    })
                }
                // Idempotent: already-parsed values pass through unchanged.
                Value::Array(_) | Value::Object(_) | Value::Number(_) | Value::Bool(_) => {
                    Ok(value.clone())
                }
            },

            // ── Numeric ──────────────────────────────────────
            TransformOp::Round(decimals) => match value {
                Value::Null => Err(TransformError::NullInput { op: "round" }),
                Value::Number(n) => {
                    let f = n.as_f64().unwrap_or(0.0);
                    if f.is_nan() || f.is_infinite() {
                        return Ok(Value::Null);
                    }
                    let d = decimals.unwrap_or(0);
                    if d == 0 {
                        // No decimals → return integer (consistent with ceil/floor)
                        Ok(Value::Number((f.round() as i64).into()))
                    } else {
                        let factor = 10f64.powi(d as i32);
                        let rounded = (f * factor).round() / factor;
                        Ok(serde_json::Number::from_f64(rounded)
                            .map(Value::Number)
                            .unwrap_or(Value::Null))
                    }
                }
                _ => Err(type_mismatch("round", "number", value)),
            },
            TransformOp::Abs => match value {
                Value::Null => Err(TransformError::NullInput { op: "abs" }),
                Value::Number(n) => {
                    if let Some(i) = n.as_i64() {
                        Ok(Value::Number(i.unsigned_abs().into()))
                    } else if let Some(f) = n.as_f64() {
                        if f.is_nan() || f.is_infinite() {
                            return Ok(Value::Null);
                        }
                        Ok(serde_json::Number::from_f64(f.abs())
                            .map(Value::Number)
                            .unwrap_or(Value::Null))
                    } else {
                        Ok(value.clone())
                    }
                }
                _ => Err(type_mismatch("abs", "number", value)),
            },
            TransformOp::Ceil => strict_num("ceil", value, |n| {
                let f = n.as_f64().unwrap_or(0.0);
                if f.is_nan() || f.is_infinite() {
                    return Ok(Value::Null);
                }
                Ok(Value::Number((f.ceil() as i64).into()))
            }),
            TransformOp::Floor => strict_num("floor", value, |n| {
                let f = n.as_f64().unwrap_or(0.0);
                if f.is_nan() || f.is_infinite() {
                    return Ok(Value::Null);
                }
                Ok(Value::Number((f.floor() as i64).into()))
            }),

            // ── Utility ──────────────────────────────────────
            TransformOp::Default(default_val) => match value {
                Value::Null => Ok(default_val.clone()),
                Value::String(s) if s.is_empty() => Ok(default_val.clone()),
                _ => Ok(value.clone()),
            },
            TransformOp::TypeOf => {
                let name = value_type_name(value);
                Ok(Value::String(name.to_string()))
            }
            TransformOp::Join(sep) => match value {
                Value::Null => Err(TransformError::NullInput { op: "join" }),
                Value::Array(arr) => {
                    let strings: Vec<String> = arr
                        .iter()
                        .map(|v| match v {
                            Value::String(s) => s.clone(),
                            other => other.to_string(),
                        })
                        .collect();
                    Ok(Value::String(strings.join(sep)))
                }
                _ => Err(type_mismatch("join", "array", value)),
            },
            TransformOp::Split(sep) => match value {
                Value::Null => Err(TransformError::NullInput { op: "split" }),
                Value::String(s) => {
                    let parts: Vec<Value> = s
                        .split(sep.as_str())
                        .map(|p| Value::String(p.to_string()))
                        .collect();
                    Ok(Value::Array(parts))
                }
                _ => Err(type_mismatch("split", "string", value)),
            },
            TransformOp::Shell => {
                // Shell escaping — all types get escaped, not just strings.
                // Null input is an error (not the string 'null').
                match value {
                    Value::Null => Err(TransformError::NullInput { op: "shell" }),
                    Value::String(s) => Ok(Value::String(shell_escape(s))),
                    _ => Ok(Value::String(shell_escape(&value.to_string()))),
                }
            }

            // ── URL ──────────────────────────────────────────
            TransformOp::UrlHost => match value {
                Value::Null => Err(TransformError::NullInput { op: "url_host" }),
                Value::String(s) => {
                    let parsed = url::Url::parse(s).map_err(|_| TransformError::TypeMismatch {
                        op: "url_host",
                        expected: "valid URL",
                        got: "invalid URL".to_string(),
                    })?;
                    let host = parsed.host_str().unwrap_or_default();
                    // Strip IPv6 brackets: [::1] → ::1
                    let host = host
                        .strip_prefix('[')
                        .and_then(|h| h.strip_suffix(']'))
                        .unwrap_or(host);
                    Ok(Value::String(host.to_string()))
                }
                _ => Err(type_mismatch("url_host", "string", value)),
            },
            TransformOp::UrlPath => match value {
                Value::Null => Err(TransformError::NullInput { op: "url_path" }),
                Value::String(s) => {
                    let parsed = url::Url::parse(s).map_err(|_| TransformError::TypeMismatch {
                        op: "url_path",
                        expected: "valid URL",
                        got: "invalid URL".to_string(),
                    })?;
                    Ok(Value::String(parsed.path().to_string()))
                }
                _ => Err(type_mismatch("url_path", "string", value)),
            },
            TransformOp::UrlWithoutQuery => match value {
                Value::Null => Err(TransformError::NullInput {
                    op: "url_without_query",
                }),
                Value::String(s) => {
                    let mut parsed =
                        url::Url::parse(s).map_err(|_| TransformError::TypeMismatch {
                            op: "url_without_query",
                            expected: "valid URL",
                            got: "invalid URL".to_string(),
                        })?;
                    parsed.set_query(None);
                    parsed.set_fragment(None);
                    Ok(Value::String(parsed.to_string()))
                }
                _ => Err(type_mismatch("url_without_query", "string", value)),
            },
            TransformOp::UrlNormalize => match value {
                Value::Null => Err(TransformError::NullInput {
                    op: "url_normalize",
                }),
                Value::String(s) => {
                    let mut parsed =
                        url::Url::parse(s).map_err(|_| TransformError::TypeMismatch {
                            op: "url_normalize",
                            expected: "valid URL",
                            got: "invalid URL".to_string(),
                        })?;

                    // 1. Remove default ports (:80 for http, :443 for https)
                    if (parsed.scheme() == "http" && parsed.port() == Some(80))
                        || (parsed.scheme() == "https" && parsed.port() == Some(443))
                    {
                        let _ = parsed.set_port(None);
                    }

                    // 2. Strip tracking parameters, sort remaining
                    let filtered: Vec<(String, String)> = parsed
                        .query_pairs()
                        .filter(|(key, _)| !is_tracking_param(key))
                        .map(|(k, v)| (k.to_string(), v.to_string()))
                        .collect();

                    if filtered.is_empty() {
                        parsed.set_query(None);
                    } else {
                        let mut sorted = filtered;
                        sorted.sort_by(|a, b| a.0.cmp(&b.0));
                        let query = sorted
                            .iter()
                            .map(|(k, v)| {
                                if v.is_empty() {
                                    k.clone()
                                } else {
                                    format!("{}={}", k, v)
                                }
                            })
                            .collect::<Vec<_>>()
                            .join("&");
                        parsed.set_query(Some(&query));
                    }

                    // 3. Strip fragment
                    parsed.set_fragment(None);

                    // 4. Remove trailing slash on non-root paths
                    let path = parsed.path().to_string();
                    if path.len() > 1 && path.ends_with('/') {
                        parsed.set_path(&path[..path.len() - 1]);
                    }

                    Ok(Value::String(parsed.to_string()))
                }
                _ => Err(type_mismatch("url_normalize", "string", value)),
            },

            // ── Slicing ─────────────────────────────────────
            TransformOp::Slice(start, end) => match value {
                Value::Null => Err(TransformError::NullInput { op: "slice" }),
                Value::Array(arr) => {
                    let len = arr.len();
                    let s = (*start).min(len);
                    let e = (*end).min(len);
                    Ok(Value::Array(arr[s..e].to_vec()))
                }
                Value::String(s) => {
                    let chars: Vec<char> = s.chars().collect();
                    let len = chars.len();
                    let si = (*start).min(len);
                    let ei = (*end).min(len);
                    Ok(Value::String(chars[si..ei].iter().collect()))
                }
                _ => Err(type_mismatch("slice", "array or string", value)),
            },

            // ── Data (array/object manipulation) ────────────
            TransformOp::Pluck(field) => strict_arr("pluck", value, |arr| {
                let result: Vec<Value> = arr
                    .iter()
                    .filter_map(|item| navigate_dot_path(item, field).cloned())
                    .collect();
                Ok(Value::Array(result))
            }),
            TransformOp::Where(field, op, expected) => match value {
                Value::Null => Err(TransformError::NullInput { op: "where" }),
                Value::Array(arr) => {
                    let result: Vec<Value> = arr
                        .iter()
                        .filter(|item| {
                            let val = navigate_dot_path(item, field);
                            match op.as_str() {
                                "eq" => val == Some(expected),
                                "ne" => val != Some(expected),
                                "gt" => val
                                    .and_then(|v| v.as_f64())
                                    .zip(expected.as_f64())
                                    .is_some_and(|(a, b)| a > b),
                                "lt" => val
                                    .and_then(|v| v.as_f64())
                                    .zip(expected.as_f64())
                                    .is_some_and(|(a, b)| a < b),
                                "gte" => val
                                    .and_then(|v| v.as_f64())
                                    .zip(expected.as_f64())
                                    .is_some_and(|(a, b)| a >= b),
                                "lte" => val
                                    .and_then(|v| v.as_f64())
                                    .zip(expected.as_f64())
                                    .is_some_and(|(a, b)| a <= b),
                                "contains" => val
                                    .and_then(|v| v.as_str())
                                    .zip(expected.as_str())
                                    .is_some_and(|(a, b)| a.contains(b)),
                                "starts_with" => val
                                    .and_then(|v| v.as_str())
                                    .zip(expected.as_str())
                                    .is_some_and(|(a, b)| a.starts_with(b)),
                                "ends_with" => val
                                    .and_then(|v| v.as_str())
                                    .zip(expected.as_str())
                                    .is_some_and(|(a, b)| a.ends_with(b)),
                                _ => false,
                            }
                        })
                        .cloned()
                        .collect();
                    Ok(Value::Array(result))
                }
                _ => Err(type_mismatch("where", "array", value)),
            },
            TransformOp::Pick(fields) => match value {
                Value::Null => Err(TransformError::NullInput { op: "pick" }),
                Value::Object(obj) => {
                    let mut result = serde_json::Map::new();
                    for field in fields {
                        if let Some(v) = obj.get(field) {
                            result.insert(field.clone(), v.clone());
                        }
                    }
                    Ok(Value::Object(result))
                }
                _ => Err(type_mismatch("pick", "object", value)),
            },
            TransformOp::Omit(fields) => strict_obj("omit", value, |obj| {
                let mut result = obj.clone();
                for field in fields {
                    result.remove(field);
                }
                Ok(Value::Object(result))
            }),
            TransformOp::SortBy(field) => match value {
                Value::Null => Err(TransformError::NullInput { op: "sort_by" }),
                Value::Array(arr) => {
                    let mut sorted = arr.clone();
                    sorted.sort_by(|a, b| {
                        let va = navigate_dot_path(a, field);
                        let vb = navigate_dot_path(b, field);
                        match (va.and_then(|v| v.as_f64()), vb.and_then(|v| v.as_f64())) {
                            (Some(x), Some(y)) => {
                                x.partial_cmp(&y).unwrap_or(std::cmp::Ordering::Equal)
                            }
                            (Some(_), None) => std::cmp::Ordering::Less,
                            (None, Some(_)) => std::cmp::Ordering::Greater,
                            _ => {
                                let sa = va.map(|v| v.to_string()).unwrap_or_default();
                                let sb = vb.map(|v| v.to_string()).unwrap_or_default();
                                sa.cmp(&sb)
                            }
                        }
                    });
                    Ok(Value::Array(sorted))
                }
                _ => Err(type_mismatch("sort_by", "array", value)),
            },
            TransformOp::GroupBy(field) => match value {
                Value::Null => Err(TransformError::NullInput { op: "group_by" }),
                Value::Array(arr) => {
                    let mut groups: indexmap::IndexMap<String, Vec<Value>> =
                        indexmap::IndexMap::new();
                    for item in arr {
                        let key = match navigate_dot_path(item, field) {
                            Some(Value::String(s)) => s.clone(),
                            Some(v) => v.to_string(),
                            None => "null".to_string(),
                        };
                        groups.entry(key).or_default().push(item.clone());
                    }
                    let result: serde_json::Map<String, Value> = groups
                        .into_iter()
                        .map(|(k, v)| (k, Value::Array(v)))
                        .collect();
                    Ok(Value::Object(result))
                }
                _ => Err(type_mismatch("group_by", "array", value)),
            },
            TransformOp::Merge(None) => match value {
                // No-arg form: merge array of objects left-to-right
                Value::Null => Err(TransformError::NullInput { op: "merge" }),
                Value::Array(arr) => {
                    let mut base = serde_json::Map::new();
                    for item in arr {
                        if let Value::Object(obj) = item {
                            deep_merge(&mut base, obj);
                        } else {
                            return Err(TransformError::TypeMismatch {
                                op: "merge",
                                expected: "array of objects",
                                got: format!("array containing {}", value_type_name(item)),
                            });
                        }
                    }
                    Ok(Value::Object(base))
                }
                _ => Err(type_mismatch("merge", "array or object", value)),
            },
            TransformOp::Merge(Some(overlay)) => match value {
                // Parametric form: deep merge overlay onto input object
                Value::Null => Err(TransformError::NullInput { op: "merge" }),
                Value::Object(base_map) => {
                    if let Value::Object(overlay_map) = overlay {
                        let mut result = base_map.clone();
                        deep_merge(&mut result, overlay_map);
                        Ok(Value::Object(result))
                    } else {
                        Err(TransformError::TypeMismatch {
                            op: "merge",
                            expected: "object as merge argument",
                            got: value_type_name(overlay).to_string(),
                        })
                    }
                }
                _ => Err(type_mismatch("merge", "object", value)),
            },
            TransformOp::Regex(pattern) => match value {
                Value::Null => Err(TransformError::NullInput { op: "regex" }),
                Value::String(s) => {
                    let re = cached_regex(pattern).map_err(|e| TransformError::TypeMismatch {
                        op: "regex",
                        expected: "valid regex pattern",
                        got: format!("invalid regex: {}", e),
                    })?;
                    match re.find(s) {
                        Some(m) => Ok(Value::String(m.as_str().to_string())),
                        None => Ok(Value::Null),
                    }
                }
                _ => Err(type_mismatch("regex", "string", value)),
            },

            // ── Encoding ────────────────────────────────────
            TransformOp::Base64Encode => strict_str("base64_encode", value, |s| {
                use base64::Engine;
                Ok(Value::String(
                    base64::engine::general_purpose::STANDARD.encode(s.as_bytes()),
                ))
            }),
            // Note: base64_decode returns UTF-8 text only. For binary data
            // (images, audio), use `nika:import` with CAS instead.
            TransformOp::Base64Decode => match value {
                Value::Null => Err(TransformError::NullInput {
                    op: "base64_decode",
                }),
                Value::String(s) => {
                    use base64::Engine;
                    let bytes = base64::engine::general_purpose::STANDARD
                        .decode(s.as_bytes())
                        .map_err(|e| TransformError::TypeMismatch {
                            op: "base64_decode",
                            expected: "valid base64 string",
                            got: format!("decode error: {}", e),
                        })?;
                    let decoded =
                        String::from_utf8(bytes).map_err(|e| TransformError::TypeMismatch {
                            op: "base64_decode",
                            expected: "UTF-8 text (binary data not supported — use nika:import)",
                            got: format!("not valid UTF-8: {}", e),
                        })?;
                    Ok(Value::String(decoded))
                }
                _ => Err(type_mismatch("base64_decode", "string", value)),
            },

            // ── Predicate (returns bool) ────────────────────────
            TransformOp::StartsWith(prefix) => strict_str("starts_with", value, |s| Ok(Value::Bool(s.starts_with(prefix.as_str())))),
            TransformOp::EndsWith(suffix) => strict_str("ends_with", value, |s| Ok(Value::Bool(s.ends_with(suffix.as_str())))),
            TransformOp::Contains(text) => strict_str("contains", value, |s| Ok(Value::Bool(s.contains(text.as_str())))),

            // ── Hashing ─────────────────────────────────────────
            TransformOp::ContentHash => match value {
                Value::Null => Err(TransformError::NullInput { op: "content_hash" }),
                Value::String(s) => {
                    let hash = xxhash_rust::xxh3::xxh3_64(s.as_bytes());
                    Ok(Value::String(format!("{:016x}", hash)))
                }
                _ => {
                    let json = serde_json::to_string(value).expect("Value is serializable");
                    let hash = xxhash_rust::xxh3::xxh3_64(json.as_bytes());
                    Ok(Value::String(format!("{:016x}", hash)))
                }
            },

            // ── URL dedup ───────────────────────────────────────
            TransformOp::UniqueUrls => match value {
                Value::Null => Err(TransformError::NullInput { op: "unique_urls" }),
                Value::Array(arr) => {
                    let mut seen = std::collections::HashSet::new();
                    let unique: Vec<Value> = arr
                        .iter()
                        .filter(|v| {
                            let key = match TransformOp::UrlNormalize.apply(v) {
                                Ok(Value::String(normalized)) => normalized,
                                _ => v.to_string(),
                            };
                            seen.insert(key)
                        })
                        .cloned()
                        .collect();
                    Ok(Value::Array(unique))
                }
                _ => Err(type_mismatch("unique_urls", "array", value)),
            },

            // ── String manipulation ────────────────────────────────
            TransformOp::Replace(from, to) => strict_str("replace", value, |s| Ok(Value::String(s.replace(from.as_str(), to.as_str())))),
            TransformOp::Truncate(n) => strict_str("truncate", value, |s| {
                let truncated: String = s.chars().take(*n).collect();
                Ok(Value::String(truncated))
            }),

            // ── Aggregation ───────────────────────────────────────────
            TransformOp::Add => match value {
                Value::Null => Ok(Value::Null), // propagating
                Value::Array(arr) if arr.is_empty() => Ok(Value::Null),
                Value::Array(arr) => {
                    // Detect type from first non-null element
                    let first_non_null = arr.iter().find(|v| !v.is_null());
                    match first_non_null {
                        Some(Value::Number(_)) | None => {
                            // Sum numbers (null → 0)
                            let mut sum = 0.0_f64;
                            for item in arr {
                                match item {
                                    Value::Number(n) => {
                                        sum += n.as_f64().unwrap_or(0.0);
                                    }
                                    Value::Null => {} // skip nulls
                                    _ => {
                                        return Err(type_mismatch("add", "array of numbers", item))
                                    }
                                }
                            }
                            Ok(f64_to_json_number(sum))
                        }
                        Some(Value::String(_)) => {
                            // Concat strings
                            let mut result = String::new();
                            for item in arr {
                                match item {
                                    Value::String(s) => result.push_str(s),
                                    Value::Null => {} // skip nulls
                                    _ => {
                                        return Err(type_mismatch("add", "array of strings", item))
                                    }
                                }
                            }
                            Ok(Value::String(result))
                        }
                        Some(Value::Array(_)) => {
                            // Concat arrays
                            let mut result = Vec::new();
                            for item in arr {
                                match item {
                                    Value::Array(inner) => result.extend(inner.iter().cloned()),
                                    Value::Null => {} // skip nulls
                                    _ => return Err(type_mismatch("add", "array of arrays", item)),
                                }
                            }
                            Ok(Value::Array(result))
                        }
                        _ => Err(type_mismatch(
                            "add",
                            "array of numbers, strings, or arrays",
                            value,
                        )),
                    }
                }
                _ => Err(type_mismatch("add", "array", value)),
            },
            TransformOp::Min => match value {
                Value::Null => Ok(Value::Null), // propagating
                Value::Array(arr) if arr.is_empty() => Ok(Value::Null),
                Value::Array(arr) => {
                    let mut min_val: Option<f64> = None;
                    for item in arr {
                        match item {
                            Value::Number(n) => {
                                if let Some(v) = n.as_f64() {
                                    min_val = Some(match min_val {
                                        Some(current) => current.min(v),
                                        None => v,
                                    });
                                }
                                // skip numbers that can't be represented as f64
                            }
                            Value::Null => {} // skip nulls
                            _ => return Err(type_mismatch("min", "array of numbers", item)),
                        }
                    }
                    match min_val {
                        Some(v) => Ok(f64_to_json_number(v)),
                        None => Ok(Value::Null), // all nulls
                    }
                }
                _ => Err(type_mismatch("min", "array", value)),
            },
            TransformOp::Max => match value {
                Value::Null => Ok(Value::Null), // propagating
                Value::Array(arr) if arr.is_empty() => Ok(Value::Null),
                Value::Array(arr) => {
                    let mut max_val: Option<f64> = None;
                    for item in arr {
                        match item {
                            Value::Number(n) => {
                                if let Some(v) = n.as_f64() {
                                    max_val = Some(match max_val {
                                        Some(current) => current.max(v),
                                        None => v,
                                    });
                                }
                            }
                            Value::Null => {} // skip nulls
                            _ => return Err(type_mismatch("max", "array of numbers", item)),
                        }
                    }
                    match max_val {
                        Some(v) => Ok(f64_to_json_number(v)),
                        None => Ok(Value::Null), // all nulls
                    }
                }
                _ => Err(type_mismatch("max", "array", value)),
            },

            TransformOp::MinBy(field) => match value {
                Value::Null => Ok(Value::Null),
                Value::Array(arr) if arr.is_empty() => Ok(Value::Null),
                Value::Array(arr) => {
                    let mut best: Option<&Value> = None;
                    let mut best_val: Option<f64> = None;
                    let mut skipped = 0u32;
                    for item in arr {
                        if let Some(fv) = navigate_dot_path(item, field) {
                            if let Some(n) = fv.as_f64() {
                                if best_val.is_none() || n < best_val.unwrap() {
                                    best = Some(item);
                                    best_val = Some(n);
                                }
                            } else {
                                skipped += 1;
                            }
                        } else {
                            skipped += 1;
                        }
                    }
                    if skipped > 0 {
                        tracing::debug!(
                            "min_by('{}'): skipped {} item(s) with missing or non-numeric field",
                            field,
                            skipped
                        );
                    }
                    Ok(best.cloned().unwrap_or(Value::Null))
                }
                _ => Err(type_mismatch("min_by", "array", value)),
            },
            TransformOp::MaxBy(field) => match value {
                Value::Null => Ok(Value::Null),
                Value::Array(arr) if arr.is_empty() => Ok(Value::Null),
                Value::Array(arr) => {
                    let mut best: Option<&Value> = None;
                    let mut best_val: Option<f64> = None;
                    let mut skipped = 0u32;
                    for item in arr {
                        if let Some(fv) = navigate_dot_path(item, field) {
                            if let Some(n) = fv.as_f64() {
                                if best_val.is_none() || n > best_val.unwrap() {
                                    best = Some(item);
                                    best_val = Some(n);
                                }
                            } else {
                                skipped += 1;
                            }
                        } else {
                            skipped += 1;
                        }
                    }
                    if skipped > 0 {
                        tracing::debug!(
                            "max_by('{}'): skipped {} item(s) with missing or non-numeric field",
                            field,
                            skipped
                        );
                    }
                    Ok(best.cloned().unwrap_or(Value::Null))
                }
                _ => Err(type_mismatch("max_by", "array", value)),
            },
            TransformOp::Sum => match value {
                // Numeric-only sum (unlike `add` which also concats strings/arrays)
                Value::Null => Ok(Value::Null),
                Value::Array(arr) if arr.is_empty() => Ok(Value::Null),
                Value::Array(arr) => {
                    let mut sum = 0.0_f64;
                    for item in arr {
                        match item {
                            Value::Number(n) => {
                                sum += n.as_f64().unwrap_or(0.0);
                            }
                            Value::Null => {} // skip nulls
                            _ => return Err(type_mismatch("sum", "array of numbers", item)),
                        }
                    }
                    Ok(f64_to_json_number(sum))
                }
                _ => Err(type_mismatch("sum", "array of numbers", value)),
            },
            TransformOp::Avg => match value {
                Value::Null => Ok(Value::Null),
                Value::Array(arr) if arr.is_empty() => Ok(Value::Null),
                Value::Array(arr) => {
                    let mut sum = 0.0_f64;
                    let mut count = 0u64;
                    for item in arr {
                        match item {
                            Value::Number(n) => {
                                sum += n.as_f64().unwrap_or(0.0);
                                count += 1;
                            }
                            Value::Null => {} // skip
                            _ => return Err(type_mismatch("avg", "array of numbers", item)),
                        }
                    }
                    if count == 0 {
                        return Ok(Value::Null);
                    }
                    let avg = sum / count as f64;
                    Ok(f64_to_json_number(avg))
                }
                _ => Err(type_mismatch("avg", "array", value)),
            },

            // ── Introspection ─────────────────────────────────────────
            TransformOp::Has(key) => match value {
                Value::Null => Ok(Value::Null),
                Value::Object(map) => Ok(Value::Bool(map.contains_key(key.as_str()))),
                _ => Err(type_mismatch("has", "object", value)),
            },

            // ── Logic ─────────────────────────────────────────────────
            TransformOp::Not => match value {
                Value::Null => Ok(Value::Null), // propagating
                Value::Bool(b) => Ok(Value::Bool(!b)),
                _ => Err(type_mismatch("not", "boolean", value)),
            },

            // ── jq expression ──────────────────────────────────────
            TransformOp::Jq(expr) => {
                eval_jq(expr, value).map_err(|e| TransformError::TypeMismatch {
                    op: "jq",
                    expected: "valid jq expression",
                    got: e,
                })
            }

            // ── Security escaping (Nika Shield) ────────────────
            TransformOp::HtmlEscape => match value {
                Value::Null => Err(TransformError::NullInput { op: "html_escape" }),
                Value::String(s) => Ok(Value::String(
                    s.replace('&', "&amp;")
                        .replace('<', "&lt;")
                        .replace('>', "&gt;")
                        .replace('"', "&quot;")
                        .replace('\'', "&#x27;"),
                )),
                _ => Err(type_mismatch("html_escape", "string", value)),
            },
            TransformOp::MdEscape => match value {
                Value::Null => Err(TransformError::NullInput { op: "md_escape" }),
                Value::String(s) => Ok(Value::String(
                    s.replace('\\', "\\\\")
                        .replace('`', "\\`")
                        .replace('*', "\\*")
                        .replace('_', "\\_")
                        .replace('[', "\\[")
                        .replace(']', "\\]")
                        .replace('#', "\\#"),
                )),
                _ => Err(type_mismatch("md_escape", "string", value)),
            },
            TransformOp::Sanitize => match value {
                Value::Null => Err(TransformError::NullInput { op: "sanitize" }),
                Value::String(s) => {
                    let mut result = s.clone();
                    // Case-insensitive removal of common injection patterns
                    let patterns = [
                        "ignore previous",
                        "ignore all previous",
                        "disregard above",
                        "disregard previous",
                        "forget your instructions",
                        "you are now",
                        "new instructions:",
                        "system prompt:",
                    ];
                    for pat in &patterns {
                        // Case-insensitive single removal
                        let lower = result.to_lowercase();
                        if let Some(idx) = lower.find(pat) {
                            // Byte-safe slicing since patterns are ASCII
                            result = format!("{}{}", &result[..idx], &result[idx + pat.len()..]);
                        }
                    }
                    Ok(Value::String(result.trim().to_string()))
                }
                _ => Err(type_mismatch("sanitize", "string", value)),
            },
        }
    }
}

/// Type alias for the compiled jq filter (jaq 3.x).
type JaqFilter = jaq_core::Filter<jaq_core::data::JustLut<jaq_json::Val>>;

/// Evaluate a jq expression against a JSON value.
///
/// Used by `| jq()` transform AND `nika:jq` builtin tool.
/// Returns single value for one result, array for multiple, null for empty.
pub fn eval_jq(expr: &str, data: &Value) -> Result<Value, String> {
    let filter = compile_jq(expr)?;

    let jaq_val: jaq_json::Val =
        serde_json::from_value(data.clone()).map_err(|e| format!("jq input error: {e}"))?;

    // Safety cap: jq expressions like `def r:.,r; null|r` produce infinite
    // iterators. Without this limit, such expressions exhaust memory before
    // catch_unwind can intervene.
    const JQ_MAX_RESULTS: usize = 100_000;

    // Wrap in catch_unwind to convert panics into clean errors.
    let run_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let ctx: jaq_core::Ctx<jaq_core::data::JustLut<jaq_json::Val>> =
            jaq_core::Ctx::new(&filter.lut, jaq_core::Vars::new([]));
        let mut results: Vec<Value> = Vec::new();
        for r in filter.id.run((ctx, jaq_val)) {
            if results.len() >= JQ_MAX_RESULTS {
                return Err(format!(
                    "jq expression produced more than {JQ_MAX_RESULTS} results (possible infinite loop)"
                ));
            }
            match r {
                Ok(val) => {
                    let json_str = format!("{val}");
                    let serde_val: Value =
                        serde_json::from_str(&json_str).unwrap_or(Value::String(json_str));
                    results.push(serde_val);
                }
                Err(e) => return Err(format!("jq runtime error: {e:?}")),
            }
        }
        Ok(results)
    }));

    let results = match run_result {
        Ok(Ok(r)) => r,
        Ok(Err(e)) => return Err(e),
        Err(_) => return Err("jq expression panicked (likely regex on null input)".into()),
    };

    match results.len() {
        0 => Ok(Value::Null),
        1 => Ok(results.into_iter().next().unwrap()),
        _ => Ok(Value::Array(results)),
    }
}

/// Global LRU cache for compiled jq filters.
/// `Filter` is not Clone in jaq 3.x, so we wrap in Arc.
/// 64 entries covers realistic workflow diversity (most workflows use <10 distinct expressions).
static JQ_FILTER_CACHE: LazyLock<Mutex<lru::LruCache<String, Arc<JaqFilter>>>> =
    LazyLock::new(|| Mutex::new(lru::LruCache::new(NonZeroUsize::new(64).unwrap())));

/// Compile a jq expression using jaq-core 3.x.
/// Results are cached in a global LRU — for_each loops with the same expression
/// compile once instead of N times.
fn compile_jq(expr: &str) -> Result<Arc<JaqFilter>, String> {
    // Fast path: check cache
    {
        let mut cache = JQ_FILTER_CACHE.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(filter) = cache.get(expr) {
            return Ok(Arc::clone(filter));
        }
    }

    // Slow path: parse + compile
    let defs = jaq_core::defs()
        .chain(jaq_std::defs())
        .chain(jaq_json::defs());
    let funs = jaq_core::funs()
        .chain(jaq_std::funs())
        .chain(jaq_json::funs());

    let loader = jaq_core::load::Loader::new(defs);
    let arena = jaq_core::load::Arena::default();
    let program = jaq_core::load::File {
        code: expr,
        path: (),
    };

    let modules = loader.load(&arena, program).map_err(|errs| {
        format!(
            "parse error: {}",
            errs.into_iter()
                .map(|e| format!("{e:?}"))
                .collect::<Vec<_>>()
                .join(", ")
        )
    })?;

    let filter = jaq_core::Compiler::default()
        .with_funs(funs)
        .compile(modules)
        .map_err(|errs| {
            format!(
                "compile error: {}",
                errs.into_iter()
                    .map(|e| format!("{e:?}"))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        })?;

    let filter = Arc::new(filter);

    // Store in cache
    {
        let mut cache = JQ_FILTER_CACHE.lock().unwrap_or_else(|e| e.into_inner());
        cache.put(expr.to_string(), Arc::clone(&filter));
    }

    Ok(filter)
}

/// Deep merge overlay into base (RFC 7396 semantics).
pub fn deep_merge(
    base: &mut serde_json::Map<String, Value>,
    overlay: &serde_json::Map<String, Value>,
) {
    for (key, value) in overlay {
        match (base.get_mut(key), value) {
            (Some(Value::Object(base_obj)), Value::Object(overlay_obj)) => {
                deep_merge(base_obj, overlay_obj);
            }
            _ => {
                base.insert(key.clone(), value.clone());
            }
        }
    }
}

/// Navigate a dot-separated path into a JSON value.
/// e.g. `navigate_dot_path(obj, "address.city")` → `obj["address"]["city"]`
pub fn navigate_dot_path<'a>(value: &'a Value, path: &str) -> Option<&'a Value> {
    let mut current = value;
    for segment in path.split('.') {
        current = current.get(segment)?;
    }
    Some(current)
}

/// Cache for compiled regexes (dynamic patterns from user expressions).
/// Bounded LRU (128 entries) to prevent unbounded growth from user-supplied patterns.
static REGEX_CACHE: LazyLock<Mutex<lru::LruCache<String, regex::Regex>>> =
    LazyLock::new(|| Mutex::new(lru::LruCache::new(NonZeroUsize::new(128).unwrap())));

/// Get or compile a regex pattern, caching the result.
fn cached_regex(pattern: &str) -> Result<regex::Regex, regex::Error> {
    let mut cache = REGEX_CACHE.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(re) = cache.get(pattern) {
        return Ok(re.clone());
    }
    let re = regex::Regex::new(pattern)?;
    cache.put(pattern.to_string(), re.clone());
    Ok(re)
}

/// Known tracking / analytics parameters that don't affect page content.
/// Sources: Firecrawl, Scrapy w3lib, Google SEO guidelines, industry standard.
fn is_tracking_param(key: &str) -> bool {
    let lower = key.to_ascii_lowercase();
    matches!(
        lower.as_str(),
        // Google Analytics
        "utm_source"
            | "utm_medium"
            | "utm_campaign"
            | "utm_term"
            | "utm_content"
            | "utm_id"
            // Google Ads
            | "gclid"
            | "gclsrc"
            | "dclid"
            | "gbraid"
            | "wbraid"
            // Facebook / Meta
            | "fbclid"
            | "fb_action_ids"
            | "fb_action_types"
            | "fb_source"
            | "fb_ref"
            // Microsoft / Bing
            | "msclkid"
            // Mailchimp
            | "mc_cid"
            | "mc_eid"
            // HubSpot
            | "hsa_cam"
            | "hsa_grp"
            | "hsa_mt"
            | "hsa_src"
            | "hsa_ad"
            | "hsa_acc"
            | "hsa_net"
            | "hsa_ver"
            | "hsa_la"
            | "hsa_ol"
            | "hsa_kw"
            | "hsa_tgt"
            // Other common trackers
            | "_ga"
            | "_gl"
            | "_hsenc"
            | "_hsmi"
            | "mkt_tok"
            | "igshid"
            | "si"
            | "s_kwcid"
            | "ef_id"
            // TikTok
            | "ttclid"
            // Twitter / X
            | "twclid"
            // Adobe
            | "s_cid"
            // Matomo / Piwik
            | "mtm_source"
            | "mtm_medium"
            | "mtm_campaign"
            | "mtm_keyword"
            | "mtm_content"
            | "pk_source"
            | "pk_medium"
            | "pk_campaign"
            | "pk_keyword"
            | "pk_content"
    )
}

