use types::{Candidate, Identifier};

use crate::Error;

/// Return the identifier of the currently active version for a candidate.
/// Returns None if no version is currently set.
pub fn current(candidate: &Candidate) -> Result<Option<Identifier>, Error> {
    Ok(store::get_current(candidate)?)
}
