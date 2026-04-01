# sdkvers release notes

## v1.1.0 (unreleased)

### New features

- **`sdkvers bootstrap`** — generates a `.sdkvers` file from your currently active SDKMAN versions. Reads the `sdk use`-selected version for every installed candidate and writes a ready-to-commit `.sdkvers` file. Accepts `--directory <dir>` to write to a specific location.
- **`sdkvers selfupdate`** — updates the sdkvers installation in-place. Fetches the latest release from GitHub, downloads and extracts the archive, and atomically replaces all files using rename so the running binary's inode is preserved. Subcommands:
  - `selfupdate check` — report whether an update is available without applying it
  - `selfupdate force` — install even if already on the latest version
- **Install suggestions** — when no installed version matches a `.sdkvers` constraint, sdkvers now prints a `hint:` line suggesting the best `sdk install` command to run. For `java` entries without an explicit vendor, the suggestion prefers vendors already present in the local installation.

### Internal changes

- The shell-function output protocol was redesigned. The binary now writes two UUID-delimited, Quoted-Printable-encoded sections (eval content and stdout content) in a single pass. The shell function pipes this through `sdkvers-resolve internal extract` to route and decode each section. This gives the binary full control over what gets eval'd versus printed, enabling commands like `bootstrap` that need to both write a file and print a message.
- Development/inspection commands (`resolve-project`, `parse-version`, `self-test`, etc.) are now grouped under the `sdkvers-resolve debug` namespace, keeping the top-level CLI surface clean for end users.
- Test fixtures for 11 SDKMAN candidates (ant, gradle, groovy, java, kotlin, maven, micronaut, sbt, scala, springboot) were added as committed files, allowing the resolver tests to run against real SDK listing data without a live SDKMAN installation.
- `DEVELOPER.md` was added with build, test, cross-compilation, and release instructions.

## v1.0.0

Initial release.

### Features

- **`.sdkvers` file format** — a simple line-oriented config file declaring SDK versions per project (`java = 21`, `maven = 3.9`, `gradle = [8,9)`, etc.). Supports exact versions, open/closed range expressions, and vendor qualifiers for Java distributions.
- **Version resolution** — walks the directory tree upward to find `.sdkvers`, queries the locally installed SDKMAN versions for each candidate, and selects the best installed match. Prefers the in-use version when multiple candidates satisfy the constraint.
- **`sdkvers` shell function** — defined by sourcing `sdkvers-init.sh`. Running `sdkvers` in a project directory resolves `.sdkvers` and `eval`s the resulting `sdk use` commands in the current shell so environment variables propagate correctly.
- **Multi-platform binaries** — pre-built for macOS (arm64, x86_64) and Linux (aarch64, x86_64, arm, armv7 via musl/static linking). A POSIX sh launcher script selects the correct binary at runtime.
- **Built-in self-test** (`sdkvers-resolve debug self-test`) — smoke-tests the resolver against the live SDKMAN installation.
- **Inspection subcommands** under `sdkvers-resolve debug` for parsing and dumping versions, expressions, config lines, files, and SDK listings — useful for troubleshooting and development.
