// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Pure CDP → kernel mappers — **headless-testable · mutation-killed**.
//!
//! Everything DECIDABLE without a live browser lives here: the CDP `Node` →
//! kernel [`DomNode`] mapper (depth-capped — untrusted input), the flat
//! attribute-pair pairing, the RFC-3986 absolute-URL validation that gates
//! `navigate`, and the screenshot PNG → RGBA8 [`Frame`] decode. The CDP
//! residue in `lib.rs` calls these and adds only wire I/O.

use std::collections::BTreeMap;

use chromiumoxide::cdp::browser_protocol::dom::Node as CdpNode;
use nika_kernel::io::browser::{BrowserError, DomNode};
use nika_kernel::io::screen::{DisplayId, Frame, Rect};

use crate::MAX_DOM_DEPTH;

/// CDP `nodeType` for element nodes (DOM spec: `ELEMENT_NODE == 1`).
const ELEMENT_NODE: i64 = 1;

/// Pair CDP's flat attribute array (`[name1, value1, name2, value2, …]`)
/// into the kernel's deterministic `BTreeMap`. A trailing odd name (protocol
/// corruption) is DROPPED — never invent a value for a hostile page.
#[must_use]
pub(crate) fn attrs_to_map(flat: &[String]) -> BTreeMap<String, String> {
    flat.chunks_exact(2)
        .map(|pair| (pair[0].clone(), pair[1].clone()))
        .collect()
}

/// Map a CDP element `Node` to the kernel [`DomNode`] — depth-capped at
/// [`MAX_DOM_DEPTH`] (untrusted input: children beyond the cap are TRUNCATED,
/// the node itself survives). Non-element children (text, comments) are
/// skipped — the kernel tree is elements-only. Snapshot nodes carry
/// `bbox: None` (box models are resolved live on the click path only).
#[must_use]
pub(crate) fn map_element(node: &CdpNode, depth: u16) -> DomNode {
    let children = if depth >= MAX_DOM_DEPTH {
        Vec::new() // cap: a hostile deep tree cannot recurse past here
    } else {
        node.children
            .as_deref()
            .unwrap_or_default()
            .iter()
            .filter(|c| c.node_type == ELEMENT_NODE)
            .map(|c| map_element(c, depth.saturating_add(1)))
            .collect()
    };
    DomNode::new(
        node.local_name.to_ascii_lowercase(),
        attrs_to_map(node.attributes.as_deref().unwrap_or_default()),
        children,
        None,
    )
}

/// Map a CDP document tree to the kernel root element (`<html>`): breadth-first
/// search for the FIRST element node, then [`map_element`] from there.
///
/// # Errors
/// [`BrowserError::SelectorFailed`] when the document contains no element node
/// (a page with zero elements is not a navigable DOM — protocol anomaly).
pub(crate) fn map_document(root: &CdpNode) -> Result<DomNode, BrowserError> {
    let mut queue: Vec<&CdpNode> = vec![root];
    while let Some(n) = queue.pop() {
        if n.node_type == ELEMENT_NODE {
            return Ok(map_element(n, 0));
        }
        queue.extend(n.children.as_deref().unwrap_or_default().iter());
    }
    Err(BrowserError::SelectorFailed {
        reason: "dom snapshot: document contains no element node".to_owned(),
    })
}

/// Validate `url` as an RFC-3986 ABSOLUTE http(s) URI — the pure gate the
/// `navigate` path runs BEFORE any CDP dispatch (per the kernel trait doc:
/// malformed input → [`BrowserError::NavigationFailed`] · NIKA-1402).
///
/// Only `http`/`https` navigate: `file://` would hand the page filesystem
/// reach, `javascript:` is script injection by construction — fail CLOSED.
///
/// # Errors
/// [`BrowserError::NavigationFailed`] naming the rejected shape (the URL
/// itself is agent-authored, safe to echo).
pub(crate) fn validate_absolute_url(url: &str) -> Result<(), BrowserError> {
    let parsed = url::Url::parse(url).map_err(|e| BrowserError::NavigationFailed {
        reason: format!("malformed URL {url:?}: {e}"),
    })?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err(BrowserError::NavigationFailed {
            reason: format!(
                "URL scheme {:?} refused — only http/https navigation is allowed",
                parsed.scheme()
            ),
        });
    }
    if parsed.host_str().is_none() {
        return Err(BrowserError::NavigationFailed {
            reason: format!("URL {url:?} has no host — absolute URI required"),
        });
    }
    Ok(())
}

