// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Vision-LLM traits — async multimodal frame analysis (describe · locate · detect).
//!
//! Phase 2 M1 PR 6 (sprint plan `2026-05-14-nika-phase-2-m1-kernel-
//! modules-sprint-plan.md` §4 · FINAL M1 PR · SEALS the L0.5 io+ai
//! kernel sealed-trait + DTO surface). Error codes NIKA-1500..1599 per
//! the ADR-081 `nika_codes` matrix (the sprint plan's draft 1040-block
//! was superseded — see `crate::errors`). The L1 impl lands via the
//! `nika-vision-local` crate per ADR-037 bottom-up plan (in-process
//! ONNX runtime + remote multimodal-LLM fallback through
//! `nika-verb-infer`).
//!
//! ADR-006 monolithic-kernel-spirit (single trait + DTOs · no proc
//! macro · no async runtime dep). ADR-016 ISP discipline (1 trait =
//! 1 capability · vision inference is 1 concern · L1 impls split per
//! backend · local ONNX · vision-LLM via provider catalog · CDP
//! browser-based via screenshot pipeline). ADR-037 bottom-up layer
//! (L0.5 traits-only · zero tokio · zero ort / onnxruntime / vision-
//! LLM crates · those land in L1 at Phase 2 M4 · `nika-vision-local`
//! ships the ONNX backend + cloud fallback per the 9-provider catalog).
//!
//! Cross-PR deps · vision.rs imports `Frame` from `io::screen` (M1.1 ·
//! canonical RGBA8 frame payload · zero-copy `bytes::Bytes`) AND
//! `TextRegion` from `io::ocr` (M1.2 · canonical OCR text region with
//! bbox + confidence + BCP-47 language tag). Keeping a single
//! canonical taxonomy across capture + OCR + vision prevents downstream
//! consumers from coercing between parallel geometry / text-region
//! types (cohérent `no-legacy-no-back-compat.md` Class 1 single-
//! canonical-enum discipline · sister to M1.2 OCR `Rect` re-use).
//!
//! `DetectedObject::attributes: BTreeMap<String, String>` per workspace
//! `clippy.toml` `disallowed-types` ban on `HashMap` (DEV-3 lesson ·
//! `BTreeMap` provides serde-determinism + ordered iteration · zero
//! extra dep · std-only · cohérent M1.5 `BrowserSession::cookies` +
//! M1.3 `AxNode::attributes` pattern).
//!
//! `BoundingBox` 5-f32 fields lack `Eq` per DEV-1 lesson (`f32` is not
//! `Eq` · partial-ord-only) · `PartialEq` sufficient for projection-
//! layer diff semantics + integration test assertions (cohérent M1.2
//! `TextRegion::confidence: f32` precedent).

use nika_kernel_core::io::ocr::TextRegion;
use nika_kernel_core::io::screen::Frame;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Bounding box in pixel coordinates with a model-confidence score.
///
/// All 5 fields are `f32` · cohérent vision-LLM upstream conventions
/// (Ultralytics YOLO · ONNX vision model outputs · CDP `getBoxModel`).
/// `confidence` is a probability score in `[0.0, 1.0]` · L1 impls
/// SHOULD clamp out-of-range upstream values before constructing this
/// DTO. Coordinates are physical-pixel relative to the source `Frame`
/// (top-left origin · cohérent `io::screen::Rect` + `io::ocr::TextRegion::bbox`).
///
/// `PartialEq` only (no `Eq`) per DEV-1 · `f32` lacks total equality.
/// Structural `==` is sufficient for the projection-layer diff
/// semantics + integration test assertions.
///
/// `#[non_exhaustive]` keeps the struct extensible per `no-legacy-no-
/// back-compat.md` (NUKE-LEGACY · breaking changes ship on MINOR
/// until forever-v0.x). Future fields land additively (e.g. `class_id`
/// · `track_id` for multi-frame tracking · `mask` for instance
/// segmentation).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct BoundingBox {
    /// X coordinate of the top-left corner · physical pixels.
    pub x: f32,
    /// Y coordinate of the top-left corner · physical pixels.
    pub y: f32,
    /// Width of the box · physical pixels.
    pub width: f32,
    /// Height of the box · physical pixels.
    pub height: f32,
    /// Model confidence score in `[0.0, 1.0]`.
    pub confidence: f32,
}

