// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Resolved Bindings - runtime value resolution
//!
//! ResolvedBindings holds resolved values from `with:` blocks for template resolution.
//! Supports both eager (immediate) and lazy (deferred) resolution.
//!
//! ## Binding syntax
//!
//! Unified syntax: `alias: task.path [?? default]`
//! Extended syntax: `alias: {path: task.path, lazy: true}`
//!
//! Rich typed paths with transforms:
//! ```yaml
//! with:
//!   summary: $step1.abstract | lower | trim ?? "No abstract"
//!   data:
//!     from: $step1.data
//!     type: object
//!     transform: sort_keys
//! ```
//!
//! Data flow:
//! ```text
//! WithEntry.source (BindingPath)
//!     ↓ dispatch by BindingSource
//!     ├── Task(id) → datastore.get_output(id) + navigate PathSegments
//!     ├── Input(sub) → datastore.resolve_input_path("inputs.{sub}")
//!     ├── Context(sub) → datastore.get_context_file/session
//!     ├── Env(var) → std::env::var(var)
//!     └── LoopVar(_) → error (should be pre-resolved)
//!     ↓
//! Apply WithEntry.transform (TransformExpr pipeline)
//!     ↓
//! Apply WithEntry.default (if value is null/missing)
//!     ↓
//! Validate WithEntry.binding_type (BindingType constraint)
//!     ↓
//! LazyBinding::Resolved(value) or error
//! ```
//!
//! Uses FxHashMap for faster hashing (consistent with Dag).

use std::sync::Arc;

use rustc_hash::FxHashMap;
use serde_json::Value;

use super::jsonpath;
use super::store::BindingStore;
use super::transform::TransformExpr;
use super::types::{BindingPath, BindingSource, BindingType, PathSegment};
use super::{BindingEntry, BindingEvent, BindingResolveError, BindingSpec, WithEntry, WithSpec};

/// Lazy binding state - either resolved or pending
///
/// Pending now stores `BindingPath` + optional `TransformExpr` + optional default
/// instead of raw `String` path. This enables typed source dispatch and transform
/// application during lazy resolution.
#[derive(Debug, Clone)]
#[allow(clippy::large_enum_variant)]
pub enum LazyBinding {
    /// Already resolved to a concrete value (eager bindings)
    Resolved(Value),
    /// Pending resolution — stores raw path string
    Pending {
        path: String,
        default: Option<Value>,
    },
    /// Pending typed resolution — stores BindingPath + transforms
    PendingWithEntry {
        source: BindingPath,
        binding_type: BindingType,
        default: Option<Value>,
        transform: Option<TransformExpr>,
    },
}

impl LazyBinding {
    /// Check if this binding is pending resolution
    pub fn is_pending(&self) -> bool {
        matches!(
            self,
            LazyBinding::Pending { .. } | LazyBinding::PendingWithEntry { .. }
        )
    }

    /// Get the value if already resolved
    pub fn get_value(&self) -> Option<&Value> {
        match self {
            LazyBinding::Resolved(v) => Some(v),
            LazyBinding::Pending { .. } | LazyBinding::PendingWithEntry { .. } => None,
        }
    }
}

/// Resolved bindings from with: block (alias -> value or pending)
///
/// Uses FxHashMap for faster hashing on small string keys.
/// Supports both eager and lazy bindings.
/// Also provides `from_with_spec()` for the typed binding system.
#[derive(Debug, Clone, Default)]
pub struct ResolvedBindings {
    /// Alias -> binding mappings (resolved or pending)
    bindings: FxHashMap<String, LazyBinding>,
    /// Alias -> source task ID (for media path resolution)
    ///
    /// When a binding like `img: $gen_img` is created, this maps
    /// `"img"` -> `"gen_img"`. This is needed because media refs live
    /// in the TaskResult side-channel, not in the task output value.
    /// Without this, templates like `{{with.img.media[0].hash}}` and
    /// binary artifact `source: img` cannot resolve media paths.
    source_tasks: FxHashMap<String, String>,
    /// Aliases whose values were resolved from $env — must be masked in traces.
    env_sourced: rustc_hash::FxHashSet<String>,
    /// Aliases whose values came from `inputs:` (workflow inputs) — used by
    /// the Nika Shield spotlight pre-pass to know whether to wrap based on
    /// `RunContext::invocation_source().input_trust()`.
    input_sourced: rustc_hash::FxHashSet<String>,
    /// Aliases whose values came from a `for_each` loop variable. The
    /// spotlight pre-pass treats these as untrusted unless the parent
    /// for_each iterator is trusted (Sprint 3 will refine this).
    loop_var_sourced: rustc_hash::FxHashSet<String>,
}

impl ResolvedBindings {
    /// Create empty bindings
    pub fn new() -> Self {
        Self::default()
    }

    // ═══════════════════════════════════════════════════════════════
    // String-path resolution: from_binding_spec (BindingEntry / BindingSpec)
    // ═══════════════════════════════════════════════════════════════

