//! Process-wide chrome language. Documents and user-owned strings never enter this catalog.
use gpui_kit::{
    App, IntoElement, ParentElement as _, SharedString, Styled as _, Window,
    component::{
        IndexPath,
        searchable_list::{SearchableListChange, SearchableListDelegate, SearchableListItem},
    },
    div,
};

macro_rules! tr {
    ($source:expr) => {
        $crate::i18n::text($source)
    };
}
macro_rules! tf {
    ($source:literal, $($value:expr),* $(,)?) => {
        $crate::i18n::format_message(tr!($source), &[$(format!("{}", $value)),*])
    };
}

/// A language the catalog can render. Automatic detection resolves to one of these.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(usize)]
pub(super) enum Language {
    SimplifiedChinese = 0,
    English = 1,
    TraditionalChinese = 2,
}

impl Language {
    #[cfg(not(test))]
    fn from_index(value: usize) -> Self {
        match value {
            1 => Self::English,
            2 => Self::TraditionalChinese,
            _ => Self::SimplifiedChinese,
        }
    }

    pub fn locale(self) -> &'static str {
        match self {
            Self::SimplifiedChinese => "zh-CN",
            Self::English => "en",
            Self::TraditionalChinese => "zh-TW",
        }
    }
}

/// The stored interface-language choice. `Auto` follows the operating system and never
/// reaches the catalog, which only knows concrete languages.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(usize)]
pub(super) enum LanguagePreference {
    #[default]
    Auto = 0,
    English = 1,
    SimplifiedChinese = 2,
    TraditionalChinese = 3,
}

impl LanguagePreference {
    pub const ALL: [Self; 4] = [
        Self::Auto,
        Self::English,
        Self::SimplifiedChinese,
        Self::TraditionalChinese,
    ];

    pub fn from_saved(value: usize) -> Self {
        Self::ALL.get(value).copied().unwrap_or_default()
    }

    /// The dropdown label for this choice. Language names stay written in their own
    /// language.
    fn dropdown_title(self) -> SharedString {
        match self {
            Self::Auto => tr!("Auto detect"),
            Self::English => "English",
            Self::SimplifiedChinese => "简体中文",
            Self::TraditionalChinese => "繁體中文",
        }
        .into()
    }

    /// The language the UI actually renders for this choice.
    pub fn resolve(self) -> Language {
        match self {
            Self::Auto => detect_system_language(),
            Self::English => Language::English,
            Self::SimplifiedChinese => Language::SimplifiedChinese,
            Self::TraditionalChinese => Language::TraditionalChinese,
        }
    }
}

/// One row of the language dropdown. Detection and the explicit languages are choices;
/// the label row only separates them. The label is a row rather than a section header
/// because a Kit Select measures only its first section header and applies that height to
/// every header, so a real header above detection would reserve an equally tall but empty
/// band above the first option.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum LanguageItem {
    Choice(LanguagePreference),
    LanguagesLabel,
}

impl LanguageItem {
    /// The preference to apply, or `None` for the label row.
    pub(super) fn choice(self) -> Option<LanguagePreference> {
        match self {
            Self::Choice(preference) => Some(preference),
            Self::LanguagesLabel => None,
        }
    }
}

impl SearchableListItem for LanguageItem {
    type Value = Self;

    fn title(&self) -> SharedString {
        match self {
            Self::Choice(preference) => preference.dropdown_title(),
            Self::LanguagesLabel => tr!("Languages").into(),
        }
    }

    fn value(&self) -> &Self::Value {
        self
    }

    /// The label is not a choice: it stays muted and never takes hover feedback.
    fn disabled(&self) -> bool {
        matches!(self, Self::LanguagesLabel)
    }

    fn render(&self, _: &mut Window, _: &mut App) -> impl IntoElement {
        match self {
            Self::Choice(preference) => preference.dropdown_title().into_any_element(),
            // Smaller than a choice, like a Kit section header.
            Self::LanguagesLabel => div().text_sm().child(tr!("Languages")).into_any_element(),
        }
    }
}

/// The language dropdown rows: detection, the `Languages` label, then the three explicit
/// languages.
pub(super) struct LanguageChoices(Vec<LanguageItem>);

impl LanguageChoices {
    pub(super) fn new() -> Self {
        Self(vec![
            LanguageItem::Choice(LanguagePreference::Auto),
            LanguageItem::LanguagesLabel,
            LanguageItem::Choice(LanguagePreference::English),
            LanguageItem::Choice(LanguagePreference::SimplifiedChinese),
            LanguageItem::Choice(LanguagePreference::TraditionalChinese),
        ])
    }
}

impl SearchableListDelegate for LanguageChoices {
    type Item = LanguageItem;

    fn items_count(&self, _: usize) -> usize {
        self.0.len()
    }

    fn item(&self, ix: IndexPath) -> Option<&Self::Item> {
        self.0.get(ix.row)
    }

    fn position<V>(&self, value: &V) -> Option<IndexPath>
    where
        Self::Item: SearchableListItem<Value = V>,
        V: PartialEq,
    {
        self.0
            .iter()
            .position(|item| item.value() == value)
            .map(IndexPath::new)
    }

