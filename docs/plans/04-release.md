# Stage 4: Native Platform and Release Acceptance

## Required work

- Validate macOS, Windows, and Linux native windows, titlebar dragging/controls, DPI, GPU, dialogs, IME, accessibility, and shortcuts.
- Finish explicit window-opening policies, OS file-open events/associations, and full UI localization. Do not claim persisted selector values implement these behaviors.
- Configure a real signed release source before adding update checks. No placeholder URL, fictitious update result, or silent installer execution.
- Produce platform packaging, license inventory, signing/notarization, and documented minimum versions.
- Run the agreed Release-mode rendering/fidelity corpus, startup/interaction benchmarks, memory budgets, and malformed-input checks.

## Implemented automation (not acceptance)

The user authorized tag-triggered packaging and publication. `.github/workflows/release.yml` validates SemVer and the workspace version, checks and builds macOS Apple Silicon/Intel, Windows x64, and Linux x64, then publishes all unsigned archives with checksums only after the full matrix succeeds. Prerelease tags are labeled accordingly. See [release operations](../releases.md).

This is workflow configuration, not evidence of a completed GitHub Actions run or native runtime acceptance. Signing/notarization, a complete audited license inventory, installers, and production acceptance remain pending.

## Existing blockers

The suspected standard-font substitution defect and historical 117 ms page-raster p95 against a 100 ms target remain unresolved. Native Windows/Linux acceptance has not been performed. Do not add an alternative engine or raise budgets without explicit review.

## Exit criteria

- Every release capability has a tested implemented path, a documented restriction, or a disabled UI with a truthful explanation.
- Persistence, crash/reopen behavior, source conflicts, save/export integrity, and input limits have automated regression tests.
- Native UI checks and performance/fidelity results identify the exact build, machine, corpus, and limitations.
- A user-accepted release candidate is distinguished from development builds and UI approval.
