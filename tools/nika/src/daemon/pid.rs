//! PID File Management
//!
//! Ensures single-instance daemon via file locking (flock).
//!
//! ## How It Works
//!
//! 1. Open PID file with exclusive lock
//! 2. Write current PID
//! 3. Keep file open (lock held) for daemon lifetime
//! 4. On drop, lock is released and file removed

use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

#[cfg(unix)]
use nix::fcntl::{Flock, FlockArg};

use crate::error::NikaError;
use crate::error::Result;

/// PID lock guard - holds the lock for the lifetime of the daemon.
///
/// Uses `nix::fcntl::Flock` which auto-unlocks on drop.
pub struct PidLock {
    /// Path to PID file
    path: PathBuf,
    /// Flock guard (auto-unlocks on drop, keeps file handle alive)
    #[cfg(unix)]
    _flock: Flock<File>,
    /// Open file handle (non-unix fallback)
    #[cfg(not(unix))]
    #[allow(dead_code)]
    file: File,
}

impl PidLock {
    /// Get the path to the PID file
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for PidLock {
    fn drop(&mut self) {
        // Remove PID file on drop (best effort)
        let _ = fs::remove_file(&self.path);
    }
}

/// Acquire an exclusive lock on the PID file
///
/// Returns a `PidLock` guard that holds the lock. When the guard is dropped,
/// the lock is released and the PID file is removed.
///
/// # Errors
///
/// Returns an error if:
/// - The PID file cannot be created
/// - Another daemon instance holds the lock
/// - The lock cannot be acquired
pub fn acquire_pid_lock(pid_path: &Path) -> Result<PidLock> {
    // Ensure parent directory exists
    if let Some(parent) = pid_path.parent() {
        fs::create_dir_all(parent).map_err(|e| NikaError::IoPathError {
            path: parent.to_path_buf(),
            operation: "create directory".to_string(),
            source: e,
        })?;
    }

    // Open PID file for writing
    let file = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .read(true)
        .open(pid_path)
        .map_err(|e| NikaError::IoPathError {
            path: pid_path.to_path_buf(),
            operation: "open PID file".to_string(),
            source: e,
        })?;

    // Try to acquire exclusive lock via Flock (non-deprecated API)
    #[cfg(unix)]
    {
        let flock = Flock::lock(file, FlockArg::LockExclusiveNonblock).map_err(
            |(_file, errno)| match errno {
                nix::errno::Errno::EWOULDBLOCK => NikaError::DaemonAlreadyRunning {
                    pid_file: pid_path.to_path_buf(),
                },
                e => NikaError::DaemonError {
                    message: format!("Failed to acquire PID lock: {}", e),
                },
            },
        )?;

        // Write current PID (Flock<File> derefs to File)
        let pid = std::process::id();
        writeln!(&*flock, "{}", pid).map_err(|e| NikaError::IoPathError {
            path: pid_path.to_path_buf(),
            operation: "write PID".to_string(),
            source: e,
        })?;

        // Sync to disk
        flock.sync_all().map_err(|e| NikaError::IoPathError {
            path: pid_path.to_path_buf(),
            operation: "sync PID file".to_string(),
            source: e,
        })?;

        return Ok(PidLock {
            path: pid_path.to_path_buf(),
            _flock: flock,
        });
    }

    // Non-unix fallback (no file locking)
    #[cfg(not(unix))]
    {
        let pid = std::process::id();
        writeln!(file, "{}", pid).map_err(|e| NikaError::IoPathError {
            path: pid_path.to_path_buf(),
            operation: "write PID".to_string(),
            source: e,
        })?;

        file.sync_all().map_err(|e| NikaError::IoPathError {
            path: pid_path.to_path_buf(),
            operation: "sync PID file".to_string(),
            source: e,
        })?;

        Ok(PidLock {
            path: pid_path.to_path_buf(),
            file,
        })
    }
}

/// Release the PID lock (happens automatically on drop)
///
/// This is a convenience function for explicit release.
pub fn release_pid_lock(lock: PidLock) {
    // Just drop it - the Drop impl handles cleanup
    drop(lock);
}

/// Read the PID from a PID file
///
/// Returns `None` if the file doesn't exist or can't be read.
///
/// # TOCTOU Note
///
/// This function uses atomic file reading (read_to_string) instead of
/// separate open/read operations to minimize race window.
#[allow(dead_code)] // Used by daemon status command
pub fn read_pid(pid_path: &Path) -> Option<u32> {
    // TOCTOU-safe: Use read_to_string directly instead of open+read.
    // This minimizes the race window by performing a single syscall.
    match std::fs::read_to_string(pid_path) {
        Ok(contents) => contents.trim().parse().ok(),
        Err(_) => None,
    }
}

/// Check if a process with the given PID is still running
#[allow(dead_code)] // Used by daemon status command
#[cfg(unix)]
pub fn is_process_running(pid: u32) -> bool {
    // Send signal 0 to check if process exists
    use nix::sys::signal::kill;
    use nix::unistd::Pid;

    kill(Pid::from_raw(pid as i32), None).is_ok()
}

#[allow(dead_code)] // Used by daemon status command
#[cfg(not(unix))]
pub fn is_process_running(_pid: u32) -> bool {
    // On non-Unix, assume running if we can't check
    true
}

/// Check if the daemon is running by examining the PID file
///
/// # TOCTOU Note
///
/// This function has an inherent TOCTOU race: the daemon could exit between
/// reading the PID file and checking if the process is running. This is
/// acceptable for status checks because:
///
/// 1. The result is informational, not used for security decisions
/// 2. The `acquire_pid_lock` function uses flock() for actual mutual exclusion
/// 3. There is no atomic "read PID and check process" syscall available
///
/// For daemon startup, always use `acquire_pid_lock` which provides true
/// mutual exclusion via file locking.
#[allow(dead_code)] // Used by daemon status command
pub fn is_daemon_running(pid_path: &Path) -> bool {
    // Note: Inherent TOCTOU between read_pid and is_process_running.
    // This is acceptable for status checks; use acquire_pid_lock for exclusion.
    if let Some(pid) = read_pid(pid_path) {
        is_process_running(pid)
    } else {
        false
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// TESTS
// ═══════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_acquire_and_release_pid_lock() {
        let temp = tempdir().unwrap();
        let pid_path = temp.path().join("daemon/test.pid");

        // Acquire lock
        let lock = acquire_pid_lock(&pid_path).unwrap();

        // Verify PID file exists and contains our PID
        assert!(pid_path.exists());
        let stored_pid = read_pid(&pid_path).unwrap();
        assert_eq!(stored_pid, std::process::id());

        // Release lock
        release_pid_lock(lock);

        // PID file should be removed
        assert!(!pid_path.exists());
    }

    #[test]
    fn test_pid_lock_creates_parent_dirs() {
        let temp = tempdir().unwrap();
        let pid_path = temp.path().join("deep/nested/dir/test.pid");

        let lock = acquire_pid_lock(&pid_path).unwrap();
        assert!(pid_path.exists());

        drop(lock);
    }

    #[test]
    fn test_read_pid_nonexistent_file() {
        let temp = tempdir().unwrap();
        let pid_path = temp.path().join("nonexistent.pid");

        assert!(read_pid(&pid_path).is_none());
    }

    #[test]
    fn test_read_pid_invalid_content() {
        let temp = tempdir().unwrap();
        let pid_path = temp.path().join("invalid.pid");

        fs::write(&pid_path, "not-a-number").unwrap();
        assert!(read_pid(&pid_path).is_none());
    }

    #[test]
    fn test_read_pid_valid_content() {
        let temp = tempdir().unwrap();
        let pid_path = temp.path().join("valid.pid");

        fs::write(&pid_path, "12345\n").unwrap();
        assert_eq!(read_pid(&pid_path), Some(12345));
    }

    #[test]
    fn test_is_process_running_current() {
        // Our own process should be running
        let our_pid = std::process::id();
        assert!(is_process_running(our_pid));
    }

    #[test]
    fn test_is_process_running_invalid() {
        // PID 0 is the kernel, PID 99999999 is unlikely to exist
        // We use a very high PID that's unlikely to be in use
        assert!(!is_process_running(4000000));
    }

    #[test]
    fn test_is_daemon_running_no_file() {
        let temp = tempdir().unwrap();
        let pid_path = temp.path().join("nonexistent.pid");

        assert!(!is_daemon_running(&pid_path));
    }

    #[test]
    fn test_is_daemon_running_stale_file() {
        let temp = tempdir().unwrap();
        let pid_path = temp.path().join("stale.pid");

        // Write a PID that doesn't exist
        fs::write(&pid_path, "4000000\n").unwrap();

        assert!(!is_daemon_running(&pid_path));
    }

    #[test]
    fn test_is_daemon_running_current_process() {
        let temp = tempdir().unwrap();
        let pid_path = temp.path().join("current.pid");

        // Write our own PID
        let our_pid = std::process::id();
        fs::write(&pid_path, format!("{}\n", our_pid)).unwrap();

        assert!(is_daemon_running(&pid_path));
    }

    #[cfg(unix)]
    #[test]
    fn test_double_lock_fails() {
        let temp = tempdir().unwrap();
        let pid_path = temp.path().join("double.pid");

        // First lock succeeds
        let _lock1 = acquire_pid_lock(&pid_path).unwrap();

        // Second lock should fail
        let result = acquire_pid_lock(&pid_path);
        assert!(result.is_err());

        if let Err(NikaError::DaemonAlreadyRunning { .. }) = result {
            // Expected
        } else {
            panic!("Expected DaemonAlreadyRunning error");
        }
    }

    #[test]
    fn test_pid_lock_path_accessor() {
        let temp = tempdir().unwrap();
        let pid_path = temp.path().join("test.pid");

        let lock = acquire_pid_lock(&pid_path).unwrap();
        assert_eq!(lock.path(), pid_path);

        drop(lock);
    }
}
