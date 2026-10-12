//! Session persistence is independent from document-sidecar persistence.
use crate::*;
use breeze_core::Rotation;
use breeze_storage::StateStore;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct SavedBook {
    pub path: Option<PathBuf>,
    pub title: String,
    pub favorite: bool,
    #[serde(default, skip_serializing)]
    pub category: usize,
    #[serde(default)]
    pub categories: Vec<usize>,
    pub page: Option<usize>,
    pub zoom: Zoom,
    #[serde(default)]
    pub layout: ReadingLayout,
    /// View-only too; old snapshots simply load unrotated.
    #[serde(default)]
    pub rotation: Rotation,
    pub opened: bool,
    pub last_opened: u64,
    pub annotation_path: Option<PathBuf>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct SavedState {
    pub version: u32,
    pub books: Vec<SavedBook>,
    pub active: usize,
    #[serde(default)]
    pub categories: Vec<breeze_core::library::Category>,
    #[serde(default = "first_category_id")]
    pub next_category_id: usize,
    pub preference: ThemePreference,
    // Accept retired palette fields without writing them into new snapshots.
    #[serde(default, skip_serializing, rename = "light")]
    legacy_light: Option<String>,
    #[serde(default, skip_serializing, rename = "dark")]
    legacy_dark: Option<String>,
    #[serde(default)]
    pub library_grid: bool,
    pub sidebar: bool,
    pub width: f32,
    pub thumbnails: bool,
    pub remember_files: bool,
    pub remember_position: bool,
    #[serde(default)]
    pub reading_preferences: ReadingPreferences,
    pub language: usize,
    pub updates: usize,
    pub window: usize,
}
fn first_category_id() -> usize {
    1
}
impl Default for SavedState {
    fn default() -> Self {
        Self {
            version: 8,
            books: Vec::new(),
            active: 0,
            categories: Vec::new(),
            next_category_id: 1,
            preference: ThemePreference::System,
            legacy_light: None,
            legacy_dark: None,
            library_grid: false,
            sidebar: true,
            width: 208.,
            thumbnails: false,
            remember_files: true,
            remember_position: true,
            reading_preferences: ReadingPreferences::default(),
            language: 0,
            updates: 0,
            window: 0,
        }
    }
}
impl SavedState {
    /// Maps the retired "Never" policy to Manual, which never checks on its own.
    fn migrate_update_policy(&mut self) {
        if self.version < 8 && self.updates >= 2 {
            self.updates = 0;
        }
    }
    fn validate(&self) -> Result<()> {
        anyhow::ensure!(matches!(self.version, 1..=8), "Unsupported state version");
        anyhow::ensure!(self.books.len() <= 4096, "Too many library entries");
        anyhow::ensure!(
            self.width.is_finite() && (168.0..=360.0).contains(&self.width),
            "Invalid sidebar width"
        );
        // Version 8 retired the third update policy. Older snapshots may still
        // carry it until `restore` migrates the value.
        let update_policy_is_valid = self.updates < 2 || (self.version < 8 && self.updates < 3);
        anyhow::ensure!(
            self.language < 3 && update_policy_is_valid && self.window < 3,
            "Invalid preferences"
        );
        anyhow::ensure!(
            self.categories.len() <= 100
                && self.next_category_id > 0
                && self.next_category_id < usize::MAX - 3,
            "Invalid categories"
        );
        for (index, category) in self.categories.iter().enumerate() {
            anyhow::ensure!(
                category.id > 0
                    && category.id < self.next_category_id
                    && !self.categories[..index].iter().any(|c| c.id == category.id),
                "Invalid category ID"
            );
            anyhow::ensure!(
                breeze_core::library::validate_name(
                    &category.name,
                    &self.categories[..index],
                    None
                )
                .is_ok(),
                "Invalid category name"
            );
        }
        if let Zoom::Percent(n) = self.reading_preferences.zoom {
            anyhow::ensure!((25..=200).contains(&n), "Invalid default zoom");
        }
        for b in &self.books {
            anyhow::ensure!(
                (if self.version == 1 {
                    b.category < 3
                } else if self.version < 6 {
                    b.category == 0 || self.categories.iter().any(|c| c.id == b.category)
                } else {
                    b.category == 0
                }) && b.title.len() <= 4096,
                "Invalid library entry"
            );
            anyhow::ensure!(
                if self.version < 6 {
                    b.categories.is_empty()
                } else {
                    b.categories.len() <= 100
                        && b.categories.iter().enumerate().all(|(index, id)| {
                            self.categories.iter().any(|c| c.id == *id)
                                && !b.categories[..index].contains(id)
                        })
                },
                "Invalid category memberships"
            );
            if let Zoom::Percent(n) = b.zoom {
                anyhow::ensure!((25..=200).contains(&n), "Invalid zoom");
            }
        }
        Ok(())
    }
}
impl Breeze {
    pub(super) fn snapshot(&self) -> SavedState {
        SavedState {
            version: 8,
            books: self
                .books
                .iter()
                .map(|b| SavedBook {
                    path: match &b.source {
                        Source::Sample => None,
                        Source::File(p) => Some(p.clone()),
                    },
                    title: b.title.clone(),
                    favorite: b.favorite,
                    category: 0,
                    categories: b.categories.clone(),
                    page: self.remember_position.then_some(b.state.navigation.page()),
                    zoom: b.state.zoom,
                    layout: b.state.layout,
                    rotation: b.state.rotation,
                    opened: self.remember_files && b.opened,
                    last_opened: b.last_opened,
                    annotation_path: b.annotations.path.clone(),
                })
                .collect(),
            active: self.active,
            categories: self.categories.clone(),
            next_category_id: self.next_category_id,
            preference: self.theme_preference.clone(),
            legacy_light: None,
            legacy_dark: None,
            library_grid: self.library_grid,
            sidebar: self.sidebar,
            width: self
                .navigation_width
                .clamp(reader::NAVIGATION_MIN_WIDTH, 360.),
            thumbnails: self.thumbs,
            remember_files: self.remember_files,
            remember_position: self.remember_position,
            reading_preferences: self.reading_preferences,
            language: self.language,
            updates: self.update_policy,
            window: self.window_policy,
        }
    }
    pub(super) fn initialize_storage(&mut self, initial: Option<PathBuf>, cx: &mut Context<Self>) {
        self.restoring = true;
        let root = match breeze_storage::data_directory() {
            Ok(p) => p,
            Err(e) => {
                self.storage_error = Some(format!("{e:#}"));
                self.restoring = false;
                return;
            }
        };
        self.data_dir = Some(root.clone());
        let (store, ready) = match StateStore::<SavedState>::start(root) {
            Ok(v) => v,
            Err(e) => {
                self.storage_error = Some(format!("{e:#}"));
                self.restoring = false;
                return;
            }
        };
        self.storage = Some(store);
        cx.spawn(async move |this, cx| {
            let result = ready.await;
            let _ = this.update(cx, |this, cx| {
                this.restoring = false;
                match result {
                    Ok(Ok(state)) => {
                        if let Err(e) = state.validate().and_then(|_| {
                            state
                                .preference
                                .config(this.system_appearance, cx)
                                .map(|_| ())
                        }) {
                            this.storage_error = Some(format!("{e:#}"));
                            this.storage = None;
                        } else {
                            this.restore(state, cx);
                            this.storage_ready = true;
                            this.last_snapshot = Some(this.snapshot());
                            this.check_updates(true, cx);
                            let observer =
                                cx.observe(&cx.entity(), |this, _, cx| this.persist_if_changed(cx));
                            this._subscriptions.push(observer);
                            if let Some(path) = initial {
                                this.open_path(path, cx);
                            } else if this.remember_files && this.books.iter().any(|b| b.opened) {
                                this.screen = Screen::Reader;
                                this.pending_window_policy = true;
                                this.load(cx);
                            }
                        }
                    }
                    Ok(Err(e)) => {
                        this.storage_error =
                            Some(tf!("本地状态加载失败，已禁止写入：{}", format!("{:#}", e)));
                        this.storage = None;
                    }
                    Err(_) => {
                        this.storage_error = Some(tr!("本地状态线程不可用").into());
                        this.storage = None;
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }
    pub(super) fn restore(&mut self, mut state: SavedState, cx: &mut Context<Self>) {
        state.migrate_update_policy();
        if state.version < 5 {
            // Earlier snapshots treated every 100% value as Actual Size.
            if state.reading_preferences.zoom == Zoom::Percent(100) {
                state.reading_preferences.zoom = Zoom::ActualSize;
            }
            for book in &mut state.books {
                if book.zoom == Zoom::Percent(100) {
                    book.zoom = Zoom::ActualSize;
                }
            }
        }
        if state.version == 1 {
            // Preserve only legacy categories actually assigned to a document.
            state.categories = [(1, "灵感与设计"), (2, "工作与学习")]
                .into_iter()
                .filter(|(id, _)| state.books.iter().any(|b| b.category == *id))
                .map(|(id, name)| breeze_core::library::Category {
                    id,
                    name: name.into(),
                    color: breeze_core::library::CategoryColor::default(),
                })
                .collect();
            state.next_category_id = 3;
        }
        if state.version < 6 {
            for book in &mut state.books {
                book.categories = if book.category == 0 {
                    Vec::new()
                } else {
                    vec![book.category]
                };
            }
        }
        self.categories = state.categories;
        self.next_category_id = state.next_category_id;
        self.theme_preference = state.preference;
        self.library_grid = state.library_grid;
        self.sidebar = state.sidebar;
        self.navigation_width = state.width.max(reader::NAVIGATION_MIN_WIDTH);
        self.thumbs = state.thumbnails;
        self.remember_files = state.remember_files;
        self.remember_position = state.remember_position;
        self.reading_preferences = state.reading_preferences;
        self.language = state.language;
        self.update_policy = state.updates;
        self.window_policy = state.window;
        self.books = state
            .books
            .into_iter()
            .map(|b| {
                let mut reader = ReaderState {
                    zoom: b.zoom,
                    layout: b.layout,
                    rotation: b.rotation,
                    ..Default::default()
                };
                let page = if state.remember_position {
                    b.page.unwrap_or(0)
                } else {
                    0
                };
                reader.navigation.set_total(page.saturating_add(1));
                reader.navigation.go_to(page);
                Book {
                    source: b.path.map(Source::File).unwrap_or(Source::Sample),
                    title: b.title,
                    favorite: b.favorite,
                    categories: b.categories,
                    state: reader,
                    opened: state.remember_files && b.opened,
                    last_opened: b.last_opened,
                    annotations: DocumentAnnotations {
                        path: b.annotation_path,
                        ..Default::default()
                    },
                }
            })
            .collect();
        self.active = state.active.min(self.books.len().saturating_sub(1));
        if !self.books.get(self.active).is_some_and(|book| book.opened) {
            self.active = self.books.iter().position(|b| b.opened).unwrap_or(0);
        }
        self.apply_theme(cx);
    }
    pub(super) fn persist_if_changed(&mut self, cx: &mut Context<Self>) {
        if !self.storage_ready {
            return;
        }
        let state = self.snapshot();
        if self.last_snapshot.as_ref() == Some(&state) {
            return;
        }
        self.last_snapshot = Some(state.clone());
        let Some(store) = self.storage.as_ref() else {
            return;
        };
        match store.save(state) {
            Ok(receive) => {
                self.state_save_generation += 1;
                let generation = self.state_save_generation;
                cx.spawn(async move |this, cx| {
                    let result = receive.await;
                    let _ = this.update(cx, |this, cx| {
                        if generation != this.state_save_generation {
                            return;
                        }
                        match result {
                            Ok(Ok(())) => {
                                if this.storage_error.take().is_some() {
                                    cx.notify();
                                }
                            }
                            Ok(Err(error)) => {
                                this.storage_error =
                                    Some(tf!("本地状态保存失败：{}", format!("{:#}", error)));
                                this.last_snapshot = None;
                                this.storage_ready = false;
                                cx.notify();
                            }
                            Err(error) => {
                                this.storage_error = Some(tf!("本地状态保存被中断：{}", error));
                                this.last_snapshot = None;
                                this.storage_ready = false;
                                cx.notify();
                            }
                        }
                    });
                })
                .detach();
            }
            Err(e) => {
                self.storage_error = Some(format!("{e:#}"));
                self.storage_ready = false;
                cx.notify();
            }
        }
    }
    pub(super) fn request_close(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if self.close_ready {
            return true;
        }
        if self.closing || !self.allow_close(cx) {
            return false;
        }
        self.page_task = None;
        self.reading.cancel();
        if self.restoring {
            self.message = tr!("请等待本地状态加载完成").into();
            cx.notify();
            return false;
        }
        let Some(store) = self.storage.as_ref() else {
            return true;
        };
        let receive = match store.save(self.snapshot()) {
            Ok(r) => r,
            Err(e) => {
                self.storage_error = Some(tf!("关闭前保存失败：{}", format!("{:#}", e)));
                cx.notify();
                return false;
            }
        };
        self.closing = true;
        cx.notify();
        cx.spawn_in(window, async move |this, window| {
            let result = receive.await;
            let _ = this.update_in(window, |this, window, cx| {
                this.closing = false;
                match result {
                    Ok(Ok(())) => {
                        this.close_ready = true;
                        window.remove_window();
                    }
                    Ok(Err(e)) => {
                        this.storage_error =
                            Some(tf!("关闭前保存失败，窗口已保留：{}", format!("{:#}", e)));
                        cx.notify();
                    }
                    Err(e) => {
                        this.storage_error = Some(tf!("保存被中断：{}", e));
                        cx.notify();
                    }
                }
            });
        })
        .detach();
        false
    }
    pub(super) fn flush_state(&self) -> Option<impl std::future::Future<Output = ()> + use<>> {
        let receiver = self.storage.as_ref()?.save(self.snapshot()).ok()?;
        Some(async move {
            match receiver.await {
                Ok(Ok(())) => {}
                Ok(Err(e)) => eprintln!("Breeze state flush failed: {e:#}"),
                Err(e) => eprintln!("Breeze state writer stopped: {e}"),
            }
        })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn category_colors_roundtrip_and_legacy_categories_default_to_blue() {
        use breeze_core::library::{Category, CategoryColor};
        let legacy: Category = serde_json::from_str(r#"{"id":1,"name":"Work"}"#).unwrap();
        assert_eq!(legacy.color, CategoryColor::Blue);
        for color in CategoryColor::ALL {
            let category = Category {
                color,
                ..legacy.clone()
            };
            assert_eq!(
                serde_json::from_str::<Category>(&serde_json::to_string(&category).unwrap())
                    .unwrap(),
                category
            );
        }
        assert!(
            serde_json::from_str::<Category>(r#"{"id":1,"name":"Work","color":"Unknown"}"#)
                .is_err()
        );
    }
    #[test]
    fn memberships_are_validated_and_legacy_fields_are_read_only() {
        let mut state = SavedState {
            categories: vec![breeze_core::library::Category {
                id: 1,
                name: "Research".into(),
                color: breeze_core::library::CategoryColor::default(),
            }],
            next_category_id: 2,
            ..Default::default()
        };
        state.books.push(SavedBook {
            path: Some("document.pdf".into()),
            title: "Document".into(),
            favorite: true,
            category: 0,
            categories: vec![1],
            page: Some(2),
            zoom: Zoom::FitPage,
            layout: ReadingLayout::Continuous,
            rotation: Rotation::Left,
            opened: false,
            last_opened: 1,
            annotation_path: None,
        });
        state.validate().unwrap();
        let mut json = serde_json::to_value(&state).unwrap();
        assert!(json["books"][0].get("category").is_none());
        assert_eq!(
            serde_json::from_value::<SavedState>(json.clone()).unwrap(),
            state
        );
        for ids in [vec![0], vec![1, 1], vec![99]] {
            state.books[0].categories = ids;
            assert!(state.validate().is_err());
        }
        json["version"] = 5.into();
        json["books"][0]
            .as_object_mut()
            .unwrap()
            .remove("categories");
        json["books"][0]["category"] = 1.into();
        let legacy: SavedState = serde_json::from_value(json).unwrap();
        legacy.validate().unwrap();
        assert_eq!(legacy.books[0].category, 1);
    }
    #[test]
    fn view_rotation_roundtrips_and_absent_snapshots_stay_upright() {
        let mut state = SavedState {
            version: 8,
            ..Default::default()
        };
        state.books.push(SavedBook {
            path: None,
            title: "Document".into(),
            favorite: false,
            category: 0,
            categories: Vec::new(),
            page: Some(1),
            zoom: Zoom::FitPage,
            layout: ReadingLayout::Single,
            rotation: Rotation::Right,
            opened: true,
            last_opened: 3,
            annotation_path: None,
        });
        state.validate().unwrap();
        let bytes = serde_json::to_vec(&state).unwrap();
        assert_eq!(serde_json::from_slice::<SavedState>(&bytes).unwrap(), state);
        // Version 6 snapshots predate the view rotation and must keep loading.
        let mut legacy = serde_json::to_value(&state).unwrap();
        legacy["version"] = 6.into();
        legacy["books"][0]
            .as_object_mut()
            .unwrap()
            .remove("rotation");
        let restored: SavedState = serde_json::from_value(legacy).unwrap();
        restored.validate().unwrap();
        assert_eq!(restored.books[0].rotation, Rotation::None);
    }
    #[test]
    fn invalid_state_rejected() {
        let mut state = SavedState {
            width: f32::NAN,
            ..Default::default()
        };
        assert!(state.validate().is_err());
        state.width = 200.;
        state.version = 999;
        assert!(state.validate().is_err());
    }
    #[test]
    fn state_json_roundtrip() {
        for preference in [
            ThemePreference::System,
            ThemePreference::Theme("Default Dark".into()),
        ] {
            let state = SavedState {
                preference,
                ..Default::default()
            };
            let bytes = serde_json::to_vec(&state).unwrap();
            assert_eq!(serde_json::from_slice::<SavedState>(&bytes).unwrap(), state);
        }
    }
    #[test]
    fn reading_preferences_roundtrip_and_legacy_layouts_survive() {
        let mut state = SavedState::default();
        for layout in ReadingLayout::ALL {
            for zoom in [
                Zoom::FitPage,
                Zoom::FitWidth,
                Zoom::ActualSize,
                Zoom::Percent(100),
                Zoom::Percent(137),
            ] {
                state.reading_preferences = ReadingPreferences { layout, zoom };
                let bytes = serde_json::to_vec(&state).unwrap();
                let restored: SavedState = serde_json::from_slice(&bytes).unwrap();
                restored.validate().unwrap();
                assert_eq!(restored, state);
            }
        }
        let mut legacy = serde_json::to_value(SavedState::default()).unwrap();
        legacy["version"] = 3.into();
        legacy
            .as_object_mut()
            .unwrap()
            .remove("reading_preferences");
        let restored: SavedState = serde_json::from_value(legacy).unwrap();
        restored.validate().unwrap();
        assert_eq!(restored.reading_preferences, ReadingPreferences::default());
        for (json, layout) in [
            ("Single", ReadingLayout::Single),
            ("Continuous", ReadingLayout::Continuous),
            ("Spread", ReadingLayout::Spread),
        ] {
            assert_eq!(
                serde_json::from_value::<ReadingLayout>(json.into()).unwrap(),
                layout
            );
        }
        state.reading_preferences.zoom = Zoom::Percent(0);
        assert!(state.validate().is_err());
    }
    #[test]
    fn retired_never_update_policy_restores_as_manual() {
        let legacy = SavedState {
            version: 7,
            updates: 2,
            ..Default::default()
        };
        // An old snapshot carrying the retired choice must still load.
        assert!(legacy.validate().is_ok());
        let bytes = serde_json::to_vec(&legacy).unwrap();
        let mut restored: SavedState = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(restored.updates, 2);
        restored.migrate_update_policy();
        restored.validate().unwrap();
        assert_eq!(restored.updates, 0);

        // Version 8 accepts only the two remaining policies.
        let current = SavedState {
            version: 8,
            updates: 2,
            ..Default::default()
        };
        assert!(current.validate().is_err());
    }
    #[test]
    fn legacy_palettes_are_read_but_no_longer_written() {
        for version in 1..=5 {
            let mut json = serde_json::to_value(SavedState::default()).unwrap();
            json["version"] = version.into();
            json["preference"] = "Dark".into();
            json["light"] = "Blue".into();
            json["dark"] = "Forest".into();
            let state: SavedState = serde_json::from_value(json).unwrap();
            state.validate().unwrap();
            assert_eq!(
                state.preference,
                ThemePreference::Theme("Default Dark".into())
            );
            let saved = serde_json::to_value(&state).unwrap();
            assert!(saved.get("light").is_none());
            assert!(saved.get("dark").is_none());
            assert_eq!(
                saved["preference"],
                serde_json::json!({"Theme": "Default Dark"})
            );
        }
    }
}
