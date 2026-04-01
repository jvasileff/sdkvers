# API TODO

## 1. Empty hook fingerprint default

**Location:** `crates/broker/src/hooks.rs`, `fetch_hook`

When the broker returns an empty hook body, `fetch_hook` currently returns
`HookFingerprint::DefaultZip` as a stand-in. This is wrong: an empty hook
provides no information about extraction layout. The correct fix is to add a
`HookFingerprint::Default` (or similar) variant representing "no hook, use
format-based defaults", and handle it in `store::extract` by falling back to
a sensible per-format strategy (e.g. strip-leading-dir for both zip and tar.gz).

## 2. Archive format detection is inert

**Location:** `crates/broker/src/download.rs`, `detect_format` /
`DownloadedArchive::format`

`detect_format` inspects the temp filename extension to determine the archive
format, but temp filenames (`sdkvers-{candidate}-{identifier}`) have no
extension, so it always returns `ArchiveFormat::TarGz`. The field is also
unused end-to-end now that `store::extract` takes only the hook fingerprint.

Options:
- Detect format from the `Content-Type` response header during download.
- Detect format from archive magic bytes after download.
- Remove `DownloadedArchive::format` entirely and derive format from the hook
  fingerprint (which already implies the archive type for all known templates).

These two items are related: resolving the empty-hook default (item 1) and
deciding whether format is derived from the hook or detected independently
(item 2) should be tackled together.
