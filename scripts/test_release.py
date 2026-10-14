"""Release tooling tests; all generated fixtures stay under ignored target/."""

import hashlib
from pathlib import Path
import plistlib
import shutil
import stat
import subprocess
import sys
import tempfile
import tomllib
import unittest
import xml.etree.ElementTree as ET

import release

WIX = {"w": "http://schemas.microsoft.com/wix/2006/wi"}
APPLE = "aarch64-apple-darwin"
WINDOWS = "x86_64-pc-windows-msvc"
LINUX = "x86_64-unknown-linux-gnu"
DOCUMENTS = ("VERSION", "Cargo.lock", "README.md", "RELEASE-NOTES.md")


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

    def payload(self, target, version="1.2.3-rc.1+build.2"):
        """Build an installer payload with a stand-in binary and runtime DLL."""
        binary = self.root / "breeze-app"
        binary.write_bytes(b"test executable\n")
        (self.root / "runtime.dll").write_bytes(b"test runtime\n")
        staging = self.root / f"stage-{target}"
        staging.mkdir(exist_ok=True)
        return release.build_payload(version, target, binary, staging), binary

    def files_of(self, payload):
        return sorted(
            path.relative_to(payload).as_posix()
            for path in payload.rglob("*")
            if path.is_file()
        )

    # Tag validation ---------------------------------------------------------

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

    # Installer formats ------------------------------------------------------

    def test_every_target_maps_to_a_known_format(self):
        self.assertEqual(
            set(release.FORMATS),
            {APPLE, "x86_64-apple-darwin", WINDOWS, LINUX},
        )
        for target, formats in release.FORMATS.items():
            with self.subTest(target=target):
                self.assertTrue(formats)
                for name in formats:
                    self.assertIn(name, release.BUILDERS)

    def test_payloads_carry_binary_versions_and_notices(self):
        version = "1.2.3-rc.1+build.2"
        for target in release.TARGETS:
            with self.subTest(target=target):
                payload, binary = self.payload(target, version)
                files = self.files_of(payload)
                self.assertFalse(
                    [name for name in files if release._is_metadata(Path(name).name)],
                    "installer payloads must not carry OS metadata",
                )
                if target.endswith("apple-darwin"):
                    self.assertEqual(payload.name, "Breeze.app")
                    executable = payload / "Contents/MacOS/Breeze"
                    with (payload / "Contents/Info.plist").open("rb") as source:
                        plist = plistlib.load(source)
                    self.assertEqual(plist["CFBundleShortVersionString"], "1.2.3")
                    self.assertEqual(plist["CFBundleVersion"], "1.2.3")
                    self.assertEqual(plist["BreezeReleaseVersion"], version)
                    self.assertEqual(plist["CFBundleExecutable"], "Breeze")
                    self.assertEqual(plist["CFBundleIconFile"], "Breeze.icns")
                    self.assertEqual(plist["LSMinimumSystemVersion"], "15.0")
                    self.assertIn("Contents/Resources/Breeze.icns", files)
                    documents = "Contents/Resources"
                elif target.endswith("windows-msvc"):
                    executable = payload / "breeze-app.exe"
                    self.assertEqual((payload / "runtime.dll").read_bytes(), b"test runtime\n")
                    documents = "."
                else:
                    executable = payload / "usr/bin/breeze-app"
                    self.assertEqual(
                        (payload / "usr/share/applications/breeze.desktop").read_text(),
                        release.desktop_entry(),
                    )
                    self.assertIn(
                        "usr/share/icons/hicolor/256x256/apps/breeze.png", files
                    )
                    documents = "usr/share/doc/breeze"
                self.assertEqual(executable.read_bytes(), binary.read_bytes())
                if sys.platform == "win32":
                    # Windows marks executability by extension, so only the .exe
                    # payload reports an execute bit there.
                    if target.endswith("windows-msvc"):
                        self.assertTrue(executable.stat().st_mode & stat.S_IXUSR)
                else:
                    self.assertTrue(executable.stat().st_mode & stat.S_IXUSR)
                for name in DOCUMENTS:
                    self.assertIn(f"{documents}/{name}".lstrip("./"), files)
                self.assertIn(f"{documents}/licenses/LICENSE-LUCIDE.txt".lstrip("./"), files)
                self.assertEqual(
                    (payload / documents / "VERSION").read_text(encoding="utf-8"),
                    version + "\n",
                )

    def test_generated_windows_sources_are_well_formed_and_stable(self):
        version = "1.2.3-rc.1"
        payload, _ = self.payload(WINDOWS, version)
        icon = release.ROOT / "assets/brand/breeze.ico"
        source = release.wix_source(version, WINDOWS, payload, icon)
        self.assertEqual(source, release.wix_source(version, WINDOWS, payload, icon))
        root = ET.fromstring(source)
        product = root.find(".//w:Product", WIX)
        # Windows Installer versions are numeric, so the suffix moves to a property.
        self.assertEqual(product.get("Version"), "1.2.3")
        self.assertEqual(product.get("Manufacturer"), release.PUBLISHER)
        self.assertEqual(
            root.find(".//w:Property[@Id='BreezeReleaseVersion']", WIX).get("Value"),
            version,
        )
        install = root.find(".//w:Directory[@Id='INSTALLFOLDER']", WIX)
        self.assertEqual(install.get("Name"), "Breeze")
        installed = {
            Path(file.get("Source")).name for file in root.findall(".//w:File", WIX)
        }
        self.assertLessEqual(
            {Path(name).name for name in self.files_of(payload)}, installed
        )
        self.assertEqual(
            len(root.findall(".//w:ComponentRef", WIX)),
            len(root.findall(".//w:Component", WIX)),
            "every component must be referenced by a feature",
        )
        shortcut = root.find(".//w:Shortcut", WIX)
        self.assertEqual(shortcut.get("Target"), "[INSTALLFOLDER]breeze-app.exe")

        script = release.inno_script(version, WINDOWS, payload, icon, self.root)
        self.assertIn("AppId={{", script, "Inno needs an escaped literal brace")
        self.assertIn("AppVersion=1.2.3-rc.1", script)
        self.assertIn("VersionInfoVersion=1.2.3", script)
        self.assertIn("DefaultDirName={autopf}\\Breeze", script)
        self.assertIn("ArchitecturesInstallIn64BitMode=x64compatible", script)
        self.assertIn("SetupIconFile=" + str(icon), script)
        self.assertIn("[Files]", script)
        self.assertIn("[Icons]", script)

    def test_debian_metadata_follows_policy(self):
        payload, _ = self.payload(LINUX, "1.2.3-rc.1")
        control = release.debian_control("1.2.3-rc.1", payload)
        fields = dict(
            line.split(": ", 1) for line in control.splitlines() if ": " in line
        )
        self.assertEqual(fields["Package"], "breeze")
        # Debian sorts `~` before the empty string, so the prerelease stays below
        # the matching stable release, as SemVer requires.
        self.assertEqual(fields["Version"], "1.2.3~rc.1-1")
        self.assertEqual(fields["Architecture"], "amd64")
        self.assertGreater(int(fields["Installed-Size"]), 0)
        self.assertIn("libc6", fields["Depends"])
        self.assertEqual(release._debian_version("1.2.3"), "1.2.3-1")

    def test_appimage_payload_has_entry_point_desktop_file_and_icon(self):
        payload, _ = self.payload(LINUX, "1.2.3")
        desktop = release.desktop_entry()
        self.assertIn("Exec=breeze-app %F", desktop)
        self.assertIn("Icon=breeze", desktop)
        self.assertNotIn("MimeType", desktop, "file associations are unimplemented")
        self.assertTrue((payload / "usr/bin/breeze-app").is_file())
        self.assertTrue((payload / "usr/share/icons/hicolor/256x256/apps/breeze.png").is_file())

    def test_packaging_rejects_invalid_inputs_before_writing(self):
        output = self.root / "dist"
        binary = self.root / "missing"
        for version, target in (("../evil", release.TARGETS[0]), ("1.2.3", "unknown"),
                                ("1.2.3", release.TARGETS[0])):
            with self.subTest(version=version, target=target), self.assertRaises(ValueError):
                release.package(version, target, binary, output)
        self.assertFalse(output.exists())

    # Real containers, where this platform can build them --------------------

    @unittest.skipUnless(shutil.which("dpkg-deb"), "dpkg-deb is Linux-only")
    def test_debian_package_builds_with_expected_contents(self):
        version = "1.2.3-rc.1"
        payload, _ = self.payload(LINUX, version)
        output = self.root / "deb"
        output.mkdir()
        artifact = release.BUILDERS["deb"](version, LINUX, payload, output)
        release._checksum(artifact)
        digest = hashlib.sha256(artifact.read_bytes()).hexdigest()
        self.assertEqual(
            artifact.with_name(artifact.name + ".sha256").read_text(encoding="utf-8"),
            f"{digest}  {artifact.name}\n",
        )
        fields = subprocess.run(
            ["dpkg-deb", "--field", str(artifact)], capture_output=True, text=True, check=True
        ).stdout
        self.assertIn("Package: breeze", fields)
        self.assertIn("Version: 1.2.3~rc.1-1", fields)
        contents = subprocess.run(
            ["dpkg-deb", "--contents", str(artifact)], capture_output=True, text=True, check=True
        ).stdout
        for path in (
            "./usr/bin/breeze-app",
            "./usr/share/applications/breeze.desktop",
            "./usr/share/icons/hicolor/256x256/apps/breeze.png",
            "./usr/share/doc/breeze/VERSION",
            "./usr/share/doc/breeze/licenses/LICENSE-LUCIDE.txt",
        ):
            self.assertIn(path, contents)

    @unittest.skipUnless(shutil.which("hdiutil"), "hdiutil is macOS-only")
    def test_disk_image_builds_and_mounts(self):
        version = "1.2.3-rc.1"
        payload, _ = self.payload(APPLE, version)
        output = self.root / "dmg"
        output.mkdir()
        artifact = release.BUILDERS["dmg"](version, APPLE, payload, output)
        release._checksum(artifact)
        digest = hashlib.sha256(artifact.read_bytes()).hexdigest()
        self.assertEqual(
            artifact.with_name(artifact.name + ".sha256").read_text(encoding="utf-8"),
            f"{digest}  {artifact.name}\n",
        )
        mount = self.root / "mount"
        mount.mkdir()
        subprocess.run(
            ["hdiutil", "attach", "-nobrowse", "-readonly", "-mountpoint", str(mount), str(artifact)],
            capture_output=True, text=True, check=True,
        )
        try:
            self.assertTrue((mount / "Breeze.app/Contents/MacOS/Breeze").is_file())
            self.assertTrue((mount / "Breeze.app/Contents/Resources/VERSION").is_file())
            self.assertTrue((mount / "Breeze.app/Contents/Resources/Breeze.icns").is_file())
            self.assertTrue((mount / "Applications").is_symlink())
            self.assertEqual(
                [
                    path.name
                    for path in mount.rglob("*")
                    if release._is_metadata(path.name)
                ],
                [],
            )
        finally:
            subprocess.run(
                ["hdiutil", "detach", str(mount)], capture_output=True, text=True, check=True
            )


if __name__ == "__main__":
    unittest.main()
