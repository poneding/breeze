# PDF engine strategy comparison

Date: October 14, 2026. Status: decision recorded. The user selected MuPDF through `mupdf-rs` and accepted AGPL-compatible distribution; implementation is tracked in [work package 22](../plans/22-mupdf-migration.md).

## Recommendation

For the stated goal of high performance, low resource use, and a polished reading experience, **use MuPDF through `mupdf-rs` as the target architecture**, subject to completing the accepted AGPL source path and passing adapter/corpus gates. Its display-list plus clipped-pixmap path directly addresses whole-page allocation and can reuse page preparation across zoom levels.

Implement the scheduler fixes shared by both options first: prioritize currently visible pixels, give visible thumbnails bounded service, admit prefetch by estimated bytes before rendering, and prevent predictions that cannot remain resident from being retried indefinitely. These changes survive either backend choice and improve the current reader while the engine decision is tested.

Keep PDF Oxide only if the migration fails its fidelity/build gates or AGPL distribution becomes infeasible. Borrowing SumatraPDF's queue/cache ideas alone improves responsiveness, but does not put a hard bound on the current full-page raster. Achieving the same high-DPI memory ceiling under option 2 requires a maintained PDF Oxide renderer change or fork.

This decision does not imply a full application rewrite. Keep the UI, core navigation, persistence, sidecars, and protected `lopdf` export. Replace only the PDF reader behind `breeze-pdf`.

## Comparison

| Criterion | 1. MuPDF via `mupdf-rs` | 2. PDF Oxide plus Sumatra-inspired optimization |
|---|---|---|
| High-DPI memory | Strongest direct path. Record a page display list, replay only the requested clip, and allocate only the tile/viewport pixmap. A retained list also avoids reparsing/re-preparing page operators for each zoom. | Scheduler and cache changes reduce waste, but the renderer still allocates a whole page for each device-density request. A true limit needs a new region path in the engine. Lowering resolution would trade away the requested sharpness. |
| Page-turn latency | Region replay can reduce the work for each visible update; display-list reuse amortizes page preparation. MuPDF still has decode-heavy worst cases, and Breeze must prioritize visible jobs. | Visible-first scheduling and bounded lookahead can help quickly. A synchronous full-page render already running is not preemptible in the current adapter, so one expensive page can still hold up navigation work. |
| Short-term implementation | Native probe and API feasibility are established. The adapter's open/page/text/outline/search/render/error paths still need replacement and comparison. | Engine-agnostic scheduler work is comparatively small. A region-capable engine change is substantial: it must clip allocation and drawing correctly across transforms, images, masks, transparency, ICC, rotation, and page boxes. |
| Fidelity ceiling | MuPDF is a mature native renderer with broad PDF APIs. This raises the likely compatibility ceiling, but Breeze still needs its own real-document corpus and independent visual comparisons. | Retains known PDF Oxide risks and dependency-specific rendering behavior. Fixes remain under application/upstream control if they can be implemented and maintained in Rust. |
| License | `mupdf`/`mupdf-sys` are AGPL-3.0. A commercial Artifex license is the alternative if the product cannot meet AGPL terms. | PDF Oxide is MIT OR Apache-2.0. The app still needs a whole-dependency license audit, but there is no PDF-engine copyleft obligation. |
| Build and release | Compiles MuPDF C/C++ through `mupdf-sys`; requires the native build toolchain and libclang. The local macOS Release probe built. Windows/Linux native builds, package size, runtime loading, and release notices need validation. | Cargo/Rust build remains simpler and fits the existing project policy. A private fork/patch brings ongoing engine-diff, update, regression and provenance maintenance. |
| Main risk | License/product choice, FFI/native build surface, and an adapter/corpus migration. | Under-delivering the central memory target unless Breeze takes responsibility for genuine region rendering in the engine. |

## What the measurements do and do not say

