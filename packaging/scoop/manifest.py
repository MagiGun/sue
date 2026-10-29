"""Writes Sue's Scoop manifest (`sue.json`) for a release.

The Release workflow runs it on the release's files and publishes the
manifest with them, so Windows users can install the newest Sue with

    scoop install https://github.com/MagiGun/sue/releases/latest/download/sue.json

and `scoop update sue` follows the same link. Scoop downloads and unpacks
the portable zip itself, so Windows does not warn about the download the
way it does for one from a browser.

    python3 packaging/scoop/manifest.py --tag v0.0.3 --dist dist
"""

import argparse
import hashlib
import json
import pathlib

REPOSITORY = "https://github.com/MagiGun/sue"
# The release's Windows architectures and Scoop's names for them.
ARCHITECTURES = {"x64": "64bit", "x86": "32bit", "arm64": "arm64"}


def sha256(path: pathlib.Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as file:
        for block in iter(lambda: file.read(1 << 20), b""):
            digest.update(block)
    return digest.hexdigest()


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--tag", required=True, help="the release's tag, such as v0.0.3")
    parser.add_argument("--dist", default="dist", help="the folder with the release's files")
    args = parser.parse_args()
    version = args.tag.removeprefix("v")
    dist = pathlib.Path(args.dist)

    architecture = {}
    for arch, scoop_name in ARCHITECTURES.items():
        name = f"Sue-{version}-windows-{arch}-portable.zip"
        path = dist / name
        if path.exists():
            architecture[scoop_name] = {
                "url": f"{REPOSITORY}/releases/download/{args.tag}/{name}",
                "hash": sha256(path),
            }
    if not architecture:
        raise SystemExit(f"no Sue-{version}-windows-*-portable.zip in {dist}: no Scoop manifest")

    manifest = {
        "version": version,
        "description": "One creative environment for painting, comics, animation, writing and sound.",
        "homepage": "https://magigun.github.io/sue/",
        "license": {"identifier": "LicenseRef-Sayn", "url": f"{REPOSITORY}/blob/main/LICENSE"},
        "notes": "Sue is in the Start menu; `sue` in a terminal runs sue-cli.exe.",
        "architecture": architecture,
        # The window from the Start menu, the console twin as `sue`.
        "shortcuts": [["Sue.exe", "Sue"]],
        "bin": [["sue-cli.exe", "sue"]],
    }
    out = dist / "sue.json"
    out.write_text(json.dumps(manifest, indent=4) + "\n", encoding="utf-8")
    print(f"wrote {out} for {', '.join(architecture)}")


if __name__ == "__main__":
    main()
