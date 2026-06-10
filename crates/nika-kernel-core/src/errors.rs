// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `NikaErrorCode` implementations for the CORE-owned kernel error
//! types (orphan-rule home · the impls live with their types).
//!
//! Core-owned ranges (the full cross-domain registry stays in the hub
//! `nika-kernel/src/errors.rs` module doc):
//! - 050–099: Shell/exec (`ShellError`)
//! - 100–139: File/IO — blob 100–109 (`BlobError`) · fs 110–119 (`FsError`)
//! - 140–189: Http/network (`HttpError`)
//! - 1000–1099: Screen capture (`ScreenError`) — `io/screen` · ADR-081
//! - 1100–1199: OCR (`OcrError`) — `io/ocr`
//! - 1200–1299: Accessibility (`A11yError`) — `io/a11y`
//! - 1300–1399: Synthetic input (`InputError`) — `io/input` · M2.4
//! - 1400–1499: Browser automation (`BrowserError`) — `io/browser`
//!
//! The 1000–1499 computer-use error enums live in their `io/*` module (the
//! typed boundary · Pattern A · FCI-023bis); the `NikaErrorCode` impls live
//! here; the code constants live in `nika_error::codes`.

use nika_error::prelude::*;

use crate::io::a11y::A11yError;
use crate::io::blob::BlobError;
use crate::io::browser::BrowserError;
use crate::io::fs::FsError;
use crate::io::http::HttpError;
use crate::io::input::InputError;
use crate::io::ocr::OcrError;
use crate::io::process::ShellError;
use crate::io::screen::ScreenError;

// ─── Error code constants ────────────────────────────────────────────

// Shell/exec: 050–099
/// Program not found.
pub const NIKA_050: NikaCode = NikaCode {
    num: 50,
    category: Category::Shell,
    severity: Severity::Error,
    slug: "shell-not-found",
};
/// Shell execution timed out.
pub const NIKA_051: NikaCode = NikaCode {
    num: 51,
    category: Category::Shell,
    severity: Severity::Error,
    slug: "shell-timeout",
};
/// Shell execution cancelled.
pub const NIKA_052: NikaCode = NikaCode {
    num: 52,
    category: Category::Shell,
    severity: Severity::Error,
    slug: "shell-cancelled",
};
/// Command blocked by security.
pub const NIKA_053: NikaCode = NikaCode {
    num: 53,
    category: Category::Shell,
    severity: Severity::Error,
    slug: "shell-blocked",
};
/// Shell other error.
pub const NIKA_059: NikaCode = NikaCode {
    num: 59,
    category: Category::Shell,
    severity: Severity::Error,
    slug: "shell-other",
};

// File/IO (blob): 100–139
/// Blob not found.
pub const NIKA_100: NikaCode = NikaCode {
    num: 100,
    category: Category::FileIo,
    severity: Severity::Error,
    slug: "blob-not-found",
};
/// Blob I/O error.
pub const NIKA_101: NikaCode = NikaCode {
    num: 101,
    category: Category::FileIo,
    severity: Severity::Error,
    slug: "blob-io",
};
/// Blob too large.
pub const NIKA_102: NikaCode = NikaCode {
    num: 102,
    category: Category::FileIo,
    severity: Severity::Error,
    slug: "blob-too-large",
};

// File/IO (fs): 110–119 (sibling sub-range of FileIo, distinct from blob 100–109)
/// Filesystem path not found.
pub const NIKA_110: NikaCode = NikaCode {
    num: 110,
    category: Category::FileIo,
    severity: Severity::Error,
    slug: "fs-not-found",
};
/// Filesystem permission denied.
pub const NIKA_111: NikaCode = NikaCode {
    num: 111,
    category: Category::FileIo,
    severity: Severity::Error,
    slug: "fs-permission-denied",
};
/// Filesystem path already exists.
pub const NIKA_112: NikaCode = NikaCode {
    num: 112,
    category: Category::FileIo,
    severity: Severity::Error,
    slug: "fs-already-exists",
};
/// Filesystem invalid data (non-UTF-8 read, bad glob pattern).
pub const NIKA_113: NikaCode = NikaCode {
    num: 113,
    category: Category::FileIo,
    severity: Severity::Error,
    slug: "fs-invalid-data",
};
/// Filesystem other I/O error.
pub const NIKA_119: NikaCode = NikaCode {
    num: 119,
    category: Category::FileIo,
    severity: Severity::Error,
    slug: "fs-io",
};

