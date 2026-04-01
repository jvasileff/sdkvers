# Color and TTY

This document covers how `sdkvers` handles color output and TTY detection, and the direction for future interactive features.

## The stdout-is-a-pipe problem

The `sdkvers` shell function captures output from `sdkvers-resolve` using command substitution (`$()`). This means the binary's stdout is **always a pipe**, regardless of whether the user's terminal is attached. The binary cannot use `isatty(stdout)` to decide whether to colorize output—it will always return false.

`git` and similar tools face the same constraint and solve it by checking stderr or using other heuristics. We solve it explicitly: the shell function is the authoritative source of terminal state for stdout, and communicates it to the binary via a flag.

## The `--stdout-is-tty` flag

`sdkvers-init.sh` checks `[ -t 1 ]` (is the shell function's stdout a terminal?) before invoking the binary:

```bash
if [ -t 1 ]; then
  sdkvers_output=$("$sdkvers_resolver" sdkvers --stdout-is-tty "$@")
else
  sdkvers_output=$("$sdkvers_resolver" sdkvers "$@")
fi
```

The binary strips `--stdout-is-tty` from the argument list before dispatch and uses it to decide whether to colorize output. This means color is suppressed correctly when the user pipes (`sdkvers | cat`) or redirects (`sdkvers > file`).

## Fallback for direct invocation

When `sdkvers-resolve` is invoked directly (not through the shell function), `--stdout-is-tty` is absent. In this case the binary falls back to `isatty(stdout)`, which is accurate because no `$()` capture is involved.

## Environment variable overrides

The following standard conventions are respected and always take precedence:

- `NO_COLOR` (any value) — disables color unconditionally
- `TERM=dumb` — disables color unconditionally

## Stderr

The binary's stderr is not captured by the shell function, so color on stderr is determined independently via `isatty(stderr)`. This is correct: stderr goes directly to the user's terminal.

## Current color usage

- Success messages: green
- Error messages: red
- Hint lines (`\nhint: ...`): yellow

Color is applied as a shim in `main.rs` (`src/main.rs`). The library (`src/lib.rs`) generates plain text; the binary colorizes it before writing to stdout/stderr. See the TODO for the planned refactor to make this cleaner.

## Future interactivity

The `--stdout-is-tty` mechanism establishes the general pattern for communicating terminal state from the shell function to the binary. Future interactive features (prompts, confirmations) will require knowing whether **stdin** is a terminal, which has the same constraint: `isatty(stdin)` inside `$()` would be unreliable because stdin may be inherited from the parent shell, but explicit propagation is safer.

The expected extension is a `--stdin-is-tty` flag, set by the shell function based on `[ -t 0 ]`, passed alongside `--stdout-is-tty` when appropriate. Any interactive feature should gate on both flags being present before attempting to read from the terminal.
