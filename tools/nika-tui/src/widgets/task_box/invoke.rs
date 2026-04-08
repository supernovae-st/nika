// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! InvokeBox Widget
//!
//! MCP tool call box with params/result visualization.
//! Shows tool name, server, parameters, and result.
//!
//! Rendering logic lives in the sibling `invoke_render` module.

use super::{BoxState, RenderMode};

/// Specialized rendering hints for known builtin tools.
/// Detected at InvokeBox construction time from the tool name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BuiltinHint {
    #[default]
    Generic,
    /// nika:read, nika:glob, nika:grep
    FileRead,
    /// nika:write, nika:edit
    FileWrite,
    /// nika:thumbnail, nika:convert, nika:strip, nika:metadata, nika:optimize, nika:svg_render
    MediaThumbnail,
    /// nika:pipeline
    MediaPipeline,
    /// nika:import
    Import,
    /// nika:assert
    Assert,
    /// nika:complete
    Complete,
    /// nika:sleep
    Sleep,
}

impl BuiltinHint {
    /// Detect the builtin hint from a tool name.
    pub fn from_tool_name(name: &str) -> Self {
        match name {
            "nika:read" | "nika:glob" | "nika:grep" => Self::FileRead,
            "nika:write" | "nika:edit" => Self::FileWrite,
            "nika:thumbnail" | "nika:convert" | "nika:strip" | "nika:metadata"
            | "nika:optimize" | "nika:svg_render" => Self::MediaThumbnail,
            "nika:pipeline" => Self::MediaPipeline,
            "nika:import" => Self::Import,
            "nika:assert" => Self::Assert,
            "nika:complete" => Self::Complete,
            "nika:sleep" => Self::Sleep,
            _ => Self::Generic,
        }
    }

    /// Returns true if this hint corresponds to a known builtin tool.
    pub fn is_builtin(&self) -> bool {
        !matches!(self, Self::Generic)
    }
}

/// InvokeBox data and rendering
#[derive(Debug, Clone)]
pub struct InvokeBox {
    /// Tool name (e.g., "novanet_describe")
    pub tool: String,
    /// MCP server name
    pub server: String,
    /// Input parameters (JSON)
    pub params: serde_json::Value,
    /// Result (JSON) - None if still running
    pub result: Option<serde_json::Value>,
    /// Error message if failed
    pub error: Option<String>,
    /// Execution state
    pub state: BoxState,
    /// Is params section expanded
    pub expanded_params: bool,
    /// Is result section expanded
    pub expanded_result: bool,
    // === PERF: JSON Cache Fields (avoid serde in render loop) ===
    // WARNING: These caches are ONLY updated via with_params()/with_result() builders.
    // Direct mutation of self.params/self.result will NOT invalidate the cache.
    // ALWAYS use builders or set_params()/set_result() methods to keep cache in sync.
    /// Cached one-line JSON for params (render collapsed mode)
    pub(crate) params_oneline_cached: Option<String>,
    /// Cached pretty JSON for params (render expanded mode)
    pub(crate) params_pretty_cached: Option<String>,
    /// Cached one-line JSON for result (render collapsed mode)
    pub(crate) result_oneline_cached: Option<String>,
    /// Cached pretty JSON for result (render expanded mode)
    pub(crate) result_pretty_cached: Option<String>,
    /// Pulse intensity for border animation (0.0-1.0)
    pub pulse_intensity: f32,
    /// Render mode (Compact/Expanded/Full)
    pub render_mode: RenderMode,
    /// Specialized rendering hint for known builtin tools
    pub builtin_hint: BuiltinHint,
}

impl InvokeBox {
    /// Create a new InvokeBox
    pub fn new(tool: impl Into<String>, server: impl Into<String>) -> Self {
        let tool = tool.into();
        let builtin_hint = BuiltinHint::from_tool_name(&tool);
        Self {
            tool,
            server: server.into(),
            params: serde_json::Value::Null,
            result: None,
            error: None,
            state: BoxState::default(),
            expanded_params: false,
            expanded_result: false,
            params_oneline_cached: None,
            params_pretty_cached: None,
            result_oneline_cached: None,
            result_pretty_cached: None,
            pulse_intensity: 0.0,
            render_mode: RenderMode::default(),
            builtin_hint,
        }
    }

    /// Set the state
    pub fn with_state(mut self, state: BoxState) -> Self {
        self.state = state;
        self
    }

    /// Set parameters (pre-caches JSON strings for render performance)
    pub fn with_params(mut self, params: serde_json::Value) -> Self {
        // PERF: Pre-compute JSON strings to avoid serde in render loop
        if !params.is_null() {
            self.params_oneline_cached = serde_json::to_string(&params).ok();
            self.params_pretty_cached = serde_json::to_string_pretty(&params).ok();
        }
        self.params = params;
        self
    }

