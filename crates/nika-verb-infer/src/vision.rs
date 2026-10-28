// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `infer.vision:` — loaded image bytes become `data:` URLs; remote URLs stay URLs.
//!
//! The verb never opens a file. A `source: file` entry is a read inside the workflow's
//! `permits.fs.read` boundary (spec 02 §Vision inputs): the runtime reads it through
//! `nika:read` under that boundary and hands the bytes here as [`VisionPart::Loaded`]. An
//! empty image is a typed `InvalidParam` (`param: "vision"`), never a green run over a
//! dropped image (#1135). CAS / content-addressed staging still belongs to `nika-media-*`.

use std::path::Path;

use nika_kernel::ai::provider::ContentBlock;

use crate::VerbInferError;

/// One image the author pointed `infer.vision:` at (spec `02-verbs.md` §Vision inputs).
#[derive(Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum VisionPart {
    /// `source: file`, already read: the caller judged the path against the workflow's
    /// `permits.fs.read` boundary and read it there (the runtime reads through `nika:read`).
    /// Its bytes are inlined as a `data:` URL on an API route and sent as an image block over
    /// ACP; the path only names the image and gives a media-type fallback, never reopened.
    Loaded {
        /// The path as rendered, naming the image in a refusal.
        path: String,
        /// The file's bytes.
        bytes: Vec<u8>,
    },
    /// `source: url` — forwarded as-is (the provider fetches, not nika).
    Url {
        /// Remote image URL, already `${{ }}`-rendered by the dispatcher.
        url: String,
    },
}

impl std::fmt::Debug for VisionPart {
    /// The bytes are counted, never printed: an image in a debug line is noise and may be a
    /// file the reader was never shown.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Loaded { path, bytes } => f
                .debug_struct("Loaded")
                .field("path", path)
                .field("bytes", &format_args!("<{} bytes>", bytes.len()))
                .finish(),
            Self::Url { url } => f.debug_struct("Url").field("url", url).finish(),
        }
    }
}

impl VisionPart {
    /// A `source: file` image the caller already read inside the workflow's read boundary.
    #[must_use]
    pub fn loaded(path: impl Into<String>, bytes: impl Into<Vec<u8>>) -> Self {
        Self::Loaded {
            path: path.into(),
            bytes: bytes.into(),
        }
    }

    /// A remote image URL.
    #[must_use]
    pub fn url(url: impl Into<String>) -> Self {
        Self::Url { url: url.into() }
    }
}

/// Turn every vision part into a kernel image block, before any provider round-trip.
pub(crate) fn vision_blocks(parts: &[VisionPart]) -> Result<Vec<ContentBlock>, VerbInferError> {
    let mut out = Vec::with_capacity(parts.len());
    for part in parts {
        out.push(vision_block(part)?);
    }
    Ok(out)
}

fn vision_block(part: &VisionPart) -> Result<ContentBlock, VerbInferError> {
    match part {
        VisionPart::Url { url } => {
            if url.trim().is_empty() {
                return Err(invalid("vision URL is empty"));
            }
            Ok(ContentBlock::Image {
                source: url.clone(),
                detail: None,
            })
        }
        VisionPart::Loaded { path, bytes } => {
            nonempty(path, bytes)?;
            let mime = image_mime(path, bytes);
            Ok(ContentBlock::Image {
                source: format!("data:{mime};base64,{}", base64_encode(bytes)),
                detail: None,
            })
        }
    }
}

/// A loaded image with no bytes is refused before any call.
fn nonempty(path: &str, bytes: &[u8]) -> Result<(), VerbInferError> {
    if bytes.is_empty() {
        return Err(invalid(format!("image `{path}` is empty")));
    }
    Ok(())
}

