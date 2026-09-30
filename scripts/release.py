#!/usr/bin/env python3
"""Validate release tags and package prebuilt binaries (Python 3.11+, stdlib only)."""

import argparse
import hashlib
from pathlib import Path
import plistlib
import re
import shutil
import sys
import tarfile
import tempfile
import tomllib
import zipfile

ROOT = Path(__file__).resolve().parent.parent
# SemVer 2.0.0: numeric prerelease identifiers cannot have leading zeroes;
# build identifiers can. Git tag convention permits one optional lowercase v.
NUMBER = r"(?:0|[1-9][0-9]*)"
PRE = rf"(?:{NUMBER}|[0-9]*[A-Za-z-][0-9A-Za-z-]*)"
SEMVER = re.compile(
    rf"(?P<core>{NUMBER}\.{NUMBER}\.{NUMBER})"
    rf"(?:-(?P<pre>{PRE}(?:\.{PRE})*))?"
    r"(?:\+[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?"
)
TARGETS = (
    "aarch64-apple-darwin",
    "x86_64-apple-darwin",
    "x86_64-pc-windows-msvc",
    "x86_64-unknown-linux-gnu",
)


def tag_metadata(tag: str, manifest: Path) -> dict[str, str]:
    version = tag.removeprefix("v")
    match = SEMVER.fullmatch(version)
    if not match:
        return {"valid": "false"}
    with manifest.open("rb") as source:
        expected = tomllib.load(source)["workspace"]["package"]["version"]
    if version != expected:
        raise ValueError(f"tag version {version!r} does not match Cargo.toml {expected!r}")
    return {
        "valid": "true",
        "version": version,
        "prerelease": str(match["pre"] is not None).lower(),
    }


def package(version: str, target: str, binary: Path, output: Path) -> Path:
    match = SEMVER.fullmatch(version)
    if not match or target not in TARGETS:
        raise ValueError("packaging requires a SemVer version and a supported target")
    if not binary.is_file():
        raise ValueError(f"release binary not found: {binary}")
    output.mkdir(parents=True, exist_ok=True)
    name = f"Breeze-{version}-{target}"
    with tempfile.TemporaryDirectory(prefix=".package-", dir=output) as scratch:
        contents = Path(scratch) / name
        contents.mkdir()
        if target.endswith("apple-darwin"):
            app = contents / "Breeze.app" / "Contents"
            executable = app / "MacOS" / "Breeze"
            resources = app / "Resources"
            resources.mkdir(parents=True)
            shutil.copy2(ROOT / "assets/brand/Breeze.icns", resources)
            with (app / "Info.plist").open("wb") as plist:
                plistlib.dump(
                    {
                        "CFBundleIdentifier": "dev.breeze.reader.prototype",
                        "CFBundleName": "Breeze",
                        "CFBundleDisplayName": "Breeze",
                        "CFBundleExecutable": "Breeze",
                        "CFBundleIconFile": "Breeze.icns",
                        "CFBundlePackageType": "APPL",
                        # Apple's bundle version fields cannot contain SemVer suffixes.
                        "CFBundleShortVersionString": match["core"],
                        "CFBundleVersion": match["core"],
                        "BreezeReleaseVersion": version,
                        "LSMinimumSystemVersion": "15.0",
                        "NSHighResolutionCapable": True,
                    },
                    plist,
                )
        else:
            executable = contents / (
                "breeze-app.exe" if target.endswith("windows-msvc") else "breeze-app"
            )
        executable.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(binary, executable)
        executable.chmod(0o755)
        # Copy any adjacent runtime DLLs emitted by the Windows build, if present.
        if target.endswith("windows-msvc"):
            for dll in binary.parent.glob("*.dll"):
                shutil.copy2(dll, contents)
        shutil.copy2(ROOT / "README.md", contents)
        shutil.copy2(ROOT / "Cargo.lock", contents)
        shutil.copy2(ROOT / "docs/releases.md", contents / "RELEASE-NOTES.md")
        shutil.copytree(ROOT / "assets/licenses", contents / "licenses")
        (contents / "VERSION").write_text(version + "\n", encoding="utf-8", newline="\n")
        if target.endswith("linux-gnu"):
            archive = output / f"{name}.tar.gz"
            with tarfile.open(archive, "w:gz") as bundle:
                bundle.add(contents, arcname=name)
        else:
            archive = output / f"{name}.zip"
            with zipfile.ZipFile(archive, "w", zipfile.ZIP_DEFLATED) as bundle:
                for file in sorted(contents.rglob("*")):
                    if file.is_file():
                        bundle.write(file, file.relative_to(scratch))
    with archive.open("rb") as source:
        digest = hashlib.file_digest(source, "sha256").hexdigest()
    archive.with_name(archive.name + ".sha256").write_text(
        f"{digest}  {archive.name}\n", encoding="utf-8", newline="\n"
    )
    return archive


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    validate = commands.add_parser("validate", help="skip non-SemVer tags; check Cargo version")
    validate.add_argument("--tag", required=True)
    validate.add_argument("--github-output", type=Path)
    pack = commands.add_parser("package", help="archive an already-built release binary")
    pack.add_argument("--version", required=True)
    pack.add_argument("--target", choices=TARGETS, required=True)
    pack.add_argument("--binary", type=Path, required=True)
    pack.add_argument("--output", type=Path, default=ROOT / "target/dist")
    args = parser.parse_args()
    try:
        if args.command == "validate":
            metadata = tag_metadata(args.tag, ROOT / "Cargo.toml")
            result = "".join(f"{key}={value}\n" for key, value in metadata.items())
            print(result, end="")
            if args.github_output:
                with args.github_output.open("a", encoding="utf-8") as output:
                    output.write(result)
        else:
            print(package(args.version, args.target, args.binary, args.output))
    except (OSError, ValueError, KeyError) as error:
        print(f"release: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
