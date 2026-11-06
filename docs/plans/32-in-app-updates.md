# In-app update checks, banner, installation, and restart

Requested on October 30, 2026:

1. Every launch checks for updates on an asynchronous background path.
2. A new release must appear in a prominent banner that reads as a reminder.
3. The application must offer the update directly, and after installation ask
   the reader whether to restart.

The reader chose three-platform automatic installation, a startup check that is
on by default, and the theme's blue accent for the banner. This package replaces
the earlier read-only behavior described in [work package
17](17-language-and-updates.md), which only opened the release page.

## Implemented behavior

### Startup check

`initialize_storage` starts one background check on every launch path, including
storage failures that skip state restoration. It runs after the stored update
policy is restored, so the check uses the preference this session will actually
persist. Settings keeps Manual and Automatic; Automatic (index 1) is now the
fresh-install default. Session-state schema version 12 migrates a stored
pre-12 default of Manual to Automatic once, because the old value is identical
to a deliberate Manual choice. Choosing Manual in version 12 and later no longer
checks automatically. Explicit checks in About remain available under every
policy. Only one automatic check runs per process.

The request stays bounded: fixed GitHub release page, 15-second total
deadline, no redirects followed, fixed repository URLs, and SemVer validation
before any newer release is offered. The release must publish both the platform
package and its checksum; otherwise the banner only links to the release page.

Discovery does not use the GitHub REST API. `api.github.com` allows 60
unauthenticated requests per hour per source address, so the budget belongs to
whatever egress the reader shares — a proxy, VPN, carrier NAT or office network
— and a client that sends one request per launch can still be refused through no
fault of its own. Instead:

- `HEAD https://github.com/poneding/breeze/releases/latest` answers `302` with a
  `Location` of `/releases/tag/<tag>` for the newest published stable release, or
  a redirect to the release list when none exists. Only the tag after
  `/releases/tag/` is used, and it must parse as a stable SemVer version.
- `HEAD .../releases/download/<tag>/<asset>` answers `302` for a published asset
  and `404` for a missing one, which confirms the package and its checksum
  without reading release metadata.

Neither endpoint spends the API budget, no response body is read, and the check
issues at most three bodyless requests: one for the tag and two more only when a
newer release exists and this build has a package to offer. An unexpected status
there still fails the check instead of claiming the package is missing.

### Banner

`updates.rs` renders a full-width band below the title bar and any storage
notice, above the document or Home content. It uses the active theme's base blue
(`theme.blue`) at 18% for the background, 60% for the bottom border, and for the
download icon, with the normal text colors for the message. The band carries the
release message and actions:

- Update now (primary) while the package is available;
- Restart now after installation;
- Retry update after a failed download or install;
- Show downloaded file for a package-manager file;
- View release, and a dismiss button.

The banner is suppressed on non-library Home pages because About already shows
the same release state and actions. Immersive fullscreen reading remains free of
all chrome. A dismissed banner stays hidden until the next check. Split-pane
peers share the window but not the state store, so only the primary pane offers
installation or restart; peers can still open the release page.

### Download and verification

`updates/install.rs` is the only updater code that writes application files.

- Asset names are derived from the validated version and this build's target:
  `Breeze-<version>-<target>.dmg/.exe/.msi/.deb/.AppImage` and the matching
  `.sha256`. The check confirms both on the download path before offering an
  install; no URL from a response body is used.
- The checksum sidecar is fetched first (4 KiB limit), then the package is
  streamed to `<data>/updates` while SHA-256 is computed. The HTTPS-only client
  follows redirects only to other HTTPS URLs, caps the package at 512 MiB, and
  uses a 15-minute body budget. A mismatch deletes the download and leaves the
  installation untouched.
- The byte counter refreshes the banner every 200 ms and stops as soon as the
  download phase ends.

### Platform installation

| Platform | Detected installation | Install action | Restart action |
|---|---|---|---|
| macOS | `.app/Contents/MacOS` bundle | mount DMG read-only, verify `Resources/VERSION`, `ditto` into a staged bundle beside the installed one, rename old → new, remove the old copy | `open -n <bundle>` |
| Windows | Inno (`unins000.exe`) or MSI (`VERSION` beside the executable) | download the matching `.exe` or `.msi` and keep it staged | the setup starts as Breeze closes; UAC asks for permission |
| Linux AppImage | `APPIMAGE` environment path | copy to a sibling `.new` file, mark executable, atomic rename over the image, remove the backup | launch the replaced AppImage |
| Linux `.deb` | `/usr/share/doc/breeze/VERSION` | download only; reveal the file | none; the package manager owns installation |
| unsupported | development binary or no matching package | none | none; only View release |

