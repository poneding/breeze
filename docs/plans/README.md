# Implementation Roadmap

Implementation was authorized on September 29, 2026, after acceptance of the compact UI direction. This authorizes product work, not weaker fidelity/performance requirements or a non-Rust PDF engine.

## Delivery order

| Stage | Plan | Outcome |
|---|---|---|
| 1 | [Local reader and persistence](01-local-reader.md) | A usable local application with durable library/settings, restoration, and search |
| 2 | [Annotations and document integrity](02-annotations.md) | Actual page annotations, undo/redo, and fingerprinted sidecars |
| 3 | [Structural editing and PDF interoperability](03-editing-export.md) | Precise supported-object edits and independently validated export |
| 4 | [Platform and release acceptance](04-release.md) | Native three-platform validation, packaging, and performance gates |

Stages are independently testable, not a promise that every PDF can be edited. Current implementation status is maintained in [delivery status](STATUS.md). Completed and verified phases are committed locally as required by [AGENTS.md](../../AGENTS.md); remote pushes and publication still require separate authorization.

## Invariants

- Preserve the accepted compact layout and GPUI Kit components.
- Do not write to the original PDF while drawing annotations.
- Never replace structural editing with cover-up, region-erasure/reinsertion, or rasterization.
- Do not silently discard corrupted state or overwrite changed source/sidecar files.
- Keep file I/O and document work outside the UI thread; bound pending work.
- Keep unavailable functionality visibly unavailable rather than simulating success.
- Earlier font-substitution and raster-tail-latency risks remain release blockers. No engine replacement or budget relaxation has been approved.

## Next executable work packages

1. [05 — Annotation export](05-annotation-export.md): implement and validate a restricted standard-annotation exporter without source-content editing.
2. [06 — Opening-window policies](06-window-policy.md): implement the currently preview-only window-size settings.
3. [07 — Detailed remaining backlog](07-remaining-work.md): concrete renderer, layout, structural-editing, localization, release, and recovery tasks with explicit dependencies/gates.

These work-package documents were written before the corresponding code changes. Earlier broad stages remain the roadmap; STATUS records actual acceptance.

4. [08 — Reading layouts and library refinement](08-reading-layout-and-library.md): user-requested density, tabs, categories, full thumbnails, and real reading layouts.

5. [09 — Pixel-accurate display](09-pixel-accurate-display.md): device-resolution rendering and true viewport tiles, with an engine feasibility gate and a reproducible resolution-cost probe. This is a proposal, not a shipped rendering policy.

6. [10 — Library presentation and categories](10-library-presentation.md): seven separately committed improvements to navigation, rows, covers, Open controls, and category memberships/colors.

8. [14 — Reader tools and document details](14-reader-tools-and-details.md): filled favorites, an optional information/annotation sidebar, and interactive tool settings with a shared eight-color vocabulary.

9. [15 — Direct annotation editing](15-direct-annotation-editing.md): native numeric controls, text input at the clicked page position, and selection/editing/deletion of sidecar annotations.

11. [17 — Interface languages and release checks](17-language-and-updates.md): three persisted interface languages and bounded stable-release checks for the user-designated GitHub repository.