    /// Build bindings from with: block by resolving paths from datastore
    ///
    /// Unified resolution for both syntax styles:
    /// - String: `task.path [?? default]` → eager resolution
    /// - Object: `{path, lazy?, default?}` → lazy or eager based on flag
    ///
    /// Lazy bindings are stored as Pending and resolved on first access.
    /// Eager bindings are resolved immediately and fail if source is missing.
    ///
    /// Returns empty bindings if binding_spec is None.
    pub fn from_binding_spec(
        binding_spec: Option<&BindingSpec>,
        datastore: &dyn BindingStore,
    ) -> Result<Self, BindingResolveError> {
        let Some(spec) = binding_spec else {
            return Ok(Self::new());
        };

        let mut resolved = Self::new();

        for (alias, entry) in spec {
            // Track source task ID for media path resolution
            let (task_id, _) = split_path(&entry.path);
            if !task_id.starts_with("inputs.")
                && !task_id.starts_with("context.")
                && !task_id.starts_with("env.")
            {
                resolved
                    .source_tasks
                    .insert(alias.clone(), task_id.to_string());
            }

            // Track env-sourced bindings for secret masking in traces
            if task_id.starts_with("env.") {
                resolved.env_sourced.insert(alias.clone());
            }

            if entry.is_lazy() {
                // Lazy binding - defer resolution
                resolved.bindings.insert(
                    alias.clone(),
                    LazyBinding::Pending {
                        path: entry.path.clone(),
                        default: entry.default.clone(),
                    },
                );
            } else {
                // Eager binding - resolve immediately
                let value = resolve_entry(entry, alias, datastore)?;
                resolved
                    .bindings
                    .insert(alias.clone(), LazyBinding::Resolved(value));
            }
        }

        Ok(resolved)
    }

    // ═══════════════════════════════════════════════════════════════
    // Typed resolution: from_with_spec (WithEntry / WithSpec)
    // ═══════════════════════════════════════════════════════════════

    /// Build bindings from with: spec by resolving typed BindingPaths
    ///
    /// Resolution order per entry:
    /// 1. Dispatch by BindingSource (Task/Input/Context/Env)
    /// 2. Navigate PathSegments for nested value access
    /// 3. Apply TransformExpr pipeline (if present)
    /// 4. Apply default value (if result is null/missing)
    /// 5. Validate BindingType constraint
    ///
    /// Lazy bindings are stored as PendingWithEntry for later resolution.
    pub fn from_with_spec(
        with_spec: Option<&WithSpec>,
        datastore: &dyn BindingStore,
    ) -> Result<Self, BindingResolveError> {
        let Some(spec) = with_spec else {
            return Ok(Self::new());
        };

        let mut bindings = Self::new();

        for (alias, entry) in spec {
            // Track source task ID for media path resolution
            if let Some(task_id) = entry.source.task_id() {
                bindings
                    .source_tasks
                    .insert(alias.clone(), task_id.to_string());
            }

            // Track env-sourced and vault-sourced bindings for secret masking in traces
            if matches!(
                &entry.source.source,
                BindingSource::Env(_) | BindingSource::Vault { .. }
            ) {
                bindings.env_sourced.insert(alias.clone());
            }

            // Track Input + LoopVar sources so the Nika Shield spotlight pre-pass
            // can decide whether to wrap based on the run's invocation source.
            match &entry.source.source {
                BindingSource::Input(_) => {
                    bindings.input_sourced.insert(alias.clone());
                }
                BindingSource::LoopVar(_) => {
                    bindings.loop_var_sourced.insert(alias.clone());
                }
                _ => {}
            }

            if entry.is_lazy() {
                bindings.bindings.insert(
                    alias.clone(),
                    LazyBinding::PendingWithEntry {
                        source: entry.source.clone(),
                        binding_type: entry.binding_type,
                        default: entry.default.clone(),
                        transform: entry.transform.clone(),
                    },
                );
            } else {
                let value = resolve_with_entry(entry, alias, datastore)?;
                bindings
                    .bindings
                    .insert(alias.clone(), LazyBinding::Resolved(value));
            }
        }

        Ok(bindings)
    }

