#[macro_use]
mod i18n;
mod localization;
mod updates;
#[cfg(test)]
use core::prelude::v1::test;
use gpui_kit::prelude::FluentBuilder;
mod about;
mod annotation_editing;
mod annotations;
mod appearance;
mod export;
mod persistence;
mod search;
mod window_policy;
use annotations::DocumentAnnotations;
use persistence::SavedState;
mod categories;
mod colors;
mod commands;
mod covers;
mod document_icons;
mod document_panel;
mod home;
mod raster;
mod reader;
mod reading;
mod settings;
mod shortcuts;
mod style;
mod tool_settings;
mod view_controls;
use anyhow::Result;
use appearance::ThemePreference;
use breeze_core::{Mode, ReaderState, ReadingLayout, ReadingPreferences, Zoom};
use breeze_pdf::{PdfWorker, Pixels, Source};
use gpui_kit::component::{
    ResizableState, Theme, TitleBar,
    select::{SelectEvent, SelectState},
};
use gpui_kit::{
    assets::{AllAssets, IconName},
    component::{
        Disableable, Selectable, Sizable,
        button::{Button, ButtonVariants},
        input::{Input, InputEvent, InputState},
    },
    *,
};
use std::{path::PathBuf, sync::Arc};
use style::{Palette, column, row};

