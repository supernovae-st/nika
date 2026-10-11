// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The retained r2 payload (`knowledge-0.123.0-r2-json-filter-records-20261009`, profile r2,
//! `policy-r2`), current before the r3 Foundry release and embedded whole from its directory.
//! Earlier r2 pins reopen these exact bytes; new authoring never selects them.

use std::collections::BTreeMap;

use include_dir::{Dir, include_dir};

static PAYLOAD: Dir<'static> = include_dir!("$CARGO_MANIFEST_DIR/assets/knowledge-release-r2");

pub(super) const SNAPSHOT_SHA256: &str =
    "1be7d6101dab9eff54f35be07463f4e320607c4a837d04ef90519abaadf48166";

/// Every embedded file, by its path relative to the payload root (`/`-separated).
pub(super) fn files() -> BTreeMap<String, Vec<u8>> {
    super::current::walk(&PAYLOAD)
}
