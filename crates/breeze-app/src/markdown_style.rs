//! Typography for the reader's Markdown rich text.
//!
//! Kit sizes headings from a fixed 14 px base, so they ignore the reading font
//! size and h3-h6 end up smaller than the body text they introduce.
//! `TextViewStyle` is the only public lever for those sizes, and
//! gpui-component installs its themed style privately (`text::
//! base_text_view_style` in 0.7.1), so this module rebuilds that same mapping
//! and changes only the scale. Keep it in step with the pinned Kit release.
use crate::*;
use gpui_kit::base::text::TextViewStyle;

/// Heading sizes as a multiple of the body size, for levels 1-6. The last two
/// stay at body size, so a heading is never smaller than the text below it.
const HEADING_SCALE: [f32; 6] = [1.6, 1.4, 1.25, 1.1, 1.0, 1.0];

/// Code reads slightly smaller than prose, as in a printed listing.
const CODE_SCALE: f32 = 0.85;

/// The size a heading of `level` renders at, clamped to the six levels that
/// exist.
pub(super) fn heading_size(level: u8, body: f32) -> f32 {
    let index = usize::from(level.clamp(1, 6) - 1);
    body * HEADING_SCALE[index]
}

/// The size a fenced or indented code block renders at.
pub(super) fn code_size(body: f32) -> f32 {
    body * CODE_SCALE
}