    /// Committing the label keeps the current language instead of selecting a row that is
    /// not a choice; a real row replaces the selection as usual.
    fn on_will_change(
        &mut self,
        selection: &mut Vec<(IndexPath, Self::Item)>,
        changes: &[SearchableListChange],
    ) {
        let on_label = changes.iter().any(|change| {
            matches!(change, SearchableListChange::Select { index }
                if self.0.get(index.row) == Some(&LanguageItem::LanguagesLabel))
        });
        if on_label {
            return;
        }
        for change in changes {
            match change {
                SearchableListChange::Select { index } => {
                    if let Some(item) = self.item(*index) {
                        selection.clear();
                        selection.push((*index, *item));
                    }
                }
                SearchableListChange::Deselect { index } => {
                    selection.retain(|(selected, _)| selected != index);
                }
            }
        }
    }
}

/// Map the operating-system language tag onto the catalog. Only Chinese is distinguished;
/// every other language uses English, the source language of the catalog.
fn language_for_locale(locale: &str) -> Language {
    let tag = locale.to_ascii_lowercase();
    if !(tag == "zh" || tag.starts_with("zh-") || tag.starts_with("zh_")) {
        return Language::English;
    }
    const TRADITIONAL_MARKERS: [&str; 7] = ["hant", "-tw", "_tw", "-hk", "_hk", "-mo", "_mo"];
    if TRADITIONAL_MARKERS
        .iter()
        .any(|marker| tag.contains(marker))
    {
        Language::TraditionalChinese
    } else {
        Language::SimplifiedChinese
    }
}

/// The system language for [`LanguagePreference::Auto`]. Tests resolve to Simplified
/// Chinese so their expected chrome does not depend on the machine's locale.
fn detect_system_language() -> Language {
    #[cfg(test)]
    {
        Language::SimplifiedChinese
    }
    #[cfg(not(test))]
    {
        sys_locale::get_locale()
            .as_deref()
            .map_or(Language::English, language_for_locale)
    }
}

// The application has one settings owner. Workers use the same locale for new
// diagnostics; test windows isolate their language from parallel test runners.
#[cfg(not(test))]
static LANGUAGE: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(Language::SimplifiedChinese as usize);
#[cfg(test)]
thread_local! { static LANGUAGE: std::cell::Cell<Language> = const { std::cell::Cell::new(Language::SimplifiedChinese) }; }

pub(super) fn language() -> Language {
    #[cfg(not(test))]
    {
        Language::from_index(LANGUAGE.load(std::sync::atomic::Ordering::Relaxed))
    }
    #[cfg(test)]
    {
        LANGUAGE.get()
    }
}

pub(super) fn set_language(language: Language) {
    #[cfg(not(test))]
    LANGUAGE.store(language as usize, std::sync::atomic::Ordering::Relaxed);
    #[cfg(test)]
    LANGUAGE.set(language);
}

macro_rules! catalog {
    ($($source:literal => ($simplified:literal, $traditional:literal)),* $(,)?) => {
        pub(super) fn text_in(source: &str, language: Language) -> &str {
            match (source, language) {
                $(($source, Language::SimplifiedChinese) => $simplified,
                  ($source, Language::TraditionalChinese) => $traditional,)*
                _ => source,
            }
        }
        #[cfg(test)]
        const MESSAGES: &[(&str, &str, &str)] = &[$(($source, $simplified, $traditional)),*];
    };
}
include!("i18n_catalog.rs");

pub(super) fn text(source: &str) -> &str {
    text_in(source, language())
}