// Http/network: 140–189
/// HTTP timeout.
pub const NIKA_140: NikaCode = NikaCode {
    num: 140,
    category: Category::Http,
    severity: Severity::Error,
    slug: "http-timeout",
};
/// HTTP connection failed.
pub const NIKA_141: NikaCode = NikaCode {
    num: 141,
    category: Category::Http,
    severity: Severity::Error,
    slug: "http-connection",
};
/// SSRF blocked.
pub const NIKA_142: NikaCode = NikaCode {
    num: 142,
    category: Category::Http,
    severity: Severity::Error,
    slug: "http-ssrf-blocked",
};
/// HTTP response too large.
pub const NIKA_143: NikaCode = NikaCode {
    num: 143,
    category: Category::Http,
    severity: Severity::Error,
    slug: "http-too-large",
};
/// HTTP unsupported.
pub const NIKA_144: NikaCode = NikaCode {
    num: 144,
    category: Category::Http,
    severity: Severity::Error,
    slug: "http-unsupported",
};
/// HTTP other.
pub const NIKA_149: NikaCode = NikaCode {
    num: 149,
    category: Category::Http,
    severity: Severity::Error,
    slug: "http-other",
};

// ─── NikaErrorCode implementations ──────────────────────────────────

impl NikaErrorCode for ShellError {
    fn nika_code(&self) -> NikaCode {
        match self {
            Self::NotFound { .. } => NIKA_050,
            Self::Timeout { .. } => NIKA_051,
            Self::Cancelled { .. } => NIKA_052,
            Self::Blocked { .. } => NIKA_053,
            Self::Other { .. } => NIKA_059,
        }
    }
}

impl NikaErrorCode for BlobError {
    fn nika_code(&self) -> NikaCode {
        match self {
            Self::NotFound { .. } => NIKA_100,
            Self::Io { .. } => NIKA_101,
            Self::TooLarge { .. } => NIKA_102,
        }
    }
}

impl NikaErrorCode for FsError {
    fn nika_code(&self) -> NikaCode {
        match self {
            Self::NotFound { .. } => NIKA_110,
            Self::PermissionDenied { .. } => NIKA_111,
            Self::AlreadyExists { .. } => NIKA_112,
            Self::InvalidData { .. } => NIKA_113,
            Self::Io { .. } => NIKA_119,
        }
    }
}

// Screen capture: 1000–1099 (Category::Screen · codes in nika_error::codes ·
// reserved per ADR-081 computer-use · enum in `crate::io::screen`).
impl NikaErrorCode for ScreenError {
    fn nika_code(&self) -> NikaCode {
        use nika_error::codes;
        match self {
            Self::BackendNotWired => codes::NIKA_1000,
            Self::DisplayNotFound { .. } => codes::NIKA_1001,
            Self::NoDisplaysFound => codes::NIKA_1002,
            Self::CaptureFailed { .. } => codes::NIKA_1003,
            Self::RegionOutOfBounds { .. } => codes::NIKA_1004,
            Self::InvalidFrameFormat { .. } => codes::NIKA_1005,
            Self::ConsentDenied => codes::NIKA_1006,
            Self::ConsentRevoked => codes::NIKA_1007,
            Self::IndicatorUnavailable { .. } => codes::NIKA_1008,
            Self::BackendInit { .. } => codes::NIKA_1009,
        }
    }

    /// Capture/init failures may be transient (device contention · GPU busy);
    /// consent + region + format errors are structural.
    fn is_transient(&self) -> bool {
        matches!(self, Self::CaptureFailed { .. } | Self::BackendInit { .. })
    }
}

