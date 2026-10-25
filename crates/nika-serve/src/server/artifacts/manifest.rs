// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! What a settled job's journal records as its artifacts, read from the events alone (pure).
//!
//! Two event shapes record a written byte stream with its content hash:
//! - `task_completed` of a builtin that saves files and reports each one's path, size and
//!   sha256 (`nika:chart` · `nika:image_generate` · `nika:tts_generate` · `nika:image_fx`), on
//!   its own successful invocation (`outcome` class `success`, cause `normal`: never a
//!   recovered or author-shaped value);
//! - `agent_image_observed` of an image the harness sent, stored as a blob under
//!   `.nika/blobs` (`storage: stored`) with the sha256 of the bytes received.
//!
//! `nika:write` and `nika:edit` journal no hash of what they wrote, so their files are not
//! artifacts. A file qualifies only by a relative path inside the project, outside `.nika`;
//! its last journaled write wins. Every number and digest here is the journal's own; the
//! download route reads the bytes and checks them against it.

use std::collections::BTreeMap;
use std::path::{Component, Path};

use nika_event::{Event, EventKind};
use serde::Serialize;
use serde_json::Value;

/// The longest project-relative path an artifact may carry (the snapshot path bound).
const MAX_PATH_BYTES: usize = crate::MAX_EXECUTION_SNAPSHOT_PATH_BYTES;

/// The directory the harness image store keeps its blobs under, from the run root, sharded
/// `<2 hex>/<62 hex>` (`nika_runtime_laws::image_room` over `nika_blob::FsBlobStore`; a test
/// pins both sources).
pub(super) const BLOB_ROOT: &str = ".nika/blobs";

/// What an artifact is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum Kind {
    /// A file a builtin saved inside the project.
    File,
    /// An image an agent's harness sent, kept as a blob.
    AgentImage,
}

impl Kind {
    /// The first segment of every name of this kind.
    const fn prefix(self) -> &'static str {
        match self {
            Self::File => "file",
            Self::AgentImage => "agent_image",
        }
    }
}

/// One artifact the journal records.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Artifact {
    /// Unique within the job: `file/<path>` or `agent_image/<n>.<ext>`.
    pub(super) name: String,
    pub(super) kind: Kind,
    /// Where the bytes are, relative to the project root: the file's own path, or its blob.
    pub(super) at: String,
    pub(super) size: u64,
    /// Lowercase hexadecimal SHA-256 of the recorded bytes.
    pub(super) sha256: String,
    pub(super) mime: String,
    /// The task whose event records it.
    pub(super) task: Option<String>,
    /// The id of that journal event.
    pub(super) event_id: String,
}

impl Artifact {
    /// The project-relative path a file artifact names; `None` for a blob.
    pub(super) fn path(&self) -> Option<&str> {
        (self.kind == Kind::File).then_some(self.at.as_str())
    }
}

/// Every artifact `events` record, in journal order (a rewritten file at its last write).
pub(super) fn derive(events: &[Event]) -> Vec<Artifact> {
    let mut slots: Vec<Option<Artifact>> = Vec::new();
    let mut files: BTreeMap<String, usize> = BTreeMap::new();
    let mut images = 0_usize;
    for event in events {
        match event.kind {
            EventKind::TaskCompleted => {
                for file in written_files(event) {
                    if let Some(earlier) = files.insert(file.at.clone(), slots.len())
                        && let Some(slot) = slots.get_mut(earlier)
                    {
                        *slot = None;
                    }
                    slots.push(Some(file));
                }
            }
            EventKind::AgentImageObserved => {
                if let Some(image) = observed_image(event, images.saturating_add(1)) {
                    images = images.saturating_add(1);
                    slots.push(Some(image));
                }
            }
            _ => {}
        }
    }
    slots.into_iter().flatten().collect()
}

/// The files one `task_completed` frame records, when its tool saves files and the value is
/// the tool's own successful answer.
fn written_files(event: &Event) -> Vec<Artifact> {
    let Some(tool) = event
        .str_field("note")
        .and_then(|note| note.strip_prefix("invoke · "))
    else {
        return Vec::new();
    };
    let Some(outcome) = event
        .str_field("outcome")
        .and_then(|text| serde_json::from_str::<Value>(text).ok())
    else {
        return Vec::new();
    };
    let own_success = outcome["class"] == "success"
        && outcome["cause"] == "normal"
        && outcome["payload"].get("recovered_from").is_none();
    if !own_success {
        return Vec::new();
    }
    let value = &outcome["payload"]["value"];
    // A fan-out's value is one answer per item.
    let answers = value
        .as_array()
        .map_or_else(|| vec![value], |all| all.iter().collect());
    let mut found = Vec::new();
    for answer in answers {
        let saved: Vec<(&Value, &str, Option<&str>)> = match tool {
            "nika:chart" => vec![(answer, "bytes", Some("image/svg+xml"))],
            "nika:image_fx" => vec![(answer, "size_bytes", format_mime(&answer["format"]))],
            "nika:image_generate" => (answer["images"].as_array().into_iter().flatten())
                .map(|image| (image, "size_bytes", None))
                .collect(),
            "nika:tts_generate" => vec![(&answer["audio"], "size_bytes", None)],
            _ => Vec::new(),
        };
        for (object, size_key, mime) in saved {
            found.extend(saved_file(event, object, size_key, mime));
        }
    }
    found
}

