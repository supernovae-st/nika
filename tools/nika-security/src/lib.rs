// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Nika security validators (L1, pure sync, zero I/O).
//!
//! Exec command blocklist, SEC-2 binding injection scan, NFKC normalization,
//! environment variable validation, and artifact path boundary checks.
//!
//! All functions are synchronous, allocation-light, and depend only on
//! `nika-core` + `unicode-normalization`. No tokio, no reqwest, no async —
//! callers can invoke these validators from any layer without pulling a
//! runtime.
