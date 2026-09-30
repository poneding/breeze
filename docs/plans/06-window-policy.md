# Work Package 06: Opening-Window Policies

Status: implementation and automated tests completed after this plan was written. An initial native smoke check exercised Fit Document; the final recheck after the origin-bounds correction was blocked by the locked Mac. Windows/Linux native checks remain pending. No OS file-association or updater changes are included.

## Behavior

- Default: keep the current window geometry.
- Maximized: maximize once after a newly opened document becomes readable; do not toggle an already maximized window.
- Fit Document: calculate a window size from the visible page bounds at nominal logical size plus current application chrome, clamp to the remaining display space at the current window origin and the existing minimum window size, and resize once. It does not change the document's zoom preference.
- A tab switch, page turn, search result, or cached-page refresh must not repeatedly resize the window.
- Apply the configured policy to the active restored document at startup, and to a newly opened/previously closed document, not every restored background tab.

## Code changes

1. Add pure policy/size calculations in `crates/breeze-app/src/window_policy.rs` with explicit logical-pixel units.
2. Keep one pending policy request tied to the reader load generation. Clear it on Home, failed load, document replacement, or application.
3. Defer native GPUI window changes until the accepted page result is available; never reenter a borrowed window from rendering.
4. Update Settings copy to distinguish implemented window behavior from unchanged page zoom.

## Tests

- Default produces no resize; maximized does not unmaximize an already maximized window.
- Fit dimensions are finite, bounded by the available display area, and respect minimums where the display permits them.
- Mixed page orientations and sidebar visibility produce appropriate sizes.
- Repeated page loads and tab switches do not reapply the policy.
- Native macOS smoke check; Windows/Linux native checks remain required by the release plan.

## Native QA correction

The first native check showed that sizing against the full display while preserving a centered window's origin could push the footer off screen. The calculation now uses the remaining space from that origin, including secondary-display coordinates. Restoring a maximized window waits for the native bounds event before fitting; it does not guess with a fixed delay or poll. Fullscreen windows retain their user-selected mode.