/// One loaded image for an ACP session: only bytes that open with an image signature
/// (PNG, JPEG, GIF, WebP) ride, under the media type they declare. Any other file is refused
/// before the session starts, never relabelled `image/png` and handed to the agent.
#[cfg(feature = "access-harness")]
fn signed_image(path: &str, bytes: &[u8]) -> Result<(&'static str, String), VerbInferError> {
    nonempty(path, bytes)?;
    let mime = image_signature(bytes).ok_or_else(|| {
        invalid(format!(
            "`{path}` is not an image (no PNG, JPEG, GIF or WebP signature): an ACP session \
             receives image files only"
        ))
    })?;
    Ok((mime, base64_encode(bytes)))
}

/// The `vision:` files as the images an ACP session sends with its prompt (image content
/// blocks), each an image by its own signature. A URL is never fetched by Nika: the caller
/// refuses it before reaching here, and this loader refuses it again rather than drop it.
#[cfg(feature = "access-harness")]
pub(crate) fn prompt_images(
    parts: &[VisionPart],
) -> Result<Vec<nika_kernel::ai::harness::HarnessPromptImage>, VerbInferError> {
    parts
        .iter()
        .map(|part| match part {
            VisionPart::Loaded { path, bytes } => signed_image(path, bytes)
                .map(|(mime, b64)| nika_kernel::ai::harness::HarnessPromptImage::new(mime, b64)),
            VisionPart::Url { url } => Err(invalid(format!(
                "vision URL `{url}` cannot ride an ACP session: Nika does not fetch it; point \
                 `vision:` at a local file"
            ))),
        })
        .collect()
}

/// A direct one-shot seat (`codex exec`, `claude -p` …) carries the prompt, the system, the
/// model and a `schema:` on its command, never an image: an `infer:` that names `vision:`
/// there is refused before the seat is spawned, as the ACP one-shot refuses a URL, instead of
/// answering without the images it names. A seat whose ACP one-shot reads images is named as
/// the route that carries them.
#[cfg(feature = "access-harness")]
pub(crate) fn refuse_on_direct_seat(
    parts: &[VisionPart],
    seat: &str,
) -> Result<(), VerbInferError> {
    if parts.is_empty() {
        return Ok(());
    }
    let route = if nika_harness::meet_acp_one_shot(seat)
        .is_ok_and(nika_harness::AcpOneShot::image_prompts)
    {
        format!(
            "declare `run.access: {{ via: {seat}, protocol: acp }}`, whose ACP one-shot carries \
             `vision:` files, or an API route"
        )
    } else {
        "run the task on an API route".to_owned()
    };
    Err(invalid(format!(
        "`{seat}` serves this `infer:` through its direct one-shot, whose command carries the \
         prompt, system, model and a `schema:`, never an image, so the model would not see \
         `vision:`: {route}, or drop `vision:` (nothing was sent)"
    )))
}

fn invalid(detail: impl Into<String>) -> VerbInferError {
    VerbInferError::InvalidParam {
        param: "vision",
        detail: detail.into(),
    }
}

/// The media type an image's leading bytes declare (PNG, JPEG, GIF, WebP), if any.
fn image_signature(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(&[0x89, b'P', b'N', b'G']) {
        return Some("image/png");
    }
    if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        return Some("image/jpeg");
    }
    if bytes.starts_with(b"GIF8") {
        return Some("image/gif");
    }
    if bytes.len() >= 12
        && bytes.starts_with(b"RIFF")
        && bytes.get(8..12) == Some(b"WEBP".as_slice())
    {
        return Some("image/webp");
    }
    None
}

/// Magic-byte first, extension fallback, `image/png` last.
fn image_mime(path: &str, bytes: &[u8]) -> &'static str {
    if let Some(mime) = image_signature(bytes) {
        return mime;
    }
    match Path::new(path)
        .extension()
        .and_then(|ext| ext.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        Some("bmp") => "image/bmp",
        _ => "image/png",
    }
}

