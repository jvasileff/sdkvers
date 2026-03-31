# sdkalt Research Scripts

This document preserves the scripts used to survey SDKMAN's post-install hook system, which informed the archive extraction design in [SDKALT.md](SDKALT.md). The survey was conducted once; these scripts are not expected to be run again.

## Scripts

### `list-candidates.py`

Fetches the full list of SDKMAN candidate names from the Broker API, one per line. Used to generate the input list for the hook download loop.

```python
#!/usr/bin/env python3
import urllib.request

req = urllib.request.Request(
    "https://api.sdkman.io/2/candidates/all",
    headers={"User-Agent": "sdkalt/1.0"}
)
with urllib.request.urlopen(req, timeout=15) as resp:
    candidates = resp.read().decode("utf-8").strip().split(",")

for c in sorted(candidates):
    print(c)
```

---

### `fetch_hooks.py`

Downloads all post-install hook scripts for a given candidate across all five supported platforms. Hooks are saved to `hooks/<candidate>/<platform>/<identifier>.sh`. Run once per candidate; the full survey was done with a shell loop over the output of `list-candidates.py`.

```
python fetch_hooks.py [candidate]
```

Notes:
- Defaults to `java` if no candidate is given.
- The version list parser handles both the Java pipe-delimited table format and the plain whitespace layout used by most other candidates.
- A small delay between requests is included to avoid hammering the API.

```python
#!/usr/bin/env python3
"""
Download all SDKMAN post-install hooks for a candidate across all platforms.

Usage:
    python fetch_hooks.py [candidate]

Defaults to "java" if no candidate is given. Hooks are saved to:
    hooks/<candidate>/<platform>/<identifier>.sh

A summary is printed at the end showing unique hook contents.
"""

import sys
import os
import urllib.request
import urllib.error
import time

CANDIDATE = sys.argv[1] if len(sys.argv) > 1 else "java"
BASE_URL = "https://api.sdkman.io/2"
PLATFORMS = ["linuxx64", "darwinarm64", "darwinx64", "linuxarm64", "linuxarm32hf"]
OUTPUT_DIR = os.path.join("hooks", CANDIDATE)
DELAY = 0.2  # seconds between requests


def fetch(url, retries=3):
    for attempt in range(retries):
        try:
            req = urllib.request.Request(url, headers={"User-Agent": "sdkalt-hook-survey/1.0"})
            with urllib.request.urlopen(req, timeout=15) as resp:
                return resp.read().decode("utf-8")
        except urllib.error.HTTPError as e:
            if e.code == 404:
                return None
            if attempt < retries - 1:
                time.sleep(2 ** attempt)
            else:
                raise
        except Exception:
            if attempt < retries - 1:
                time.sleep(2 ** attempt)
            else:
                raise
    return None


def get_versions(candidate, platform):
    url = f"{BASE_URL}/candidates/{candidate}/{platform}/versions/list?installed="
    print(f"  Fetching version list for {platform}...")
    text = fetch(url)
    if not text:
        return []

    # Java-style: | Vendor | Use | Version | Dist | Status | Identifier |
    if "|" in text:
        identifiers = []
        for line in text.splitlines():
            if "|" not in line:
                continue
            parts = [p.strip() for p in line.split("|")]
            if len(parts) < 2:
                continue
            ident = parts[-1].strip()
            if not ident or ident == "Identifier" or ident.startswith("=") or ident.startswith("-"):
                continue
            identifiers.append(ident)
        return identifiers

    # Plain style: versions as whitespace-separated tokens between === blocks
    import re
    sep_count = 0
    identifiers = []
    for line in text.splitlines():
        if line.startswith("="):
            sep_count += 1
            continue
        if sep_count != 2:
            continue
        for token in line.split():
            token = token.lstrip("+*>")
            if token and re.match(r"^[0-9]", token):
                identifiers.append(token)
    return identifiers


def save_hook(candidate, identifier, platform, content):
    path = os.path.join(OUTPUT_DIR, platform, f"{identifier}.sh")
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, "w") as f:
        f.write(content)
    return path


def main():
    print(f"Surveying hooks for candidate: {CANDIDATE}")
    print(f"Output directory: {OUTPUT_DIR}")
    print()

    # Collect all versions across platforms (deduplicated)
    versions_by_platform = {}
    all_versions = set()
    for platform in PLATFORMS:
        versions = get_versions(CANDIDATE, platform)
        versions_by_platform[platform] = versions
        all_versions.update(versions)
        print(f"    {len(versions)} versions found")
        time.sleep(DELAY)

    print(f"\nTotal unique versions: {len(all_versions)}")
    print(f"Downloading hooks...\n")

    downloaded = 0
    empty = 0
    errors = 0

    for platform in PLATFORMS:
        versions = versions_by_platform[platform]
        if not versions:
            continue
        print(f"Platform: {platform} ({len(versions)} versions)")
        for identifier in versions:
            url = f"{BASE_URL}/hooks/post/{CANDIDATE}/{identifier}/{platform}"
            try:
                content = fetch(url)
                time.sleep(DELAY)
                if content is None or content.strip() == "":
                    empty += 1
                else:
                    save_hook(CANDIDATE, identifier, platform, content)
                    downloaded += 1
            except Exception as e:
                print(f"    ERROR {identifier}: {e}")
                errors += 1
        print(f"  done")

    print(f"\nResults:")
    print(f"  Hooks saved:  {downloaded}")
    print(f"  Empty/none:   {empty}")
    print(f"  Errors:       {errors}")

    # Summary: group saved hooks by content
    print(f"\nUnique hook contents:")
    content_map = {}  # content -> list of (platform, identifier)
    for platform in PLATFORMS:
        platform_dir = os.path.join(OUTPUT_DIR, platform)
        if not os.path.isdir(platform_dir):
            continue
        for fname in sorted(os.listdir(platform_dir)):
            if not fname.endswith(".sh"):
                continue
            fpath = os.path.join(platform_dir, fname)
            with open(fpath) as f:
                content = f.read()
            # Use first comment line as a label if present
            label = next(
                (line.strip() for line in content.splitlines() if line.strip().startswith("#")),
                content[:60].strip()
            )
            content_map.setdefault(label, []).append(f"{platform}/{fname[:-3]}")

    for label, instances in sorted(content_map.items()):
        print(f"\n  [{label}]  ({len(instances)} hooks)")
        for i in instances[:5]:
            print(f"    {i}")
        if len(instances) > 5:
            print(f"    ... and {len(instances) - 5} more")


if __name__ == "__main__":
    main()
```

