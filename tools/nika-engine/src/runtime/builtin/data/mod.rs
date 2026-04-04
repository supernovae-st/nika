//! Data processing builtin tools — mechanical operations without LLM.
//!
//! Split into categorized modules:
//! - `merge` — json_merge, set_diff, zip
//! - `transform` — map, filter, enrich
//! - `aggregate` — group_by, tree_data
//! - `jq` — jq (full jq stdlib), json_query (deprecated)
//! - `text` — chunk, token_count
//! - `io` — inject (template marker replacement)

mod aggregate;
mod io;
mod jq;
mod merge;
mod text;
mod transform;

pub use aggregate::{GroupByTool, TreeDataTool};
pub use io::InjectTool;
pub use jq::{JqTool, JsonQueryTool};
pub use merge::{JsonMergeTool, SetDiffTool, ZipTool};
pub use text::{ChunkTool, TokenCountTool};
pub use transform::{EnrichTool, FilterTool, MapTool};