// OCR: 1100–1199 (Category::Ocr · codes in nika_error::codes · enum in
// `crate::io::ocr`).
impl NikaErrorCode for OcrError {
    fn nika_code(&self) -> NikaCode {
        use nika_error::codes;
        match self {
            Self::ModelNotFound { .. } => codes::NIKA_1101,
            Self::ModelLoadFailed { .. } => codes::NIKA_1102,
            Self::EngineInit { .. } => codes::NIKA_1103,
            Self::RegionOutOfBounds { .. } => codes::NIKA_1104,
            Self::InvalidFrameFormat { .. } => codes::NIKA_1105,
            Self::PrepareInputFailed { .. } => codes::NIKA_1106,
            Self::DetectionFailed { .. } => codes::NIKA_1107,
            Self::RecognitionFailed { .. } => codes::NIKA_1108,
            Self::TaskJoinFailed { .. } => codes::NIKA_1109,
        }
    }

    /// True for retryable failures (transient inference / task) · false for
    /// structural ones (model missing · bad frame · bad region).
    fn is_transient(&self) -> bool {
        matches!(
            self,
            Self::DetectionFailed { .. }
                | Self::RecognitionFailed { .. }
                | Self::TaskJoinFailed { .. }
        )
    }
}

// Accessibility: 1200–1299 (Category::A11y · codes in nika_error::codes · enum
// in `crate::io::a11y`).
impl NikaErrorCode for A11yError {
    fn nika_code(&self) -> NikaCode {
        use nika_error::codes;
        match self {
            Self::PermissionDenied => codes::NIKA_1201,
            Self::NoFocusedApplication => codes::NIKA_1202,
            Self::AttributeError { .. } => codes::NIKA_1203,
            Self::TreeWalkFailed { .. } => codes::NIKA_1204,
            Self::BackendUnavailable => codes::NIKA_1205,
            Self::TaskJoinFailed { .. } => codes::NIKA_1206,
        }
    }

    /// Attribute/walk/join failures may be transient (focus churn · app
    /// teardown mid-walk) · permission + backend absence are structural.
    fn is_transient(&self) -> bool {
        matches!(
            self,
            Self::AttributeError { .. } | Self::TreeWalkFailed { .. } | Self::TaskJoinFailed { .. }
        )
    }
}

// Synthetic input: 1300–1399 (Category::Input · codes in nika_error::codes ·
// enum in `crate::io::input`).
impl NikaErrorCode for InputError {
    fn nika_code(&self) -> NikaCode {
        use nika_error::codes;
        match self {
            Self::ConsentDenied => codes::NIKA_1301,
            Self::ConsentExpired => codes::NIKA_1302,
            Self::EventPostFailed { .. } => codes::NIKA_1303,
            Self::BackendUnavailable => codes::NIKA_1304,
            Self::TaskJoinFailed { .. } => codes::NIKA_1305,
        }
    }

    /// Event-post + task-join failures may be transient (device contention);
    /// consent + backend absence are structural.
    fn is_transient(&self) -> bool {
        matches!(
            self,
            Self::EventPostFailed { .. } | Self::TaskJoinFailed { .. }
        )
    }
}

// Browser automation: 1400–1499 (Category::Browser · codes in nika_error::codes
// · enum in `crate::io::browser`).
impl NikaErrorCode for BrowserError {
    fn nika_code(&self) -> NikaCode {
        use nika_error::codes;
        match self {
            Self::LaunchFailed { .. } => codes::NIKA_1401,
            Self::NavigationFailed { .. } => codes::NIKA_1402,
            Self::SessionNotFound { .. } => codes::NIKA_1403,
            Self::SelectorFailed { .. } => codes::NIKA_1404,
            Self::BackendUnavailable => codes::NIKA_1405,
            Self::TaskJoinFailed { .. } => codes::NIKA_1406,
        }
    }

    /// Launch/navigation/selector/task failures may be transient (network ·
    /// timing); backend absence is structural.
    fn is_transient(&self) -> bool {
        !matches!(self, Self::BackendUnavailable)
    }
}

