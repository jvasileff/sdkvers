# sdkalt Rust Implementation Plan

This document defines the Rust crate structure and public API for sdkalt. It covers workspace layout, shared types, and the public function signatures for each layer.

## Workspace Layout

```
Cargo.toml              # workspace root
crates/
  types/                # shared domain types — no workspace dependencies
  broker/               # SDKMAN web API client — depends on: types
  store/                # local SDKMAN directory operations — depends on: types
  ops/                  # high-level operations — depends on: types, broker, store
  sdkalt/               # CLI binary — depends on: ops, types; uses anyhow
```

Dependency graph:

```
sdkalt → ops → broker → types
              → store  ↗
```

`broker` and `store` are independent of each other. Neither can call into `ops`. The compiler enforces this.

## Error Handling

Each library crate (`broker`, `store`, `ops`) defines its own error enum using `thiserror`. Public functions return `Result<T, CrateError>`.

The `ops` crate wraps `broker::Error` and `store::Error` directly via `#[from]`, rather than translating them into separate domain variants. This keeps the CLI code simple — it gets the lower-level error's `Display` message directly without needing to match on broker/store internals.

Error struct fields use `String` rather than domain types (`Candidate`, `Identifier`) for ergonomic `thiserror` display formatting.

The `sdkalt` binary uses `anyhow` for ergonomic error propagation and display.

Suggested dependencies: `thiserror` in all library crates, `anyhow` in `sdkalt` only.

---

## `types` crate

Shared domain types used across all layers. No network, no filesystem, no external dependencies beyond standard parsing utilities.

**Note:** `ArchiveFormat` and `HookFingerprint` live here (not in `broker`) so that `store` can use them without creating a circular dependency between `broker` and `store`.

Suggested dependencies: none beyond `std`.

### Types

```rust
/// A SDKMAN candidate name, e.g. "java", "gradle".
pub struct Candidate(pub String);

/// A fully qualified SDKMAN version identifier, e.g. "21.0.7-tem", "8.7".
/// For Java, encodes both version and vendor/distribution.
pub struct Identifier(pub String);

/// The version portion of an identifier, e.g. "21.0.7" from "21.0.7-tem".
pub struct Version(pub String);

/// The vendor/distribution portion of a Java identifier, e.g. "tem" from "21.0.7-tem".
pub struct Vendor(pub String);

/// A parsed SDKMAN identifier with optional vendor field.
pub struct ParsedIdentifier {
    pub candidate: Candidate,
    pub identifier: Identifier,
    pub version: Version,
    pub vendor: Option<Vendor>,
}

/// Target platform for API requests.
pub enum Platform {
    LinuxX64,
    LinuxArm64,
    LinuxArm32Hf,
    DarwinX64,
    DarwinArm64,
}

/// A version expression: a bare version, an explicit Maven-style range, or a wildcard.
/// Stored as a raw string; matching logic is stub-only (prefix match + "*"/"latest").
/// Full range semantics are deferred to a future implementation.
pub struct VersionExpr(String);

/// Archive format of a downloaded SDK binary.
pub enum ArchiveFormat {
    Zip,
    TarGz,
}

/// The recognised hook templates, determined by normalised MD5 fingerprint.
/// Variants that carry data extract those values from the raw hook before discarding it.
pub enum HookFingerprint {
    DefaultZip,
    DefaultTarball,
    LinuxJavaTarball,
    OsxJavaTarball,
    UnixJmcTarballFolder { executable_binary: String },
    UnixJmcTarballFlat   { executable_binary: String },
    /// Hook did not match any known fingerprint.
    Unknown { hash: String },
}
```

```rust
impl Platform {
    /// Detect the current system's platform.
    pub fn current() -> Result<Platform, types::Error>;

    /// Convert to the platform string used in SDKMAN API URLs, e.g. "linuxx64".
    pub fn as_api_str(&self) -> &'static str;
}

impl Identifier {
    /// Parse a raw SDKMAN identifier string into its components.
    pub fn parse(candidate: &Candidate, raw: &str) -> Result<ParsedIdentifier, types::Error>;
}

impl VersionExpr {
    pub fn parse(s: &str) -> Result<Self, types::Error>;

    /// Returns true if the expression matches the given version.
    /// Stub: prefix match, plus "*" and "latest" match everything.
    pub fn matches(&self, version: &Version) -> bool;

    /// Returns true if the expression denotes exactly one version (not a range).
    /// Stub: ranges start with '[' or '('; "*" and "latest" are not exact.
    pub fn is_exact(&self) -> bool;
}
```

---

## `broker` crate

HTTP client for the SDKMAN Broker API. All network access is isolated here. Maintains an in-process URL deduplication cache (`LazyLock<Mutex<HashMap>>`) so the same URL is never fetched twice within a single invocation.

Uses `reqwest` in blocking mode — no async runtime. Appropriate for a CLI with no parallelism requirement.

Suggested dependencies: `reqwest` (blocking feature), `thiserror`, `md5`.

### Errors