    /// Build bindings from with: spec with event collection for telemetry.
    ///
    /// Same as `from_with_spec` but collects binding events (defaults applied,
    /// transforms executed, env vars resolved) for the caller to emit.
    pub fn from_with_spec_traced(
        with_spec: Option<&WithSpec>,
        datastore: &dyn BindingStore,
        task_id: &Arc<str>,
    ) -> Result<(Self, Vec<BindingEvent>), BindingResolveError> {
        let Some(spec) = with_spec else {
            return Ok((Self::new(), vec![]));
        };

        let mut bindings = Self::new();
        let mut events = Vec::with_capacity(spec.len());

        for (alias, entry) in spec {
            if let Some(tid) = entry.source.task_id() {
                bindings.source_tasks.insert(alias.clone(), tid.to_string());
            }

            // Track env-sourced and vault-sourced bindings for secret masking in traces
            if matches!(
                &entry.source.source,
                BindingSource::Env(_) | BindingSource::Vault { .. }
            ) {
                bindings.env_sourced.insert(alias.clone());
            }

            // Track Input + LoopVar sources for the Nika Shield spotlight pre-pass.
            match &entry.source.source {
                BindingSource::Input(_) => {
                    bindings.input_sourced.insert(alias.clone());
                }
                BindingSource::LoopVar(_) => {
                    bindings.loop_var_sourced.insert(alias.clone());
                }
                _ => {}
            }

            if entry.is_lazy() {
                bindings.bindings.insert(
                    alias.clone(),
                    LazyBinding::PendingWithEntry {
                        source: entry.source.clone(),
                        binding_type: entry.binding_type,
                        default: entry.default.clone(),
                        transform: entry.transform.clone(),
                    },
                );
            } else {
                let value =
                    resolve_with_entry_traced(entry, alias, datastore, task_id, &mut events)?;
                bindings
                    .bindings
                    .insert(alias.clone(), LazyBinding::Resolved(value));
            }
        }

        Ok((bindings, events))
    }

    // ═══════════════════════════════════════════════════════════════
    // Common API
    // ═══════════════════════════════════════════════════════════════

    /// Set a resolved value (always eager)
    pub fn set(&mut self, alias: impl Into<String>, value: Value) {
        self.bindings
            .insert(alias.into(), LazyBinding::Resolved(value));
    }

    /// Test-only helper: insert a raw `LazyBinding` directly, bypassing
    /// the normal resolution path. Not part of the stable API — production
    /// code should use `set()` / `from_with_spec()`.
    #[doc(hidden)]
    pub fn insert_raw(&mut self, alias: impl Into<String>, binding: LazyBinding) {
        self.bindings.insert(alias.into(), binding);
    }

    /// Test-only helper: mark an alias as `$env`-sourced so `to_value_redacted`
    /// masks its value. Not part of the stable API.
    #[doc(hidden)]
    pub fn mark_env_sourced(&mut self, alias: impl Into<String>) {
        self.env_sourced.insert(alias.into());
    }

    /// Set a resolved value with source task ID tracking.
    ///
    /// Use this when the binding originates from a task output, so that
    /// media path resolution (e.g., `{{with.alias.media[0].hash}}`) can
    /// trace back to the correct task's media refs.
    pub fn set_with_source(
        &mut self,
        alias: impl Into<String>,
        value: Value,
        source_task_id: impl Into<String>,
    ) {
        let alias = alias.into();
        self.source_tasks
            .insert(alias.clone(), source_task_id.into());
        self.bindings.insert(alias, LazyBinding::Resolved(value));
    }

    /// Get the source task ID for a binding alias.
    ///
    /// Returns `Some("gen_img")` when the binding was `img: $gen_img`.
    /// Used by artifact processor to resolve media paths and binary artifact sources.
    pub fn source_task_id(&self, alias: &str) -> Option<&str> {
        self.source_tasks.get(alias).map(|s| s.as_str())
    }

    /// True if the binding was sourced from a workflow `inputs:` parameter.
    /// Used by the Nika Shield spotlight pre-pass to derive trust from
    /// `RunContext::invocation_source().input_trust()`.
    #[inline]
    pub fn is_input_sourced(&self, alias: &str) -> bool {
        self.input_sourced.contains(alias)
    }

    /// True if the binding was sourced from a `for_each` loop variable.
    /// Currently treated as untrusted in the spotlight pre-pass; Sprint 3
    /// will refine this once `for_each` carries upstream trust through.
    #[inline]
    pub fn is_loop_var_sourced(&self, alias: &str) -> bool {
        self.loop_var_sourced.contains(alias)
    }

    /// Get a resolved value (only works for already-resolved bindings)
    ///
    /// For lazy bindings that haven't been resolved yet, returns None.
    /// Use `get_resolved()` to force resolution of lazy bindings.
    pub fn get(&self, alias: &str) -> Option<&Value> {
        self.bindings.get(alias).and_then(|b| b.get_value())
    }

    /// Get a resolved value, resolving lazy bindings on demand
    ///
    /// For eager bindings, returns the pre-resolved value.
    /// For lazy bindings, resolves from datastore on first call.
    ///
    /// Note: This doesn't cache the resolution - each call re-resolves.
    /// This is intentional to support changing datastore values.
    pub fn get_resolved(&self, alias: &str, datastore: &dyn BindingStore) -> Result<Value, BindingResolveError> {
        match self.bindings.get(alias) {
            Some(LazyBinding::Resolved(value)) => Ok(value.clone()),
            Some(LazyBinding::Pending { path, default }) => {
                // String-path: resolve via BindingEntry
                let entry = BindingEntry {
                    path: path.clone(),
                    default: default.clone(),
                    lazy: true,
                };
                resolve_entry(&entry, alias, datastore)
            }
            Some(LazyBinding::PendingWithEntry {
                source,
                binding_type,
                default,
                transform,
            }) => {
                // Typed: resolve via WithEntry
                let entry = WithEntry {
                    source: source.clone(),
                    binding_type: *binding_type,
                    default: default.clone(),
                    lazy: true,
                    transform: transform.clone(),
                };
                resolve_with_entry(&entry, alias, datastore)
            }
            None => Err(BindingResolveError::NotFound {
                alias: alias.to_string(),
            }),
        }
    }

