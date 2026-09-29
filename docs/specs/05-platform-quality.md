# Platforms, Performance, and Quality Gates

## Platform baseline

Initial targets are macOS 15+ (ARM64/x64), Windows 10+ (x64/MSVC), and Ubuntu 24.04 (x64, Wayland and X11). Confirm exact versions/architectures before making release commitments.

Use native GPUI Kit platform paths. Windows requires the MSVC/SDK build toolchain; Linux requires its font/window-system libraries and a working Vulkan driver. One Rust codebase does not mean zero system dependencies.

The repository's CI defines build, test, lint, and formatting checks. A workflow file is not evidence of a successful remote run, and cross-target compilation is not proof of GPU/window/IME behavior. Native acceptance is required on every release platform.

Default-file association must follow OS-supported user-choice flows. Update checking, installer packaging, signing/notarization, and update authenticity are separate platform/release gates, not a checkbox-only implementation.

## Proposed performance budgets

These are future acceptance targets, **not measured guarantees**. Establish an agreed reference machine and corpus before implementation decisions depend on them.

| Metric | Target |
|---|---|
| Cold application shell | p95 below 1 second from launch to first frame |
| Ordinary PDF open to readable page | p50 at most 300 ms, p95 at most 1.2 seconds, including I/O, parsing, rasterization, upload, and presentation |
| UI interaction/scrolling | p95 frame time at most 16.7 ms for 60 Hz; 120 Hz is an additional target |
| Cached-page feedback | Below 50 ms, without blocking input on background rendering |
| Ordinary page rasterization | 144-DPI p95 at most 100 ms; classify complex scans separately |
| Future image caches | CPU 128 MiB, GPU 128 MiB, thumbnails 16 MiB, with explicit eviction |
| Prototype output raster | At most four megapixels; disclose resolution limitations |
| Ordinary reading working set | Target RSS at most 350 MiB, with decoder/intermediate peaks measured separately |
| Idle | No continuous repainting or polling; near-idle CPU |

A renderer output-pixel limit does not bound total memory. A benchmark average must not conceal expensive tail cases. Existing historical misses remain open in [engine assessment](01-validation.md).

## Escalation rule

When a budget or fidelity requirement fails, report the input identity, workload, hardware/build profile, dimensions, p50/p95/peak memory, reproduction steps, user impact, likely cause, and optimization costs/options. Obtain review before lowering the target, changing architecture, or accepting a different engine.

The read-only UI prototype may remain available for design review while an engine fails production gates. Do not present that as a passed engine evaluation.

## Verification layers

1. **Unit tests:** navigation boundaries, zoom, independent session state, appearance resolution, coordinate transforms, and eventual edit commands.
2. **Adapter tests:** real loading/rendering, errors, pixel format, bounded scheduling, cancellation/stale results, and eventual cache budgets.
3. **Component tests:** real control event dispatch, keyboard context, input submission, sidebar dragging/limits, titlebar actions, and palette selection.
4. **Corpus checks:** CJK, subset/CID/Type 3 fonts, transparency, color spaces/ICC, JPEG2000/JBIG2, scans, complex graphics, rotations, cropped/nonzero-origin pages, large documents, and malformed inputs.
5. **Save round trips:** independent parsing/viewing; text semantics, unrelated-object preservation, image instances, pixel differences, repeated saves, undo/redo, and failure integrity.
6. **Native platform checks:** windows, controls, DPI, GPU, file dialogs, theme events, IME, accessibility, and packaging.
7. **Performance:** fixed inputs and resolutions, Release builds, adequate sample counts, cold/warm separation, and explicit measurement scope.

Do not infer compatibility percentages from a few fixtures. Do not label a process-first render as a disk-cold read unless the file-cache conditions justify it.

## Untrusted-document boundary

Do not execute PDF JavaScript, embedded scripts, or automatically opened links. Bound work queues and raster sizes; separately evaluate decoder allocations and recursion/resource consumption. Report malformed inputs explicitly.

The prototype has not passed fuzzing or a process-isolation assessment. Use trusted samples. Whether to add a dedicated document process is a measured design decision, not an excuse for speculative infrastructure; basic error boundaries and file integrity remain mandatory.
