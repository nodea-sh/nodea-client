"""Tests for scripts/updater_manifest.py.

The manifest is what every installed app trusts to find its next version, so
a release must never publish one that leaves a platform out or points at a
different release than its own.
"""

from __future__ import annotations

import json
import subprocess
import sys
from pathlib import Path

SCRIPT = Path(__file__).resolve().parents[1] / "updater_manifest.py"
REPO = "nodea-sh/nodea-client"
ASSETS = ["Nodea-mac.app.tar.gz", "Nodea-windows.msi", "Nodea-linux.AppImage"]


def stage(directory: Path, skip: str | None = None) -> None:
    """Writes every bundle and its .sig, except the file named by skip."""
    for name in ASSETS:
        for file, content in ((name, "bundle"), (f"{name}.sig", f"sig-of-{name}\n")):
            if file != skip:
                (directory / file).write_text(content)


def run(directory: Path, version: str = "26.11.3") -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        [
            sys.executable,
            str(SCRIPT),
            "--version",
            version,
            "--repo",
            REPO,
            "--assets",
            str(directory),
            "--pub-date",
            "2026-11-05T12:00:00Z",
        ],
        capture_output=True,
        text=True,
    )


def test_writes_every_platform_with_its_signature(tmp_path: Path) -> None:
    stage(tmp_path)

    result = run(tmp_path)

    assert result.returncode == 0, result.stderr
    manifest = json.loads(result.stdout)
    base = f"https://github.com/{REPO}/releases/download/v26.11.3"
    assert manifest == {
        "version": "26.11.3",
        "notes": f"https://github.com/{REPO}/releases/tag/v26.11.3",
        "pub_date": "2026-11-05T12:00:00Z",
        "platforms": {
            "darwin-aarch64": {
                "url": f"{base}/Nodea-mac.app.tar.gz",
                "signature": "sig-of-Nodea-mac.app.tar.gz",
            },
            "windows-x86_64": {
                "url": f"{base}/Nodea-windows.msi",
                "signature": "sig-of-Nodea-windows.msi",
            },
            "linux-x86_64": {
                "url": f"{base}/Nodea-linux.AppImage",
                "signature": "sig-of-Nodea-linux.AppImage",
            },
        },
    }


def test_urls_point_at_the_tag_not_latest(tmp_path: Path) -> None:
    """A "latest" URL could serve a newer bundle than the signature is for."""
    stage(tmp_path)

    manifest = json.loads(run(tmp_path).stdout)

    for entry in manifest["platforms"].values():
        assert "/releases/download/v26.11.3/" in entry["url"]
        assert "latest" not in entry["url"]


def test_fails_when_a_signature_is_missing(tmp_path: Path) -> None:
    stage(tmp_path, skip="Nodea-windows.msi.sig")

    result = run(tmp_path)

    assert result.returncode == 1
    assert result.stdout == ""
    assert "Nodea-windows.msi.sig" in result.stderr


def test_fails_when_a_bundle_is_missing(tmp_path: Path) -> None:
    stage(tmp_path, skip="Nodea-mac.app.tar.gz")

    result = run(tmp_path)

    assert result.returncode == 1
    assert "Nodea-mac.app.tar.gz" in result.stderr


def test_fails_on_an_empty_signature(tmp_path: Path) -> None:
    stage(tmp_path)
    (tmp_path / "Nodea-linux.AppImage.sig").write_text("\n")

    result = run(tmp_path)

    assert result.returncode == 1
    assert "Nodea-linux.AppImage.sig is empty" in result.stderr


def test_rejects_a_version_that_is_not_plain_semver(tmp_path: Path) -> None:
    """The tag adds the "v"; passing it in too would produce vv26.11.3 URLs."""
    stage(tmp_path)

    for version in ("v26.11.3", "26.11", "26.11.3-beta"):
        result = run(tmp_path, version=version)
        assert result.returncode == 1, version
        assert "plain semver" in result.stderr
