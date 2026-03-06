//! Document Store - In-memory cache for open documents
//!
//! Manages the text content of open `.nika.yaml` files, supporting
//! incremental updates from the LSP client.

use std::collections::HashMap;

#[cfg(feature = "lsp")]
use tower_lsp::lsp_types::{TextDocumentContentChangeEvent, Url};

#[cfg(feature = "lsp")]
use super::conversion::position_to_offset;

/// In-memory store for open document contents
///
/// Tracks the latest text content of each open document, applying
/// incremental changes as they arrive from the LSP client.
#[derive(Debug, Default)]
pub struct DocumentStore {
    /// Map from document URI to current text content
    #[cfg(feature = "lsp")]
    documents: HashMap<Url, String>,
    #[cfg(not(feature = "lsp"))]
    documents: HashMap<String, String>,
}

impl DocumentStore {
    /// Create a new empty document store
    pub fn new() -> Self {
        Self {
            documents: HashMap::new(),
        }
    }

    /// Insert or replace a document's content
    #[cfg(feature = "lsp")]
    pub fn insert(&mut self, uri: Url, content: String) {
        self.documents.insert(uri, content);
    }

    /// Get a document's current content
    #[cfg(feature = "lsp")]
    pub fn get(&self, uri: &Url) -> Option<&String> {
        self.documents.get(uri)
    }

    /// Remove a document from the store
    #[cfg(feature = "lsp")]
    pub fn remove(&mut self, uri: &Url) -> Option<String> {
        self.documents.remove(uri)
    }

    /// Check if a document is open
    #[cfg(feature = "lsp")]
    pub fn contains(&self, uri: &Url) -> bool {
        self.documents.contains_key(uri)
    }

    /// Apply an incremental change to a document
    ///
    /// Handles both full-document and range-based changes.
    /// Returns true if the document was found and updated, false otherwise.
    #[cfg(feature = "lsp")]
    pub fn apply_change(&mut self, uri: &Url, change: TextDocumentContentChangeEvent) -> bool {
        if let Some(content) = self.documents.get_mut(uri) {
            match change.range {
                Some(range) => {
                    // Incremental change - replace the specified range
                    let start_offset = position_to_offset(range.start, content);
                    let end_offset = position_to_offset(range.end, content);

                    // Ensure valid offsets (start <= end <= content.len())
                    let start_offset = start_offset.min(content.len());
                    let end_offset = end_offset.min(content.len()).max(start_offset);

                    // Build new content
                    let mut new_content = String::with_capacity(
                        content.len() - (end_offset - start_offset) + change.text.len(),
                    );
                    new_content.push_str(&content[..start_offset]);
                    new_content.push_str(&change.text);
                    new_content.push_str(&content[end_offset..]);

                    *content = new_content;
                }
                None => {
                    // Full document replacement
                    *content = change.text;
                }
            }
            true
        } else {
            false
        }
    }

    /// Get the number of open documents
    pub fn len(&self) -> usize {
        self.documents.len()
    }

    /// Check if the store is empty
    pub fn is_empty(&self) -> bool {
        self.documents.is_empty()
    }

    /// Get all document URIs
    #[cfg(feature = "lsp")]
    pub fn uris(&self) -> impl Iterator<Item = &Url> {
        self.documents.keys()
    }
}

// Stub implementations when LSP feature is disabled
#[cfg(not(feature = "lsp"))]
impl DocumentStore {
    pub fn insert(&mut self, uri: String, content: String) {
        self.documents.insert(uri, content);
    }

    pub fn get(&self, uri: &str) -> Option<&String> {
        self.documents.get(uri)
    }

    pub fn remove(&mut self, uri: &str) -> Option<String> {
        self.documents.remove(uri)
    }

