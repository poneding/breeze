//! Theme choices come from Kit's registry; System remains an application preference.
use anyhow::{Context as _, Result};
use gpui_kit::component::{
    ThemeConfig, ThemeMode, ThemeRegistry,
    select::{SearchableVec, SelectGroup, SelectItem},
};
use gpui_kit::{App, SharedString, WindowAppearance};
use serde::{Deserialize, Deserializer, Serialize};
use std::rc::Rc;

pub type ThemeChoices = SearchableVec<SelectGroup<ThemePreference>>;

// These are unchanged official gallery assets, pinned alongside Kit 0.7.0.
// Core Kit initialization alone registers only Default Light and Default Dark.
macro_rules! gallery {
    ($($name:literal),+ $(,)?) => {
        &[$(($name, include_str!(concat!("../../../assets/themes/gpui-kit/", $name, ".json")))),+]
    };
}
const GALLERY: &[(&str, &str)] = gallery![
    "adventure",
    "alduin",
    "asciinema",
    "aurora",
    "ayu",
    "catppuccin",
    "everforest",
    "fahrenheit",
    "flexoki",
    "gruvbox",
    "harper",
    "hybrid",
    "jellybeans",
    "kibble",
    "macos-classic",
    "mellifluous",
    "molokai",
    "solarized",
    "spaceduck",
    "tokyonight",
    "twilight",
];

pub fn init(cx: &mut App) -> Result<()> {
    let registry = ThemeRegistry::global_mut(cx);
    for (name, content) in GALLERY {
        registry
            .load_themes_from_str(content)
            .with_context(|| tf!("Cannot load bundled theme: {}", name))?;
    }
    Ok(())
}

#[derive(Serialize, Clone, Debug, Default, PartialEq, Eq)]
pub enum ThemePreference {
    #[default]
    System,
    Theme(String),
}

impl<'de> Deserialize<'de> for ThemePreference {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        // Versions 1–2 stored only a mode plus Breeze-owned palettes.
        #[derive(Deserialize)]
        enum StoredPreference {
            System,
            Light,
            Dark,
            Theme(String),
        }
        Ok(match StoredPreference::deserialize(deserializer)? {
            StoredPreference::System => Self::System,
            StoredPreference::Light => Self::Theme("Default Light".into()),
            StoredPreference::Dark => Self::Theme("Default Dark".into()),
            StoredPreference::Theme(name) => Self::Theme(name),
        })
    }
}

impl ThemePreference {
    pub fn groups(cx: &App) -> Vec<SelectGroup<Self>> {
        let themes = ThemeRegistry::global(cx).sorted_themes();
        let group = |label, mode| {
            SelectGroup::new(label).items(
                themes
                    .iter()
                    .filter(|theme| theme.mode == mode)
                    .map(|theme| Self::Theme(theme.name.to_string())),
            )
        };
        vec![
            SelectGroup::new(tr!("System")).item(Self::System),
            group(tr!("Light"), ThemeMode::Light),
            group(tr!("Dark"), ThemeMode::Dark),
        ]
    }

    pub fn options(cx: &App) -> Vec<Self> {
        Self::groups(cx)
            .into_iter()
            .flat_map(|group| group.items)
            .collect()
    }

    pub fn next(&self, cx: &App) -> Self {
        let options = Self::options(cx);
        let index = options
            .iter()
            .position(|option| option == self)
            .unwrap_or(0);
        options[(index + 1) % options.len()].clone()
    }

    pub fn config(&self, appearance: WindowAppearance, cx: &App) -> Result<Rc<ThemeConfig>> {
        let registry = ThemeRegistry::global(cx);
        match self {
            Self::System => Ok(
                if matches!(
                    appearance,
                    WindowAppearance::Dark | WindowAppearance::VibrantDark
                ) {
                    registry.default_dark_theme().clone()
                } else {
                    registry.default_light_theme().clone()
                },
            ),
            Self::Theme(name) => registry
                .themes()
                .get(name.as_str())
                .cloned()
                .with_context(|| tf!("Theme unavailable: {}", name)),
        }
    }
}

impl SelectItem for ThemePreference {
    type Value = Self;

    fn title(&self) -> SharedString {
        match self {
            Self::System => tr!("Follow system").into(),
            Self::Theme(name) => name.clone().into(),
        }
    }

