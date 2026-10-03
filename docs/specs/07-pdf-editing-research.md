# PDF Editing: Market Research and Breeze Recommendation

Research date: **October 3, 2026**. Status: **research and proposal**, not implementation or production acceptance.

## Recommendation

Under Breeze's confirmed Rust, native-UI, and precise-content-editing requirements, the best-supported next step is to retain the existing reader and build a **restricted source-operator editor in `breeze-pdf`, using the already pinned `lopdf 0.45.0` for PDF object reading/writing**. Implement object provenance, encoding checks, instance isolation, and transactions before exposing editing controls.

This is a substantial editor subsystem, not a thin wrapper around a replacement API. None of the Rust projects inspected provides demonstrated, turnkey, arbitrary-PDF editing comparable to a mature commercial editor. This is a conclusion about the inspected implementations, not proof that no such project exists.

Use Quark as a reference for direct selection, contextual properties, and low-friction interaction. Its publicly documented scanned-document editor is a separate AI reconstruction capability, outside the current product scope. Keep the existing [editing contract](03-pdf-editing.md), renderer gates, and source-preservation rules unchanged.

The proposed next work package is [structural-editing validation](../plans/12-structural-editing-validation.md). This research does not enable editing, replace the renderer, add dependencies, or claim that historical experiments have been reproduced.

## Evidence and limits

The investigation used official product documentation, upstream source files and licenses, and the current Breeze source/dependency lockfile. Search results were used to discover sources, not as proof of engine behavior. Source inspection establishes what a particular path does; vendor documentation establishes an advertised capability, not measured interoperability or fidelity.

No Quark account was accessed, no private document was uploaded, and no commercial product was installed or tested. No market-wide benchmark, native editor comparison, or output round-trip experiment was performed. GitHub API requests encountered a rate limit; public Git refs and raw source files supplied the subsequent source snapshots. Disposable downloads are under `target/pdf-editing-research/` and are not repository assets.

## What Quark publicly discloses

Quark's official PC help page explicitly distinguishes two modes [S1]:

| Mode | Officially described behavior | What it establishes |
|---|---|---|
| Ordinary PDF | Select and edit text/images; automatically match original fonts, colors, and layout | A direct-content-editing product experience is advertised |
| Scanned PDF | AI recognizes text, layout, colors, artistic fonts, and image layout, then makes the page editable | An additional recognition/reconstruction product experience is advertised |
| Image operations | Rotate, delete, download, and remove watermarks | User-facing operations are documented; internal object semantics are not |

The documented interaction is Open → Edit → automatic document-type recognition → select text/image → edit → save. Claims of unchanged formatting and seamless replacement are vendor claims; this investigation did not independently validate them.

The inspected help page does **not** identify the PDF engine, SDK supplier, source repository, on-device/cloud processing boundary, font-matching algorithm, or exact save semantics. A focused search for Quark PDF editing engine/SDK/open-source information did not locate a reliable technical disclosure. Do not infer PDFium, Foxit, Apryse, or another supplier from a similar interface or the fact that Quark also ships a browser.

This source describes the current **Quark PC / Super PDF** product family. It does not establish identical capabilities in every Quark Drive client, release, or subscription tier, or identify the version the user has used.

For Breeze, adopt the interaction goals for ordinary PDFs. Automatic OCR, scan reconstruction, and image-background repair remain deferred. Scanned text is ordinarily image pixels, so changing it requires a different operation from editing an existing PDF text operator. Do not label those routes equivalent.

## The different meanings of “PDF editing”

| Category | Typical implementation | Relevance to Breeze |
|---|---|---|
| Annotations | `/Annots`, appearance streams, separate undo history | Already partly implemented; distinct from original content |
| New content | Add text/image drawing operations and resources | Useful future insertion feature; does not replace old content |
| Page organization | Modify page trees, ordering, rotation, or boxes | A separate feature; not proof of text editing |
| Original-content editing | Identify source operators/usages, modify them, preserve unrelated state | Required route for Breeze's existing text/image editor |
| Import and reconstruct | Convert into another scene/document model and export again | May be useful in other products; preservation must not be assumed |
| Scan reconstruction | OCR/layout/font analysis plus image/document reconstruction | Outside Breeze's current scope |

