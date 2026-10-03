# Work Package 13: Code Review Remediation

Authorized on October 3, 2026. Execute phases in order, committing each completed and verified phase separately. Preserve existing user changes. This checklist records planned work and executed evidence, not production acceptance.

## Ordered checklist

- [x] **0 — Restore verification (R06).** Register the library container observation and verify the empty-state layout contract. Run the complete workspace checks.
- [x] **1 — Transactional sidecar Save As (R01/R02).** Preserve the previous association and saved baseline until publication succeeds. Reject unloaded histories; handle cancellation, failure, concurrent edits and document changes. Add permanent regressions.
- [ ] **2 — Annotation recovery and gestures (R04/R12/F07).** Separate transient retry from source conflicts. Provide recovery copies retaining the original fingerprint, explicit recovery/discard, and safe closing. Cancel gestures on mode transitions; never weaken fingerprint checks.
- [ ] **3 — Rendering correctness (R03/R05).** Surface failed drawing operations instead of successful incomplete pages. Apply or explicitly reject unsupported UserUnit. Verify reader/export agreement and independent output.
- [ ] **4 — Library, search, accessibility (R07–R11).** Correct history labels, match counts/snippets and navigation, accessible names, and targeted cover refresh. Define Recent versus All before changing its semantics.
- [ ] **5 — Replace prototype interactions (F02/F03/F04).** Implement language switching and native file-open delivery. Remove dead preview branches and label unavailable capabilities honestly. Update checking requires a real endpoint/channel decision; do not invent one.
- [ ] **6 — Annotation editing and navigation (F05/F06).** Logical selection, named/page-position destinations, saved annotation manipulation, and validated imported annotations without duplicate appearances.
- [ ] **7 — Precise original-content editing (F01).** Follow work package 12's source-identity, isolated image-use, limited text-editing, and save/reopen gates. No erasure/reinsertion, cover-up, or rasterized editing.
- [ ] **8 — Security, resources and release (V01–V05).** Working dependency auditing, representative Release measurements, decoder/resource limits, malformed-input tests, native three-platform acceptance, licenses, signing and packaging.

## Acceptance discipline

- Run formatting, workspace tests, Clippy with warnings denied and isolated native startup for implementation phases. Add focused integrity tests; do not skip existing failures to claim success.
- UI behavior changes require native interaction/visual checks against the tested revision. Record tool/platform limitations explicitly.
- Keep source PDFs unchanged. Use scratch files and isolated `BREEZE_DATA_DIR` under `target/`.
- Record phase outcomes here and in `STATUS.md`; generated logs, snapshots and screenshots remain under `target/`.
- Platform acceptance, representative fidelity/performance, real release endpoints, signing credentials and distribution choices remain separate gates. This plan does not waive the structural-editing contract.

## Execution record

Phase 0 completed: the missing `home-content` observation is registered. Formatting, the full workspace suite (103 passed, three existing ignored probes), Clippy with warnings denied, and isolated native startup passed on macOS. This instrumentation-only change does not change the rendered UI. Phase 1 completed next: Save As commits its destination only after a successful non-clobbering write. It rejects unloaded/source-conflicted histories, preserves the previous association on failure/cancellation, serializes dialogs/writes, and rejects stale picker results. Five permanent regressions cover failures, cancellation, readiness, concurrent edits and stale sources. Formatting, 108 workspace tests (three existing probes ignored), Clippy and isolated macOS startup passed. Native Save As cancellation and success were exercised; the saved association and exact annotation data were read back and the PDF hash was unchanged. Phase 2 is next.
