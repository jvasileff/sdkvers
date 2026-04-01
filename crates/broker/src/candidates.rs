use types::{Candidate, Platform, SdkListNode};

use crate::{Error, client};

/// Return all available SDKMAN candidate names.
/// Hits GET /candidates/all.
pub fn list_candidates() -> Result<Vec<Candidate>, Error> {
    let body = client::fetch("/candidates/all")?;
    let candidates = body
        .trim()
        .split(',')
        .map(|s| Candidate::new(s.trim()))
        .collect();
    Ok(candidates)
}

/// Return all available versions for a candidate on a given platform.
/// Hits GET /candidates/{candidate}/{platform}/versions/list.
pub fn list_versions(
    candidate: &Candidate,
    platform: &Platform,
) -> Result<SdkListNode, Error> {
    let path = format!(
        "/candidates/{}/{}/versions/list?installed=",
        candidate.as_str(),
        platform.as_api_str()
    );
    let body = client::fetch(&path)?;
    Ok(types::parse_sdk_list(candidate.as_str(), &body))
}
