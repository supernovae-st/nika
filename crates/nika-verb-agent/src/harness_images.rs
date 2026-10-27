// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! A seat's images as task values (spec 02 §Harness images · `images:`).
//!
//! The harness path keeps the bytes of every image received on THIS task's own session
//! during this attempt ([`ReceivedImage`]); a path the harness only REPORTED is never one
//! of them and is never opened. Under `images:` the task's `.output` becomes
//! `{ message, images }`, and under `images: { save: <path> }` the engine writes each
//! received image through `nika:write` — the same `permits.fs.write` boundary every file
//! builtin meets, judged before any byte — then settles the image's trace observation with
//! the `saved_path` (`HarnessImageSaved`). A declared save with no received image fails
//! `NIKA-AGENT-007`, never a success with nothing written.

use base64::Engine as _;
use nika_kernel::runtime::tool_executor::ToolExecuteDyn;
use nika_verb_invoke::InvokeInput;
use sha2::Digest as _;

use crate::{AgentEvent, AgentImages, AgentObserver, AgentOutput, AgentValue, VerbAgentError};

/// One image whose bytes were received on the task's own harness session.
#[derive(Debug, Clone)]
pub(crate) struct ReceivedImage {
    /// The peer-local operation id: the observation this image settles.
    pub(crate) tool_call_id: String,
    /// The received bytes, exactly as decoded from the wire.
    pub(crate) bytes: bytes::Bytes,
    /// SHA-256 of [`Self::bytes`], lowercase hexadecimal.
    pub(crate) sha256: String,
    /// The media type the wire declared with the bytes.
    pub(crate) mime_type: String,
}

impl ReceivedImage {
    /// The received image of an observation whose bytes were taken for storage.
    pub(crate) fn of(image: &nika_kernel::ai::harness::HarnessImage, bytes: bytes::Bytes) -> Self {
        Self {
            tool_call_id: image.tool_call_id.clone(),
            sha256: format!("{:x}", sha2::Sha256::digest(&bytes)),
            mime_type: image
                .mime_type
                .clone()
                .unwrap_or_else(|| "application/octet-stream".to_owned()),
            bytes,
        }
    }

    /// Width and height from a PNG header (`IHDR`, big-endian), when the bytes are a PNG.
    fn dimensions(&self) -> (Option<u32>, Option<u32>) {
        let header = self.bytes.get(..24).filter(|h| {
            h.starts_with(b"\x89PNG\r\n\x1a\n") && h.get(12..16) == Some(b"IHDR".as_slice())
        });
        let read = |at: usize| {
            header
                .and_then(|h| h.get(at..at + 4))
                .and_then(|b| <[u8; 4]>::try_from(b).ok())
                .map(u32::from_be_bytes)
        };
        (read(16), read(20))
    }
}

/// One finished harness turn: the shaped output, and the images whose bytes were received
/// on THIS session during this attempt — a path-only report is never one of them, only
/// counted.
#[derive(Debug)]
pub(crate) struct HarnessRun {
    /// The task's output, its final message validated against a `schema:`.
    pub(crate) output: AgentOutput,
    /// The received images, in the order the session delivered them.
    pub(crate) received: Vec<ReceivedImage>,
    /// Image observations that carried only a reported path (never opened).
    pub(crate) reported_only: usize,
}

/// The final message against the task `schema:` (spec 02 §agent): the extracted value, or
/// NIKA-464 (wire `NIKA-INFER-002`). A seat gets no repair turn: a new session would replay
/// the agent's work, so the miss fails the task with what was answered.
pub(crate) fn validated(
    output: &AgentValue,
    validator: &jsonschema::Validator,
) -> Result<AgentValue, VerbAgentError> {
    let AgentValue::Text(text) = output else {
        return Ok(output.clone());
    };
    crate::shape::validate_text(text, validator)
        .map(AgentValue::Structured)
        .map_err(|detail| VerbAgentError::SchemaValidation {
            detail: format!(
                "{detail} (a seat's final message is validated without a repair turn: a new \
                 session would replay the agent's work)"
            ),
            spend: Box::default(),
        })
}

