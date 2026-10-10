// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Where a conversation's tree is kept: one file beside the conversation history under the home
//! (`.nika/sessions/<project digest>/tree.jsonl`), never in the project, or in memory when the
//! Session keeps no history. Each line is appended and synced before the tree takes its entry;
//! the person's lines are indexed as they become durable, so the tools check a citation against
//! what the tree holds while the loop holds the tree.

use std::io;
use std::sync::{Arc, Mutex, PoisonError};

use nika_fs::OwnedDir;
use nika_session_agent::Store;
use nika_session_agent::tree::read_line;

use nika_session_agent::conversation::Citations;

/// The tree's file in the history directory.
pub(crate) const TREE_FILE: &str = "tree.jsonl";

/// A tree's lines made durable, its citations indexed.
pub(crate) struct TreeFile {
    target: Target,
    citations: Arc<Mutex<Citations>>,
}

enum Target {
    /// The history's own directory, its descriptor held.
    Home(OwnedDir),
    /// No history: the lines are kept in memory for this Session only.
    Memory(Vec<String>),
}

impl TreeFile {
    /// The tree kept in the history directory `dir`.
    pub(crate) fn home(dir: OwnedDir) -> Self {
        Self {
            target: Target::Home(dir),
            citations: Arc::default(),
        }
    }

    /// A tree no file keeps.
    pub(crate) fn memory() -> Self {
        Self {
            target: Target::Memory(Vec::new()),
            citations: Arc::default(),
        }
    }

    /// The citations, shared with the tools.
    pub(crate) fn citations(&self) -> Arc<Mutex<Citations>> {
        Arc::clone(&self.citations)
    }

    /// The tree's text as kept, for a reopen; none when nothing was kept yet.
    ///
    /// # Errors
    ///
    /// The file exists and cannot be read.
    pub(crate) fn text(&self) -> io::Result<Option<String>> {
        match &self.target {
            Target::Home(dir) => dir.read_optional(TREE_FILE),
            Target::Memory(lines) if lines.is_empty() => Ok(None),
            Target::Memory(lines) => Ok(Some(lines.join("\n") + "\n")),
        }
    }

    /// Index the lines of a tree read back, as if they had just been written.
    pub(crate) fn index(&mut self, text: &str) {
        for line in text.lines() {
            self.record(line);
        }
    }

    fn record(&mut self, line: &str) {
        if let Some(read) = read_line(line) {
            let person = read.person.map(|p| (p.cite, p.text));
            (self.citations.lock())
                .unwrap_or_else(PoisonError::into_inner)
                .record(read.n, person);
        }
    }
}

impl Store for TreeFile {
    fn append(&mut self, line: &str) -> io::Result<()> {
        match &mut self.target {
            Target::Home(dir) => dir.append_line(TREE_FILE, line)?,
            Target::Memory(lines) => lines.push(line.to_owned()),
        }
        self.record(line);
        Ok(())
    }
}
