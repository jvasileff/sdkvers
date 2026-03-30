set shell := ["bash", "-eu", "-o", "pipefail", "-c"]

dist_dir := "dist"

default:
  @just --list

# Install Rust cross-compilation toolchain and cross
install-toolchain:
  rustup target add x86_64-apple-darwin
  rustup target add aarch64-apple-darwin
  cargo install cross@0.2.5

# Run tests
test:
  cargo test

# Run integration tests (requires SDKMAN installation)
test-integration:
  cargo test -- --include-ignored

# Remove cargo build artifacts
clean-build:
  cargo clean

# Remove the dist directory
clean-dist:
  rm -rf {{dist_dir}}

# Remove all build artifacts and dist
clean:
  just clean-build
  just clean-dist

# Assemble a complete release distribution in dist/
dist:
  just clean-dist
  just _dist-scripts
  just dist-macos
  just dist-linux

# Build and copy all macOS binaries to dist/
dist-macos:
  just dist-macos-arm64
  just dist-macos-x86_64

# Build and copy all Linux binaries to dist/
dist-linux:
  just dist-linux-aarch64
  just dist-linux-x86_64
  just dist-linux-arm
  just dist-linux-armv7

# Compile all macOS binaries (without copying to dist/)
build-macos:
  just build-macos-arm64
  just build-macos-x86_64

# Compile all Linux binaries (without copying to dist/)
build-linux:
  just build-linux-aarch64
  just build-linux-x86_64
  just build-linux-arm
  just build-linux-armv7

# Compile all binaries for all platforms (without copying to dist/)
build-all:
  just build-macos
  just build-linux

build-macos-arm64:
  cargo build --release --target aarch64-apple-darwin

build-macos-x86_64:
  cargo build --release --target x86_64-apple-darwin

build-linux-aarch64:
  just _cross-build aarch64-unknown-linux-musl

build-linux-x86_64:
  just _cross-build x86_64-unknown-linux-musl

build-linux-arm:
  just _cross-build arm-unknown-linux-musleabihf

build-linux-armv7:
  just _cross-build armv7-unknown-linux-musleabihf

dist-macos-arm64:
  #!/usr/bin/env bash
  set -eu -o pipefail
  cargo build --release --target aarch64-apple-darwin
  mkdir -p {{dist_dir}}
  cp "target/aarch64-apple-darwin/release/sdkvers-resolve" "{{dist_dir}}/sdkvers-resolve-arm64-apple-darwin"
  chmod +x "{{dist_dir}}/sdkvers-resolve-arm64-apple-darwin"

dist-macos-x86_64:
  #!/usr/bin/env bash
  set -eu -o pipefail
  cargo build --release --target x86_64-apple-darwin
  mkdir -p {{dist_dir}}
  cp "target/x86_64-apple-darwin/release/sdkvers-resolve" "{{dist_dir}}/sdkvers-resolve-x86_64-apple-darwin"
  chmod +x "{{dist_dir}}/sdkvers-resolve-x86_64-apple-darwin"

dist-linux-aarch64:
  just _cross-dist aarch64-unknown-linux-musl aarch64-linux-musl

dist-linux-x86_64:
  just _cross-dist x86_64-unknown-linux-musl x86_64-linux-musl

dist-linux-arm:
  just _cross-dist arm-unknown-linux-musleabihf arm-linux-musleabihf

dist-linux-armv7:
  just _cross-dist armv7-unknown-linux-musleabihf armv7-linux-musleabihf

# Copy the init script and launcher into dist/
_dist-scripts:
  #!/usr/bin/env bash
  set -eu -o pipefail
  mkdir -p {{dist_dir}}
  cp sdkvers-init.sh {{dist_dir}}/sdkvers-init.sh
  cp sdkvers-resolve {{dist_dir}}/sdkvers-resolve
  chmod +x {{dist_dir}}/sdkvers-init.sh
  chmod +x {{dist_dir}}/sdkvers-resolve

_cross-build target:
  #!/usr/bin/env bash
  set -eu -o pipefail
  cross_bin="${CROSS_BIN:-}"
  if [ -z "$cross_bin" ]; then
    if command -v cross >/dev/null 2>&1; then
      cross_bin="$(command -v cross)"
    elif [ -x "$HOME/.cargo/bin/cross" ]; then
      cross_bin="$HOME/.cargo/bin/cross"
    else
      printf '%s\n' 'error: could not find `cross`; install it with `cargo install cross`' >&2
      exit 1
    fi
  fi
  if [ -z "${CROSS_CONTAINER_OPTS:-}" ]; then
    if [ "$(uname -s)" = "Darwin" ] && [ "$(uname -m)" = "arm64" ]; then
      export CROSS_CONTAINER_OPTS="--platform linux/amd64"
    fi
  fi
  "$cross_bin" build --release --target {{target}}

_cross-dist target triple:
  #!/usr/bin/env bash
  set -eu -o pipefail
  just _cross-build {{target}}
  mkdir -p {{dist_dir}}
  cp "target/{{target}}/release/sdkvers-resolve" "{{dist_dir}}/sdkvers-resolve-{{triple}}"
  chmod +x "{{dist_dir}}/sdkvers-resolve-{{triple}}"
