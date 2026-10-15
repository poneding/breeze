<div align="center">

<img src="assets/brand/breeze-icon-256.png" width="128" alt="Breeze application icon" />

# Breeze

**A compact, native reading application for PDF, EPUB, and Markdown.**

Local-first · Native Rust and GPUI Kit · No WebView

[![CI](https://github.com/poneding/breeze/actions/workflows/ci.yml/badge.svg)](https://github.com/poneding/breeze/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/poneding/breeze?include_prereleases&sort=semver&label=release)](https://github.com/poneding/breeze/releases)
[![License: AGPL-3.0-only](https://img.shields.io/badge/license-AGPL--3.0--only-blue.svg)](LICENSE)
[![Rust 1.97+](https://img.shields.io/badge/rust-1.97%2B-orange.svg?logo=rust&logoColor=white)](https://www.rust-lang.org)
[![Platforms](https://img.shields.io/badge/platform-macOS%20%7C%20Windows%20%7C%20Linux-lightgrey.svg)](#quick-start)
[![Built with GPUI Kit](https://img.shields.io/badge/GPUI%20Kit-0.7.1-1f6feb.svg)](https://crates.io/crates/gpui-kit)

</div>

Breeze is a native, cross-format reading application. PDF is the first backend,
not the product's document model: the same reader also opens EPUB and Markdown
documents. Documents stay on your machine, annotations live in sidecar files, and
the original file is never modified.

## Highlights

- **Local library** — durable favorites, user categories, reading positions, open
  documents, and tabs; no account, and your documents never leave your machine.
- **Reading and navigation** — outline, virtualized thumbnails, page input,
  fit/zoom, and Single, Continuous, or Spread layouts.
- **Search and selection** — in-document search (Primary+F) and rectangular
  glyph selection with Primary+C.
- **Sidecar annotations** — ink, shapes, and notes saved to
  `<filename>.breeze.json`, with undo/redo and autosave; the PDF page content
  stays read-only.
- **Export to a new PDF** — restricted standard annotations, including sticky
  notes with the full Unicode text; existing files are never overwritten.
- **Native desktop integration** — file associations and Open With, reveal in the
  system file manager, tooltips with real shortcuts, and the full GPUI Kit theme
  gallery plus Follow System.
- **Interface languages** — Auto detect plus English, Simplified Chinese, and
  Traditional Chinese, applied immediately and persisted.
- **Update checks** — stable GitHub Releases, Manual by default, never installs
  anything on its own.

## Quick start

Requires Rust 1.97 or newer, plus the platform toolchain described in the
[development guide](docs/development.md#development-environment).

```sh
cargo run --locked -p breeze-app
cargo run --locked -p breeze-app -- /absolute/path/document.pdf
```

The bundled eight-page sample is available from Home.

## Status and limitations

Breeze is under active development and is **not production-accepted**. Implemented
and locally verified: the PDF reader and library, EPUB/Markdown reflow reading,
search, glyph-based text selection, sidecar annotations, restricted
standard-annotation export, opening-window policies, file associations, and
update checks. Editing controls for original page content remain previews.

Native Windows/Linux runtime acceptance, code signing and notarization, a complete
dependency-license audit, and the font-fidelity and tail-latency gates are still
pending. Page rasterization is fixed at 144 DPI, text selection uses glyph boxes
rather than general paragraph selection, no OCR is performed, and the reader caps
input at 256 MiB and 20,000 pages. See the [delivery status](docs/plans/STATUS.md)
for the verified record.

## Documentation

| Document | Contents |
| --- | --- |
| [Delivery status](docs/plans/STATUS.md) | What is implemented, verified, and still open |
| [Implementation roadmap](docs/plans/README.md) | Work packages and delivery order |
| [Specifications](docs/specs/README.md) | Product and technical constraints |
| [Development guide](docs/development.md) | Environment, commands, shortcuts, data safety, release |
| [Release notes](docs/releases.md) | Packaging, install steps, and release limitations |
| [Contributing](AGENTS.md) | Repository rules and commit convention |

## License

Breeze is distributed under the GNU Affero General Public License v3.0 only; see
[LICENSE](LICENSE). The PDF engine is MuPDF through `mupdf-rs`, which is also
AGPL-licensed. Release packages carry MuPDF's license notice and the
corresponding-source location.