PDF ordinarily describes painting operations rather than an editable paragraph model. Text can be split across operators, drawn out of reading order, use subset/custom font encodings, and live inside reused Forms. Optional structure tags do not supply a reliable editable layout for every file. Extracted Unicode and screen rectangles alone cannot identify the operation to change.

## Comparison of inspected implementations

Licenses below summarize the inspected component, not every bundled dependency or every product edition. A Rust wrapper around a C/C++ engine does not satisfy Breeze's Rust document-processing requirement.

| Project / product | Verified technical basis and scope | License / integration consideration | Decision for Breeze |
|---|---|---|---|
| **PDFium** | Public APIs enumerate/remove page objects, transform objects, replace a text object's text/charcodes, and generate updated page content [S2] | C/C++; permissive main license with third-party notices | Strong low-level API reference; excluded as an application engine by current requirements |
| **MuPDF / PyMuPDF** | Native PDF object manipulation; public object APIs include dictionary/array/deep-copy helpers [S3]. Rendering or region-redaction APIs alone do not establish exact selected-instance replacement | MuPDF is C; AGPL or commercial licensing; Python bindings do not change the native engine | Reference/independent validation only; not an approved engine or replacement shortcut |
| **Apache PDFBox** | Its `RemoveAllText` example parses operators, writes modified streams, and traverses Form/pattern resources [S4] | Java, Apache-2.0 | Useful reference for stream/state traversal. Its global-removal example is not an instance-safe editor and must not be copied as one |
| **LibreOffice Draw import** | The import module explicitly parses PDF using Poppler and constructs an editable LibreOffice document; its README documents heuristic line reconstruction and clipping/fill limitations [S5] | C++ application; component-specific licenses, including the Poppler-linked GPL helper | Useful interaction comparison; import/export reconstruction is not evidence of preserving untouched PDF structure |
| **ONLYOFFICE PDF Editor** | Official product advertises text editing, objects, annotations, and pages. Inspected JS PDF UI and C++ `PdfEditor.cpp` use its reader/writer and Xpdf types [S6] | Core is AGPLv3; substantial JS/C++ office stack | A real product/source reference, but not a small Rust component. Exact arbitrary-edit preservation was not validated |
| **Stirling PDF** | Current developer guide describes a React UI and Java/PDFBox backend; the build also declares `jpdfium`, and the frontend lists EmbedPDF and a pdf-lib fork. It is a multi-tool platform, not one interchangeable editor library [S7] | Open-core: MIT default with explicit directory exceptions; the inspected `engine/` has a restrictive User License | Useful workflow reference. Do not describe all current source as MIT or assume every editing path has the same engine/license |
| **PDF.js** | The inspected display editor is explicitly `AnnotationEditor`; the project is a JS PDF viewer [S8] | Apache-2.0, browser-oriented | Annotation/selection reference, not demonstrated arbitrary original-text replacement; no WebView adoption |
| **pdf-lib** | Its own limitations explicitly say it has no API for removing/editing page text outside form fields [S9] | MIT, JavaScript | Good example of why “modify existing PDF” does not imply original-content editing; unsuitable here |
| **`lopdf`** | Rust object graph, stream decoding/encoding, PDF writing, and text-replacement helpers; already used by Breeze export [S10] | MIT; already pinned at 0.45.0 | Recommended low-level substrate, with Breeze-owned provenance/state/font/transaction logic |
| **`pdf_oxide`** | Existing Breeze reader. Local and inspected upstream `save_page` collect geometric erasures and invoke destructive redaction, while collecting additions separately [S11] | Rust; MIT/Apache-2.0 in project metadata | Retain the existing reading role provisionally; do not use this high-level editing path |
| **`pdf-rs/pdf`** | README says read/alter/write, but explicitly calls modification and writing experimental [S12] | MIT, Rust | No evidence that migrating from already-integrated `lopdf` reduces editor risk |
| **Hayro / `hayro-write`** | Renderer/interpreter with known limitations. `hayro-write` converts pages to XObjects/new pages and labels itself internal, not intended for external use [S13] | MIT/Apache-2.0, Rust | Worth observing as a Rust ecosystem project; not a demonstrated complete editor or reason to replace the current renderer |
| **Krilla / `pdf-writer`** | Creation APIs, positioned glyphs, font embedding/subsetting, and PDF generation; Krilla builds on `pdf-writer` [S14] | MIT/Apache-2.0, Rust | Font/output design reference; adding a generator does not solve provenance or preserve an arbitrary existing document |
| **Apryse WebViewer content editing** | Official guide exposes content boxes, text/image editing, properties, insertion, and separate content-edit history; WASM loads on entering edit mode [S15] | Proprietary PDF Editing add-on, separate from standard WebViewer licensing | Closest inspected packaged UX reference. Public APIs do not disclose all algorithms or prove Breeze's exact-preservation contract |

