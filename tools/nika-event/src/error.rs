// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

use thiserror::Error;

/// Errors that can occur in the event system.
#[derive(Debug, Error)]
pub enum EventError {
    #[error("Failed to write trace: {0}")]
    TraceWrite(#[from] std::io::Error),

    #[error("Failed to serialize event: {0}")]
    Serialization(#[from] serde_json::Error),
}

/// Result type for event operations.
pub type Result<T> = std::result::Result<T, EventError>;