impl BoundingBox {
    /// Construct a new bounding-box record.
    ///
    /// Per Invariant #19 · every `#[non_exhaustive]` struct ships a
    /// `new()` constructor so downstream code never field-literals.
    #[must_use]
    pub fn new(x: f32, y: f32, width: f32, height: f32, confidence: f32) -> Self {
        Self {
            x,
            y,
            width,
            height,
            confidence,
        }
    }
}

/// Lossy conversion from a vision-LLM `BoundingBox` (`f32`) to a
/// screen-capture `Rect` (`i32 + u32`) · truncates fractional pixels and
/// guards non-finite / negative dimensions.
///
/// `x` · `y` truncate to `i32` (saturating cast · `as` semantics in
/// Rust 1.45+ saturate at `i32::MIN`/`MAX` for out-of-range `f32`).
/// `width` · `height` clamp to `u32` · NaN · Infinity · or `<= 0.0`
/// produce `0` (degenerate rect · L1 consumers MUST check for empty
/// before downstream capture / paint). Cohérent the canonical Class 1
/// single-geometry-type discipline · enables vision-LLM bbox results
/// to flow into screen-capture region requests without manual coercion
/// at every call site.
///
/// Lossy by design · vision-LLM outputs are sub-pixel · screen capture
/// is pixel-integer. Per Invariant #19 spirit · this conversion is
/// surfaced as `From<&BoundingBox>` not `From<BoundingBox>` so the
/// vision response retains ownership for downstream consumers.
impl From<&BoundingBox> for nika_kernel_core::io::screen::Rect {
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss,
        reason = "lossy by contract · vision-LLM bbox `f32` sub-pixel coordinates \
                  truncate to screen-capture `Rect` `i32 + u32` pixel-integer · \
                  documented on the impl docstring · negative / NaN / Infinity \
                  width/height guarded to 0 explicitly · x/y saturating-cast per \
                  Rust 1.45+ `as` semantics for out-of-range f32"
    )]
    fn from(bb: &BoundingBox) -> Self {
        let x = bb.x as i32;
        let y = bb.y as i32;
        let width = if bb.width.is_finite() && bb.width > 0.0 {
            bb.width as u32
        } else {
            0
        };
        let height = if bb.height.is_finite() && bb.height > 0.0 {
            bb.height as u32
        } else {
            0
        };
        // `Rect` is `#[non_exhaustive]` and lives in `nika-kernel-core`
        // post-split — cross-crate struct literals are forbidden; the
        // Invariant #19 constructor is the canonical path.
        Self::new(x, y, width, height)
    }
}

/// Detected object · label + bounding box + arbitrary attribute map.
///
/// `label` is the recognized class name (e.g. `"person"` · `"button"`
/// · `"text-input"`) · open-vocabulary L1 impls (e.g. CLIP-based
/// vision-LLM) ship arbitrary strings · closed-set ONNX classifiers
/// pick from a fixed taxonomy. `bbox` is the localization in pixel
/// coordinates relative to the source `Frame`. `attributes` carries
/// model-specific or backend-specific metadata (e.g. `"color"` →
/// `"blue"` · `"text"` → `"Submit"` for OCR-augmented detection ·
/// `"interactable"` → `"true"` for UI-aware vision · `"class_id"` →
/// `"42"` for closed-set classifiers).
///
/// `attributes: BTreeMap<String, String>` per DEV-3 lesson · workspace
/// `clippy.toml` bans `HashMap` for serde-determinism + ordered
/// iteration (cohérent M1.3 `AxNode::attributes` + M1.5
/// `BrowserSession` cookie-map precedent · zero extra dep · std-only).
///
/// `Default` impl produces `attributes: BTreeMap::new()` for ergonomic
/// spread-update at construction sites that don't carry extra metadata.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct DetectedObject {
    /// Recognized class label · open or closed vocabulary per L1 impl.
    pub label: String,
    /// Localization in physical-pixel coordinates relative to the source frame.
    pub bbox: BoundingBox,
    /// Backend-specific attribute map · ordered + serde-deterministic.
    pub attributes: BTreeMap<String, String>,
}

impl DetectedObject {
    /// Construct a new detected-object record.
    ///
    /// Per Invariant #19 · every `#[non_exhaustive]` struct ships a
    /// `new()` constructor so downstream code never field-literals.
    #[must_use]
    pub fn new(label: String, bbox: BoundingBox, attributes: BTreeMap<String, String>) -> Self {
        Self {
            label,
            bbox,
            attributes,
        }
    }
}

