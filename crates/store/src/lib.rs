use thiserror::Error;

mod candidates;
mod extraction;
mod symlinks;

pub use candidates::{InstalledVersion, list_candidates, list_installed};
pub use extraction::{extract, remove, verify_checksum, version_path};
pub use symlinks::{clear_current, get_all_current, get_current, set_current};

// ── Errors ────────────────────────────────────────────────────────────────────

#[derive(Debug, Error)]
pub enum Error {
    #[error("SDKMAN_DIR could not be determined (set SDKMAN_DIR or HOME)")]
    SdkmanDirUnknown,

    #[error("filesystem error: {0}")]
    Io(#[from] std::io::Error),

    #[error("candidate {0} not found in candidates directory")]
    CandidateNotFound(String),

    #[error("{candidate} {identifier} is not installed")]
    VersionNotInstalled { candidate: String, identifier: String },

    #[error("checksum mismatch: expected {expected}, got {actual}")]
    ChecksumMismatch { expected: String, actual: String },

    #[error("unrecognised hook fingerprint {hash} for {candidate} {identifier} — cannot install")]
    UnknownHookFingerprint {
        candidate: String,
        identifier: String,
        hash: String,
    },

    #[error("unexpected archive layout for {candidate} {identifier}: {detail}")]
    UnexpectedLayout {
        candidate: String,
        identifier: String,
        detail: String,
    },
}

// ── SDKMAN_DIR resolution ─────────────────────────────────────────────────────

pub fn sdkman_dir() -> Result<std::path::PathBuf, Error> {
    if let Ok(dir) = std::env::var("SDKMAN_DIR") {
        return Ok(std::path::PathBuf::from(dir));
    }
    if let Ok(home) = std::env::var("HOME") {
        return Ok(std::path::PathBuf::from(home).join(".sdkman"));
    }
    Err(Error::SdkmanDirUnknown)
}

pub fn candidates_dir() -> Result<std::path::PathBuf, Error> {
    Ok(sdkman_dir()?.join("candidates"))
}
