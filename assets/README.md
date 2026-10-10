# Prototype Assets

## Bundled sample

`samples/field-notes.pdf` is a project-authored, synthetic eight-page document retained for the built-in reader demo and adapter tests. It contains standard English fonts, vector elements, an image shared across pages, transparency, and one embedded outline entry. It is not user reading history.

The sample was generated during the initial technical investigation. Its one-off generator was removed during repository cleanup; preserve the PDF itself because the application embeds it at compile time and tests rely on it. No third-party test paper is distributed with the prototype.

## Icons and notices

The application uses the SVG catalog embedded by `gpui-kit-assets`; this directory does not contain a second icon implementation. Preserve `licenses/LICENSE-LUCIDE.txt` with redistributed Lucide assets.

`icons/bookmark-filled.svg` is a filled derivative of the Lucide bookmark from `gpui-kit-assets` 0.7.0. It retains the upstream path and stroke and changes `fill` from `none` to `currentColor` for the favorite state. The same Lucide ISC notice applies.

The function-to-icon map and selection guidance are maintained in [the icon specification](../docs/specs/06-icons.md). Other dependency licenses remain governed by their respective crates; this notice is not a complete release-license audit.

## Built-in themes

`themes/gpui-kit/` contains the complete official theme gallery, pinned to the
source commit of GPUI Kit 0.7.0 and embedded into the application. See
[theme provenance and licensing](themes/README.md). Preserve the upstream JSON
author metadata and `licenses/LICENSE-GPUI-KIT-THEMES.txt` when distributing
these assets.

## Breeze identity

`brand/` contains the project-authored Breeze logo and platform icon assets. These
are application identity assets, not replacements for the Lucide command icons.
See [the brand asset guide](brand/README.md) for sources and usage.

## Pinned PDF renderer patch

`vendor/pdf_oxide/BREEZE-PATCH.md` documents the local 0.3.78 device-region patch. Its MIT/Apache licenses, NOTICE, trademark notice, and bundled-font license texts are also included in `assets/licenses/` for review/release packaging. These notices do not complete the broader transitive-dependency license audit.
