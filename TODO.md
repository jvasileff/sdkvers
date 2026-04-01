# TODO

## Future product idea placeholder

## Structured output: move text generation from lib to binary

Currently `src/lib.rs` generates human-readable prose strings (error messages, success messages, hints). Color is applied as a post-processing shim in `src/main.rs`: the binary receives plain text from the library and wraps it in ANSI escape codes before writing to stdout/stderr.

This is a layering violation. The library should return structured data describing what happened (resolved version, error kind, affected path, hint text, etc.), and the binary should be solely responsible for rendering that data into human-readable, colorized output.

The refactor involves:
- Defining result/error types in the library that carry structured fields rather than pre-formatted strings
- Moving all prose construction and colorization into `main.rs` (and eventually into a dedicated rendering module)
- Keeping the library free of any formatting or presentation concerns

Until this is done, adding new color treatments or adjusting message wording requires touching both files, and the color shim in `main.rs` must make assumptions about the internal structure of strings (e.g., scanning for `\nhint: ` to split the hint from the body).
