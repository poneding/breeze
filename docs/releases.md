# Release automation

## Scope and limitations

Tag pushes build **unsigned development installers**, not production-accepted
releases. Nothing is code-signed or notarized, so macOS Gatekeeper and Windows
SmartScreen warn before installation, and the Linux packages are only exercised
against the Ubuntu 24.04 build baseline. Native Windows/Linux runtime
acceptance, the complete dependency-license audit, and the rendering/fidelity/
performance gates in `docs/plans/04-release.md` remain pending. A stable SemVer
tag does not waive those gates. There is no automatic update feed or
file-association registration, and the Linux desktop entry deliberately declares
no MIME types.

| Runner | Rust target | Package | Installed layout |
|---|---|---|---|
| macOS 15 (Apple Silicon) | `aarch64-apple-darwin` | `.dmg` | `Breeze.app` beside an `/Applications` link |
| macOS 15 (Intel) | `x86_64-apple-darwin` | `.dmg` | same |
| Windows Server 2022 | `x86_64-pc-windows-msvc` | `.msi` (WiX), `.exe` (Inno Setup) | `breeze-app.exe` and its DLLs under `%ProgramFiles%\Breeze` |
| Ubuntu 24.04 | `x86_64-unknown-linux-gnu` | `.deb`, `.AppImage` | `/usr/bin/breeze-app`, desktop entry, and icon |

macOS requires 15+. Windows 10+ is the intended baseline for the MSI and the
setup executable. Linux uses the Ubuntu 24.04/glibc baseline, the system
libraries listed in `.github/workflows/ci.yml`, and a graphical Wayland/X11
session with a working Vulkan driver. These are build baselines, not claims of
native runtime validation. Linux libraries and Windows system runtimes are not
bundled.

Every installer carries `VERSION`, `Cargo.lock`, `README.md`, these release notes
as `RELEASE-NOTES.md`, and the repository's maintained third-party notices under
`licenses/`: inside `Breeze.app/Contents/Resources` on macOS, beside the
executable on Windows, and under `/usr/share/doc/breeze` on Linux. The PDF
sample, Kit icons, and macOS shaders are embedded in the executable. The included
notices are not yet a complete audited dependency-license inventory.

Two platform conventions are worth knowing. The Debian package maps a SemVer
prerelease onto Debian's `~` (`0.1.0~alpha.1-1`) so that prereleases sort *below*
the matching stable version, and it declares the libraries the Ubuntu 24.04 CI
build links against — that list is not an audited runtime inventory. The MSI and
the setup executable store the numeric SemVer core in the Windows version fields
and carry the full version in a property and in the uninstall entry, because
those fields cannot hold a suffix.

## Publish a version

1. Set `[workspace.package].version` in `Cargo.toml` to the intended SemVer, and
   update the workspace package versions in `Cargo.lock` with Cargo. Commit both
   files with the release changes after verification.
2. Create a tag whose version **exactly matches** the manifest (including any
   prerelease/build suffix), then push that tag to the GitHub repository:

   ```sh
   git tag -a v0.1.0 -m "Release 0.1.0"
   git push origin v0.1.0
   ```

   This is an operator action; local build/package commands never create or push
   tags. Configure the GitHub remote first if none exists.
3. The `Release` workflow accepts SemVer 2.0.0 with an optional lowercase `v`:
   `v1.2.3`, `1.2.3-rc.1`, and `v1.2.3+build.7`, for example. Non-SemVer tags
   are skipped; numeric leading zeroes, empty identifiers, and extra prefixes are
   rejected. A valid tag with a mismatched manifest version **fails**, rather
   than shipping misleading version metadata.
4. Every matrix build must pass formatting, workspace tests, Clippy,
   release-tool tests, compilation, packaging, and the per-platform installer
   checks before the publication job runs. Those checks mount the disk image and
   read its bundle, list the Debian package contents, read the MSI database and
   extract its payload with an administrative install, and confirm the AppImage
   type-2 container magic. Nothing installs or launches an application, so
   installation and launch remain unverified. Only the
   final job gets `contents: write` through the built-in `GITHUB_TOKEN`; no
   personal access token is needed. Repository/organization Actions policy must
   permit this permission.
5. After all installers and their SHA-256 checksums are downloaded and verified,
   the workflow creates a draft with generated release notes, uploads the files,
   then publishes it. Versions with a `-prerelease` component become GitHub
   prereleases; build metadata alone does not. A failed upload leaves a draft for
   a workflow rerun. An already-published release is never overwritten by a
   rerun; create a new version for corrections.

Do not move an existing release tag. Tag/release protection rules should
restrict who can initiate publication. If a build fails, no release is
published. Build failures and runner limitations must be investigated, not
hidden with a different document engine or lower quality gate.

A manual `workflow_dispatch` run of the same workflow, given the workspace
version, builds and verifies every installer on all four runners **without**
publishing anything. Use it to validate packaging before creating a tag.

## Verify and use downloads

Keep each installer beside its `.sha256` file. On macOS run
`shasum -a 256 -c <installer>.sha256`; on Linux use
`sha256sum --check <installer>.sha256`. On Windows compare
`Get-FileHash <installer> -Algorithm SHA256` with the accompanying checksum.
Checksums detect corruption; they are not signatures or proof of publisher
identity.

| Platform | Install |
|---|---|
| macOS | Open the `.dmg` and drag `Breeze` onto `Applications`. Gatekeeper blocks the first launch of an unsigned bundle: right-click → Open, or `xattr -dr com.apple.quarantine /Applications/Breeze.app`. |
| Windows | Run the `.msi` or the setup `.exe`. SmartScreen reports "Windows protected your PC" → More info → Run anyway. The MSI requires Windows 10+, installs for all users, adds a Start-menu shortcut, and uninstalls from Apps & features. |
| Debian/Ubuntu | `sudo apt install ./Breeze-<version>-x86_64-unknown-linux-gnu.deb` |
| Other Linux | `chmod +x Breeze-<version>-x86_64-unknown-linux-gnu.AppImage && ./Breeze-<version>-x86_64-unknown-linux-gnu.AppImage`. AppImages need FUSE, or `--appimage-extract-and-run` where FUSE is unavailable. |

Use an isolated `BREEZE_DATA_DIR` when reviewing a build. A successful download,
installation, or launch is not production acceptance, and the Linux packages
have not been installed on a distribution other than the Ubuntu 24.04 baseline.

## Reproduce packaging locally

Release tooling uses Python **3.11+**, standard library only. Python is a
development/CI tool, not an application runtime dependency. Payload assembly runs
on any platform, but each installer needs that platform's own tooling:
`hdiutil` for the disk image, WiX 3 (`candle`, `light`) and Inno Setup 6 (`iscc`)
for the Windows installers, and `dpkg-deb` plus `appimagetool` for the Linux
packages. Build the native Rust target before packaging; the tool does not compile
the application:

```sh
python3 -B -m unittest discover -s scripts -p 'test_*.py' -v
python3 scripts/release.py validate --tag v0.1.0
MACOSX_DEPLOYMENT_TARGET=15.0 cargo build --locked --release -p breeze-app --target aarch64-apple-darwin
python3 scripts/release.py package --version 0.1.0 --target aarch64-apple-darwin \
  --binary target/aarch64-apple-darwin/release/breeze-app
```

Installers and checksums are written under ignored `target/dist/`. The test suite
covers payload assembly for all four targets everywhere, and builds the Debian
package and the disk image for real wherever `dpkg-deb` and `hdiutil` exist. The
existing `scripts/bundle-macos.sh` remains a separate local **debug** review
helper; it is not used to generate release artifacts.
