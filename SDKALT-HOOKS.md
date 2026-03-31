# sdkalt Hook Fingerprinting Implementation

This document specifies exactly how sdkalt normalizes and fingerprints SDKMAN post-install hook scripts to determine the correct native extraction behaviour. It is intended as a direct implementation reference.

## Background

The SDKMAN Broker API serves a hook script for every candidate/version/platform combination at:

```
GET https://api.sdkman.io/2/hooks/post/<candidate>/<version>/<platform>
```

These scripts are bash functions that repack vendor archives into zip format for the official SDKMAN CLI. sdkalt fetches them but does not execute them. Instead, it normalizes and fingerprints the script to identify which of 6 known templates it matches, then executes a native Rust extraction branch instead.

A survey of all 15,000+ hooks across all candidates and platforms confirmed that every hook matches one of these 6 templates exactly, with no deviations after normalization.

## Normalization

Normalization is applied to the raw hook text before hashing. The following substitutions are applied in order:

1. Replace all occurrences of the version identifier (e.g. `21.0.7-tem`) with `IDENTIFIER`
2. Replace all occurrences of the title-cased candidate name (e.g. `Gradle`) with `CANDIDATE`
3. Replace all occurrences of the lowercase candidate name (e.g. `gradle`) with `CANDIDATE`
4. Replace the platform description string with `PLATFORM`:
   - `Linux ARM 32bit Hard Float` → `PLATFORM`
   - `Linux ARM 64bit` → `PLATFORM`
   - `Linux 64bit` → `PLATFORM`
   - `macOS ARM 64bit` → `PLATFORM`
   - `macOS 64bit` → `PLATFORM`
5. For JMC hooks only: replace `JMC <vendor>` (where vendor is the suffix after the last `-` in the identifier, e.g. `zulu` from `9.1.1-zulu`) with `JMC VENDOR`
6. Replace the `executable_binary` value with a placeholder: any line matching `local executable_binary="..."` → `local executable_binary="EXECUTABLE"`
7. Replace the `containing_folder` assignment with a placeholder: any line matching `local containing_folder=...` → `local containing_folder="FOLDER"`

Steps 5–7 are only relevant for JMC hooks but are safe to apply universally as they will match nothing in non-JMC scripts.

## Known Fingerprints

The following are the normalized MD5 hashes of the 6 known templates. These were computed by the survey and are the ground truth for fingerprint matching.

| Hash | Template | Extraction behaviour |
|---|---|---|
| `fcb2384eb3d368d473c1e99d1561ff9c` | `default-zip` | Extract zip, strip leading dir |
| `ed8b941f3f502247fa2fb59477337c4c` | `default-tarball` | Extract tar.gz, strip leading dir |
| `87d5e14e777c1a77a805d4da7d9fe36e` | `linux-java-tarball` | Extract tar.gz, strip leading dir (same as `default-tarball`) |
| `71cfc4bb7c090de0b8b5e2674f65ba62` | `osx-java-tarball` | Extract tar.gz, use `Contents/Home/` as root |
| `a091ec7b7b0b2f9a3c27e98f9e8728af` | `unix-jmc-tarball` (folder) | Extract tar.gz, strip leading dir, create `bin/jmc` symlink |
| `0c11a5f98122448338c99a3cb9cc8789` | `unix-jmc-tarball` (flat) | Extract tar.gz, no leading dir, create `bin/jmc` symlink |

## Extraction Branches

### `default-zip`

1. Inspect archive TOC for a common leading path prefix.
2. Extract zip, stripping the leading prefix if present.
3. Contents land at `$SDKMAN_DIR/candidates/<candidate>/<identifier>/`.

### `default-tarball` / `linux-java-tarball`

These two templates are structurally identical and use the same extraction branch.

1. Inspect archive TOC for a common leading path prefix.
2. Extract tar.gz, stripping the leading prefix if present.
3. Contents land at `$SDKMAN_DIR/candidates/<candidate>/<identifier>/`.

### `osx-java-tarball`

1. Inspect archive TOC for a `Contents/Home/` path segment.
2. Extract tar.gz using `Contents/Home/` as the extraction root, discarding everything above it.
3. Contents land at `$SDKMAN_DIR/candidates/java/<identifier>/` with the JDK root directly inside.

### `unix-jmc-tarball` (folder) / `unix-jmc-tarball` (flat)

Before discarding the hook script, extract the `executable_binary` value from the line:

```bash
local executable_binary="<path>"
```

For example:
- `Azul Mission Control/zmc` (zulu)
- `JDK Mission Control/jmc` (adpt, amzn)
- `Azul Mission Control.app/Contents/MacOS/zmc` (zulu on macOS)
- `JDK Mission Control.app/Contents/MacOS/jmc` (adpt on macOS)

Then:

