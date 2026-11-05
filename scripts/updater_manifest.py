#!/usr/bin/env python3
"""Write the updater manifest (latest.json) for one release.

The app's updater reads this file from releases/latest/download/latest.json.
It lists, per platform, the bundle to download and its signature, so the
release job builds it from the staged bundles and their .sig files.

Every platform must be present. A manifest that leaves one out would tell
those users there is nothing new, and they would quietly stay behind.

URLs point at the versioned tag, never at "latest", so a manifest and the
bundles it names always come from the same release.
"""

from __future__ import annotations

import argparse
import json
import re
import sys
from datetime import datetime, timezone
from pathlib import Path

# Updater target key -> staged asset name (see the build matrix in
# .github/workflows/release.yml). Apple Silicon only: there is no Intel build.
PLATFORMS = {
    "darwin-aarch64": "Nodea-mac.app.tar.gz",
    "windows-x86_64": "Nodea-windows.msi",
    "linux-x86_64": "Nodea-linux.AppImage",
}

VERSION_RE = re.compile(r"^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$")


class ManifestError(Exception):
    pass


def build_manifest(version: str, repo: str, assets: Path, pub_date: str) -> dict:
    if not VERSION_RE.match(version):
        raise ManifestError(f"Expected a plain semver version such as 26.11.3, got '{version}'")

    base = f"https://github.com/{repo}/releases/download/v{version}"
    platforms = {}
    missing = []
    for target, name in PLATFORMS.items():
        bundle = assets / name
        sig = assets / f"{name}.sig"
        if not bundle.is_file():
            missing.append(name)
            continue
        if not sig.is_file():
            missing.append(sig.name)
            continue
        signature = sig.read_text().strip()
        if not signature:
            raise ManifestError(f"{sig.name} is empty")
        platforms[target] = {"url": f"{base}/{name}", "signature": signature}

    if missing:
        raise ManifestError(f"Missing from {assets}: {', '.join(missing)}")

    return {
        "version": version,
        "notes": f"https://github.com/{repo}/releases/tag/v{version}",
        "pub_date": pub_date,
        "platforms": platforms,
    }


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="Write the updater manifest (latest.json).")
    parser.add_argument("--version", required=True)
    parser.add_argument("--repo", required=True, help="owner/name")
    parser.add_argument("--assets", required=True, type=Path, help="directory of staged bundles")
    parser.add_argument("--pub-date", help="RFC 3339 timestamp (default: now, UTC)")
    args = parser.parse_args(argv)

    pub_date = args.pub_date or datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")
    try:
        manifest = build_manifest(args.version, args.repo, args.assets, pub_date)
    except ManifestError as error:
        print(error, file=sys.stderr)
        return 1

    json.dump(manifest, sys.stdout, indent=2)
    sys.stdout.write("\n")
    return 0


if __name__ == "__main__":
    sys.exit(main())