/// Interpolate ordered slots once so braces inside filenames/notes remain literal.
pub(super) fn format_message(template: &str, values: &[String]) -> String {
    let mut parts = template.split("{}");
    let mut result = parts.next().unwrap_or_default().to_owned();
    for (part, value) in parts.zip(values) {
        result.push_str(value);
        result.push_str(part);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_literal_call_has_a_catalog_entry() {
        let files = [
            include_str!("main.rs"),
            include_str!("about.rs"),
            include_str!("settings.rs"),
            include_str!("shortcuts.rs"),
            include_str!("reader.rs"),
            include_str!("home.rs"),
            include_str!("commands.rs"),
            include_str!("categories.rs"),
            include_str!("document_panel.rs"),
            include_str!("tool_settings.rs"),
            include_str!("appearance.rs"),
            include_str!("colors.rs"),
            include_str!("view_controls.rs"),
            include_str!("annotations.rs"),
            include_str!("annotations/recovery.rs"),
            include_str!("annotation_editing.rs"),
            include_str!("export.rs"),
            include_str!("search.rs"),
            include_str!("reading.rs"),
            include_str!("raster.rs"),
            include_str!("covers.rs"),
            include_str!("persistence.rs"),
            include_str!("localization.rs"),
            include_str!("updates.rs"),
        ];
        for source in files {
            for call in source.split("tr!(").chain(source.split("tf!(")).skip(1) {
                if let Some(literal) = call.trim_start().strip_prefix('"') {
                    let key = literal.split('"').next().unwrap();
                    assert!(
                        MESSAGES.iter().any(|entry| entry.0 == key),
                        "Missing translation: {key}"
                    );
                }
            }
        }
    }

    #[test]
    fn catalogs_have_unique_keys_and_matching_slots() {
        let mut keys = std::collections::HashSet::new();
        for &(source, simplified, traditional) in MESSAGES {
            assert!(keys.insert(source), "duplicate key: {source}");
            assert!(!simplified.is_empty() && !traditional.is_empty());
            assert_eq!(
                source.matches("{}").count(),
                simplified.matches("{}").count(),
                "{source}"
            );
            assert_eq!(
                source.matches("{}").count(),
                traditional.matches("{}").count(),
                "{source}"
            );
            assert!(
                !source
                    .chars()
                    .any(|c| ('\u{3400}'..='\u{9fff}').contains(&c)),
                "{source}"
            );
        }
        assert_eq!(
            format_message("Page {}: {}", &["2".into(), "file{}中文".into()]),
            "Page 2: file{}中文"
        );
        assert_eq!(text_in("Settings", Language::English), "Settings");
        assert_eq!(text_in("Settings", Language::SimplifiedChinese), "设置");
        assert_eq!(text_in("Settings", Language::TraditionalChinese), "設定");
    }

    #[test]
    fn detection_prefers_chinese_and_falls_back_to_english() {
        for (locale, expected) in [
            ("zh", Language::SimplifiedChinese),
            ("zh-CN", Language::SimplifiedChinese),
            ("zh-Hans", Language::SimplifiedChinese),
            ("zh-Hans-CN", Language::SimplifiedChinese),
            ("zh_SG", Language::SimplifiedChinese),
            ("zh-TW", Language::TraditionalChinese),
            ("zh-HK", Language::TraditionalChinese),
            ("zh-Hant", Language::TraditionalChinese),
            ("zh-Hant-HK", Language::TraditionalChinese),
            ("zh_MO", Language::TraditionalChinese),
            ("en-US", Language::English),
            ("ja-JP", Language::English),
            ("de", Language::English),
            ("", Language::English),
        ] {
            assert_eq!(language_for_locale(locale), expected, "{locale}");
        }
    }

    #[test]
    fn preferences_roundtrip_and_resolve() {
        assert_eq!(LanguagePreference::from_saved(0), LanguagePreference::Auto);
        assert_eq!(
            LanguagePreference::from_saved(3),
            LanguagePreference::TraditionalChinese
        );
        assert_eq!(LanguagePreference::from_saved(9), LanguagePreference::Auto);
        for preference in LanguagePreference::ALL {
            assert_eq!(
                LanguagePreference::from_saved(preference as usize),
                preference
            );
        }
        assert_eq!(LanguagePreference::English.resolve(), Language::English);
        assert_eq!(
            LanguagePreference::TraditionalChinese.resolve(),
            Language::TraditionalChinese
        );
        assert_eq!(
            LanguagePreference::Auto.resolve(),
            Language::SimplifiedChinese
        );
    }

    #[gpui_kit::test]
    fn choices_offer_detection_before_the_explicit_languages(cx: &mut gpui_kit::TestAppContext) {
        use gpui_kit::component::searchable_list::SearchableListDelegate as _;

        cx.update(|cx| {
            let choices = LanguageChoices::new();
            assert_eq!(choices.sections_count(cx), 1);
            assert_eq!(choices.items_count(0), 5);
            assert_eq!(
                choices.item(IndexPath::new(0)),
                Some(&LanguageItem::Choice(LanguagePreference::Auto))
            );
            assert_eq!(
                choices.item(IndexPath::new(1)),
                Some(&LanguageItem::LanguagesLabel)
            );
            assert_eq!(
                choices.position(&LanguageItem::Choice(LanguagePreference::Auto)),
                Some(IndexPath::new(0))
            );
            for (row, preference) in [
                LanguagePreference::English,
                LanguagePreference::SimplifiedChinese,
                LanguagePreference::TraditionalChinese,
            ]
            .into_iter()
            .enumerate()
            {
                assert_eq!(
                    choices.position(&LanguageItem::Choice(preference)),
                    Some(IndexPath::new(row + 2))
                );
            }
        });
    }

    #[test]
    fn committing_the_label_keeps_the_current_language() {
        let mut choices = LanguageChoices::new();
        let current = (
            IndexPath::new(0),
            LanguageItem::Choice(LanguagePreference::Auto),
        );
        let mut selection = vec![current];
        choices.on_will_change(
            &mut selection,
            &[
                SearchableListChange::Deselect {
                    index: IndexPath::new(0),
                },
                SearchableListChange::Select {
                    index: IndexPath::new(1),
                },
            ],
        );
        assert_eq!(selection, vec![current]);

        choices.on_will_change(
            &mut selection,
            &[
                SearchableListChange::Deselect {
                    index: IndexPath::new(0),
                },
                SearchableListChange::Select {
                    index: IndexPath::new(2),
                },
            ],
        );
        assert_eq!(
            selection,
            vec![(
                IndexPath::new(2),
                LanguageItem::Choice(LanguagePreference::English)
            )]
        );
    }
}
