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
    for pixel in pixels.chunks_exact_mut(4) {
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
