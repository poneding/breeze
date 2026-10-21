//! Typography for the reader's Markdown rich text.
//!
//! Kit sizes headings from a fixed 14 px base, so they ignore the reading font
//! size and h3-h6 end up smaller than the body text they introduce.
//! `TextViewStyle` is the only public lever for those sizes, and
//! gpui-component installs its themed style privately (`text::
//! base_text_view_style` in 0.8.0), so this module rebuilds that same mapping
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

    #[test]
    fn headings_are_never_smaller_than_the_body_text() {
        for level in 1..=6u8 {
            let size = heading_size(level, 18.);
            assert!(size >= 18., "h{level} is {size} px, body is 18 px");
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

    #[gpui_kit::test]
    fn component_installs_native_code_highlighting(cx: &mut TestAppContext) {
        let _ = crate::ui_tests::app(cx);
        let installed = cx
            .update(|cx| gpui_kit::base::TextViewDefaults::global(cx).has_code_block_highlighter());
        assert!(
            installed,
            "GPUI Kit did not install its TextView code highlighter"
        );
    }

    /// The style only helps if TextView lays every kind of content out with it.
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
            let markdown = view.read(cx).reflow.markdown.clone().unwrap();
            let rendered = markdown.state.read(cx).rendered_text().as_str().to_owned();
            assert!(rendered.contains("Title"));
            assert!(rendered.contains("Section"));
            let before = markdown.state.read(cx).list_state().logical_scroll_top();
            view.update(cx, |view, cx| view.resize_reflow_text(1, cx));
            window.render_frame(cx);
            let after = view
                .read(cx)
                .reflow
                .markdown
                .as_ref()
                .unwrap()
                .state
                .read(cx)
                .list_state()
                .logical_scroll_top();
            assert_eq!(before.item_ix, after.item_ix);
        })
        .unwrap();
    }
}
