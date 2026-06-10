// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `NikaErrorCode` implementations for the CORE-owned kernel error
//! types (orphan-rule home · the impls live with their types).
//!
//! Core-owned ranges (the full cross-domain registry stays in the hub
//! `nika-kernel/src/errors.rs` module doc):
//! - 050–099: Shell/exec (`ShellError`)
//! - 100–139: File/IO (`BlobError`)
//! - 140–189: Http/network (`HttpError`)

use nika_error::prelude::*;

use crate::io::blob::BlobError;
use crate::io::http::HttpError;
use crate::io::process::ShellError;

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
