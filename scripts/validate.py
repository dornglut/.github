#!/usr/bin/env python3
"""Read-only validation for documentation-oriented Dornglut repositories."""

from __future__ import annotations

import re
import sys
from pathlib import Path
from urllib.parse import unquote

ROOT = Path(__file__).resolve().parents[1]
TEXT_SUFFIXES = {".md", ".yml", ".yaml", ".txt", ".py"}
LINK_RE = re.compile(r"!?\[[^\]]*\]\(([^)]+)\)")


def fail(message: str, failures: list[str]) -> None:
    failures.append(message)


def validate_text_file(path: Path, failures: list[str]) -> None:
    data = path.read_bytes()
    relative = path.relative_to(ROOT).as_posix()

    if b"\x00" in data:
        fail(f"{relative}: contains a NUL byte", failures)
        return

    try:
        text = data.decode("utf-8")
    except UnicodeDecodeError:
        fail(f"{relative}: is not valid UTF-8", failures)
        return

    if text and not text.endswith("\n"):
        fail(f"{relative}: must end with a newline", failures)

    for line_number, line in enumerate(text.splitlines(), start=1):
        if line.endswith((" ", "\t")):
            fail(f"{relative}:{line_number}: trailing whitespace", failures)
        if "\t" in line:
            fail(f"{relative}:{line_number}: tab character", failures)

    if path.suffix.lower() != ".md":
        return

    for match in LINK_RE.finditer(text):
        raw_target = match.group(1).strip()
        target = raw_target.split(maxsplit=1)[0].strip("<>")
        if not target or target.startswith(("#", "http://", "https://", "mailto:")):
            continue

        target = unquote(target.split("#", 1)[0].split("?", 1)[0])
        if not target:
            continue

        resolved = (path.parent / target).resolve()
        try:
            resolved.relative_to(ROOT)
        except ValueError:
            fail(f"{relative}: link escapes repository: {raw_target}", failures)
            continue

        if not resolved.exists():
            fail(f"{relative}: broken relative link: {raw_target}", failures)


def main() -> int:
    failures: list[str] = []

    required_file = ROOT / "validation-required-files.txt"
    if not required_file.is_file():
        fail("validation-required-files.txt: missing", failures)
    else:
        for raw_line in required_file.read_text(encoding="utf-8").splitlines():
            required = raw_line.strip()
            if not required or required.startswith("#"):
                continue
            if not (ROOT / required).exists():
                fail(f"{required}: required path is missing", failures)

    for path in sorted(ROOT.rglob("*")):
        if not path.is_file() or ".git" in path.parts:
            continue
        if path.suffix.lower() in TEXT_SUFFIXES or path.name in {"CODEOWNERS"}:
            validate_text_file(path, failures)

    if failures:
        print("repository validation failed:", file=sys.stderr)
        for failure in failures:
            print(f"- {failure}", file=sys.stderr)
        return 1

    print("repository validation passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
