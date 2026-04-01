use thiserror::Error;

mod current;
mod default;
mod install;
mod list;
mod uninstall;
mod use_;

pub use current::current;
pub use default::set_default;
pub use install::install;
pub use list::{ListEntry, list, list_installed, list_local_candidates, list_remote_candidates};
pub use uninstall::uninstall;
pub use use_::use_version;

// ── Errors ────────────────────────────────────────────────────────────────────

#[derive(Debug, Error)]
pub enum Error {
    #[error("{0}")]
    Broker(#[from] broker::Error),

    #[error("{0}")]
    Store(#[from] store::Error),

    #[error("unsupported platform: {0}")]
    Platform(#[from] types::Error),

    #[error("no matching version found for {candidate} {expr}")]
    NoMatch { candidate: String, expr: String },

    #[error("ambiguous version expression '{expr}' for {candidate}: {count} candidates matched")]
    Ambiguous { candidate: String, expr: String, count: usize },

    #[error("{candidate} {identifier} is already installed")]
    AlreadyInstalled { candidate: String, identifier: String },
}