**Folder variant** (`a091ec7b7b0b2f9a3c27e98f9e8728af`):
1. Archive has a single top-level directory — strip it.
2. Extract tar.gz with leading dir stripped.
3. Create directory `$SDKMAN_DIR/candidates/jmc/<identifier>/bin/`.
4. Create symlink `bin/jmc` → `../<executable_binary>`.

**Flat variant** (`0c11a5f98122448338c99a3cb9cc8789`):
1. Archive is flat — no leading dir to strip.
2. Extract tar.gz directly.
3. Create directory `$SDKMAN_DIR/candidates/jmc/<identifier>/bin/`.
4. Create symlink `bin/jmc` → `../<executable_binary>`.

Note: the flat-vs-folder distinction is confirmed by the archive TOC at extraction time, not assumed from the hook fingerprint alone. If the TOC does not match expectations for the matched template, the install fails.

## Failure Behaviour

- **Unrecognized fingerprint**: abort install, report the hash so it can be investigated.
- **Empty or missing hook**: treat as `default-zip` if the archive is a zip, `default-tarball` if tar.gz — some candidates return an empty hook body when no special handling is needed.
- **Extraction produces unexpected layout**: fail the install and clean up the partially extracted directory.

## Normalized Script Reference

The following are the fully normalized bodies of one representative hook per template. These are the exact strings that produce the known fingerprint hashes above.

### `default-zip` (`fcb2384eb3d368d473c1e99d1561ff9c`)

```bash
#!/bin/bash
#Post Hook: default-zip
function __sdkman_post_installation_hook {
    __sdkman_echo_debug "No PLATFORM post-install hook found for CANDIDATE IDENTIFIER."
    __sdkman_echo_debug "Moving $binary_input to $zip_output"
    mv -f "$binary_input" "$zip_output"
}
```

### `default-tarball` (`ed8b941f3f502247fa2fb59477337c4c`)

```bash
#!/bin/bash
#Post Hook: default-tarball
function __sdkman_post_installation_hook {
    __sdkman_echo_debug "No PLATFORM post-install hook found for CANDIDATE IDENTIFIER."

    __sdkman_check_commands_present || return 1

    __sdkman_validate_binary_input "$binary_input" || return 1

    local present_dir="$(pwd)"
    local work_dir="${SDKMAN_DIR}/tmp/out"

    echo ""
    __sdkman_echo_green "Repackaging CANDIDATE IDENTIFIER..."

    mkdir -p "$work_dir"
    /usr/bin/env tar zxf "$binary_input" -C "$work_dir"

    cd "$work_dir"
    /usr/bin/env zip -qyr "$zip_output" .
    cd "$present_dir"

    echo ""
    __sdkman_echo_green "Done repackaging..."

    __sdkman_echo_debug "Cleaning up residual files..."
    rm -f "$binary_input"
    rm -rf "$work_dir"
}

function __sdkman_validate_binary_input {
    if ! tar tzf "$1" &> /dev/null; then
        __sdkman_echo_red "Download has failed, aborting!"
        echo ""
        __sdkman_echo_red "Can not install CANDIDATE IDENTIFIER at this time..."
        return 1
    fi
}

function __sdkman_check_commands_present {
    if ! which tar &> /dev/null || ! which gzip &> /dev/null; then
        __sdkman_echo_red 'tar and/or gzip not available on this system.'
        echo ""
        __sdkman_echo_no_colour "Please install tar/gzip on your system using your favourite package manager."
        return 1
    fi
}
```

### `linux-java-tarball` (`87d5e14e777c1a77a805d4da7d9fe36e`)

```bash
#!/bin/bash
#Post Hook: linux-CANDIDATE-tarball
function __sdkman_post_installation_hook {
    __sdkman_echo_debug "A PLATFORM post-install hook was found for CANDIDATE IDENTIFIER."

    __sdkman_validate_binary_input "$binary_input" || return 1

    local present_dir="$(pwd)"
    local work_dir="${SDKMAN_DIR}/tmp/out"

    echo ""
    echo "Repackaging CANDIDATE IDENTIFIER..."

    mkdir -p "$work_dir"
    /usr/bin/env tar zxf "$binary_input" -C "$work_dir"

    cd "$work_dir"
    /usr/bin/env zip -qyr "$zip_output" .
    cd "$present_dir"

    echo ""
    echo "Done repackaging..."

    __sdkman_echo_debug "Cleaning up residual files..."
    rm -f "$binary_input"
    rm -rf "$work_dir"
}

function __sdkman_validate_binary_input {
    if ! tar tzf "$1" &> /dev/null; then
        echo "Download has failed, aborting!"
        echo ""
        echo "Can not install CANDIDATE IDENTIFIER at this time..."
        return 1
    fi
}
```

### `osx-java-tarball` (`71cfc4bb7c090de0b8b5e2674f65ba62`)

