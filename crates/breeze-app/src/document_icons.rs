use breeze_pdf::Source;
use gpui_kit::{
    App, Styled,
    assets::IconName,
    component::{Icon, Sizable, Theme},
    rgb,
};

pub(super) fn file_icon(source: &Source, cx: &App) -> Icon {
    // Extension colors identify files; they do not enable additional reading backends.
    let extension = match source {
        Source::Sample => Some("pdf"),
        Source::File(path) => path.extension().and_then(|value| value.to_str()),
    };
    let colors = match extension.map(str::to_ascii_lowercase).as_deref() {
        Some("pdf") => Some((0xe54b35, 0xff7862)),
        Some("epub") => Some((0x56852f, 0x9bc969)),
        Some("md" | "markdown") => Some((0x356dcc, 0x7eabf5)),
        _ => None,
    };
    let theme = Theme::global(cx);
    let color = colors.map_or(theme.muted_foreground, |(light, dark)| {
        rgb(if theme.is_dark() { dark } else { light }).into()
    });
    Icon::new(IconName::FileText).small().text_color(color)
}

pub(super) fn bookmark_icon(favorite: bool) -> Icon {
    if favorite {
        Icon::default().data(include_bytes!("../../../assets/icons/bookmark-filled.svg"))
    } else {
        Icon::new(IconName::Bookmark)
    }
}

/// An annotation-tool icon: a bundled catalog name or an application SVG.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum AnnotationIcon {
    Bundled(IconName),
    WavyLine,
}

impl From<IconName> for AnnotationIcon {
    fn from(name: IconName) -> Self {
        Self::Bundled(name)
    }
}

impl From<AnnotationIcon> for Icon {
    fn from(value: AnnotationIcon) -> Self {
        match value {
            AnnotationIcon::Bundled(name) => Icon::new(name),
            // One stroke of the Lucide `zodiac-aquarius` pair, centered in its
            // viewBox: the bundled `line-squiggle` reads as a loop, not a wave.
            AnnotationIcon::WavyLine => {
                Icon::default().data(include_bytes!("../../../assets/icons/wavy-line.svg"))
            }
        }
    }
}