    /// Check if a binding is lazy (pending resolution)
    pub fn is_lazy(&self, alias: &str) -> bool {
        self.bindings
            .get(alias)
            .map(|b| b.is_pending())
            .unwrap_or(false)
    }

    /// Check if context has any bindings
    pub fn is_empty(&self) -> bool {
        self.bindings.is_empty()
    }

    /// Iterate over resolved bindings (alias, value pairs)
    ///
    /// Only returns already-resolved bindings. Pending lazy bindings are skipped.
    /// Use this for event logging where we want to capture resolved values.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &Value)> {
        self.bindings
            .iter()
            .filter_map(|(alias, binding)| binding.get_value().map(|value| (alias.as_str(), value)))
    }

    /// Serialize context to JSON Value for event logging
    ///
    /// Returns the full resolved inputs as a JSON object.
    /// Lazy bindings that haven't been resolved are represented as marker objects.
    /// Used by EventLog for TaskStarted events (inputs field).
    pub fn to_value(&self) -> Value {
        let mut map = serde_json::Map::new();
        for (alias, binding) in &self.bindings {
            match binding {
                LazyBinding::Resolved(v) => {
                    map.insert(alias.clone(), v.clone());
                }
                LazyBinding::Pending { path, default: _ } => {
                    // Represent pending as a marker object
                    map.insert(
                        alias.clone(),
                        serde_json::json!({"__lazy__": true, "path": path}),
                    );
                }
                LazyBinding::PendingWithEntry {
                    source, default: _, ..
                } => {
                    // Represent pending with typed path
                    map.insert(
                        alias.clone(),
                        serde_json::json!({"__lazy__": true, "path": source.to_string()}),
                    );
                }
            }
        }
        Value::Object(map)
    }

    /// Collect env-sourced secret values for value-based redaction in traces.
    ///
    /// Returns the actual string values of bindings resolved from `$env.*`.
    /// The caller can use these to search-and-replace in trace output,
    /// catching secrets that pattern-based redaction (`redact_secrets()`) misses
    /// (e.g., custom API keys like `ELEVENLABS_API_KEY`).
    pub fn env_sourced_values(&self) -> Vec<String> {
        self.env_sourced
            .iter()
            .filter_map(|alias| {
                self.bindings.get(alias).and_then(|b| match b {
                    LazyBinding::Resolved(v) => match v {
                        Value::String(s) if s.len() >= 8 => Some(s.clone()),
                        _ => None,
                    },
                    _ => None,
                })
            })
            .collect()
    }

    /// Serialize context to JSON Value with env-sourced secrets masked.
    ///
    /// Same as `to_value()` but replaces env-sourced binding values with
    /// `"[REDACTED:$env]"` to prevent secret leakage in trace files.
    /// Also applies the standard API key regex pattern to all string values.
    pub fn to_value_redacted(&self) -> Value {
        let mut map = serde_json::Map::new();
        for (alias, binding) in &self.bindings {
            if self.env_sourced.contains(alias) {
                map.insert(alias.clone(), Value::String("[REDACTED:$env]".to_string()));
                continue;
            }
            match binding {
                LazyBinding::Resolved(v) => {
                    map.insert(alias.clone(), crate::util::redact_value(v.clone()));
                }
                LazyBinding::Pending { path, default: _ } => {
                    map.insert(
                        alias.clone(),
                        serde_json::json!({"__lazy__": true, "path": path}),
                    );
                }
                LazyBinding::PendingWithEntry {
                    source, default: _, ..
                } => {
                    map.insert(
                        alias.clone(),
                        serde_json::json!({"__lazy__": true, "path": source.to_string()}),
                    );
                }
            }
        }
        Value::Object(map)
    }
}

// ═══════════════════════════════════════════════════════════════
// String-path resolution: BindingEntry (simple path bindings)
// ═══════════════════════════════════════════════════════════════

