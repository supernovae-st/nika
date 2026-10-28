// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! HTTP client exec programs — the native-first/001 program set as a
//! capability fact (issue 1597).
//!
//! An `exec:` of one of these reaches the network through the run's egress
//! proxy, which admits only `permits.net.http`. The hint classifier and the
//! permits inference walk this table, so the program a hint calls an HTTP
//! fetch is the program whose computed URL leaves the inferred net face
//! partial.

/// Programs native-first/001 names as HTTP clients (bare basenames). ONE
/// list: the hint classifier and the permits inference cannot drift.
pub const HTTP_CLIENT_PROGRAMS: &[&str] = &["curl", "wget", "xh", "http", "https"];
