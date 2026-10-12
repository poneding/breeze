//! Process-wide chrome language. Documents and user-owned strings never enter this catalog.
use gpui_kit::{SharedString, component::select::SelectItem};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(usize)]
pub(super) enum Language {
    #[default]
    SimplifiedChinese = 0,
    English = 1,
    TraditionalChinese = 2,
}

impl Language {
    pub const ALL: [Self; 3] = [
        Self::SimplifiedChinese,
        Self::English,
        Self::TraditionalChinese,
    ];

    pub fn from_saved(value: usize) -> Self {
        Self::ALL.get(value).copied().unwrap_or_default()
    }

    pub fn locale(self) -> &'static str {
        match self {
            Self::SimplifiedChinese => "zh-CN",
            Self::English => "en",
            Self::TraditionalChinese => "zh-TW",
        }
    }
}

impl SelectItem for Language {
    type Value = Self;
    fn title(&self) -> SharedString {
        match self {
            Self::SimplifiedChinese => "简体中文",
            Self::English => "English",
            Self::TraditionalChinese => "繁體中文",
        }
        .into()
    }
    fn value(&self) -> &Self {
        self
    }
}

// The application has one settings owner. Workers use the same locale for new
// diagnostics; test windows isolate their language from parallel test runners.
#[cfg(not(test))]
static LANGUAGE: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
#[cfg(test)]
thread_local! { static LANGUAGE: std::cell::Cell<Language> = const { std::cell::Cell::new(Language::SimplifiedChinese) }; }

pub(super) fn language() -> Language {
    #[cfg(not(test))]
    {
        Language::from_saved(LANGUAGE.load(std::sync::atomic::Ordering::Relaxed))
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
    ($($source:literal => ($english:literal, $traditional:literal)),* $(,)?) => {
        pub(super) fn text_in(source: &str, language: Language) -> &str {
            match (source, language) {
                $(($source, Language::English) => $english,
                  ($source, Language::TraditionalChinese) => $traditional,)*
                _ => source,
            }
        }
        #[cfg(test)]
        const MESSAGES: &[(&str, &str, &str)] = &[$(($source, $english, $traditional)),*];
    };
}
include!("i18n_catalog.rs");

pub(super) fn text(source: &str) -> &str {
    text_in(source, language())
}

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
        for &(source, english, traditional) in MESSAGES {
            assert!(keys.insert(source), "duplicate key: {source}");
            assert!(!english.is_empty() && !traditional.is_empty());
            assert_eq!(
                source.matches("{}").count(),
                english.matches("{}").count(),
                "{source}"
            );
            assert_eq!(
                source.matches("{}").count(),
                traditional.matches("{}").count(),
                "{source}"
            );
            assert!(
                !english
                    .chars()
                    .any(|c| ('\u{3400}'..='\u{9fff}').contains(&c)),
                "{source}"
            );
        }
        assert_eq!(
            format_message("Page {}: {}", &["2".into(), "file{}中文".into()]),
            "Page 2: file{}中文"
        );
        assert_eq!(text_in("设置", Language::English), "Settings");
        assert_eq!(text_in("设置", Language::TraditionalChinese), "設定");
    }
}