actions!(
    breeze,
    [
        Open,
        Preferences,
        NextPage,
        PreviousPage,
        Home,
        Quit,
        CloseTab,
        NextTab,
        PreviousTab,
        ToggleSidebar,
        ToggleDocumentPanel,
        DeleteSelectedAnnotation,
        CancelAnnotationEdit,
        FocusPage,
        Favorite,
        CycleTheme,
        ReadMode,
        AnnotateMode,
        EditMode,
        ShowOutline,
        ShowThumbnails,
        ZoomIn,
        ZoomOut,
        ActualSize,
        FitPage,
        FitWidth,
        SaveAnnotations,
        ExportPdf,
        SaveAnnotationsAs,
        UndoAnnotation,
        RedoAnnotation,
        SearchDocument,
        CopySelection,
        CycleLayout,
        RotateLeft,
        RotateRight,
        ToggleFullScreen,
        ShowAbout,
        ShowShortcuts
    ]
);
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Screen {
    Home,
    Reader,
}
/// Home content selection. Home is the only navigation surface for global pages.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HomePage {
    Library,
    About,
    Shortcuts,
    Settings,
}
struct Book {
    source: Source,
    title: String,
    favorite: bool,
    categories: Vec<usize>,
    state: ReaderState,
    opened: bool,
    last_opened: u64,
    annotations: DocumentAnnotations,
}
struct Preview {
    page: usize,
    geometry: breeze_core::annotations::PageGeometry,
    glyphs: Vec<breeze_pdf::Glyph>,
    text_warning: Option<String>,
    points: (f32, f32),
    outline: Arc<Vec<breeze_pdf::Outline>>,
    bytes: usize,
}
struct Breeze {
    page_task: Option<Task<()>>,
    reading: reading::ReadingView,
    raster: raster::RasterView,
    worker: PdfWorker,
    window_handle: AnyWindowHandle,
    pending_window_policy: bool,
    pending_fit: Option<window_policy::PendingFit>,
    export_busy: bool,
    export_notice: Option<String>,
    last_export: Option<PathBuf>,
    storage: Option<breeze_storage::StateStore<SavedState>>,
    storage_ready: bool,
    restoring: bool,
    closing: bool,
    close_ready: bool,
    storage_error: Option<String>,
    last_snapshot: Option<SavedState>,
    state_save_generation: u64,
    data_dir: Option<PathBuf>,
    surface_bounds: Option<Bounds<gpui_kit::Pixels>>,
    gesture: Vec<breeze_core::annotations::Point>,
    selected_text: String,
    search_input: Entity<InputState>,
    search_open: bool,
    search_restore_document_panel_open: Option<bool>,
    search_busy: bool,
    search_generation: u64,
    search_hits: Vec<breeze_pdf::SearchHit>,
    search_notice: String,
    search_cancel: Option<Arc<std::sync::atomic::AtomicBool>>,
    full_screen: bool,
    library_input: Entity<InputState>,
    _subscriptions: Vec<Subscription>,
    books: Vec<Book>,
    active: usize,
    screen: Screen,
    home_page: HomePage,
    brand_logo: Option<Arc<RenderImage>>,
    system_appearance: WindowAppearance,
    theme_preference: ThemePreference,
    theme_select: Entity<SelectState<appearance::ThemeChoices>>,
    navigation_split: Entity<ResizableState>,
    navigation_width: f32,
    sidebar: bool,
    thumbs: bool,
    library_filter: usize,
    library_grid: bool,
    library_width: f32,
    covers: covers::LibraryCovers,
    categories: Vec<breeze_core::library::Category>,
    next_category_id: usize,
    category_input: Entity<InputState>,
    category_edit: Option<usize>,
    category_color: breeze_core::library::CategoryColor,
    category_form: bool,
    generation: u64,
    loading: bool,
    preview: Option<Arc<Preview>>,
    message: String,
    page_input: Entity<InputState>,
    font_input: Entity<InputState>,
    edit_text_input: Entity<InputState>,
    focus: FocusHandle,
    tool: usize,
    tool_settings: tool_settings::ToolSettings,
    document_panel: document_panel::DocumentPanel,
    annotation_editing: annotation_editing::AnnotationEditing,
    remember_files: bool,
    remember_position: bool,
    reading_preferences: ReadingPreferences,
    language: usize,
    applied_language: Option<usize>,
    language_select: Entity<SelectState<Vec<i18n::Language>>>,
    update_policy: usize,
    updates: updates::Updates,
    window_policy: usize,
}
impl Breeze {
    fn new(worker: PdfWorker, window: &mut Window, cx: &mut Context<Self>) -> Self {
        i18n::set_language(i18n::Language::SimplifiedChinese);
        let language_select = cx.new(|cx| {
            SelectState::new(
                i18n::Language::ALL.to_vec(),
                Some(gpui_kit::component::IndexPath::new(0)),
                window,
                cx,
            )
        });
        let language_subscription =
            cx.subscribe_in(&language_select, window, |this, _, event, window, cx| {
                if let SelectEvent::Confirm(Some(language)) = event {
                    this.language = *language as usize;
                    this.refresh_language(window, cx);
                    cx.notify();
                }
            });
        let edit_text_input =
            cx.new(|cx| InputState::new(window, cx).placeholder(tr!("让思绪，在此停留。")));
        let edit_subscription = cx.observe(&edit_text_input, |_, _, cx| cx.notify());
        let page_input = cx.new(|cx| InputState::new(window, cx).default_value("1"));
        let page_submission = cx.subscribe_in(&page_input, window, |this, _, event, window, cx| {
            if matches!(event, InputEvent::PressEnter { .. }) && this.screen == Screen::Reader {
                this.submit_page(cx);
                window.focus(&this.focus, cx);
            }
        });
        let search_input =
            cx.new(|cx| InputState::new(window, cx).placeholder(tr!("搜索文档文字（回车）")));
        let search_subscription = cx.subscribe(&search_input, |this, _, event, cx| {
            if matches!(event, InputEvent::PressEnter { .. }) {
                this.run_search(cx);
            }
        });
        let library_input =
            cx.new(|cx| InputState::new(window, cx).placeholder(tr!("筛选文档名称")));
        let library_observer = cx.observe(&library_input, |_, _, cx| cx.notify());
        let category_input = cx.new(|cx| InputState::new(window, cx).placeholder(tr!("分类名称")));
        let category_submission = cx.subscribe(&category_input, |this, _, event, cx| {
            if this.category_form && matches!(event, InputEvent::PressEnter { .. }) {
                this.save_category(cx);
            }
        });
        let window_bounds_observer = cx.observe_window_bounds(window, |this, window, cx| {
            this.on_native_window_bounds(window, cx)
        });
        let navigation_split = cx.new(|_| ResizableState::default());
        let resize_observer = cx.observe(&navigation_split, |this, state, cx| {
            if let Some(width) = state.read(cx).sizes().first() {
                let width = f32::from(*width);
                if (this.navigation_width - width).abs() > 0.5 {
                    this.resize_navigation(width);
                    cx.notify();
                }
            }
        });
        let theme_options = appearance::ThemeChoices::new(ThemePreference::groups(cx));
        let theme_select = cx.new(|cx| {
            SelectState::new(
                theme_options,
                Some(gpui_kit::component::IndexPath::new(0)),
                window,
                cx,
            )
            .searchable(true)
        });
        let theme_subscription =
            cx.subscribe_in(&theme_select, window, |this, _, event, window, cx| {
                if let SelectEvent::Confirm(Some(preference)) = event {
                    this.set_appearance(preference.clone(), window, cx);
                }
            });
        let appearance_observer = cx.observe_window_appearance(window, |this, window, cx| {
            this.system_appearance = window.appearance();
            if this.theme_preference == ThemePreference::System {
                this.apply_theme(cx);
                cx.notify();
            }
        });
        Self {
            page_task: None,
            reading: reading::ReadingView::default(),
            raster: raster::RasterView::default(),
            worker,
            window_handle: window.window_handle(),
            pending_window_policy: false,
            pending_fit: None,
            export_busy: false,
            export_notice: None,
            last_export: None,
            storage: None,
            storage_ready: false,
            restoring: false,
            closing: false,
            close_ready: false,
            storage_error: None,
            last_snapshot: None,
            state_save_generation: 0,
            data_dir: None,
            surface_bounds: None,
            gesture: Vec::new(),
            selected_text: String::new(),
            search_input,
            search_open: false,
            search_restore_document_panel_open: None,
            search_busy: false,
            search_generation: 0,
            search_hits: Vec::new(),
            search_notice: String::new(),
            search_cancel: None,
            full_screen: false,
            library_input,
            _subscriptions: vec![
                edit_subscription,
                page_submission,
                resize_observer,
                appearance_observer,
                window_bounds_observer,
                search_subscription,
                library_observer,
                category_submission,
                theme_subscription,
                language_subscription,
            ],
            books: Vec::new(),
            active: 0,
            screen: Screen::Home,
            home_page: HomePage::Library,
            brand_logo: about::brand_logo(),
            system_appearance: window.appearance(),
            theme_preference: ThemePreference::System,
            theme_select,
            navigation_split,
            navigation_width: 208.,
            sidebar: true,
            thumbs: false,
            library_filter: 0,
            library_grid: false,
            library_width: 800.,
            covers: covers::LibraryCovers::default(),
            categories: Vec::new(),
            next_category_id: 1,
            category_input,
            category_edit: None,
            category_color: breeze_core::library::CategoryColor::default(),
            category_form: false,
            generation: 0,
            loading: false,
            preview: None,
            message: "".into(),
            page_input,
            font_input: cx.new(|cx| InputState::new(window, cx).placeholder(tr!("系统默认"))),
            focus: cx.focus_handle(),
            edit_text_input,
            tool: 0,
            tool_settings: tool_settings::ToolSettings::default(),
            document_panel: document_panel::DocumentPanel::default(),
            annotation_editing: annotation_editing::AnnotationEditing::default(),
            remember_files: true,
            remember_position: true,
            reading_preferences: ReadingPreferences::default(),
            language: 0,
            applied_language: None,
            language_select,
            update_policy: 0,
            updates: updates::Updates::default(),
            window_policy: 0,
        }
    }
    fn p(&self, cx: &App) -> Palette {
        Palette::from_theme(cx)
    }
    /// Whether the library list, and therefore its cover previews, is visible.
    fn browsing_library(&self) -> bool {
        self.screen == Screen::Home && self.home_page == HomePage::Library
    }
    fn show_home(&mut self, cx: &mut Context<Self>) {
        if self.show_home_page(HomePage::Library, cx) {
            self.covers.clear(cx);
        }
    }
    /// Home owns the global pages; selecting one leaves any open gesture behind.
    /// Returns false when a pending text annotation blocks the navigation.
    fn show_home_page(&mut self, page: HomePage, cx: &mut Context<Self>) -> bool {
        if !self.annotation_navigation_allowed(cx) {
            return false;
        }
        if page != HomePage::Library {
            // Leaving the library stops in-flight preview work; returning re-requests it.
            self.covers.cancel();
        }
        self.tool_settings.dismiss();
        self.page_task = None;
        self.reading.cancel();
        self.raster.cancel();
        self.cancel_search();
        self.pending_window_policy = false;
        self.pending_fit = None;
        self.search_open = false;
        self.search_restore_document_panel_open = None;
        self.screen = Screen::Home;
        self.home_page = page;
        self.generation += 1;
        self.loading = false;
        cx.notify();
        true
    }
    fn open_book(&mut self, index: usize, cx: &mut Context<Self>) {
        if !self.annotation_navigation_allowed(cx) {
            return;
        }
        self.tool_settings.dismiss();
        if self.active != index {
            self.document_panel.reset_document();
            self.cancel_search();
            self.search_open = false;
            self.search_restore_document_panel_open = None;
        }
        self.pending_window_policy = !self.books[index].opened;
        self.active = index;
        self.tool = 0;
        self.books[index].opened = true;
        self.books[index].last_opened = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        self.screen = Screen::Reader;
        self.message.clear();
        self.load(cx);
    }
    fn load(&mut self, cx: &mut Context<Self>) {
        self.covers.cancel();
        self.page_task = None;
        self.reading.cancel();
        self.raster.cancel();
        self.pending_fit = None;
        self.gesture.clear();
        self.selected_text.clear();
        self.surface_bounds = None;
        self.generation += 1;
        let generation = self.generation;
        self.loading = true;
        let source = self.books[self.active].source.clone();
        let current = self.books[self.active].state.navigation.page();
        if self.reading.source.as_ref() != Some(&source) {
            self.reading.release_images(cx);
            self.reading = reading::ReadingView::default();
            self.raster.clear(cx);
            self.reading.source = Some(source);
        }
        self.reading.pages_scroll.scroll_to_item(
            self.books[self.active].state.layout.row(current),
            ScrollStrategy::Top,
        );
        self.scroll_thumbnail_to_page(current);
        self.preview = self.reading.page(current);
        let receiver = match self.worker.request_metadata(
            self.books[self.active].source.clone(),
            self.books[self.active].state.navigation.page(),
        ) {
            Ok(r) => r,
            Err(e) => {
                self.loading = false;
                self.pending_window_policy = false;
                self.message = format!("{e:#}");
                cx.notify();
                return;
            }
        };
        self.page_task = Some(cx.spawn(async move |this, cx| {
            let result = receiver.await;
            let _ = this.update(cx, |this, cx| {
                if generation != this.generation {
                    return;
                }
                this.loading = false;
                match result {
                    Ok(Ok(page)) => {
                        let nav = &mut this.books[this.active].state.navigation;
                        nav.set_total(page.total);
                        nav.go_to(page.page);
                        let fingerprint = page.fingerprint.clone();
                        if !this.reading.fingerprint.is_empty()
                            && this.reading.fingerprint != fingerprint
                        {
                            this.reading.release_images(cx);
                            this.reading = reading::ReadingView::default();
                            this.raster.clear(cx);
                            this.reading.source = Some(this.books[this.active].source.clone());
                            this.reading.pages_scroll.scroll_to_item(
                                this.books[this.active].state.layout.row(page.page),
                                ScrollStrategy::Top,
                            );
                            this.scroll_thumbnail_to_page(page.page);
                        }
                        this.reading.sizes = page.page_sizes.clone();
                        this.reading.fingerprint = fingerprint.clone();
                        let page_index = page.page;
                        let converted = Preview::from_page(page);
                        match converted {
                            Ok(preview) => {
                                this.reading.insert(page_index, preview.clone());
                                this.preview = Some(preview);
                                this.prepare_annotations(fingerprint, cx);
                                this.apply_opening_policy(cx);
                            }
                            Err(e) => this.message = format!("{e:#}"),
                        }
                    }
                    Ok(Err(error)) => this.message = format!("{error:#}"),
                    Err(_) => this.message = tr!("请求已取消，请重新打开文档").into(),
                }
                if this.preview.is_none() {
                    this.pending_window_policy = false;
                }
                this.pump_visible(cx);
                cx.notify();
            });
        }));
        cx.notify();
    }
    fn forget_book(&mut self, index: usize, cx: &mut Context<Self>) {
        if !self.annotation_navigation_allowed(cx) {
            return;
        }
        if self.books[index].annotations.dirty() || self.books[index].annotations.saving {
            self.message = tr!("请先保存批注，再移除资料库条目").into();
            cx.notify();
            return;
        }
        self.books.remove(index);
        if self.active >= index {
            self.active = self.active.saturating_sub(1);
        }
        self.show_home(cx);
    }
    fn next_page(&mut self, delta: i32, cx: &mut Context<Self>) {
        if self.screen != Screen::Reader {
            return;
        }
        if !self.annotation_navigation_allowed(cx) {
            return;
        }
        let layout = self.books[self.active].state.layout;
        let n = &mut self.books[self.active].state.navigation;
        let old = n.page();
        n.go_to(layout.step(old, n.total(), delta > 0));
        if n.page() != old {
            self.load(cx);
        }
    }
    fn choose_file(&mut self, cx: &mut Context<Self>) {
        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some(tr!("打开文档 · 当前原型支持 PDF").into()),
        });
        cx.spawn(async move |this, cx| match receiver.await {
            Ok(Ok(Some(paths))) => {
                if let Some(path) = paths.into_iter().next() {
                    let _ = this.update(cx, |this, cx| this.open_path(path, cx));
                }
            }
            Ok(Err(e)) => {
                let _ = this.update(cx, |this, cx| {
                    this.message = tf!("文件对话框失败：{}", e);
                    cx.notify();
                });
            }
            _ => {}
        })
        .detach();
    }
    fn open_path(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        let work = cx.background_spawn(async move {
            std::fs::canonicalize(&path).map_err(|e| tf!("无法打开 {}：{}", path.display(), e))
        });
        cx.spawn(async move |this, cx| {
            let result = work.await;
            let _ = this.update(cx, |this, cx| match result {
                Ok(path) => this.open_canonical_path(path, cx),
                Err(error) => {
                    this.message = error;
                    cx.notify();
                }
            });
        })
        .detach();
    }
    fn open_canonical_path(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        if !self.annotation_navigation_allowed(cx) {
            return;
        }
        if let Some(index) = self
            .books
            .iter()
            .position(|b| b.source == Source::File(path.clone()))
        {
            self.open_book(index, cx);
            return;
        }
        let title = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| tr!("文档").into());
        if self.books.len() >= 4096 {
            self.message = tr!("资料库条目已达上限").into();
            cx.notify();
            return;
        }
        self.books.push(Book {
            source: Source::File(path),
            title,
            favorite: false,
            categories: Vec::new(),
            state: ReaderState {
                layout: self.reading_preferences.layout,
                zoom: self.reading_preferences.zoom,
                ..Default::default()
            },
            opened: false,
            last_opened: 0,
            annotations: DocumentAnnotations::default(),
        });
        self.open_book(self.books.len() - 1, cx);
    }
    fn apply_theme(&mut self, cx: &mut App) {
        match self.theme_preference.config(self.system_appearance, cx) {
            Ok(config) => style::apply(&config, cx),
            Err(error) => self.storage_error = Some(format!("{error:#}")),
        }
    }
    /// Toggles immersive reading. Native fullscreen removes the OS chrome and
    /// the immersive layout removes every application row, leaving the pages.
    pub(crate) fn set_full_screen(
        &mut self,
        on: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.full_screen == on {
            return;
        }
        self.full_screen = on;
        if window.is_fullscreen() != on {
            window.toggle_fullscreen();
        }
        if on {
            // The controls that used to hold focus are gone; keep keys on the reader.
            window.focus(&self.focus, cx);
        }
        // The pages own the whole window, so presentation geometry changes.
        self.reading.invalidate_metrics();
        cx.notify();
    }
    fn set_appearance(
        &mut self,
        preference: ThemePreference,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        self.theme_preference = preference;
        self.system_appearance = window.appearance();
        self.apply_theme(cx);
        cx.notify();
    }
}
impl Render for Breeze {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.refresh_language(window, cx);
        let p = self.p(cx);
        if self.theme_select.read(cx).selected_value() != Some(&self.theme_preference) {
            self.theme_select.update(cx, |select, cx| {
                select.set_selected_value(&self.theme_preference, window, cx);
            });
        }
        if self.screen == Screen::Reader
            && !self.page_input.read(cx).focus_handle(cx).is_focused(window)
        {
            let value = (self.books[self.active].state.navigation.page() + 1).to_string();
            if self.page_input.read(cx).value().as_ref() != value {
                self.page_input
                    .update(cx, |input, cx| input.set_value(value, window, cx));
            }
        }
        // Immersive reading keeps the document only: no title bar, no banners.
        let immersive = self.full_screen && self.screen == Screen::Reader;
        let mut chrome: Vec<AnyElement> = Vec::new();
        if !immersive {
            chrome.push(self.header(window, cx).into_any_element());
            if let Some(error) = self.storage_error.clone() {
                chrome.push(
                    div()
                        .px_3()
                        .py_1()
                        .text_size(px(12.))
                        .text_color(rgb(0xb56b3c))
                        .child(error)
                        .into_any_element(),
                );
            }
            if let Some(banner) = self.update_banner(cx) {
                chrome.push(banner.into_any_element());
            }
        }
        let body = if self.restoring || self.closing {
            column()
                .flex_1()
                .items_center()
                .justify_center()
                .child(if self.closing {
                    tr!("正在保存本地状态…")
                } else {
                    tr!("正在恢复本地阅读空间…")
                })
                .into_any_element()
        } else {
            match self.screen {
                Screen::Home => self.home(window, cx).into_any_element(),
                Screen::Reader => self.reader(window, cx).into_any_element(),
            }
        };
        column()
            .size_full()
            .bg(p.bg)
            .text_color(p.text)
            .text_size(px(13.))
            .track_focus(&self.focus)
            .key_context("Breeze")
            .on_action(
                cx.listener(|this, action: &Open, window, cx| this.execute(action, window, cx)),
            )
            .on_action(cx.listener(|this, action: &Preferences, window, cx| {
                this.execute(action, window, cx)
            }))
            .on_action(
                cx.listener(|this, action: &ShowAbout, window, cx| {
                    this.execute(action, window, cx)
                }),
            )
            .on_action(cx.listener(|this, action: &ShowShortcuts, window, cx| {
                this.execute(action, window, cx)
            }))
            .on_action(
                cx.listener(|this, action: &Home, window, cx| this.execute(action, window, cx)),
            )
            .on_action(
                cx.listener(|this, action: &CloseTab, window, cx| this.execute(action, window, cx)),
            )
            .on_action(
                cx.listener(|this, action: &NextTab, window, cx| this.execute(action, window, cx)),
            )
            .on_action(cx.listener(|this, action: &PreviousTab, window, cx| {
                this.execute(action, window, cx)
            }))
            .on_action(cx.listener(|this, action: &ToggleSidebar, window, cx| {
                this.execute(action, window, cx)
            }))
            .on_action(
                cx.listener(|this, action: &ToggleDocumentPanel, window, cx| {
                    this.execute(action, window, cx)
                }),
            )
            .on_action(
                cx.listener(|this, _: &DeleteSelectedAnnotation, window, cx| {
                    this.delete_selected_annotation(window, cx);
                }),
            )
            .on_action(cx.listener(|this, _: &CancelAnnotationEdit, window, cx| {
                this.cancel_annotation_edit(window, cx);
            }))
            .on_action(
                cx.listener(|this, action: &FocusPage, window, cx| {
                    this.execute(action, window, cx)
                }),
            )
            .on_action(
                cx.listener(|this, action: &Favorite, window, cx| this.execute(action, window, cx)),
            )
            .on_action(
                cx.listener(|this, action: &CycleTheme, window, cx| {
                    this.execute(action, window, cx)
                }),
            )
            .on_action(
                cx.listener(|this, action: &ReadMode, window, cx| this.execute(action, window, cx)),
            )
            .on_action(cx.listener(|this, action: &AnnotateMode, window, cx| {
                this.execute(action, window, cx)
            }))
            .on_action(
                cx.listener(|this, action: &EditMode, window, cx| this.execute(action, window, cx)),
            )
            .on_action(cx.listener(|this, action: &ShowOutline, window, cx| {
                this.execute(action, window, cx)
            }))
            .on_action(cx.listener(|this, action: &ShowThumbnails, window, cx| {
                this.execute(action, window, cx)
            }))
            .on_action(
                cx.listener(|this, action: &NextPage, window, cx| this.execute(action, window, cx)),
            )
            .on_action(cx.listener(|this, action: &PreviousPage, window, cx| {
                this.execute(action, window, cx)
            }))
            .on_action(
                cx.listener(|this, action: &ZoomIn, window, cx| this.execute(action, window, cx)),
            )
            .on_action(
                cx.listener(|this, action: &ZoomOut, window, cx| this.execute(action, window, cx)),
            )
            .on_action(
                cx.listener(|this, action: &ActualSize, window, cx| {
                    this.execute(action, window, cx)
                }),
            )
            .on_action(
                cx.listener(|this, action: &FitPage, window, cx| this.execute(action, window, cx)),
            )
            .on_action(
                cx.listener(|this, action: &FitWidth, window, cx| this.execute(action, window, cx)),
            )
            .on_action(
                cx.listener(|this, action: &ExportPdf, window, cx| {
                    this.execute(action, window, cx)
                }),
            )
            .on_action(cx.listener(|this, action: &SaveAnnotations, window, cx| {
                this.execute(action, window, cx)
            }))
            .on_action(cx.listener(|this, action: &SaveAnnotationsAs, window, cx| {
                this.execute(action, window, cx)
            }))
            .on_action(cx.listener(|this, action: &UndoAnnotation, window, cx| {
                this.execute(action, window, cx)
            }))
            .on_action(cx.listener(|this, action: &RedoAnnotation, window, cx| {
                this.execute(action, window, cx)
            }))
            .on_action(cx.listener(|this, action: &SearchDocument, window, cx| {
                this.execute(action, window, cx)
            }))
            .on_action(cx.listener(|this, action: &CopySelection, window, cx| {
                this.execute(action, window, cx)
            }))
            .on_action(cx.listener(|this, action: &CycleLayout, window, cx| {
                this.execute(action, window, cx)
            }))
            .on_action(
                cx.listener(|this, action: &RotateLeft, window, cx| {
                    this.execute(action, window, cx)
                }),
            )
            .on_action(cx.listener(|this, action: &RotateRight, window, cx| {
                this.execute(action, window, cx)
            }))
            .on_action(cx.listener(|this, action: &ToggleFullScreen, window, cx| {
                this.execute(action, window, cx)
            }))
            .on_action(cx.listener(|this, _: &Quit, window, cx| {
                if this.request_close(window, cx) {
                    window.remove_window();
                }
            }))
            .children(chrome)
            .child(body)
    }
}
fn texture(p: Pixels) -> Result<Arc<RenderImage>> {
    let image = image::RgbaImage::from_raw(p.width, p.height, p.bgra)
        .ok_or_else(|| anyhow::anyhow!(tr!("页面像素大小不一致")))?;
    Ok(Arc::new(RenderImage::new(vec![image::Frame::new(image)])))
}
fn main() -> Result<()> {
    let worker = PdfWorker::new()?;
    let initial = std::env::args_os().nth(1).map(PathBuf::from);
    gpui_kit::application()
        .with_assets(AllAssets)
        .run(move |cx| {
            gpui_kit::init(cx);
            if let Err(error) = appearance::init(cx) {
                eprintln!("{}", tf!("无法初始化内置主题：{}", format!("{:#}", error)));
                cx.quit();
                return;
            }
            gpui_kit::component::set_locale("zh-CN");

            commands::bind(cx);

            cx.on_window_closed(|cx, _| {
                if cx.windows().is_empty() {
                    cx.quit();
                }
            })
            .detach();
            let result = gpui_kit::open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                        None,
                        size(px(1340.), px(900.)),
                        cx,
                    ))),
                    window_min_size: Some(size(px(900.), px(650.))),
                    ..TitleBar::window_options()
                },
                cx,
                move |window, cx| {
                    let view = cx.new(|cx| Breeze::new(worker, window, cx));
                    view.update(cx, |this, cx| {
                        window.focus(&this.focus, cx);
                        window.set_window_title(tr!("Breeze · 本地阅读工作台"));
                        this.apply_theme(cx);
                        this.initialize_storage(initial, cx);
                    });
                    let closing = view.downgrade();
                    window.on_window_should_close(cx, move |window, cx| {
                        closing
                            .update(cx, |this, cx| this.request_close(window, cx))
                            .unwrap_or(true)
                    });
                    let flushing = view.clone();
                    cx.on_app_quit(move |cx| {
                        let future = flushing.read(cx).flush_state();
                        async move {
                            if let Some(future) = future {
                                future.await;
                            }
                        }
                    })
                    .detach();
                    view
                },
            );
            if let Err(error) = result {
                eprintln!("{}", tf!("无法启动 Breeze：{}", format!("{:#}", error)));
                cx.quit();
            }
            cx.activate(true);
        });
    Ok(())
}

#[cfg(test)]
mod ui_tests;