If the requirements changed to prioritize the shortest path to broad editing over Rust processing/native UI, a commercial SDK evaluation would become a credible alternative. It would still require CJK/font, output-preservation, offline operation, platform, and licensing checks. That is a conditional trade-off, not an approved fallback or the recommendation for the current project.

## Findings specific to the current repository

### Selection has no source identity

`Glyph` in [`breeze-pdf/src/lib.rs`](../../crates/breeze-pdf/src/lib.rs) currently contains `character` and `bounds`. It cannot distinguish equal text in different streams, different invocations of a shared Form, or a many-character ligature. Selection can serve reading/markup; it must not become an editing address by matching a rectangle or Unicode string.

### The rejected high-level path is still rejected by current evidence

The local [`document_editor.rs`](../../vendor/pdf_oxide/src/editor/document_editor.rs), `save_page`, records `overlay_additions`, reads `pending_erasures`, turns bounding boxes into redaction regions, and calls `apply_redactions_destructive_for_page`. The inspected upstream snapshot contains the same path [S11]. An upgrade is therefore not currently supported as a solution to precise original-content editing.

This finding is about that path, not a claim that all upstream low-level APIs are unusable. Deleting actual operators by their identity and replacing a selected span is materially different from selecting everything intersecting a geometric erasure region.

### `lopdf` is infrastructure, not the finished editor

In 0.45.0, `Operation` has an operator string and operands, without source byte spans. `replace_text` reads a page's combined content, loops through `Tf` and `Tj`/`TJ`, then writes page content. `change_page_content` can replace a multi-stream `/Contents` with a newly created stream [S10]. These helpers are not the proposed editing API.

Use explicit object access and a checked, provenance-preserving patch layer. Retain stream boundaries and the original unaffected decoded bytes where possible. Unknown operators, inline images, or ambiguous token boundaries must not silently vanish through decode/re-encode. A narrowly scoped tokenizer extension/adapter needs evaluation; no registry source should be modified.

### Existing integrity checks are valuable but need a different contract

[`export.rs`](../../crates/breeze-pdf/src/export.rs) already fingerprints input, validates source-object signatures, writes a temporary output, reopens it, and publishes only to a new destination. Structural editing necessarily changes some of those objects. Reuse the transaction principles, but validate an **explicit allowed change set** plus every untouched object; do not disable the annotation export invariant globally.

The current tile renderer has local changes beyond the historical full-page rendering assessment. This proposal retains those changes and does not reopen the renderer choice. Existing broad fidelity/performance gates remain unresolved.

## Proposed implementation design

### 1. Preserve source provenance and execution state

