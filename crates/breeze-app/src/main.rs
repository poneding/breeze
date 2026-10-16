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
mod errors;
mod home;
mod open_events;
mod raster;
mod reader;
mod reading;
mod reflow;
mod reflow_images;
mod reveal;
mod settings;
mod shortcuts;
mod style;
mod tool_settings;
mod view_controls;
use anyhow::Result;
use appearance::ThemePreference;
use breeze_core::document::{DocumentFormat, ReflowState};
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
use std::{collections::HashMap, path::PathBuf, sync::Arc};
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
    reflow: ReflowState,
    opened: bool,
    last_opened: u64,
    annotations: DocumentAnnotations,
}
impl Book {
    fn format(&self) -> Option<DocumentFormat> {
        document_format(&self.source)
    }
    fn is_pdf(&self) -> bool {
        self.format() == Some(DocumentFormat::Pdf)
    }
    fn format_label(&self) -> &'static str {
        self.format()
            .map(DocumentFormat::label)
            .unwrap_or("Unknown")
    }
    fn position_label(&self) -> String {
        if self.last_opened == 0 {
            return tr!("Not opened").into();
        }
        if self.is_pdf() {
            tf!("Page {}", self.state.navigation.page() + 1)
        } else {
            format!("{:.0}%", self.reflow.progress * 100.)
        }
    }
}
fn document_format(source: &Source) -> Option<DocumentFormat> {
    match source {
        Source::Sample => Some(DocumentFormat::Pdf),
        Source::File(path) => DocumentFormat::from_path(path),
    }
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
    reflow: reflow::ReflowView,
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
    /// Where each category row's revealed action row was laid out last frame.
    category_action_bounds: HashMap<usize, Bounds<gpui_kit::Pixels>>,
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
    /// Page text kept between searches of the same document revision.
    search_cache: Option<Arc<breeze_pdf::SearchCache>>,
    full_screen: bool,
    /// Whether the status-bar Fullscreen action entered native fullscreen from
    /// a windowed state and should therefore leave it again on exit.
    full_screen_entered_windowed: bool,
    library_input: Entity<InputState>,
    _subscriptions: Vec<Subscription>,
    books: Vec<Book>,
    active: usize,
    /// Documents the operating system requested while the library was still
    /// loading. Restore replaces the whole library, so they wait for it.
    pending_opens: Vec<PathBuf>,
    screen: Screen,
    home_page: HomePage,
    brand_logo: Option<Arc<RenderImage>>,
    system_appearance: WindowAppearance,
    theme_preference: ThemePreference,
    /// The dropdown row the user is previewing but has not confirmed, if any.
    theme_preview: Option<ThemePreference>,
    theme_choices: appearance::ThemeChoices,
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
    language_select: Entity<SelectState<i18n::LanguageChoices>>,
    update_policy: usize,
    updates: updates::Updates,
    window_policy: usize,
}
impl Breeze {
    fn new(worker: PdfWorker, window: &mut Window, cx: &mut Context<Self>) -> Self {
        i18n::set_language(i18n::LanguagePreference::default().resolve());
        let language_select = cx.new(|cx| {
            SelectState::new(
                i18n::LanguageChoices::new(),
                // Detection is the default and the first row.
                Some(gpui_kit::component::IndexPath::new(0)),
                window,
                cx,
            )
        });
        let language_subscription =
            cx.subscribe_in(&language_select, window, |this, _, event, window, cx| {
                if let SelectEvent::Confirm(Some(item)) = event
                    && let Some(preference) = item.choice()
                {
                    this.language = preference as usize;
                    this.refresh_language(window, cx);
                    cx.notify();
                }
            });
        let page_input = cx.new(|cx| InputState::new(window, cx).default_value("1"));
        let page_submission = cx.subscribe_in(&page_input, window, |this, _, event, window, cx| {
            if matches!(event, InputEvent::PressEnter { .. }) && this.screen == Screen::Reader {
                this.submit_page(cx);
                window.focus(&this.focus, cx);
            }
        });
        let search_input = cx
            .new(|cx| InputState::new(window, cx).placeholder(tr!("Search document text (Enter)")));
        let search_subscription = cx.subscribe(&search_input, |this, _, event, cx| {
            if matches!(event, InputEvent::PressEnter { .. }) {
                this.run_search(cx);
            }
        });
        let library_input =
            cx.new(|cx| InputState::new(window, cx).placeholder(tr!("Filter document names")));
        let library_observer = cx.observe(&library_input, |_, _, cx| cx.notify());
        let category_input =
            cx.new(|cx| InputState::new(window, cx).placeholder(tr!("Category name")));
        let window_bounds_observer = cx.observe_window_bounds(window, |this, window, cx| {
            this.on_native_window_bounds(window, cx);
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
        let theme_choices = appearance::ThemeChoices::new(ThemePreference::groups(cx));
        let theme_select = cx.new(|cx| {
            SelectState::new(
                theme_choices.clone(),
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
        // Escape, an outside click, or focus loss must not leave a preview applied.
        let theme_dismiss_subscription = cx.subscribe_in(
            &theme_select,
            window,
            |this, _, _: &DismissEvent, window, cx| {
                this.cancel_theme_preview(window, cx);
            },
        );
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
            reflow: reflow::ReflowView::default(),
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
            category_action_bounds: HashMap::new(),
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
            search_cache: None,
            full_screen: false,
            full_screen_entered_windowed: false,
            library_input,
            _subscriptions: vec![
                page_submission,
                resize_observer,
                appearance_observer,
                window_bounds_observer,
                search_subscription,
                library_observer,
                theme_subscription,
                theme_dismiss_subscription,
                language_subscription,
            ],
            books: Vec::new(),
            active: 0,
            pending_opens: Vec::new(),
            screen: Screen::Home,
            home_page: HomePage::Library,
            brand_logo: about::brand_logo(),
            system_appearance: window.appearance(),
            theme_preference: ThemePreference::System,
            theme_preview: None,
            theme_choices,
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
            focus: cx.focus_handle(),
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
            self.search_cache = None;
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
        if !self.books[self.active].is_pdf() {
            self.load_reflow(cx);
            return;
        }
        self.reflow.clear(cx);
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
                self.message = errors::error_text(&e);
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
                            Err(e) => this.message = errors::error_text(&e),
                        }
                    }
                    Ok(Err(error)) => this.message = errors::error_text(&error),
                    Err(_) => this.message = tr!("Request cancelled. Reopen the document.").into(),
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
            self.message = tr!("Save annotations before removing the library entry").into();
            cx.notify();
            return;
        }
        self.books.remove(index);
        if self.active >= index {
            self.active = self.active.saturating_sub(1);
        }
        self.show_home(cx);
    }
    /// Show a library document in the platform's file manager. The document is
    /// never opened, moved or edited by this action.
    fn reveal_book(&mut self, index: usize, cx: &mut Context<Self>) {
        let Source::File(path) = &self.books[index].source else {
            return;
        };
        let path = path.clone();
        let title = self.books[index].title.clone();
        let work = cx.background_spawn(async move { reveal::reveal(&path) });
        cx.spawn(async move |this, cx| {
            let failure = match work.await {
                Ok(()) => None,
                Err(reveal::Error::Missing) => Some(tf!(
                    "Cannot show {}: the file is missing from its saved location",
                    title
                )),
                Err(reveal::Error::Launch(error)) => Some(tf!("Cannot show {}: {}", title, error)),
            };
            if let Some(message) = failure {
                let _ = this.update(cx, |this, cx| {
                    this.message = message;
                    cx.notify();
                });
            }
        })
        .detach();
    }
    fn next_page(&mut self, delta: i32, cx: &mut Context<Self>) {
        if self.screen != Screen::Reader {
            return;
        }
        if !self.books[self.active].is_pdf() {
            self.turn_reflow(delta, cx);
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
            prompt: Some(tr!("Open document · PDF, EPUB, Markdown").into()),
        });
        cx.spawn(async move |this, cx| match receiver.await {
            Ok(Ok(Some(paths))) => {
                if let Some(path) = paths.into_iter().next() {
                    let _ = this.update(cx, |this, cx| this.open_path(path, cx));
                }
            }
            Ok(Err(e)) => {
                let _ = this.update(cx, |this, cx| {
                    this.message = tf!("File dialog failed: {}", e);
                    cx.notify();
                });
            }
            _ => {}
        })
        .detach();
    }
    fn open_path(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        self.open_paths(vec![path], cx);
    }
    /// Open the documents the operating system handed to this instance.
    ///
    /// Requests that arrive before the saved library has been restored are kept
    /// aside: restore replaces the whole book list, so opening them earlier
    /// would drop the document the user asked for.
    pub(crate) fn open_paths(&mut self, paths: Vec<PathBuf>, cx: &mut Context<Self>) {
        if paths.is_empty() {
            return;
        }
        if self.restoring {
            self.pending_opens.extend(paths);
            return;
        }
        let work = cx.background_spawn(async move {
            let mut canonical = Vec::with_capacity(paths.len());
            let mut failures = Vec::new();
            for path in paths {
                match std::fs::canonicalize(&path) {
                    Ok(path) => canonical.push(path),
                    Err(e) => failures.push(tf!("Cannot open {}: {}", path.display(), e)),
                }
            }
            (canonical, failures)
        });
        cx.spawn(async move |this, cx| {
            let (paths, failures) = work.await;
            let _ = this.update(cx, |this, cx| {
                for path in paths {
                    this.open_canonical_path(path, cx);
                }
                if let Some(failure) = failures.into_iter().next() {
                    this.message = failure;
                    cx.notify();
                }
            });
        })
        .detach();
    }
    fn open_canonical_path(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        if DocumentFormat::from_path(&path).is_none() {
            self.message = tr!("Supported formats: PDF, EPUB and Markdown").into();
            cx.notify();
            return;
        }
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
            .unwrap_or_else(|| tr!("Document").into());
        if self.books.len() >= 4096 {
            self.message = tr!("The library entry limit has been reached").into();
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
            reflow: ReflowState::default(),
        });
        self.open_book(self.books.len() - 1, cx);
    }
    /// Applies the preference in effect, which is the previewed row while the user is
    /// still moving around the open theme dropdown.
    fn apply_theme(&mut self, cx: &mut App) {
        let preference = self
            .theme_preview
            .clone()
            .unwrap_or_else(|| self.theme_preference.clone());
        match preference.config(self.system_appearance, cx) {
            Ok(config) => style::apply(&config, cx),
            Err(error) => self.storage_error = Some(errors::error_text(&error)),
        }
    }
    /// The theme under the open dropdown's cursor, resolved through its own delegate.
    fn highlighted_theme(&self, cx: &App) -> Option<ThemePreference> {
        self.theme_select
            .read(cx)
            .selected_index(cx)
            .and_then(|index| self.theme_choices.theme_at(index))
    }
    /// Whether the Settings page that owns the theme dropdown is on screen.
    fn settings_visible(&self) -> bool {
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
    fn preview_highlighted_theme(&mut self, cx: &mut Context<Self>) {
        let Some(highlighted) = self.highlighted_theme(cx) else {
            return;
        };
        if highlighted == self.theme_preference {
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
    fn schedule_theme_preview(&mut self, window: &Window, cx: &mut Context<Self>) {
        cx.defer_in(window, |this, _, cx| this.preview_highlighted_theme(cx));
    }
    /// Drops an unconfirmed preview so the committed theme comes back on screen, and
    /// puts the dropdown's cursor back on the committed row for the next open.
    fn cancel_theme_preview(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.theme_preview.take().is_none() {
            return;
        }
        self.theme_select.update(cx, |select, cx| {
            select.set_selected_value(&self.theme_preference, window, cx);
        });
        self.apply_theme(cx);
        cx.notify();
    }
    /// Toggles immersive reading. The window and the reader mode are separate:
    /// macOS's green traffic light can enter native fullscreen without hiding
    /// application chrome, while this control also requests native fullscreen
    /// and then removes the chrome from the reading surface.
    pub(crate) fn set_full_screen(
        &mut self,
        on: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.full_screen == on {
            return;
        }
        if on {
            self.full_screen_entered_windowed = !window.is_fullscreen();
            if self.full_screen_entered_windowed {
                window.toggle_fullscreen();
            }
        } else {
            if self.full_screen_entered_windowed && window.is_fullscreen() {
                window.toggle_fullscreen();
            }
            self.full_screen_entered_windowed = false;
        }
        self.apply_full_screen(on, window, cx);
    }
    fn apply_full_screen(&mut self, on: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.full_screen = on;
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
        // Committing a row, or cycling from the keyboard, replaces any preview.
        self.theme_preview = None;
        self.theme_preference = preference;
        self.system_appearance = window.appearance();
        self.apply_theme(cx);
        cx.notify();
    }
}
impl Render for Breeze {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.refresh_language(window, cx);
        // An unconfirmed preview only lives while the dropdown that can confirm it is on
        // screen. Kit reports its own closes directly, so this covers leaving Settings
        // through a command that unmounts the dropdown without running those callbacks.
        if self.theme_preview.is_some() && !self.settings_visible() {
            cx.defer_in(window, |this, window, cx| {
                this.cancel_theme_preview(window, cx)
            });
        }
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
        // Keep Breeze's document tabs reachable when AppKit owns the fullscreen
        // Space. The status-bar control still hides this row for immersive
        // reading; a native fullscreen transition must not strand the tabs
        // behind the system title bar's hover reveal.
        let header = (!immersive).then(|| self.header(window, cx).into_any_element());
        let mut chrome: Vec<AnyElement> = Vec::new();
        if !immersive {
            if let Some(header) = header {
                chrome.push(header);
            }
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
                    tr!("Saving local state…")
                } else {
                    tr!("Restoring your reading workspace…")
                })
                .into_any_element()
        } else {
            match self.screen {
                Screen::Home => self.home(window, cx).into_any_element(),
                Screen::Reader => self.reader(window, cx).into_any_element(),
            }
        };
        column()
            .id("breeze-root")
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
        .ok_or_else(|| anyhow::anyhow!(tr!("Page pixel size mismatch")))?;
    Ok(Arc::new(RenderImage::new(vec![image::Frame::new(image)])))
}
fn main() -> Result<()> {
    let worker = PdfWorker::new()?;
    let requests = open_events::OpenRequests::new();
    let requested = requests.clone();
    let application = gpui_kit::application().with_assets(AllAssets);
    // macOS hands documents to the running application through this event; it
    // has no application context, so the queue delivers them to the window.
    application.on_open_urls(move |urls| requested.receive_urls(&urls));
    application.run(move |cx| {
        gpui_kit::init(cx);
        if let Err(error) = appearance::init(cx) {
            eprintln!(
                "{}",
                tf!(
                    "Cannot initialize bundled themes: {}",
                    format!("{:#}", error)
                )
            );
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
                // Attach before draining so an event that arrives during
                // startup is delivered to this window instead of waiting.
                requests.attach(cx.to_async(), window.window_handle(), view.downgrade());
                let mut initial = open_events::argument_paths(std::env::args_os().skip(1));
                initial.extend(requests.take_pending());
                view.update(cx, |this, cx| {
                    window.focus(&this.focus, cx);
                    window.set_window_title(tr!("Breeze · Local reading workspace"));
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
            eprintln!("{}", tf!("Cannot start Breeze: {}", format!("{:#}", error)));
            cx.quit();
        }
        cx.activate(true);
    });
    Ok(())
}

#[cfg(test)]
mod ui_tests;