/// Resolve a single BindingEntry to a Value
///
/// Unified resolution logic:
/// 1. Check for inputs.* path (workflow inputs support)
/// 2. Extract task_id from path (first segment)
/// 3. Get task output from datastore
/// 4. Resolve remaining path within output
/// 5. Apply default if value is null/missing
pub fn resolve_entry(
    entry: &BindingEntry,
    alias: &str,
    datastore: &dyn BindingStore,
) -> Result<Value, BindingResolveError> {
    let path = &entry.path;

    // Check for inputs.* path first
    if path.starts_with("inputs.") {
        let value = datastore.resolve_input_path(path);
        return match value {
            Some(v) if !v.is_null() => Ok(v),
            Some(_) => entry
                .default
                .as_ref()
                .cloned()
                .ok_or_else(|| BindingResolveError::NullValue {
                    path: path.clone(),
                    alias: alias.to_string(),
                }),
            None => entry
                .default
                .as_ref()
                .cloned()
                .ok_or_else(|| BindingResolveError::PathNotFound { path: path.clone() }),
        };
    }

    // Split path into task_id and remaining path
    let (task_id, field_path) = split_path(path);

    // Intercept media paths: task_id.media, task_id.media[0].hash, etc.
    // The media side-channel is in TaskResult.media, not in TaskResult.output,
    // so get_output() cannot see it. Delegate to resolve_path() which handles
    // both output and media resolution.
    if let Some(fp) = field_path {
        if fp == "media" || fp.starts_with("media.") || fp.starts_with("media[") {
            let value = datastore.resolve_path(path);
            return match value {
                Some(v) if !v.is_null() => Ok(v),
                Some(_) => entry
                    .default
                    .as_ref()
                    .cloned()
                    .ok_or_else(|| BindingResolveError::NullValue {
                        path: path.clone(),
                        alias: alias.to_string(),
                    }),
                None => entry
                    .default
                    .as_ref()
                    .cloned()
                    .ok_or_else(|| BindingResolveError::PathNotFound { path: path.clone() }),
            };
        }
    }

    // Warn if binding to a failed task — output may be partial/null
    if datastore.is_failed(task_id) {
        tracing::warn!(
            task_id = %task_id,
            alias = %alias,
            "Binding to output of failed task — value may be null or partial"
        );
    }

    // Resolve the value from task output
    let value = match datastore.get_output(task_id) {
        Some(output) => {
            if let Some(fp) = field_path {
                jsonpath::resolve(&output, fp)?
            } else {
                Some((*output).clone())
            }
        }
        None => None,
    };

    // Apply default if value is null or missing
    match value {
        Some(v) if !v.is_null() => Ok(v),
        Some(_) => entry
            .default
            .as_ref()
            .cloned()
            .ok_or_else(|| BindingResolveError::NullValue {
                path: path.clone(),
                alias: alias.to_string(),
            }),
        None => entry
            .default
            .as_ref()
            .cloned()
            .ok_or_else(|| BindingResolveError::PathNotFound { path: path.clone() }),
    }
}

/// Split a path into task_id and remaining field path
///
/// Examples:
/// - "weather" -> ("weather", None)
/// - "weather.summary" -> ("weather", Some("summary"))
/// - "weather.data.temp" -> ("weather", Some("data.temp"))
pub fn split_path(path: &str) -> (&str, Option<&str>) {
    if let Some(dot_idx) = path.find('.') {
        let task_id = &path[..dot_idx];
        let field_path = &path[dot_idx + 1..];
        (task_id, Some(field_path))
    } else {
        (path, None)
    }
}

// ═══════════════════════════════════════════════════════════════
// Typed resolution: WithEntry (BindingPath dispatch + transforms)
// ═══════════════════════════════════════════════════════════════

/// Resolve a single WithEntry to a Value using typed BindingPath dispatch
///
/// Resolution pipeline:
/// 1. Dispatch by BindingSource to get raw value
/// 2. Navigate PathSegments for nested access
/// 3. Apply transform pipeline (if present)
/// 4. Apply default (if value is null/missing, AFTER transforms)
/// 5. Validate BindingType constraint
pub fn resolve_with_entry(
    entry: &WithEntry,
    alias: &str,
    datastore: &dyn BindingStore,
) -> Result<Value, BindingResolveError> {
    let path_str = entry.source.to_string();

    // Step 1+2: Dispatch by source and navigate segments
    let raw_value = resolve_binding_path(&entry.source, alias, datastore)?;

    // Step 3: Apply transforms.
    // For null values: attempt transforms so `default()` in the chain can fire.
    // If a non-default transform fails on null (NullInput), fall through to
    // the entry-level default in Step 4.
    let transformed = match (&raw_value, &entry.transform) {
        (Some(v), Some(expr)) if !v.is_null() => {
            Some(expr.apply(v).map_err(|e| BindingResolveError::PathNotFound {
                path: format!("{} (transform error: {})", path_str, e),
            })?)
        }
        (Some(v), Some(expr)) if v.is_null() => {
            // Null value: try transforms (default() handles null).
            // If transform fails on null input, skip to Step 4 default.
            match expr.apply(v) {
                Ok(result) => Some(result),
                Err(e) => {
                    tracing::debug!(path = %path_str, error = %e, "Transform failed on null value — falling through to default");
                    raw_value
                }
            }
        }
        // Missing source with a transform chain containing default() —
        // apply transforms on null so default() can fire.
        // e.g. $env.MISSING | default("fallback") | upper → "FALLBACK"
        (None, Some(expr)) if expr.has_default() => match expr.apply(&Value::Null) {
            Ok(result) => Some(result),
            Err(e) => {
                tracing::debug!(path = %path_str, error = %e, "Transform with default() failed on missing value");
                None // fall through to Step 4 PathNotFound
            }
        },
        _ => raw_value,
    };

    // Step 4: Apply default if null/missing
    let value = match transformed {
        Some(v) if !v.is_null() => v,
        Some(_null) => {
            // Value is null — use default or error
            match &entry.default {
                Some(d) => d.clone(),
                None => {
                    return Err(BindingResolveError::NullValue {
                        path: path_str,
                        alias: alias.to_string(),
                    });
                }
            }
        }
        None => {
            // Value not found — use default or error
            match &entry.default {
                Some(d) => d.clone(),
                None => {
                    return Err(BindingResolveError::PathNotFound { path: path_str });
                }
            }
        }
    };

    // Step 5: Validate BindingType constraint
    validate_binding_type(&value, entry.binding_type, alias, &path_str)?;

    Ok(value)
}