/// The style one reader block's text view renders with.
///
/// The colours repeat what gpui-component derives from the active theme, so a
/// block keeps the reader's palette; only the heading scale, the code size and
/// the paragraph rhythm follow the reading font size.
pub(super) fn text_style(cx: &App, body: f32) -> TextViewStyle {
    let theme = Theme::global(cx).clone();
    let radius = theme.semantic_tokens().radius.md;
    let mut rounded = StyleRefinement::default();
    rounded.corner_radii.top_left = Some(radius.into());
    rounded.corner_radii.top_right = Some(radius.into());
    rounded.corner_radii.bottom_left = Some(radius.into());
    rounded.corner_radii.bottom_right = Some(radius.into());
    TextViewStyle::default()
        .with_foreground(theme.foreground)
        .with_muted_foreground(theme.muted_foreground)
        .with_link(theme.link)
        .with_selection(theme.selection)
        .with_border(theme.border)
        .with_code_background(theme.muted)
        // One em of body text between blocks, matching the reading size.
        .with_paragraph_gap(rems(body / 16.))
        .with_heading(move |level| {
            StyleRefinement::default().text_size(px(heading_size(level, body)))
        })
        .with_code_block(rounded.clone().text_size(px(code_size(body))))
        .with_table(rounded)
        .with_table_head(
            StyleRefinement::default()
                .bg(theme.table_head)
                .text_color(theme.table_head_foreground),
        )
        .with_inline_code(HighlightStyle {
            background_color: Some(theme.accent),
            ..Default::default()
        })
        .with_dark(theme.is_dark())
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui_kit::base::TextViewState;

    #[test]
    fn headings_are_never_smaller_than_the_body_text() {
        for level in 1..=6u8 {
            let size = heading_size(level, 18.);
            assert!(size >= 18., "h{level} is {size} px, body is 18 px");
            // The reading size is adjustable, so the guarantee holds at both
            // ends of the reader's 12-32 px range.
            for body in [12., 18., 24., 32.] {
                assert!(heading_size(level, body) >= body, "h{level} at {body} px");
            }
        }
    }

    #[test]
    fn heading_sizes_follow_the_reading_size_and_level() {
        let near = |left: f32, right: f32| (left - right).abs() < 0.01;
        assert!(near(heading_size(1, 18.), 28.8));
        assert!(near(heading_size(3, 18.), 22.5));
        assert!(near(heading_size(5, 32.), 32.));
        assert!(heading_size(1, 18.) > heading_size(2, 18.));
        assert!(heading_size(2, 18.) > heading_size(3, 18.));
        assert!(heading_size(3, 18.) > heading_size(4, 18.));
        assert_eq!(heading_size(0, 18.), heading_size(1, 18.));
        assert_eq!(heading_size(9, 18.), heading_size(6, 18.));
    }

    #[test]
    fn code_follows_the_reading_size() {
        assert!((code_size(18.) - 15.3).abs() < 0.01);
        assert!(code_size(32.) > code_size(18.));
    }

    /// The style only helps if the reader actually lays a block out with it,
    /// so this renders a real document and measures the heading and the body.
    #[gpui_kit::test]
    fn reader_headings_grow_with_the_reading_size(cx: &mut TestAppContext) {
        use gpui_kit::test::TestWindowExt;
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("headings.md");
        std::fs::write(
            &path,
            "# Title\n\nBody text with 中文.\n\n## Section\n\nMore body text here.\n",
        )
        .unwrap();
        let (handle, view) = crate::ui_tests::app(cx);
        cx.update_window(handle, |_, _, cx| {
            view.update(cx, |view, cx| {
                view.books[0].source = Source::File(path.clone());
                view.books[0].title = "headings.md".into();
                view.load(cx);
            });
        })
        .unwrap();
        cx.run_until_parked();
        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            window.render_frame(cx);
            assert!(
                view.read(cx).reflow.markdown.is_some(),
                "the Markdown document did not load: {}",
                view.read(cx).message
            );
            let heading = window.find(("markdown-block", 0usize)).bounds();
            let body = window.find(("markdown-block", 1usize)).bounds();
            assert!(
                heading.size.height > body.size.height,
                "h1 {heading:?} is not taller than the body block {body:?}"
            );
            // The reading size still scales both, so a larger size must open a
            // taller heading rather than a fixed one.
            view.update(cx, |view, cx| view.resize_reflow_text(1, cx));
            window.render_frame(cx);
            let larger = window.find(("markdown-block", 0usize)).bounds();
            assert!(
                larger.size.height > heading.size.height,
                "the heading kept its height after the reading size grew: {larger:?}"
            );
        })
        .unwrap();
    }

    /// The reading model's HTML is only useful if GPUI Kit turns it into the
    /// text the parser meant. This pins a document-wide reference, inline
    /// styles, a table and a footnote label together, which is what the mdast
    /// projection replaced the pulldown-cmark event stream with.
    #[gpui_kit::test]
    fn mdast_html_renders_as_parsed_text(cx: &mut TestAppContext) {
        use breeze_reflow::markdown::Document;
        cx.update(gpui_kit::init);
        let source = "## 标题\n\nsee [label][ref] and **bold** and `code`\n\n| 甲 | 乙 |\n|---|---|\n| 1 | 2 |\n\n脚注[^a]\n\n[ref]: https://example.com\n\n[^a]: 脚注正文\n";
        let document = Document::parse(source).expect("parses");
        let html: String = document
            .blocks
            .iter()
            .map(|block| block.html.as_ref())
            .collect();
        let state = cx.update(|cx| cx.new(|cx| TextViewState::html(&html, cx)));
        cx.run_until_parked();
        let rendered = cx.update(|cx| state.read(cx).rendered_text().as_str().to_owned());
        for expected in [
            "标题",
            "see label and bold and code",
            "甲",
            "乙",
            "脚注正文",
        ] {
            assert!(
                rendered.contains(expected),
                "{expected:?} is missing from {rendered:?}"
            );
        }
        assert!(
            !rendered.contains("[ref]"),
            "the reference label resolved instead of showing its source: {rendered:?}"
        );
    }

    /// The reader hands a fence's own source to the Markdown parser, so this
    /// pins what that parser makes of the slice: code, not algorithm text.
    #[gpui_kit::test]
    fn a_fence_slice_parses_as_code(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        let state =
            cx.update(|cx| cx.new(|cx| TextViewState::markdown("```rust\nfn main() {}\n```", cx)));
        cx.run_until_parked();
        let rendered = cx.update(|cx| state.read(cx).rendered_text().as_str().trim().to_owned());
        assert_eq!(rendered, "fn main() {}");
    }

    /// A fence has to reach the reader as a code block, including one nested in
    /// a list item, where the HTML path drew it as inline-code runs.
    #[gpui_kit::test]
    fn reader_lays_fenced_code_out_as_code(cx: &mut TestAppContext) {
        use gpui_kit::test::TestWindowExt;
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("code.md");
        std::fs::write(
            &path,
            "Intro\n\n```rust\nfn main() {\n    let x = 1;\n}\n```\n\n- item\n  ```\n  nested\n  ```\n",
        )
        .unwrap();
        let (handle, view) = crate::ui_tests::app(cx);
        cx.update_window(handle, |_, _, cx| {
            view.update(cx, |view, cx| {
                view.books[0].source = Source::File(path.clone());
                view.books[0].title = "code.md".into();
                view.load(cx);
            });
        })
        .unwrap();
        cx.run_until_parked();
        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            window.render_frame(cx);
            let document = view
                .read(cx)
                .reflow
                .markdown
                .clone()
                .expect("the Markdown document did not load");
            assert!(
                document.blocks[1].markdown.is_some(),
                "the reader kept no Markdown for the top-level fence"
            );
            assert!(
                document.blocks[2].markdown.is_some(),
                "the reader kept no Markdown for the fence inside the list"
            );
            let intro = window.find(("markdown-block", 0usize)).bounds();
            let fence = window.find(("markdown-block", 1usize)).bounds();
            let item = window.find(("markdown-block", 2usize)).bounds();
            assert!(fence.size.height > intro.size.height, "{fence:?}");
            assert!(item.size.height > intro.size.height, "{item:?}");
        })
        .unwrap();
    }
}
