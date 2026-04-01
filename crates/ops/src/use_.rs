use types::{Candidate, Identifier};

use crate::Error;

/// Set the current version for a candidate in the current shell session.
/// The version must already be installed. Updates the `current` symlink.
/// The caller (CLI) is responsible for emitting the eval-able env-var output.
pub fn use_version(candidate: &Candidate, identifier: &Identifier) -> Result<(), Error> {
    store::set_current(candidate, identifier)?;
    Ok(())
}