    fn value(&self) -> &Self {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[gpui_kit::test]
    fn choices_and_resolution_use_the_registry(cx: &mut gpui_kit::TestAppContext) {
        cx.update(|cx| {
            gpui_kit::init(cx);
            init(cx).unwrap();
            let registry = ThemeRegistry::global(cx);
            let options = ThemePreference::options(cx);
            assert_eq!(options[0], ThemePreference::System);
            assert_eq!(options.len(), registry.themes().len() + 1);
            for option in &options[1..] {
                let config = registry.themes().get(&option.title()).unwrap();
                for appearance in [WindowAppearance::Light, WindowAppearance::Dark] {
                    assert!(Rc::ptr_eq(&option.config(appearance, cx).unwrap(), config));
                }
            }
            for appearance in [WindowAppearance::Light, WindowAppearance::VibrantLight] {
                assert!(Rc::ptr_eq(
                    &ThemePreference::System.config(appearance, cx).unwrap(),
                    registry.default_light_theme()
                ));
            }
            for appearance in [WindowAppearance::Dark, WindowAppearance::VibrantDark] {
                assert!(Rc::ptr_eq(
                    &ThemePreference::System.config(appearance, cx).unwrap(),
                    registry.default_dark_theme()
                ));
            }
            assert_eq!(options.last().unwrap().next(cx), ThemePreference::System);
            assert!(
                ThemePreference::Theme("Missing".into())
                    .config(WindowAppearance::Light, cx)
                    .is_err()
            );
        });
    }

    #[gpui_kit::test]
    fn complete_gallery_is_grouped_and_sorted(cx: &mut gpui_kit::TestAppContext) {
        cx.update(|cx| {
            gpui_kit::init(cx);
            init(cx).unwrap();
            let groups = ThemePreference::groups(cx);
            assert_eq!(groups.len(), 3);
            assert_eq!(groups[0].items, [ThemePreference::System]);
            let names = |group: &SelectGroup<ThemePreference>| {
                group
                    .items
                    .iter()
                    .map(|item| item.title().to_string())
                    .collect::<Vec<_>>()
            };
            assert_eq!(
                names(&groups[1]),
                [
                    "Default Light",
                    "Aurora Light",
                    "Ayu Light",
                    "Catppuccin Latte",
                    "Everforest Light",
                    "Flexoki Light",
                    "Gruvbox Light",
                    "Hybrid Light",
                    "macOS Classic Light",
                    "Mellifluous Light",
                    "Molokai Light",
                    "Solarized Light",
                ]
            );
            assert_eq!(
                names(&groups[2]),
                [
                    "Default Dark",
                    "Adventure",
                    "Adventure Time",
                    "Alduin",
                    "Asciinema",
                    "Ayu Dark",
                    "Catppuccin Frappe",
                    "Catppuccin Macchiato",
                    "Catppuccin Mocha",
                    "Everforest Dark",
                    "Fahrenheit",
                    "Flexoki Dark",
                    "Gruvbox Dark",
                    "Harper",
                    "Hybrid Dark",
                    "Jellybeans",
                    "Kibble",
                    "macOS Classic Dark",
                    "Mellifluous Dark",
                    "Molokai Dark",
                    "Solarized Dark",
                    "Spaceduck",
                    "Tokyo Moon",
                    "Tokyo Night",
                    "Tokyo Storm",
                    "Twilight",
                ]
            );
            assert_eq!(ThemeRegistry::global(cx).themes().len(), 38);
            assert_eq!(ThemePreference::options(cx).len(), 39);
            for (group, mode) in [
                (&groups[1], ThemeMode::Light),
                (&groups[2], ThemeMode::Dark),
            ] {
                for item in &group.items {
                    assert_eq!(item.config(WindowAppearance::Light, cx).unwrap().mode, mode);
                }
            }
            assert_eq!(
                ThemeRegistry::global(cx)
                    .default_light_theme()
                    .name
                    .as_ref(),
                "Default Light"
            );
            assert_eq!(
                ThemeRegistry::global(cx).default_dark_theme().name.as_ref(),
                "Default Dark"
            );
        });
    }

    #[test]
    fn legacy_modes_migrate_to_builtin_names() {
        for (stored, expected) in [
            ("System", ThemePreference::System),
            ("Light", ThemePreference::Theme("Default Light".into())),
            ("Dark", ThemePreference::Theme("Default Dark".into())),
        ] {
            let preference: ThemePreference =
                serde_json::from_value(serde_json::json!(stored)).unwrap();
            assert_eq!(preference, expected);
            assert_eq!(
                serde_json::from_str::<ThemePreference>(
                    &serde_json::to_string(&preference).unwrap()
                )
                .unwrap(),
                expected
            );
        }
    }
}
