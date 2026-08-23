// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `infer.vision:` — file paths become `data:` URLs; remote URLs stay URLs.
//!
//! CAS / content-addressed staging still belongs to `nika-media-*`. This
//! module is the v0.1 file/url ref path: a missing local file is a typed
//! `InvalidParam` (`param: "vision"`), never a green run over a dropped
//! image (#1135).

use std::path::Path;

use nika_kernel::ai::provider::ContentBlock;

use crate::VerbInferError;

/// One image the author pointed `infer.vision:` at (spec `02-verbs.md`).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum VisionPart {
    /// `source: file` — bytes are read at run and inlined as a `data:` URL.
    File {
        /// Local path, already `${{ }}`-rendered by the dispatcher.
        path: String,
    },
    /// `source: url` — forwarded as-is (the provider fetches, not nika).
    Url {
        /// Remote image URL, already `${{ }}`-rendered by the dispatcher.
        url: String,
    },
}

impl VisionPart {
    /// A local file path.
    #[must_use]
    pub fn file(path: impl Into<String>) -> Self {
        Self::File { path: path.into() }
    }

    /// A remote image URL.
    #[must_use]
    pub fn url(url: impl Into<String>) -> Self {
        Self::Url { url: url.into() }
    }
}

/// Load every vision part into kernel image blocks. File IO happens here:
/// a missing path fails closed before any provider round-trip.
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
        VisionPart::File { path } => {
            if path.trim().is_empty() {
                return Err(invalid("vision file path is empty"));
            }
            let bytes = std::fs::read(path) // seam-bypass-ok: v0.1 vision file load · nika-media CAS staging still deferred · missing path must refuse before any provider call
                .map_err(|err| invalid(format!("cannot read image `{path}`: {err}")))?;
            if bytes.is_empty() {
                return Err(invalid(format!("image `{path}` is empty")));
            }
            let mime = image_mime(path, &bytes);
            let b64 = base64_encode(&bytes);
            Ok(ContentBlock::Image {
                source: format!("data:{mime};base64,{b64}"),
                detail: None,
            })
        }
    }
}

fn invalid(detail: impl Into<String>) -> VerbInferError {
    VerbInferError::InvalidParam {
        param: "vision",
        detail: detail.into(),
    }
}

/// Magic-byte first, extension fallback, `image/png` last.
fn image_mime(path: &str, bytes: &[u8]) -> &'static str {
    if bytes.starts_with(&[0x89, b'P', b'N', b'G']) {
        return "image/png";
    }
    if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        return "image/jpeg";
    }
    if bytes.starts_with(b"GIF8") {
        return "image/gif";
    }
    if bytes.len() >= 12 && bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WEBP" {
        return "image/webp";
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

    #[test]
    fn missing_file_is_invalid_param_vision() {
        let err =
            vision_block(&VisionPart::file("./this-file-does-not-exist.png")).expect_err("missing");
        match err {
            VerbInferError::InvalidParam { param, detail } => {
                assert_eq!(param, "vision");
                assert!(
                    detail.contains("cannot read image"),
                    "missing file is named: {detail}"
                );
            }
            other => panic!("expected InvalidParam, got {other:?}"),
        }
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
