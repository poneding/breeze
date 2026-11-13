//! Appearance modes and their independent theme choices use Kit's registry.
use crate::{Breeze, HomePage, Screen, errors, style};
use anyhow::{Context as _, Result};
use gpui_kit::component::{
    IndexPath, ThemeConfig, ThemeMode, ThemeRegistry,
    select::{SelectDelegate, SelectGroup, SelectItem},
};
use gpui_kit::{
    AnyElement, App, Context, IntoElement as _, SharedString, Task, Window, WindowAppearance,
};
use serde::{Deserialize, Deserializer, Serialize};
use std::cell::RefCell;
use std::rc::Rc;

/// The Settings dropdown's options: the bundled themes, grouped by appearance.
///
/// Kit's `SelectState` reports the dropdown cursor only as an `IndexPath`, and a search
/// query renumbers the rows before that index is read. This delegate therefore keeps the
/// filtered view itself, so the view can resolve the row under the cursor and preview
/// that theme while the user is still moving around the open dropdown.
#[derive(Clone)]
pub struct ThemeChoices {
    groups: Rc<Vec<SelectGroup<ThemePreference>>>,
    /// One entry per visible section: its group, and the group rows a query kept.
    visible: Rc<RefCell<Vec<VisibleGroup>>>,
}

struct VisibleGroup {
    group: usize,
    rows: Vec<usize>,
}

impl ThemeChoices {
    pub fn new(groups: Vec<SelectGroup<ThemePreference>>) -> Self {
        let visible = (0..groups.len())
            .map(|group| VisibleGroup {
                group,
                rows: (0..groups[group].items.len()).collect(),
            })
            .collect();
        Self {
            groups: Rc::new(groups),
            visible: Rc::new(RefCell::new(visible)),
        }
    }

    /// The theme at `index`, in the coordinates the open dropdown's cursor uses.
    pub fn theme_at(&self, index: IndexPath) -> Option<ThemePreference> {
        self.item(index).cloned()
    }
}

impl SelectDelegate for ThemeChoices {
    type Item = ThemePreference;

    fn sections_count(&self, _: &App) -> usize {
        self.visible.borrow().len()
    }

    fn items_count(&self, section: usize) -> usize {
        self.visible
            .borrow()
            .get(section)
            .map_or(0, |visible| visible.rows.len())
    }

    /// Kit's default group header only formats the element this returns, so the
    /// deprecated hook keeps the rows looking exactly as the previous
    /// `SearchableVec<SelectGroup<_>>` delegate did.
    #[allow(deprecated)]
    fn section(&self, section: usize) -> Option<AnyElement> {
        let group = self.visible.borrow().get(section)?.group;
        Some(self.groups.get(group)?.title.clone().into_any_element())
    }

    fn item(&self, index: IndexPath) -> Option<&Self::Item> {
        // Resolve the row index first: the returned reference borrows the groups,
        // not the cell holding which of their rows are visible.
        let (group, row) = {
            let visible = self.visible.borrow();
            let visible_group = visible.get(index.section)?;
            (visible_group.group, *visible_group.rows.get(index.row)?)
        };
        self.groups.get(group)?.items.get(row)
    }

    fn position<V>(&self, value: &V) -> Option<IndexPath>
    where
        Self::Item: SelectItem<Value = V>,
        V: PartialEq,
    {
        let visible = self.visible.borrow();
        for (section, visible_group) in visible.iter().enumerate() {
            let group = self.groups.get(visible_group.group)?;
            for (row, group_row) in visible_group.rows.iter().enumerate() {
                if group.items.get(*group_row)?.value() == value {
                    return Some(IndexPath::default().section(section).row(row));
                }
            }
        }
        None
    }

    fn perform_search(&mut self, query: &str, _: &mut Window, _: &mut App) -> Task<()> {
        let visible = self
            .groups
            .iter()
            .enumerate()
            .filter_map(|(group, entries)| {
                matched_rows(entries, query).map(|rows| VisibleGroup { group, rows })
            })
            .collect();
        *self.visible.borrow_mut() = visible;
        Task::ready(())
    }
}

