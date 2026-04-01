use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};

use crate::Error;

const BASE_URL: &str = "https://api.sdkman.io/2";
const USER_AGENT: &str = concat!("sdkvers/", env!("CARGO_PKG_VERSION"));

/// In-process URL deduplication cache. Never hits the same URL twice per invocation.
static CACHE: LazyLock<Mutex<HashMap<String, String>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Fetch a text response from the given URL, using the in-process cache.
pub(crate) fn fetch(path: &str) -> Result<String, Error> {
    let url = format!("{BASE_URL}{path}");

    {
        let cache = CACHE.lock().unwrap();
        if let Some(cached) = cache.get(&url) {
            return Ok(cached.clone());
        }
    }

    let client = reqwest::blocking::Client::builder()
        .user_agent(USER_AGENT)
        .build()?;

    let response = client.get(&url).send()?;
    let status = response.status().as_u16();
    if !response.status().is_success() {
        return Err(Error::UnexpectedStatus { url, status });
    }

    let body = response.text()?;

    {
        let mut cache = CACHE.lock().unwrap();
        cache.insert(url, body.clone());
    }

    Ok(body)
}

/// Download a binary response to a temporary file. Not cached — always fetches.
pub(crate) fn download(path: &str, dest: &std::path::Path) -> Result<DownloadHeaders, Error> {
    let url = format!("{BASE_URL}{path}");

    let client = reqwest::blocking::Client::builder()
        .user_agent(USER_AGENT)
        .build()?;

    let mut response = client.get(&url).send()?;
    let status = response.status().as_u16();
    if !response.status().is_success() {
        return Err(Error::UnexpectedStatus { url, status });
    }

    let sha256 = response
        .headers()
        .get("x-sdkman-checksum-sha256")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    let md5 = response
        .headers()
        .get("x-sdkman-checksum-md5")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    let mut file = std::fs::File::create(dest)?;
    response.copy_to(&mut file)?;

    Ok(DownloadHeaders { sha256, md5 })
}

pub(crate) struct DownloadHeaders {
    pub sha256: Option<String>,
    pub md5: Option<String>,
}
