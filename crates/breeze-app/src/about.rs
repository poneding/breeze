//! Application identity: what this build is, where it comes from, and its update state.
use crate::*;
use gpui_kit::component::Icon;

const AUTHOR: &str = "Pone Ding<poneding@gmail.com>";
/// The tile is displayed at 64 logical pixels; decode once at a bounded size.
const LOGO_MAX_PIXELS: u32 = 160;
const LOGO_BYTES: &[u8] = include_bytes!("../../../assets/brand/breeze-icon.png");
/// Keeps the two credit labels in one left-aligned column.
const CREDIT_LABEL_WIDTH: f32 = 64.;

/// Decode the bundled brand tile into a bounded BGRA texture.
///
/// GPUI presents `RenderImage` frames as straight BGRA, matching its own still-image
/// decoder. The 1024 px master is downscaled once so the atlas keeps one small tile.
pub(super) fn brand_logo() -> Option<Arc<RenderImage>> {
    let decoded = image::load_from_memory(LOGO_BYTES).ok()?;
    let resized = decoded
        .resize(
            LOGO_MAX_PIXELS,
            LOGO_MAX_PIXELS,
            image::imageops::FilterType::Lanczos3,
        )
        .to_rgba8();
    let (width, height) = resized.dimensions();
    let mut pixels = resized.into_raw();
    for pixel in pixels.as_chunks_mut::<4>().0 {
        pixel.swap(0, 2);
    }
    let buffer = image::RgbaImage::from_raw(width, height, pixels)?;
    Some(Arc::new(RenderImage::new(vec![image::Frame::new(buffer)])))
}

