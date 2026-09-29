# Implementation Planning Status

## Accepted baseline

On September 29, 2026, the user confirmed that the compact prototype's general UI direction matches the intended product. The latest request authorizes repository cleanup, English documentation/comments, Git initialization, and logical single-line commits.

This acceptance does **not** resolve PDF rendering fidelity/performance risks or approve a specific production editing engine. It is not authorization to implement additional product features during repository cleanup.

## Current scope

Keep the native prototype, required assets, tests, build support, and current specifications. Remove superseded review notes and disposable experiments rather than presenting them as active plans.

No detailed production implementation plan has been authored yet. The next planning request can split work into focused documents for:

- Reading-engine fidelity, rendering scheduling, and performance gates.
- Library/settings persistence and platform integration.
- Semantic text selection, annotations, sidecars, and PDF export.
- Precise structural text/image editing and save validation.

This list identifies planning topics, not an approved execution schedule.

## Requirements for future plans

Write plans in English under this directory. Each plan should identify scope, dependencies, concrete changes, tests, performance budgets, data-integrity risks, and review gates. Distinguish implementation tasks from unresolved feasibility work.

Reference the current [specifications](../specs/README.md), especially the [engine assessment](../specs/01-validation.md) and [structural-editing contract](../specs/03-pdf-editing.md). Do not lower targets or introduce a prohibited fallback engine without explicit review.