Build the editable page representation only on demand. Each target needs the document revision, page object, `/Contents` occurrence, stream object, operator/operand position, encoded byte/code range, resource scope, and full Form invocation path. Record pre-edit hashes so a stale selection cannot modify a different object after another transaction.

Interpret enough PDF state to establish geometry and side effects: `q/Q`, `cm`, `BT/ET`, text/line matrices, spacing, horizontal scaling, rise, font, render mode, clipping, graphics state, marked content, and Form matrices/BBoxes. State can continue across page content streams; retaining separate byte origins must not reset interpretation state incorrectly.

Use the existing renderer/interpreter as the preferred location to investigate provenance hooks, rather than joining independently extracted glyphs by proximity. Whether its current internals can emit sufficient origins without a large fork is an explicit validation question. `lopdf` owns output objects; keep engine-specific types behind `breeze-pdf` APIs.

### 2. Return explicit per-operation capabilities

A selected target should report capabilities and reasons: transform image, delete image usage, replace text using the existing encoding, change supported style, require another font, or unsupported. Treat uncertainty as unsupported, not as permission to use visual cover-up.

Start with isolated top-level image `Do` usages and uncomplicated horizontal text runs. Forms, vertical text, Type 3 fonts, text clipping, patterns, transparency groups, and ambiguous semantics require dedicated gates. Encrypted, signed, form-bearing, or otherwise deferred documents remain rejected according to the existing product policy.

### 3. Edit one usage and preserve following state

For image scale/rotate/delete, edit the selected `Do` usage. Scaling or deleting one use does not require rewriting the shared image pixels. Do not modify an earlier `cm` that also affects later content. Local transforms must respect the inherited matrix and clipping.

If a page stream or Form is shared, clone the minimum reference chain and resource dictionaries needed for that one invocation. Merely redirecting `/Fm1` in a resource dictionary can change several `/Fm1 Do` calls on the same page; give only the selected call a new resource name when required. Detect recursion and bound traversal.

For text, replace only the selected operand/span and preserve `TJ` adjustments and advances where required. A changed string length can move subsequent text even when its bytes are unchanged. Track and preserve both the text matrix and line matrix; **`q/Q` alone does not restore those matrices**. Initially reject cases where continuation state cannot be proven correct. Quoted text operators also have spacing/line side effects.

Marked-content `/ActualText`, tags, clipping text, and overlapping objects can invalidate a naive replacement. Update associated semantics only when the association is proven; otherwise return unsupported. Successful extraction alone does not prove that the correct object was changed.

### 4. Treat fonts and layout as their own engineering work

The mapping is not simply Unicode ↔ bytes. Distinguish character codes, CIDs, glyph IDs, font encodings/CMaps, `CIDToGIDMap`, and `ToUnicode`. Extraction mappings may be absent, incorrect, many-to-one, or represent ligatures. An embedded subset may not contain the glyphs for new input.

Prefer the existing embedded font only when every replacement character/cluster has a verified usable mapping and metrics. Otherwise offer an explicit, embeddable replacement font and preview the resulting layout. Do not silently stretch text, invent glyphs, or simulate bold/italic. Select an actual face when available.

For a later new-font path, evaluate the already-present Rust ecosystem components (`fontdb`, `ttf-parser`/`skrifa`, `harfrust` or `rustybuzz`, and `subsetter`) by concrete responsibility. Presence in `Cargo.lock` does not establish a public Breeze API or tested editor pipeline. Shaping produces positioned glyphs; subsetting and writing coherent PDF font resources, widths, encoding, and Unicode extraction remain separate tasks. Keep direct dependency choices/version changes out of the research phase.

Begin with fixed-position runs, then explicitly selected lines/blocks. Wrapping and alignment operate only inside the selected block; no implicit page-wide Word-style reflow. CJK coverage must be a first-class acceptance track, not something inferred from an ASCII demo. When a replacement exceeds a supported box, report overflow and let the user choose supported layout/font changes.

### 5. Use native editing transactions and real document previews

