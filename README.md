# Breeze

A compact, native Rust reading application built with GPUI Kit. PDF is the first backend, not the product's format boundary.

**Current delivery:** a local reader with durable library/settings, search, glyph-based rectangular selection, and fingerprinted sidecar annotations. Restricted standard-annotation export and opening-window policies are implemented; original-content PDF editing and production acceptance remain gated. See the [implementation roadmap](docs/plans/README.md) and [delivery status](docs/plans/STATUS.md).

## Run

```sh
cargo run --locked -p breeze-app
cargo run --locked -p breeze-app -- /absolute/path/document.pdf
```

The bundled eight-page sample is available from Home. The UI currently uses Chinese strings; documentation and code comments are English.

### Development environment

- Rust 1.97 or newer; CI specifies 1.97.1.
- macOS 15+ with Xcode Command Line Tools.
- Windows 10+ with MSVC Rust, Visual Studio C++ Build Tools, Windows SDK, and CMake.
- Ubuntu 24.04 is the initial Linux baseline; install the libraries in [CI](.github/workflows/ci.yml), use a graphical Wayland/X11 session, and provide a working Vulkan driver.

Native development checks have run on an Apple M1 Pro with macOS 15.7.9. Windows/Linux runtime acceptance is still pending. No WebView, Node.js, or Python runtime is required.

## Developer commands

Use GNU Make from the repository root (`make` shows help). On Windows, run it in a POSIX environment such as Git Bash with GNU Make installed and the MSVC Rust toolchain available; the Cargo commands above remain usable directly in PowerShell.

| Command | Purpose |
|---|---|
| `make check` | Check formatting and compile all workspace targets |
| `make lint` | Run Clippy with warnings treated as errors |
| `make test` | Run Rust tests and the Python 3.11+ release-tool tests |
| `make verify` | Run check, lint, and test |
| `make build` | Build the workspace in the development profile |
| `make run` | Launch with isolated data under `target/manual-state/` |
| `make release` | Build the optimized app for the host Rust target |
| `make package` | Build the release, then write its installer/checksum to `target/dist/` |
| `make fmt` / `make fmt-check` | Apply formatting / check without editing |
| `make clean` | Remove Cargo output (including scratch data under the target directory) |

Examples:

```sh
make run ARGS='"/absolute/path/document with spaces.pdf"'
make run BREEZE_DATA_DIR="$PWD/target/another-review"
make package TARGET=aarch64-apple-darwin
# Use PYTHON=python on systems where Python 3.11+ has that executable name.
make test-tools PYTHON=python
```

`CARGO`, `RUSTC`, and `PYTHON` can override tool commands. `CARGO_TARGET_DIR` defaults to `target` and is explicitly exported (overriding any Cargo config target-dir); optimized binaries go to `<CARGO_TARGET_DIR>/<TARGET>/release/`. `TARGET` defaults to the host triple. Cross-compiling still requires the target's SDK/linker/system libraries. Packaging reads `VERSION` from `Cargo.toml` unless explicitly overridden for a local artifact, and supports only the four targets in the release workflow. macOS builds default to `MACOSX_DEPLOYMENT_TARGET=15.0`. Neither `make release` nor `make package` creates tags, pushes, or publishes anything.

## Daily workflow

1. Open a local PDF or the bundled sample. Use tabs, the outline, the full scrollable thumbnail list, page input, and fit/zoom controls to navigate.
2. Use Primary+F for document search; result buttons navigate to their page. Primary means Command on macOS and Control on Windows/Linux.
3. In Reading mode, drag a rectangle over recognized horizontal glyphs and use Primary+C to copy the selected text.
4. Switch to Annotate, select a tool, and draw. For a note, enter text in the properties panel, then place it on the page.
5. Changes autosave after a 500 ms pause to `<filename>.breeze.json`; Primary+S saves immediately. Primary+Shift+S selects a new sidecar destination. The original PDF is not modified.
6. Use Primary+Z / Primary+Shift+Z to undo/redo annotations outside text inputs. The eraser only removes ink. The annotation list can remove other annotation types.
7. Use Primary+Shift+E or the export button to write annotations into a **new PDF**. Existing files are never overwritten. Text notes become standard sticky-note icons with the full Unicode text in their popup; export does not acknowledge pending sidecar saves.

Use the status-bar layout menu for Single, Continuous, or Spread reading; Primary+Shift+L cycles them. Spread pairs pages 1–2, 3–4, and so on. Continuous mode updates the current-page indicator as you scroll.

Create categories with the + button next to Categories on Home. Select a category to rename/delete it; use each document's category menu to assign it. Deleting a category moves its documents to Unassigned without deleting files. New libraries contain no seeded named categories.

Favorites, user categories, appearance, sidebar width, layout, and reading state persist automatically. Restoring open documents and restoring page positions are independent settings.

## Local data and safety

State is stored under the OS local application-data directory in `Breeze/state.json`. The built-in sample's sidecar lives in the same directory. External-document annotations default to a document-adjacent sidecar; an explicitly selected alternative is associated with the library entry.

Use an isolated directory for development/testing:

```sh
BREEZE_DATA_DIR="$PWD/target/manual-state" cargo run --locked -p breeze-app
```

Only one process may write a given state directory. Invalid state, changed source fingerprints, externally edited sidecars/state, and write failures are reported rather than silently overwritten. Normal closing waits for state persistence and refuses to discard dirty/in-flight annotations. If a write error blocks closing, correct the destination/conflict; do not force-quit and assume unsaved changes were persisted.