/// The rows of one group that `query` keeps, or `None` when the group is hidden.
///
/// This mirrors Kit's own grouped filtering, so the rows the cursor walks and the rows
/// this delegate reports stay the same.
fn matched_rows(group: &SelectGroup<ThemePreference>, query: &str) -> Option<Vec<usize>> {
    let rows = group
        .items
        .iter()
        .enumerate()
        .filter(|(_, item)| item.matches(query))
        .map(|(row, _)| row)
        .collect::<Vec<_>>();
    (!rows.is_empty() || group.title.to_lowercase().contains(&query.to_lowercase())).then_some(rows)
}

// These are unchanged official gallery assets, pinned alongside Kit 0.8.0.
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
    Light,
    Dark,
    /// A registry choice, also accepted from snapshots predating split settings.
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
            StoredPreference::Light => Self::Light,
            StoredPreference::Dark => Self::Dark,
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
            group(tr!("Light"), ThemeMode::Light),
            group(tr!("Dark"), ThemeMode::Dark),
        ]
    }

    pub fn options() -> Vec<Self> {
        vec![Self::System, Self::Light, Self::Dark]
    }

    pub fn next(&self) -> Self {
        let options = Self::options();
        let index = options
            .iter()
            .position(|option| option == self)
            .unwrap_or(0);
        options[(index + 1) % options.len()].clone()
    }

    pub fn config(&self, appearance: WindowAppearance, cx: &App) -> Result<Rc<ThemeConfig>> {
        let registry = ThemeRegistry::global(cx);
        match self {
            Self::System | Self::Light | Self::Dark => {
                Ok(if self.mode(appearance) == ThemeMode::Dark {
                    registry.default_dark_theme().clone()
                } else {
                    registry.default_light_theme().clone()
                })
            }
            Self::Theme(name) => registry
                .themes()
                .get(name.as_str())
                .cloned()
                .with_context(|| tf!("Theme unavailable: {}", name)),
        }
    }

    pub fn mode(&self, appearance: WindowAppearance) -> ThemeMode {
        match self {
            Self::Dark => ThemeMode::Dark,
            Self::System
                if matches!(
                    appearance,
                    WindowAppearance::Dark | WindowAppearance::VibrantDark
                ) =>
            {
                ThemeMode::Dark
            }
            _ => ThemeMode::Light,
        }
    }

    /// Resolves a mode against the two committed registry names.
    pub fn configured(
        &self,
        light: &str,
        dark: &str,
        appearance: WindowAppearance,
        cx: &App,
    ) -> Result<Rc<ThemeConfig>> {
        if matches!(self, Self::Theme(_)) {
            return self.config(appearance, cx);
        }
        let mode = self.mode(appearance);
        let name = if mode == ThemeMode::Light {
            light
        } else {
            dark
        };
        let config = Self::Theme(name.into()).config(appearance, cx)?;
        anyhow::ensure!(
            config.mode == mode,
            tf!("Theme appearance does not match its setting: {}", name)
        );
        Ok(config)
    }

    /// Preserves the old named choice while adopting the corresponding fixed mode.
    pub fn migrate(&mut self, light: &mut String, dark: &mut String, cx: &App) -> Result<()> {
        if let Self::Theme(name) = self {
            let config = Self::Theme(name.clone()).config(WindowAppearance::Light, cx)?;
            if config.mode == ThemeMode::Light {
                *light = name.clone();
                *self = Self::Light;
            } else {
                *dark = name.clone();
                *self = Self::Dark;
            }
        }
        Ok(())
    }
}

pub fn default_light_theme() -> String {
    "Default Light".into()
}
pub fn default_dark_theme() -> String {
    "Default Dark".into()
}

impl SelectItem for ThemePreference {
    type Value = Self;

    fn title(&self) -> SharedString {
        match self {
            Self::System => tr!("Follow system").into(),
            Self::Light => tr!("Light").into(),
            Self::Dark => tr!("Dark").into(),
            Self::Theme(name) => name.clone().into(),
        }
    }

