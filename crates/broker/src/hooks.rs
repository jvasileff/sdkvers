use types::{Candidate, HookFingerprint, Identifier, Platform};

use crate::{Error, client};

/// A fetched hook script with its raw text and computed fingerprint.
pub struct FetchedHook {
    pub raw: String,
    /// The normalized form of the hook used for fingerprinting (useful for diagnosing
    /// unknown hashes).
    pub normalized: String,
    pub fingerprint: HookFingerprint,
}

// Known normalized MD5 fingerprints — see SDKALT-HOOKS.md.
const HASH_DEFAULT_ZIP: &str = "fcb2384eb3d368d473c1e99d1561ff9c";
const HASH_DEFAULT_TARBALL: &str = "ed8b941f3f502247fa2fb59477337c4c";
const HASH_LINUX_JAVA_TARBALL: &str = "87d5e14e777c1a77a805d4da7d9fe36e";
const HASH_OSX_JAVA_TARBALL: &str = "71cfc4bb7c090de0b8b5e2674f65ba62";
const HASH_JMC_FOLDER: &str = "a091ec7b7b0b2f9a3c27e98f9e8728af";
const HASH_JMC_FLAT: &str = "0c11a5f98122448338c99a3cb9cc8789";

/// Fetch the post-install hook script for a candidate version and fingerprint it.
/// Hits GET /hooks/post/{candidate}/{identifier}/{platform}.
/// Returns a no-op fingerprint if the hook body is empty.
pub fn fetch_hook(
    candidate: &Candidate,
    identifier: &Identifier,
    platform: &Platform,
) -> Result<FetchedHook, Error> {
    let path = format!(
        "/hooks/post/{}/{}/{}",
        candidate.as_str(),
        identifier.as_str(),
        platform.as_api_str()
    );

    let raw = client::fetch(&path)?;

    let (fingerprint, normalized) = if raw.trim().is_empty() {
        (HookFingerprint::DefaultZip, String::new())
    } else {
        let norm = normalize(candidate, identifier, &raw);
        let fp = classify(&norm, &raw);
        (fp, norm)
    };

    Ok(FetchedHook { raw, normalized, fingerprint })
}

/// Normalize a hook script by substituting variable parts with fixed placeholders.
/// See SDKALT-HOOKS.md for the full normalization specification.
fn normalize(candidate: &Candidate, identifier: &Identifier, raw: &str) -> String {
    let vendor = identifier.as_str()
        .rfind('-')
        .map(|pos| &identifier.as_str()[pos + 1..])
        .filter(|s| !s.is_empty() && s.chars().all(|c| c.is_ascii_alphabetic()))
        .unwrap_or("");

    let candidate_title = {
        let mut c = candidate.as_str().chars();
        match c.next() {
            None => String::new(),
            Some(first) => first.to_uppercase().collect::<String>() + c.as_str(),
        }
    };

    let normalized = raw
        .replace(identifier.as_str(), "IDENTIFIER")
        .replace(&candidate_title, "CANDIDATE")
        .replace(candidate.as_str(), "CANDIDATE")
        .replace("Linux ARM 32bit Hard Float", "PLATFORM")
        .replace("Linux ARM 64bit", "PLATFORM")
        .replace("Linux 64bit", "PLATFORM")
        .replace("macOS ARM 64bit", "PLATFORM")
        .replace("macOS 64bit", "PLATFORM")
        .replace(&format!("JMC {vendor}"), "JMC VENDOR");

    // Normalize JMC-specific lines.
    normalize_jmc_lines(&normalized)
}

/// Classify a normalized hook script by its MD5 hash.
fn classify(normalized: &str, raw: &str) -> HookFingerprint {
    let hash = format!("{:x}", md5::compute(normalized.as_bytes()));

    match hash.as_str() {
        HASH_DEFAULT_ZIP => HookFingerprint::DefaultZip,
        HASH_DEFAULT_TARBALL => HookFingerprint::DefaultTarball,
        HASH_LINUX_JAVA_TARBALL => HookFingerprint::LinuxJavaTarball,
        HASH_OSX_JAVA_TARBALL => HookFingerprint::OsxJavaTarball,
        HASH_JMC_FOLDER => {
            let exe = extract_executable_binary(raw);
            HookFingerprint::UnixJmcTarballFolder { executable_binary: exe }
        }
        HASH_JMC_FLAT => {
            let exe = extract_executable_binary(raw);
            HookFingerprint::UnixJmcTarballFlat { executable_binary: exe }
        }
        _ => HookFingerprint::Unknown { hash },
    }
}

/// Replace `executable_binary` and `containing_folder` lines with placeholders.
/// Preserves the trailing newline of the input, since Rust's `.lines()` strips it
/// and the reference hashes were computed from files that include it.
fn normalize_jmc_lines(s: &str) -> String {
    let mut result = s.lines()
        .map(|line| {
            if line.trim_start().starts_with("local executable_binary=") {
                "    local executable_binary=\"EXECUTABLE\""
            } else if line.trim_start().starts_with("local containing_folder=") {
                "    local containing_folder=\"FOLDER\""
            } else {
                line
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    if s.ends_with('\n') {
        result.push('\n');
    }
    result
}

/// Extract the raw executable_binary value from a JMC hook script.
fn extract_executable_binary(raw: &str) -> String {
    for line in raw.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("local executable_binary=\"") {
            if let Some(value) = rest.strip_suffix('"') {
                return value.to_string();
            }
        }
    }
    String::new()
}
