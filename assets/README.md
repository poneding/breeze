# Prototype Assets

## Bundled sample

`samples/field-notes.pdf` is a project-authored, synthetic eight-page document retained for the built-in reader demo and adapter tests. It contains standard English fonts, vector elements, an image shared across pages, transparency, and one embedded outline entry. It is not user reading history.

The sample was generated during the initial technical investigation. Its one-off generator was removed during repository cleanup; preserve the PDF itself because the application embeds it at compile time and tests rely on it. No third-party test paper is distributed with the prototype.

## Icons and notices

The application uses the SVG catalog embedded by `gpui-kit-assets`; this directory does not contain a second icon implementation. Preserve `licenses/LICENSE-LUCIDE.txt` with redistributed Lucide assets.

The function-to-icon map and selection guidance are maintained in [the icon specification](../docs/specs/06-icons.md). Other dependency licenses remain governed by their respective crates; this notice is not a complete release-license audit.
