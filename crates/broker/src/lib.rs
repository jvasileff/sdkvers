use thiserror::Error;

mod client;
mod candidates;
mod download;
mod hooks;

pub use candidates::{list_candidates, list_versions, list_versions_raw};
pub use download::{download_archive, DownloadedArchive};
pub use hooks::{fetch_hook, FetchedHook};

// ── Errors ────────────────────────────────────────────────────────────────────

#[derive(Debug, Error)]
pub enum Error {
    #[error("network error: {0}")]
    Network(#[from] reqwest::Error),

    #[error("unexpected HTTP status {status} for {url}")]
    UnexpectedStatus { url: String, status: u16 },

    #[error("response body is not valid UTF-8: {0}")]
    Encoding(#[from] std::string::FromUtf8Error),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("unrecognised version list format for candidate {0}")]
    UnrecognisedVersionListFormat(String),
}