Suggested interaction: enter Edit → show supported object bounds → select a target → contextual properties → native text input or image handles → one undoable commit. Keep technical object identifiers out of the ordinary UI. Give concise Chinese explanations for unsupported operations and missing glyphs.

Text composition/caret state belongs to the native UI, with tested IME and focus behavior. Preview the actual edited draft through the PDF backend with coalescing and revision checks. Do not implement the preview/final output as an opaque rectangle over unchanged text. Dragging can display geometry handles before committing a transform.

Keep annotation history and content-edit history distinct. Add a document content revision to affected metadata, search, glyph, and tile cache identities; a source-file fingerprint alone cannot distinguish unsaved edits. Bound work and discard stale previews. Invalidate affected pages/resource users instead of the entire document.

Save As captures an immutable edit revision, validates the declared mutations and untouched objects, reopens the PDF, checks source fingerprint again, then publishes without clobbering. A saved edited copy has a new fingerprint. Do not automatically replay the old sidecar against it or imply that exporting/saving content saved annotations. Annotation reassociation and combined export need explicit, tested semantics.

## Why this route is preferable under the existing constraints

It preserves the reader investment, reuses an already integrated Rust writer, and makes the difficult requirements explicit rather than hiding them behind a high-level API. A modest first release can support actual edits on a declared subset while explaining unsupported files honestly.

The cost is ownership of provenance, font encoding, selected-instance mutation, paragraph inference, native editing behavior, and long-term regression coverage. Broad commercial-editor parity is not a small follow-up. Renderer correctness also remains a prerequisite for trustworthy on-screen edit previews. The next phase should measure those risks before promising coverage or schedules.

## Primary sources

Links to moving product documentation are access-date observations. Source links below pin the inspected versions where available; the pinned `lopdf` documentation is the project's actual 0.45.0 dependency, not its moving main branch.