/// Every received image's path is judged against the declared `permits.fs.write` before the
/// first byte is written (spec 02 §Harness images): a sibling path the grant cannot hold while
/// it holds the first one (an exact-file grant, say, that has no room for `-2`) refuses the
/// whole delivery with `NIKA-SEC-004` and nothing written. The judge is the lexical fit
/// `nika check` and the harness permission bridge apply. A first path that fit cannot judge
/// (an absolute or home spelling) is left, with its siblings, to the write builtin's own
/// boundary, which judges each path on the resolved filesystem.
fn refuse_unheld_siblings(
    paths: &[String],
    permits: Option<&nika_schema::types::Permits>,
) -> Result<(), VerbAgentError> {
    let (Some(permits), Some(first)) = (permits, paths.first()) else {
        return Ok(());
    };
    if !permits.allows_path(first, true) {
        return Ok(());
    }
    match paths.iter().find(|path| !permits.allows_path(path, true)) {
        Some(_) => Err(VerbAgentError::SecurityBoundary {
            tool: "nika:write".to_owned(),
            code: "NIKA-SEC-004".to_owned(),
            spend: Box::default(),
        }),
        None => Ok(()),
    }
}

/// The message `.output` holds without `images:` (text, or the validated object).
fn message(value: &AgentValue) -> serde_json::Value {
    match value {
        AgentValue::Text(text) => serde_json::Value::String(text.clone()),
        AgentValue::Structured(value) => value.clone(),
    }
}

