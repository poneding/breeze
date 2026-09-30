# Release automation

## Scope and limitations

Tag pushes build **unsigned development distributions**, not production-accepted releases. macOS bundles are not Developer ID signed or notarized; platform security policies may block them. Windows/Linux native runtime acceptance, the complete dependency-license audit, and the rendering/fidelity/performance gates in `docs/plans/04-release.md` remain pending. A stable SemVer tag does not waive those gates. There is no installer, automatic update feed, or file-association registration.

| Runner | Rust target | Download |
|---|---|---|
| macOS 15 (Apple Silicon) | `aarch64-apple-darwin` | ZIP containing `Breeze.app` |
| macOS 15 (Intel) | `x86_64-apple-darwin` | ZIP containing `Breeze.app` |
| Windows Server 2022 | `x86_64-pc-windows-msvc` | ZIP containing `breeze-app.exe` |
| Ubuntu 24.04 | `x86_64-unknown-linux-gnu` | tar.gz containing `breeze-app` |

macOS requires 15+. Windows 10+ is the intended baseline, with the Microsoft Visual C++ x64 runtime potentially required; an archive is not a standalone installer. Linux uses the Ubuntu 24.04/glibc baseline, the system libraries listed in `.github/workflows/ci.yml`, and a graphical Wayland/X11 session with a working Vulkan driver. These are build baselines, not claims of native runtime validation. Linux libraries and Windows system runtimes are not bundled.

Each archive includes `VERSION`, `README.md`, these release notes, `Cargo.lock`, and the repository's maintained third-party notices under `licenses/`. The PDF sample, Kit icons, and macOS shaders are embedded in the executable. Adjacent Windows DLLs emitted by the build are copied if present. The included notices are not yet a complete audited dependency-license inventory.

## Publish a version

1. Set `[workspace.package].version` in `Cargo.toml` to the intended SemVer, and update the workspace package versions in `Cargo.lock` with Cargo. Commit both files with the release changes after verification.
2. Create a tag whose version **exactly matches** the manifest (including any prerelease/build suffix), then push that tag to the GitHub repository:

   ```sh
   git tag -a v0.1.0 -m "Release 0.1.0"
   git push origin v0.1.0
   ```

   This is an operator action; local build/package commands never create or push tags. Configure the GitHub remote first if none exists.
3. The `Release` workflow accepts SemVer 2.0.0 with an optional lowercase `v`: `v1.2.3`, `1.2.3-rc.1`, and `v1.2.3+build.7`, for example. Non-SemVer tags are skipped; numeric leading zeroes, empty identifiers, and extra prefixes are rejected. A valid tag with a mismatched manifest version **fails**, rather than shipping misleading version metadata.
4. Every matrix build must pass formatting, workspace tests, Clippy, release-tool tests, compilation, and packaging before the publication job runs. Only that final job gets `contents: write` through the built-in `GITHUB_TOKEN`; no personal access token is needed. Repository/organization Actions policy must permit this permission.
5. After all four archives and their SHA-256 checksums are downloaded and verified, the workflow creates a draft with generated release notes, uploads the files, then publishes it. Versions with a `-prerelease` component become GitHub prereleases; build metadata alone does not. A failed upload leaves a draft for a workflow rerun. An already-published release is never overwritten by a rerun; create a new version for corrections.

Do not move an existing release tag. Tag/release protection rules should restrict who can initiate publication. If a build fails, no release is published. Build failures and runner limitations must be investigated, not hidden with a different document engine or lower quality gate.

## Verify and use downloads

Keep each archive beside its `.sha256` file. On macOS run `shasum -a 256 -c <archive>.sha256`; on Linux use `sha256sum --check <archive>.sha256`. On Windows compare `Get-FileHash <archive> -Algorithm SHA256` with the accompanying checksum. Checksums detect corruption; they are not signatures or proof of publisher identity.

Extract the complete archive. On macOS open `Breeze.app`; on Windows run `breeze-app.exe`; on Linux run `./breeze-app`. Use an isolated `BREEZE_DATA_DIR` when reviewing a build. Do not treat a successful download or launch as production acceptance.

## Reproduce packaging locally

Release tooling uses Python **3.11+**, standard library only. Python is a development/CI tool, not an application runtime dependency. Build for the native Rust target before invoking the packager; it does not compile the application or validate cross-platform executables:

```sh
python3 -B -m unittest discover -s scripts -p 'test_*.py' -v
python3 scripts/release.py validate --tag v0.1.0
MACOSX_DEPLOYMENT_TARGET=15.0 cargo build --locked --release -p breeze-app --target aarch64-apple-darwin
python3 scripts/release.py package --version 0.1.0 --target aarch64-apple-darwin \
  --binary target/aarch64-apple-darwin/release/breeze-app
```

Archives and checksums are written under ignored `target/dist/`. The existing `scripts/bundle-macos.sh` remains a separate local **debug** review helper; it is not used to generate release assets.