macOS installs are verified twice: the published checksum, then the mounted
bundle's `VERSION` must equal the offered release. A mismatch or a failed rename
restores/retains the installed bundle and reports the failure. The macOS install
path is exercised by a native test that creates a real DMG with `hdiutil` and
performs the bundle swap in a temporary directory.

Windows cannot replace a running image. The setup package is started as Breeze
closes and performs the replacement after the process exits. Declining UAC leaves the old installation; the next launch offers the
update again. `.deb` installations require root tools and are intentionally not
elevated by Breeze.

### Restart prompt

When an install finishes, the primary pane opens the Kit `AlertDialog` titled
“Update ready” with the installed/downloaded version, “Restart now”, and
“Later”. It is shown once per installation. Dismissing it leaves the banner and
About entries so the reader can restart later. Restarting goes through the
normal close path: local state is saved, and unsaved annotations or drafts block
the restart instead of being discarded. For macOS and AppImage updates, restart
spawns a detached helper that waits for this process to exit before launching the
new instance, so the replacement can acquire the state-store lock and restore the
saved session. The Windows installer starts as Breeze closes. A helper spawn or
path-validation failure keeps the window open and shows the diagnostic in the
banner; a failure inside the detached launcher is outside the application's
control.

### Localization

All new visible strings, including download, verification, install, and restart
failures, are in the three-language catalog. System-provided diagnostics are
reduced to a bounded single-line detail and never include remote response
bodies.

## Security boundaries

The update package is authenticated only by the SHA-256 checksum published in the
same release. That detects damaged or substituted downloads but is not publisher
authentication. Release installers remain unsigned and not notarized or
code-signed, so the updater must not be described as a verified or trusted
channel until signing/notarization and an audited update-authenticity mechanism
are added. HTTPS, the fixed repository, deterministic asset names, version
verification on macOS, and no response-supplied URLs are integrity and
scope controls, not a substitute for signing.

Breeze does not install system packages with elevated privileges itself, does not
silently relaunch, and does not download in the background without the reader
starting the update.

## Verification

- `cargo fmt --all -- --check` and
  `cargo clippy --workspace --all-targets --locked -- -D warnings` pass.
- `cargo test --workspace --locked` passes: 276 application tests plus all crate
  and integration suites. New tests cover install-plan asset names, confirmation
  of both assets on the download path, SHA-256 parsing, a loopback download with
  a matching checksum, mismatched/missing checksums, banner/About entries, the
  single restart dialog, state-version migration, and the macOS DMG install and
  version-rejection paths.
- Native macOS startup with an isolated `BREEZE_DATA_DIR` used the real GitHub
  endpoint and About reported “当前版本无需更新 · 最新正式版本 0.1.0”, proving the
  startup check ran without a manual action.
- A temporary local preview of `v0.2.0` (reverted before the commit) was
  inspected on macOS. OCR of the window confirmed the blue banner “发现新版本
  0.2.0”, “立即更新”, “查看发布版本”, the About action row, and the modal “更新已就绪”
  with “稍后” / “立即重启”. Pixel sampling measured the banner tint as
  `(0.876, 0.914, 0.985)` against the white content below.
- Windows and Linux installation were not run natively. Their detection,
  download selection, install action, and restart handoff compile on those
  targets but remain runtime-unverified until native acceptance.

## Known limits

- Packages are unsigned; production updates require signing/notarization and an
  update-authenticity decision.
- Release discovery depends on GitHub's release-page redirect. A mirror or an
  offline environment has no configured alternative, and a repository with no
  stable release redirects to the release list, which the check reports as no
  public stable release rather than as a failure.
- Windows needs UAC and the installer runs after Breeze exits.
- macOS cannot replace a bundle launched from a read-only image, a translocated
  location, or a directory the user cannot write; the banner reports the system
  error.
- Package-manager installations (`.deb`) cannot be completed in-app.
- There is no delta download, background download, staged rollout, or rollback.
