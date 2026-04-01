use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};

use types::{Candidate, Identifier};

// Serialize all tests that touch SDKMAN_DIR so they don't interfere.
static ENV_LOCK: Mutex<()> = Mutex::new(());

// ── Test fixture ──────────────────────────────────────────────────────────────

struct TestSdkman {
    dir: PathBuf,
    _temp: tempfile::TempDir,
    _guard: MutexGuard<'static, ()>,
}

impl TestSdkman {
    fn new() -> Self {
        let guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let temp = tempfile::TempDir::new().unwrap();
        let dir = temp.path().to_path_buf();
        std::env::set_var("SDKMAN_DIR", &dir);
        std::fs::create_dir_all(dir.join("candidates")).unwrap();
        TestSdkman { dir, _temp: temp, _guard: guard }
    }

    fn candidates_dir(&self) -> PathBuf {
        self.dir.join("candidates")
    }

    /// Create an installed version directory and return its path.
    fn add_version(&self, candidate: &str, identifier: &str) -> PathBuf {
        let path = self.candidates_dir().join(candidate).join(identifier);
        std::fs::create_dir_all(&path).unwrap();
        path
    }

    /// Create the `current` symlink for a candidate pointing at identifier.
    fn set_current_symlink(&self, candidate: &str, identifier: &str) {
        let cand_dir = self.candidates_dir().join(candidate);
        let link = cand_dir.join("current");
        if link.symlink_metadata().is_ok() {
            std::fs::remove_file(&link).unwrap();
        }
        #[cfg(unix)]
        std::os::unix::fs::symlink(identifier, &link).unwrap();
    }
}

impl Drop for TestSdkman {
    fn drop(&mut self) {
        std::env::remove_var("SDKMAN_DIR");
    }
}

// ── list_candidates ───────────────────────────────────────────────────────────

#[test]
fn list_candidates_empty_when_no_candidates_installed() {
    let sdk = TestSdkman::new();
    let result = store::list_candidates().unwrap();
    assert!(result.is_empty(), "expected no candidates, got {result:?}");
    drop(sdk);
}

#[test]
fn list_candidates_returns_sorted_names() {
    let sdk = TestSdkman::new();
    sdk.add_version("scala", "3.4.0");
    sdk.add_version("java", "21.0.7-tem");
    sdk.add_version("gradle", "8.5");

    let result = store::list_candidates().unwrap();
    let names: Vec<&str> = result.iter().map(|c| c.as_str()).collect();
    assert_eq!(names, ["gradle", "java", "scala"]);
    drop(sdk);
}

#[test]
fn list_candidates_skips_files() {
    let sdk = TestSdkman::new();
    sdk.add_version("java", "21.0.7-tem");
    // Create a file (not a directory) in candidates/.
    std::fs::write(sdk.candidates_dir().join("not-a-candidate"), "").unwrap();

    let result = store::list_candidates().unwrap();
    let names: Vec<&str> = result.iter().map(|c| c.as_str()).collect();
    assert_eq!(names, ["java"]);
    drop(sdk);
}

// ── list_installed ────────────────────────────────────────────────────────────

#[test]
fn list_installed_error_when_candidate_not_found() {
    let sdk = TestSdkman::new();
    let cand = Candidate::new("java");
    let err = store::list_installed(&cand).unwrap_err();
    assert!(err.to_string().contains("java"), "error should mention candidate: {err}");
    drop(sdk);
}

#[test]
fn list_installed_returns_versions_for_candidate() {
    let sdk = TestSdkman::new();
    sdk.add_version("gradle", "8.5");
    sdk.add_version("gradle", "8.4");

    let cand = Candidate::new("gradle");
    let mut result = store::list_installed(&cand).unwrap();
    result.sort_by(|a, b| a.identifier.as_str().cmp(b.identifier.as_str()));

    assert_eq!(result.len(), 2);
    assert_eq!(result[0].identifier.as_str(), "8.4");
    assert_eq!(result[1].identifier.as_str(), "8.5");
    drop(sdk);
}

#[test]
fn list_installed_marks_current_version() {
    let sdk = TestSdkman::new();
    sdk.add_version("gradle", "8.5");
    sdk.add_version("gradle", "8.4");
    sdk.set_current_symlink("gradle", "8.5");

    let cand = Candidate::new("gradle");
    let result = store::list_installed(&cand).unwrap();

    let current: Vec<_> = result.iter().filter(|v| v.is_current).collect();
    assert_eq!(current.len(), 1);
    assert_eq!(current[0].identifier.as_str(), "8.5");

    let not_current: Vec<_> = result.iter().filter(|v| !v.is_current).collect();
    assert_eq!(not_current.len(), 1);
    assert_eq!(not_current[0].identifier.as_str(), "8.4");
    drop(sdk);
}

#[test]
fn list_installed_no_current_when_no_symlink() {
    let sdk = TestSdkman::new();
    sdk.add_version("gradle", "8.5");

    let cand = Candidate::new("gradle");
    let result = store::list_installed(&cand).unwrap();
    assert!(!result[0].is_current);
    drop(sdk);
}