```rust
pub enum Error {
    /// HTTP request failed.
    Network(reqwest::Error),
    /// Server returned an unexpected status code.
    UnexpectedStatus { url: String, status: u16 },
    /// Response body could not be decoded as UTF-8.
    Encoding(std::string::FromUtf8Error),
    /// Filesystem error writing downloaded archive to temp file.
    Io(std::io::Error),
    /// Version list response was in an unrecognised format.
    UnrecognisedVersionListFormat(String),
}
```

### Types

```rust
/// Metadata for a single available version returned by the version list API.
pub struct RemoteVersion {
    pub identifier: Identifier,
    pub version: Version,
    pub vendor: Option<Vendor>,
}

/// A downloaded archive, held as a temporary file pending extraction.
pub struct DownloadedArchive {
    pub path: std::path::PathBuf,
    pub format: ArchiveFormat,
    pub checksum_sha256: Option<String>,
    pub checksum_md5: Option<String>,
}

/// A fetched hook script with its raw text and computed fingerprint.
pub struct FetchedHook {
    pub raw: String,
    pub fingerprint: HookFingerprint,
}
```

(`ArchiveFormat` and `HookFingerprint` are defined in `types` and re-used here.)

### Functions

```rust
/// Return all available SDKMAN candidate names.
/// Hits GET /candidates/all.
pub fn list_candidates() -> Result<Vec<Candidate>, Error>;

/// Return all available versions for a candidate on a given platform.
/// Hits GET /candidates/{candidate}/{platform}/versions/list.
/// Handles both the Java pipe-delimited table format and the plain whitespace layout.
pub fn list_versions(
    candidate: &Candidate,
    platform: &Platform,
) -> Result<Vec<RemoteVersion>, Error>;

/// Download the archive for a specific candidate version to a temp file.
/// Hits GET /broker/download/{candidate}/{identifier}/{platform}.
/// Checksums are read from response headers (x-sdkman-checksum-sha256,
/// x-sdkman-checksum-md5) and stored in the returned struct.
pub fn download_archive(
    candidate: &Candidate,
    identifier: &Identifier,
    platform: &Platform,
) -> Result<DownloadedArchive, Error>;

/// Fetch the post-install hook script for a candidate version.
/// Hits GET /hooks/post/{candidate}/{identifier}/{platform}.
/// Normalises and fingerprints the script (see SDKALT-HOOKS.md).
/// Returns a default fingerprint (DefaultZip or DefaultTarball) if hook body is empty.
pub fn fetch_hook(
    candidate: &Candidate,
    identifier: &Identifier,
    platform: &Platform,
) -> Result<FetchedHook, Error>;
```

---

## `store` crate

Filesystem operations on `$SDKMAN_DIR/candidates/`. No network access. All paths are derived from `SDKMAN_DIR` (from environment, fallback `$HOME/.sdkman`).

**Note:** `store` does not depend on `broker`. Signatures that logically involve broker types (`ArchiveFormat`, `HookFingerprint`) accept them from `types` directly. Signatures that would otherwise need `DownloadedArchive` or `FetchedHook` take their component fields individually.

Suggested dependencies: `thiserror`, `tar`, `flate2`, `zip`, `md5`, `sha2`.

### Errors

```rust
pub enum Error {
    /// SDKMAN_DIR could not be determined.
    SdkmanDirUnknown,
    /// A filesystem operation failed.
    Io(std::io::Error),
    /// The candidate directory does not exist.
    CandidateNotFound(String),
    /// The specified version is not installed.
    VersionNotInstalled { candidate: String, identifier: String },
    /// Archive checksum verification failed.
    ChecksumMismatch { expected: String, actual: String },
    /// Hook fingerprint was not recognised; install cannot proceed.
    UnknownHookFingerprint { candidate: String, identifier: String, hash: String },
    /// Extracted archive did not produce the expected layout.
    UnexpectedLayout { candidate: String, identifier: String, detail: String },
}
```

### Types

```rust
/// An installed version found in the local candidates directory.
pub struct InstalledVersion {
    pub candidate: Candidate,
    pub identifier: Identifier,
    pub version: Version,
    pub vendor: Option<Vendor>,
    pub is_current: bool,
}
```

### Functions

