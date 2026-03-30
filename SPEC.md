# sdkvers Specification

This document is the authoritative specification for `sdkvers` behavior. It covers the `.sdkvers` file format, version expression syntax, version comparison semantics, vendor filtering, resolution algorithm, shell interface, and error handling.

A separate README covers installation and day-to-day usage. This document is for contributors and users who need precise, unambiguous definitions of how `sdkvers` works.

## Table of Contents

1. [Purpose](#1-purpose)
2. [Scope and Non-goals](#2-scope-and-non-goals)
3. [The `.sdkvers` File](#3-the-sdkvers-file)
4. [Version Expressions](#4-version-expressions)
5. [Version Comparison](#5-version-comparison)
6. [Range Membership](#6-range-membership)
7. [Vendor Filtering](#7-vendor-filtering)
8. [SDKMAN Integration](#8-sdkman-integration)
9. [Resolution Algorithm](#9-resolution-algorithm)
10. [Selection Policy](#10-selection-policy)
11. [Shell Interface and Packaging](#11-shell-interface-and-packaging)
12. [Error Handling](#12-error-handling)

## 1. Purpose

`sdkvers` reads a `.sdkvers` file in your project directory and activates the declared tool versions in your current shell by running `sdk use` for each one.

It is a developer convenience layer, not a lockfile and not a build constraint system. The build remains responsible for strict compatibility validation. `sdkvers` only automates the manual process of running `sdk use java 21.0.2-tem` when you enter a project directory.

## 2. Scope and Non-goals

### In scope for v1

- Parsing `.sdkvers` files
- Walking upward from the current directory to find `.sdkvers`
- Resolving version requirements against locally installed SDKMAN versions
- Optional vendor filtering for candidates that expose a distribution field (e.g. Java)
- Activating matched versions with `sdk use`
- Reporting all candidates that could not be resolved or activated

### Explicitly out of scope for v1

- Installing missing versions automatically
- Remote candidate discovery
- Rollback on partial failure
- Windows support
- Subcommands (`sdkvers use`, `sdkvers explain`, etc.)

## 3. The `.sdkvers` File

### Location and discovery

`sdkvers` searches upward from the current working directory for a file named `.sdkvers`. It checks the current directory first, then each parent, stopping at the filesystem root. The first file found is used. If no file is found, `sdkvers` exits with an error.

### File format

Each non-empty, non-comment line declares one candidate requirement.

```
<candidate> = <version-expr> [vendor]
```

- Leading and trailing whitespace on each line is ignored.
- Blank lines are ignored.
- Lines starting with `#` are comments and are ignored.
- Inline comments are not supported in v1.
- Candidate names are case-sensitive and must match SDKMAN candidate names exactly (e.g. `java`, `maven`, `gradle`).
- `=` is required and must be present.
- The vendor token, if present, follows the version expression and is separated by whitespace.
- Range tokens must contain no internal spaces in v1 (see [Version Expressions](#4-version-expressions)).

### Examples

```
# Activate Java 21 (any patch), Temurin distribution
java = 21 tem

# Maven anywhere in the 3.9.x line
maven = [3.9,4)

# Gradle exactly 8.7.0
gradle = 8.7.0

```

Lines are processed top-to-bottom. All candidates are attempted even when earlier ones fail.

## 4. Version Expressions

There are two kinds of version expression: **bare versions** and **explicit ranges**.

### 4.1 Bare versions

Bare versions are shorthand. They are expanded according to how many purely numeric segments they contain.

| Form | Segments | Interpretation | Example expansion |
|------|----------|----------------|-------------------|
| `21` | one numeric | major-line range | `[21,22)` |
| `21.1` | two numeric | minor-line range | `[21.1,21.2)` |
| `21.0.10` | three or more numeric | exact match | `[21.0.10]` |
| `4.10.1.3` | four numeric | exact match | `[4.10.1.3]` |

Any bare version containing a letter or underscore is always treated as an exact match, regardless of segment count.

| Bare form | Interpretation |
|-----------|----------------|
| `26.ea.35` | exact match |
| `2.16.0.Final` | exact match |
| `21.0.10.fx` | exact match |
| `5.23.0.0_2` | exact match |
| `v0.1.0` | exact match |
| `swan-lake-p3` | exact match |

This rule exists because mixed alphanumeric versions are too irregular to safely interpret as prefix ranges.

### 4.2 Explicit ranges

Explicit ranges use Maven-style interval notation. The brackets and parentheses determine whether each bound is inclusive or exclusive.

| Syntax | Meaning |
|--------|---------|
| `[a,b]` | `a <= v <= b` |
| `[a,b)` | `a <= v < b` |
| `(a,b]` | `a < v <= b` |
| `(a,b)` | `a < v < b` |
| `[a,)` | `v >= a` (no upper bound) |
| `(,b]` | `v <= b` (no lower bound) |
| `(,b)` | `v < b` (no lower bound) |
| `[a]` | exact match to `a` |

Commas within range brackets must not be surrounded by spaces in v1. `[21,22)` is accepted; `[21, 22)` is rejected.

Version comparison follows the rules in [Section 5](#5-version-comparison). Range membership follows the rules in [Section 6](#6-range-membership), which includes prerelease filtering behavior.

## 5. Version Comparison

This section defines how two version strings are ordered. These rules are used when filtering candidates against range bounds and when choosing the best match from multiple qualifying candidates.

`sdkvers` uses Maven-style range *syntax*, but does not use Maven's comparison algorithm. Maven's algorithm produces some results that are undesirable for tool selection, for example:

- `26.ea.35 > 26` (early-access builds outranking stable)
- `4.0.0-preview1 > 4.0.0` (preview builds outranking stable)
- `21.0.10.fx > 21.0.10` (feature-variant builds outranking stable)

`sdkvers` defines its own comparison rules that give stable releases priority over pre-release and variant builds, while preserving sensible ordering within each category.

### 5.1 Tokenization

Version strings are split into a sequence of components at these boundaries:

- `.` separator
- `-` separator
- `_` separator
- Transitions between digit characters and letter characters (no separator character consumed)

Each component is classified as **numeric** (all digits) or **textual** (contains letters).

| Version string | Tokens |
|----------------|--------|
| `21.0.10` | `21`, `0`, `10` |
| `26.ea.35` | `26`, `ea`, `35` |
| `9.4.0-rc-1` | `9`, `4`, `0`, `rc`, `1` |
| `5.23.0.0_2` | `5`, `23`, `0`, `0`, `2` |
| `25.0.2.r25` | `25`, `0`, `2`, `r`, `25` |
| `2024.1.0-M2` | `2024`, `1`, `0`, `M`, `2` |
| `v0.1.0` | `v`, `0`, `1`, `0` |

Comparison works on these normalized tokens, not on the raw string. If normalized comparison produces equality, the raw strings are compared lexically as a final tiebreaker to ensure deterministic ordering.

### 5.2 Qualifier classes

Textual tokens are classified into one of five groups. The group determines how the token sorts relative to a position where no token is present (i.e., the plain release endpoint).

#### Pre-release qualifiers — sort *before* the plain release

These indicate unstable or early builds and are ordered among themselves as shown:

```
alpha = a
beta = b
milestone = m
rc = cr
ea
preview
snapshot
```

Full ordering: `alpha < beta < milestone < rc < ea < preview < snapshot < (plain release)`

When a numeric suffix follows a qualifier, it compares numerically within the same family:

```
rc1 < rc2
M1 < M2
ea.13 < ea.35
```

#### Release aliases — compare *equal* to the plain release

```
final
ga
release
```

Examples:
- `2.16.0.Final == 2.16.0`
- `1.3.1.final == 1.3.1`

#### Variant qualifiers — sort *after* the plain release

These represent feature variants or packaging differences, not instability:

```
fx
crac
```

Examples:
- `21.0.10 < 21.0.10.fx`
- `26 < 26.crac`

#### Post-release / build qualifiers — sort *after* the plain release

These indicate minor post-release revisions:

- `r` followed by a number: `r25`
- `_` followed by a number: `_2`
- A hyphen followed only by a number: `-2`

Examples:
- `25.0.2 < 25.0.2.r25`
- `5.23.0.0_1 < 5.23.0.0_2`
- `1.0.1-1 < 1.0.1-2`

#### Unknown textual qualifiers — sort *after* the plain release by default

Any textual token not recognized above is treated conservatively as a post-release variant. This default avoids accidentally preferring an unknown qualifier over the plain stable release.

When both sides have the same unknown qualifier at the same position, they compare case-insensitively. If both are equal and are followed by numeric suffixes, those suffixes compare numerically.

### 5.3 Comparison algorithm

Comparison proceeds left-to-right over the normalized token sequences.

- Numeric tokens compare numerically.
- Textual tokens compare by qualifier class first, then by value within the class.
- When one side is exhausted and the other has only release-alias tokens remaining, they compare equal.
- When one side is exhausted and the other has pre-release tokens remaining, the exhausted side is **greater** (the plain release beats the pre-release).
- When one side is exhausted and the other has variant or post-release tokens remaining, the exhausted side is **smaller** (the variant or post-release build comes after the plain release).

### 5.4 Summary table

```
9.4.0-rc-1    <  9.4.0-rc-2  <  9.4.0
2024.1.0-M1   <  2024.1.0-M2  <  2024.1.0-RC1  <  2024.1.0
26.ea.13      <  26.ea.35  <  26
4.0.0-preview1  <  4.0.0-preview2  <  4.0.0
2.16.0        <  2.16.0.Final     (string tiebreaker; unit comparison is equal)
21.0.10       <  21.0.10.fx
25.0.2        <  25.0.2.r25
5.23.0.0_1    <  5.23.0.0_2
1.0.1-1       <  1.0.1-2
```

The `2.16.0.Final` case deserves a note: the unit comparison treats `Final` as a release alias and skips it, producing equality. Because equality falls through to a raw string tiebreaker, `compare_versions` returns `2.16.0 < 2.16.0.Final`. However, **exact match testing** uses the unit comparison only (no tiebreaker), so `[2.16.0]` and `[2.16.0.Final]` are treated as the same selector. See [Section 6.3](#63-exact-match-semantics).

## 6. Range Membership

A candidate version is considered inside a range when it passes **prerelease eligibility filtering** and then satisfies the **bound conditions**.

### 6.1 Prerelease eligibility

By default, if neither bound of a range explicitly contains a pre-release qualifier, pre-release candidates are excluded from consideration entirely. This prevents `[21,22)` from unexpectedly matching `21.0.1-ea.3`.

Default behavior (no pre-release qualifier in bounds):

```
[26,)   does not match  26.ea.35
[26,)   does not match  27.ea.14
[21,22) does not match  21.0.1-ea.3
```

When a bound explicitly contains a pre-release qualifier, pre-release candidates are allowed — but only for that same pre-release *base line*. A pre-release base line is the numeric release line that immediately precedes the pre-release qualifier in the version string.

- `26.ea.35` has base line `26`
- `9.4.0-rc-1` has base line `9.4.0`
- `2024.1.0-M2` has base line `2024.1.0`

Stable candidates that satisfy the ordinary bounds remain eligible regardless.

```
[26.ea,)  matches         26.ea.13
[26.ea,)  matches         26.ea.35
[26.ea,)  does not match  27.ea.14   (different base line)
[26.ea,)  matches         27.0       (stable, satisfies bound)
```

For an upper bound containing a pre-release qualifier, the same rule applies at the upper end:

```
[26.1,27.ea]  matches         27.ea
[26.1,27.ea]  does not match  27.ea.14  (27.ea.14 > 27.ea, exceeds upper bound)
[26.1,27.ea]  does not match  26.ea.35  (base 26 pre-release, no bound opts in at that base)
```

### 6.2 Variant qualifiers in ranges

Variant qualifiers (`fx`, `crac`) are not pre-release. They are not excluded by the prerelease filter. A range that covers the base version will also cover its variants unless an explicit upper bound excludes them.

```
[21.0.10,21.0.11)  matches  21.0.10
[21.0.10,21.0.11)  matches  21.0.10.fx
[21.0.10,21.0.11)  matches  21.0.10.crac
```

This is intentional. Variant qualifiers represent a packaging or feature difference, not a stability difference, and they sort after the plain release. If you want only the plain release, use an exact match: `[21.0.10]`.

### 6.3 Exact match semantics

An exact match (`[a]` or a bare version with three or more numeric segments) matches any candidate that compares equal to `a` under the normalized comparison rules. Because release aliases compare equal to the plain release:

```
[2.16.0]  matches  2.16.0
[2.16.0]  matches  2.16.0.Final
```

When multiple candidates compare equal, the implementation prefers the one whose raw string exactly matches the user input. This gives intuitive behavior without changing the comparison rules.

### 6.4 Range membership examples

```
[26.ea.1,26)        matches  26.ea.13, 26.ea.35  — not  26
[9.4.0-rc-1,9.4.0)  matches  9.4.0-rc-1, 9.4.0-rc-2  — not  9.4.0
[21.0.10,21.0.11)   matches  21.0.10, 21.0.10.fx, 21.0.10.crac
[26,)               matches  stable 26+  — not  26.ea.*, 27.ea.*
[26.ea,)            matches  26.ea.*, stable 27+, stable 28+, ...  — not  27.ea.*
```

## 7. Vendor Filtering

Some SDKMAN candidates expose a separate distribution or vendor field alongside the version number. Java is the primary example: a Java identifier like `21.0.2-tem` encodes both a version (`21.0.2`) and a distribution (`tem`).

### 7.1 Syntax

An optional vendor token follows the version expression on a `.sdkvers` line:

```
java = 21 tem
java = [21,22) graalce
java = [17,) zulu
```

### 7.2 Matching rules

- Vendor filtering applies only to candidates that expose a distinct distribution field.
- Version constraints are evaluated against the version portion only.
- Vendor constraints are evaluated against the distribution portion only.
- Vendor matching is case-sensitive.
- Vendor matching is exact, not substring-based.

```
java = 21 graalce   matches      21.0.2-graalce
java = 21 graalce   does not match  21.0.2-graal         (prefix, not exact)
java = 21 graalce   does not match  21.0.2-graalce-openj9 (suffix, not exact)
java = 21 graalce   does not match  21.0.2-tem
```

### 7.3 Candidates without vendor fields

If a candidate does not expose a distribution field in a form the implementation can parse, specifying a vendor token for that candidate produces an error. The implementation does not silently ignore unsupported vendor requests.

### 7.4 Java specifically

For Java candidates, the identifier format is `<version>-<dist>`. The implementation splits on the last hyphen component that matches a known or reasonable distribution label.

- The `Version` field is used for version comparison.
- The `Dist` field is used for vendor comparison.
- The `Identifier` (e.g. `21.0.2-tem`) is passed directly to `sdk use`.

## 8. SDKMAN Integration

### 8.1 Discovering installed versions

`sdkvers` discovers locally installed versions from the SDKMAN candidates directory. It does not invoke `sdk list` for local resolution. The directory inspected is:

```
$SDKMAN_DIR/candidates/<candidate>/
```

If `SDKMAN_DIR` is not set, the implementation falls back to `$HOME/.sdkman`.

Each subdirectory under `candidates/<candidate>/` that is not named `current` represents an installed version. The `current` symlink, if present, indicates the currently active version but does not affect candidate discovery or matching.

### 8.2 Activating a version

Once a version is resolved, the implementation emits:

```
sdk use <candidate> <identifier>
```

Because `sdk use` must modify the current shell environment, it cannot be run inside a subprocess. The init script sources these commands into the current shell using `eval`. See [Section 11](#11-shell-interface-and-packaging).

### 8.3 What `sdkvers` does not do

- It does not install missing versions.
- It does not invoke `sdk install`.
- It does not modify SDKMAN configuration.
- It does not call `sdk list` for local resolution (that subcommand is available for diagnostic purposes only).

## 9. Resolution Algorithm

For each line in `.sdkvers`, in order:

1. Parse the candidate name.
2. Parse the version expression and expand bare versions to their canonical range form.
3. Parse the optional vendor token.
4. Read the locally installed versions from `$SDKMAN_DIR/candidates/<candidate>/`.
5. Parse each installed identifier into version and distribution fields where applicable.
6. Apply prerelease eligibility filtering (see [Section 6.1](#61-prerelease-eligibility)).
7. Retain only candidates whose version satisfies the version constraint.
8. Retain only candidates whose distribution satisfies the vendor constraint (if a vendor was specified).
9. From the remaining candidates, select the one with the highest version (see [Section 10](#10-selection-policy)).
10. Emit `sdk use <candidate> <identifier>` for the selected candidate.

If any step fails for a line, record the error with the candidate name and line number and continue to the next line. After all lines are processed, if any errors were recorded, report all of them and return a non-zero exit status.

There is no rollback. Candidates that were successfully activated before a failure remain active.

## 10. Selection Policy

When multiple installed versions satisfy all constraints, `sdkvers` selects the **highest matching installed version**, as determined by the comparison rules in [Section 5](#5-version-comparison).

Examples:

```
java = 21
  Installed: 17.0.9, 21.0.1-tem, 21.0.2-tem, 22.0.0-tem
  Selected:  21.0.2-tem

maven = [3.9,4)
  Installed: 3.8.8, 3.9.6, 3.9.9, 4.0.0-rc-1
  Selected:  3.9.9              (4.0.0-rc-1 excluded by upper bound; also a pre-release)

java = [21.0.10,21.0.11)
  Installed: 21.0.10-tem, 21.0.10.fx-zulu
  Selected:  21.0.10.fx-zulu   (fx variant sorts higher; no vendor filter specified)
```

The selection policy is applied after all filtering (version, vendor, prerelease). There is no alternative policy in v1.

## 11. Shell Interface and Packaging

### 11.1 Components

`sdkvers` is distributed as three components:

- **`sdkvers-init.sh`** — a shell script that must be sourced into the current shell. Defines the `sdkvers` shell function.
- **`sdkvers-resolve`** — a POSIX `sh` launcher script that selects and invokes the correct platform binary.
- **`sdkvers-resolve-<target>`** — platform-specific native binaries (one per supported target).

### 11.2 Why a sourced init file is required

`sdk use` modifies environment variables in the current shell. An external subprocess cannot make those changes visible to the parent shell. The `sdkvers` shell function bridges this gap by running `sdkvers-resolve resolve-project` and `eval`-ing its output.

### 11.3 The `sdkvers` function

Usage:

```sh
sdkvers
```

Behavior:
1. Searches upward from `$PWD` for `.sdkvers`.
2. Invokes `sdkvers-resolve resolve-project <path>`.
3. For each `sdk use ...` line emitted, evaluates it in the current shell.
4. Prints progress and error messages to stderr.
5. Returns zero if and only if all candidates were successfully activated.

Preconditions:
- `sdkvers-init.sh` must already be sourced into the current shell.
- `sdk` must already be available in the current shell (i.e. SDKMAN must already be initialized).

### 11.4 Supported targets

| Target | Description |
|--------|-------------|
| `arm64-apple-darwin` | macOS on Apple Silicon |
| `x86_64-apple-darwin` | macOS on Intel |
| `aarch64-linux-musl` | Linux on ARM64 (musl libc) |
| `x86_64-linux-musl` | Linux on x86_64 (musl libc) |
| `arm-linux-musleabihf` | Linux on ARMv6 (musl libc, hard-float) |
| `armv7-linux-musleabihf` | Linux on ARMv7 (musl libc, hard-float) |

Musl targets produce fully static binaries with no runtime dependencies.

### 11.5 Shell compatibility

`sdkvers-init.sh` is written in POSIX `sh` and is compatible with macOS `/bin/sh` (which is `dash`-based) and Linux `/bin/sh`. BusyBox `sh` compatibility is a best-effort target.

## 12. Error Handling

### 12.1 Error message format

Error messages are written to stderr and include the program name, and where applicable the candidate name and line number.

```
sdkvers: no .sdkvers file found
sdkvers: invalid line 3: expected "<candidate> = <version-expr> [vendor]"
sdkvers: invalid version expression on line 5: [21,22
sdkvers: candidate "java" has no installed version matching [21,22) with vendor "graalce"
sdkvers: candidate "maven" has no installed version matching [3.9,4)
sdkvers: sdk command is not available in this shell
sdkvers: failed to activate java 21.0.2-graalce
```

### 12.2 Behavior on failure

- Parse errors and resolution failures are both reported.
- All candidates are attempted before reporting failures.
- A non-zero exit status is returned if any candidate fails.
- Malformed lines are never silently ignored.

### 12.3 Partial success

If some candidates are activated and others fail, the activated ones remain active. `sdkvers` does not roll back successful activations.

```sh
$ sdkvers
Using java 21.0.2-graalce
sdkvers: candidate "maven" has no installed version matching [3.9,4)
```

Exit status in this case is non-zero.

### 12.4 Success output

On full success, `sdkvers` prints one confirmation line per candidate:

```sh
$ sdkvers
Using java 21.0.2-graalce
Using maven 3.9.14
Using gradle 8.7.0
```

Exit status is zero.