```bash
#!/bin/bash
#Post Hook: osx-CANDIDATE-tarball
function __sdkman_post_installation_hook {
    __sdkman_echo_debug "A PLATFORM post-install hook was found for CANDIDATE IDENTIFIER-openjdk."

     __sdkman_validate_binary_input "$binary_input" || return 1

    local present_dir="$(pwd)"
    local work_dir="${SDKMAN_DIR}/tmp/out"
    local work_jdk_dir="${SDKMAN_DIR}/tmp/CANDIDATE-IDENTIFIER"

    echo ""
    __sdkman_echo_green "Repackaging CANDIDATE IDENTIFIER..."

    mkdir -p "$work_dir"
    /usr/bin/env tar zxf "$binary_input" -C "$work_dir"

    cd "$work_dir"/*/Contents
    mv -f Home "$work_jdk_dir"
    cd "${SDKMAN_DIR}"/tmp
    /usr/bin/env zip -qyr "$zip_output" "CANDIDATE-IDENTIFIER"
    cd "$present_dir"

    echo ""
    __sdkman_echo_green "Done repackaging..."

    __sdkman_echo_green "Cleaning up residual files..."
    rm -f "$binary_input"
    rm -rf "$work_dir"
    rm -rf "$work_jdk_dir"
}

function __sdkman_validate_binary_input {
    if ! tar tzf "$1" &> /dev/null; then
        echo "Download has failed, aborting!"
        echo ""
        echo "Can not install CANDIDATE IDENTIFIER at this time..."
        return 1
    fi
}
```

### `unix-jmc-tarball` folder variant (`a091ec7b7b0b2f9a3c27e98f9e8728af`)

```bash
#!/bin/bash
#Post Hook: unix-CANDIDATE-tarball
function __sdkman_post_installation_hook {
    __sdkman_echo_debug "A unix post-install hook was found for JMC VENDOR IDENTIFIER."

    __sdkman_validate_binary_input "$binary_input" || return 1

    local present_dir="$(pwd)"
    local work_dir="${SDKMAN_DIR}/tmp/out"
    local executable_binary="EXECUTABLE"

    echo ""
    __sdkman_echo_green "Repackaging JMC VENDOR IDENTIFIER..."

    mkdir -p "$work_dir"

    # deal with zulu folder structure
    /usr/bin/env tar zxf "$binary_input" -C "$work_dir"
    cd "$work_dir"/*

    mkdir bin
    cd bin
    ln -s ../"${executable_binary}" CANDIDATE

    cd "$work_dir"
    /usr/bin/env zip -qyr "$zip_output" .
    cd "$present_dir"

    echo ""
    __sdkman_echo_green "Done repackaging..."

    __sdkman_echo_debug "Cleaning up residual files..."
    rm "$binary_input"
    rm -rf "$work_dir"
}

function __sdkman_validate_binary_input {
    if ! tar tzf "$1" &> /dev/null; then
        __sdkman_echo_red "Download has failed, aborting!"
        echo ""
        __sdkman_echo_red "Can not install java IDENTIFIER at this time..."
        return 1
    fi
}
```

### `unix-jmc-tarball` flat variant (`0c11a5f98122448338c99a3cb9cc8789`)

```bash
#!/bin/bash
#Post Hook: unix-CANDIDATE-tarball
function __sdkman_post_installation_hook {
    __sdkman_echo_debug "A unix post-install hook was found for JMC VENDOR IDENTIFIER."

    __sdkman_validate_binary_input "$binary_input" || return 1

    local present_dir="$(pwd)"
    local work_dir="${SDKMAN_DIR}/tmp/out"
    local executable_binary="EXECUTABLE"

    echo ""
    __sdkman_echo_green "Repackaging JMC VENDOR IDENTIFIER..."

    mkdir -p "$work_dir"

    # deal with CANDIDATE flat structure
    local containing_folder="FOLDER"
    mkdir -p "$containing_folder"
    /usr/bin/env tar zxf "$binary_input" -C "$containing_folder"
    cd "$containing_folder"

    mkdir bin
    cd bin
    ln -s ../"${executable_binary}" CANDIDATE

    cd "$work_dir"
    /usr/bin/env zip -qyr "$zip_output" .
    cd "$present_dir"

    echo ""
    __sdkman_echo_green "Done repackaging..."

    __sdkman_echo_debug "Cleaning up residual files..."
    rm "$binary_input"
    rm -rf "$work_dir"
}

function __sdkman_validate_binary_input {
    if ! tar tzf "$1" &> /dev/null; then
        __sdkman_echo_red "Download has failed, aborting!"
        echo ""
        __sdkman_echo_red "Can not install java IDENTIFIER at this time..."
        return 1
    fi
}
```

## Implementation Notes

- Normalization must be applied to the raw hook bytes before hashing. Use MD5.
- The identifier substitution (step 1) must happen before candidate substitution (steps 2–3) to avoid partial matches when the identifier contains the candidate name as a substring.
- Platform description strings are matched literally and are case-sensitive. The exact strings listed above are the only ones observed in the survey.
- Hook scripts are fetched in parallel with the archive download where possible, since both are needed before extraction can begin.