/// One saved file's record: a contained relative `path`, its size under `size_key`, its
/// `sha256`, and its media type (`mime`, else the record's `mime_type`, else its extension).
fn saved_file(
    event: &Event,
    object: &Value,
    size_key: &str,
    mime: Option<&str>,
) -> Option<Artifact> {
    let at = contained(object["path"].as_str()?)?;
    let sha256 = digest(object["sha256"].as_str()?)?;
    let size = object[size_key].as_u64()?;
    let mime = mime
        .or_else(|| object["mime_type"].as_str().filter(|text| media_type(text)))
        .map_or_else(|| extension_mime(&at).to_owned(), str::to_owned);
    Some(Artifact {
        name: format!("{}/{at}", Kind::File.prefix()),
        kind: Kind::File,
        at,
        size,
        sha256,
        mime,
        task: event.str_field("task").map(str::to_owned),
        event_id: event.id.uuid.to_string(),
    })
}

/// The `ordinal`-th image an agent's harness sent that the store kept, when this one was.
fn observed_image(event: &Event, ordinal: usize) -> Option<Artifact> {
    let observation: Value = serde_json::from_str(event.str_field("harness_image")?).ok()?;
    if observation["storage"] != "stored" {
        return None;
    }
    let blob = &observation["blob"];
    let hash = blob["hash"].as_str()?.strip_prefix("blake3:")?;
    let size = blob["size"].as_u64()?;
    let sha256 = digest(observation["received_sha256"].as_str()?)?;
    if digest(hash).is_none()
        || observation["received_bytes"]
            .as_u64()
            .is_some_and(|received| received != size)
    {
        return None;
    }
    let (shard, rest) = hash.split_at(2);
    // The harness names the type; only an image type is passed on.
    let mime = (observation["mime_type"].as_str())
        .or_else(|| blob["mime_type"].as_str())
        .filter(|text| media_type(text) && text.starts_with("image/"))
        .unwrap_or("application/octet-stream");
    Some(Artifact {
        name: format!(
            "{}/{ordinal}.{}",
            Kind::AgentImage.prefix(),
            image_extension(mime)
        ),
        kind: Kind::AgentImage,
        at: format!("{BLOB_ROOT}/{shard}/{rest}"),
        size,
        sha256,
        mime: mime.to_owned(),
        task: event.str_field("task").map(str::to_owned),
        event_id: event.id.uuid.to_string(),
    })
}

/// `path` as a normalized project-relative path (`./out/a.png` → `out/a.png`), when it is
/// one: relative, without `..`, outside `.nika` (in any case), within the path bound.
pub(super) fn contained(path: &str) -> Option<String> {
    if path.len() > MAX_PATH_BYTES || path.contains('\0') || path.contains('\\') {
        return None;
    }
    let mut parts = Vec::new();
    for component in Path::new(path).components() {
        match component {
            Component::CurDir => {}
            Component::Normal(part) => parts.push(part.to_str()?),
            _ => return None,
        }
    }
    // `.NIKA` is `.nika` on a case-insensitive filesystem (the macOS default).
    (parts
        .first()
        .is_some_and(|first| !first.eq_ignore_ascii_case(".nika")))
    .then(|| parts.join("/"))
}

/// A canonical lowercase SHA-256 (or BLAKE3) hexadecimal digest.
fn digest(text: &str) -> Option<String> {
    (text.len() == 64
        && text
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)))
    .then(|| text.to_owned())
}

/// Whether `text` is a plain `type/subtype` media type a header can carry.
fn media_type(text: &str) -> bool {
    let token = |part: &str| {
        !part.is_empty()
            && part
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"!#$&-^_.+".contains(&b))
    };
    text.len() <= 127
        && text
            .split_once('/')
            .is_some_and(|(kind, sub)| token(kind) && token(sub))
}

/// The media type of an image format word (`nika:image_fx`'s `format`).
fn format_mime(format: &Value) -> Option<&'static str> {
    match format.as_str()? {
        "png" => Some("image/png"),
        "jpeg" | "jpg" => Some("image/jpeg"),
        "webp" => Some("image/webp"),
        "gif" => Some("image/gif"),
        _ => None,
    }
}

/// The media type a file's extension names; bytes otherwise.
fn extension_mime(path: &str) -> &'static str {
    let extension = path
        .rsplit_once('.')
        .map(|(_, ext)| ext.to_ascii_lowercase());
    match extension.as_deref() {
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("webp") => "image/webp",
        Some("gif") => "image/gif",
        Some("svg") => "image/svg+xml",
        Some("mp3") => "audio/mpeg",
        Some("wav") => "audio/wav",
        Some("ogg" | "opus") => "audio/ogg",
        Some("flac") => "audio/flac",
        Some("aac") => "audio/aac",
        Some("m4a") => "audio/mp4",
        Some("json") => "application/json",
        _ => "application/octet-stream",
    }
}

/// The extension an agent image's name carries.
fn image_extension(mime: &str) -> &'static str {
    match mime {
        "image/png" => "png",
        "image/jpeg" => "jpg",
        "image/webp" => "webp",
        "image/gif" => "gif",
        _ => "bin",
    }
}
