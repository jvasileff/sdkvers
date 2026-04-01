/// Tests for the ops crate.
///
/// Local-only tests (current, use_version, uninstall, list_installed,
/// list_local_candidates) run unconditionally using a temp SDKMAN_DIR.
///
/// Tests that require the broker (install, list, list_remote_candidates)
/// are #[ignore] by default; run with:
///   cargo test -p ops -- --ignored
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};

use types::{Candidate, Identifier};

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

    fn add_version(&self, candidate: &str, identifier: &str) {
        let path = self.dir.join("candidates").join(candidate).join(identifier);
        std::fs::create_dir_all(&path).unwrap();
    }

    fn set_current_symlink(&self, candidate: &str, identifier: &str) {
        let cand_dir = self.dir.join("candidates").join(candidate);
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

// ── current ───────────────────────────────────────────────────────────────────

#[test]
fn current_returns_none_when_no_version_active() {
    let sdk = TestSdkman::new();
    sdk.add_version("gradle", "8.5");

    let cand = Candidate::new("gradle");
    let result = ops::current(&cand).unwrap();
    assert!(result.is_none());
    drop(sdk);
}

#[test]
fn current_returns_active_identifier() {
    let sdk = TestSdkman::new();
    sdk.add_version("gradle", "8.5");
    sdk.set_current_symlink("gradle", "8.5");

    let cand = Candidate::new("gradle");
    let result = ops::current(&cand).unwrap();
    assert_eq!(result.as_ref().map(|i| i.as_str()), Some("8.5"));
    drop(sdk);
}

// ── use_version ───────────────────────────────────────────────────────────────

#[test]
fn use_version_sets_current() {
    let sdk = TestSdkman::new();
    sdk.add_version("gradle", "8.5");
    sdk.add_version("gradle", "8.4");
    sdk.set_current_symlink("gradle", "8.4");

    let cand = Candidate::new("gradle");
    let ident = Identifier::new("8.5");
    ops::use_version(&cand, &ident).unwrap();

    let current = ops::current(&cand).unwrap();
    assert_eq!(current.as_ref().map(|i| i.as_str()), Some("8.5"));
    drop(sdk);
}

#[test]
fn use_version_errors_when_not_installed() {
    let sdk = TestSdkman::new();
    sdk.add_version("gradle", "8.5");

    let cand = Candidate::new("gradle");
    let ident = Identifier::new("8.4");
    let err = ops::use_version(&cand, &ident).unwrap_err();
    assert!(err.to_string().contains("8.4"), "error should mention identifier: {err}");
    drop(sdk);
}

// ── uninstall ─────────────────────────────────────────────────────────────────

#[test]
fn uninstall_removes_version_directory() {
    let sdk = TestSdkman::new();
    sdk.add_version("gradle", "8.5");

    let cand = Candidate::new("gradle");
    let ident = Identifier::new("8.5");
    ops::uninstall(&cand, &ident).unwrap();

    let installed = ops::list_installed(&cand).unwrap();
    assert!(installed.is_empty());
    drop(sdk);
}

#[test]
fn uninstall_clears_current_symlink_when_active_version_removed() {
    let sdk = TestSdkman::new();
    sdk.add_version("gradle", "8.5");
    sdk.set_current_symlink("gradle", "8.5");

    let cand = Candidate::new("gradle");
    let ident = Identifier::new("8.5");
    ops::uninstall(&cand, &ident).unwrap();

    let current = ops::current(&cand).unwrap();
    assert!(current.is_none(), "current should be cleared after uninstalling active version");
    drop(sdk);
}

#[test]
fn uninstall_preserves_current_symlink_when_inactive_version_removed() {
    let sdk = TestSdkman::new();
    sdk.add_version("gradle", "8.5");
    sdk.add_version("gradle", "8.4");
    sdk.set_current_symlink("gradle", "8.5");

    let cand = Candidate::new("gradle");
    let ident = Identifier::new("8.4");
    ops::uninstall(&cand, &ident).unwrap();

    let current = ops::current(&cand).unwrap();
    assert_eq!(current.as_ref().map(|i| i.as_str()), Some("8.5"));
    drop(sdk);
}

// ── get_all_current ───────────────────────────────────────────────────────────

#[test]
fn get_all_current_returns_active_versions_across_candidates() {
    let sdk = TestSdkman::new();
    sdk.add_version("java", "21.0.7-tem");
    sdk.add_version("gradle", "8.5");
    sdk.add_version("gradle", "8.4");
    sdk.set_current_symlink("java", "21.0.7-tem");
    sdk.set_current_symlink("gradle", "8.5");

    let mut result = ops::get_all_current().unwrap();
    result.sort_by(|a, b| a.0.as_str().cmp(b.0.as_str()));

    assert_eq!(result.len(), 2);
    assert_eq!(result[0].0.as_str(), "gradle");
    assert_eq!(result[0].1.as_str(), "8.5");
    assert_eq!(result[1].0.as_str(), "java");
    assert_eq!(result[1].1.as_str(), "21.0.7-tem");
    drop(sdk);
}

#[test]
fn get_all_current_excludes_candidates_with_no_active_version() {
    let sdk = TestSdkman::new();
    sdk.add_version("java", "21.0.7-tem");
    sdk.add_version("gradle", "8.5");
    sdk.set_current_symlink("gradle", "8.5");
    // java has no current symlink

    let result = ops::get_all_current().unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].0.as_str(), "gradle");
    drop(sdk);
}

