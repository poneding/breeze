# Library interaction refinement

Requested October 1, 2026. Implement and verify each item separately:

1. Use a Kit Popover for creating and editing categories, with validation and dismissal.
2. Give category/document rows one shared interactive surface and independent secondary-button hover/click behavior.
3. Center grid-card metadata and reveal card actions on hover.
4. Use a restrained Kit Empty for empty document results.

Preserve saved documents, category memberships/colors, native control behavior, and the compact chrome dimensions. Native macOS review uses isolated data under `target/`; Windows/Linux acceptance remains separate.

## Verification record

- Item 1: Create/edit forms use controlled Kit Popovers with name/color controls. Save/cancel/Escape and invalid input are covered by component tests; the sidebar retains its layout. Formatting, 91 workspace tests, Clippy, and native macOS Popover inspection passed.
- Item 2: Category rows now use a full-width Kit Button including their action region. Row hover stays visible over secondary controls; category/document actions use independent hover/active colors and stop click propagation. Formatting, 92 workspace tests, Clippy, and native row/Popover checks passed.
- Item 3: Grid titles, reading positions, and categories are centered. Action rows reserve their space but appear only while the card is hovered. Geometry, hover transitions, and favorite isolation are tested. Formatting, 93 workspace tests, Clippy, and native grid inspection passed.