- **[S1]** [Quark official PDF editing help](https://www.quark.cn/documents/help/pdf-editing): standard/scanned modes and advertised interaction. Engine/supplier and save internals are not disclosed on this page.
- **[S2]** [PDFium `fpdf_edit.h`](https://github.com/chromium/pdfium/blob/a84323421e94f484faca52dd9d027934eba42ab8/public/fpdf_edit.h) and [license](https://github.com/chromium/pdfium/blob/a84323421e94f484faca52dd9d027934eba42ab8/LICENSE): `FPDFPage_RemoveObject`, `FPDFPageObj_Transform`, `FPDFText_SetText`, `FPDFText_SetCharcodes`, `FPDFPage_GenerateContent`.
- **[S3]** [MuPDF README/license](https://github.com/ArtifexSoftware/mupdf/blob/d0f5d7f37cd0dceabc88c6bd8b9080a5cb951530/README) and [PDF object API](https://github.com/ArtifexSoftware/mupdf/blob/d0f5d7f37cd0dceabc88c6bd8b9080a5cb951530/include/mupdf/pdf/object.h). The hosted API documentation request was blocked; the cited public headers were read.
- **[S4]** [PDFBox `RemoveAllText.java`](https://github.com/apache/pdfbox/blob/c4d556abc9d5f0cbc486d459c84682321dd38ff4/examples/src/main/java/org/apache/pdfbox/examples/util/RemoveAllText.java) and [official FAQ](https://pdfbox.apache.org/3.0/faq.html): stream parsing/writing, resource traversal, encoding/reading-order caveats.
- **[S5]** [LibreOffice PDF import README](https://github.com/LibreOffice/core/blob/342dabb28d51986e8bae52fd93b17df2874ac84b/sdext/source/pdfimport/README.md): import architecture and documented limitations. Only the relevant implementation statements are relied upon, not every general claim in that README.
- **[S6]** [ONLYOFFICE PDF product](https://www.onlyoffice.com/pdf-editor.aspx), [core README](https://github.com/ONLYOFFICE/core/blob/3250a848ee4ef20c2fb8c38dc86350ec579124b8/Readme.md), [core license](https://github.com/ONLYOFFICE/core/blob/3250a848ee4ef20c2fb8c38dc86350ec579124b8/LICENSE), [C++ PDF editor](https://github.com/ONLYOFFICE/core/blob/3250a848ee4ef20c2fb8c38dc86350ec579124b8/PdfFile/PdfEditor.cpp), and [JS PDF API](https://github.com/ONLYOFFICE/sdkjs/blob/72b0421c0bbf9d01eed9cf14834ae47eb2df1b50/pdf/api.js).
- **[S7]** Stirling PDF [developer guide](https://github.com/Stirling-Tools/Stirling-PDF/blob/302367dc851716e4f0cd34955f8aea54792faec8/DeveloperGuide.md), [build](https://github.com/Stirling-Tools/Stirling-PDF/blob/302367dc851716e4f0cd34955f8aea54792faec8/build.gradle), [frontend dependencies](https://github.com/Stirling-Tools/Stirling-PDF/blob/302367dc851716e4f0cd34955f8aea54792faec8/frontend/package.json), [root license](https://github.com/Stirling-Tools/Stirling-PDF/blob/302367dc851716e4f0cd34955f8aea54792faec8/LICENSE), and [`engine/` license](https://github.com/Stirling-Tools/Stirling-PDF/blob/302367dc851716e4f0cd34955f8aea54792faec8/engine/LICENSE). The latter is one directory-specific license, not evidence that all PDF editing runs there.
- **[S8]** [PDF.js](https://github.com/mozilla/pdf.js) and [`AnnotationEditor`](https://github.com/mozilla/pdf.js/blob/ef62f31f2486d7cf2c3854b2db4520b9ca389b2f/src/display/editor/editor.js).
- **[S9]** [pdf-lib limitations](https://github.com/Hopding/pdf-lib/blob/93dd36e85aa659a3bca09867d2d8fac172501fbe/README.md#limitations).
- **[S10]** [`lopdf 0.45.0`](https://docs.rs/lopdf/0.45.0/lopdf/), [`content.rs`](https://docs.rs/crate/lopdf/0.45.0/source/src/content.rs), [`parser_aux.rs`](https://docs.rs/crate/lopdf/0.45.0/source/src/parser_aux.rs), and [`processor.rs`](https://docs.rs/crate/lopdf/0.45.0/source/src/processor.rs). Source inspection used the installed 0.45.0 registry files as well as upstream documentation.
- **[S11]** [PDF Oxide upstream `save_page`](https://github.com/yfedoseev/pdf_oxide/blob/ad49c4cb3638dc950a29350ef882161372fde473/src/editor/document_editor.rs#L4427), compared with Breeze's vendored 0.3.78.
- **[S12]** [pdf-rs README](https://github.com/pdf-rs/pdf/blob/5dea68f36a26efab80e15eeb180de0966a578331/README.md).
- **[S13]** [Hayro](https://github.com/LaurenzV/hayro/blob/dc151e07b8b0783eb5b3635d7c9c877346b770ce/README.md), [`hayro-write` scope](https://github.com/LaurenzV/hayro/blob/dc151e07b8b0783eb5b3635d7c9c877346b770ce/hayro-write/README.md), and [extraction API](https://github.com/LaurenzV/hayro/blob/dc151e07b8b0783eb5b3635d7c9c877346b770ce/hayro-write/src/lib.rs).
- **[S14]** [Krilla README](https://github.com/LaurenzV/krilla/blob/bdefa1037d322762a037dacd8976bcc0bada8a2f/README.md): generation, glyph APIs, font subsetting, scope, and license.
- **[S15]** [Apryse official content-editing guide](https://docs.apryse.com/web/edit-page-content/text-edit): UI/API behavior, content-edit history, WASM loading, and separate add-on license. Read through the browser; direct Markdown retrieval returned HTTP 403.
