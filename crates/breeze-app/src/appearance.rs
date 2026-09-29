//! User preference is distinct from the currently resolved platform appearance.
use gpui_kit::WindowAppearance;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ThemePreference {
    #[default]
    System,
    Light,
    Dark,
}
impl ThemePreference {
    pub const ALL: [Self; 3] = [Self::System, Self::Light, Self::Dark];
    pub fn index(self) -> usize {
        match self {
            Self::System => 0,
            Self::Light => 1,
            Self::Dark => 2,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::System => "跟随系统",
            Self::Light => "浅色",
            Self::Dark => "深色",
        }
    }
    pub fn next(self) -> Self {
        Self::ALL[(self.index() + 1) % 3]
    }
    pub fn resolve(self, appearance: WindowAppearance) -> bool {
        match self {
            Self::Light => false,
            Self::Dark => true,
            Self::System => matches!(
                appearance,
                WindowAppearance::Dark | WindowAppearance::VibrantDark
            ),
        }
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ColorScheme {
    #[default]
    Neutral,
    Sand,
    Blue,
    Forest,
    Violet,
}
impl ColorScheme {
    pub const ALL: [Self; 5] = [
        Self::Neutral,
        Self::Sand,
        Self::Blue,
        Self::Forest,
        Self::Violet,
    ];
    pub fn name(self, dark: bool) -> &'static str {
        match (self, dark) {
            (Self::Neutral, false) => "暖白 · 默认",
            (Self::Neutral, true) => "深灰 · 默认",
            (Self::Sand, false) => "米杏",
            (Self::Sand, true) => "暖墨",
            (Self::Blue, false) => "雾蓝",
            (Self::Blue, true) => "午夜蓝",
            (Self::Forest, false) => "浅松绿",
            (Self::Forest, true) => "深松绿",
            (Self::Violet, false) => "淡紫灰",
            (Self::Violet, true) => "暮紫",
        }
    }
    // background / panel / canvas / foreground / muted / border / accent / selected
    pub fn colors(self, dark: bool) -> [u32; 8] {
        match (self, dark) {
            (Self::Neutral, false) => [
                0xfaf9f6, 0xf2f1ee, 0xe7e6e2, 0x292b2e, 0x717378, 0xdadbd8, 0x49678a, 0xe6eaf0,
            ],
            (Self::Neutral, true) => [
                0x242528, 0x292a2e, 0x1c1d20, 0xe6e6e8, 0xa1a3aa, 0x3b3d43, 0x9ab5db, 0x353f50,
            ],
            (Self::Sand, false) => [
                0xfcf8f0, 0xf3ecdf, 0xe9e0d0, 0x443a30, 0x766858, 0xded2bf, 0x906238, 0xefe2cb,
            ],
            (Self::Sand, true) => [
                0x2b2724, 0x332e29, 0x211e1b, 0xeee6dd, 0xb0a296, 0x484039, 0xdbb58c, 0x4b3e30,
            ],
            (Self::Blue, false) => [
                0xf6f8fb, 0xecf0f6, 0xe0e6ef, 0x29384b, 0x5e6d80, 0xd4deeb, 0x416fa5, 0xe0eafa,
            ],
            (Self::Blue, true) => [
                0x202833, 0x26313e, 0x181f28, 0xe0e9f5, 0x96a9bf, 0x3a485a, 0x93bbed, 0x334a69,
            ],
            (Self::Forest, false) => [
                0xfafbf8, 0xf0f3ec, 0xe8ece5, 0x243b30, 0x5f7063, 0xd8e0d4, 0x386a50, 0xe3ecdf,
            ],
            (Self::Forest, true) => [
                0x1d2522, 0x232d28, 0x151d19, 0xe7ece7, 0x96a49a, 0x354239, 0x8dbba1, 0x34493d,
            ],
            (Self::Violet, false) => [
                0xf9f7fc, 0xf0ecf5, 0xe6e0ed, 0x3c334b, 0x70617f, 0xdcd4e7, 0x7d619e, 0xeadff5,
            ],
            (Self::Violet, true) => [
                0x28232f, 0x302a38, 0x201c27, 0xebe3f4, 0xa99ab8, 0x463c53, 0xc0a7dc, 0x49385e,
            ],
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn system_appearance_resolves() {
        assert!(!ThemePreference::System.resolve(WindowAppearance::Light));
        assert!(ThemePreference::System.resolve(WindowAppearance::Dark));
        assert!(ThemePreference::System.resolve(WindowAppearance::VibrantDark));
    }
    #[test]
    fn explicit_preference_ignores_system() {
        assert!(ThemePreference::Dark.resolve(WindowAppearance::Light));
        assert!(!ThemePreference::Light.resolve(WindowAppearance::Dark));
    }
    #[test]
    fn scheme_slots_are_independent() {
        let light = ColorScheme::Sand;
        let mut dark = ColorScheme::Neutral;
        assert_ne!(light.colors(false), dark.colors(true));
        dark = ColorScheme::Forest;
        assert_eq!(light, ColorScheme::Sand);
        assert_eq!(dark.name(true), "深松绿");
    }
    #[test]
    fn foregrounds_have_readable_contrast() {
        fn luminance(hex: u32) -> f64 {
            let [r, g, b] = [(hex >> 16) & 255, (hex >> 8) & 255, hex & 255].map(|c| {
                let v = c as f64 / 255.;
                if v <= 0.04045 {
                    v / 12.92
                } else {
                    ((v + 0.055) / 1.055).powf(2.4)
                }
            });
            0.2126 * r + 0.7152 * g + 0.0722 * b
        }
        for scheme in ColorScheme::ALL {
            for dark in [false, true] {
                let c = scheme.colors(dark);
                for text in [c[3], c[4]] {
                    let a = luminance(c[0]);
                    let b = luminance(text);
                    assert!(
                        (a.max(b) + 0.05) / (a.min(b) + 0.05) >= 4.5,
                        "{:?}, dark={dark}",
                        scheme
                    );
                }
            }
        }
    }
}