/// Decode a CDP `Page.captureScreenshot` PNG into the kernel RGBA8 [`Frame`]
/// — **pure** (bytes in, Frame out · headless-testable). RGB input is
/// expanded to RGBA (alpha 255); 16-bit and palette inputs are normalized by
/// the decoder transformations.
///
/// `display_id` is the virtual browser-viewport display (`DisplayId(0)` —
/// a CDP screenshot has no OS display); `scale_factor` 1.0 (CDP returns CSS
/// pixels unless device-scale emulation is active).
///
/// # Errors
/// A corrupt screenshot payload is a TRANSIENT protocol fault on an
/// established session — mapped to [`BrowserError::TaskJoinFailed`]
/// (NIKA-1406 · `is_transient() == true` per the kernel impl): retrying the
/// capture is the correct caller move, and no other variant in the kernel
/// enum carries transient semantics for an in-session fault.
pub(crate) fn png_to_frame(png_bytes: &[u8], captured_at_ns: u64) -> Result<Frame, BrowserError> {
    let fault = |reason: String| BrowserError::TaskJoinFailed { reason };
    let mut decoder = png::Decoder::new(std::io::Cursor::new(png_bytes));
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder
        .read_info()
        .map_err(|e| fault(format!("screenshot PNG header decode failed: {e}")))?;
    let buf_size = reader
        .output_buffer_size()
        .ok_or_else(|| fault("screenshot PNG dimensions overflow the buffer size".to_owned()))?;
    let mut buf = vec![0u8; buf_size];
    let info = reader
        .next_frame(&mut buf)
        .map_err(|e| fault(format!("screenshot PNG frame decode failed: {e}")))?;
    buf.truncate(info.buffer_size());
    let rgba = match info.color_type {
        png::ColorType::Rgba => buf,
        png::ColorType::Rgb => {
            let mut out = Vec::with_capacity(buf.len() / 3 * 4);
            for px in buf.chunks_exact(3) {
                out.extend_from_slice(px);
                out.push(255);
            }
            out
        }
        other => {
            return Err(fault(format!(
                "screenshot PNG color type {other:?} unsupported (expected RGB/RGBA)"
            )));
        }
    };
    Ok(Frame::new(
        info.width,
        info.height,
        1.0,
        bytes::Bytes::from(rgba),
        DisplayId::new(0),
        captured_at_ns,
    ))
}

