use types::{Candidate, Identifier};

use crate::{Error, candidates_dir};

/// Return the identifier of the currently active version for a candidate, if any.
pub fn get_current(candidate: &Candidate) -> Result<Option<Identifier>, Error> {
    let link = candidates_dir()?.join(candidate.as_str()).join("current");
    if !link.exists() {
        return Ok(None);
    }
    let target = std::fs::read_link(&link)?;
    let name = target
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    Ok(Some(Identifier::new(name)))
}

/// Return the currently active version for all candidates that have a `current` symlink.
pub fn get_all_current() -> Result<Vec<(Candidate, Identifier)>, Error> {
    let dir = candidates_dir()?;
    if !dir.exists() {
        return Ok(Vec::new());
    }

    let mut result = Vec::new();
    for entry in std::fs::read_dir(&dir)? {
        let entry = entry?;
        let candidate = Candidate::new(entry.file_name().to_string_lossy().as_ref());
        if let Some(ident) = get_current(&candidate)? {
            result.push((candidate, ident));
        }
    }
    Ok(result)
}

/// Update (or create) the `current` symlink for a candidate to point to the given identifier.
/// The version must already be installed.
pub fn set_current(candidate: &Candidate, identifier: &Identifier) -> Result<(), Error> {
    let cand_dir = candidates_dir()?.join(candidate.as_str());
    let version_dir = cand_dir.join(identifier.as_str());

    if !version_dir.exists() {
        return Err(Error::VersionNotInstalled {
            candidate: candidate.to_string(),
            identifier: identifier.to_string(),
        });
    }

    let link = cand_dir.join("current");

    // Remove existing symlink if present.
    if link.exists() || link.symlink_metadata().is_ok() {
        std::fs::remove_file(&link)?;
    }

    #[cfg(unix)]
    std::os::unix::fs::symlink(identifier.as_str(), &link)?;

    #[cfg(not(unix))]
    return Err(Error::Io(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "symlinks not supported on this platform",
    )));

    Ok(())
}

/// Remove the `current` symlink for a candidate, leaving no default set.
pub fn clear_current(candidate: &Candidate) -> Result<(), Error> {
    let link = candidates_dir()?.join(candidate.as_str()).join("current");
    if link.exists() || link.symlink_metadata().is_ok() {
        std::fs::remove_file(&link)?;
    }
    Ok(())
}
