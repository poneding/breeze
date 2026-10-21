# Work package 30: GPUI Kit 0.8 and lopdf 0.46 upgrade

Planning baseline: October 21, 2026. Status: **complete pending platform
acceptance on Windows and Linux**.

## Scope

Upgrade the pinned native UI stack from GPUI Kit 0.7.1 to 0.8.0 and the
restricted annotation-copy writer from `lopdf` 0.45.0 to 0.46.0. Keep the
existing Markdown parser, renderer, and document model unchanged. This phase
does not consolidate Markdown rendering into a single whole-document
`TextView`; that remains a separate proposal.

## Compatibility changes

GPUI Kit 0.8 raises the GPUI snapshot from 0.3.8 to 0.3.9 and adds the optional
GPUI Fast backend. Breeze keeps the default upstream GPUI backend. The `lopdf`
writer API used by `breeze-pdf` compiles unchanged against 0.46.0.

The Popover now keeps Enter and Space inside its content for the focused
control instead of closing the popover with the trigger's Confirm binding.
The category editor handles an unmodified Enter directly and ignores shifted or
composed input. The language select test no longer performs a redundant
initial keyboard confirmation, because opening and changing language is
verified in the following step.

The 0.8 test harness carries the current window modifiers into synthetic wheel
events. The wheel-zoom regression releases the zoom modifier before asserting
plain-wheel scrolling, matching the platform sequence after a user lets go.

## Verification

Verified on Apple M1 Pro, macOS, in an isolated worktree based on `HEAD` plus
only this dependency phase:

- `cargo fmt --all -- --check`
- `cargo test --workspace --locked`
- `cargo clippy --workspace --all-targets --locked -- -D warnings`
- `cargo run --locked -p breeze-app` with `BREEZE_DATA_DIR` under `target/`

Native inspection confirmed the full theme gallery, keyboard language selection
(including the English-to-Chinese switch), and Enter confirmation in the
category popover. Windows and Linux runtime behavior remains unverified.

## Upstream provenance

The pinned GPUI Kit source is commit
`1676826d9d49b90f4d7b6808b69924f58796f6f1`. The 21 bundled theme files are
byte-identical to that commit's `themes/` directory.
