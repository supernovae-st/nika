// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Telemetry and miscellaneous event handlers
//!
//! Log, Custom, ArtifactWritten/Failed, StructuredOutput*,
//! Guardrail*, Http*, Media*, Vision, and the no-op fallthrough arms.

use std::sync::Arc;

use nika_engine::event::{GuardrailType, Severity};

use super::TuiState;
use crate::state::notification::Notification;

impl TuiState {
    pub(super) fn on_log(
        &mut self,
        level: &str,
        message: &str,
        task_id: &Option<Arc<str>>,
        timestamp_ms: u64,
    ) {
        let prefix = match level {
            "error" => "[ERR]",
            "warn" => "[WRN]",
            "info" => "[INF]",
            "debug" => "[DBG]",
            "trace" => "[TRC]",
            _ => "[LOG]",
        };
        let task_suffix = task_id
            .as_ref()
            .map(|t| format!(" [{}]", t))
            .unwrap_or_default();
        self.add_notification(Notification::info(
            format!("{} {}{}", prefix, message, task_suffix),
            timestamp_ms,
        ));
        self.dirty.notifications = true;
    }

    pub(super) fn on_custom(
        &mut self,
        name: &str,
        payload: &serde_json::Value,
        task_id: &Option<Arc<str>>,
        timestamp_ms: u64,
    ) {
        let task_suffix = task_id
            .as_ref()
            .map(|t| format!(" [{}]", t))
            .unwrap_or_default();
        // Compact payload display (first 50 chars, UTF-8 safe)
        let payload_preview = if payload.is_null() {
            String::new()
        } else {
            let s = payload.to_string();
            if s.chars().count() > 50 {
                let truncated: String = s.chars().take(47).collect();
                format!(": {}...", truncated)
            } else {
                format!(": {}", s)
            }
        };
        self.add_notification(Notification::info(
            format!("[EVT] {}{}{}", name, payload_preview, task_suffix),
            timestamp_ms,
        ));
        self.dirty.notifications = true;
    }

    pub(super) fn on_artifact_written(
        &mut self,
        task_id: &str,
        path: &str,
        size: u64,
        timestamp_ms: u64,
    ) {
        let size_str = if size < 1024 {
            format!("{} B", size)
        } else if size < 1024 * 1024 {
            format!("{:.1} KB", size as f64 / 1024.0)
        } else {
            format!("{:.1} MB", size as f64 / (1024.0 * 1024.0))
        };
        self.add_notification(Notification::success(
            format!("[{}] Artifact written: {} ({})", task_id, path, size_str),
            timestamp_ms,
        ));
        self.dirty.notifications = true;
    }