    fn value(&self) -> &Self {
        self
    }
}

impl Breeze {
    /// Applies the preference in effect, which is the previewed row while the user is
    /// still moving around the open theme dropdown.
    pub(super) fn apply_theme(&mut self, cx: &mut App) {
        let preference = self
            .theme_preview
            .clone()
            .unwrap_or_else(|| self.theme_preference.clone());
        match preference.configured(
            &self.light_theme,
            &self.dark_theme,
            self.system_appearance,
            cx,
        ) {
            Ok(config) => style::apply(&config, cx),
            Err(error) => self.storage_error = Some(errors::error_text(&error)),
        }
    }
    /// The theme under the open dropdown's cursor, resolved through its own delegate.
    pub(super) fn highlighted_theme(&self, mode: ThemeMode, cx: &App) -> Option<ThemePreference> {
        let (select, choices) = if mode == ThemeMode::Light {
            (&self.light_theme_select, &self.light_theme_choices)
        } else {
            (&self.dark_theme_select, &self.dark_theme_choices)
        };
        select
            .read(cx)
            .selected_index(cx)
            .and_then(|index| choices.theme_at(index))
    }
    /// Whether the Settings page that owns the theme dropdown is on screen.
    pub(super) fn settings_visible(&self) -> bool {
        !self.restoring
            && !self.closing
            && self.screen == Screen::Home
            && self.home_page == HomePage::Settings
    }
    /// Shows the theme under the open dropdown's cursor without committing it.
    ///
    /// Kit reports the cursor as an index into its filtered rows, so the row is
    /// resolved through the dropdown's own delegate. Confirming the row is what
    /// commits the preference; until then the applied theme stays a preview.
    pub(super) fn preview_highlighted_theme(&mut self, mode: ThemeMode, cx: &mut Context<Self>) {
        if !self.settings_visible() || self.theme_preference.mode(self.system_appearance) != mode {
            return;
        }
        let Some(highlighted) = self.highlighted_theme(mode, cx) else {
            return;
        };
        let committed = if mode == ThemeMode::Light {
            &self.light_theme
        } else {
            &self.dark_theme
        };
        if highlighted == ThemePreference::Theme(committed.clone()) {
            // Moving back onto the committed row ends the preview.
            if self.theme_preview.take().is_some() {
                self.apply_theme(cx);
                cx.notify();
            }
            return;
        }
        if self.theme_preview.as_ref() == Some(&highlighted) {
            return;
        }
        self.theme_preview = Some(highlighted);
        self.apply_theme(cx);
        cx.notify();
    }
    /// Previews the highlighted row once Kit's own key handler has moved the cursor.
    pub(super) fn schedule_theme_preview(
        &mut self,
        mode: ThemeMode,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        cx.defer_in(window, move |this, _, cx| {
            this.preview_highlighted_theme(mode, cx)
        });
    }
    /// Drops an unconfirmed preview so the committed theme comes back on screen, and
    /// puts the dropdown's cursor back on the committed row for the next open.
    pub(super) fn cancel_theme_preview(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.theme_preview.take().is_none() {
            return;
        }
        for (select, name) in [
            (&self.light_theme_select, &self.light_theme),
            (&self.dark_theme_select, &self.dark_theme),
        ] {
            select.update(cx, |select, cx| {
                select.set_selected_value(&ThemePreference::Theme(name.clone()), window, cx)
            });
        }
        self.apply_theme(cx);
        cx.notify();
    }
    pub(super) fn set_appearance(
        &mut self,
        mut preference: ThemePreference,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Committing a row, or cycling from the keyboard, replaces any preview.
        self.cancel_theme_preview(window, cx);
        if let Err(error) = preference.migrate(&mut self.light_theme, &mut self.dark_theme, cx) {
            self.storage_error = Some(errors::error_text(&error));
            cx.notify();
            return;
        }
        self.theme_preview = None;
        self.theme_preference = preference;
        self.system_appearance = window.appearance();
        self.apply_theme(cx);
        cx.notify();
    }

