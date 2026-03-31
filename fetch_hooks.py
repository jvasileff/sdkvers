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
