# Upstreaming the engine changes so `vendor/` can be removed

## Goal

The user requires both of the following, and confirmed them explicitly:

- **No vendored engine source.** `vendor/pdf_oxide` must go, rather than stay as a
  permanent local patch.
- **Device-resolution display.** Pages stay sharp at any zoom; downscaled or
  blurry rendering is not an acceptable trade for removing the copy.

Two decisions were recorded when the goal was set:

1. **Path: upstream first, delete after the release.** Both engine changes are
   submitted to PDF Oxide; `vendor/` is deleted once a published release contains
   them. Until then the patched copy stays, and the sharp, memory-bounded region
   path stays in use.
2. **When `vendor/` is deleted, zoom is not refused for memory reasons.** The
   fallback below renders whole pages, whose cost grows with zoom. The user
   accepts that growth rather than a limit that would refuse a deep zoom, so the
   documented memory target must be re-measured and revised at that point instead
   of being defended silently.

## Why the fallback is whole-page rendering

Stock PDF Oxide has no way to rasterize a sub-region. `RenderOptions` carries
only `dpi / format / background / render_annotations / jpeg_quality /
excluded_layers / max_output_pixels` — no clip, viewport, region or transform —
and `render_page_region` renders the whole page, decodes the encoded buffer,
crops it and re-encodes it. This is true of the current `main` as well, which is
the 0.3.78 release commit.

Sharpness does not depend on the local patch: `render_page` renders at any DPI in
`ImageFormat::RawRgba8`, so a whole page at device resolution is already sharp.
What the patch buys is a **bounded working set when zoomed in**. Removing it
therefore changes memory, not clarity:

| Letter page 612 x 792 pt, 2x display | Whole-page raster | Visible-region raster |
|---|---:|---:|
| Fit width, 1600 px window | 2784 x 3604 = 10.0 Mpx / 40 MiB | 2784 x 1792 ~= 5.0 Mpx / 20 MiB |
| 400% zoom | 6528 x 8448 = 55.2 Mpx / 221 MiB | ~5-10 Mpx / 20-40 MiB |

A stock-engine adapter must also guard against `max_output_pixels`, which
*reduces the scale* when a page is over budget: returned dimensions have to be
validated against `ceil(points x dpi / 72)`, and a reduced raster must be treated
as a failure to render sharply rather than displayed as the requested density.

## Prepared submissions

`target/upstream/` (not committed; regenerate from the repository as described in
its `SUBMISSION.md`) holds:

- `01-region-rendering.patch` — adds `PageRenderer::render_device_region`, the 49
  line change currently carried in `vendor/`.
- `02-render-integrity.patch` — the render-integrity propagation (failed image,
  mask, Form and shading draws become errors instead of logged skips).
- Issue texts for both, and a drop-in upstream-style test for the region API.

Verified before writing this plan:

- The archive's SHA-256 matches the value recorded in
  `vendor/pdf_oxide/BREEZE-PATCH.md`, and `main` is byte-identical to it in the
  patched file, so no rebase is needed.
- Applying `01` then `02` to the archive reproduces the working tree's
  `page_renderer.rs` byte for byte: these two patches are the entire divergence
  from upstream, in one file, 230 changed lines; the other 400 tracked files are
  identical.
- The region test file passes 4 of 4 against the patched engine here.

## Sequence

Upstream's `CONTRIBUTING.md` and `AGENTS.md` are gate-kept, and an agent may not
open an issue or pull request on its own: the human submits, discloses AI
assistance, and signs off. Each change needs an issue the maintainer has approved
and assigned, one logical change per PR, and one open PR at a time for a
non-maintainer. The integrity change is a behaviour change for every consumer, so
its issue proposes it as an opt-in `RenderOptions` field rather than a silent
default change.

Order:

1. Post the region issue; agree the API shape; get it assigned.
2. Open the region PR with the test, a changelog entry and the `rendering` tier
   run; answer reviews; merge.
3. Post the integrity issue; agree default-on versus opt-in; get it assigned.
4. Open the integrity PR with its fixtures in a commit before the fix commit, as
   their CI verifies that the test goes red without it.
5. When a release contains both, do the deletion described in
   `target/upstream/SUBMISSION.md`.

## Consequences to handle at deletion time

- The region scheduler, cell grid, group slicing and prefetch keys in
  `crates/breeze-app/src/raster.rs` are replaced by whole-page requests at the
  device scale; one texture per page replaces many 512 px cells, so atlas packing
  and seam-gutter handling disappear with them.
- **Memory has to be re-measured and the target revised.** At 2x, one page is
  40 MiB at fit width and 221 MiB at 400%, before the GPU texture of the same
  size. The 350 MiB RSS target in the existing documentation was measured for the
  region path and cannot be claimed afterwards; the new figure belongs in
  `docs/plans/STATUS.md` with the zoom level and window it was measured at.
- Deriving the raster scale needs `dpi = ceil(scale x 72)` because the stock API
  takes an integer DPI, so a whole-page raster is at or slightly above the
  required density. That is safe for sharpness but must not be presented as an
  exact 1:1 device grid.
- Rotation stays in our adapter, where it already is.

## Open risks

- The maintainer may decline either change, or accept the integrity change only
  with different semantics (for example default-on in a breaking release).
- The merged API may differ from the patch, so the call site and its tests must
  be revisited rather than assumed compatible.
- If the region API is declined, the choice is between keeping the narrow patch,
  accepting whole-page memory growth, or evaluating another pure-Rust engine.
  The last option was not researched to a conclusion: this session could not
  reach the documentation hosts needed to check whether any candidate offers a
  device-rectangle render entry point, and any candidate would additionally have
  to match the text, outline, page-geometry, annotation and encryption surfaces
  the adapter uses today.