#[test]
fn list_installed_parses_java_version_and_vendor() {
    let sdk = TestSdkman::new();
    sdk.add_version("java", "21.0.7-tem");
    sdk.add_version("java", "17.0.9-graalce");

    let cand = Candidate::new("java");
    let mut result = store::list_installed(&cand).unwrap();
    result.sort_by(|a, b| a.identifier.as_str().cmp(b.identifier.as_str()));

    assert_eq!(result[0].version.as_str(), "17.0.9");
    assert_eq!(result[0].vendor.as_ref().unwrap().as_str(), "graalce");
    assert_eq!(result[1].version.as_str(), "21.0.7");
    assert_eq!(result[1].vendor.as_ref().unwrap().as_str(), "tem");
    drop(sdk);
}

#[test]
fn list_installed_skips_current_symlink_entry() {
    let sdk = TestSdkman::new();
    sdk.add_version("gradle", "8.5");
    sdk.set_current_symlink("gradle", "8.5");

    let cand = Candidate::new("gradle");
    let result = store::list_installed(&cand).unwrap();
    // Should only have "8.5", not a separate "current" entry.
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].identifier.as_str(), "8.5");
    drop(sdk);
}

// ── get_current / set_current / clear_current ─────────────────────────────────

#[test]
fn get_current_returns_none_when_no_symlink() {
    let sdk = TestSdkman::new();
    sdk.add_version("gradle", "8.5");

    let cand = Candidate::new("gradle");
    let result = store::get_current(&cand).unwrap();
    assert!(result.is_none());
    drop(sdk);
}

#[test]
fn set_current_and_get_current_roundtrip() {
    let sdk = TestSdkman::new();
    sdk.add_version("gradle", "8.5");

    let cand = Candidate::new("gradle");
    let ident = Identifier::new("8.5");
    store::set_current(&cand, &ident).unwrap();

    let current = store::get_current(&cand).unwrap();
    assert_eq!(current.as_ref().map(|i| i.as_str()), Some("8.5"));
    drop(sdk);
}

#[test]
fn set_current_updates_existing_symlink() {
    let sdk = TestSdkman::new();
    sdk.add_version("gradle", "8.5");
    sdk.add_version("gradle", "8.4");
    sdk.set_current_symlink("gradle", "8.4");

    let cand = Candidate::new("gradle");
    let ident = Identifier::new("8.5");
    store::set_current(&cand, &ident).unwrap();

    let current = store::get_current(&cand).unwrap();
    assert_eq!(current.as_ref().map(|i| i.as_str()), Some("8.5"));
    drop(sdk);
}

#[test]
fn set_current_errors_when_version_not_installed() {
    let sdk = TestSdkman::new();
    sdk.add_version("gradle", "8.5");

    let cand = Candidate::new("gradle");
    let ident = Identifier::new("8.4");
    let err = store::set_current(&cand, &ident).unwrap_err();
    assert!(err.to_string().contains("8.4"), "error should mention identifier: {err}");
    drop(sdk);
}

#[test]
fn clear_current_removes_symlink() {
    let sdk = TestSdkman::new();
    sdk.add_version("gradle", "8.5");
    sdk.set_current_symlink("gradle", "8.5");

    let cand = Candidate::new("gradle");
    store::clear_current(&cand).unwrap();

    let current = store::get_current(&cand).unwrap();
    assert!(current.is_none());
    drop(sdk);
}

#[test]
fn clear_current_is_ok_when_no_symlink_exists() {
    let sdk = TestSdkman::new();
    sdk.add_version("gradle", "8.5");

    let cand = Candidate::new("gradle");
    store::clear_current(&cand).unwrap(); // should not error
    drop(sdk);
}

// ── remove ────────────────────────────────────────────────────────────────────

#[test]
fn remove_deletes_version_directory() {
    let sdk = TestSdkman::new();
    let path = sdk.add_version("gradle", "8.5");

    let cand = Candidate::new("gradle");
    let ident = Identifier::new("8.5");
    store::remove(&cand, &ident).unwrap();

    assert!(!path.exists());
    drop(sdk);
}

#[test]
fn remove_errors_when_version_not_installed() {
    let sdk = TestSdkman::new();
    sdk.add_version("gradle", "8.5");

    let cand = Candidate::new("gradle");
    let ident = Identifier::new("8.4");
    let err = store::remove(&cand, &ident).unwrap_err();
    assert!(err.to_string().contains("8.4"), "error should mention identifier: {err}");
    drop(sdk);
}

// ── version_path ──────────────────────────────────────────────────────────────

#[test]
fn version_path_returns_expected_path() {
    let sdk = TestSdkman::new();

    let cand = Candidate::new("java");
    let ident = Identifier::new("21.0.7-tem");
    let path = store::version_path(&cand, &ident).unwrap();

    let expected = sdk.dir.join("candidates").join("java").join("21.0.7-tem");
    assert_eq!(path, expected);
    drop(sdk);
}
