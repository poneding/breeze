# Work package 24: EPUB and Markdown

Authorized October 15, 2026. See the [assessment](../specs/12-reflowable-reading.md).

1. **Assessment:** inspect dependencies and current PDF assumptions, document
   format controls, native locations, integrity limits and benchmark risks.
2. **Document adapters:** add format-independent capabilities and locations,
   bounded EPUB package/layout and Markdown parsing, fixtures and Release probes.
   Verify adapter behavior before committing this phase.
3. **Native reader integration:** connect opening, independent sessions, native
   content, format-specific toolbar/panels/status, search, library and restoration.
   Test unsupported action guards, layout, tab changes and source failures.
4. **Acceptance checks:** run locked workspace tests, rustfmt, Clippy and an
   isolated native app. Record measured performance and remaining limitations,
   verify packaged file declarations, and commit the verified integration.

No remote publication is authorized. Preserve unrelated in-progress sidebar
width work. Generated samples, screenshots, benchmark reports and builds belong
under `target/`; authored fixture sources and test generators belong in source.
