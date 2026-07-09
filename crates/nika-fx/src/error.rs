//! Typed error surface. The nika-builtin wiring maps these onto
//! `NIKA-BUILTIN-IMAGE_FX-NNN` codes (001 args · 003 unsupported-format ·
//! 004 decode · 005 budget — see the builtin module).

use std::fmt;

/// Every failure of the pure transform pipeline. `#[non_exhaustive]` per the
/// forward-compat law — additive variants only within a major.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FxError {
    /// Argument rejected before any pixel work (bad op parameter, empty ops,
    /// ascii not terminal, palette too large, …).
    Args(String),
    /// Input bytes are not a PNG we accept (magic bytes, color type, bit
    /// depth, interlace). Carries the honest hint for the author.
    UnsupportedFormat(String),
    /// Structurally invalid PNG / DEFLATE stream (CRC, Adler, filters, sizes).
    Decode(String),
    /// Decoded-pixel budget exceeded — bounded BEFORE inflate from IHDR.
    Budget { pixels: u64, max: u64 },
    /// Internal encode failure (should not happen on valid pipelines).
    Encode(String),
}

impl fmt::Display for FxError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FxError::Args(m) => write!(f, "invalid image_fx args: {m}"),
            FxError::UnsupportedFormat(m) => write!(f, "unsupported input format: {m}"),
            FxError::Decode(m) => write!(f, "png decode failed: {m}"),
            FxError::Budget { pixels, max } => {
                write!(f, "decoded-pixel budget exceeded: {pixels} > {max}")
            }
            FxError::Encode(m) => write!(f, "png encode failed: {m}"),
        }
    }
}

impl std::error::Error for FxError {}
