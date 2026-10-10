// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! A replay trial: a rehearsal lent captures of the candidate's public GET sources. A host
//! observed each page before the trial and lends its exact bytes; the room's fetch plane answers
//! a GET whose address is exactly a capture's with that capture, and refuses every other request
//! as before, with no socket opened and no name resolved. A capture answers only the address it
//! was taken at, compared byte for byte: no scheme case, trailing slash, query or fragment is
//! normalized. The captures are bounded as the host's observation is, one body and all together.
//! What the trial cannot exercise is named before the run by [`screen`](fn@screen), and
//! [`settle`](fn@settle) reads the run without holding it against the candidate.

use std::collections::BTreeMap;
use std::fmt;

use bytes::Bytes;
use nika_event::source_id::sha256_hex;
use nika_schema::raw::RawWorkflow;
use nika_schema::{FileId, ParseMode};

mod screen;
mod settle;
#[cfg(test)]
mod tests;

pub use screen::{ReplayScreen, screen, sources};

/// `candidate` read strictly, as a trial admits it; `None` when it does not parse.
#[must_use]
pub fn parsed(candidate: &str) -> Option<RawWorkflow> {
    nika_schema::parse(candidate, FileId::new(0), ParseMode::Strict).ok()
}
pub use settle::{KEPT_NOTHING, Settled, settle, settle_into};

/// One page a host observed before a trial: where, what it answered, and its exact bytes.
#[derive(Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Capture {
    url: String,
    status: u16,
    content_type: Option<String>,
    body: Bytes,
    sha256: String,
    captured_at_ms: u64,
}

impl Capture {
    /// The page at `url` answered `status`, its `content_type` when it named one, and `body`,
    /// observed at `captured_at_ms` (milliseconds since the Unix epoch). The body's sha256 is
    /// computed here.
    #[must_use]
    pub fn new(
        url: impl Into<String>,
        status: u16,
        content_type: Option<String>,
        body: Vec<u8>,
        captured_at_ms: u64,
    ) -> Self {
        let sha256 = sha256_hex(&body);
        Self {
            url: url.into(),
            status,
            content_type,
            body: Bytes::from(body),
            sha256,
            captured_at_ms,
        }
    }

    /// The address the page was observed at, exactly as a trial must request it.
    #[must_use]
    pub fn url(&self) -> &str {
        &self.url
    }

    /// The status the page answered.
    #[must_use]
    pub fn status(&self) -> u16 {
        self.status
    }

    /// The content type the page named, when it named one.
    #[must_use]
    pub fn content_type(&self) -> Option<&str> {
        self.content_type.as_deref()
    }

    /// The exact bytes the page answered.
    #[must_use]
    pub fn body(&self) -> &[u8] {
        &self.body
    }

    /// The bytes a replayed GET answers with: the capture's own, shared, never copied.
    pub(super) fn served(&self) -> Bytes {
        self.body.clone()
    }

    /// The sha256 of the body, in lowercase hex.
    #[must_use]
    pub fn sha256(&self) -> &str {
        &self.sha256
    }

    /// When the page was observed, in milliseconds since the Unix epoch.
    #[must_use]
    pub fn captured_at_ms(&self) -> u64 {
        self.captured_at_ms
    }
}

impl fmt::Debug for Capture {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Capture")
            .field("url", &self.url)
            .field("status", &self.status)
            .field("content_type", &self.content_type)
            .field("body", &format_args!("<{} bytes>", self.body.len()))
            .field("sha256", &self.sha256)
            .field("captured_at_ms", &self.captured_at_ms)
            .finish()
    }
}

/// The captures one trial is lent, by exact address. A capture of an address already held
/// replaces it.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Captures {
    held: BTreeMap<String, Capture>,
    bytes: usize,
}

impl Captures {
    /// The bytes one capture's body may hold: the host's own observation bound.
    pub const MAX_CAPTURE_BYTES: usize = 512 * 1024;
    /// The bytes every body may hold together.
    pub const MAX_TOTAL_BYTES: usize = 4 * 1024 * 1024;
    /// The addresses one trial may be lent.
    pub const MAX_CAPTURES: usize = 16;

    /// No capture.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Lend `capture` too. A refused capture leaves the captures as they were.
    ///
    /// # Errors
    /// [`CaptureRefused`] when its address is not an `http://` or `https://` one, its body is
    /// over [`Self::MAX_CAPTURE_BYTES`], [`Self::MAX_CAPTURES`] other addresses are held
    /// already, or the bodies together would pass [`Self::MAX_TOTAL_BYTES`].
    pub fn insert(&mut self, capture: Capture) -> Result<(), CaptureRefused> {
        if !web_address(&capture.url) {
            return Err(CaptureRefused::Address);
        }
        let bytes = capture.body.len();
        if bytes > Self::MAX_CAPTURE_BYTES {
            return Err(CaptureRefused::Body { bytes });
        }
        let replaced = self.held.get(&capture.url).map(|held| held.body.len());
        if replaced.is_none() && self.held.len() >= Self::MAX_CAPTURES {
            return Err(CaptureRefused::Count);
        }
        let total = (self.bytes.saturating_sub(replaced.unwrap_or(0))).saturating_add(bytes);
        if total > Self::MAX_TOTAL_BYTES {
            return Err(CaptureRefused::Total { bytes: total });
        }
        self.bytes = total;
        self.held.insert(capture.url.clone(), capture);
        Ok(())
    }

    /// The capture taken at exactly `url`, compared byte for byte and never normalized.
    #[must_use]
    pub fn get(&self, url: &str) -> Option<&Capture> {
        self.held.get(url)
    }

    /// Every capture, in address order.
    #[must_use]
    pub fn iter(&self) -> impl ExactSizeIterator<Item = &Capture> {
        self.held.values()
    }

    /// How many addresses are lent.
    #[must_use]
    pub fn len(&self) -> usize {
        self.held.len()
    }

    /// Whether no address is lent.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.held.is_empty()
    }
}

/// Why a capture was not lent. Nothing changed.
///
/// A verdict, not a coded error: the lending host reports it in its own words.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum CaptureRefused {
    /// The address is not an `http://` or `https://` one.
    Address,
    /// The body is over [`Captures::MAX_CAPTURE_BYTES`].
    Body {
        /// The body's bytes.
        bytes: usize,
    },
    /// [`Captures::MAX_CAPTURES`] other addresses are lent already.
    Count,
    /// The bodies together would pass [`Captures::MAX_TOTAL_BYTES`].
    Total {
        /// The bytes they would hold.
        bytes: usize,
    },
}

impl fmt::Display for CaptureRefused {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Address => {
                f.write_str("a capture is lent for an http:// or https:// address only")
            }
            Self::Body { bytes } => write!(
                f,
                "a capture of {bytes} bytes passes the {} bytes one capture holds",
                Captures::MAX_CAPTURE_BYTES
            ),
            Self::Count => write!(
                f,
                "a trial is lent {} captures at most",
                Captures::MAX_CAPTURES
            ),
            Self::Total { bytes } => write!(
                f,
                "the captures would hold {bytes} bytes, over the {} bytes a trial is lent",
                Captures::MAX_TOTAL_BYTES
            ),
        }
    }
}

/// Whether `url` is an `http://` or `https://` address that names something after its scheme.
fn web_address(url: &str) -> bool {
    (url.strip_prefix("https://"))
        .or_else(|| url.strip_prefix("http://"))
        .is_some_and(|rest| !rest.is_empty())
}