/// Dispatch resolution by BindingSource variant
///
/// Returns the raw value before transforms/defaults are applied.
/// Returns `Ok(None)` if the source exists but the specific path is missing.
pub fn resolve_binding_path(
    binding_path: &BindingPath,
    alias: &str,
    datastore: &dyn BindingStore,
) -> Result<Option<Value>, BindingResolveError> {
    match &binding_path.source {
        BindingSource::Task(task_id) => {
            // Warn if binding to a failed task — output may be partial/null
            if datastore.is_failed(task_id) {
                tracing::warn!(
                    task_id = %task_id,
                    alias = %alias,
                    "Binding to output of failed task — value may be null or partial"
                );
            }

            // Intercept media paths: segments starting with Field("media")
            // Media data lives in TaskResult.media (side-channel), not in
            // TaskResult.output. Delegate to resolve_path() which handles both.
            if matches!(
                binding_path.segments.first(),
                Some(crate::binding::types::PathSegment::Field(f)) if f.as_ref() == "media"
            ) {
                // Reconstruct the full dot-separated path for resolve_path
                let full_path = format!(
                    "{}{}",
                    task_id,
                    binding_path
                        .segments
                        .iter()
                        .fold(String::new(), |mut acc, seg| {
                            match seg {
                                crate::binding::types::PathSegment::Field(f) => {
                                    acc.push('.');
                                    acc.push_str(f);
                                }
                                crate::binding::types::PathSegment::Index(i) => {
                                    acc.push_str(&format!("[{}]", i));
                                }
                            }
                            acc
                        })
                );
                return Ok(datastore.resolve_path(&full_path));
            }

            // Record-aware bindings: if task has a compressed Record,
            // use Record fields instead of raw output.
            // get_record returns the opaque to_binding_value() JSON:
            // {"summary": "...", "key_findings": [...], "confidence": ...}
            if let Some(record_value) = datastore.get_record(task_id) {
                if binding_path.segments.is_empty() {
                    // $task → Record summary string (extract from opaque JSON)
                    let summary = record_value
                        .get("summary")
                        .and_then(|s| s.as_str())
                        .unwrap_or("");
                    return Ok(Some(Value::String(summary.to_string())));
                }
                // $task.raw → access raw output from TaskResult (bypass Record)
                if matches!(
                    binding_path.segments.first(),
                    Some(crate::binding::types::PathSegment::Field(f)) if f.as_ref() == "raw"
                ) {
                    return Ok(datastore.get_output(task_id).map(|v| v.as_ref().clone()));
                }
                // $task.field → navigate Record binding value
                return navigate_segments(&record_value, &binding_path.segments);
            }

            let output = match datastore.get_output(task_id) {
                Some(o) => o,
                None => return Ok(None),
            };

            // Navigate path segments through the value
            navigate_segments(&output, &binding_path.segments)
        }

        BindingSource::Input(sub_path) => {
            // RunContext.resolve_input_path expects "inputs.{sub_path}" format
            let full_path = format!("inputs.{}", sub_path);
            Ok(datastore.resolve_input_path(&full_path))
        }

        BindingSource::Context(sub_path) => {
            // Delegate to resolve_context_path which handles nested navigation:
            //   "files.brand"        → get file "brand" (full content)
            //   "files.brand.colors" → get file "brand", then navigate into .colors
            //   "session"            → full session object
            //   "session.focus"      → session field "focus"
            let full_path = format!("context.{}", sub_path);
            Ok(datastore.resolve_context_path(&full_path))
        }

        BindingSource::Env(var_name) => {
            // NOTE: daemon/vault secrets are pre-loaded into the SecretStore
            // by `load_from_daemon_or_fallback()` during boot. `resolve_env`
            // checks the store first, then falls back to std::env::var.

            // SEC-1: Block access to dangerous system/process env vars that
            // could enable privilege escalation or secret exfiltration if a
            // malicious workflow (e.g. from a package registry) references them.
            const BLOCKED_ENV_VARS: &[&str] = &[
                "SSH_AUTH_SOCK",
                "GPG_AGENT_INFO",
                "SUDO_ASKPASS",
                "LD_PRELOAD",
                "LD_LIBRARY_PATH",
                "DYLD_INSERT_LIBRARIES",
                "DYLD_LIBRARY_PATH",
                "NIKA_VAULT_PASSPHRASE",
                "NIKA_DAEMON_SOCKET",
                "NIKA_DAEMON_TOKEN",
            ];
            let name_upper = var_name.to_uppercase();
            if BLOCKED_ENV_VARS
                .iter()
                .any(|&blocked| name_upper == blocked)
            {
                tracing::warn!(var = %var_name, "Blocked access to restricted env var via $env binding");
                return Ok(None);
            }

            // Audit trail: log all $env accesses at INFO for secret patterns,
            // DEBUG for regular vars.
            const SECRET_PATTERNS: &[&str] =
                &["KEY", "SECRET", "TOKEN", "PASSWORD", "CREDENTIAL", "AUTH"];
            if SECRET_PATTERNS.iter().any(|p| name_upper.contains(p)) {
                tracing::info!(var = %var_name, "Accessing secret-pattern env var via $env binding");
            } else {
                tracing::debug!(var = %var_name, "Accessing env var via $env binding");
            }
            match datastore.resolve_env(var_name.as_ref()) {
                Some(val) => Ok(Some(Value::String(val))),
                None => Ok(None),
            }
        }

        BindingSource::Vault { service, field } => {
            // $vault.SERVICE.FIELD → read from NikaVault encrypted store
            // Vault values are ALWAYS secrets — they are marked for redaction
            // by the caller (env_sourced / vault_sourced).
            tracing::debug!(
                service = %service,
                field = %field,
                "Resolving $vault binding"
            );
            match datastore.vault_get_credential(service, field) {
                Ok(Some(val)) => Ok(Some(Value::String(val))),
                Ok(None) => Ok(None),
                Err(e) => Err(BindingResolveError::VaultAccess {
                    service: service.to_string(),
                    field: field.to_string(),
                    reason: e.to_string(),
                }),
            }
        }

        BindingSource::LoopVar(name) => {
            // Loop variables should be pre-resolved by the executor before reaching here.
            // If we get here, it means the loop variable wasn't set.
            Err(BindingResolveError::NotFound {
                alias: format!("{} (loop variable '{}' not pre-resolved)", alias, name),
            })
        }
    }
}