impl NikaErrorCode for HttpError {
    fn nika_code(&self) -> NikaCode {
        match self {
            Self::Timeout { .. } => NIKA_140,
            Self::Connection { .. } => NIKA_141,
            Self::SsrfBlocked { .. } => NIKA_142,
            Self::TooLarge { .. } => NIKA_143,
            Self::Unsupported { .. } => NIKA_144,
            Self::Other { .. } => NIKA_149,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_error_codes_in_range() {
        let err = ShellError::NotFound {
            program: "x".into(),
        };
        let code = err.nika_code();
        assert!(code.num >= 50 && code.num <= 99, "shell code {}", code.num);
        assert_eq!(code.category, Category::Shell);
    }

    #[test]
    fn blob_error_codes_in_range() {
        let err = BlobError::NotFound { hash: "x".into() };
        let code = err.nika_code();
        assert!(code.num >= 100 && code.num <= 139, "blob code {}", code.num);
        assert_eq!(code.category, Category::FileIo);
    }

    #[test]
    fn fs_error_codes_in_range() {
        let err = FsError::NotFound { path: "x".into() };
        let code = err.nika_code();
        assert!(
            code.num >= 110 && code.num <= 119,
            "fs code {} must be in the 110-119 sub-range",
            code.num
        );
        assert_eq!(code.category, Category::FileIo);
    }

    #[test]
    fn all_fs_variants_have_codes() {
        let _ = FsError::NotFound {
            path: String::new(),
        }
        .nika_code();
        let _ = FsError::PermissionDenied {
            path: String::new(),
        }
        .nika_code();
        let _ = FsError::AlreadyExists {
            path: String::new(),
        }
        .nika_code();
        let _ = FsError::InvalidData {
            path: String::new(),
            reason: String::new(),
        }
        .nika_code();
        let _ = FsError::Io {
            reason: String::new(),
        }
        .nika_code();
    }

    #[test]
    fn fs_from_io_maps_error_kinds() {
        use std::io::{Error, ErrorKind};
        use std::path::Path;
        let p = Path::new("/tmp/x");
        assert!(matches!(
            FsError::from_io(&Error::from(ErrorKind::NotFound), p),
            FsError::NotFound { .. }
        ));
        assert!(matches!(
            FsError::from_io(&Error::from(ErrorKind::PermissionDenied), p),
            FsError::PermissionDenied { .. }
        ));
        assert!(matches!(
            FsError::from_io(&Error::from(ErrorKind::AlreadyExists), p),
            FsError::AlreadyExists { .. }
        ));
        // An unmapped kind falls through to Io.
        assert!(matches!(
            FsError::from_io(&Error::from(ErrorKind::Other), p),
            FsError::Io { .. }
        ));
    }

    #[test]
    fn http_error_codes_in_range() {
        let err = HttpError::Timeout { duration_ms: 1000 };
        let code = err.nika_code();
        assert!(code.num >= 140 && code.num <= 189, "http code {}", code.num);
        assert_eq!(code.category, Category::Http);
    }

    #[test]
    fn all_shell_variants_have_codes() {
        let _ = ShellError::NotFound { program: "".into() }.nika_code();
        let _ = ShellError::Timeout { duration_ms: 0 }.nika_code();
        let _ = ShellError::Cancelled { id: "".into() }.nika_code();
        let _ = ShellError::Blocked { reason: "".into() }.nika_code();
        let _ = ShellError::Other { reason: "".into() }.nika_code();
    }

    #[test]
    fn all_http_variants_have_codes() {
        let _ = HttpError::Timeout { duration_ms: 0 }.nika_code();
        let _ = HttpError::Connection { reason: "".into() }.nika_code();
        let _ = HttpError::SsrfBlocked { url: "".into() }.nika_code();
        let _ = HttpError::TooLarge { size: 0, max: 0 }.nika_code();
        let _ = HttpError::Unsupported { reason: "".into() }.nika_code();
        let _ = HttpError::Other { reason: "".into() }.nika_code();
    }
}
