# Breeze

A compact, native Rust reading application built with GPUI Kit. PDF is the first document backend; the product is not limited to PDF.

**Status:** the compact prototype's general UI direction has been accepted. Production annotations, structural editing, persistence, and cross-platform runtime acceptance are still pending. See the [planning status](docs/plans/README.md).

## Run

```sh
cargo run --locked -p breeze-app
# Open a local PDF without modifying the original.
cargo run --locked -p breeze-app -- /absolute/path/document.pdf
```

The bundled sample is project-authored and available from the Home screen. The prototype currently uses Chinese UI strings; repository documentation and code comments are in English.

### Development environment

- Rust 1.97 or newer; the configured CI toolchain is 1.97.1.
- macOS 15 or newer with Xcode Command Line Tools.
- Windows 10 or newer with the MSVC Rust target, Visual Studio C++ Build Tools, Windows SDK, and CMake.
- Linux: Ubuntu 24.04 is the initial baseline, with the GPUI system libraries listed in [CI](.github/workflows/ci.yml), a graphical Wayland or X11 session, and a working Vulkan driver.

The previous native checks ran on an Apple M1 Pro with macOS 15.7.9. Windows and Linux runtime behavior remains unverified. System libraries and build tools do not change the Rust document-engine requirement. No WebView, Node.js, or Python runtime is required by the application.

For a local macOS review bundle:

```sh
./scripts/bundle-macos.sh
open target/Breeze.app
```

The bundle is a development artifact, not a signed or notarized release.

## Prototype capabilities

- Compact document tabs inside the native title bar; icon-based global commands.
- Session-only recent documents, favorites, category selection, and per-document reading position.
- Background PDF rendering, opening local files, page navigation, page-number entry, zoom, fit-to-page, and fit-to-width.
- Embedded outlines and up to four nearby thumbnails; a collapsible navigation panel with a 168–360-pixel resize range.
- Follow System, Light, and Dark appearance modes with independently selected light/dark palettes. Warm white and dark gray are defaults; four additional palette pairs are available.
- Reading, annotation, and editing mode UI, native shortcut tooltips, and independent style previews.

**Not implemented:** annotation drawing/saving, text selection/search, original text/image editing, PDF export, continuous/spread layouts, persisted settings, full localization, file associations, and updates. Editing controls are previews; the prototype does not cover or replace original content to fake editing. Use trusted documents while parser/resource-limit validation is incomplete.

## Repository

```text
crates/
  breeze-core/    Navigation and prototype state without UI/engine dependencies
  breeze-pdf/     Read-only PDF adapter and bounded rendering worker
  breeze-app/     Native UI, themes, commands, and component tests
assets/          Required sample and third-party license notices
docs/specs/      Current product and technical specifications
docs/plans/      Planning status and subsequent implementation plans
scripts/         Local packaging helper
.github/         Cross-platform build/test workflow
```

Start with the [specification index](docs/specs/README.md), [engine assessment](docs/specs/01-validation.md), [UI design](docs/specs/04-design.md), and [icon map](docs/specs/06-icons.md). Contribution rules, including single-line Conventional Commits, are in [AGENTS.md](AGENTS.md).

## Validate

```sh
cargo fmt --all -- --check
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
```

Tests include navigation boundaries, background rendering, pixel conversion, native component interactions, shortcuts, resize limits, and palette selection. They do not substitute for native GPU, IME, or window-control testing on each platform.

## Dependency constraints and known risks

- `gpui-kit` is pinned to 0.7.0 and `pdf_oxide` to 0.3.78.
- Keep the lockfile's `office_oxide` at 0.1.9 until its pairing is revalidated: 0.1.12 failed to compile with the evaluated PDF library.
- The PDF engine is a provisional read-only choice. Earlier probes found a rendering tail-latency miss and a suspected standard-font substitution defect.
- The PDF library's high-level DOM editing path is not an acceptable structural editor for this project.
- Preserve these constraints when changing dependencies; do not perform an unreviewed blanket `cargo update` or patch the local Cargo registry.

Historical screenshots, raw reports, and standalone experimental tools have been removed. Their decision-relevant findings and limitations remain in the specifications. Build caches and live review bundles belong under ignored `target/`, not in Git.