    /// Set parameters from string (pre-caches JSON strings)
    pub fn with_params_str(self, params: &str) -> Self {
        let parsed = serde_json::from_str(params).unwrap_or(serde_json::Value::Null);
        self.with_params(parsed)
    }

    /// Set result (pre-caches JSON strings for render performance)
    pub fn with_result(mut self, result: serde_json::Value) -> Self {
        // PERF: Pre-compute JSON strings to avoid serde in render loop
        self.result_oneline_cached = serde_json::to_string(&result).ok();
        self.result_pretty_cached = serde_json::to_string_pretty(&result).ok();
        self.result = Some(result);
        self
    }

    /// Set result from string (pre-caches JSON strings)
    pub fn with_result_str(self, result: &str) -> Self {
        if let Ok(parsed) = serde_json::from_str(result) {
            self.with_result(parsed)
        } else {
            self
        }
    }

    /// Update result in-place (keeps JSON cache coherent)
    pub fn set_result(&mut self, result: serde_json::Value) {
        self.result_oneline_cached = serde_json::to_string(&result).ok();
        self.result_pretty_cached = serde_json::to_string_pretty(&result).ok();
        self.result = Some(result);
    }

    /// Set error
    pub fn with_error(mut self, error: impl Into<String>) -> Self {
        self.error = Some(error.into());
        self
    }

    /// Set pulse intensity for border animation (clamped to 0.0-1.0)
    pub fn with_pulse_intensity(mut self, intensity: f32) -> Self {
        self.pulse_intensity = intensity.clamp(0.0, 1.0);
        self
    }

    /// Set render mode
    pub fn with_render_mode(mut self, mode: RenderMode) -> Self {
        self.render_mode = mode;
        self
    }

    /// Toggle params expansion
    pub fn toggle_params(&mut self) {
        self.expanded_params = !self.expanded_params;
    }

    /// Toggle result expansion
    pub fn toggle_result(&mut self) {
        self.expanded_result = !self.expanded_result;
    }

