// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The address of one node of a document: the mapping keys and sequence
//! indices from the root, never a text search. A key that reads like a
//! path inside a prompt, a command or a data value is one leaf value, so no
//! path can address a substring of it.

use std::fmt;

/// One node's address: mapping keys and sequence indices (decimal) from the
/// root. The root itself is the empty path.
#[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub struct Path {
    segments: Vec<String>,
}

impl Path {
    /// The document root.
    #[must_use]
    pub fn root() -> Self {
        Self::default()
    }

    /// A path of these segments, in order from the root.
    #[must_use]
    pub fn new<I, S>(segments: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self {
            segments: segments.into_iter().map(Into::into).collect(),
        }
    }

    /// An RFC 6901 JSON pointer (`/tasks/fetch/retry/max_attempts`; `""` is
    /// the root; `~1` is `/` and `~0` is `~` inside a segment). `None` when
    /// the text is not a pointer or carries an unknown `~` escape.
    #[must_use]
    pub fn pointer(text: &str) -> Option<Self> {
        if text.is_empty() {
            return Some(Self::root());
        }
        let rest = text.strip_prefix('/')?;
        let mut segments = Vec::new();
        for raw in rest.split('/') {
            let mut segment = String::with_capacity(raw.len());
            let mut chars = raw.chars();
            while let Some(c) = chars.next() {
                if c == '~' {
                    match chars.next() {
                        Some('0') => segment.push('~'),
                        Some('1') => segment.push('/'),
                        _ => return None,
                    }
                } else {
                    segment.push(c);
                }
            }
            segments.push(segment);
        }
        Some(Self { segments })
    }

    /// A dotted path (`tasks.fetch.retry.max_attempts`): every segment is
    /// non-empty and holds no dot. `None` for an empty text or segment; use
    /// [`Path::pointer`] for a key that holds a dot.
    #[must_use]
    pub fn dotted(text: &str) -> Option<Self> {
        let segments: Vec<String> = text.split('.').map(str::to_owned).collect();
        segments
            .iter()
            .all(|s| !s.is_empty())
            .then_some(Self { segments })
    }

    /// The segments, in order from the root.
    #[must_use]
    pub fn segments(&self) -> &[String] {
        &self.segments
    }

    /// This path extended by one segment.
    #[must_use]
    pub fn child(&self, segment: impl Into<String>) -> Self {
        let mut segments = self.segments.clone();
        segments.push(segment.into());
        Self { segments }
    }

    /// The path of the enclosing node; `None` at the root.
    #[must_use]
    pub fn parent(&self) -> Option<Self> {
        let (_, init) = self.segments.split_last()?;
        Some(Self {
            segments: init.to_vec(),
        })
    }

    /// The last segment; `None` at the root.
    #[must_use]
    pub fn last(&self) -> Option<&str> {
        self.segments.last().map(String::as_str)
    }

    /// Whether this is the document root.
    #[must_use]
    pub fn is_root(&self) -> bool {
        self.segments.is_empty()
    }

    /// Whether `prefix` addresses this node or one of its ancestors.
    #[must_use]
    pub fn starts_with(&self, prefix: &Self) -> bool {
        self.segments.starts_with(&prefix.segments)
    }

    /// The RFC 6901 pointer form (`""` for the root).
    #[must_use]
    pub fn to_pointer(&self) -> String {
        let mut out = String::new();
        for segment in &self.segments {
            out.push('/');
            out.push_str(&segment.replace('~', "~0").replace('/', "~1"));
        }
        out
    }
}

impl fmt::Display for Path {
    /// The pointer form; the root reads `/` so a message never names an empty path.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_root() {
            f.write_str("/")
        } else {
            f.write_str(&self.to_pointer())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Path;

    #[test]
    fn pointer_and_dotted_forms_address_the_same_segments() {
        let pointer = Path::pointer("/tasks/fetch/retry/max_attempts").expect("pointer");
        let dotted = Path::dotted("tasks.fetch.retry.max_attempts").expect("dotted");
        assert_eq!(pointer, dotted);
        assert_eq!(pointer.to_pointer(), "/tasks/fetch/retry/max_attempts");
        assert_eq!(pointer.segments().len(), 4);
    }

    #[test]
    fn escapes_round_trip_and_unknown_escapes_are_refused() {
        let path = Path::pointer("/args/a~1b/c~0d").expect("pointer");
        assert_eq!(path.segments(), ["args", "a/b", "c~d"]);
        assert_eq!(path.to_pointer(), "/args/a~1b/c~0d");
        assert_eq!(Path::pointer("/a/~2"), None);
        assert_eq!(Path::pointer("no-slash"), None);
        assert_eq!(Path::pointer(""), Some(Path::root()));
    }

    #[test]
    fn dotted_refuses_empty_segments() {
        assert_eq!(Path::dotted(""), None);
        assert_eq!(Path::dotted("a..b"), None);
        assert_eq!(Path::dotted(".a"), None);
    }

    #[test]
    fn family_relations_follow_segments_not_text() {
        let task = Path::new(["tasks", "fetch"]);
        let retry = task.child("retry");
        assert!(retry.starts_with(&task));
        assert!(!Path::new(["tasks", "fetch_all"]).starts_with(&task));
        assert_eq!(retry.parent(), Some(task));
        assert_eq!(retry.last(), Some("retry"));
        assert_eq!(Path::root().parent(), None);
        assert_eq!(Path::root().to_string(), "/");
    }
}