On the user's PDF, an isolated MuPDF Release probe sampled 21 pages. A 512 × 512 clipped display-list replay produced 1 MiB of pixels, with p95 17.76 ms and probe peak RSS 26.1 MiB. Whole-page MuPDF rendering at a 2× matrix produced 5.70 MiB for the sampled page size, with p95 44.86 ms and probe peak RSS 77.4 MiB. The probe code and conditions are described in [the MuPDF assessment](09-mupdf-rs-assessment.md).

This is evidence that the MuPDF region API works and that the requested output allocation follows region dimensions. It is not a native Breeze-versus-Sumatra comparison, nor a full application benchmark. It does not include GPUI upload, application caches, a retained display-list cache, annotations, or page-turn scheduling. The earlier PDF Oxide worker probe had different setup work and sampling, so its RSS and timings should not be treated as a controlled engine shootout. Benchmark end-to-end behavior again after the adapter exists.

The two options share fixes that do not decide the engine: reserve memory before prefetch, stop prediction churn, prioritize visible page and thumbnail work, and track CPU/GPU residency separately. These are useful under option 1 too; they are not an alternative to region rendering.

## License readiness

The user accepted AGPL-compatible distribution. The workspace now declares
`AGPL-3.0-only` and includes the license texts. The remaining gate is to verify
the source archive, installer notices, and enabled-feature dependency notices in
the release flow.

Because Breeze distributes installers, AGPL source-distribution terms apply to conveying object code. AGPL section 13 adds a source-offer requirement for users interacting remotely when the program supports that interaction; the current local desktop app does not appear to have that service model. Artifex describes MuPDF as available under AGPL or commercial terms and advises that AGPL use must comply with its obligations. Whether linking and the exact package make Breeze a single covered work, and the exact corresponding-source scope, are questions for legal review. Do not treat `mupdf-rs` being a safe Rust crate as removing MuPDF's license obligations.

## Migration gates

1. Verify each installer points to the matching corresponding-source archive; retain the Artifex commercial path as a contingency if the product license changes.
2. Build the smallest adapter around the existing `breeze-pdf` contract: document open, page geometry, outline, text quads/search, thumbnail, clipped visible-region rendering, cancellation, and source-change invalidation. Keep all MuPDF objects on the PDF worker.
3. Compare both engines on the same fixed corpus: the user's PDF locally, the bundled sample, and redistributable scans, CJK/subset fonts, transparency/ICC, JPEG2000/JBIG2, rotation, CropBox, and malformed inputs. Compare geometry/text as well as pixels.
4. Integrate GPUI and measure open-to-first-frame, visible-thumbnail p50/p95, page-turn p50/p95/max, private/RSS memory, GPU residency, cache plateau, and 400% zoom on macOS, Windows, and Linux Release builds.
5. Switch only if fidelity gates pass and end-to-end memory/latency improve without making normal page interaction less responsive. Retain `lopdf` export and Breeze's read-only page-content rule as separate boundaries.

If adapter, fidelity, or native-build gates reject option 1, deliver the shared scheduling fixes and run a scoped PDF Oxide region-render prototype before considering a fallback. A prototype must establish clipped allocation and painting through images/masks, transparency, and ICC without rendering the full page internally.

## References

- [mupdf-rs README and feature/build requirements](https://github.com/messense/mupdf-rs): safe wrapper API, optional reduced feature set, C/C++ build and libclang requirements, AGPL-3.0 license.
- [Artifex licensing](https://artifex.com/licensing): MuPDF's AGPL and commercial licensing paths.
- [GNU AGPLv3](https://www.gnu.org/licenses/agpl-3.0.en.html): corresponding source for conveyed object code and the network interaction provision.
- [PDF Oxide README](https://github.com/yfedoseev/pdf_oxide): current upstream license statement, MIT OR Apache-2.0.
- [Sumatra/PDF Oxide source findings and local measurements](08-performance-assessment.md): mechanisms worth borrowing and limits of the current Breeze scheduler/backend.