    pub(super) fn on_artifact_failed(
        &mut self,
        task_id: &str,
        path: &str,
        reason: &str,
        timestamp_ms: u64,
    ) {
        self.add_notification(Notification::error(
            format!("[{}] Artifact failed: {} - {}", task_id, path, reason),
            timestamp_ms,
        ));
        self.dirty.notifications = true;
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn on_structured_output_attempt(
        &mut self,
        task_id: &str,
        layer: u8,
        layer_name: &str,
        attempt: u32,
        success: bool,
        error: &Option<String>,
        timestamp_ms: u64,
    ) {
        if success {
            self.add_notification(Notification::info(
                format!(
                    "[{}] Layer {} ({}) attempt {} succeeded",
                    task_id, layer, layer_name, attempt
                ),
                timestamp_ms,
            ));
        } else if let Some(err) = error {
            self.add_notification(Notification::warning(
                format!(
                    "[{}] Layer {} ({}) attempt {} failed: {}",
                    task_id, layer, layer_name, attempt, err
                ),
                timestamp_ms,
            ));
        }
        self.dirty.notifications = true;
    }

    pub(super) fn on_structured_output_success(
        &mut self,
        task_id: &str,
        layer: u8,
        layer_name: &str,
        total_attempts: u32,
        timestamp_ms: u64,
    ) {
        self.add_notification(Notification::success(
            format!(
                "[{}] Structured output extracted via layer {} ({}) after {} attempt(s)",
                task_id, layer, layer_name, total_attempts
            ),
            timestamp_ms,
        ));
        self.dirty.notifications = true;
    }

    pub(super) fn on_guardrail_passed(
        &mut self,
        task_id: &str,
        guardrail_type: &GuardrailType,
        description: &str,
        timestamp_ms: u64,
    ) {
        self.add_notification(Notification::success(
            format!(
                "[{}] Guardrail {} passed: {}",
                task_id, guardrail_type, description
            ),
            timestamp_ms,
        ));
        self.dirty.notifications = true;
    }

    pub(super) fn on_guardrail_failed(
        &mut self,
        task_id: &str,
        guardrail_type: &GuardrailType,
        description: &str,
        message: &str,
        timestamp_ms: u64,
    ) {
        self.add_notification(Notification::error(
            format!(
                "[{}] Guardrail {} failed ({}): {}",
                task_id, guardrail_type, description, message
            ),
            timestamp_ms,
        ));
        self.dirty.notifications = true;
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn on_guardrail_escalation(
        &mut self,
        task_id: &str,
        guardrail_type: &GuardrailType,
        guardrail_id: &str,
        message: &str,
        severity: &Severity,
        suggested_action: &Option<String>,
        timestamp_ms: u64,
    ) {
        let action_text = suggested_action
            .as_ref()
            .map(|a| format!(" Action: {}", a))
            .unwrap_or_default();
        self.add_notification(Notification::error(
            format!(
                "[{}] ESCALATION ({}) {} [{}]: {}{}",
                task_id, severity, guardrail_type, guardrail_id, message, action_text
            ),
            timestamp_ms,
        ));
        self.dirty.notifications = true;
    }

    pub(super) fn on_http_request(
        &mut self,
        task_id: &str,
        method: &str,
        url: &str,
        timestamp_ms: u64,
    ) {
        self.add_notification(Notification::info(
            format!("[{}] HTTP {} {}", task_id, method, url),
            timestamp_ms,
        ));
        self.dirty.notifications = true;
    }

    pub(super) fn on_http_response(
        &mut self,
        task_id: &str,
        status_code: u16,
        elapsed_ms: u64,
        timestamp_ms: u64,
    ) {
        let status_label = if status_code < 400 { "OK" } else { "ERR" };
        self.add_notification(Notification::info(
            format!(
                "[{}] HTTP {} {} ({}ms)",
                task_id, status_code, status_label, elapsed_ms
            ),
            timestamp_ms,
        ));
        self.dirty.notifications = true;
    }

    pub(super) fn on_media_extracted(
        &mut self,
        task_id: &str,
        block_count: u32,
        timestamp_ms: u64,
    ) {
        self.add_notification(Notification::info(
            format!("[{}] Extracted {} media block(s)", task_id, block_count),
            timestamp_ms,
        ));
        self.dirty.notifications = true;
    }

    pub(super) fn on_media_store_failed(&mut self, task_id: &str, reason: &str, timestamp_ms: u64) {
        self.add_notification(Notification::error(
            format!("[{}] Media store failed: {}", task_id, reason),
            timestamp_ms,
        ));
        self.dirty.notifications = true;
    }

    pub(super) fn on_media_cleanup(
        &mut self,
        removed: u64,
        bytes_freed: u64,
        dry_run: bool,
        timestamp_ms: u64,
    ) {
        if !dry_run {
            self.add_notification(Notification::info(
                format!(
                    "Media cleanup: removed {} files, freed {} bytes",
                    removed, bytes_freed
                ),
                timestamp_ms,
            ));
            self.dirty.notifications = true;
        }
    }

    pub(super) fn on_media_integrity_check(
        &mut self,
        checked: u64,
        warnings: u64,
        timestamp_ms: u64,
    ) {
        if warnings > 0 {
            self.add_notification(Notification::warning(
                format!(
                    "Media integrity: {}/{} refs had warnings",
                    warnings, checked
                ),
                timestamp_ms,
            ));
            self.dirty.notifications = true;
        }
    }

    pub(super) fn on_vision_content_resolved(
        &mut self,
        task_id: &str,
        image_count: u32,
        total_bytes: u64,
        resolve_ms: u64,
        timestamp_ms: u64,
    ) {
        self.add_notification(Notification::info(
            format!(
                "Vision: {} image(s) resolved ({} bytes, {}ms) for task {}",
                image_count, total_bytes, resolve_ms, task_id
            ),
            timestamp_ms,
        ));
        self.dirty.notifications = true;
    }
}
