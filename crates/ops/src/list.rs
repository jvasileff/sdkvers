use types::{Candidate, Identifier, Platform, SdkListRow};

use crate::Error;

/// A version entry from the broker annotated with local install status.
#[derive(Debug, Clone)]
pub struct ListEntry {
    pub row: SdkListRow,
    pub installed: bool,
    pub is_current: bool,
}

/// Return the currently active version for every candidate that has one set.
pub fn get_all_current() -> Result<Vec<(Candidate, Identifier)>, Error> {
    Ok(store::get_all_current()?)
}

/// Return all candidates that are available remotely via the broker API.
pub fn list_remote_candidates() -> Result<Vec<Candidate>, Error> {
    Ok(broker::list_candidates()?)
}

/// Return all candidates that have at least one version installed locally.
pub fn list_local_candidates() -> Result<Vec<Candidate>, Error> {
    Ok(store::list_candidates()?)
}

/// Return all locally installed versions for a candidate.
pub fn list_installed(candidate: &Candidate) -> Result<Vec<store::InstalledVersion>, Error> {
    Ok(store::list_installed(candidate)?)
}

/// List all available versions of a candidate, annotated with local install status.
pub fn list(candidate: &Candidate) -> Result<Vec<ListEntry>, Error> {
    let platform = Platform::current()?;
    let remote = broker::list_versions(candidate, &platform)?;
    let installed = store::list_installed(candidate).unwrap_or_default();

    let entries = remote.rows.into_iter().map(|row| {
        let id = row.identifier.as_deref().unwrap_or(&row.version);
        let inst = installed.iter().find(|i| i.identifier.as_str() == id);
        ListEntry {
            row,
            installed: inst.is_some(),
            is_current: inst.map(|i| i.is_current).unwrap_or(false),
        }
    }).collect();

    Ok(entries)
}
