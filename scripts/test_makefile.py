"""Check Make command contracts without compiling, launching, or deleting data."""

from pathlib import Path
import shutil
import subprocess
import unittest

ROOT = Path(__file__).resolve().parent.parent
MAKE = shutil.which("make")


@unittest.skipUnless(MAKE and shutil.which("sh"), "GNU Make and a POSIX shell are required")
class MakefileTests(unittest.TestCase):
    def dry_run(self, target, *overrides):
        return subprocess.run(
            [MAKE, "--no-print-directory", "--dry-run", target, "CARGO=cargo",
             "PYTHON=python3", "TARGET=aarch64-apple-darwin", "VERSION=1.2.3",
             "CARGO_TARGET_DIR=target", *overrides],
            cwd=ROOT, capture_output=True, text=True, check=True,
        ).stdout

    def test_verification_commands_are_locked_and_cover_workspace(self):
        commands = self.dry_run("verify")
        for expected in (
            "cargo fmt --all -- --check",
            "cargo check --workspace --all-targets --locked",
            "cargo clippy --workspace --all-targets --locked -- -D warnings",
            "cargo test --workspace --locked",
            "python3 -B -m unittest discover",
        ):
            self.assertIn(expected, commands)

    def test_build_and_release_profiles_are_distinct(self):
        self.assertIn("cargo build --workspace --locked", self.dry_run("build"))
        commands = self.dry_run("release")
        self.assertIn('cargo build --release --locked -p breeze-app --target "aarch64-apple-darwin"', commands)
        self.assertNotIn("git ", commands)
        self.assertNotIn("gh ", commands)

    def test_package_uses_target_directory_and_windows_extension(self):
        commands = self.dry_run(
            "package", "TARGET=x86_64-pc-windows-msvc", "CARGO_TARGET_DIR=target/custom"
        )
        self.assertIn('--version "1.2.3"', commands)
        self.assertIn('--binary "target/custom/x86_64-pc-windows-msvc/release/breeze-app.exe"', commands)
        self.assertLess(commands.index("cargo build"), commands.index("scripts/release.py package"))
        self.assertNotIn("git ", commands)
        self.assertNotIn("gh ", commands)

    def test_run_preserves_isolated_state_and_document_arguments(self):
        commands = self.dry_run("run", 'ARGS="/tmp/document with spaces.pdf"')
        self.assertIn('/target/manual-state" cargo run --locked -p breeze-app --', commands)
        self.assertIn('"/tmp/document with spaces.pdf"', commands)
        overridden = self.dry_run("run", "BREEZE_DATA_DIR=target/another review")
        self.assertIn('BREEZE_DATA_DIR="target/another review"', overridden)

    def test_clean_and_format_are_explicit_only(self):
        self.assertEqual(self.dry_run("clean").strip(), "cargo clean")
        self.assertEqual(self.dry_run("fmt").strip(), "cargo fmt --all")
        self.assertNotIn("cargo clean", self.dry_run("verify"))


if __name__ == "__main__":
    unittest.main()
