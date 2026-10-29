"""Writes a release's notes: what changed (the version's section of
CHANGELOG.md), then how to install Sue (packaging/release_install.md).

The Release workflow publishes them, and GitHub adds its own list of
commits after them.

    python3 packaging/release_notes.py 0.0.4 > notes.md

A version CHANGELOG.md has no section for yet takes the Unreleased one,
with a warning, so a forgotten changelog never holds a release back.
"""

import pathlib
import re
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent


def section(changelog: str, heading: str) -> str | None:
    """The text under `## [heading]`, up to the next section or the links."""
    lines: list[str] | None = None
    for line in changelog.splitlines():
        if line.startswith("## ["):
            if lines is not None:
                break
            if line.startswith(f"## [{heading}]"):
                lines = []
            continue
        if lines is not None:
            if re.match(r"^\[[^\]]+\]: ", line):
                break
            lines.append(line)
    return None if lines is None else "\n".join(lines).strip()


def unwrap(text: str) -> str:
    """Joins wrapped lines back into whole paragraphs and list items:
    GitHub shows every line break in release notes, so the changelog's
    wrapping at 80 columns would break sentences there."""
    out: list[str] = []
    fenced = False
    for line in text.splitlines():
        stripped = line.strip()
        if stripped.startswith("```"):
            fenced = not fenced
            out.append(line)
            continue
        starts_block = (
            not stripped
            or stripped.startswith("#")
            or re.match(r"^([-*+]|\d+\.)\s", stripped) is not None
        )
        joins = (
            not fenced
            and not starts_block
            and out
            and out[-1].strip()
            and not out[-1].lstrip().startswith(("#", "```"))
        )
        if joins:
            out[-1] = f"{out[-1].rstrip()} {stripped}"
        else:
            out.append(line)
    return "\n".join(out)


def main() -> None:
    if len(sys.argv) != 2:
        sys.exit(__doc__)
    version = sys.argv[1].removeprefix("v")
    changelog = (ROOT / "CHANGELOG.md").read_text(encoding="utf-8")
    changes = section(changelog, version)
    if changes is None:
        print(
            f"::warning::CHANGELOG.md has no section for {version}: "
            "the notes use the Unreleased one",
            file=sys.stderr,
        )
        changes = section(changelog, "Unreleased") or ""
    install = (ROOT / "packaging" / "release_install.md").read_text(encoding="utf-8")
    parts = []
    if changes:
        parts.append(f"## What's new in {version}\n\n{unwrap(changes)}")
    parts.append(install.replace("{version}", version).strip())
    sys.stdout.write("\n\n".join(parts) + "\n")


if __name__ == "__main__":
    main()
