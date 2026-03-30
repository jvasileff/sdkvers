# sdkvers

`sdkvers` activates the right tool versions for a project by reading a `.sdkvers` file and running `sdk use` for each declared candidate. Drop a `.sdkvers` file in your project, run `sdkvers`, and your shell switches to the right Java, Maven, Gradle, or Kotlin versions automatically.

## Prerequisites

- [SDKMAN](https://sdkman.io) must be installed and initialized in your shell.
- The tool versions you want to use must already be installed via `sdk install`.

`sdkvers` only selects from what is already installed locally. It does not install missing versions.

## Installation

Create `~/.sdkvers` and extract the latest release into it:

```sh
mkdir -p ~/.sdkvers
curl -L https://github.com/jvasileff/sdkvers/releases/latest/download/sdkvers.tar.gz \
  | tar xz --strip-components=1 -C ~/.sdkvers
```

Then add these two lines to your shell profile (`~/.bashrc`, `~/.zshrc`, etc.):

```sh
export SDKVERS_HOME="$HOME/.sdkvers"
. "$SDKVERS_HOME/sdkvers-init.sh"
```

Reload your shell or `source` the profile file to apply the changes.

### macOS: removing quarantine

If you downloaded the release via a browser instead of `curl`, macOS will quarantine the binaries and refuse to run them. Remove the quarantine attribute with:

```sh
xattr -dr com.apple.quarantine ~/.sdkvers
```

## Usage

In any directory at or below a project containing a `.sdkvers` file, run:

```sh
sdkvers
```

`sdkvers` walks upward from the current directory until it finds a `.sdkvers` file, then activates each declared version in the current shell.

On success:

```
Using java 21.0.2-tem
Using maven 3.9.14
```

If a required version is not installed:

```
sdkvers: candidate "java" has no installed version matching [21,22) with vendor "tem"
```

All candidates are attempted before any errors are reported. The exit status is non-zero if any candidate could not be activated.

## The `.sdkvers` file

Each line declares one candidate requirement:

```
<candidate> = <version-expr> [vendor]
```

Blank lines and lines starting with `#` are ignored.

### Examples

```
# Simple version lines
java = 21
maven = 3.9
gradle = 8.7.0

# Explicit version range
java = [21,22)

# Specific vendor (e.g. Temurin, GraalVM CE)
java = 21 tem
java = 21 graalce
```

Candidate names must match SDKMAN candidate names exactly (`java`, `maven`, `gradle`, etc.).

### Version expressions

A bare numeric version is shorthand for a range:

| Expression | Matches |
|------------|---------|
| `21` | any installed Java 21.x.x |
| `3.9` | any installed 3.9.x |
| `8.7.0` | exactly 8.7.0 |

For more control, use explicit Maven-style range syntax:

| Expression | Matches |
|------------|---------|
| `[21,22)` | `>= 21` and `< 22` |
| `[3.9,4)` | `>= 3.9` and `< 4` |
| `[21,)` | `>= 21` (no upper bound) |
| `[21.0.5]` | exactly 21.0.5 |

When multiple installed versions satisfy a requirement, the highest matching version is selected.

Pre-release versions (ea, rc, alpha, beta, etc.) are excluded from ranges unless you explicitly opt in by including a pre-release qualifier in a bound, e.g. `[26.ea,)`.

### Vendor filtering

For candidates that expose a distribution field (currently Java), an optional vendor token after the version expression filters by distribution:

```
java = 21 tem       # Temurin only
java = [21,) graalce  # GraalVM CE, any 21+
```

Vendor matching is case-sensitive and exact.

For full details on version comparison and range semantics, see [SPEC.md](SPEC.md).
