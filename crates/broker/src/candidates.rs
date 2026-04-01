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
    let body = list_versions_raw(candidate, platform)?;
    Ok(types::parse_sdk_list(candidate.as_str(), &body))
}

/// Return the raw response text for a candidate's version list.
/// Useful for capturing fixture data or inspecting the API response directly.
pub fn list_versions_raw(
    candidate: &Candidate,
    platform: &Platform,
) -> Result<String, Error> {
    let path = format!(
        "/candidates/{}/{}/versions/list?installed=",
        candidate.as_str(),
        platform.as_api_str()
    );
    client::fetch(&path)
}
