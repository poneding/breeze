# Library interaction refinement

Requested October 1, 2026. Implement and verify each item separately:

1. Use a Kit Popover for creating and editing categories, with validation and dismissal.
2. Give category/document rows one shared interactive surface and independent secondary-button hover/click behavior.
3. Center grid-card metadata and reveal card actions on hover.
4. Use a restrained Kit Empty for empty document results.

Preserve saved documents, category memberships/colors, native control behavior, and the compact chrome dimensions. Native macOS review uses isolated data under `target/`; Windows/Linux acceptance remains separate.

## Verification record

- Item 1: Create/edit forms use controlled Kit Popovers with name/color controls. Save/cancel/Escape and invalid input are covered by component tests; the sidebar retains its layout. Formatting, 91 workspace tests, Clippy, and native macOS Popover inspection passed.
