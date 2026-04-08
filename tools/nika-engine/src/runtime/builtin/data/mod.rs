// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Data processing builtin tools — mechanical operations without LLM.
//!
//! Split into categorized modules:
//! - `merge` — json_merge, set_diff, zip
//! - `transform` — map, filter, enrich
//! - `aggregate` — group_by, tree_data
//! - `jq` — jq (full jq stdlib via jaq-core)
//! - `text` — chunk, token_count
//! - `io` — inject (template marker replacement)

mod aggregate;
mod io;
mod jq;
mod json_diff;
mod merge;
mod text;
mod transform;

pub use aggregate::{GroupByTool, TreeDataTool};
pub use io::InjectTool;
pub use jq::JqTool;
pub use json_diff::JsonDiffTool;
pub use merge::{JsonMergeTool, SetDiffTool, ZipTool};
pub use text::{ChunkTool, TokenCountTool};
pub use transform::{EnrichTool, FilterTool, MapTool};