impl Breeze {
    /// About is a single centered column: identity, update state, then credits.
    pub(super) fn about_page(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let p = self.p(cx);
        let version = env!("CARGO_PKG_VERSION");
        let content = column()
            .w_full()
            .max_w(px(home::PAGE_MAX_WIDTH))
            .gap_5()
            .items_center()
            .child(self.brand_tile(p))
            .child(
                column()
                    .gap_1()
                    .items_center()
                    .child(
                        div()
                            .text_size(px(20.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("Breeze"),
                    )
                    .child(
                        div()
                            .text_size(px(12.))
                            .text_color(p.muted)
                            .child(tf!("Version {}", version)),
                    ),
            )
            .child(
                column()
                    .gap_2()
                    .items_center()
                    .child(row().gap_3().child(self.update_check_button(cx)).when_some(
                        self.update_release_url(),
                        |row, url| {
                            row.child(
                                Button::new("open-update-release")
                                    .small()
                                    .label(tr!("View release"))
                                    .on_click(move |_, _, cx| cx.open_url(&url)),
                            )
                        },
                    ))
                    // Only a real result earns a line here; long failures wrap
                    // inside a bounded width and stay centered under the button.
                    .when_some(self.updates.message(), |column, message| {
                        column.child(
                            div()
                                .id("update-status")
                                .test_support()
                                .w_full()
                                .max_w(px(360.))
                                .text_center()
                                .text_size(px(12.))
                                .text_color(p.muted)
                                .child(message),
                        )
                    }),
            )
            .child(
                column()
                    .gap_3()
                    .items_start()
                    .child(about_credit(
                        "about-credit-author",
                        tr!("Author"),
                        div()
                            .id("about-author")
                            .test_support()
                            .text_size(px(12.))
                            .child(AUTHOR.to_string()),
                        p,
                    ))
                    .child(about_credit(
                        "about-credit-source",
                        tr!("Source code"),
                        Button::new("open-repository")
                            .ghost()
                            .small()
                            .icon(IconName::ExternalLink)
                            .label(updates::REPOSITORY.trim_start_matches("https://"))
                            .accessibility_label(tr!("Open the GitHub repository"))
                            .tooltip(tr!("Open the GitHub repository"))
                            .on_click(|_, _, cx| cx.open_url(updates::REPOSITORY)),
                        p,
                    )),
            );
        column()
            .id("about-body")
            .test_support()
            .flex_1()
            .min_h_0()
            .w_full()
            .items_center()
            .justify_center()
            .child(content)
    }

    fn brand_tile(&self, p: Palette) -> impl IntoElement {
        div()
            .id("brand-tile")
            .test_support()
            .w(px(64.))
            .h(px(64.))
            .flex_shrink_0()
            .flex()
            .items_center()
            .justify_center()
            .map(|tile| match self.brand_logo.as_ref() {
                Some(logo) => tile.child(
                    img(logo.clone())
                        .size(px(64.))
                        .object_fit(ObjectFit::Contain),
                ),
                None => tile.child(
                    Icon::new(IconName::BookOpen)
                        .size(px(32.))
                        .text_color(p.muted),
                ),
            })
    }
}

/// One credit row: a muted label and its value share a line and start at the
/// same left edge, so the two values line up in one column.
fn about_credit(
    id: &'static str,
    label: &str,
    value: impl IntoElement,
    p: Palette,
) -> impl IntoElement {
    row()
        .id(id)
        .test_support()
        .gap_3()
        .items_center()
        .child(
            div()
                .w(px(CREDIT_LABEL_WIDTH))
                .flex_shrink_0()
                .text_size(px(11.))
                .text_color(p.muted)
                .child(label.to_string()),
        )
        .child(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The bundled raster is the only brand asset loaded at runtime, so a broken
    /// export would ship unnoticed: a rasterizer that composites onto an opaque
    /// background turns the icon into a tile on a white square. Check that the
    /// decoded tile keeps the clear corner of its canvas, its charcoal tile, and
    /// its ivory pages.
    #[test]
    fn bundled_logo_keeps_its_transparent_margin_and_colours() {
        let logo = brand_logo().expect("bundled brand logo decodes");
        assert_eq!(logo.frame_count(), 1);
        let size = logo.size(0);
        assert_eq!(
            (size.width.0, size.height.0),
            (LOGO_MAX_PIXELS as i32, LOGO_MAX_PIXELS as i32)
        );
        let pixels = logo.as_bytes(0).expect("decoded frame pixels");

        // GPUI frames are BGRA. Sample points sit on the decoded canvas: the clear
        // corner outside the tile's rounded corner, the charcoal tile between the
        // top edge and the pages, the ivory left page, and the narrow spine
        // between the two pages.
        let at = |x: usize, y: usize| {
            let i = (y * LOGO_MAX_PIXELS as usize + x) * 4;
            [pixels[i], pixels[i + 1], pixels[i + 2], pixels[i + 3]]
        };
        assert_eq!(at(0, 0), [0, 0, 0, 0], "canvas corner stays transparent");
        assert_eq!(at(80, 34), [44, 44, 44, 255], "tile stays charcoal");
        assert_eq!(at(58, 80), [232, 241, 245, 255], "page stays ivory");
        let spine = at(80, 80);
        assert_eq!(spine[3], 255, "pages are opaque inside the tile");
        assert!(
            spine[..3].iter().all(|channel| *channel < 100),
            "spine stays dark, not page-coloured: {spine:?}"
        );
    }

    /// The Dock and a Windows taskbar draw the icon at the raster's own size, so a
    /// generous margin makes the application look smaller than the system icons
    /// beside it. macOS frames app icons on a 1024 px grid whose artwork covers
    /// 824 px, and the bundled raster has to stay on that grid instead of drifting
    /// back to a smaller tile.
    #[test]
    fn bundled_logo_fills_the_standard_icon_grid() {
        let logo = brand_logo().expect("bundled brand logo decodes");
        let pixels = logo.as_bytes(0).expect("decoded frame pixels");
        let span = LOGO_MAX_PIXELS as usize;

        // A 50% alpha threshold measures the tile's rounded square, which leaves
        // the canvas margin and the corners out of the span.
        let (mut min_x, mut min_y, mut max_x, mut max_y) = (span, span, 0, 0);
        for y in 0..span {
            for x in 0..span {
                if pixels[(y * span + x) * 4 + 3] >= 128 {
                    min_x = min_x.min(x);
                    min_y = min_y.min(y);
                    max_x = max_x.max(x);
                    max_y = max_y.max(y);
                }
            }
        }

        let expected = 824. / 1024. * span as f32;
        let (width, height) = ((max_x - min_x + 1) as f32, (max_y - min_y + 1) as f32);
        assert!(
            (width - expected).abs() <= 1.5 && (height - expected).abs() <= 1.5,
            "tile spans {width}x{height} px of {span}, expected {expected:.2} px"
        );
        assert_eq!(
            (min_x, min_y),
            (span - 1 - max_x, span - 1 - max_y),
            "the canvas keeps the tile centred"
        );
    }
}
