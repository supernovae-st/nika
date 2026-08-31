// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

#![allow(clippy::unwrap_used, clippy::expect_used)]

use serde_json::Value;

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