/// Vision-LLM response · holistic frame analysis.
///
/// `description` is an optional natural-language summary of the frame
/// (e.g. `"A screenshot of a code editor with a green diff in the
/// active pane and a sidebar listing 3 modified files"`) · `None`
/// when the L1 impl only performs object detection / localization and
/// skips the description pass (cheaper · faster · zero-LLM in the
/// hot path). `objects` is the list of detected entities · empty when
/// no objects pass the model confidence threshold. `text_regions`
/// re-uses the M1.2 OCR `TextRegion` DTO when the L1 impl runs OCR
/// alongside vision (e.g. vision-LLM with grounded text · ONNX model
/// with co-trained OCR head) · empty when OCR is not part of the
/// pass.
///
/// All three fields are independent · L1 impls populate the subset
/// relevant to the calling verb (`describe` → `description` populated ·
/// `detect` → `objects` populated · `locate` returns `Vec<BoundingBox>`
/// directly via the trait so doesn't surface here · OCR-augmented
/// passes populate `text_regions` alongside).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct VisionResponse {
    /// Optional natural-language description of the frame.
    pub description: Option<String>,
    /// Detected objects · empty when no objects pass confidence threshold.
    pub objects: Vec<DetectedObject>,
    /// Text regions extracted alongside (when L1 impl runs co-OCR).
    pub text_regions: Vec<TextRegion>,
}

impl VisionResponse {
    /// Construct a new vision-response record.
    ///
    /// Per Invariant #19 · every `#[non_exhaustive]` struct ships a
    /// `new()` constructor so downstream code never field-literals.
    #[must_use]
    pub fn new(
        description: Option<String>,
        objects: Vec<DetectedObject>,
        text_regions: Vec<TextRegion>,
    ) -> Self {
        Self {
            description,
            objects,
            text_regions,
        }
    }
}

/// Vision-LLM trait · async multimodal frame analysis.
///
/// CANCEL SAFETY: every method is cancel-safe · vision-LLM ops are
/// read-only inference over a captured `Frame` and produce no side
/// effects. L1 impls SHOULD wrap synchronous inference (local ONNX
/// runtime · `ort` / `tract` / `candle`) in a `spawn_blocking` +
/// cancel-token shim · async-native impls (vision-LLM via
/// `nika-verb-infer` 9-provider catalog) propagate cancel through
/// the request future. Dropping the future abandons the inference
/// without partial state · partial detections / descriptions MUST
/// NOT leak to the caller.
///
/// Vision-inference errors — the typed boundary of the `VisionModel`
/// trait (Pattern A · FCI-023bis). `#[non_exhaustive]`; the `NikaErrorCode`
/// impl + the reserved range (NIKA-1500..1599 · ADR-081 computer-use block)
/// live in `crate::errors`.
///
/// SECURITY (ADR-081 Guard-4 posture): a `VisionError` MUST NEVER carry
/// frame pixels or prompt content — vision inputs can contain on-screen
/// secrets, and prompts can embed page-derived text. Variants carry only
/// structural detail (model ids · reason strings · never analyzed content).
#[derive(Debug, thiserror::Error, miette::Diagnostic)]
#[non_exhaustive]
pub enum VisionError {
    /// The requested vision model is not loaded / not found on this host.
    #[error("NIKA-1501 · vision model unavailable: {model}")]
    ModelUnavailable {
        /// Model identifier (a card/file id · never content).
        model: String,
    },
    /// The input frame failed validation (dimensions · pixel-buffer size).
    #[error("NIKA-1502 · vision input invalid: {reason}")]
    InvalidInput {
        /// Structural reason (sizes/dimensions · NEVER pixel or prompt content).
        reason: String,
    },
    /// The inference run itself failed (runtime/backend fault).
    #[error("NIKA-1503 · vision inference failed: {reason}")]
    InferenceFailed {
        /// Backend-reported reason (sanitized · never analyzed content).
        reason: String,
    },
    /// No vision backend is compiled/available on this platform.
    #[error("NIKA-1504 · no vision backend on this platform")]
    BackendUnavailable,
    /// A `spawn_blocking` inference task panicked or was cancelled.
    #[error("NIKA-1505 · vision task join failed: {reason}")]
    TaskJoinFailed {
        /// Join failure detail.
        reason: String,
    },
}