    pub(super) fn set_theme(
        &mut self,
        mode: ThemeMode,
        name: String,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        let config = ThemePreference::Theme(name.clone()).config(window.appearance(), cx);
        match config {
            Ok(config) if config.mode == mode => {
                if mode == ThemeMode::Light {
                    self.light_theme = name;
                } else {
                    self.dark_theme = name;
                }
                self.theme_preview = None;
                self.system_appearance = window.appearance();
                self.apply_theme(cx);
                cx.notify();
            }
            Ok(_) => {
                self.storage_error =
                    Some(tf!("Theme appearance does not match its setting: {}", name))
            }
            Err(error) => self.storage_error = Some(errors::error_text(&error)),
        }
    }

    pub(super) fn sync_theme_selects(&self, window: &mut Window, cx: &mut Context<Self>) {
        if self.theme_select.read(cx).selected_value() != Some(&self.theme_preference) {
            self.theme_select.update(cx, |select, cx| {
                select.set_selected_value(&self.theme_preference, window, cx)
            });
        }
        for (select, name) in [
            (&self.light_theme_select, &self.light_theme),
            (&self.dark_theme_select, &self.dark_theme),
        ] {
            let value = ThemePreference::Theme(name.clone());
            if select.read(cx).selected_value() != Some(&value) {
                select.update(cx, |select, cx| {
                    select.set_selected_value(&value, window, cx)
                });
            }
        }
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
            let options: Vec<_> = ThemePreference::groups(cx)
                .into_iter()
                .flat_map(|group| group.items)
                .collect();
            assert_eq!(options[0], ThemePreference::Theme("Default Light".into()));
            assert_eq!(options.len(), registry.themes().len());
            for option in &options {
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
            assert_eq!(ThemePreference::System.next(), ThemePreference::Light);
            assert_eq!(ThemePreference::Light.next(), ThemePreference::Dark);
            assert_eq!(ThemePreference::Dark.next(), ThemePreference::System);
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
            assert_eq!(groups.len(), 2);
            let names = |group: &SelectGroup<ThemePreference>| {
                group
                    .items
                    .iter()
                    .map(|item| item.title().to_string())
                    .collect::<Vec<_>>()
            };
            assert_eq!(
                names(&groups[0]),
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
                names(&groups[1]),
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
            assert_eq!(ThemePreference::options().len(), 3);
            for (group, mode) in [
                (&groups[0], ThemeMode::Light),
                (&groups[1], ThemeMode::Dark),
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

    #[gpui_kit::test]
    fn the_dropdown_cursor_resolves_through_the_filtered_view(cx: &mut gpui_kit::TestAppContext) {
        let cx = cx.add_empty_window();
        cx.update(|window, cx| {
            gpui_kit::init(cx);
            init(cx).unwrap();
            let groups = ThemePreference::groups(cx);
            let mut choices = ThemeChoices::new(groups.clone());
            assert_eq!(
                choices.theme_at(IndexPath::default()),
                Some(ThemePreference::Theme("Default Light".into()))
            );
            assert_eq!(
                choices.theme_at(IndexPath::default().section(1).row(1)),
                Some(groups[1].items[1].clone())
            );

            // A query renumbers the rows the cursor walks, so the highlighted row has
            // to resolve against the filtered view rather than the full list.
            _ = choices.perform_search("mocha", window, cx);
            assert_eq!(choices.sections_count(cx), 1);
            assert_eq!(
                choices.theme_at(IndexPath::default()),
                Some(ThemePreference::Theme("Catppuccin Mocha".into()))
            );

            // Clearing the query restores the unfiltered grouping.
            _ = choices.perform_search("", window, cx);
            assert_eq!(choices.sections_count(cx), 2);
            assert_eq!(
                choices.theme_at(IndexPath::default().section(1).row(0)),
                Some(ThemePreference::Theme("Default Dark".into()))
            );
        });
    }

    #[test]
    fn legacy_modes_migrate_to_builtin_names() {
        for (stored, expected) in [
            ("System", ThemePreference::System),
            ("Light", ThemePreference::Light),
            ("Dark", ThemePreference::Dark),
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
