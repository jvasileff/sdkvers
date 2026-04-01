use types::{Candidate, Identifier};

use crate::Error;

/// Set the default (persistent) version for a candidate.
/// The version must already be installed. Updates the `current` symlink.
pub fn set_default(candidate: &Candidate, identifier: &Identifier) -> Result<(), Error> {
    store::set_current(candidate, identifier)?;
    Ok(())
}
