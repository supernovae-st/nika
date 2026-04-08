// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Infer Stream Box — Data types only
//!
//! Widget rendering code removed. Data types (`InferStatus`, `InferStreamData`)
//! are used by `chat/types.rs` and `chat/tests.rs`.

use std::time::Duration;

use ratatui::style::Color;

use crate::icons;
use crate::tokens::compat;

// ═══════════════════════════════════════════════════════════════════════════
// DEFAULT COLORS (kept for InferStatus::indicator)
// ═══════════════════════════════════════════════════════════════════════════

const DEFAULT_RUNNING_COLOR: Color = compat::AMBER_500; // running/active
const DEFAULT_SUCCESS_COLOR: Color = compat::GREEN_500; // success
const DEFAULT_FAILED_COLOR: Color = compat::RED_500; // failed

/// Status of inference
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum InferStatus {
    #[default]
    Running,
    Complete,
    Failed,
}

impl InferStatus {
    pub fn indicator(&self, frame: u8) -> (&'static str, Color) {
        match self {
            Self::Running => {
                let spinners = ["⣾", "⣽", "⣻", "⢿", "⡿", "⣟", "⣯", "⣷"];
                let idx = (frame as usize) % spinners.len();
                (spinners[idx], DEFAULT_RUNNING_COLOR)
            }
            Self::Complete => (icons::status::SUCCESS, DEFAULT_SUCCESS_COLOR),
            Self::Failed => (icons::status::FAILED, DEFAULT_FAILED_COLOR),
        }
    }
}

/// Data for rendering an inference stream box
#[derive(Debug, Clone, Default)]
pub struct InferStreamData {
    /// Model name
    pub model: String,
    /// Tokens input
    pub tokens_in: u64,
    /// Tokens output (so far)
    pub tokens_out: u64,
    /// Max tokens
    pub max_tokens: u64,
    /// Temperature
    pub temperature: f32,
    /// Duration
    pub duration: Duration,
    /// Status
    pub status: InferStatus,
    /// Streaming content
    pub content: String,
    /// Animation frame
    pub frame: u8,
}

impl InferStreamData {
    pub fn new(model: impl Into<String>) -> Self {
        Self {
            model: model.into(),
            tokens_in: 0,
            tokens_out: 0,
            max_tokens: 2000,
            temperature: 0.7,
            duration: Duration::ZERO,
            status: InferStatus::Running,
            content: String::new(),
            frame: 0,
        }
    }

    pub fn with_tokens(mut self, input: u64, output: u64) -> Self {
        self.tokens_in = input;
        self.tokens_out = output;
        self
    }

    pub fn with_max_tokens(mut self, max: u64) -> Self {
        self.max_tokens = max;
        self
    }

    pub fn with_content(mut self, content: impl Into<String>) -> Self {
        self.content = content.into();
        self
    }

    pub fn with_duration(mut self, duration: Duration) -> Self {
        self.duration = duration;
        self
    }

    pub fn with_status(mut self, status: InferStatus) -> Self {
        self.status = status;
        self
    }

    pub fn with_frame(mut self, frame: u8) -> Self {
        self.frame = frame;
        self
    }

    /// Append content during streaming
    pub fn append_content(&mut self, text: &str) {
        self.content.push_str(text);
    }

    /// Update tokens during streaming
    pub fn update_tokens(&mut self, output: u64) {
        self.tokens_out = output;
    }

    /// Mark as complete
    pub fn complete(&mut self) {
        self.status = InferStatus::Complete;
    }

    /// Mark as failed
    pub fn fail(&mut self) {
        self.status = InferStatus::Failed;
    }

    /// Tick animation frame
    pub fn tick(&mut self) {
        self.frame = self.frame.wrapping_add(1);
    }

    /// Progress percentage
    pub fn progress_percent(&self) -> f64 {
        if self.max_tokens == 0 {
            0.0
        } else {
            (self.tokens_out as f64 / self.max_tokens as f64) * 100.0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_infer_stream_creation() {
        let data = InferStreamData::new("claude-sonnet-4")
            .with_tokens(100, 50)
            .with_max_tokens(2000);

        assert_eq!(data.model, "claude-sonnet-4");
        assert_eq!(data.tokens_in, 100);
        assert_eq!(data.tokens_out, 50);
        assert_eq!(data.max_tokens, 2000);
    }

    #[test]
    fn test_progress_percent() {
        let data = InferStreamData::new("model")
            .with_tokens(0, 500)
            .with_max_tokens(2000);
        assert_eq!(data.progress_percent(), 25.0);

        let data = InferStreamData::new("model").with_max_tokens(0);
        assert_eq!(data.progress_percent(), 0.0);
    }

    #[test]
    fn test_content_operations() {
        let mut data = InferStreamData::new("model").with_content("Hello");
        assert_eq!(data.content, "Hello");

        data.append_content(" World");
        assert_eq!(data.content, "Hello World");
    }

    #[test]
    fn test_status_transitions() {
        let mut data = InferStreamData::new("model");
        assert_eq!(data.status, InferStatus::Running);

        data.complete();
        assert_eq!(data.status, InferStatus::Complete);

        let mut data2 = InferStreamData::new("model");
        data2.fail();
        assert_eq!(data2.status, InferStatus::Failed);
    }

    #[test]
    fn test_tick() {
        let mut data = InferStreamData::new("model");
        assert_eq!(data.frame, 0);
        data.tick();
        assert_eq!(data.frame, 1);
    }

    #[test]
    fn test_status_indicators() {
        let (char, color) = InferStatus::Complete.indicator(0);
        assert_eq!(char, icons::status::SUCCESS);
        assert_eq!(color, DEFAULT_SUCCESS_COLOR);

        let (char, color) = InferStatus::Failed.indicator(0);
        assert_eq!(char, icons::status::FAILED);
        assert_eq!(color, DEFAULT_FAILED_COLOR);

        // Running shows spinner
        let (char1, _) = InferStatus::Running.indicator(0);
        let (char2, _) = InferStatus::Running.indicator(1);
        assert_ne!(char1, char2);
    }

    #[test]
    fn test_update_tokens() {
        let mut data = InferStreamData::new("model");
        data.update_tokens(100);
        assert_eq!(data.tokens_out, 100);
    }
}
