// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `infer.vision:` (spec 02 §Vision inputs): a `source: file` entry is a READ inside the
//! workflow's `permits.fs.read` boundary, on every route.
//!
//! The dispatch renders every entry first (a template error fails the task as any field's
//! would), then, once the task's lane is admitted, reads each file through `nika:read`
//! (`binary: true`) on the runtime's OWN invoke verb. The builtin plane judges it exactly as
//! it judges a workflow's own `nika:read`: on its canonical path (symlinks and `..` resolved),
//! re-judged at open with no symlink followed, anchored at the run's root (a rehearsal room's
//! copy, a child's intersected boundary) and witnessed as one `permit_checked` fs frame. A
//! path outside the boundary fails the task `NIKA-SEC-004`; a file that cannot be read fails it
//! with the read's own canonical code (`NIKA-BUILTIN-READ-001` when no file is there, `-002`
//! for any other read failure, as `nika:read` itself fails), both before any provider call
//! (#1135). The verb receives the bytes and never opens a file. A `source: url` is not an fs
//! effect: the provider fetches it.

use base64::Engine as _;
use nika_error::traits::NikaErrorCode;
use nika_kernel::tool_executor::ToolExecuteDyn;
use nika_schema::Spanned;
use nika_schema::raw::VisionInput;
use nika_verb_infer::VisionPart;
use nika_verb_invoke::{InvokeInput, VerbInvokeError};

use super::{Dispatched, FailedDispatch};
use crate::Runtime;
use crate::errors::RuntimeError;
use crate::expr::{self, Scope};
use crate::record::TaskErrorRecord;

/// The builtin that reads a vision file, and the code its boundary refuses with.
const READ: &str = "nika:read";
const OUTSIDE: &str = "NIKA-SEC-004";
/// The read's own canonical codes (stdlib `nika:read`): no file at the path, and any other
/// read failure.
const NOT_FOUND: &str = "NIKA-BUILTIN-READ-001";
const READ_FAILED: &str = "NIKA-BUILTIN-READ-002";

/// One `vision:` entry rendered over the scope; a file is not read yet.
pub(super) enum Rendered {
    /// `source: file` — the rendered path, judged when it is read.
    File(String),
    /// `source: url` — handed to the provider as-is.
    Url(String),
}

/// Render every `vision:` entry through the one `${{ }}` seam. Nothing is read here: the
/// lane decides first whether the task runs at all.
pub(super) fn render(
    items: &[Spanned<VisionInput>],
    scope: &Scope<'_>,
) -> Result<Vec<Rendered>, Box<Dispatched>> {
    let mut out = Vec::with_capacity(items.len());
    for item in items {
        let (template, file) = match &item.value {
            VisionInput::File { path } => (&path.value, true),
            VisionInput::Url { url } => (&url.value, false),
            other => {
                return Err(Box::new(Dispatched::unwired(
                    "infer · ?",
                    format!("vision source form not wired yet: {other:?}"),
                )));
            }
        };
        let value = expr::render(template, scope).map_err(|err| {
            Box::new(Dispatched::template_err(
                "infer · ?",
                &RuntimeError::from(err),
            ))
        })?;
        out.push(if file {
            Rendered::File(value)
        } else {
            Rendered::Url(value)
        });
    }
    Ok(out)
}

impl<S, T, H, P, D, C> Runtime<S, T, H, P, D, C>
where
    T: ToolExecuteDyn,
{
    /// Read every rendered file inside the workflow's boundary (module docs): the parts the
    /// verb receives, or the task's refusal (`note` names the task's lane).
    pub(super) async fn load_vision(
        &self,
        rendered: Vec<Rendered>,
        note: &str,
    ) -> Result<Vec<VisionPart>, Box<Dispatched>> {
        let mut parts = Vec::with_capacity(rendered.len());
        for entry in rendered {
            parts.push(match entry {
                Rendered::Url(url) => VisionPart::url(url),
                Rendered::File(path) => {
                    let bytes = self.read_vision_file(&path, note).await?;
                    VisionPart::loaded(path, bytes)
                }
            });
        }
        Ok(parts)
    }

    /// One file through `nika:read` (`binary: true`) on the runtime's own invoke verb.
    async fn read_vision_file(&self, path: &str, note: &str) -> Result<Vec<u8>, Box<Dispatched>> {
        if path.trim().is_empty() {
            return Err(unreadable(
                note,
                NOT_FOUND.to_owned(),
                "the path rendered empty, so it names no file",
                false,
            ));
        }
        let mut call = InvokeInput::new(READ);
        call.args = serde_json::json!({ "path": path, "binary": true });
        let read = match self.invoke.run(call).await {
            Ok(read) => read,
            Err(err) if err.spec_code() == OUTSIDE => {
                return Err(Box::new(Dispatched::security_err(
                    note,
                    format!(
                        "a `vision:` file is a read inside permits.fs.read: {}",
                        refusal(&err)
                    ),
                )));
            }
            // The read's own failure, under its own code: the voice `nika:read` itself
            // fails with (`on_codes:` names it), never an engine-internal number.
            Err(err) => {
                return Err(unreadable(
                    note,
                    err.spec_code(),
                    &format!("`{path}`: {}", refusal(&err)),
                    err.is_transient(),
                ));
            }
        };
        let encoded = read
            .structured
            .as_ref()
            .and_then(|value| value.get("bytes_base64"))
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| {
                unreadable(
                    note,
                    READ_FAILED.to_owned(),
                    &format!("`{path}`: `{READ}` returned no bytes"),
                    false,
                )
            })?;
        base64::engine::general_purpose::STANDARD
            .decode(encoded)
            .map_err(|err| {
                unreadable(
                    note,
                    READ_FAILED.to_owned(),
                    &format!("`{path}`: {err}"),
                    false,
                )
            })
    }
}

/// The reason the builtin reported, without its code prefix (`<code> · `): the boundary's
/// sentence names the path, where it resolves and the grant. Any other failure (the tool did
/// not resolve, the dispatch failed) speaks as itself.
fn refusal(err: &VerbInvokeError) -> String {
    let reported = match err {
        VerbInvokeError::ToolReportedError { content_tail, .. }
        | VerbInvokeError::ToolReportedDetailed { content_tail, .. } => content_tail.as_str(),
        other => return other.to_string(),
    };
    let code = err.spec_code();
    reported
        .strip_prefix(&format!("{code} · "))
        .unwrap_or(reported)
        .to_owned()
}

/// A file the run cannot read (or a path naming no file): the task fails with the read's code
/// (`code`, canonical: spec 05 §Taxonomy ownership keeps engine-internal numbers out of
/// `tasks.X.error`), and never a provider call (#1135).
fn unreadable(note: &str, code: String, detail: &str, transient: bool) -> Box<Dispatched> {
    Box::new(Dispatched {
        note: note.to_owned(),
        result: Err(FailedDispatch::unspent(TaskErrorRecord::new(
            code,
            format!("a `vision:` file cannot be read: {detail}"),
            transient,
        ))),
    })
}

#[cfg(test)]
mod tests;