/// `#[trait_variant::make(VisionModelDyn: Send)]` generates a `Send`
/// companion trait `VisionModelDyn` for generic constraints (cohérent
/// `io::screen::ScreenCaptureDyn` · `io::ocr::OcrEngineDyn` · `io::a11y::
/// AccessibilityTreeDyn` · `io::browser::BrowserAutomationDyn` pattern
/// · ADR-006 + `io::fs::FsRead` canonical shape). Downstream code uses
/// `T: VisionModelDyn` for monomorphized hot paths. Per `trait_variant`
/// 0.1 semantics · the companion uses `impl Future + Send` returns
/// which are NOT dyn-compatible · this is intentional (kernel traits
/// stay zero-cost · L1 impls wrap via `Arc<T>` not `Arc<dyn _>`).
#[trait_variant::make(VisionModelDyn: Send)]
pub trait VisionModel: Send + Sync {
    /// Describe a frame · LLM-powered holistic analysis.
    ///
    /// Returns a `VisionResponse` with `description` populated (natural-
    /// language summary) + optionally `objects` / `text_regions` when
    /// the L1 impl runs grounded detection alongside. `prompt` steers
    /// the description (e.g. `"What is the user doing in this
    /// screenshot?"` · `"List all interactive elements"` · `""` for
    /// model default).
    ///
    /// CANCEL SAFETY: cancel-safe (read-only inference · no side
    /// effects · L1 impls wrap sync runtimes in `spawn_blocking`).
    async fn describe(&self, frame: &Frame, prompt: &str) -> Result<VisionResponse, VisionError>;

    /// Locate target object or text in a frame · returns bounding boxes.
    ///
    /// Returns a `Vec<BoundingBox>` for every region matching the
    /// `target` string. `target` may be an object class name (e.g.
    /// `"button"` · `"text-input"`) · a free-text query for open-
    /// vocabulary L1 impls (e.g. vision-LLM with grounded localization
    /// · OWL-ViT) · or a specific text snippet for OCR-augmented
    /// localization. Empty `Vec` means no matches above the confidence
    /// threshold.
    ///
    /// CANCEL SAFETY: cancel-safe (read-only inference · no side
    /// effects · partial localization results MUST NOT leak on cancel).
    async fn locate(&self, frame: &Frame, target: &str) -> Result<Vec<BoundingBox>, VisionError>;

    /// Detect all objects in a frame · open-set or closed-set classification.
    ///
    /// Returns a `Vec<DetectedObject>` for every detected entity above
    /// the model confidence threshold. Open-vocabulary L1 impls (e.g.
    /// CLIP-grounded vision-LLM) ship arbitrary `label` strings ·
    /// closed-set classifiers (ONNX YOLO · DETR) pick from a fixed
    /// taxonomy declared in the model card. Empty `Vec` means no
    /// objects detected.
    ///
    /// CANCEL SAFETY: cancel-safe (read-only inference · no side
    /// effects · partial detection results MUST NOT leak on cancel).
    async fn detect(&self, frame: &Frame) -> Result<Vec<DetectedObject>, VisionError>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;
    use nika_kernel_core::io::screen::{DisplayId, Rect};

    fn synthetic_frame() -> Frame {
        Frame::new(
            1920,
            1080,
            1.0,
            Bytes::from(vec![0u8; 16]),
            DisplayId::new(0),
            1_700_000_000_000,
        )
    }

    fn synthetic_text_region() -> TextRegion {
        TextRegion::new(
            "Submit".to_string(),
            Rect::new(100, 200, 80, 32),
            0.92,
            Some("en".to_string()),
        )
    }

    #[test]
    fn bounding_box_serde_roundtrip() {
        let bb = BoundingBox {
            x: 100.0,
            y: 200.0,
            width: 80.0,
            height: 32.0,
            confidence: 0.95,
        };
        let json = serde_json::to_string(&bb).expect("serialize bbox");
        let back: BoundingBox = serde_json::from_str(&json).expect("deserialize bbox");
        assert_eq!(back, bb);
    }

    #[test]
    fn bounding_box_new_constructor_matches_struct_literal() {
        let from_new = BoundingBox::new(10.0, 20.0, 30.0, 40.0, 0.5);
        let from_lit = BoundingBox {
            x: 10.0,
            y: 20.0,
            width: 30.0,
            height: 40.0,
            confidence: 0.5,
        };
        assert_eq!(from_new, from_lit);
    }

