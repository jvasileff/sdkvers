# sdkalt Specification

sdkalt is an alternate SDKMAN client that adds support for version range expressions and vendor filtering to the standard SDK management workflow. It installs into and is fully compatible with SDKMAN's candidates directory.

## Table of Contents

1. [Overview](#1-overview)
2. [Commands](#2-commands)
   - [list](#21-list)
   - [install](#22-install)
   - [uninstall](#23-uninstall)
   - [use](#24-use)
   - [default](#25-default)
   - [current](#26-current)
3. [Version Expressions](#3-version-expressions)
4. [Vendor Filtering](#4-vendor-filtering)
5. [Archive Extraction](#5-archive-extraction)
6. [Shell Environment Setup](#6-shell-environment-setup)
7. [SDKMAN Compatibility](#7-sdkman-compatibility)
8. [Implementation Notes](#8-implementation-notes)
9. [Out of Scope / Future Work](#9-out-of-scope--future-work)

## 1. Overview

sdkalt provides an alternative command-line interface for managing software development kits. It uses the SDKMAN Broker API (`https://api.sdkman.io/2`) for remote candidate and version discovery, and installs SDKs into SDKMAN's standard candidates directory (`$SDKMAN_DIR/candidates/`). Installations made by sdkalt are fully compatible with the official SDKMAN CLI and vice versa.

The primary enhancement over the official `sdk` CLI is support for **version range expressions** and **explicit vendor filtering** in all commands that accept a version. Rather than specifying an exact identifier like `21.0.7-tem`, you can write `21 tem` or `[21,22) tem` and sdkalt resolves the best matching version automatically.

sdkalt does **not** wrap or delegate to the SDKMAN bash CLI. It communicates with the SDKMAN Broker API directly and manages the candidates directory itself.

## 2. Commands

### 2.1 `list`

```
sdkalt list
sdkalt list <candidate>
sdkalt list <candidate> [<version-expr>] [<vendor>]
```

**Without a candidate:** Fetches the full list of available candidates from the Broker API and displays them.

**With a candidate:** Fetches available versions for the candidate from the Broker API for the current platform. Displays a table of versions indicating which are installed locally. Accepts an optional version expression and vendor to filter the displayed results.

**Technical notes:**
- Platform is detected automatically from the current system.
- Versions are fetched from `GET /candidates/<candidate>/<platform>/versions/list`.
- Installed versions are determined by reading `$SDKMAN_DIR/candidates/<candidate>/` directly — no subprocess invocation.
- The `current` symlink, if present, is used to annotate which version is currently active.
- Output is sorted by version descending (newest first) using the version comparison rules from the sdkvers specification.
- No local metadata cache in v1; every invocation hits the API.

**Examples:**
```
sdkalt list
sdkalt list java
sdkalt list java 21 tem
sdkalt list java [21,22)
```

---

### 2.2 `install`

```
sdkalt install <candidate> <version-expr> [<vendor>]
```

Downloads and installs the highest available version matching the given expression and optional vendor constraint. The version is resolved against the remote candidate list from the Broker API.

If the resolved version is already installed locally, sdkalt reports this and exits without re-downloading.

After installation, sdkalt prompts whether to set the installed version as the default. This can be suppressed with `--no-default`.

**Technical notes:**

**Version resolution:**
- Fetches available versions from `GET /candidates/<candidate>/<platform>/versions/list`.
- Applies the version expression and vendor filter.
- Selects the highest matching version per the sdkvers selection policy.
- Prerelease versions are excluded unless the expression explicitly opts in (e.g., `[26.ea,)`).

**Download:**
- Downloads the archive from `GET /broker/download/<candidate>/<version>/<platform>`.
- The platform string is detected from the current system (e.g., `linuxx64`, `darwinarm64`).
- The archive is written to a temporary file and deleted after successful extraction.
- No persistent download cache in v1.

**Checksum verification:** See [Section 5.8](#58-checksum-verification).

**Archive extraction:** See [Section 5](#5-archive-extraction).

**Post-install default prompt:**
- If `--no-default` is not specified, sdkalt prompts: `Do you want <candidate> <identifier> to be set as default? (Y/n)`.
- If yes, sdkalt updates the `current` symlink (equivalent to `sdkalt default`).

**Examples:**
```
sdkalt install java 21 tem
sdkalt install java [21,22) graalce
sdkalt install java 21.0.7-tem        # exact identifier also accepted
sdkalt install gradle 8
sdkalt install maven [3.9,4)
```

---

### 2.3 `uninstall`

```
sdkalt uninstall <candidate> <identifier>
```

Removes a locally installed version. The identifier must be an exact SDKMAN version identifier (e.g., `21.0.7-tem`), not a range expression.

**Technical notes:**
- Deletes `$SDKMAN_DIR/candidates/<candidate>/<identifier>/`.
- If the version is currently symlinked as `current`, sdkalt warns the user and removes the symlink, leaving no default set for that candidate.
- Does not affect any shell session currently using the version via `sdkalt use`.

**Examples:**
```
sdkalt uninstall java 21.0.7-tem
sdkalt uninstall gradle 8.6
```

---

### 2.4 `use`

```
sdkalt use <candidate> <version-expr> [<vendor>]
```

Activates an installed version in the current shell session. The version is resolved against locally installed versions only — no network call is made.

Because `use` must modify the current shell's environment variables (e.g., `JAVA_HOME`, `PATH`), the `sdkalt` binary cannot apply these changes directly. Instead, the `sdkalt` shell function (defined in `sdkalt-init.sh`) evals the output of the binary. See [Section 6](#6-shell-environment-setup).

**Technical notes:**
- Resolves against `$SDKMAN_DIR/candidates/<candidate>/` filesystem entries.
- Applies the same version expression and vendor filtering as `install`.
- Emits shell commands to set `<CANDIDATE>_HOME` and update `PATH`.
- Does not update the `current` symlink — use `default` for persistent activation.
- The resolved identifier is used to construct the activation path: `$SDKMAN_DIR/candidates/<candidate>/<identifier>/`.

**Examples:**
```
sdkalt use java 21 tem
sdkalt use java [21,22) graalce
sdkalt use gradle 8
```

---

### 2.5 `default`

```
sdkalt default <candidate> <version-expr> [<vendor>]
```

Sets an installed version as the persistent default and activates it in the current shell session.

**Technical notes:**
- Resolves against locally installed versions (same as `use`).
- Updates (or creates) the symlink `$SDKMAN_DIR/candidates/<candidate>/current` to point to the resolved version directory.
- Also emits shell activation commands (same as `use`), so the new default is immediately active in the current shell.
- Equivalent to SDKMAN's `sdk default`, but accepts range expressions.

**Examples:**
```
sdkalt default java 21 tem
sdkalt default gradle 8
```

---

### 2.6 `current`

```
sdkalt current
sdkalt current <candidate>
```

Displays the currently active version(s).

**Without a candidate:** Reports the active version for every candidate that has a `current` symlink in `$SDKMAN_DIR/candidates/`.

**With a candidate:** Reports the active version for that specific candidate.

**Technical notes:**
- Reads `current` symlinks from the candidates directory — no network call, no subprocess.
- Reports the symlink target name, not the full path.
- If no `current` symlink exists for a candidate, reports "none".

**Examples:**
```
sdkalt current
sdkalt current java
```

---

## 3. Version Expressions

sdkalt uses the same version expression syntax and comparison semantics defined in the sdkvers SPEC.md. This includes:

- **Bare versions**: `21` (major-line range), `21.0` (minor-line range), `21.0.7` (exact match)
- **Explicit ranges**: `[21,22)`, `[17,)`, `[21.0.7]`
- **Exact identifiers**: `21.0.7-tem` (alphanumeric, always treated as exact match)

All commands that accept a `<version-expr>` also accept a plain SDKMAN identifier (e.g., `21.0.7-tem`). When the identifier encodes a vendor suffix and no separate vendor argument is provided, the vendor is parsed from the identifier automatically.

## 4. Vendor Filtering

An optional vendor token may follow the version expression. Vendor matching is exact and case-sensitive.

```
sdkalt install java 21 tem        # Temurin
sdkalt install java 21 graalce    # GraalVM CE
sdkalt install java 21 zulu       # Azul Zulu
```

Vendor filtering applies to candidates that encode a distribution in their identifier (primarily Java). Specifying a vendor for a candidate that does not use distribution identifiers is an error.

## 5. Archive Extraction

sdkalt never executes SDKMAN hook scripts. Instead, it fetches the hook for each install, fingerprints it against a set of known templates, and executes a native Rust implementation of the corresponding extraction behaviour. If the hook does not match any known fingerprint, the install fails with a clear error. If the extraction succeeds but produces an unexpected layout, the install also fails.

This approach avoids executing arbitrary bash, keeps sdkalt self-contained, and is safe because the hook space is provably closed — see Section 5.1.

### 5.1 Hook survey findings

A comprehensive survey of all SDKMAN post-install hooks across all candidates and platforms was conducted to verify that the hook space is finite and fully enumerable. The survey scripts and full results are preserved in [SDKALT-RESEARCH.md](SDKALT-RESEARCH.md). The survey downloaded hooks for every available version of every candidate across all five supported platforms (`linuxx64`, `linuxarm64`, `linuxarm32hf`, `darwinarm64`, `darwinx64`), totalling over 15,000 hooks.

After normalizing for per-invocation variable substitution (identifier, candidate name, vendor, platform description), the entire hook corpus reduces to exactly **6 distinct structural templates**:

| Template | Count | Description |
|---|---|---|
| `default-zip` | 13,775 | Archive is already a properly structured zip |
| `default-tarball` | 570 | tar.gz, no special layout |
| `linux-java-tarball` | 328 | tar.gz, Linux Java (same extraction behaviour as `default-tarball`) |
| `osx-java-tarball` | 155 | tar.gz, macOS Java — extract only `Contents/Home/` |
| `unix-jmc-tarball` (folder) | 12 | tar.gz, JMC with top-level dir — extract, create `bin/jmc` symlink |
| `unix-jmc-tarball` (flat) | 11 | tar.gz, JMC flat archive — extract, create `bin/jmc` symlink |

All hooks within each structural template are byte-for-byte identical after normalization — there are no per-candidate or per-version deviations.

The repack step (tar.gz → zip) present in all official hooks exists solely because the SDKMAN bash CLI only handles zip archives. Since sdkalt extracts both formats natively, the repack is unnecessary.

### 5.2 Hook fingerprinting

The exact normalization steps, known hashes, and per-template extraction procedures are specified in [SDKALT-HOOKS.md](SDKALT-HOOKS.md).

For each install, sdkalt:

1. Fetches the hook from `GET /hooks/post/<candidate>/<version>/<platform>`.
2. Normalizes the hook content (substituting identifier, candidate name, vendor, and platform description with placeholders).
3. Computes the normalized hash and matches it against the 6 known fingerprints.
4. If unrecognized — aborts the install with an error indicating the hook hash, so it can be investigated and a new template added if needed.
5. If recognized — proceeds with the corresponding native extraction branch, reading any hook-embedded parameters (e.g., the JMC executable path) before discarding the script.

### 5.3 Archive formats

The Broker API serves archives in vendor-provided formats:
- **zip** — the majority of candidates (Gradle, Maven, Kotlin, Ant, etc.)
- **tar.gz** — Java distributions and some others (Flink, etc.)

The format is detected from the `Content-Type` response header or file extension.

### 5.4 Leading directory stripping

Most archives contain a single top-level directory (e.g., `gradle-8.7/` or `jdk-21.0.7+9/`). sdkalt inspects the archive's table of contents, identifies any common leading path prefix, and strips it during extraction. The contents land directly in `$SDKMAN_DIR/candidates/<candidate>/<identifier>/`.

Some archives (notably certain JMC distributions) are flat with no top-level directory. For these, no stripping is applied. The flat-vs-folder distinction is detected from the archive TOC, not from the hook template.

### 5.5 macOS Java layout

Java distributions for macOS are packaged as `.tar.gz` archives following the macOS app bundle convention:

```
jdk-21.0.7.jdk/
└── Contents/
    └── Home/
        ├── bin/
        ├── lib/
        └── ...
```

When the `osx-java-tarball` template is matched, sdkalt uses `Contents/Home/` as the extraction root, discarding everything above it. The result is that `$SDKMAN_DIR/candidates/java/<identifier>/` contains the JDK root directly.

This layout is stable and tied to the macOS app bundle convention; it is not expected to change.

### 5.6 JMC (Java Mission Control)

JMC is the only candidate whose hook performs work beyond archive format conversion. The `unix-jmc-tarball` hook embeds the vendor-specific path to the JMC executable (e.g., `Azul Mission Control/zmc` for zulu, `JDK Mission Control/jmc` for adpt). After extraction, a `bin/jmc` symlink must be created pointing to this path, otherwise `$SDKMAN_DIR/candidates/jmc/<identifier>/bin/jmc` would not exist and the tool would not be runnable via `PATH`.

sdkalt reads the `executable_binary` value directly from the hook script before discarding it, then creates the symlink at extraction time. No hardcoded lookup table or archive inspection is needed.

### 5.7 No download cache

Archives are written to a temporary file and deleted immediately after successful extraction. There is no persistent download cache in v1.

### 5.8 Checksum verification

The SDKMAN Broker API returns checksums for downloaded archives in HTTP response headers. The official SDKMAN CLI extracts these headers and verifies the archive before extraction. sdkalt does the same.

The response headers carrying checksums are:
- `x-sdkman-checksum-sha256` — SHA-256 hash of the archive (preferred)
- `x-sdkman-checksum-md5` — MD5 hash of the archive (fallback)

sdkalt verifies the downloaded archive against the strongest available checksum before proceeding to hook fingerprinting and extraction. If verification fails, the archive is deleted and the install is aborted.

If neither header is present, sdkalt proceeds without verification and logs a warning. This matches the official client's behaviour when checksums are unavailable.

## 6. Shell Environment Setup

### 6.1 Why a sourced init file is required

`use` and `default` must modify environment variables in the current shell (e.g., `JAVA_HOME`, `PATH`). A subprocess cannot propagate these changes to its parent shell. sdkalt uses the same eval-based approach as SDKMAN: the `sdkalt` binary emits shell commands to stdout, and a thin wrapper shell function evals them.

### 6.2 Init script

`sdkalt-init.sh` must be sourced into the current shell, typically from `.bashrc`, `.zshrc`, or equivalent:

```sh
source /path/to/sdkalt-init.sh
```

This defines the `sdkalt` shell function.

### 6.3 The `sdkalt` shell function

For `use` and `default`, the shell function:
1. Invokes the `sdkalt` binary with the given arguments.
2. Evals any shell commands printed to stdout.
3. Passes through stderr output unchanged (progress and error messages).
4. Returns the binary's exit code.

For all other commands (`list`, `install`, `uninstall`, `current`), the shell function invokes the binary directly — no eval needed.

### 6.4 Environment variable conventions

sdkalt follows SDKMAN's conventions:
- `<CANDIDATE>_HOME` is set to `$SDKMAN_DIR/candidates/<candidate>/<identifier>/`
  e.g., `JAVA_HOME=$SDKMAN_DIR/candidates/java/21.0.7-tem`
- `$SDKMAN_DIR/candidates/<candidate>/<identifier>/bin` is prepended to `PATH` when a `bin/` directory is present.

These conventions ensure that environments activated by sdkalt are identical to those activated by `sdk use`.

### 6.5 SDKMAN_DIR

sdkalt reads `$SDKMAN_DIR` from the environment. If not set, it falls back to `$HOME/.sdkman`. This is consistent with SDKMAN's own behavior.

## 7. SDKMAN Compatibility

sdkalt is designed to be fully compatible with the official SDKMAN CLI:

- **Shared candidates directory**: sdkalt reads from and writes to `$SDKMAN_DIR/candidates/`, the same directory used by SDKMAN.
- **Interchangeable installations**: A version installed by sdkalt can be activated with `sdk use`, and vice versa.
- **Same identifier format**: sdkalt uses SDKMAN version identifier strings (e.g., `21.0.7-tem`) for directory names and `current` symlinks.
- **Hook scripts**: sdkalt does not invoke SDKMAN post-install hook scripts. A full survey of all hooks across all candidates and platforms confirms that hooks perform only archive format conversion (tar.gz → zip repack) plus, in the case of `jmc`, creation of a `bin/jmc` symlink. sdkalt handles both natively. See [Section 5](#5-archive-extraction).

sdkalt does not modify SDKMAN's configuration files, shell init scripts, or bash functions.

## 8. Implementation Notes

### 8.1 In-memory API deduplication

Within a single sdkalt invocation, the same API URL is never fetched more than once. Results are cached in memory in a lazy HashMap keyed by URL. This is purely an in-process optimisation — nothing is persisted to disk. The cache is discarded when the process exits.

### 8.2 Official SDKMAN metadata caching behaviour

The official SDKMAN CLI caches the candidate names list locally at `$SDKMAN_DIR/var/candidates` as a flat CSV (e.g. `java,gradle,kotlin,...`). This is refreshed by `sdk update`. No equivalent per-candidate version cache file has been observed, which suggests version lists are fetched live from the API on each `sdk list` or `sdk install` invocation — though this has not been verified directly from the bash source.

The candidate list is likely cached for two reasons:
- **Shell completion** — tab-completing `sdk install <TAB>` requires an instant response; hitting the API on every keystroke would be unusable.
- **Offline resilience** — having the candidate list available locally means the CLI can degrade gracefully when the network is unavailable, even if version data is not cached.

Candidate names change rarely; version lists change frequently (especially for Java). Caching candidates but not versions is a reasonable trade-off.

sdkalt does not replicate the candidate name cache in v1. Offline support, including candidate and version caching, is deferred to post-v1.

### 8.3 Version list API response formats

The Broker API returns version lists in two different formats depending on the candidate:

- **Pipe-delimited table** — used by Java and other candidates with vendor/distribution fields. Contains columns for Vendor, Use, Version, Dist, Status, and Identifier separated by `|`.
- **Plain whitespace layout** — used by most other candidates (Gradle, Maven, Kotlin, etc.). Versions are space-separated tokens arranged in columns between `====` separator lines. The versions appear between the second and third separator lines; the first block is a title header and the last block is a legend.

Both formats must be handled when fetching version lists for `list` and `install`.

### 8.4 Go client macOS incompatibility

Research into existing alternative SDKMAN clients found that the Go client (`palindrom615/sdkman`) skips post-install hooks entirely and extracts archives using a generic library. On Linux this happens to work correctly because the raw tar.gz layout matches what SDKMAN expects. On macOS, Java distributions contain a `Contents/Home/` wrapper above the JDK root. Without the hook's repackaging step (or sdkalt's equivalent native handling), `JAVA_HOME` points to the wrong directory and the installation is silently broken. This was a concrete motivation for sdkalt's hook fingerprinting approach.

## 9. Out of Scope / Future Work

The following are explicitly deferred to post-v1:

- **`upgrade` command**: Install the newest version matching a range expression if a newer one is available than what is currently installed. e.g., `sdkalt upgrade java 21 tem`.
- **Offline support**: Cache the last-known candidate and version lists locally for use when the network is unavailable.
- **`selfupdate`**: Updating the sdkalt binary itself.
- **Shell completion**: Tab completion for candidates, versions, and vendors.
- **Non-Java vendor parsing**: Vendor filtering for candidates other than Java that may encode distribution information in their identifiers.
