# Engine Assessment and Open Risks

## Decision summary

| Component | Decision |
|---|---|
| GPUI Kit 0.7.0 / GPUI snapshot 0.3.7 | Adopted for the native prototype |
| `pdf_oxide` 0.3.78 | Provisional read-only parser/renderer, not a production fidelity approval |
| `office_oxide` 0.1.9 | Keep the verified transitive version in `Cargo.lock` |
| PDF page content | Read-only; content editing is not a product capability |
| `lopdf` 0.45.0 | Adopted for restricted annotation-copy export |
| Alternative native PDF engines | Not authorized |

The exploratory executable, raw reports, screenshots, and temporary fixtures were removed during repository cleanup. The observations below are retained as a **historical decision record**, not as a currently reproducible benchmark suite. Recreate focused, versioned tests before relying on them for a production decision.

## Dependency compatibility

The earlier build of `pdf_oxide` 0.3.78 resolved `office_oxide` 0.1.12 and failed because a `DocumentIR` initializer lacked the newly required `defined_names` field. Locking the transitive dependency to 0.1.9 restored the build.

Keep that pairing until retested. Do not modify downloaded registry sources or add speculative multi-version compatibility code. The current application enables the Rust rendering and ICC features; the old initial performance probe used rendering and parallel processing without the same complete feature graph.

## Historical performance observations

Recorded on September 29, 2026: Apple M1 Pro, 16 GiB RAM, macOS 15.7.9, Rust 1.97.1, Release mode. Rendering used 144 DPI raw RGBA output, a four-megapixel output limit, and 20 rotating-page samples per file. The corpus comprised four synthetic fixtures and one PDF.js `tracemonkey` paper.

| Case | Observation |
|---|---|
| Synthetic text/image pages | Rendering p95 approximately 15 ms |
| First renderer initialization and page | Approximately 250 ms, including font setup |
| Real paper | Rendering p95 approximately **117 ms**, above the proposed 100 ms budget |
| Synthetic 1,000-page file | First text extraction approximately 71 ms; this was a small repetitive file, not a large scan |
| Separate process-resource run | Maximum RSS approximately 142 MiB, including fixture generation and sequential rendering; not application working set |

These figures exclude GPU upload and UI presentation. OS file caches were not cleared, and later files reused process font initialization. They do not establish end-to-end opening latency, frame rate, broad compatibility, or large-scan memory safety.

**Open risk:** profile the real-paper tail latency and representative scan workloads before adopting a production rendering policy. No automatic relaxation of the budget or engine constraint is approved.

## Fidelity and memory risks

- A visual comparison with an independent renderer showed a suspected standard-font substitution problem: `Helvetica-Bold` text appeared with serif characteristics in the candidate output. The root cause is unresolved.
- `render_page_region` in the evaluated version renders the entire page before cropping. It is not a memory-efficient tile renderer.
- `max_output_pixels` bounds the output raster, not all decoder/intermediate allocations. High-resolution JPEG2000/JBIG2 cases need separate peak-memory testing.
- CJK/CID/subset fonts, Type 3 fonts, complex transparency, color management, malformed files, and broad real-document compatibility are not accepted on the strength of these probes.

## Revalidation gates

Before production work depending on the engine, obtain representative, redistributable test documents and record their hashes, feature coverage, hardware, dependency graph, and expected output. Validate semantics and visual fidelity independently, and keep any new benchmark harness narrowly scoped to the decision it supports.

Windows and Linux native behavior remains unverified. System-theme event delivery, IME, titlebar controls, and GPU behavior require native checks; unit tests or configured CI are not substitutes.

## Primary references

- [GPUI Kit](https://github.com/longbridge/gpui-kit) and [versioned API](https://docs.rs/gpui-kit/0.7.0/gpui_kit/)
- [PDF Oxide](https://github.com/yfedoseev/pdf_oxide): evaluated 0.3.78 rendering source
- [lopdf](https://docs.rs/lopdf/0.45.0/lopdf/): the version used for restricted annotation-copy export

References describe upstream capabilities, not measured application guarantees.

## Current development measurement

The implemented reader now has a bounded result cache and a focused ignored Release performance test. See [delivery status](../plans/STATUS.md) for its sample-only observations. Those results do not replace the historical real-paper or independent-font findings above.

## Follow-up: annotation interoperability

The restricted annotation exporter preserves standard UTF-16 note strings and explicit vector appearances. A controlled eleven-kind fixture passed Poppler/PDFKit rendering, pypdf/PDFKit exact Unicode note decoding, and unchanged-background pixel comparison in Poppler. These tools are external validators, not application engines.

A new reader limitation was confirmed in `pdf_oxide 0.3.78`, `src/annotations.rs`: high-level Contents decoding uses `String::from_utf8_lossy` even when the PDF string has a UTF-16 BOM. Do not encode invalid UTF-8 PDF 1.7 strings to hide this. Any future imported-note UI needs a tested PDF text-string decoding adapter or a verified upstream correction.

See [work package 05](../plans/05-annotation-export.md) and [delivery status](../plans/STATUS.md) for scope. Existing font fidelity, tail latency, and platform acceptance risks remain unresolved.
