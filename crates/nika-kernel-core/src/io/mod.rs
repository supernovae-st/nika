// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! I/O traits — filesystem, HTTP, shell process, blob storage, clock, screen capture.
//!
//! Future sub-crate: `nika-kernel-io` (when kernel exceeds 10k LOC or 50 traits).

pub mod a11y;
pub mod blob;
pub mod browser;
pub mod clock;
pub mod command_sandbox;
pub mod fs;
pub mod http;
pub mod input;
pub mod ocr;
pub mod process;
pub mod screen;