/// Same as resolve_with_entry but collects telemetry events
pub fn resolve_with_entry_traced(
    entry: &WithEntry,
    alias: &str,
    datastore: &dyn BindingStore,
    task_id: &Arc<str>,
    events: &mut Vec<BindingEvent>,
) -> Result<Value, BindingResolveError> {
    let path_str = entry.source.to_string();

    // Step 1+2: Dispatch by source and navigate segments
    let raw_value = resolve_binding_path_traced(&entry.source, alias, datastore, task_id, events)?;

    // Step 3: Apply transforms.
    // For null values: attempt transforms so `default()` in the chain can fire.
    // If a non-default transform fails on null (NullInput), fall through to
    // the entry-level default in Step 4.
    let transformed = match (&raw_value, &entry.transform) {
        (Some(v), Some(expr)) if !v.is_null() => {
            let result = expr.apply(v).map_err(|e| BindingResolveError::PathNotFound {
                path: format!("{} (transform error: {})", path_str, e),
            })?;
            // EMIT: BindingTransformApplied
            events.push(BindingEvent::TransformApplied {
                task_id: Arc::clone(task_id),
                alias: alias.to_string(),
                transform_chain: format!("{:?}", expr),
            });
            Some(result)
        }
        (Some(v), Some(expr)) if v.is_null() => {
            // Null value: try transforms (default() handles null).
            // If transform fails on null input, skip to Step 4 default.
            match expr.apply(v) {
                Ok(result) => {
                    events.push(BindingEvent::TransformApplied {
                        task_id: Arc::clone(task_id),
                        alias: alias.to_string(),
                        transform_chain: format!("{:?}", expr),
                    });
                    Some(result)
                }
                Err(e) => {
                    tracing::debug!(path = %path_str, error = %e, "Transform failed on null value — falling through to default");
                    raw_value
                }
            }
        }
        // BUG-038: Missing source with a transform chain containing default() —
        // apply transforms on null so default() can fire (mirrors non-traced version).
        (None, Some(expr)) if expr.has_default() => match expr.apply(&Value::Null) {
            Ok(result) => {
                events.push(BindingEvent::TransformApplied {
                    task_id: Arc::clone(task_id),
                    alias: alias.to_string(),
                    transform_chain: format!("{:?}", expr),
                });
                Some(result)
            }
            Err(e) => {
                tracing::debug!(path = %path_str, error = %e, "Transform with default() failed on missing value");
                None
            }
        },
        _ => raw_value,
    };

    // Step 4: Apply default if null/missing
    let value = match transformed {
        Some(v) if !v.is_null() => v,
        Some(_null) => {
            match &entry.default {
                Some(d) => {
                    // EMIT: BindingDefaultApplied (redact secrets in event log)
                    events.push(BindingEvent::DefaultApplied {
                        task_id: Arc::clone(task_id),
                        alias: alias.to_string(),
                        path: path_str.clone(),
                        default_value: Value::String(crate::util::redact_secrets(&d.to_string())),
                    });
                    d.clone()
                }
                None => {
                    return Err(BindingResolveError::NullValue {
                        path: path_str,
                        alias: alias.to_string(),
                    });
                }
            }
        }
        None => {
            match &entry.default {
                Some(d) => {
                    // EMIT: BindingDefaultApplied (redact secrets in event log)
                    events.push(BindingEvent::DefaultApplied {
                        task_id: Arc::clone(task_id),
                        alias: alias.to_string(),
                        path: path_str.clone(),
                        default_value: Value::String(crate::util::redact_secrets(&d.to_string())),
                    });
                    d.clone()
                }
                None => {
                    return Err(BindingResolveError::PathNotFound { path: path_str });
                }
            }
        }
    };

    // Step 5: Validate BindingType
    validate_binding_type(&value, entry.binding_type, alias, &path_str)?;

    Ok(value)
}