impl<P, T, D> crate::AgentVerb<P, T, D>
where
    T: ToolExecuteDyn,
{
    /// Make the received images the task's value and, under `save:`, write them (spec 02
    /// §Harness images). The output's message is kept whole; every path is the one written.
    ///
    /// # Errors
    ///
    /// `NIKA-AGENT-007` when `save:` is declared and no image bytes were received; a harness
    /// refusal, before any write, when more than four were (a save names at most four files);
    /// the boundary's own `NIKA-SEC-004` when a path is outside `permits.fs.write`; the write's
    /// own code when the file cannot be written.
    pub(crate) async fn deliver_images(
        &self,
        request: &AgentImages,
        permits: Option<&nika_schema::types::Permits>,
        run: HarnessRun,
        observer: &dyn AgentObserver,
    ) -> Result<AgentOutput, VerbAgentError> {
        let HarnessRun {
            mut output,
            received,
            reported_only,
        } = run;
        let message = message(&output.output);
        if request.save.is_some() && received.len() > nika_types::access::MAX_HARNESS_IMAGES {
            // A save names at most four files, the ones every check judged (the path, then
            // `-2` to `-4`): a fifth image fails the task before any write, whoever delivered
            // it (a lent backend no ACP client bounded included).
            return Err(VerbAgentError::Harness {
                source: nika_kernel::ai::harness::HarnessError::Refused {
                    reason: format!(
                        "the session delivered {} images, more than the four images a save \
                         writes (the path, then `-2` to `-4`); nothing was written",
                        received.len()
                    ),
                },
                spend: Box::default(),
            });
        }
        if request.save.is_some() && received.is_empty() {
            return Err(VerbAgentError::NoImage {
                reported: reported_only,
                partial_output: match &message {
                    serde_json::Value::String(text) => text.clone(),
                    other => other.to_string(),
                },
                spend: Box::default(),
            });
        }
        let paths: Vec<String> = (request.save.iter())
            .flat_map(|save| {
                (0..received.len()).map(|index| nika_types::access::harness_image_path(save, index))
            })
            .collect();
        refuse_unheld_siblings(&paths, permits)?;
        let mut images = Vec::with_capacity(received.len());
        for (index, image) in received.iter().enumerate() {
            let path = match paths.get(index) {
                Some(path) => {
                    let path = path.clone();
                    self.write_image(&path, image).await?;
                    observer.on_event(&AgentEvent::HarnessImageSaved {
                        tool_call_id: image.tool_call_id.clone(),
                        path: path.clone(),
                    });
                    Some(path)
                }
                None => None,
            };
            let (width, height) = image.dimensions();
            images.push(serde_json::json!({
                "index": index,
                "path": path,
                "sha256": image.sha256,
                "size_bytes": image.bytes.len(),
                "mime_type": image.mime_type,
                "width": width,
                "height": height,
            }));
        }
        output.output =
            AgentValue::Structured(serde_json::json!({"message": message, "images": images}));
        Ok(output)
    }

    /// Write one received image through `nika:write`, under the workflow's file boundary.
    async fn write_image(&self, path: &str, image: &ReceivedImage) -> Result<(), VerbAgentError> {
        let mut call = InvokeInput::new("nika:write");
        call.args = serde_json::json!({
            "path": path,
            "content": {"bytes_base64": base64::engine::general_purpose::STANDARD.encode(&image.bytes)},
            "overwrite": true,
            "create_dirs": true,
        });
        let Err(error) = self.invoke.run(call).await else {
            return Ok(());
        };
        if let Some(code) = crate::security_boundary_code(&error) {
            return Err(VerbAgentError::SecurityBoundary {
                tool: "nika:write".to_owned(),
                code: code.to_owned(),
                spend: Box::default(),
            });
        }
        Err(VerbAgentError::ImageWrite {
            path: path.to_owned(),
            internal: nika_error::traits::NikaErrorCode::nika_code(&error),
            code: nika_error::traits::NikaErrorCode::spec_code(&error),
            detail: error.to_string(),
            spend: Box::default(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn permits(write: &[&str]) -> nika_schema::types::Permits {
        let mut permits = nika_schema::types::Permits::new();
        permits.fs = Some(nika_schema::types::FsPermits::new(
            Vec::new(),
            write.iter().map(|w| (*w).to_owned()).collect(),
        ));
        permits
    }

    /// An exact-file grant holds the first image and not its `-2` sibling: refused before any
    /// write. A directory grant holds both; a first path the lexical fit cannot judge is the
    /// write builtin's to judge, and an undeclared block is the zero boundary's.
    #[test]
    fn a_sibling_the_grant_cannot_hold_refuses_before_any_write() {
        let two = ["./out/a.png".to_owned(), "./out/a-2.png".to_owned()];
        let refused = refuse_unheld_siblings(&two, Some(&permits(&["./out/a.png"])))
            .expect_err("no room for the sibling");
        assert!(
            matches!(&refused, VerbAgentError::SecurityBoundary { tool, code, .. }
                if tool == "nika:write" && code == "NIKA-SEC-004"),
            "{refused:?}"
        );
        assert!(refuse_unheld_siblings(&two, Some(&permits(&["./out/**"]))).is_ok());
        assert!(refuse_unheld_siblings(&two[..1], Some(&permits(&["./out/a.png"]))).is_ok());
        let absolute = [
            "/srv/p/out/a.png".to_owned(),
            "/srv/p/out/a-2.png".to_owned(),
        ];
        assert!(refuse_unheld_siblings(&absolute, Some(&permits(&["./out/a.png"]))).is_ok());
        assert!(refuse_unheld_siblings(&two, None).is_ok());
    }

    #[test]
    fn png_dimensions_come_from_the_header_and_other_bytes_have_none() {
        let mut png = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
        png.extend_from_slice(&1254_u32.to_be_bytes());
        png.extend_from_slice(&941_u32.to_be_bytes());
        let image = ReceivedImage {
            tool_call_id: "ig-1".to_owned(),
            bytes: png.into(),
            sha256: String::new(),
            mime_type: "image/png".to_owned(),
        };
        assert_eq!(image.dimensions(), (Some(1254), Some(941)));
        let jpeg = ReceivedImage {
            bytes: bytes::Bytes::from_static(&[0xFF, 0xD8, 0xFF, 0xE0]),
            ..image
        };
        assert_eq!(jpeg.dimensions(), (None, None));
    }
}
