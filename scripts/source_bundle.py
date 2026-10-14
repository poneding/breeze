#!/usr/bin/env python3
"""Build an AGPL corresponding-source archive for one Breeze revision.

The archive contains the exact Git tree, Cargo.lock, release/build scripts, and
all locked crates from `cargo vendor`, including mupdf-sys and its MuPDF source.
"""

import argparse
import hashlib
from io import BytesIO
from pathlib import Path, PurePosixPath
import subprocess
import sys
import tarfile
import tempfile
import tomllib

ROOT = Path(__file__).resolve().parent.parent


def _run(command: list[str], *, cwd: Path) -> subprocess.CompletedProcess[bytes]:
    result = subprocess.run(command, cwd=cwd, capture_output=True, check=False)
    if result.returncode:
        detail = result.stderr.decode("utf-8", "replace").strip()
        raise RuntimeError(f"{command[0]} failed ({result.returncode}): {detail}")
    return result


def _extract_git_tree(revision: str, destination: Path) -> None:
    archive = _run(["git", "archive", "--format=tar", revision], cwd=ROOT)
    with tarfile.open(fileobj=BytesIO(archive.stdout), mode="r:") as bundle:
        members = bundle.getmembers()
        for member in members:
            path = PurePosixPath(member.name)
            if path.is_absolute() or ".." in path.parts:
                raise RuntimeError(f"Unsafe path in Git source archive: {member.name}")
            if member.issym() or member.islnk():
                target = PurePosixPath(member.linkname)
                resolved = (path.parent / target) if not target.is_absolute() else target
                if target.is_absolute() or ".." in resolved.parts:
                    raise RuntimeError(f"Unsafe link in Git source archive: {member.name}")
        bundle.extractall(destination, members=members)


def create_source_bundle(version: str, revision: str, output: Path) -> tuple[Path, Path]:
    output.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="breeze-source-") as temporary:
        source = Path(temporary) / f"breeze-{version}"
        source.mkdir()
        _extract_git_tree(revision, source)
        with (source / "Cargo.toml").open("rb") as manifest:
            workspace_version = tomllib.load(manifest)["workspace"]["package"]["version"]
        if workspace_version != version:
            raise RuntimeError(
                f"source revision version {workspace_version!r} does not match {version!r}"
            )
        config = _run(
            ["cargo", "vendor", "--locked", "--versioned-dirs", "vendor"], cwd=source
        )
        cargo_config = source / ".cargo" / "config.toml"
        cargo_config.parent.mkdir(parents=True, exist_ok=True)
        cargo_config.write_bytes(config.stdout)

        bundle_path = output / f"Breeze-{version}-source.tar.gz"
        with tarfile.open(bundle_path, mode="w:gz", compresslevel=6) as bundle:
            bundle.add(source, arcname=source.name)

    with bundle_path.open("rb") as source_file:
        digest = hashlib.file_digest(source_file, "sha256").hexdigest()
    checksum_path = bundle_path.with_suffix(bundle_path.suffix + ".sha256")
    checksum_path.write_text(f"{digest}  {bundle_path.name}\n", encoding="utf-8")
    return bundle_path, checksum_path


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--version", required=True)
    parser.add_argument("--revision", required=True)
    parser.add_argument("--output", type=Path, default=ROOT / "target/dist")
    args = parser.parse_args()
    try:
        for artifact in create_source_bundle(args.version, args.revision, args.output):
            print(artifact)
    except (OSError, RuntimeError, tarfile.TarError) as error:
        print(f"source bundle: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
