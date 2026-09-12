// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

#![allow(clippy::unwrap_used, clippy::expect_used)]

use serde_json::Value;

/// One deterministic filesystem mutation after the route's existence check.
pub(super) type CaptureAction = Box<dyn FnOnce() + Send>;
pub(super) type CaptureProbe = std::sync::Arc<std::sync::Mutex<Option<CaptureAction>>>;

pub(super) fn before_named_capture(probe: &CaptureProbe) {
    let action = probe.lock().expect("capture probe").take();
    if let Some(action) = action {
        action();
    }
}

pub(super) fn assert_allowlisted(event: &Value) {
    let object = event.as_object().expect("event object");
    assert!(
        object.contains_key("sequence")
            && object.contains_key("kind")
            && object.contains_key("status")
            && object.keys().all(|key| matches!(
                key.as_str(),
                "sequence" | "kind" | "status" | "code" | "message" | "outputs" | "receipt"
            )),
        "{event}"
    );
}
