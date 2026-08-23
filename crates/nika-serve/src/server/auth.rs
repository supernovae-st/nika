// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

use std::fmt;
use std::fs::File;
use std::io::Read as _;
use std::path::Path;

use hyper::HeaderMap;
use hyper::header::AUTHORIZATION;
#[cfg(unix)]
use nix::fcntl::{OFlag, open};
#[cfg(unix)]
use nix::sys::stat::Mode;
use sha2::{Digest as _, Sha256};
use subtle::ConstantTimeEq as _;
use zeroize::Zeroizing;

use super::ServerError;

const MIN_TOKEN_BYTES: usize = 32;
const MAX_TOKEN_BYTES: usize = 512;

pub(crate) struct BearerToken {
    digest: [u8; 32],
}

impl BearerToken {
    pub(crate) fn from_file(path: &Path) -> Result<Self, ServerError> {
        let file = open_secret(path)?;
        validate_file_mode(&file)?;
        let mut bytes = Zeroizing::new(Vec::new());
        file.take((MAX_TOKEN_BYTES + 3) as u64)
            .read_to_end(&mut bytes)
            .map_err(|_| ServerError::Credential)?;
        if bytes.len() > MAX_TOKEN_BYTES + 2 {
            return Err(ServerError::Credential);
        }
        if bytes.ends_with(b"\r\n") {
            let body_len = bytes.len() - 2;
            bytes.truncate(body_len);
        } else if matches!(bytes.last(), Some(b'\n' | b'\r')) {
            bytes.pop();
        }
        if bytes.len() < MIN_TOKEN_BYTES
            || bytes.len() > MAX_TOKEN_BYTES
            || !bytes.iter().all(u8::is_ascii_graphic)
        {
            return Err(ServerError::Credential);
        }
        Ok(Self {
            digest: Sha256::digest(&bytes).into(),
        })
    }

    pub(crate) fn authorizes(&self, headers: &HeaderMap) -> bool {
        let mut values = headers.get_all(AUTHORIZATION).iter();
        let Some(value) = values.next() else {
            return false;
        };
        if values.next().is_some() {
            return false;
        }
        let Ok(value) = value.to_str() else {
            return false;
        };
        let Some(candidate) = value.strip_prefix("Bearer ") else {
            return false;
        };
        if candidate.len() < MIN_TOKEN_BYTES
            || candidate.len() > MAX_TOKEN_BYTES
            || !candidate.bytes().all(|byte| byte.is_ascii_graphic())
        {
            return false;
        }
        let candidate: [u8; 32] = Sha256::digest(candidate.as_bytes()).into();
        bool::from(self.digest.ct_eq(&candidate))
    }
}

impl fmt::Debug for BearerToken {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("BearerToken([REDACTED])")
    }
}

#[cfg(unix)]
fn open_secret(path: &Path) -> Result<File, ServerError> {
    open(
        path,
        OFlag::O_RDONLY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC | OFlag::O_NONBLOCK,
        Mode::empty(),
    )
    .map(File::from)
    .map_err(|_| ServerError::Credential)
}

#[cfg(not(unix))]
fn open_secret(path: &Path) -> Result<File, ServerError> {
    File::open(path).map_err(|_| ServerError::Credential)
}

#[cfg(unix)]
fn validate_file_mode(file: &File) -> Result<(), ServerError> {
    use std::os::unix::fs::MetadataExt as _;

    let metadata = file.metadata().map_err(|_| ServerError::Credential)?;
    if metadata.mode() & 0o077 != 0 || !metadata.is_file() {
        return Err(ServerError::Credential);
    }
    Ok(())
}

#[cfg(not(unix))]
fn validate_file_mode(file: &File) -> Result<(), ServerError> {
    if file.metadata().is_ok_and(|meta| meta.is_file()) {
        Ok(())
    } else {
        Err(ServerError::Credential)
    }
}
