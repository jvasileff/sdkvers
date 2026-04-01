use types::{Candidate, Identifier};

use crate::Error;

/// Uninstall an exact version of a candidate.
/// Clears the `current` symlink if it points to the removed version.
pub fn uninstall(candidate: &Candidate, identifier: &Identifier) -> Result<(), Error> {
    store::remove(candidate, identifier)?;

    // If the removed version was current, clear the symlink.
    if let Ok(Some(current)) = store::get_current(candidate) {
        if &current == identifier {
            let _ = store::clear_current(candidate);
        }
    }

    Ok(())
}