/// RFC 4648 standard base64 — small, dep-free (the nika-builtin encoder's twin).
fn base64_encode(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        out.push(ALPHABET[((n >> 18) & 63) as usize] as char);
        out.push(ALPHABET[((n >> 12) & 63) as usize] as char);
        out.push(if chunk.len() > 1 {
            ALPHABET[((n >> 6) & 63) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            ALPHABET[(n & 63) as usize] as char
        } else {
            '='
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_url_is_invalid_param_vision() {
        let err = vision_block(&VisionPart::url("  ")).expect_err("empty");
        assert!(matches!(
            err,
            VerbInferError::InvalidParam {
                param: "vision",
                ..
            }
        ));
    }

    /// The bytes are the caller's: an empty image is refused, never sent.
    #[test]
    fn empty_loaded_image_is_invalid_param_vision() {
        let err =
            vision_block(&VisionPart::loaded("./in/blank.png", Vec::new())).expect_err("empty");
        match err {
            VerbInferError::InvalidParam { param, detail } => {
                assert_eq!(param, "vision");
                assert_eq!(detail, "image `./in/blank.png` is empty");
            }
            other => panic!("expected InvalidParam, got {other:?}"),
        }
    }

    /// The verb never opens the path: the loaded bytes ride even when no file sits there.
    #[test]
    fn a_loaded_image_rides_its_bytes_and_never_reopens_the_path() {
        let block = vision_block(&VisionPart::loaded(
            "./this-file-does-not-exist.jpg",
            b"GIF89a-bytes".to_vec(),
        ))
        .expect("the bytes are the image");
        match block {
            ContentBlock::Image { source, .. } => assert_eq!(
                source,
                format!("data:image/gif;base64,{}", base64_encode(b"GIF89a-bytes")),
                "the signature names the type before the extension"
            ),
            other => panic!("expected Image, got {other:?}"),
        }
    }

    #[test]
    fn the_debug_form_counts_the_bytes_and_never_prints_them() {
        let part = VisionPart::loaded("./in/key.png", b"API_KEY=sk-live".to_vec());
        let shown = format!("{part:?}");
        assert_eq!(
            shown,
            "Loaded { path: \"./in/key.png\", bytes: <15 bytes> }"
        );
    }

    #[cfg(feature = "access-harness")]
    #[test]
    fn prompt_images_keep_order_and_refuse_a_url_or_a_non_image() {
        let png = b"\x89PNG\r\n\x1a\nrest".to_vec();
        let jpeg = vec![0xFF, 0xD8, 0xFF, 0x00];
        let parts = [
            VisionPart::loaded("./a.png", png.clone()),
            VisionPart::loaded("./b.bin", jpeg),
        ];
        let images = prompt_images(&parts).expect("images load");
        assert_eq!(images.len(), 2);
        assert_eq!(images[0].mime_type, "image/png");
        assert_eq!(images[0].data, base64_encode(&png));
        assert_eq!(
            images[1].mime_type, "image/jpeg",
            "magic bytes, not the extension"
        );
        // Bytes with no image signature never ride, whatever the extension says.
        let refused = prompt_images(&[VisionPart::loaded(
            "./secret.png",
            b"API_KEY=not-an-image\n".to_vec(),
        )])
        .expect_err("not an image");
        assert!(
            matches!(&refused, VerbInferError::InvalidParam { param: "vision", detail }
                if detail.contains("is not an image")),
            "{refused:?}"
        );
        let url = prompt_images(&[VisionPart::url("https://example.com/x.png")])
            .expect_err("a URL is never fetched");
        assert!(
            matches!(&url, VerbInferError::InvalidParam { param: "vision", detail } if detail.contains("does not fetch")),
            "{url:?}"
        );
    }

    #[test]
    fn url_stays_a_url() {
        let block = vision_block(&VisionPart::url("http://127.0.0.1:8731/x.png")).expect("url");
        match block {
            ContentBlock::Image { source, .. } => {
                assert_eq!(source, "http://127.0.0.1:8731/x.png");
            }
            other => panic!("expected Image, got {other:?}"),
        }
    }
}
