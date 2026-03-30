# Developer Guide

This document covers how to build, test, and extend `sdkvers`.

## Prerequisites

- [Rust](https://rustup.rs) (stable toolchain)
- [just](https://github.com/casey/just) — task runner used for all build commands
- [cross](https://github.com/cross-rs/cross) — required only for Linux cross-compilation targets
- Docker or OrbStack — required by `cross` for Linux builds

## Project structure

```
Cargo.toml          — crate manifest (lib + binary)
src/
  lib.rs            — all parsing, version comparison, and resolution logic
  main.rs           — CLI entry point (sdkvers-resolve binary)
sdkvers-init.sh     — shell init script; defines the sdkvers() shell function
sdkvers-resolve     — POSIX sh launcher; selects and runs the platform binary
dist/               — assembled distribution tree (output of just dist)
justfile            — build recipes
```

The crate builds one library (`sdkvers`) and one binary (`sdkvers-resolve`). All logic lives in `lib.rs`; `main.rs` is a thin CLI wrapper.

## Running tests

Unit tests cover all parsing, version comparison, range membership, and resolution logic:

```sh
cargo test
```

Integration tests require a SDKMAN installation and are skipped by default:

```sh
just test-integration
```

The `self-test` subcommand is a live smoke test against an actual SDKMAN installation:

```sh
cargo run -- self-test
```

## Building for the local platform

```sh
cargo build --release
```

The binary is at `target/release/sdkvers-resolve`.

To test the full shell flow locally, point `SDKVERS_HOME` at the project directory (which already contains `sdkvers-init.sh` and `sdkvers-resolve`):

```sh
export SDKVERS_HOME="$(pwd)"
. sdkvers-init.sh
sdkvers
```

## Building a release distribution

`just dist` assembles a complete distribution tree in `dist/` containing the init script, the launcher, and binaries for all supported targets:

```sh
just dist
```

This requires the macOS cross-compilation targets and `cross` for the Linux targets (see below).

To build only for the current macOS host architecture:

```sh
just dist-macos-arm64   # Apple Silicon
just dist-macos-x86_64  # Intel
```

The resulting `dist/` directory is self-contained and can be copied anywhere.

## Cross-compilation for Linux

Linux targets are built using `cross`, which runs the compiler inside a Docker container.

Install the toolchain and `cross`:

```sh
just install-toolchain
```

On Apple Silicon, `cross` needs to run the container under the `linux/amd64` platform. The justfile handles this automatically by setting `CROSS_CONTAINER_OPTS` when running on `arm64` Darwin.

Build all Linux targets:

```sh
just dist-linux
```

Individual Linux targets:

```sh
just dist-linux-aarch64
just dist-linux-x86_64
just dist-linux-arm
just dist-linux-armv7
```

All Linux binaries are statically linked against musl libc and have no runtime dependencies.

## Architecture

### Why a shell init file

`sdk use` works by modifying environment variables in the current shell. A subprocess cannot propagate those changes back to its parent. The `sdkvers` shell function (defined in `sdkvers-init.sh`) works around this by running `sdkvers-resolve resolve-project` and `eval`-ing its output, which causes the `sdk use` commands to execute directly in the current shell.

### Why a launcher script

`sdkvers-resolve` (the shell launcher) selects the correct platform binary at runtime based on `uname -s` and `uname -m`. This keeps the shell init file simple and platform-agnostic.

### sdkvers-resolve subcommands

The binary exposes several subcommands that are useful during development and debugging:

| Subcommand | Description |
|------------|-------------|
| `resolve-project [dir]` | Full resolution from `.sdkvers`; emits `sdk use` lines |
| `resolve-file <path>` | Resolve a specific `.sdkvers` file |
| `resolve-line <line>` | Resolve a single config line against installed versions |
| `self-test` | Run the built-in test suite |
| `parse-version <str>` | Parse and display a version string |
| `parse-expr <str>` | Parse and display a version expression |
| `parse-line <str>` | Parse and display a `.sdkvers` line |
| `parse-file <path>` | Parse and display a `.sdkvers` file |
| `parse-sdkfile <candidate> <path>` | Parse an SDK listing from a file |
| `parse-sdklist <candidate>` | Run `sdk list <candidate>` and display parsed output |

Regular users only ever invoke `sdkvers` (the shell function). These subcommands exist for development and troubleshooting.

## Releasing

1. Update the version in `Cargo.toml`
2. Run `cargo build` to update `Cargo.lock`
3. Commit: `git commit -am "Release vX.Y.Z"`
4. Tag: `git tag vX.Y.Z`
5. Push: `git push && git push --tags`

The release workflow will validate that the tag matches `Cargo.toml`, build all platform binaries, run tests, and publish a GitHub release with the binaries attached.
