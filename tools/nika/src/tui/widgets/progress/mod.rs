//! Progress Widgets
//!
//! Reusable progress bar widgets for the TUI:
//! - DownloadProgress: Multi-file download progress with speed/ETA
//! - SyncProgress: Editor synchronization status
//! - TaskProgress: Background job progress tracking
//!
//! All widgets use Solarized color palette and ratatui's Gauge base.

mod download_progress;
mod sync_progress;
mod task_progress;

pub use download_progress::{DownloadProgress, DownloadStatus};
pub use sync_progress::{EditorSyncState, SyncProgress, SyncStatus};
pub use task_progress::{TaskProgress, TaskProgressStatus};