    pub fn contains(&self, uri: &str) -> bool {
        self.documents.contains_key(uri)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_store_is_empty() {
        let store = DocumentStore::new();
        assert!(store.is_empty());
        assert_eq!(store.len(), 0);
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn test_insert_and_get() {
        let mut store = DocumentStore::new();
        let uri = Url::parse("file:///test.nika.yaml").unwrap();
        let content = "schema: nika/workflow@0.9".to_string();

        store.insert(uri.clone(), content.clone());

        assert!(!store.is_empty());
        assert_eq!(store.len(), 1);
        assert!(store.contains(&uri));
        assert_eq!(store.get(&uri), Some(&content));
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn test_remove() {
        let mut store = DocumentStore::new();
        let uri = Url::parse("file:///test.nika.yaml").unwrap();

        store.insert(uri.clone(), "content".to_string());
        assert!(store.contains(&uri));

        let removed = store.remove(&uri);
        assert_eq!(removed, Some("content".to_string()));
        assert!(!store.contains(&uri));
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn test_apply_full_change() {
        use tower_lsp::lsp_types::TextDocumentContentChangeEvent;

        let mut store = DocumentStore::new();
        let uri = Url::parse("file:///test.nika.yaml").unwrap();

        store.insert(uri.clone(), "old content".to_string());

        // Full replacement (no range)
        let change = TextDocumentContentChangeEvent {
            range: None,
            range_length: None,
            text: "new content".to_string(),
        };

        store.apply_change(&uri, change);
        assert_eq!(store.get(&uri), Some(&"new content".to_string()));
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn test_apply_incremental_change() {
        use tower_lsp::lsp_types::{Position, Range, TextDocumentContentChangeEvent};

        let mut store = DocumentStore::new();
        let uri = Url::parse("file:///test.nika.yaml").unwrap();

        store.insert(uri.clone(), "hello world".to_string());

        // Replace "world" with "rust"
        let change = TextDocumentContentChangeEvent {
            range: Some(Range {
                start: Position {
                    line: 0,
                    character: 6,
                },
                end: Position {
                    line: 0,
                    character: 11,
                },
            }),
            range_length: None,
            text: "rust".to_string(),
        };

        store.apply_change(&uri, change);
        assert_eq!(store.get(&uri), Some(&"hello rust".to_string()));
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn test_apply_incremental_insert() {
        use tower_lsp::lsp_types::{Position, Range, TextDocumentContentChangeEvent};

        let mut store = DocumentStore::new();
        let uri = Url::parse("file:///test.nika.yaml").unwrap();

        store.insert(uri.clone(), "helloworld".to_string());

        // Insert " " between "hello" and "world"
        let change = TextDocumentContentChangeEvent {
            range: Some(Range {
                start: Position {
                    line: 0,
                    character: 5,
                },
                end: Position {
                    line: 0,
                    character: 5,
                },
            }),
            range_length: None,
            text: " ".to_string(),
        };

        store.apply_change(&uri, change);
        assert_eq!(store.get(&uri), Some(&"hello world".to_string()));
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn test_apply_incremental_delete() {
        use tower_lsp::lsp_types::{Position, Range, TextDocumentContentChangeEvent};

        let mut store = DocumentStore::new();
        let uri = Url::parse("file:///test.nika.yaml").unwrap();

        store.insert(uri.clone(), "hello world".to_string());

        // Delete " world"
        let change = TextDocumentContentChangeEvent {
            range: Some(Range {
                start: Position {
                    line: 0,
                    character: 5,
                },
                end: Position {
                    line: 0,
                    character: 11,
                },
            }),
            range_length: None,
            text: "".to_string(),
        };

        store.apply_change(&uri, change);
        assert_eq!(store.get(&uri), Some(&"hello".to_string()));
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn test_apply_multiline_change() {
        use tower_lsp::lsp_types::{Position, Range, TextDocumentContentChangeEvent};

        let mut store = DocumentStore::new();
        let uri = Url::parse("file:///test.nika.yaml").unwrap();

        store.insert(uri.clone(), "line1\nline2\nline3".to_string());

        // Replace "line2" with "replaced"
        let change = TextDocumentContentChangeEvent {
            range: Some(Range {
                start: Position {
                    line: 1,
                    character: 0,
                },
                end: Position {
                    line: 1,
                    character: 5,
                },
            }),
            range_length: None,
            text: "replaced".to_string(),
        };

        store.apply_change(&uri, change);
        assert_eq!(store.get(&uri), Some(&"line1\nreplaced\nline3".to_string()));
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn test_multiple_documents() {
        let mut store = DocumentStore::new();
        let uri1 = Url::parse("file:///test1.nika.yaml").unwrap();
        let uri2 = Url::parse("file:///test2.nika.yaml").unwrap();

        store.insert(uri1.clone(), "content1".to_string());
        store.insert(uri2.clone(), "content2".to_string());

        assert_eq!(store.len(), 2);
        assert_eq!(store.get(&uri1), Some(&"content1".to_string()));
        assert_eq!(store.get(&uri2), Some(&"content2".to_string()));
    }
}
