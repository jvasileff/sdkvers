/// Integration tests against the live SDKMAN broker API.
/// These are ignored by default; run with:
///   cargo test -p broker -- --ignored
use types::{Candidate, Platform};

// ── list_candidates ───────────────────────────────────────────────────────────

#[test]
#[ignore = "requires network access to SDKMAN API"]
fn list_candidates_returns_nonempty_list() {
    let candidates = broker::list_candidates().unwrap();
    assert!(!candidates.is_empty());
}

#[test]
#[ignore = "requires network access to SDKMAN API"]
fn list_candidates_includes_java_and_gradle() {
    let candidates = broker::list_candidates().unwrap();
    let names: Vec<&str> = candidates.iter().map(|c| c.as_str()).collect();
    assert!(names.contains(&"java"), "java not found in {names:?}");
    assert!(names.contains(&"gradle"), "gradle not found in {names:?}");
}

// ── list_versions ─────────────────────────────────────────────────────────────

#[test]
#[ignore = "requires network access to SDKMAN API"]
fn list_versions_java_returns_nonempty_list() {
    let candidate = Candidate::new("java");
    let platform = Platform::current().unwrap();
    let sdk = broker::list_versions(&candidate, &platform).unwrap();
    assert!(!sdk.rows.is_empty());
    assert_eq!(sdk.candidate, "java");
}

#[test]
#[ignore = "requires network access to SDKMAN API"]
fn list_versions_java_rows_have_identifiers() {
    let candidate = Candidate::new("java");
    let platform = Platform::current().unwrap();
    let sdk = broker::list_versions(&candidate, &platform).unwrap();
    // Every Java row should have an identifier (e.g. "21.0.7-tem").
    let missing: Vec<_> = sdk.rows.iter()
        .filter(|r| r.identifier.is_none())
        .collect();
    assert!(missing.is_empty(), "rows missing identifiers: {missing:?}");
}

#[test]
#[ignore = "requires network access to SDKMAN API"]
fn list_versions_gradle_returns_nonempty_list() {
    let candidate = Candidate::new("gradle");
    let platform = Platform::current().unwrap();
    let sdk = broker::list_versions(&candidate, &platform).unwrap();
    assert!(!sdk.rows.is_empty());
}

// ── fetch_hook ────────────────────────────────────────────────────────────────

#[test]
#[ignore = "requires network access to SDKMAN API"]
fn fetch_hook_java_returns_known_fingerprint() {
    use types::HookFingerprint;

    let candidate = Candidate::new("java");
    let platform = Platform::current().unwrap();

    // Find any installed-style identifier from the live list.
    let sdk = broker::list_versions(&candidate, &platform).unwrap();
    let row = sdk.rows.iter()
        .find(|r| r.identifier.is_some())
        .expect("no java versions found");
    let identifier = types::Identifier::new(row.identifier.clone().unwrap());

    let hook = broker::fetch_hook(&candidate, &identifier, &platform).unwrap();
    assert!(
        !matches!(hook.fingerprint, HookFingerprint::Unknown { .. }),
        "unrecognised hook fingerprint for {} {}: {:?}\n\n=== RAW HOOK ===\n{}\n\n=== NORMALIZED ===\n{}",
        candidate, identifier, hook.fingerprint, hook.raw, hook.normalized
    );
}

// ── download_archive ──────────────────────────────────────────────────────────

#[test]
#[ignore = "requires network access to SDKMAN API; downloads a full archive"]
fn download_archive_gradle_succeeds_and_file_exists() {
    let candidate = Candidate::new("gradle");
    let platform = Platform::current().unwrap();

    // Pick the most recent gradle version.
    let sdk = broker::list_versions(&candidate, &platform).unwrap();
    let row = sdk.rows.first().expect("no gradle versions");
    let identifier = types::Identifier::new(
        row.identifier.clone().unwrap_or_else(|| row.version.clone())
    );

    let dl = broker::download_archive(&candidate, &identifier, &platform).unwrap();
    assert!(dl.path.exists(), "downloaded file not found at {:?}", dl.path);
    assert!(dl.path.metadata().unwrap().len() > 0, "downloaded file is empty");

    // Clean up.
    let _ = std::fs::remove_file(&dl.path);
}
