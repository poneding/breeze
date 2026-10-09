# Reader Tools and Document Details

The user requested these changes on October 9, 2026. This authorizes the UI workflow and independent annotation styling, not original-content editing or importing existing PDF annotations.

## Delivery phases

1. Use a filled Lucide bookmark for favorited documents in the library and reader, keeping normal button hover/press feedback and the existing favorite action and persistence.
2. Move search to the reader toolbar's right edge, followed by Favorite and a right-sidebar toggle. The optional right sidebar has Information and Annotations tabs; show current-document information and a document-wide annotation list with type filtering and page navigation. Move annotation/edit settings into interactive Kit hover cards for the corresponding tools, with at least the eight category color families for fixed annotation colors independent of application appearance. Keep save/error/history controls reachable and retain explicitly unavailable original-content editing. Verify tool-specific interactions, palette behavior, annotation navigation/filtering, compact geometry, and pointer travel into settings with component tests and native inspection.

Track actual completion and limits in [STATUS](STATUS.md). Preserve the 34 px titlebar, 38 px toolbar, 32 px status bar, source-document integrity, and bounded background work. Keep review state and captures below `target/`; local commits contain only the verified phase and preserve unrelated working changes.