    /// `BoundingBox` → `Rect` lossy conversion truncates fractional
    /// pixels · guards NaN / Infinity / negative dimensions to `0` ·
    /// preserves the canonical happy path (positive finite f32 maps
    /// 1-to-1 to truncated integer pixels). Cohérent the doc-comment
    /// contract on the `From<&BoundingBox>` impl.
    #[test]
    fn bounding_box_to_rect_truncates_correctly() {
        // Happy path · positive finite f32 truncates cleanly.
        let bb = BoundingBox::new(100.5, 200.7, 80.3, 32.9, 0.95);
        let rect: Rect = (&bb).into();
        assert_eq!(rect.x, 100);
        assert_eq!(rect.y, 200);
        assert_eq!(rect.width, 80);
        assert_eq!(rect.height, 32);

        // Negative width / height clamp to 0 (degenerate rect).
        let bb_neg = BoundingBox::new(10.0, 20.0, -5.0, -10.0, 0.5);
        let rect_neg: Rect = (&bb_neg).into();
        assert_eq!(rect_neg.width, 0);
        assert_eq!(rect_neg.height, 0);

        // NaN / Infinity dimensions clamp to 0.
        let bb_nan = BoundingBox::new(0.0, 0.0, f32::NAN, f32::INFINITY, 0.5);
        let rect_nan: Rect = (&bb_nan).into();
        assert_eq!(rect_nan.width, 0);
        assert_eq!(rect_nan.height, 0);

        // Zero width / height clamps to 0 (matches positive-only guard).
        let bb_zero = BoundingBox::new(5.0, 6.0, 0.0, 0.0, 0.5);
        let rect_zero: Rect = (&bb_zero).into();
        assert_eq!(rect_zero.x, 5);
        assert_eq!(rect_zero.y, 6);
        assert_eq!(rect_zero.width, 0);
        assert_eq!(rect_zero.height, 0);
    }

    #[test]
    fn vision_response_round_trip() {
        let resp = VisionResponse::new(
            Some("A code editor with a green diff".to_string()),
            vec![DetectedObject::new(
                "button".to_string(),
                BoundingBox::new(100.0, 200.0, 80.0, 32.0, 0.95),
                {
                    let mut m = BTreeMap::new();
                    m.insert("color".to_string(), "blue".to_string());
                    m
                },
            )],
            vec![synthetic_text_region()],
        );
        let json = serde_json::to_string(&resp).expect("serialize vision response");
        let back: VisionResponse =
            serde_json::from_str(&json).expect("deserialize vision response");
        assert_eq!(back, resp);
        assert!(back.description.is_some());
        assert_eq!(back.objects.len(), 1);
        assert_eq!(back.text_regions.len(), 1);
    }

    #[test]
    fn detected_object_default_attributes_empty() {
        let obj = DetectedObject::new(
            "person".to_string(),
            BoundingBox::new(0.0, 0.0, 10.0, 20.0, 0.8),
            BTreeMap::new(),
        );
        assert!(obj.attributes.is_empty());
        assert_eq!(obj.label, "person");
    }

    #[test]
    fn vision_response_description_only_pass() {
        // L1 impls that ship description-only (no objects · no OCR) ·
        // cheap fast path · cohérent the docstring contract.
        let resp = VisionResponse::new(
            Some("A frame with no detected objects".to_string()),
            vec![],
            vec![],
        );
        assert!(resp.objects.is_empty());
        assert!(resp.text_regions.is_empty());
        assert!(resp.description.is_some());
    }

    /// DEV-2 generic-bound compile check · ensures the trait shape is
    /// usable as a generic constraint via the `Send` companion. The
    /// inner function never runs · the type-check is the assertion.
    /// Cohérent sister-modules pattern (`io::screen` · `io::ocr` ·
    /// `io::a11y` · `io::browser` · `io::input`) · `#[test]` wrapper
    /// contains the generic-bound inner fn (no `#[allow(dead_code)]` ·
    /// tests excuse `dead_code` automatically).
    #[test]
    fn vision_model_generic_bound_compile_check() {
        fn _accepts_vision_model<T: VisionModelDyn>(_: &T) {}
    }

    /// Sanity-check the synthetic frame builder used across test cases.
    #[test]
    fn synthetic_frame_constructs_cleanly() {
        let frame = synthetic_frame();
        assert_eq!(frame.width, 1920);
        assert_eq!(frame.height, 1080);
    }
}
