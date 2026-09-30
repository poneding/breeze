# Breeze device-region patch

This is the crates.io `pdf_oxide` 0.3.78 source distribution, pinned to upstream commit `ad49c4cb3638dc950a29350ef882161372fde473`. Original archive SHA-256: `95b1ecc1a60a314fbd1aea5fe8f700ec32af1efa70fef5c6879162444da3437d`.

The only upstream source file changed is `src/rendering/page_renderer.rs`. It adds `PageRenderer::render_device_region`, sharing the existing operator execution, fonts, annotations, color management, and page transform. This entry point allocates the requested device-pixel rectangle and translates the original full-page transform into it. It rejects invalid/over-budget regions instead of shrinking the full-page scale. Existing whole-page entry points retain upstream behavior. No page dictionaries or source streams are rewritten.

The caller must keep source revision, crop/rotation, scale, and region in its cache identity and account for decoder/intermediate allocations separately. Each call still visits the original operators and uses the existing document/font caches; this patch does not claim retained display-list preparation or cancellation inside an operator.

Breeze regression coverage is in `crates/breeze-pdf/tests/device_tiles.rs`. Independent renderer comparisons and native validation are recorded in `docs/plans/09-pixel-accurate-display.md`. This is a local dependency patch, not a claim that upstream has accepted the API or that every PDF feature has passed native acceptance.

The original MIT/Apache-2.0 licenses, NOTICE, trademark notice, source assets, and their notices are retained. The registry's cache markers/checksum file and its independent lockfile are omitted; the workspace Cargo.lock pins the actual build graph.
