// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Size caps for builtin file tools.
//!
//! S12.P2 — single source of truth for the read/write byte limits. Both
//! are `u64` so 32-bit targets cannot clip an oversized file back into
//! the allowed range via an `as usize` truncation.

/// Max file size accepted by `nika:read`.
///
/// DoS pre-check (S11.A3): the file is `stat`-ed before `read_to_string`
/// so a 2 GB file cannot be loaded fully into memory just to be trimmed
/// by offset/limit. Mirrors the pre-S10 engine-side `ReadTool` limit.
pub(crate) const MAX_READ_BYTES: u64 = 50 * 1024 * 1024;

/// Max payload size accepted by `nika:write`, in bytes.
pub(crate) const MAX_WRITE_BYTES: u64 = 10 * 1024 * 1024;