---

### `hashthemall.sh`

Normalizes and hashes all downloaded hook scripts to identify structurally distinct templates. Normalization replaces the version identifier, candidate name, vendor token, platform description string, and JMC-specific paths with placeholders before hashing, so that hooks differing only in those substituted values collapse to the same hash.

Run from the directory containing the `hooks/` tree produced by `fetch_hooks.py`.

```bash
#!/bin/bash

normalize() {
  local f=$1
  local id=$(basename "$f" .sh)
  local candidate=$(echo "$f" | awk -F/ '{print $2}')
  local candidate_title=$(echo "$candidate" | awk '{print toupper(substr($0,1,1)) substr($0,2)}')
  local vendor=$(echo "$id" | LC_ALL=C sed 's/.*-//')
  LC_ALL=C sed "s/$id/IDENTIFIER/g" "$f" \
    | LC_ALL=C sed "s/$candidate_title/CANDIDATE/g" \
    | LC_ALL=C sed "s/$candidate/CANDIDATE/g" \
    | LC_ALL=C sed 's/Linux ARM 32bit Hard Float/PLATFORM/g' \
    | LC_ALL=C sed 's/Linux ARM 64bit/PLATFORM/g' \
    | LC_ALL=C sed 's/Linux 64bit/PLATFORM/g' \
    | LC_ALL=C sed 's/macOS ARM 64bit/PLATFORM/g' \
    | LC_ALL=C sed 's/macOS 64bit/PLATFORM/g' \
    | LC_ALL=C sed "s/JMC $vendor/JMC VENDOR/g" \
    | LC_ALL=C sed 's|executable_binary=".*"|executable_binary="EXECUTABLE"|g' \
    | LC_ALL=C sed 's|containing_folder=.*|containing_folder="FOLDER"|g'
}

TMPDIR=$(mktemp -d)

find hooks -name "*.sh" | while read f; do
  hash=$(normalize "$f" | md5)
  example_file="$TMPDIR/$hash"
  if [ ! -f "$example_file" ]; then
    echo "$f" > "$example_file"
  fi
  echo "$hash"
done | sort | uniq -c | sort -rn | while read cnt hash; do
  example=$(cat "$TMPDIR/$hash" 2>/dev/null || echo "unknown")
  template=$(LC_ALL=C grep -o 'Post Hook:.*' "$example" 2>/dev/null | head -1)
  echo "  $cnt hooks  $hash  ${template:-no label}  (e.g. $example)"
done

rm -rf "$TMPDIR"
```

## Results

Final output of `hashthemall.sh` after downloading hooks for all SDKMAN candidates:

```
  13775 hooks  fcb2384eb3d368d473c1e99d1561ff9c  Post Hook: default-zip        (e.g. hooks/activemq/linuxarm32hf/5.12.0.sh)
    570 hooks  ed8b941f3f502247fa2fb59477337c4c  Post Hook: default-tarball     (e.g. hooks/flink/linuxarm32hf/1.10.0.sh)
    328 hooks  87d5e14e777c1a77a805d4da7d9fe36e  Post Hook: linux-java-tarball  (e.g. hooks/java/linuxarm32hf/17.0.18-librca.sh)
    155 hooks  71cfc4bb7c090de0b8b5e2674f65ba62  Post Hook: osx-java-tarball    (e.g. hooks/java/darwinarm64/24.2.2.r24-mandrel.sh)
     12 hooks  a091ec7b7b0b2f9a3c27e98f9e8728af  Post Hook: unix-jmc-tarball    (e.g. hooks/jmc/linuxarm64/9.1.1-zulu.sh)
     11 hooks  0c11a5f98122448338c99a3cb9cc8789  Post Hook: unix-jmc-tarball    (e.g. hooks/jmc/linuxarm64/9.1.1-adpt.sh)
```

The two `unix-jmc-tarball` hashes correspond to two structural variants in the `jmc` candidate: archives that contain a top-level directory (zulu, librca) and archives that are flat and require a containing directory to be created (adpt and others). Both variants create a `bin/jmc` symlink pointing to the vendor-specific executable after extraction.