    /// Calculate required height
    /// PERF: Uses cached JSON strings to avoid serde in layout calculation
    pub fn required_height(&self) -> u16 {
        // Compact mode is always 1 line
        if self.render_mode == RenderMode::Compact {
            return 1;
        }

        let mut height: u16 = 5; // Header + server + params header + result header + bottom

        // Params section - PERF: use cached pretty JSON
        if self.expanded_params && !self.params.is_null() {
            let lines = self
                .params_pretty_cached
                .as_ref()
                .map(|s| s.lines().count())
                .unwrap_or(0);
            height += lines.min(5) as u16;
        }

        // Result section - PERF: use cached pretty JSON
        if self.expanded_result && self.result.is_some() {
            let lines = self
                .result_pretty_cached
                .as_ref()
                .map(|s| s.lines().count())
                .unwrap_or(0);
            height += lines.min(5) as u16;
        }

        height
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_invoke_box_new() {
        let box_ = InvokeBox::new("novanet_describe", "novanet");
        assert_eq!(box_.tool, "novanet_describe");
        assert_eq!(box_.server, "novanet");
        assert!(box_.params.is_null());
        assert!(box_.result.is_none());
    }

    #[test]
    fn test_invoke_box_with_params() {
        let params = serde_json::json!({
            "entity": "qr-code",
            "locale": "fr-FR"
        });
        let box_ = InvokeBox::new("novanet_describe", "novanet").with_params(params.clone());

        assert_eq!(box_.params, params);
    }

    #[test]
    fn test_invoke_box_with_params_str() {
        let box_ = InvokeBox::new("tool", "server").with_params_str(r#"{"key": "value"}"#);

        assert!(box_.params.is_object());
        assert_eq!(box_.params["key"], "value");
    }

    #[test]
    fn test_invoke_box_with_result() {
        let result = serde_json::json!({
            "entity": {
                "key": "qr-code",
                "display_name": "QR Code"
            }
        });
        let box_ = InvokeBox::new("tool", "server").with_result(result.clone());

        assert_eq!(box_.result, Some(result));
    }

    #[test]
    fn test_invoke_box_with_error() {
        let box_ = InvokeBox::new("tool", "server").with_error("Entity not found");
        assert_eq!(box_.error, Some("Entity not found".to_string()));
    }

    #[test]
    fn test_toggle_sections() {
        let mut box_ = InvokeBox::new("tool", "server");
        assert!(!box_.expanded_params);
        assert!(!box_.expanded_result);

        box_.toggle_params();
        assert!(box_.expanded_params);

        box_.toggle_result();
        assert!(box_.expanded_result);
    }

    #[test]
    fn test_required_height() {
        let minimal = InvokeBox::new("tool", "server");
        assert!(minimal.required_height() >= 5);

        let with_params =
            InvokeBox::new("tool", "server").with_params(serde_json::json!({"key": "value"}));
        // Same when not expanded
        assert!(with_params.required_height() >= 5);
    }

    // === Cache Coherency Tests ===

    #[test]
    fn test_params_cache_populated_on_with_params() {
        let box_ = InvokeBox::new("tool", "server")
            .with_params(serde_json::json!({"key": "value", "num": 42}));

        // Cache should be populated
        assert!(box_.params_oneline_cached.is_some());
        assert!(box_.params_pretty_cached.is_some());

        // Cache content should match params
        let oneline = box_.params_oneline_cached.unwrap();
        assert!(oneline.contains("key"));
        assert!(oneline.contains("value"));
        assert!(oneline.contains("42"));

        // Pretty cache should have newlines (multi-line format)
        let pretty = box_.params_pretty_cached.unwrap();
        assert!(pretty.contains('\n'));
    }

    #[test]
    fn test_result_cache_populated_on_with_result() {
        let box_ = InvokeBox::new("tool", "server")
            .with_result(serde_json::json!({"entity": "qr-code", "locale": "fr-FR"}));

        // Cache should be populated
        assert!(box_.result_oneline_cached.is_some());
        assert!(box_.result_pretty_cached.is_some());

        // Cache content should match result
        let oneline = box_.result_oneline_cached.unwrap();
        assert!(oneline.contains("qr-code"));
        assert!(oneline.contains("fr-FR"));
    }

    #[test]
    fn test_empty_params_no_cache() {
        let box_ = InvokeBox::new("tool", "server");

        // No params = no cache
        assert!(box_.params_oneline_cached.is_none());
        assert!(box_.params_pretty_cached.is_none());
    }

    #[test]
    fn test_null_params_no_cache() {
        let box_ = InvokeBox::new("tool", "server").with_params(serde_json::Value::Null);

        // Null params = no cache (is_null() check in with_params)
        assert!(box_.params_oneline_cached.is_none());
        assert!(box_.params_pretty_cached.is_none());
    }

    #[test]
    fn test_params_str_uses_with_params() {
        let box_ = InvokeBox::new("tool", "server").with_params_str(r#"{"from": "string"}"#);

        // Should populate cache via with_params delegation
        assert!(box_.params_oneline_cached.is_some());
        let oneline = box_.params_oneline_cached.unwrap();
        assert!(oneline.contains("from"));
        assert!(oneline.contains("string"));
    }

    #[test]
    fn test_required_height_uses_cached_lines() {
        let mut box_ = InvokeBox::new("tool", "server").with_params(serde_json::json!({
            "line1": "value1",
            "line2": "value2",
            "line3": "value3"
        }));

        // Collapsed: base height
        let collapsed_height = box_.required_height();

        // Expanded: should use cached pretty JSON lines
        box_.expanded_params = true;
        let expanded_height = box_.required_height();

        // Expanded should be taller (adds lines from pretty JSON)
        assert!(expanded_height > collapsed_height);
    }

    #[test]
    fn test_invoke_box_with_pulse() {
        let box_ = InvokeBox::new("novanet_describe", "novanet").with_pulse_intensity(0.7);
        assert!((box_.pulse_intensity - 0.7).abs() < 0.001);
    }

    #[test]
    fn test_invoke_box_pulse_default_zero() {
        let box_ = InvokeBox::new("novanet_describe", "novanet");
        assert!((box_.pulse_intensity - 0.0).abs() < 0.001);
    }

    #[test]
    fn test_invoke_box_pulse_clamped() {
        let box_high = InvokeBox::new("tool", "server").with_pulse_intensity(1.5);
        assert!((box_high.pulse_intensity - 1.0).abs() < 0.001);

        let box_low = InvokeBox::new("tool", "server").with_pulse_intensity(-0.5);
        assert!((box_low.pulse_intensity - 0.0).abs() < 0.001);
    }

    // === Compact Mode Tests ===

    #[test]
    fn test_invoke_box_with_render_mode() {
        let box_ =
            InvokeBox::new("novanet_describe", "novanet").with_render_mode(RenderMode::Compact);
        assert_eq!(box_.render_mode, RenderMode::Compact);
    }

    #[test]
    fn test_invoke_box_compact_required_height() {
        let box_ =
            InvokeBox::new("novanet_describe", "novanet").with_render_mode(RenderMode::Compact);
        assert_eq!(box_.required_height(), 1);
    }

    // === BuiltinHint Tests ===

    #[test]
    fn test_builtin_hint_file_read() {
        assert_eq!(
            BuiltinHint::from_tool_name("nika:read"),
            BuiltinHint::FileRead
        );
        assert_eq!(
            BuiltinHint::from_tool_name("nika:glob"),
            BuiltinHint::FileRead
        );
        assert_eq!(
            BuiltinHint::from_tool_name("nika:grep"),
            BuiltinHint::FileRead
        );
    }

    #[test]
    fn test_builtin_hint_file_write() {
        assert_eq!(
            BuiltinHint::from_tool_name("nika:write"),
            BuiltinHint::FileWrite
        );
        assert_eq!(
            BuiltinHint::from_tool_name("nika:edit"),
            BuiltinHint::FileWrite
        );
    }

    #[test]
    fn test_builtin_hint_media_thumbnail() {
        assert_eq!(
            BuiltinHint::from_tool_name("nika:thumbnail"),
            BuiltinHint::MediaThumbnail
        );
        assert_eq!(
            BuiltinHint::from_tool_name("nika:convert"),
            BuiltinHint::MediaThumbnail
        );
        assert_eq!(
            BuiltinHint::from_tool_name("nika:strip"),
            BuiltinHint::MediaThumbnail
        );
        assert_eq!(
            BuiltinHint::from_tool_name("nika:metadata"),
            BuiltinHint::MediaThumbnail
        );
        assert_eq!(
            BuiltinHint::from_tool_name("nika:optimize"),
            BuiltinHint::MediaThumbnail
        );
        assert_eq!(
            BuiltinHint::from_tool_name("nika:svg_render"),
            BuiltinHint::MediaThumbnail
        );
    }

    #[test]
    fn test_builtin_hint_media_pipeline() {
        assert_eq!(
            BuiltinHint::from_tool_name("nika:pipeline"),
            BuiltinHint::MediaPipeline
        );
    }

    #[test]
    fn test_builtin_hint_import() {
        assert_eq!(
            BuiltinHint::from_tool_name("nika:import"),
            BuiltinHint::Import
        );
    }

    #[test]
    fn test_builtin_hint_assert() {
        assert_eq!(
            BuiltinHint::from_tool_name("nika:assert"),
            BuiltinHint::Assert
        );
    }

    #[test]
    fn test_builtin_hint_complete() {
        assert_eq!(
            BuiltinHint::from_tool_name("nika:complete"),
            BuiltinHint::Complete
        );
    }

    #[test]
    fn test_builtin_hint_sleep() {
        assert_eq!(
            BuiltinHint::from_tool_name("nika:sleep"),
            BuiltinHint::Sleep
        );
    }

    #[test]
    fn test_builtin_hint_generic_fallback() {
        assert_eq!(
            BuiltinHint::from_tool_name("novanet_describe"),
            BuiltinHint::Generic
        );
        assert_eq!(
            BuiltinHint::from_tool_name("some_external_tool"),
            BuiltinHint::Generic
        );
        assert_eq!(
            BuiltinHint::from_tool_name("nika:unknown"),
            BuiltinHint::Generic
        );
    }

    #[test]
    fn test_builtin_hint_is_builtin() {
        assert!(!BuiltinHint::Generic.is_builtin());
        assert!(BuiltinHint::FileRead.is_builtin());
        assert!(BuiltinHint::FileWrite.is_builtin());
        assert!(BuiltinHint::MediaThumbnail.is_builtin());
        assert!(BuiltinHint::MediaPipeline.is_builtin());
        assert!(BuiltinHint::Import.is_builtin());
        assert!(BuiltinHint::Assert.is_builtin());
        assert!(BuiltinHint::Complete.is_builtin());
        assert!(BuiltinHint::Sleep.is_builtin());
    }

    #[test]
    fn test_builtin_hint_default() {
        assert_eq!(BuiltinHint::default(), BuiltinHint::Generic);
    }

    #[test]
    fn test_invoke_box_sets_builtin_hint() {
        let read_box = InvokeBox::new("nika:read", "builtin");
        assert_eq!(read_box.builtin_hint, BuiltinHint::FileRead);

        let external_box = InvokeBox::new("novanet_describe", "novanet");
        assert_eq!(external_box.builtin_hint, BuiltinHint::Generic);

        let thumb_box = InvokeBox::new("nika:thumbnail", "builtin");
        assert_eq!(thumb_box.builtin_hint, BuiltinHint::MediaThumbnail);
    }
}