// ── list_installed ────────────────────────────────────────────────────────────

#[test]
fn list_installed_returns_installed_versions() {
    let sdk = TestSdkman::new();
    sdk.add_version("gradle", "8.5");
    sdk.add_version("gradle", "8.4");

    let cand = Candidate::new("gradle");
    let mut result = ops::list_installed(&cand).unwrap();
    result.sort_by(|a, b| a.identifier.as_str().cmp(b.identifier.as_str()));

    assert_eq!(result.len(), 2);
    assert_eq!(result[0].identifier.as_str(), "8.4");
    assert_eq!(result[1].identifier.as_str(), "8.5");
    drop(sdk);
}

// ── list_local_candidates ─────────────────────────────────────────────────────

#[test]
fn list_local_candidates_returns_installed_candidates() {
    let sdk = TestSdkman::new();
    sdk.add_version("java", "21.0.7-tem");
    sdk.add_version("gradle", "8.5");

    let mut result = ops::list_local_candidates().unwrap();
    result.sort_by(|a, b| a.as_str().cmp(b.as_str()));
    let names: Vec<&str> = result.iter().map(|c| c.as_str()).collect();

    assert_eq!(names, ["gradle", "java"]);
    drop(sdk);
}

#[test]
fn list_local_candidates_empty_when_nothing_installed() {
    let sdk = TestSdkman::new();
    let result = ops::list_local_candidates().unwrap();
    assert!(result.is_empty());
    drop(sdk);
}

// ── list_remote_candidates (network) ─────────────────────────────────────────

#[test]
#[ignore = "requires network access to SDKMAN API"]
fn list_remote_candidates_includes_java_and_gradle() {
    let _sdk = TestSdkman::new();
    let candidates = ops::list_remote_candidates().unwrap();
    let names: Vec<&str> = candidates.iter().map(|c| c.as_str()).collect();
    assert!(names.contains(&"java"));
    assert!(names.contains(&"gradle"));
}

// ── list (network) ────────────────────────────────────────────────────────────

#[test]
#[ignore = "requires network access to SDKMAN API"]
fn list_gradle_returns_entries_with_install_status() {
    let sdk = TestSdkman::new();
    sdk.add_version("gradle", "8.4");
    sdk.set_current_symlink("gradle", "8.4");

    let cand = Candidate::new("gradle");
    let entries = ops::list(&cand).unwrap();

    assert!(!entries.is_empty());

    // The locally installed version should be marked as installed and current.
    let entry_8_4 = entries.iter().find(|e| {
        e.row.identifier.as_deref() == Some("8.4") || e.row.version == "8.4"
    });
    if let Some(entry) = entry_8_4 {
        assert!(entry.installed, "8.4 should be marked installed");
        assert!(entry.is_current, "8.4 should be marked current");
    }

    drop(sdk);
}

// ── install (network) ─────────────────────────────────────────────────────────

#[test]
#[ignore = "requires network access to SDKMAN API; downloads and installs a full archive"]
fn install_gradle_latest() {
    let sdk = TestSdkman::new();

    let cand = Candidate::new("gradle");
    let expr = types::VersionParser::new("8").parse_version_expr().unwrap();
    let identifier = ops::install(&cand, &expr, None).unwrap();

    assert!(identifier.as_str().starts_with("8."), "expected 8.x, got {identifier}");

    let installed = ops::list_installed(&cand).unwrap();
    let ids: Vec<&str> = installed.iter().map(|v| v.identifier.as_str()).collect();
    assert!(ids.contains(&identifier.as_str()), "installed list should contain {identifier}");

    drop(sdk);
}
