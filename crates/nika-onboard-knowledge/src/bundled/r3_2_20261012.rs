// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The retained r3.2 payload (`knowledge-0.123.0-r3.2-foundry-20261012`, profile r2, `policy-r2`),
//! current before the r3.5 Foundry release and embedded whole from its directory. Records pinned to
//! it reopen these exact bytes; new authoring never selects them.

use std::collections::BTreeMap;

use include_dir::{Dir, include_dir};

static PAYLOAD: Dir<'static> =
    include_dir!("$CARGO_MANIFEST_DIR/assets/knowledge-release-0.123.0-r3.2-foundry-20261012");

pub(super) const SNAPSHOT_SHA256: &str =
    "8d52df01a5105fc4661851218e89ca172c5799bf3016aedbce1574ecac096075";

/// Every embedded file, by its path relative to the payload root (`/`-separated).
pub(super) fn files() -> BTreeMap<String, Vec<u8>> {
    super::current::walk(&PAYLOAD)
}
