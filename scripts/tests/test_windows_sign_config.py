"""Tests for scripts/windows_sign_config.sh.

The script decides whether a Windows release may ship unsigned. The failure it
exists to prevent is a release that was meant to be signed going out unsigned
because a secret went missing, so every path that could quietly fall back to
"report" is pinned here.
"""

from __future__ import annotations

import os
import subprocess
from pathlib import Path

import pytest

SCRIPT = Path(__file__).resolve().parents[1] / "windows_sign_config.sh"

CONFIG = {
    "AZURE_TENANT_ID": "tenant",
    "AZURE_CLIENT_ID": "client",
    "ARTIFACT_SIGNING_ENDPOINT": "https://eus.codesigning.azure.net",
    "ARTIFACT_SIGNING_ACCOUNT": "account",
    "ARTIFACT_SIGNING_PROFILE": "profile",
}


def run(env: dict[str, str], *args: str) -> subprocess.CompletedProcess[str]:
    # Start from a clean slate so a developer's own shell cannot leak a value in.
    base = {k: v for k, v in os.environ.items() if k not in CONFIG and k != "WINDOWS_SIGNING"}
    return subprocess.run(
        ["bash", str(SCRIPT), *(args or ("mode",))],
        capture_output=True,
        text=True,
        env={**base, **env},
    )


def test_unconfigured_builds_report_only() -> None:
    result = run({})

    assert result.returncode == 0, result.stderr
    assert result.stdout.strip() == "report"


def test_fully_configured_enforces() -> None:
    result = run(CONFIG)

    assert result.returncode == 0, result.stderr
    assert result.stdout.strip() == "enforce"


def test_required_and_configured_enforces() -> None:
    result = run({**CONFIG, "WINDOWS_SIGNING": "required"})

    assert result.returncode == 0, result.stderr
    assert result.stdout.strip() == "enforce"


def test_required_but_unconfigured_fails() -> None:
    """A lost secret after enforcement is on must stop the release, not ship unsigned."""
    result = run({"WINDOWS_SIGNING": "required"})

    assert result.returncode == 1
    assert result.stdout.strip() == ""
    assert "WINDOWS_SIGNING=required" in result.stderr
    for name in CONFIG:
        assert name in result.stderr


def test_empty_values_count_as_unset() -> None:
    """GitHub expands a missing secret to an empty string, not to nothing."""
    result = run({name: "" for name in CONFIG})

    assert result.returncode == 0, result.stderr
    assert result.stdout.strip() == "report"


@pytest.mark.parametrize("missing", sorted(CONFIG))
def test_half_configured_fails_and_names_what_is_missing(missing: str) -> None:
    env = {k: v for k, v in CONFIG.items() if k != missing}

    result = run(env)

    assert result.returncode == 1
    assert result.stdout.strip() == ""
    assert "half configured" in result.stderr
    assert missing in result.stderr


@pytest.mark.parametrize("value", ["require", "true", "REQUIRED", "off", "0"])
def test_unknown_switch_value_fails(value: str) -> None:
    """A typo must not read as "off"."""
    result = run({"WINDOWS_SIGNING": value})

    assert result.returncode == 1
    assert "WINDOWS_SIGNING must be empty or 'required'" in result.stderr


@pytest.mark.parametrize(
    "endpoint",
    [
        "https://eus.codesigning.azure.net",
        "https://wus2.codesigning.azure.net/",
    ],
)
def test_accepts_regional_endpoints(endpoint: str) -> None:
    result = run({**CONFIG, "ARTIFACT_SIGNING_ENDPOINT": endpoint})

    assert result.returncode == 0, result.stderr
    assert result.stdout.strip() == "enforce"


@pytest.mark.parametrize(
    "endpoint",
    [
        "http://eus.codesigning.azure.net",
        "https://eus.codesigning.azure.net.evil.example",
        "https://example.com",
        "eus.codesigning.azure.net",
        "https://eus.codesigning.azure.net/path",
    ],
)
def test_rejects_anything_but_a_microsoft_endpoint(endpoint: str) -> None:
    """The dlib sends our Azure token to this URL."""
    result = run({**CONFIG, "ARTIFACT_SIGNING_ENDPOINT": endpoint})

    assert result.returncode == 1
    assert "ARTIFACT_SIGNING_ENDPOINT" in result.stderr


def test_rejects_unknown_subcommand() -> None:
    result = run({}, "json")

    assert result.returncode == 2
    assert "usage:" in result.stderr
