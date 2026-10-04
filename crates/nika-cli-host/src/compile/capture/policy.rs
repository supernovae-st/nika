// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Trusted-host admission. Never serialized, logged or derived from model text.

use std::sync::Arc;

/// The host's decision about its context, beyond the known withheld values.
pub type TextAdmission = Arc<dyn Fn(&str) -> bool + Send + Sync>;

/// An explicit private-capture policy. Absence of this value means capture is off.
/// No Debug: even an escaped credential remains a credential.
#[derive(Clone)]
pub struct CapturePolicy {
    admitted: bool,
    needles: Vec<String>,
    context: TextAdmission,
}

impl CapturePolicy {
    /// Admit one host context and its known withheld values, under the same law as
    /// [`CapturePolicy::with_withheld`]: at most 128 values, each checked in order, and the first
    /// refusal ends the work. A failed bound refuses capture; it never reduces authoring work.
    #[must_use]
    pub fn admitted(values: Vec<String>, context: TextAdmission) -> Self {
        let mut policy = Self {
            admitted: values.len() <= 128,
            needles: Vec::new(),
            context,
        };
        for value in values {
            if !policy.admitted {
                break;
            }
            policy = policy.with_withheld(&value);
        }
        policy
    }

    /// Add a resolved host credential without making the policy or value printable.
    /// Excessive input disables capture, never the compiler's work: a value over 32 KiB raw, a
    /// 257th retained needle, or an aggregate over 512 KiB, counted as the bytes of every needle
    /// already kept (raw values and their JSON-escaped forms without quotes) plus this value's
    /// raw bytes and its JSON encoding with quotes. Each is refused before the value is kept.
    #[must_use]
    pub fn with_withheld(mut self, value: &str) -> Self {
        if !self.admitted || value.is_empty() {
            return self;
        }
        // Refuse raw size and the retained pair count before encoding or cloning.
        if value.len() > 32 * 1024 || self.needles.len().saturating_add(2) > 256 {
            self.admitted = false;
            return self;
        }
        let encoded = serde_json::to_string(value).ok();
        let size = self.needles.iter().map(String::len).sum::<usize>();
        match encoded {
            Some(encoded)
                if size
                    .saturating_add(value.len())
                    .saturating_add(encoded.len())
                    <= 512 * 1024 =>
            {
                // The encoded aggregate bound precedes both retained clones.
                self.needles.push(encoded[1..encoded.len() - 1].to_owned());
                self.needles.push(value.to_owned());
            }
            _ => self.admitted = false,
        }
        self
    }

    pub(super) fn admits(&self, text: &str) -> bool {
        self.admitted
            && self.needles.iter().all(|needle| !text.contains(needle))
            && (self.context)(text)
    }

    pub(super) fn ready(&self) -> bool {
        self.admitted
    }

    #[cfg(test)]
    pub(super) fn needle_count(&self) -> usize {
        self.needles.len()
    }

    /// A bounded selected metadata string, or null with an explicit withheld flag.
    // Both raw and escaped forms are bounded/admitted before record serialization. The temporary
    // encoder sees at most512 raw bytes (at most3074 including quotes), never a response body.
    pub(super) fn metadata(&self, text: Option<&str>) -> (Option<String>, bool) {
        let Some(text) = text else {
            return (None, false);
        };
        if text.len() > 512 || !self.admits(text) {
            return (None, true);
        }
        match serde_json::to_string(text) {
            Ok(encoded) if encoded.len() <= 514 && self.admits(&encoded[1..encoded.len() - 1]) => {
                (Some(text.to_owned()), false)
            }
            _ => (None, true),
        }
    }
}
