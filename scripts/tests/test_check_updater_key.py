"""Tests for scripts/check_updater_key.py.

The failure these protect against is concrete: release 26.11.2 built all three
platforms and then failed at the very end with "Wrong password for that key",
having already pushed its version tag. A fake `tauri signer sign` stands in for
the real CLI, accepting one key and password and writing a minisign-shaped
signature, so the tests run without Node.
"""

from __future__ import annotations

import base64
import json
import os
import subprocess
import sys
from pathlib import Path

import pytest

SCRIPT = Path(__file__).resolve().parents[1] / "check_updater_key.py"
sys.path.insert(0, str(SCRIPT.parent))

from check_updater_key import CheckFailed, minisign_key_id  # noqa: E402

KEY = "the-private-key"
PASSWORD = "the-password"
# As minisign prints it; the bytes in the file are little-endian.
KEY_ID = "7AB1ED7122CE7C43"
OTHER_KEY_ID = "8379C3AAFD887EFC"

FAKE_SIGNER = """\
import base64, os, sys
if (os.environ["TAURI_SIGNING_PRIVATE_KEY"], os.environ["TAURI_SIGNING_PRIVATE_KEY_PASSWORD"]) != (
    os.environ["FAKE_KEY"], os.environ["FAKE_PASSWORD"]
):
    print("Error failed to decode secret key: incorrect updater private key password: Wrong password for that key", file=sys.stderr)
    sys.exit(1)
key_id = bytes.fromhex(os.environ["FAKE_KEY_ID"])[::-1]
line = base64.b64encode(b"ED" + key_id + bytes(64)).decode()
text = f"untrusted comment: signature from tauri secret key\\n{line}\\ntrusted comment: x\\nAAAA\\n"
with open(sys.argv[1] + ".sig", "w") as sig:
    sig.write(base64.b64encode(text.encode()).decode())
"""


def pubkey(key_id: str) -> str:
    line = base64.b64encode(b"Ed" + bytes.fromhex(key_id)[::-1] + bytes(32)).decode()
    text = f"untrusted comment: minisign public key: {key_id}\n{line}\n"
    return base64.b64encode(text.encode()).decode()


@pytest.fixture
def config(tmp_path: Path) -> Path:
    path = tmp_path / "tauri.conf.json"
    path.write_text(json.dumps({"plugins": {"updater": {"pubkey": pubkey(KEY_ID)}}}))
    return path


@pytest.fixture
def signer(tmp_path: Path) -> list[str]:
    path = tmp_path / "fake_signer.py"
    path.write_text(FAKE_SIGNER)
    return [sys.executable, str(path)]


def run(
    signer: list[str],
    config: Path,
    *,
    key: str | None = KEY,
    password: str = PASSWORD,
    accepts: tuple[str, str] = (KEY, PASSWORD),
    signs_as: str = KEY_ID,
) -> subprocess.CompletedProcess[str]:
    env = {
        **os.environ,
        "TAURI_SIGNING_PRIVATE_KEY_PASSWORD": password,
        "FAKE_KEY": accepts[0],
        "FAKE_PASSWORD": accepts[1],
        "FAKE_KEY_ID": signs_as,
    }
    env.pop("TAURI_SIGNING_PRIVATE_KEY", None)
    if key is not None:
        env["TAURI_SIGNING_PRIVATE_KEY"] = key
    return subprocess.run(
        [sys.executable, str(SCRIPT), "--config", str(config), "--", *signer],
        env=env,
        capture_output=True,
        text=True,
    )


def test_passes_when_the_key_signs_and_is_trusted(signer: list[str], config: Path) -> None:
    result = run(signer, config)

    assert result.returncode == 0, result.stderr
    assert KEY_ID in result.stdout


def test_fails_when_the_key_is_unset(signer: list[str], config: Path) -> None:
    result = run(signer, config, key=None)

    assert result.returncode == 1
    assert "TAURI_SIGNING_PRIVATE_KEY is unset" in result.stderr


def test_names_a_trailing_newline_on_the_password(signer: list[str], config: Path) -> None:
    result = run(signer, config, password=PASSWORD + "\n")

    assert result.returncode == 1
    assert "TAURI_SIGNING_PRIVATE_KEY_PASSWORD has leading or trailing whitespace" in result.stderr


def test_names_a_password_set_for_a_key_without_one(signer: list[str], config: Path) -> None:
    result = run(signer, config, accepts=(KEY, ""))

    assert result.returncode == 1
    assert "the key has no password" in result.stderr


def test_names_whitespace_around_the_key(signer: list[str], config: Path) -> None:
    result = run(signer, config, key=KEY + "\n")

    assert result.returncode == 1
    assert "TAURI_SIGNING_PRIVATE_KEY has leading or trailing whitespace" in result.stderr


def test_reports_a_plain_mismatch(signer: list[str], config: Path) -> None:
    result = run(signer, config, password="some-other-password")

    assert result.returncode == 1
    assert "is not the password of TAURI_SIGNING_PRIVATE_KEY" in result.stderr
    assert "Wrong password for that key" in result.stderr


def test_never_prints_the_secrets(signer: list[str], config: Path) -> None:
    for result in (run(signer, config), run(signer, config, password="some-other-password")):
        assert KEY not in result.stdout + result.stderr
        assert PASSWORD not in result.stdout + result.stderr


def test_fails_when_the_key_is_not_the_one_the_app_trusts(signer: list[str], config: Path) -> None:
    result = run(signer, config, signs_as=OTHER_KEY_ID)

    assert result.returncode == 1
    assert f"is key {OTHER_KEY_ID}" in result.stderr
    assert f"trusts key {KEY_ID}" in result.stderr


def test_reads_the_key_id_of_a_real_tauri_pubkey() -> None:
    real = (
        "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IDgzNzlDM0FBRkQ4ODdFRkMK"
        "UldUOGZvajlxc041Z3lWdC9VVlZocGRZc3BWeHJ6MjBNK1gyN3ViZjkwWDB6bnd3cE40cUtNS0gK"
    )
    assert minisign_key_id(real) == "8379C3AAFD887EFC"


def test_rejects_something_that_is_not_minisign() -> None:
    with pytest.raises(CheckFailed):
        minisign_key_id(base64.b64encode(b"just text").decode())
