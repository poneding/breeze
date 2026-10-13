# Retired PDF Oxide patch (historical record)

Breeze no longer vendors a patched PDF Oxide. This file keeps the provenance and the
content of the patch that `vendor/pdf_oxide` carried until October 13, 2026, because it
is the reference for the two upstream submissions described in
[work package 20](../plans/20-upstreaming-engine-changes.md).

The vendored copy was the crates.io `pdf_oxide` 0.3.78 source distribution, pinned to
upstream commit `ad49c4cb3638dc950a29350ef882161372fde473`. Original archive SHA-256:
`95b1ecc1a60a314fbd1aea5fe8f700ec32af1efa70fef5c6879162444da3437d`, verified against the
crates.io archive before retirement. One upstream file differed,
`src/rendering/page_renderer.rs`, by 230 changed lines: the device-region entry point below
(49 lines) and the rendering-integrity change (181 lines).

Retiring the copy cost the rendering-integrity propagation, which the engine has no
equivalent for: a failed image, mask, Form or shading draw is logged and skipped upstream,
so a page can return as a successful but incomplete raster. The device-region entry point
is no longer used by Breeze, which renders whole pages at the requested density.

The only upstream source file changed is `src/rendering/page_renderer.rs`. It adds `PageRenderer::render_device_region`, sharing the existing operator execution, fonts, annotations, color management, and page transform. This entry point allocates the requested device-pixel rectangle and translates the original full-page transform into it. It rejects invalid/over-budget regions instead of shrinking the full-page scale. No page dictionaries or source streams are rewritten.

The rendering-integrity follow-up propagates failed image, inline-image, ImageMask and Form draws instead of logging and returning incomplete pixels. It also refuses missing/unsupported XObjects or shadings, malformed Type 3 glyph streams, recursion-limit failures, incomplete stencil data, unusable image masks and soft masks, and unsupported colour-key mask sample layouts. Resource scopes unwind before Form errors propagate. Whole-page, thumbnail and device-region entry points share these checks; a failed result is not admitted to Breeze's pixel cache. This changes error handling, not the rendering engine or page transform.

`crates/breeze-pdf/tests/render_integrity.rs` exercises damaged drawing fixtures, valid masks, reader/export page-unit agreement, unchanged source bytes and recovery on the next page. These checks are not a blanket fidelity guarantee. Font substitution, colour-resolution fallbacks, unsupported pattern details, annotation appearances and decoder/intermediate resource limits still require representative validation. UserUnit preflight belongs to the Breeze adapter and does not change upstream geometry.

The caller must keep source revision, crop/rotation, scale, and region in its cache identity and account for decoder/intermediate allocations separately. Each call still visits the original operators and uses the existing document/font caches; this patch does not claim retained display-list preparation or cancellation inside an operator.

Breeze regression coverage is in `crates/breeze-pdf/tests/device_tiles.rs`. Independent renderer comparisons and native validation are recorded in `docs/plans/09-pixel-accurate-display.md`. This is a local dependency patch, not a claim that upstream has accepted the API or that every PDF feature has passed native acceptance.

The original MIT/Apache-2.0 licenses, NOTICE, trademark notice, source assets, and their notices are retained. The registry's cache markers/checksum file and its independent lockfile are omitted; the workspace Cargo.lock pins the actual build graph.
