# Specifications

This directory is the current design baseline, not a history of prototype reviews.

The user accepted the compact UI's general direction on September 29, 2026. Implementation was subsequently authorized on the same date. That acceptance does not validate the production PDF engine. See [delivery status](../plans/STATUS.md) for implemented capabilities, development restrictions, and outstanding gates.

| Specification | Purpose |
|---|---|
| [00 — Product](00-product.md) | Confirmed scope, boundaries, and unresolved decisions |
| [01 — Engine assessment](01-validation.md) | Dependency decisions and unresolved validation risks |
| [02 — Architecture](02-architecture.md) | Current crate boundaries, rendering, and future data model |
| [03 — Annotations](03-annotations.md) | Annotation mapping, sidecar persistence, and export integrity |
| [04 — UI design](04-design.md) | Accepted compact layout, appearance, and shortcuts |
| [05 — Platform and quality](05-platform-quality.md) | Platform assumptions, performance targets, and verification gates |
| [06 — Icons](06-icons.md) | Icon source, licensing, and function-to-icon mapping |

Implementation plans belong in [docs/plans](../plans/README.md). Do not treat target behavior in a specification as already implemented. Consult [AGENTS.md](../../AGENTS.md) for language, coding, validation, and commit rules.

Current concrete work packages are [annotation export](../plans/05-annotation-export.md), [opening-window policies](../plans/06-window-policy.md), and the [remaining-work backlog](../plans/07-remaining-work.md). Consult [delivery status](../plans/STATUS.md) for executed tests and outstanding native/release checks.

