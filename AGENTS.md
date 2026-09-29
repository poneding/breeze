# Breeze Contributor Guide

## Project status and authority

- Breeze is a native, cross-format reading application. PDF is the first backend, not the application-wide document model.
- The user accepted the compact prototype's general UI direction on September 29, 2026. This is design acceptance, not approval of production rendering, annotations, or structural editing.
- The current repository contains the prototype, its tests, and the specifications needed for subsequent work. Do not restore historical experiments or review artifacts without a concrete need.
- Read `docs/specs/README.md`, `00-product.md`, and `01-validation.md` before changing architecture. Read `docs/plans/README.md` before beginning a new implementation phase.
- Keep technical specifications in `docs/specs/` and implementation plans in `docs/plans/`. Separate confirmed requirements, proposals, implemented behavior, and unverified assumptions.
- Do not start sub-agents, commit, push, or publish unless the user explicitly requests it or an applicable instruction authorizes it.

## Language

- Write documentation in **English by default**, including README files, specifications, plans, and contributor instructions.
- Write all code comments and documentation comments in **English**. Explain intent, constraints, and trade-offs rather than restating code.
- Use English identifiers and commit messages.
- User-facing strings are separate from documentation: preserve the prototype's current Chinese UI unless a localization change is requested. Do not claim that the language selector implements complete localization.

## Non-negotiable product constraints

- Target macOS, Windows, and Linux. Rust bindings to system windowing, GPU, fonts, and input methods are allowed.
- Keep application logic, UI, and document processing in Rust. Do not introduce PDFium, MuPDF, Poppler, or a WebView/JavaScript reader as an application document engine.
- **Never simulate existing-content editing with white rectangles, overlaid replacement text, region erasure followed by insertion, or full-page rasterization.** If an operation cannot safely target the original object or drawing instance, report that it is unsupported.
- Do not use `pdf_oxide`'s high-level `set_text` / `save_page` path as the structural editor. The evaluated version uses geometric erasure and appended content rather than precise source-operator replacement.
- Annotations default to a sidecar file with explicit export to a new PDF. Content edits are separate transactions, not annotations disguised as replacement content.
- Defer OCR, encrypted/password documents, forms, digital signatures, and DRM. Do not bypass restrictions.
- Report performance or fidelity failures with measurements, impact, and options before changing architecture or lowering quality targets. No unapproved fallback engine.

## Stack and structure

- Rust 2024, a Cargo workspace, and GPUI Kit native components. Pin critical dependencies and commit `Cargo.lock`.
- `crates/breeze-core`: format-independent prototype navigation, modes, and zoom state; no GPUI or PDF-engine dependency.
- `crates/breeze-pdf`: read-only PDF adapter and a bounded background worker; keep engine types out of the UI.
- `crates/breeze-app`: application entry point and native UI. `appearance.rs` owns theme preferences and palettes; `commands.rs` owns bindings and command dispatch; `ui_tests.rs` exercises real components.
- `assets/samples`: the bundled project-authored sample used by the prototype and tests. Preserve required assets and third-party license notices.
- `.github/workflows/ci.yml` and `scripts/bundle-macos.sh`: build checks and local macOS review packaging, not proof of cross-platform runtime acceptance.
- Add crates only for existing responsibilities. Avoid empty layers, speculative plugins, generic backend frameworks, compatibility shims, or multiple-engine fallback systems.

## UI and design rules

- Prefer GPUI Kit Button, TabBar, Input, Switch, Resizable, TitleBar, and Tooltip instead of recreating native control behavior.
- Keep the UI compact and content-first. Do not reintroduce a branding area, promotional welcome cards, decorative sidebar footers, or unnecessary whitespace.
- Place document tabs inside the native TitleBar. Use icon buttons for Home, Open, Settings, and other global commands; preserve accessibility labels and keyboard focus.
- Use centered Outline/Thumbnails tabs. The navigation panel defaults to 208 logical pixels and resizes between 168 and 360; preserve its session width when collapsed.
- Keep theme preference separate from palette choice. Follow System is the default; warm white and dark gray are the default palettes. Light and dark palettes are independently selectable; green is optional.
- Subscribe to system appearance changes rather than polling. Theme changes affect application chrome, not the PDF page's original colors.
- Use native Tooltip with real Action bindings for shortcut display. Do not invent shortcuts or intercept text-input editing keys with page/zoom commands.
- Use a consistent icon family, restrained separators, and a 4-pixel spacing system. Keep narrow-window controls accessible through layout or scrolling.
- Clearly label previews, session-only settings, and unavailable features. Never show fictitious save, update, file-association, or editing success.

## Coding and data integrity

- Use rustfmt and Clippy. Keep modules cohesive; do not put all logic in `main.rs`.
- Forbid unsafe code in first-party crates. Propagate user-input and I/O errors with context; do not hide them with `unwrap` or `expect` outside tests.
- Keep filesystem access, PDF parsing/rendering/export, and image codecs off the UI thread. Bound pending jobs and discard stale results after navigation or document changes.
- Load pages on demand. Use explicit byte budgets for future caches; do not retain a texture for every page or rerender the entire document for one annotation.
- Make RGBA/BGRA, premultiplied alpha, PDF/display coordinates, CropBox, rotation, UserUnit, and device scale explicit.
- Structural edits must identify a content stream, operator, and resource usage. Isolate shared resources before changing one instance; preserve unrelated operations and validate saved output by reopening it.
- Future persistence requires temporary-file replacement and source fingerprint checks. Never overwrite the original by default or claim that an ordinary full rewrite securely erases old content.
- Necessary input limits, error reporting, and data-integrity checks are not optional fallback architecture.

## Verification

Run the following from the repository root:

```sh
cargo fmt --all -- --check
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo run --locked -p breeze-app
```

- UI changes require native visual inspection as well as unit/component tests. Report any locked-screen, platform, or tool limitation instead of marking unperformed checks as passed.
- Distinguish a CI configuration from a completed CI run, and an installed cross-compilation target from native runtime validation.
- Benchmark in Release with hardware, OS, dimensions, sample count, cold/warm conditions, p50/p95, and memory scope recorded. Library marketing numbers are not application measurements.
- Keep generated screenshots, test reports, bundles, and scratch outputs out of version control. Use `target/` for disposable local outputs.

## Git and commit convention

- Use **Conventional Commits**: `<type>(<optional-scope>): <imperative summary>`.
- Use lowercase types such as `feat`, `fix`, `refactor`, `docs`, `test`, `build`, `ci`, and `chore`.
- Keep each commit message to **one concise English subject line**, preferably at most 72 characters, with no trailing period. Do not add a body, generated summary, or attribution trailer unless explicitly required by the user.
- Group changes by logical purpose. Do not mix unrelated cleanup, behavior changes, or speculative refactors into one commit.
- Inspect the staged diff and run appropriate checks before committing. Use the configured Git identity; never invent an author or bypass hooks to force a commit.
- Commit source, required assets, specifications, plans, and the lockfile. Never commit build output, logs, screenshots, private documents, or application sidecars.
- Preserve user changes. Do not amend, reset, rebase, force-push, create a remote, or publish without explicit authorization.