```rust
/// Return all locally installed versions for a candidate.
/// Reads $SDKMAN_DIR/candidates/{candidate}/, skipping the `current` symlink entry.
pub fn list_installed(candidate: &Candidate) -> Result<Vec<InstalledVersion>, Error>;

/// Return the identifier of the currently active version for a candidate, if any.
/// Reads the `current` symlink target.
pub fn get_current(candidate: &Candidate) -> Result<Option<Identifier>, Error>;

/// Return currently active versions for all candidates that have a `current` symlink.
pub fn get_all_current() -> Result<Vec<(Candidate, Identifier)>, Error>;

/// Update (or create) the `current` symlink for a candidate to point to the given identifier.
/// The version must already be installed.
pub fn set_current(candidate: &Candidate, identifier: &Identifier) -> Result<(), Error>;

/// Remove the `current` symlink for a candidate, leaving no default set.
pub fn clear_current(candidate: &Candidate) -> Result<(), Error>;

/// Verify the checksum of a downloaded archive file.
/// Prefers SHA-256; falls back to MD5. If neither is provided, returns Ok(()) with a warning.
/// Takes the path and checksum strings separately (not DownloadedArchive) to avoid
/// a dependency on the broker crate.
pub fn verify_checksum(
    archive: &Path,
    sha256: Option<&str>,
    md5: Option<&str>,
) -> Result<(), Error>;

/// Extract a downloaded archive to $SDKMAN_DIR/candidates/{candidate}/{identifier}/.
/// Behaviour is determined by the hook fingerprint:
///   DefaultZip                — extract zip, strip leading dir
///   DefaultTarball / LinuxJavaTarball — extract tar.gz, strip leading dir
///   OsxJavaTarball            — extract tar.gz, use Contents/Home/ as root
///   UnixJmcTarballFolder      — extract tar.gz, strip leading dir, create bin/jmc symlink
///   UnixJmcTarballFlat        — extract tar.gz flat, create bin/jmc symlink
///   Unknown                   — returns Error::UnknownHookFingerprint
/// Takes path, format, and fingerprint separately (not DownloadedArchive / FetchedHook)
/// to avoid a dependency on the broker crate.
/// Cleans up the archive file on both success and failure.
pub fn extract(
    candidate: &Candidate,
    identifier: &Identifier,
    archive: &Path,
    format: &ArchiveFormat,
    fingerprint: &HookFingerprint,
) -> Result<(), Error>;

/// Remove an installed version.
/// Deletes $SDKMAN_DIR/candidates/{candidate}/{identifier}/.
/// Returns Error::VersionNotInstalled if not present.
/// Defined in extraction.rs, re-exported from store root.
pub fn remove(candidate: &Candidate, identifier: &Identifier) -> Result<(), Error>;

/// Return the filesystem path for an installed version.
pub fn version_path(
    candidate: &Candidate,
    identifier: &Identifier,
) -> Result<std::path::PathBuf, Error>;
```

---

## `ops` crate

High-level operations that compose `broker` and `store`. This is the layer the CLI calls directly.

`ops` calls `Platform::current()` internally rather than accepting platform as a parameter — callers don't need to know about platform detection.

Suggested dependencies: `thiserror`, `types`, `broker`, `store`.

### Errors

```rust
pub enum Error {
    /// Broker error (network, unexpected status, encoding, etc.).
    Broker(broker::Error),
    /// Store error (filesystem, checksum, unknown fingerprint, etc.).
    Store(store::Error),
    /// Platform could not be detected for this system.
    Platform(types::Error),
    /// No version matching the given expression (and optional vendor) is available.
    NoMatch { candidate: String, expr: String },
    /// Expression was exact but matched more than one version.
    Ambiguous { candidate: String, expr: String, count: usize },
    /// The resolved version is already installed.
    AlreadyInstalled { candidate: String, identifier: String },
}
```

All three lower-crate variants use `#[from]` so `?` works directly without explicit mapping.

### Types

```rust
/// A version entry returned by the list command.
pub struct ListEntry {
    pub identifier: Identifier,
    pub version: Version,
    pub vendor: Option<Vendor>,
    pub installed: bool,
    pub is_current: bool,
}
```

### Functions

```rust
/// List available versions for a candidate, annotated with local install status.
pub fn list(candidate: &Candidate) -> Result<Vec<ListEntry>, Error>;

/// Resolve a version expression and install the matching version.
/// Downloads archive, verifies checksum, fingerprints hook, extracts.
/// Returns the installed identifier.
pub fn install(
    candidate: &Candidate,
    expr: &VersionExpr,
    vendor_filter: Option<&str>,
) -> Result<Identifier, Error>;

/// Set an installed version as the current default (persistent).
/// Updates the `current` symlink. The identifier must already be installed.
pub fn set_default(candidate: &Candidate, identifier: &Identifier) -> Result<(), Error>;

/// Set an installed version as current for the shell session.
/// Updates the `current` symlink. The identifier must already be installed.
/// The CLI is responsible for emitting eval-able env-var output (JAVA_HOME, PATH, etc.).
pub fn use_version(candidate: &Candidate, identifier: &Identifier) -> Result<(), Error>;

/// Remove an installed version.
/// Clears the `current` symlink first if it points to the removed version.
pub fn uninstall(candidate: &Candidate, identifier: &Identifier) -> Result<(), Error>;

/// Return the identifier of the currently active version for a candidate.
pub fn current(candidate: &Candidate) -> Result<Option<Identifier>, Error>;
```

### Not yet implemented

- `list_candidates()` in ops (currently only in broker)
- Filtering `list()` by expression / vendor
- `ActivationCommands` / shell eval output for `use` and `default`
- `current_all()` — current version for every candidate
- Full `VersionExpr` matching (currently a prefix-match stub)

---

## `sdkalt` crate (CLI binary)

The CLI binary. Parses command-line arguments, calls `ops`, and handles output formatting and error display. Uses `anyhow` for error propagation. For commands that modify the shell environment (`use`, `default`), the `sdkalt` shell function evals stdout; all other output goes to stderr.

Suggested dependencies: `clap` (argument parsing), `anyhow`, `ops`, `types`.