The reader currently caps PDF input at 256 MiB and 20,000 pages and uses trusted documents while broader malformed-input/decoder-memory validation is pending. The worker result cache is limited to 32 MiB; UI page and thumbnail caches have separate 64 MiB / 8 MiB budgets and count limits. These account for retained result data, not total engine, transient CPU/GPU upload, or process memory.

## Current limitations

- No original text/image editing or PDF content insertion. Editing controls remain previews, with no cover-up implementation.
- Annotation export is restricted: encrypted, signed, form/widget, and non-default-UserUnit documents are rejected; original/alias/existing destinations are never overwritten. Export preserves existing annotations but does not import them into editable Breeze history. It is not a sanitizer or an archival/signature-preserving converter.
- Page rasterization remains fixed at 144 DPI, capped at four megapixels. Zoom does not request higher-resolution tiles. Extremely dense visible-page ranges can hit the cache budget; affected pages report the limit rather than repeatedly evicting/rerendering one another.
- Text selection uses glyph bounding boxes, not general paragraph/complex-script selection. Rotated glyph runs are excluded. Scans require region tools; no OCR is performed.
- Outline and thumbnail lists are virtualized. Named outline destinations remain unsupported. Search is a bounded-result linear scan, not an indexed full-text service.
- Theme preferences, restoration, and opening-window policies work. Default keeps geometry; Maximized maximizes a newly opened document; Fit Document sizes once within available display space without changing zoom. Final native rechecking after the position-aware sizing fix remains pending. Interface language applies immediately through a native dropdown (English, Simplified Chinese, Traditional Chinese) and persists locally. Update checks use stable GitHub Releases from `poneding/breeze`: Manual by default, Automatic once per startup, or Never for background checks. Settings shows the installed version, result and retry button; newer versions link to the release page. File associations, signing, and automatic installation remain pending.
- Engine font fidelity and real-document tail latency remain release risks. Passing the controlled sample tests is not broad PDF compatibility certification.

## Repository

```text
crates/
  breeze-core/       Reader state, annotation geometry, and history
  breeze-pdf/        PDF reading/search/cache and protected annotation-copy export
  breeze-storage/    Background state storage, atomic writes, and sidecars
  breeze-app/        Native UI, persistence integration, tools, and tests
assets/              Required sample and third-party notices
docs/specs/          Product and technical specifications
docs/plans/          Implementation work packages and delivery status
scripts/             Local packaging helpers and release-tool tests
.github/             Cross-platform build/test and tag-release workflows
```

See [AGENTS.md](AGENTS.md) for contributor rules and single-line Conventional Commits, the [specification index](docs/specs/README.md) for design constraints, and the [icon map](docs/specs/06-icons.md) for Lucide selections.

## Validate and package

```sh
cargo fmt --all -- --check
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
# Scoped sample-only performance probe; not a release acceptance benchmark.
cargo test -p breeze-pdf --release --locked report_sample_render_and_cache_cost -- --ignored --nocapture
# Generate isolated annotation-export fixtures for independent visual checks.
cargo test -p breeze-pdf --locked write_visual_export_fixtures -- --ignored --nocapture
# Local macOS development bundle, not signed/notarized distribution.
./scripts/bundle-macos.sh
open target/Breeze.app
```

## Automated releases

Pushing a SemVer tag (for example `v0.1.0` or `v0.2.0-rc.1`) triggers the [release workflow](.github/workflows/release.yml). The tag version must exactly match the workspace version in `Cargo.toml`. After checks and all four builds succeed, it publishes a macOS `.dmg` for Apple Silicon and Intel, Windows `.msi` and setup `.exe`, and Linux `.deb` and `.AppImage`, each with a SHA-256 checksum; prerelease tags are marked accordingly. A manual run of the same workflow builds and verifies every installer without publishing.

These are unsigned development installers, not production acceptance: macOS packages are neither Developer ID signed nor notarized and Windows packages are not code-signed, so Gatekeeper and SmartScreen warn before installation. See [release instructions and limitations](docs/releases.md) for versioning, prerequisites, installed layout, per-platform install steps, and safe reruns. Python 3.11+ is used only for development/CI packaging and its tests, not by the application.

## Dependency constraints

GPUI Kit is pinned to 0.7.0, PDF Oxide to 0.3.78, and the annotation writer (`lopdf`) to 0.45.0. Other previously locked dependency versions were retained. Keep the lockfile's `office_oxide` at 0.1.9 until its pairing is revalidated. Do not blanket-update dependencies, patch the Cargo registry, or replace the pure Rust engine to hide a fidelity/performance failure. The high-level PDF DOM editor is not an approved structural-editing path.

PDF Oxide's high-level annotation Contents decoder has a known UTF-16 issue. Exported Unicode notes were independently verified with pypdf and PDFKit; standard encoding is not changed to hide that reader defect. External verification tools (Poppler, PDFKit, and pypdf) are not application dependencies.

Generated screenshots, reports, bundles, and test state belong under ignored `target/`, not in version control.


## Publishing update metadata

Update discovery is read-only and uses `poneding/breeze` GitHub Releases. Create the
public repository and publish a non-draft, non-prerelease Release with a SemVer tag
such as `v0.2.0` to advertise a version newer than the current `0.1.0` build. The
checker accepts optional `v`/`V` prefixes, compares numeric versions and ignores build
metadata for precedence. Missing/private repositories and missing stable releases
are reported as unavailable, never as up to date. GitHub rate limits and network
errors remain retryable. Breeze opens a release page only on request; it does not
download, install, sign, or publish binaries.
