//! Application identity: what this build is, where it comes from, and its update state.
use crate::*;
use gpui_kit::component::Icon;

const AUTHOR: &str = "poneding";
/// The tile is displayed at 56 logical pixels; decode once at a bounded size.
const LOGO_MAX_PIXELS: u32 = 160;
const LOGO_BYTES: &[u8] = include_bytes!("../../../assets/brand/breeze-icon.png");

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
    pub(super) fn about_page(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let p = self.p(cx);
        let version = env!("CARGO_PKG_VERSION");
        column()
            .id("about-body")
            .test_support()
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .gap_5()
            .child(
                row().gap_4().child(self.brand_tile(p)).child(
                    column()
                        .gap_1()
                        .child(
                            div()
                                .text_size(px(18.))
                                .font_weight(FontWeight::SEMIBOLD)
                                .child("Breeze"),
                        )
                        .child(
                            div()
                                .text_size(px(12.))
                                .text_color(p.muted)
                                .child(tf!("版本 {}", version)),
                        ),
                ),
            )
            .child(
                row()
                    .gap_3()
                    .child(self.update_check_button(cx))
                    .child(
                        div()
                            .id("update-status")
                            .test_support()
                            .flex_1()
                            .min_w_0()
                            .text_size(px(12.))
                            .text_color(p.muted)
                            .child(self.updates.message()),
                    )
                    .when_some(self.update_release_url(), |row, url| {
                        row.child(
                            Button::new("open-update-release")
                                .small()
                                .label(tr!("查看发布版本"))
                                .on_click(move |_, _, cx| cx.open_url(&url)),
                        )
                    }),
            )
            .child(
                column()
                    .gap_2()
                    .child(
                        row()
                            .gap_4()
                            .child(about_label(tr!("作者"), p))
                            .child(div().text_size(px(12.)).child(AUTHOR)),
                    )
                    .child(
                        row().gap_4().child(about_label(tr!("开源地址"), p)).child(
                            Button::new("open-repository")
                                .ghost()
                                .small()
                                .icon(IconName::ExternalLink)
                                .label(updates::REPOSITORY.trim_start_matches("https://"))
                                .accessibility_label(tr!("打开 GitHub 仓库"))
                                .tooltip(tr!("打开 GitHub 仓库"))
                                .on_click(|_, _, cx| cx.open_url(updates::REPOSITORY)),
                        ),
                    ),
            )
    }
    fn brand_tile(&self, p: Palette) -> impl IntoElement {
        div()
            .id("brand-tile")
            .test_support()
            .w(px(56.))
            .h(px(56.))
            .flex_shrink_0()
            .flex()
            .items_center()
            .justify_center()
            .map(|tile| match self.brand_logo.as_ref() {
                Some(logo) => tile.child(
                    img(logo.clone())
                        .size(px(56.))
                        .object_fit(ObjectFit::Contain),
                ),
                None => tile.child(
                    Icon::new(IconName::BookOpen)
                        .size(px(28.))
                        .text_color(p.muted),
                ),
            })
    }
}

fn about_label(label: &str, p: Palette) -> Div {
    div()
        .w(px(64.))
        .flex_shrink_0()
        .text_size(px(12.))
        .text_color(p.muted)
        .child(label.to_string())
}
