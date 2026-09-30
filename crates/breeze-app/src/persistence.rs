//! Session persistence is independent from document-sidecar persistence.
use crate::*;
use breeze_storage::StateStore;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct SavedBook {
    pub path: Option<PathBuf>,
    pub title: String,
    pub favorite: bool,
    pub category: usize,
    pub page: Option<usize>,
    pub zoom: Zoom,
    #[serde(default)]
    pub layout: ReadingLayout,
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
    pub light: ColorScheme,
    pub dark: ColorScheme,
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
            version: 5,
            books: Vec::new(),
            active: 0,
            categories: Vec::new(),
            next_category_id: 1,
            preference: ThemePreference::System,
            light: ColorScheme::Neutral,
            dark: ColorScheme::Neutral,
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
    fn validate(&self) -> Result<()> {
        anyhow::ensure!(matches!(self.version, 1..=5), "Unsupported state version");
        anyhow::ensure!(self.books.len() <= 4096, "Too many library entries");
        anyhow::ensure!(
            self.width.is_finite() && (168.0..=360.0).contains(&self.width),
            "Invalid sidebar width"
        );
        anyhow::ensure!(
            self.language < 2 && self.updates < 3 && self.window < 3,
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
                } else {
                    b.category == 0 || self.categories.iter().any(|c| c.id == b.category)
                }) && b.title.len() <= 4096,
                "Invalid library entry"
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
            version: 5,
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
                    category: b.category,
                    page: self.remember_position.then_some(b.state.navigation.page()),
                    zoom: b.state.zoom,
                    layout: b.state.layout,
                    opened: self.remember_files && b.opened,
                    last_opened: b.last_opened,
                    annotation_path: b.annotations.path.clone(),
                })
                .collect(),
            active: self.active,
            categories: self.categories.clone(),
            next_category_id: self.next_category_id,
            preference: self.theme_preference,
            light: self.light_scheme,
            dark: self.dark_scheme,
            library_grid: self.library_grid,
            sidebar: self.sidebar,
            width: self.navigation_width.clamp(168., 360.),
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
                        if let Err(e) = state.validate() {
                            this.storage_error = Some(format!("{e:#}"));
                            this.storage = None;
                        } else {
                            this.restore(state, cx);
                            this.storage_ready = true;
                            this.last_snapshot = Some(this.snapshot());
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
                        this.storage_error = Some(format!("本地状态加载失败，已禁止写入：{e:#}"));
                        this.storage = None;
                    }
                    Err(_) => {
                        this.storage_error = Some("本地状态线程不可用".into());
                        this.storage = None;
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }
    pub(super) fn restore(&mut self, mut state: SavedState, cx: &mut Context<Self>) {
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
                })
                .collect();
            state.next_category_id = 3;
        }
        self.categories = state.categories;
        self.next_category_id = state.next_category_id;
        self.theme_preference = state.preference;
        self.dark = state.preference.resolve(self.system_appearance);
        self.light_scheme = state.light;
        self.dark_scheme = state.dark;
        self.library_grid = state.library_grid;
        self.sidebar = state.sidebar;
        self.navigation_width = state.width;
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
                    category: b.category,
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
        self.apply_colors(cx);
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
                                this.storage_error = Some(format!("本地状态保存失败：{error:#}"));
                                this.last_snapshot = None;
                                this.storage_ready = false;
                                cx.notify();
                            }
                            Err(error) => {
                                this.storage_error = Some(format!("本地状态保存被中断：{error}"));
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
            self.message = "请等待本地状态加载完成".into();
            cx.notify();
            return false;
        }
        let Some(store) = self.storage.as_ref() else {
            return true;
        };
        let receive = match store.save(self.snapshot()) {
            Ok(r) => r,
            Err(e) => {
                self.storage_error = Some(format!("关闭前保存失败：{e:#}"));
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
                        this.storage_error = Some(format!("关闭前保存失败，窗口已保留：{e:#}"));
                        cx.notify();
                    }
                    Err(e) => {
                        this.storage_error = Some(format!("保存被中断：{e}"));
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
        let state = SavedState::default();
        let bytes = serde_json::to_vec(&state).unwrap();
        assert_eq!(serde_json::from_slice::<SavedState>(&bytes).unwrap(), state);
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
}
