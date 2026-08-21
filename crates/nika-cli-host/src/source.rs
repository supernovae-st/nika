// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Workflow bytes plus the provenance that governs `--fix` / clean-gate.

use nika_display::check_render::RepairTarget;

use crate::output::VerbOutput;
use crate::repair::repair_target_for_path;

#[derive(Clone)]
pub struct RunSource {
    logical_path: std::sync::Arc<str>,
    source: std::sync::Arc<str>,
    repair_target: RepairTarget,
}

impl RunSource {
    /// # Errors
    /// Unreadable path is an environment refusal; non-UTF-8 is a parse refusal.
    pub fn capture(path: &str) -> Result<Self, VerbOutput> {
        Self::capture_with_repair_target(path, repair_target_for_path(path))
    }

    /// # Errors
    /// Unreadable path is an environment refusal; non-UTF-8 is a parse refusal.
    pub fn capture_with_repair_target(
        path: &str,
        repair_target: RepairTarget,
    ) -> Result<Self, VerbOutput> {
        let bytes = if path == "-" {
            use std::io::Read as _;
            let mut buf = Vec::new();
            std::io::stdin()
                .read_to_end(&mut buf)
                .map_err(|e| VerbOutput::env(format!("cannot read stdin: {e}")))?;
            buf
        } else {
            std::fs::read(path).map_err(|e| VerbOutput::env(format!("cannot read {path}: {e}")))?
        };
        Self::from_bytes_with_repair_target(path, bytes, repair_target).map_err(|_| {
            VerbOutput::file(format!(
                "PARSE ✗  {}",
                nika_schema::SchemaError::YamlSyntax {
                    message: "workflow source is not valid UTF-8".to_owned(),
                    span: None,
                }
                .diagnostic()
            ))
        })
    }

    /// # Errors
    /// Fails when `bytes` are not valid UTF-8.
    pub fn from_bytes(logical_path: impl Into<String>, bytes: Vec<u8>) -> std::io::Result<Self> {
        Self::from_bytes_with_repair_target(logical_path, bytes, RepairTarget::WorkspaceFile)
    }

    fn from_bytes_with_repair_target(
        logical_path: impl Into<String>,
        bytes: Vec<u8>,
        repair_target: RepairTarget,
    ) -> std::io::Result<Self> {
        let source = String::from_utf8(bytes).map_err(|error| {
            std::io::Error::new(std::io::ErrorKind::InvalidData, error.utf8_error())
        })?;
        Ok(Self {
            logical_path: std::sync::Arc::from(logical_path.into()),
            source: std::sync::Arc::from(source),
            repair_target,
        })
    }

    #[must_use]
    pub fn logical_path(&self) -> &str {
        &self.logical_path
    }

    #[must_use]
    pub fn source(&self) -> &str {
        &self.source
    }

    #[must_use]
    pub fn repair_target(&self) -> RepairTarget {
        self.repair_target
    }
}
