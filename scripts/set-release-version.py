#!/usr/bin/env python3
"""Set release-only Cargo metadata without resolving or refreshing dependencies."""

import re
import sys
from pathlib import Path


def replace_one(pattern, replacement, text):
    updated, count = re.subn(pattern, replacement, text, flags=re.MULTILINE)
    if count != 1:
        raise ValueError(f"expected one match for {pattern!r}, found {count}")
    return updated


def prepare(tag):
    number = r"(?:0|[1-9][0-9]*)"
    if not re.fullmatch(rf"v{number}\.{number}\.{number}(?:-rc\.[1-9][0-9]*)?", tag):
        raise ValueError("expected vX.Y.Z or vX.Y.Z-rc.N without leading zeroes")
    version = tag[1:]
    updates = {}
    names = ("gitpm", "gitpm-cli", "gitpm-ffi")
    for name in names:
        manifest = Path("crates") / name / "Cargo.toml"
        text = manifest.read_text()
        # Bound the replacement to [package], leaving metadata and dependencies intact.
        text = replace_one(
            r'(\[package\]\n(?:(?!\[)[^\n]*\n)*?version = ")[^"]+(".*)$',
            lambda m: m[1] + version + m[2], text,
        )
        if name != "gitpm":
            text = replace_one(
                r'^(gitpm = \{[^\n]*\bversion = ")[^"]+("[^\n]*\})$',
                lambda m: m[1] + version + m[2], text,
            )
        updates[manifest] = text

    lock = Path("Cargo.lock")
    text = lock.read_text()
    for name in names:
        text = replace_one(
            rf'(\[\[package\]\]\nname = "{name}"\nversion = ")[^"]+("\n)(?!source =)',
            lambda m: m[1] + version + m[2], text,
        )
    updates[lock] = text
    # Validate every expected record before writing any file.
    for file, content in updates.items():
        file.write_text(content)


if __name__ == "__main__":
    if len(sys.argv) != 2:
        sys.exit("usage: set-release-version.py TAG")
    try:
        prepare(sys.argv[1])
    except ValueError as error:
        sys.exit(str(error))
