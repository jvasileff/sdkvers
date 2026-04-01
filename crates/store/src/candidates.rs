use types::{Candidate, Identifier, Version, Vendor};

use crate::{Error, candidates_dir};

/// Return all candidates that have at least one version installed locally.
/// Reads directory names from $SDKMAN_DIR/candidates/.
pub fn list_candidates() -> Result<Vec<Candidate>, Error> {
    let dir = candidates_dir()?;
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let mut names = Vec::new();
    for entry in std::fs::read_dir(&dir)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            let name = entry.file_name().to_string_lossy().into_owned();
            names.push(Candidate::new(&name));
        }
    }
    names.sort_by(|a, b| a.as_str().cmp(b.as_str()));
    Ok(names)
}

/// An installed version found in the local candidates directory.
#[derive(Debug, Clone)]
pub struct InstalledVersion {
    pub candidate: Candidate,
    pub identifier: Identifier,
    pub version: Version,
    pub vendor: Option<Vendor>,
    pub is_current: bool,
}

/// Return all locally installed versions for a candidate.
/// Reads $SDKMAN_DIR/candidates/{candidate}/, skipping the `current` symlink entry.
pub fn list_installed(candidate: &Candidate) -> Result<Vec<InstalledVersion>, Error> {
    let dir = candidates_dir()?.join(candidate.as_str());
    if !dir.exists() {
        return Err(Error::CandidateNotFound(candidate.to_string()));
    }

    // Resolve the current symlink target to annotate results.
    let current_target = current_identifier(&dir);

    let mut versions = Vec::new();
    for entry in std::fs::read_dir(&dir)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().to_string();
        if name == "current" {
            continue;
        }
        let parsed = Identifier::parse(candidate, &name)
            .map_err(|_| Error::UnexpectedLayout {
                candidate: candidate.to_string(),
                identifier: name.clone(),
                detail: "could not parse installed identifier".to_string(),
            })?;
        let is_current = current_target.as_deref() == Some(name.as_str());
        versions.push(InstalledVersion {
            candidate: candidate.clone(),
            identifier: parsed.identifier,
            version: parsed.sdk_version,
            vendor: parsed.vendor,
            is_current,
        });
    }

    Ok(versions)
}

/// Read the target name of the `current` symlink, if present.
fn current_identifier(candidate_dir: &std::path::Path) -> Option<String> {
    let link = candidate_dir.join("current");
    std::fs::read_link(&link)
        .ok()
        .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_string()))
}
