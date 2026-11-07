#!/usr/bin/env python3
"""Check the updater signing key before a release spends a build on it.

`tauri build` signs the updater bundles last, so a key and password that do not
match only fail after every platform has compiled, bundled and notarised. This
signs a throwaway file with the same key and password instead, which takes
seconds.

It also checks the key is the one the app trusts: the signature's key ID must
match the `pubkey` in tauri.conf.json. A different key still signs fine, but
every installed copy of the app would reject the update.

The key and password are read from TAURI_SIGNING_PRIVATE_KEY and
TAURI_SIGNING_PRIVATE_KEY_PASSWORD and are never printed. When signing fails it
retries with common paste mistakes corrected, only to say which one it is.

Usage: check_updater_key.py --config src-tauri/tauri.conf.json -- <tauri> signer sign
"""

from __future__ import annotations

import argparse
import base64
import json
import os
import subprocess
import sys
import tempfile
from pathlib import Path

KEY_VAR = "TAURI_SIGNING_PRIVATE_KEY"
PASSWORD_VAR = "TAURI_SIGNING_PRIVATE_KEY_PASSWORD"


class CheckFailed(Exception):
    pass


def minisign_key_id(encoded: str) -> str:
    """Key ID of a Tauri public key or .sig, as minisign prints it.

    Both are base64 of a minisign file whose first base64 line is a two-byte
    algorithm tag followed by the eight-byte key ID.
    """
    try:
        text = base64.b64decode(encoded.strip(), validate=True).decode()
        payload = base64.b64decode(text.splitlines()[1].strip(), validate=True)
    except (ValueError, IndexError, UnicodeDecodeError) as exc:
        raise CheckFailed(f"not a minisign key or signature: {exc}") from exc
    if len(payload) < 10:
        raise CheckFailed("not a minisign key or signature: too short")
    return payload[2:10][::-1].hex().upper()


def configured_pubkey(config: Path) -> str:
    try:
        return json.loads(config.read_text())["plugins"]["updater"]["pubkey"]
    except KeyError as exc:
        raise CheckFailed(f"{config} has no plugins.updater.pubkey") from exc


def sign(command: list[str], key: str, password: str, target: Path) -> subprocess.CompletedProcess[str]:
    sig = Path(f"{target}.sig")
    sig.unlink(missing_ok=True)
    env = {**os.environ, KEY_VAR: key, PASSWORD_VAR: password}
    return subprocess.run([*command, str(target)], env=env, capture_output=True, text=True)


def diagnose(command: list[str], key: str, password: str, target: Path) -> str:
    """Name the paste mistake that explains the failure, if one does."""
    candidates = [
        (key, password.strip(), f"{PASSWORD_VAR} has leading or trailing whitespace (often a newline from pasting); set it again without"),
        (key, "", f"the key has no password, so {PASSWORD_VAR} must be empty"),
        (key.strip(), password, f"{KEY_VAR} has leading or trailing whitespace; set it again without"),
    ]
    for candidate_key, candidate_password, cause in candidates:
        if (candidate_key, candidate_password) == (key, password):
            continue
        if sign(command, candidate_key, candidate_password, target).returncode == 0:
            return cause
    return (
        f"{PASSWORD_VAR} is not the password of {KEY_VAR}. Check the pair locally "
        "with `tauri signer sign`, or generate a new pair and update both secrets"
    )


def check(command: list[str], config: Path) -> str:
    key = os.environ.get(KEY_VAR, "")
    password = os.environ.get(PASSWORD_VAR, "")
    if not key:
        raise CheckFailed(f"{KEY_VAR} is unset - the updater bundles cannot be signed")

    with tempfile.TemporaryDirectory() as tmp:
        target = Path(tmp) / "updater-key-probe.txt"
        target.write_text("nodea updater key probe\n")

        result = sign(command, key, password, target)
        if result.returncode != 0:
            raise CheckFailed(
                f"the updater key cannot sign: {diagnose(command, key, password, target)}\n"
                f"tauri said: {result.stderr.strip()}"
            )
        signed_with = minisign_key_id(Path(f"{target}.sig").read_text())

    trusted = minisign_key_id(configured_pubkey(config))
    if signed_with != trusted:
        raise CheckFailed(
            f"{KEY_VAR} is key {signed_with}, but {config} trusts key {trusted}. "
            "Installed apps would reject this release's updates; update the pubkey "
            "or use the matching private key"
        )
    return signed_with


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--config", type=Path, required=True, help="tauri.conf.json holding the updater pubkey")
    parser.add_argument("command", nargs="+", help="the signing command, e.g. `npx tauri signer sign`")
    args = parser.parse_args(argv)

    try:
        key_id = check(args.command, args.config)
    except CheckFailed as exc:
        print(f"::error::{exc}", file=sys.stderr)
        return 1
    print(f"The updater key signs, and is the key the app trusts ({key_id})")
    return 0


if __name__ == "__main__":
    sys.exit(main())