/// Same as resolve_binding_path but collects env var / vault resolution events
pub fn resolve_binding_path_traced(
    binding_path: &BindingPath,
    alias: &str,
    datastore: &dyn BindingStore,
    task_id: &Arc<str>,
    events: &mut Vec<BindingEvent>,
) -> Result<Option<Value>, BindingResolveError> {
    match &binding_path.source {
        BindingSource::Env(var_name) => {
            // Delegate to the secure resolve_binding_path (which enforces the
            // allowlist/blocklist), then emit the telemetry event.
            let result = resolve_binding_path(binding_path, alias, datastore)?;
            let found = result.is_some();
            events.push(BindingEvent::EnvResolved {
                task_id: Arc::clone(task_id),
                var_name: var_name.to_string(),
                found,
            });
            Ok(result)
        }
        BindingSource::Vault { service, field } => {
            let result = resolve_binding_path(binding_path, alias, datastore)?;
            let found = result.is_some();
            events.push(BindingEvent::VaultResolved {
                task_id: Arc::clone(task_id),
                service: service.to_string(),
                field: field.to_string(),
                found,
            });
            Ok(result)
        }
        // For all other sources, delegate to the original function
        _ => resolve_binding_path(binding_path, alias, datastore),
    }
}

/// Navigate a sequence of PathSegments through a JSON value
///
/// Returns `Ok(None)` if a segment doesn't match (missing field, out-of-bounds index).
pub fn navigate_segments(value: &Value, segments: &[PathSegment]) -> Result<Option<Value>, BindingResolveError> {
    if segments.is_empty() {
        return Ok(Some(value.clone()));
    }

    // Auto-parse JSON strings so exec: output like '{"name":"Nika"}'
    // can be navigated with segments like .name
    let parsed;
    let root = if let Some(v) = crate::binding::jsonpath::try_parse_json_str(value) {
        parsed = v;
        &parsed
    } else {
        value
    };

    let mut current = root;
    for segment in segments {
        match segment {
            PathSegment::Field(name) => match current {
                Value::Object(map) => match map.get(name.as_ref()) {
                    Some(v) => current = v,
                    None => return Ok(None),
                },
                _ => return Ok(None),
            },
            PathSegment::Index(idx) => match current {
                Value::Array(arr) => match arr.get(*idx) {
                    Some(v) => current = v,
                    None => return Ok(None),
                },
                _ => return Ok(None),
            },
        }
    }

    Ok(Some(current.clone()))
}

/// Validate that a value matches the expected BindingType constraint
///
/// BindingType::Any always passes. Other types check the JSON value variant.
pub fn validate_binding_type(
    value: &Value,
    binding_type: BindingType,
    alias: &str,
    path: &str,
) -> Result<(), BindingResolveError> {
    let matches = match binding_type {
        BindingType::Any => true,
        BindingType::String => value.is_string(),
        BindingType::Number => value.is_number(),
        BindingType::Integer => value.is_i64() || value.is_u64(),
        BindingType::Boolean => value.is_boolean(),
        BindingType::Array => value.is_array(),
        BindingType::Object => value.is_object(),
    };

    if !matches {
        return Err(BindingResolveError::TypeMismatch {
            expected: binding_type.to_string(),
            actual: json_type_name(value).to_string(),
            path: format!("{} (alias: {})", path, alias),
        });
    }

    Ok(())
}

/// Get a human-readable type name for a JSON value
pub fn json_type_name(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

