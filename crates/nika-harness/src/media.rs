// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Bounded terminal image observations. No filesystem or network access lives here.
use std::collections::{BTreeMap, BTreeSet};

use base64::{Engine as _, engine::general_purpose::STANDARD};
use nika_kernel::ai::harness::{HarnessError, HarnessImage};
use serde_json::Value;
use sha2::{Digest as _, Sha256};

// The adapter duplicates base64 in content and rawOutput; its 18 MiB wire bound
// leaves room for both copies of this 8 MiB total. Receipts carry metadata only.
const MAX_BASE64_TOTAL: usize = 8 * 1024 * 1024;
const MAX_IMAGES: usize = 4;
const MAX_TRACKED: usize = 16;
const MAX_PATH: usize = 4096;

#[derive(Default)]
pub(crate) struct MediaState {
    tracked: BTreeSet<String>,
    images: BTreeMap<String, HarnessImage>,
    encoded: usize,
}
impl MediaState {
    pub(crate) fn starting(&self, update: &Value) -> Option<String> {
        let id = update["toolCallId"].as_str()?;
        (update["sessionUpdate"] == "tool_call"
            && update["title"] == "Image generation"
            && !id.is_empty()
            && id.len() <= 256
            && !self.tracked.contains(id))
        .then(|| id.to_owned())
    }
    pub(crate) fn images(&self) -> Vec<HarnessImage> {
        self.images.values().cloned().collect()
    }
    /// A possibly effectful image operation must not be automatically replayed after wire loss.
    pub(crate) fn no_replay(&self, error: HarnessError) -> HarnessError {
        if self.tracked.is_empty() {
            return error;
        }
        refusal(format!(
            "{error}; image activity was observed, no automatic replay; inspect the receipt"
        ))
    }
    pub(crate) fn check_stop(&self, stop: &str) -> Result<(), HarnessError> {
        if !self.tracked.is_empty()
            && (stop != "end_turn" || self.images.len() != self.tracked.len())
        {
            return Err(refusal(format!(
                "image turn ended as `{stop}` without a complete result for every observed image operation"
            )));
        }
        Ok(())
    }
    pub(crate) fn observe(&mut self, update: &Value) -> Result<Option<HarnessImage>, HarnessError> {
        let kind = update["sessionUpdate"].as_str().unwrap_or_default();
        if !matches!(kind, "tool_call" | "tool_call_update") {
            return Ok(None);
        }
        let Some(id) = update["toolCallId"]
            .as_str()
            .filter(|s| !s.is_empty() && s.len() <= 256)
        else {
            return Ok(None);
        };
        // Codex publishes this exact tool title at start (and in self-contained replay updates).
        // A random image in another tool's response never becomes generated-image evidence.
        if kind == "tool_call" && update["title"] == "Image generation" {
            if self.tracked.len() >= MAX_TRACKED && !self.tracked.contains(id) {
                return Err(refusal("too many image operations in one harness turn"));
            }
            self.tracked.insert(id.to_owned());
        }
        if !self.tracked.contains(id) {
            return Ok(None);
        }
        match update["status"].as_str() {
            Some("failed") => return Err(refusal("the harness reported image generation failed")),
            Some("completed") => {}
            _ => return Ok(None),
        }
        let raw = &update["rawOutput"];
        if raw["status"] != "completed" {
            return Err(refusal(
                "image completion is unconfirmed by the harness result",
            ));
        }
        let image = image_of(id, raw, update.get("content"))?;
        if let Some(previous) = self.images.get(id) {
            return if previous == &image {
                Ok(None)
            } else {
                Err(refusal("conflicting image result for the same tool call"))
            };
        }
        let bytes = raw["result"].as_str().map_or(0, str::len);
        if self.images.len() >= MAX_IMAGES || bytes > MAX_BASE64_TOTAL.saturating_sub(self.encoded)
        {
            return Err(refusal(
                "image receipt exceeds four images or 8 MiB of base64; nothing was truncated",
            ));
        }
        self.encoded += bytes;
        self.images.insert(id.to_owned(), image.clone());
        Ok(Some(image))
    }
}
fn refusal(reason: impl Into<String>) -> HarnessError {
    HarnessError::Refused {
        reason: reason.into(),
    }
}
fn image_of(id: &str, raw: &Value, content: Option<&Value>) -> Result<HarnessImage, HarnessError> {
    let mut image = HarnessImage::new(id);
    image.reported_saved_path = match raw.get("savedPath").filter(|v| !v.is_null()) {
        None => None,
        Some(value) => Some(
            value
                .as_str()
                .filter(|p| {
                    !p.is_empty() && p.len() <= MAX_PATH && !p.chars().any(char::is_control)
                })
                .ok_or_else(|| refusal("invalid reported image path; no file was opened"))?
                .to_owned(),
        ),
    };
    let data = match raw.get("result") {
        None => "",
        Some(Value::String(data)) => data.as_str(),
        Some(_) => return Err(refusal("image result payload is not a string")),
    };
    if data.is_empty() {
        if image.reported_saved_path.is_none() {
            return Err(refusal(
                "image result contains neither bytes nor a reported path",
            ));
        }
        if content
            .and_then(Value::as_array)
            .is_some_and(|blocks| blocks.iter().any(|b| b["content"]["type"] == "image"))
        {
            return Err(refusal(
                "path-only result contradicts its attached image content",
            ));
        }
        return Ok(image);
    }
    if data.len() > MAX_BASE64_TOTAL {
        return Err(refusal(
            "image exceeds the 8 MiB base64 receipt bound; nothing was truncated",
        ));
    }
    let blocks = content
        .and_then(Value::as_array)
        .ok_or_else(|| refusal("image result has no matching ACP image content"))?;
    let mut payloads = blocks
        .iter()
        .filter_map(|b| b.get("content"))
        .filter(|b| b["type"] == "image");
    let block = payloads
        .next()
        .ok_or_else(|| refusal("image result has no matching ACP image content"))?;
    if payloads.next().is_some() || block["mimeType"] != "image/png" || block["data"] != data {
        return Err(refusal(
            "image payload/MIME disagrees with its reported native PNG result",
        ));
    }
    if block
        .get("uri")
        .is_some_and(|uri| uri.as_str() != image.reported_saved_path.as_deref())
    {
        return Err(refusal(
            "image content URI contradicts the reported saved path",
        ));
    }
    let decoded = STANDARD
        .decode(data)
        .map_err(|_| refusal("image data is not valid base64"))?;
    // Header evidence only: never claim that this is a fully decoded raster or a verified file.
    if decoded.len() < 24
        || !decoded.starts_with(b"\x89PNG\r\n\x1a\n")
        || decoded[12..16] != *b"IHDR"
        || decoded[16..20] == [0; 4]
        || decoded[20..24] == [0; 4]
    {
        return Err(refusal(
            "image bytes do not contain a PNG header with nonzero dimensions",
        ));
    }
    image.mime_type = Some("image/png".into());
    image.sha256 = Some(format!("{:x}", Sha256::digest(&decoded)));
    image.received_bytes = Some(decoded.len() as u64);
    image.data = Some(decoded.into());
    Ok(image)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    const PNG: &str = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAusB9Wl6lmYAAAAASUVORK5CYII=";
    fn complete() -> Value {
        json!({"sessionUpdate":"tool_call","toolCallId":"im-1","title":"Image generation","status":"completed",
            "rawOutput":{"status":"completed","result":PNG,"savedPath":"/outside/project/result.png"},
            "content":[{"type":"content","content":{"type":"image","mimeType":"image/png","data":PNG}}]})
    }
    #[test]
    fn retains_bytes_and_reported_path_without_opening_it() {
        let mut state = MediaState::default();
        let image = state.observe(&complete()).unwrap().unwrap();
        assert_eq!(
            image.data.as_deref(),
            Some(STANDARD.decode(PNG).unwrap().as_slice())
        );
        assert_eq!(
            image.reported_saved_path.as_deref(),
            Some("/outside/project/result.png")
        );
        assert_eq!(image.observation()["file_verified"], false);
        assert!(image.sha256.is_some());
        assert!(state.observe(&complete()).unwrap().is_none());
    }
    #[test]
    fn unrelated_stale_failed_conflicting_and_malformed_results_never_become_images() {
        let mut foreign = complete();
        foreign["title"] = json!("Read image");
        assert!(MediaState::default().observe(&foreign).unwrap().is_none());
        for (field, value) in [
            ("status", json!("failed")),
            ("rawOutput", json!({"status":"incomplete","result":PNG})),
        ] {
            let mut bad = complete();
            bad[field] = value;
            assert!(MediaState::default().observe(&bad).is_err());
        }
        let mut bad = complete();
        bad["content"][0]["content"]["data"] = json!("different");
        assert!(MediaState::default().observe(&bad).is_err());
        let mut bad = complete();
        bad["rawOutput"]["result"] = json!("PHN2Zy8+");
        bad["content"][0]["content"]["data"] = json!("PHN2Zy8+");
        assert!(
            MediaState::default().observe(&bad).is_err(),
            "SVG never qualifies as a PNG"
        );
        let mut state = MediaState::default();
        state.observe(&complete()).unwrap();
        let mut different = complete();
        different["rawOutput"]["savedPath"] = json!("changed.png");
        assert!(state.observe(&different).is_err());
        assert!(state.check_stop("cancelled").is_err());
        assert!(
            !state
                .no_replay(HarnessError::Session {
                    reason: "EOF".into()
                })
                .is_transient()
        );
    }
    #[test]
    fn bounded_path_only_reports_stay_unverified_and_have_no_fabricated_bytes() {
        let mut value = complete();
        value["rawOutput"]["result"] = json!("");
        value["content"] = json!([]);
        let image = MediaState::default().observe(&value).unwrap().unwrap();
        assert!(image.data.is_none());
        assert!(image.sha256.is_none());
        assert!(image.mime_type.is_none());
        value["rawOutput"]["savedPath"] = json!("x".repeat(MAX_PATH + 1));
        assert!(MediaState::default().observe(&value).is_err());
        let mut large = complete();
        large["rawOutput"]["result"] = json!("A".repeat(MAX_BASE64_TOTAL + 1));
        assert!(MediaState::default().observe(&large).is_err());
    }
    #[test]
    fn base64_budget_is_inclusive_aggregate_and_never_decodes_invalid_data() {
        let mut bytes = vec![0; MAX_BASE64_TOTAL / 4 * 3];
        let header = STANDARD.decode(PNG).unwrap();
        bytes[..header.len()].copy_from_slice(&header);
        let data = STANDARD.encode(&bytes);
        assert_eq!(data.len(), MAX_BASE64_TOTAL);
        let mut limit = complete();
        limit["rawOutput"]["result"] = json!(data);
        limit["content"][0]["content"]["data"] = limit["rawOutput"]["result"].clone();
        let mut state = MediaState::default();
        let image = state.observe(&limit).unwrap().unwrap();
        assert_eq!(image.received_bytes, Some(bytes.len() as u64));
        assert!(
            image.observation().to_string().len() < 1024,
            "receipt never inlines bytes"
        );
        let mut extra = complete();
        extra["toolCallId"] = json!("im-2");
        assert!(
            state
                .observe(&extra)
                .unwrap_err()
                .to_string()
                .contains("8 MiB")
        );
        let mut bad = complete();
        bad["rawOutput"]["result"] = json!("%%%%");
        bad["content"][0]["content"]["data"] = json!("%%%%");
        assert!(
            MediaState::default()
                .observe(&bad)
                .unwrap_err()
                .to_string()
                .contains("base64")
        );
    }
}
