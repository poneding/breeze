# Stage 1: Durable Local Reader

## Scope

Replace session-only library/settings behavior with durable local state without changing the PDF engine. Keep the existing single-page renderer and add practical navigation/search. Adaptive rasterization and continuous/two-page layouts remain separate performance-sensitive work.

## Changes

1. Add a focused `breeze-storage` crate for atomic JSON persistence, file fingerprints, optimistic sidecar writes, and process locking. No database or migration framework.
2. Use the OS application-data directory by default; allow `BREEZE_DATA_DIR` for isolated testing/portable setups. Hold a state-directory lock and report a second writer explicitly.
3. Restore favorites, categories, independent palettes, theme preference, sidebar width, and per-document zoom/position. File restoration and position restoration are independent switches.
4. Load state in a worker. Coalesce pending snapshots, acknowledge save completion, and drain writes during explicit application shutdown. A failed/corrupt load must disable automatic overwriting until addressed.
5. Add library name filtering and removal of entries without deleting source files. Preserve recent ordering and avoid duplicate canonical file paths.
6. Add explicit, cancellable document search with bounded results, page destinations, and snippets. Search stays out of the render/input thread. No regex engine or whole-corpus indexing service.
7. Resolve page geometry and cache only bounded rendered output. Stale jobs must never become the active preview. Reject unsupported format/encrypted files visibly.

## Acceptance

- Restart with isolated test data and recover state; test both restoration switches independently.
- Corrupt JSON, read-only locations, another process holding the lock, and interrupted writes do not silently destroy state.
- Search can be cancelled and result clicks navigate to the correct page; an old search does not affect another document.
- Missing files produce errors without removing their library entries.
- Existing keyboard/input, resizable panel, theme, PDF, and core tests continue to pass.

## Performance gate

No claim that the existing renderer meets production targets. Avoid full-document work on page navigation. Record worker/output limits and keep UI responsive while hashing/searching. Any new cache must have a byte limit; do not infer engine working-set bounds from output dimensions.
