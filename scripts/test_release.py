"""Release tooling tests; all generated fixtures stay under ignored target/."""

import hashlib
from pathlib import Path
import plistlib
import stat
import subprocess
import sys
import tarfile
import tempfile
import tomllib
import unittest
import zipfile

import release


class ReleaseTests(unittest.TestCase):
    def setUp(self):
        scratch = release.ROOT / "target/release-tests"
        scratch.mkdir(parents=True, exist_ok=True)
        self.temp = tempfile.TemporaryDirectory(dir=scratch)
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.manifest = self.root / "Cargo.toml"

    def metadata(self, tag, version):
        self.manifest.write_text(
            f'[workspace.package]\nversion = "{version}"\n', encoding="utf-8"
        )
        return release.tag_metadata(tag, self.manifest)

    def test_valid_semver_and_prerelease_classification(self):
        for version, prerelease in (
            ("0.1.0", "false"),
            ("1.2.3+build.001", "false"),
            ("10.20.30-rc.1", "true"),
            ("1.0.0-0.3.7", "true"),
            ("1.0.0-01a.0-x+build.001", "true"),
            ("1.0.0-alpha-beta.11+sha.abcdef", "true"),
        ):
            for prefix in ("", "v"):
                with self.subTest(tag=prefix + version):
                    self.assertEqual(
                        self.metadata(prefix + version, version),
                        {"valid": "true", "version": version, "prerelease": prerelease},
                    )

    def test_invalid_tags_are_skipped_without_reading_manifest(self):
        for tag in (
            "", "main", "release-1.2.3", "V1.2.3", "vv1.2.3", "1.2", "1.2.3.4",
            "01.2.3", "1.02.3", "1.2.03", "1.2.3-01", "1.2.3-rc.01",
            "1.2.3-", "1.2.3+", "1.2.3-rc..1", "1.2.3+a..b", "1.2.3-α",
            " 1.2.3", "1.2.3\n", "1.2.3/evil", "1.2.3+$(echo bad)",
        ):
            with self.subTest(tag=tag):
                self.assertEqual(release.tag_metadata(tag, self.manifest), {"valid": "false"})

    def test_version_mismatch_fails_including_suffixes(self):
        for tag in ("v1.2.4", "v1.2.3-rc.1", "v1.2.3+build.1"):
            with self.subTest(tag=tag), self.assertRaisesRegex(ValueError, "does not match"):
                self.metadata(tag, "1.2.3")

    def test_github_output_skips_invalid_tag(self):
        output = self.root / "github-output"
        result = subprocess.run(
            [sys.executable, str(release.ROOT / "scripts/release.py"), "validate",
             "--tag", "not-a-version", "--github-output", str(output)],
            capture_output=True, text=True, check=False,
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(output.read_text(), "valid=false\n")

    def test_github_output_accepts_workspace_version(self):
        with (release.ROOT / "Cargo.toml").open("rb") as source:
            version = tomllib.load(source)["workspace"]["package"]["version"]
        output = self.root / "github-output"
        result = subprocess.run(
            [sys.executable, str(release.ROOT / "scripts/release.py"), "validate",
             "--tag", "v" + version, "--github-output", str(output)],
            capture_output=True, text=True, check=False,
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        expected = release.tag_metadata("v" + version, release.ROOT / "Cargo.toml")
        self.assertEqual(output.read_text(), "".join(f"{k}={v}\n" for k, v in expected.items()))

    def test_cli_mismatch_fails_without_writing_outputs(self):
        with (release.ROOT / "Cargo.toml").open("rb") as source:
            version = tomllib.load(source)["workspace"]["package"]["version"]
        major = int(version.split(".")[0]) + 1
        output = self.root / "github-output"
        result = subprocess.run(
            [sys.executable, str(release.ROOT / "scripts/release.py"), "validate",
             "--tag", f"v{major}.0.0", "--github-output", str(output)],
            capture_output=True, text=True, check=False,
        )
        self.assertEqual(result.returncode, 1)
        self.assertIn("does not match", result.stderr)
        self.assertFalse(output.exists())

    def test_packages_include_binary_versions_notices_and_checksums(self):
        binary = self.root / "breeze-app"
        binary.write_bytes(b"test executable\n")
        (self.root / "runtime.dll").write_bytes(b"test runtime\n")
        output = self.root / "dist"
        for target in release.TARGETS:
            with self.subTest(target=target):
                version = "1.2.3-rc.1+build.2"
                archive = release.package(version, target, binary, output)
                prefix = f"Breeze-{version}-{target}/"
                if target.endswith("linux-gnu"):
                    with tarfile.open(archive) as bundle:
                        files = {m.name: bundle.extractfile(m).read()
                                 for m in bundle.getmembers() if m.isfile()}
                        mode = bundle.getmember(prefix + "breeze-app").mode
                    executable = "breeze-app"
                else:
                    with zipfile.ZipFile(archive) as bundle:
                        files = {name: bundle.read(name) for name in bundle.namelist()}
                        executable = ("Breeze.app/Contents/MacOS/Breeze"
                                      if target.endswith("apple-darwin") else "breeze-app.exe")
                        mode = bundle.getinfo(prefix + executable).external_attr >> 16
                    if target.endswith("apple-darwin"):
                        plist = plistlib.loads(files[prefix + "Breeze.app/Contents/Info.plist"])
                        self.assertEqual(plist["CFBundleShortVersionString"], "1.2.3")
                        self.assertEqual(plist["CFBundleVersion"], "1.2.3")
                        self.assertEqual(plist["BreezeReleaseVersion"], version)
                        self.assertIn(prefix + "Breeze.app/Contents/Resources/Breeze.icns", files)
                    else:
                        self.assertEqual(files[prefix + "runtime.dll"], b"test runtime\n")
                if sys.platform != "win32":
                    self.assertTrue(mode & stat.S_IXUSR)
                self.assertEqual(files[prefix + executable], binary.read_bytes())
                self.assertEqual(files[prefix + "VERSION"], (version + "\n").encode())
                for name in ("README.md", "Cargo.lock", "RELEASE-NOTES.md", "licenses/LICENSE-LUCIDE.txt"):
                    self.assertIn(prefix + name, files)
                expected = hashlib.sha256(archive.read_bytes()).hexdigest()
                self.assertEqual(archive.with_name(archive.name + ".sha256").read_text(),
                                 f"{expected}  {archive.name}\n")
        self.assertEqual(len(list(output.iterdir())), 8)
        self.assertEqual(binary.read_bytes(), b"test executable\n")

    def test_packaging_rejects_invalid_inputs_before_writing(self):
        output = self.root / "dist"
        binary = self.root / "missing"
        for version, target in (("../evil", release.TARGETS[0]), ("1.2.3", "unknown"),
                                ("1.2.3", release.TARGETS[0])):
            with self.subTest(version=version, target=target), self.assertRaises(ValueError):
                release.package(version, target, binary, output)
        self.assertFalse(output.exists())


if __name__ == "__main__":
    unittest.main()