/// Round a CDP `BoundingBox` (f64 CSS-pixel space) to the kernel integer
/// [`Rect`] — pure. Negative/NaN dimensions clamp to 0 (an unrenderable box
/// must read as INVISIBLE to Guard 5, never as a huge cast artifact).
#[must_use]
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "f64→i32/u32 at the CDP boundary · dimensions are sign-checked \
              (v > 0.0) and clamped to the target domain before the cast · \
              viewport coordinates never exceed ±2^31 px"
)]
pub(crate) fn bbox_to_rect(x: f64, y: f64, width: f64, height: f64) -> Rect {
    let dim = |v: f64| -> u32 {
        if v.is_finite() && v > 0.0 {
            v.round().min(f64::from(u32::MAX)) as u32
        } else {
            0
        }
    };
    let pos = |v: f64| -> i32 {
        if v.is_finite() {
            v.round().clamp(f64::from(i32::MIN), f64::from(i32::MAX)) as i32
        } else {
            0
        }
    };
    Rect::new(pos(x), pos(y), dim(width), dim(height))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a CDP `Node` from JSON — version-proof (the codegen struct has
    /// 40+ fields that churn per protocol revision; `Deserialize` is its
    /// stable construction surface).
    fn cdp_node(
        node_type: i64,
        local_name: &str,
        attrs: Option<Vec<String>>,
        children: Option<Vec<CdpNode>>,
    ) -> CdpNode {
        let mut v = serde_json::json!({
            "nodeId": 1,
            "backendNodeId": 1,
            "nodeType": node_type,
            "nodeName": local_name.to_ascii_uppercase(),
            "localName": local_name,
            "nodeValue": "",
        });
        if let Some(a) = attrs {
            v["attributes"] = serde_json::json!(a);
        }
        if let Some(c) = children {
            v["children"] = serde_json::to_value(c).expect("children serialize");
        }
        serde_json::from_value(v).expect("CDP Node deserializes")
    }

    #[test]
    fn attrs_pairing_drops_trailing_odd_name() {
        let map = attrs_to_map(&[
            "id".to_owned(),
            "submit".to_owned(),
            "type".to_owned(),
            "button".to_owned(),
            "dangling".to_owned(), // hostile odd tail — dropped, never invented
        ]);
        assert_eq!(map.len(), 2);
        assert_eq!(map.get("id").map(String::as_str), Some("submit"));
        assert_eq!(map.get("type").map(String::as_str), Some("button"));
        assert!(!map.contains_key("dangling"));
    }

    #[test]
    fn map_element_lowercases_tag_skips_non_elements_and_caps_depth() {
        // text node (type 3) child must be skipped; tag normalized.
        let text_child = cdp_node(3, "#text", None, None);
        let button = cdp_node(
            1,
            "BUTTON",
            Some(vec!["id".to_owned(), "ok".to_owned()]),
            None,
        );
        let div = cdp_node(1, "div", None, Some(vec![text_child, button]));
        let mapped = map_element(&div, 0);
        assert_eq!(mapped.tag, "div");
        assert_eq!(mapped.children.len(), 1, "text node skipped");
        assert_eq!(mapped.children[0].tag, "button");
        assert_eq!(
            mapped.children[0].attributes.get("id").map(String::as_str),
            Some("ok")
        );
        // Depth cap: at MAX_DOM_DEPTH the children are truncated.
        let leafy = cdp_node(1, "span", None, None);
        let parent = cdp_node(1, "div", None, Some(vec![leafy]));
        let capped = map_element(&parent, MAX_DOM_DEPTH);
        assert!(capped.children.is_empty(), "children truncated at the cap");
        let uncapped = map_element(&parent, MAX_DOM_DEPTH - 1);
        assert_eq!(uncapped.children.len(), 1, "one below the cap maps fully");
    }

    #[test]
    fn map_document_finds_the_root_element_or_fails_closed() {
        let html = cdp_node(1, "html", None, None);
        let doc = cdp_node(9, "#document", None, Some(vec![html]));
        assert_eq!(map_document(&doc).expect("root element").tag, "html");
        let empty_doc = cdp_node(9, "#document", None, None);
        assert!(
            map_document(&empty_doc).is_err(),
            "no element = protocol anomaly"
        );
    }

    #[test]
    fn url_validation_allows_http_s_rejects_everything_else() {
        assert!(validate_absolute_url("https://example.org/login").is_ok());
        assert!(validate_absolute_url("http://127.0.0.1:8080/x?y=1").is_ok());
        for bad in [
            "example.org/no-scheme",
            "file:///etc/passwd",
            "javascript:alert(1)",
            "ftp://host/x",
            "data:text/html,<script>",
            "",
            "http://",
        ] {
            assert!(
                validate_absolute_url(bad).is_err(),
                "{bad:?} must be refused"
            );
        }
    }

    #[test]
    fn png_round_trip_decodes_rgba_and_expands_rgb() {
        // Encode a 2×2 RGBA PNG with the same crate, decode through the
        // mapper, assert pixel fidelity (kills the Ok-stub mutants).
        let pixels: [u8; 16] = [
            255, 0, 0, 255, /* red */ 0, 255, 0, 255, /* green */
            0, 0, 255, 255, /* blue */ 9, 8, 7, 6, /* arbitrary */
        ];
        let mut png_buf = Vec::new();
        {
            let mut enc = png::Encoder::new(&mut png_buf, 2, 2);
            enc.set_color(png::ColorType::Rgba);
            enc.set_depth(png::BitDepth::Eight);
            let mut w = enc.write_header().expect("header");
            w.write_image_data(&pixels).expect("data");
        }
        let frame = png_to_frame(&png_buf, 42).expect("decode");
        assert_eq!((frame.width, frame.height), (2, 2));
        assert_eq!(frame.captured_at_ns, 42);
        assert_eq!(frame.pixels.as_ref(), &pixels);
        // RGB source → alpha expanded to 255.
        let rgb: [u8; 12] = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12];
        let mut rgb_png = Vec::new();
        {
            let mut enc = png::Encoder::new(&mut rgb_png, 2, 2);
            enc.set_color(png::ColorType::Rgb);
            enc.set_depth(png::BitDepth::Eight);
            let mut w = enc.write_header().expect("header");
            w.write_image_data(&rgb).expect("data");
        }
        let frame = png_to_frame(&rgb_png, 7).expect("decode rgb");
        assert_eq!(
            frame.pixels.as_ref(),
            &[1, 2, 3, 255, 4, 5, 6, 255, 7, 8, 9, 255, 10, 11, 12, 255]
        );
        // Garbage bytes fail CLOSED (kills read-side stub mutants).
        assert!(png_to_frame(b"not a png", 0).is_err());
    }

    #[test]
    fn bbox_rounding_clamps_hostile_values_to_invisible() {
        let r = bbox_to_rect(10.4, 20.6, 99.5, 40.2);
        assert_eq!((r.x, r.y, r.width, r.height), (10, 21, 100, 40));
        // NaN / negative dimensions must read INVISIBLE (0), never huge.
        let nan = bbox_to_rect(f64::NAN, 0.0, f64::NAN, -5.0);
        assert_eq!((nan.x, nan.width, nan.height), (0, 0, 0));
    }
}
