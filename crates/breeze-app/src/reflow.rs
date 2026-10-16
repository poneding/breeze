//! Native reflow readers share chrome, not PDF navigation or annotation state.
use crate::*;

/// Comfortable measure for Markdown text in logical pixels.
const MARKDOWN_MAX_WIDTH: f32 = 760.;
use breeze_reflow::{EpubWorker, epub, markdown};
use gpui_kit::base::{TextView, TextViewState};
use gpui_kit::component::{
    ElementExt, h_resizable,
    menu::{DropdownMenu, PopupMenuItem},
    resizable_panel,
    tab::{Tab, TabBar},
    tooltip::Tooltip,
};
use std::collections::{HashMap, HashSet, VecDeque};

struct TextBlock {
    index: usize,
    state: Entity<TextViewState>,
    _subscription: Subscription,
}

/// One row of the navigation panel. EPUB chapters and Markdown headings share
/// this shape so both render exactly like the PDF outline: the same row height,
/// indentation, active highlight and nesting controls.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct OutlineEntry {
    pub title: String,
    pub depth: usize,
    pub href: String,
    pub block: usize,
}

/// The navigation-outline title search. Mirrors the PDF reader's
/// `reading::OutlineSearch`.
struct OutlineSearch {
    input: Entity<InputState>,
    collapsed: HashSet<usize>,
    saved_offset: Point<gpui_kit::Pixels>,
    _subscription: Subscription,
}

/// A caret selection over the selectable text layers in the current EPUB frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ReflowTextSelection {
    anchor: (usize, usize),
    focus: (usize, usize),
}

impl ReflowTextSelection {
    fn ordered(self) -> ((usize, usize), (usize, usize)) {
        if self.anchor <= self.focus {
            (self.anchor, self.focus)
        } else {
            (self.focus, self.anchor)
        }
    }

    fn collapsed(self) -> bool {
        self.anchor == self.focus
    }

    fn on_page(self, page: usize, glyphs: usize) -> std::ops::Range<usize> {
        let (start, end) = self.ordered();
        if page < start.0 || page > end.0 {
            return 0..0;
        }
        let low = if page == start.0 { start.1 } else { 0 }.min(glyphs);
        let high = if page == end.0 { end.1 } else { glyphs }.min(glyphs);
        low..high.max(low)
    }
}

pub(super) struct ReflowView {
    pub source: Option<Source>,
    pub markdown: Option<Arc<markdown::Document>>,
    pub frame: Option<epub::Frame>,
    pub(super) page_images: Vec<Arc<RenderImage>>,
    worker: Option<EpubWorker>,
    blocks: VecDeque<TextBlock>,
    pub list: ListState,
    task: Option<Task<()>>,
    request: Option<epub::Request>,
    viewport: (f32, f32, f32),
    /// Render generation, so a stale worker result can be discarded.
    pub epoch: u64,
    queued: bool,
    pub hits: Vec<(String, usize, String)>,
    pub images: reflow_images::Images,
    /// Selection over the current EPUB frame, plus the rectangles to paint.
    text_selection: Option<ReflowTextSelection>,
    selection: Vec<(usize, Vec<[f32; 4]>)>,
    selection_active: bool,
    /// Where each page image was laid out last frame, and its raster size.
    page_bounds: HashMap<usize, Bounds<gpui_kit::Pixels>>,
    page_sizes: Vec<(f32, f32)>,
    /// Outline rows for the open document, built once when it loads.
    pub outline: Arc<Vec<OutlineEntry>>,
    /// Folded outline rows, addressed by their index in `outline`.
    pub outline_folds: HashSet<usize>,
    pub outline_scroll: UniformListScrollHandle,
    outline_search: Option<OutlineSearch>,
    /// Block index at the top of the Markdown view, kept from the list's own
    /// scroll reports so rendering never re-borrows the list.
    pub active_block: usize,
    /// Wheel distance not yet worth a page turn, in logical pixels.
    scroll_accum: f32,
    /// Page steps the reader has asked for that have not been issued yet.
    ///
    /// A step can only be resolved against a landed frame, because chapter
    /// boundaries live in the layout. Without this queue a second click while the
    /// first was still rendering rebuilt the same request from the previous
    /// position and was dropped, so clicking Next twice moved one page. Steps are
    /// issued one at a time rather than as one large delta, so a burst of clicks
    /// lands on every page instead of depending on the engine to cross several
    /// chapters at once.
    pending_turn: i16,
}

impl Default for ReflowView {
    fn default() -> Self {
        Self {
            source: None,
            markdown: None,
            frame: None,
            page_images: Vec::new(),
            worker: None,
            blocks: VecDeque::new(),
            list: ListState::new(0, ListAlignment::Top, px(256.)),
            task: None,
            request: None,
            viewport: (600., 800., 1.),
            epoch: 0,
            queued: false,
            hits: Vec::new(),
            images: Default::default(),
            text_selection: None,
            selection: Vec::new(),
            selection_active: false,
            page_bounds: HashMap::new(),
            page_sizes: Vec::new(),
            outline: Arc::new(Vec::new()),
            outline_folds: HashSet::new(),
            outline_scroll: UniformListScrollHandle::new(),
            outline_search: None,
            active_block: 0,
            scroll_accum: 0.,
            pending_turn: 0,
        }
    }
}

impl ReflowView {
    fn outline_folds(&self) -> &HashSet<usize> {
        match &self.outline_search {
            Some(search) => &search.collapsed,
            None => &self.outline_folds,
        }
    }

    pub fn clear(&mut self, cx: &mut App) {
        self.epoch += 1;
        self.task = None;
        self.request = None;
        self.queued = false;
        self.source = None;
        self.markdown = None;
        self.frame = None;
        self.blocks.clear();
        self.hits.clear();
        self.text_selection = None;
        self.selection.clear();
        self.selection_active = false;
        self.page_bounds.clear();
        self.page_sizes.clear();
        self.outline = Arc::new(Vec::new());
        self.outline_folds.clear();
        self.outline_scroll = UniformListScrollHandle::new();
        self.outline_search = None;
        self.active_block = 0;
        self.scroll_accum = 0.;
        self.pending_turn = 0;
        self.worker = None;
        self.images.clear(cx);
        let page_images = std::mem::take(&mut self.page_images);
        if !page_images.is_empty() {
            cx.defer(move |cx| {
                for image in page_images {
                    cx.drop_image(image, None);
                }
            });
        }
    }
}

impl Breeze {
    pub(super) fn run_reflow_search(&mut self, cx: &mut Context<Self>) {
        use std::sync::atomic::AtomicBool;
        let query = self.search_input.read(cx).value().to_string();
        if query.trim().is_empty() || query.len() > 256 {
            self.search_notice = tr!("Enter a search term of 1–256 bytes").into();
            cx.notify();
            return;
        }
        self.cancel_search();
        let generation = self.search_generation;
        let cancel = Arc::new(AtomicBool::new(false));
        self.search_cancel = Some(cancel.clone());
        self.search_busy = true;
        self.search_notice = tr!("Searching…").into();
        let document = self.reflow.markdown.clone();
        let Source::File(path) = self.books[self.active].source.clone() else {
            return;
        };
        let fingerprint = self
            .reflow
            .frame
            .as_ref()
            .map(|frame| frame.metadata.fingerprint.clone());
        let work = cx.background_spawn(async move {
            if let Some(document) = document {
                Ok(document
                    .search(&query)
                    .into_iter()
                    .map(|(block, text)| (String::new(), block, text))
                    .collect::<Vec<_>>())
            } else {
                let (revision, hits) = epub::search(&path, &query, &cancel)?;
                anyhow::ensure!(
                    fingerprint.as_ref() == Some(&revision),
                    "The EPUB changed; reopen it before searching"
                );
                Ok::<_, anyhow::Error>(
                    hits.into_iter()
                        .map(|hit| {
                            let text =
                                reflow_search_result_text(&hit.title, &hit.href, &hit.snippet);
                            (hit.href, 0, text)
                        })
                        .collect(),
                )
            }
        });
        cx.spawn(async move |this, cx| {
            let result = work.await;
            let _ = this.update(cx, |this, cx| {
                if this.search_generation != generation {
                    return;
                }
                this.search_busy = false;
                this.search_cancel = None;
                match result {
                    Ok(hits) => {
                        this.search_notice = tf!("{} matching sections", hits.len());
                        this.reflow.hits = hits;
                    }
                    Err(error) => this.search_notice = format!("{error:#}"),
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    pub(super) fn reflow_search_results(&self, cx: &mut Context<Self>) -> Div {
        column()
            .flex_1()
            .min_h_0()
            .p_2()
            .gap_2()
            .child(div().text_size(px(12.)).child(self.search_notice.clone()))
            .child(
                column()
                    .id("search-results")
                    .test_support()
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .children(self.reflow.hits.iter().enumerate().map(
                        |(index, (href, block, text))| {
                            let (href, block) = (href.clone(), *block);
                            Button::new(("search-result", index))
                                .ghost()
                                .small()
                                .w_full()
                                .justify_start()
                                .accessibility_label(text.clone())
                                .tooltip(text.clone())
                                .child(
                                    div()
                                        .w_full()
                                        .text_left()
                                        .text_ellipsis()
                                        .child(text.clone()),
                                )
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    if this.books[this.active].format()
                                        == Some(DocumentFormat::Epub)
                                    {
                                        this.books[this.active].reflow.epub =
                                            breeze_core::document::EpubLocation {
                                                href: href.clone(),
                                                progression: 0.,
                                            };
                                        this.request_epub(
                                            0,
                                            0.,
                                            Some(this.search_input.read(cx).value().to_string()),
                                            cx,
                                        );
                                    } else {
                                        this.jump_reflow(String::new(), block, cx);
                                    }
                                }))
                        },
                    )),
            )
    }

    pub(super) fn reflow_sidebar(&self, cx: &mut Context<Self>) -> AnyElement {
        let p = self.p(cx);
        let book = &self.books[self.active];
        let field = |name: &str, value: String| {
            column()
                .gap_1()
                .child(
                    div()
                        .text_size(px(11.))
                        .text_color(p.muted)
                        .child(name.to_owned()),
                )
                .child(div().text_size(px(12.)).child(value))
        };
        let info = column()
            .id("document-information")
            .test_support()
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .p_3()
            .gap_3()
            .child(field(tr!("Name"), book.title.clone()))
            .child(field(tr!("Format"), book.format_label().into()))
            .when_some(self.reflow.frame.as_ref(), |info, frame| {
                info.child(field(tr!("Title"), frame.metadata.title.clone()))
                    .child(field(tr!("Author"), frame.metadata.author.clone()))
                    .child(field(
                        tr!("Chapters"),
                        frame.metadata.chapters.len().to_string(),
                    ))
            })
            .when_some(self.reflow.markdown.as_ref(), |info, document| {
                info.child(field(
                    tr!("Text"),
                    tf!("{} characters", document.characters),
                ))
            })
            .child(field(
                tr!("Reading progress"),
                format!("{:.0}%", book.reflow.progress * 100.),
            ))
            .child(field(
                tr!("File location"),
                match &book.source {
                    Source::File(path) => path.display().to_string(),
                    _ => String::new(),
                },
            ))
            // A Markdown image that could not be shown is a property of that one
            // image. It is reported here rather than as a status notice, which
            // describes the document as a whole.
            .when(self.reflow.images.failures > 0, |info| {
                info.child(field(
                    tr!("Images not shown"),
                    self.reflow.images.failures.to_string(),
                ))
            })
            .child(
                div()
                    .text_size(px(11.))
                    .text_color(p.muted)
                    .child(tr!("Read-only document")),
            );
        column()
            .id("document-panel")
            .test_support()
            .w(px(256.))
            .h_full()
            .min_h_0()
            .flex_shrink_0()
            .bg(p.sidebar)
            .border_l_1()
            .border_color(p.line)
            .child(
                row()
                    .id("document-panel-header")
                    .test_support()
                    .h(px(style::TOOLBAR_HEIGHT))
                    .flex_shrink_0()
                    .px_2()
                    .border_b_1()
                    .border_color(p.line)
                    .when(self.search_open, |bar| bar.child(self.search_bar(cx)))
                    .when(!self.search_open, |bar| {
                        bar.child(
                            Button::new("document-panel-information")
                                .ghost()
                                .small()
                                .icon(IconName::Info)
                                .selected(true)
                                .accessibility_label(tr!("Document information"))
                                .tooltip(tr!("Document information"))
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.cancel_search();
                                    this.search_open = false;
                                    this.search_restore_document_panel_open = None;
                                    this.document_panel.open = true;
                                    cx.notify();
                                })),
                        )
                        .child(div().flex_1())
                    })
                    .child(
                        self.command(
                            "document-panel-toggle",
                            IconName::PanelRight,
                            tr!("Document sidebar"),
                            &ToggleDocumentPanel,
                            cx,
                        )
                        .selected(true),
                    ),
            )
            .child(if self.search_open {
                self.reflow_search_results(cx).into_any_element()
            } else {
                info.into_any_element()
            })
            .into_any_element()
    }
    pub(super) fn load_reflow(&mut self, cx: &mut Context<Self>) {
        self.pending_window_policy = false;
        self.preview = None;
        self.reading.release_images(cx);
        self.reading = reading::ReadingView::default();
        self.raster.clear(cx);
        let source = self.books[self.active].source.clone();
        if self.reflow.source.as_ref() == Some(&source) {
            self.loading = false;
            cx.notify();
            return;
        }
        self.reflow.clear(cx);
        self.reflow.source = Some(source.clone());
        self.loading = true;
        let Source::File(path) = source else { return };
        match self.books[self.active].format() {
            Some(DocumentFormat::Markdown) => {
                let generation = self.generation;
                let work = cx
                    .background_spawn(async move { markdown::Document::open(&path).map(Arc::new) });
                self.reflow.task = Some(cx.spawn(async move |this, cx| {
                    let result = work.await;
                    let _ = this.update(cx, |this, cx| {
                        if this.generation != generation {
                            return;
                        }
                        this.loading = false;
                        match result {
                            Ok(document) => {
                                let list = ListState::new(
                                    document.blocks.len(),
                                    ListAlignment::Top,
                                    px(256.),
                                )
                                .with_uniform_item_height(px(72.));
                                let position = &this.books[this.active].reflow;
                                let restored = document.block_at_offset(position.markdown_offset);
                                list.scroll_to(ListOffset {
                                    item_ix: restored,
                                    offset_in_item: px(position.scroll_offset),
                                });
                                this.reflow.active_block = restored;
                                this.reflow.outline = Arc::new(
                                    document
                                        .headings
                                        .iter()
                                        .map(|heading| OutlineEntry {
                                            title: heading.title.clone(),
                                            depth: heading.level.saturating_sub(1),
                                            href: heading.anchor.clone(),
                                            block: heading.block,
                                        })
                                        .collect(),
                                );
                                let view = cx.entity().downgrade();
                                list.set_scroll_handler(move |event, _, cx| {
                                    // Kit dispatches this handler while the list's own
                                    // state is mutably borrowed for the scroll, so it
                                    // reports the event instead of asking the list
                                    // where it is; reading the list here panics with a
                                    // second borrow.
                                    let first = event.visible_range.start;
                                    let at_end = event.visible_range.end >= event.count;
                                    let _ = view.update(cx, |this, cx| {
                                        this.record_markdown_position(first, at_end, cx)
                                    });
                                });
                                this.reflow.list = list;
                                this.reflow.markdown = Some(document);
                            }
                            Err(error) => this.message = format!("{error:#}"),
                        }
                        cx.notify();
                    });
                }));
            }
            Some(DocumentFormat::Epub) => match EpubWorker::new() {
                Ok(worker) => {
                    self.reflow.worker = Some(worker);
                    self.request_epub(0, 0., None, cx);
                }
                Err(error) => {
                    self.loading = false;
                    self.message = format!("{error:#}");
                }
            },
            _ => {
                self.loading = false;
                self.message = tr!("Supported formats: PDF, EPUB and Markdown").into();
            }
        }
        cx.notify();
    }

    fn request_epub(
        &mut self,
        turn: i8,
        scroll: f32,
        find: Option<String>,
        cx: &mut Context<Self>,
    ) {
        let Source::File(path) = &self.books[self.active].source else {
            return;
        };
        if self.books[self.active].format() != Some(DocumentFormat::Epub) {
            return;
        }
        let (width, height, scale) = self.reflow.viewport;
        // Queue the step and, while a request is on its way, let it land first:
        // its result carries the position the next step starts from.
        self.reflow.pending_turn = self.reflow.pending_turn.saturating_add(i16::from(turn));
        if turn != 0 && self.reflow.task.is_some() {
            return;
        }
        let step = self.reflow.pending_turn.signum() as i8;
        self.reflow.pending_turn -= i16::from(step);
        let request = epub::Request {
            path: path.clone(),
            location: self.books[self.active].reflow.epub.clone(),
            width,
            height,
            scale,
            font_size: self.books[self.active].reflow.font_size as f32,
            turn: step,
            scroll,
            find,
            // Two-page reading is the document's own setting.
            spread: self.books[self.active].reflow.spread,
        };
        if self.reflow.request.as_ref() == Some(&request) && self.reflow.task.is_some() {
            return;
        }
        let Some(worker) = &self.reflow.worker else {
            return;
        };
        let receiver = match worker.request(request.clone()) {
            Ok(receiver) => receiver,
            Err(error) => {
                self.message = format!("{error:#}");
                self.loading = false;
                return;
            }
        };
        self.reflow.request = Some(request);
        self.reflow.epoch += 1;
        let epoch = self.reflow.epoch;
        self.reflow.task = Some(cx.spawn(async move |this, cx| {
            let result = receiver.await;
            let _ = this.update(cx, |this, cx| {
                if this.reflow.epoch != epoch || this.screen != Screen::Reader {
                    return;
                }
                this.loading = false;
                this.reflow.task = None;
                match result
                    .map_err(anyhow::Error::from)
                    .and_then(|result| result)
                {
                    Ok(mut frame) => {
                        // The buffer is moved into the texture: copying the page
                        // on the UI thread is what made a turn stutter.
                        // Raster dimensions are device pixels at the request
                        // scale; layout and the text layer are in logical page
                        // space, so normalize the displayed aspect only.
                        let page_sizes = frame
                            .pages
                            .iter()
                            .map(|page| {
                                let scale = scale.max(f32::EPSILON);
                                (page.width as f32 / scale, page.height as f32 / scale)
                            })
                            .collect::<Vec<_>>();
                        let images = frame
                            .pages
                            .drain(..)
                            .map(|page| {
                                texture(Pixels {
                                    width: page.width,
                                    height: page.height,
                                    bgra: page.bgra,
                                })
                            })
                            .collect::<Result<Vec<_>>>();
                        match images {
                            Ok(images) => {
                                this.clear_reflow_text_selection();
                                this.reflow.page_sizes = page_sizes;
                                let old = std::mem::replace(&mut this.reflow.page_images, images);
                                if !old.is_empty() {
                                    cx.defer(move |cx| {
                                        for image in old {
                                            cx.drop_image(image, None);
                                        }
                                    });
                                }
                                this.books[this.active].reflow.epub = frame.location.clone();
                                this.books[this.active].reflow.progress = frame.progress;
                                // The queue is consumed when a step is issued, so
                                // it survives this landing and the steps behind
                                // it are issued below.
                                this.reflow.outline = Arc::new(
                                    frame
                                        .metadata
                                        .outline
                                        .iter()
                                        .map(|entry| OutlineEntry {
                                            title: entry.title.clone(),
                                            depth: entry.depth,
                                            href: entry.href.clone(),
                                            block: 0,
                                        })
                                        .collect(),
                                );
                                this.reflow.frame = Some(frame);
                                this.reflow.scroll_accum = 0.;
                                this.message.clear();
                                this.prefetch_epub(cx);
                            }
                            Err(error) => this.message = format!("{error:#}"),
                        }
                    }
                    Err(error) => this.message = format!("{error:#}"),
                }
                // A burst of clicks queued steps while this one rendered; issue
                // the next from the position that just landed.
                if this.reflow.pending_turn != 0 {
                    this.request_epub(0, 0., None, cx);
                }
                cx.notify();
            });
        }));
    }

    /// Store the reading position the list reports for a scrolled Markdown view.
    ///
    /// This must not call into [`ListState`]: Kit runs the scroll handler while
    /// the list mutably borrows its own state, so any list query here panics
    /// with `RefCell already mutably borrowed`. The caller passes the reported
    /// block instead.
    fn record_markdown_position(&mut self, block: usize, at_end: bool, cx: &mut Context<Self>) {
        if self
            .books
            .get(self.active)
            .is_none_or(|b| b.format() != Some(DocumentFormat::Markdown))
        {
            return;
        }
        let Some(document) = &self.reflow.markdown else {
            return;
        };
        if document.blocks.is_empty() {
            return;
        }
        let index = block.min(document.blocks.len() - 1);
        self.reflow.active_block = index;
        let source_start = document.blocks[index].source.start;
        let position = &mut self.books[self.active].reflow;
        position.markdown_offset = source_start;
        position.scroll_offset = 0.;
        position.progress = if at_end {
            1.
        } else {
            source_start as f32 / document.source_bytes.max(1) as f32
        };
        cx.notify();
    }

    /// Remember the current position after a command moved the list.
    ///
    /// Commands run outside Kit's scroll dispatch, so this may read the list.
    /// Prepare the next page while the reader looks at this one, so a turn
    /// hands over a raster that already exists.
    fn prefetch_epub(&mut self, cx: &mut Context<Self>) {
        if self.books[self.active].format() != Some(DocumentFormat::Epub) {
            return;
        }
        let Source::File(path) = &self.books[self.active].source else {
            return;
        };
        let Some(worker) = &self.reflow.worker else {
            return;
        };
        // Build the prefetch on the page now on screen: the stored request
        // carries the viewport and text size it was produced with, but its
        // location is the one the turn started from.
        let mut next = match &self.reflow.request {
            Some(request) => request.clone(),
            None => return,
        };
        next.path = path.clone();
        next.location = self.books[self.active].reflow.epub.clone();
        next.turn = 1;
        next.scroll = 0.;
        next.find = None;
        let _ = worker.prefetch(next);
        let _ = cx;
    }

    fn remember_markdown_position(&mut self, cx: &mut Context<Self>) {
        let top = self.reflow.list.logical_scroll_top();
        let at_end = self.reflow.list.is_scrolled_to_end() == Some(true);
        self.record_markdown_position(top.item_ix, at_end, cx);
    }

    pub(super) fn turn_reflow(&mut self, delta: i32, cx: &mut Context<Self>) {
        if self.books[self.active].format() == Some(DocumentFormat::Epub) {
            self.request_epub(delta.signum() as i8, 0., None, cx);
        } else {
            self.reflow
                .list
                .scroll_by(px(delta as f32 * self.reflow.viewport.1 * 0.85));
            self.remember_markdown_position(cx);
        }
        cx.notify();
    }

    pub(super) fn scroll_markdown(&mut self, distance: gpui_kit::Pixels, cx: &mut Context<Self>) {
        if self.reflow.markdown.is_none() {
            return;
        }
        let mut offset = self.reflow.list.scroll_px_offset_for_scrollbar();
        offset.y -= distance;
        // Use the list's bounded pixel offset before recording its position.
        // Unlike scroll_by, this cannot temporarily overshoot the document end.
        self.reflow.list.set_offset_from_scrollbar(offset);
        let top = self.reflow.list.logical_scroll_top();
        let at_end = -self.reflow.list.scroll_px_offset_for_scrollbar().y
            >= self.reflow.list.max_offset_for_scrollbar().y;
        self.record_markdown_position(top.item_ix, at_end, cx);
    }

    /// Two-page EPUB reading.
    ///
    /// The chapter is re-paginated at half the content width, so a toggle is a
    /// new layout rather than a re-scale of the page already on screen. Only the
    /// fraction read is preserved, exactly as when the viewport or text size
    /// changes.
    pub(super) fn toggle_reflow_spread(&mut self, cx: &mut Context<Self>) {
        if self.books[self.active].format() != Some(DocumentFormat::Epub) {
            return;
        }
        let spread = &mut self.books[self.active].reflow.spread;
        *spread = !*spread;
        self.reflow.scroll_accum = 0.;
        self.request_epub(0, 0., None, cx);
        cx.notify();
    }

    pub(super) fn resize_reflow_text(&mut self, delta: i16, cx: &mut Context<Self>) {
        let current = &mut self.books[self.active].reflow.font_size;
        *current = (*current as i16 + delta).clamp(12, 32) as u8;
        self.reflow.list.remeasure();
        self.request_epub(0, 0., None, cx);
        cx.notify();
    }

    fn jump_reflow(&mut self, href: String, block: usize, cx: &mut Context<Self>) {
        if self.books[self.active].format() == Some(DocumentFormat::Epub) {
            self.books[self.active].reflow.epub = breeze_core::document::EpubLocation {
                href,
                progression: 0.,
            };
            self.request_epub(0, 0., None, cx);
        } else {
            self.reflow.active_block = block;
            self.reflow.list.scroll_to(ListOffset {
                item_ix: block,
                offset_in_item: px(0.),
            });
            self.remember_markdown_position(cx);
        }
        cx.notify();
    }

    pub(super) fn reflow_reader(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Div {
        if self.full_screen {
            return column().size_full().child(self.reflow_content(window, cx));
        }
        let p = self.p(cx);
        let epub = self.books[self.active].format() == Some(DocumentFormat::Epub);
        let main = column()
            .size_full()
            .min_w_0()
            .min_h_0()
            .child(
                row()
                    .id("reader-toolbar")
                    .test_support()
                    .overflow_x_scroll()
                    .h(px(style::TOOLBAR_HEIGHT))
                    .flex_shrink_0()
                    .px_2()
                    .gap_1()
                    .border_b_1()
                    .border_color(p.line)
                    .when(!self.sidebar, |bar| {
                        bar.child(self.command(
                            "collapse",
                            IconName::PanelLeft,
                            tr!("Navigation panel"),
                            &ToggleSidebar,
                            cx,
                        ))
                    })
                    // Markdown authors in the same content area the reader uses;
                    // these controls stay inside the existing toolbar.
                    .when(self.books[self.active].is_markdown(), |bar| {
                        let editing = self.books[self.active].markdown_editing;
                        bar.child(
                            Button::new("markdown-edit-toggle")
                                .ghost()
                                .small()
                                .icon(if editing {
                                    IconName::BookOpenText
                                } else {
                                    IconName::Pencil
                                })
                                .selected(editing)
                                .accessibility_label(if editing {
                                    tr!("Reading view")
                                } else {
                                    tr!("Edit document")
                                })
                                .tooltip(if editing {
                                    tr!("Reading view")
                                } else {
                                    tr!("Edit document")
                                })
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.toggle_markdown_editing(window, cx)
                                })),
                        )
                    })
                    .when(
                        self.books[self.active].is_markdown()
                            && self.books[self.active].markdown_editing,
                        |bar| {
                            let view = self.books[self.active]
                                .markdown_edit
                                .as_ref()
                                .map(|editor| editor.read(cx).view());
                            let live = view == Some(markdown_editor::EditView::Live);
                            let source = view == Some(markdown_editor::EditView::Source);
                            bar.child(
                                Button::new("markdown-live-view")
                                    .ghost()
                                    .small()
                                    .selected(live)
                                    .accessibility_label(tr!("Live formatting"))
                                    .tooltip(tr!("Live formatting"))
                                    .child(IconName::Type)
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.set_markdown_view(markdown_editor::EditView::Live, cx)
                                    })),
                            )
                            .child(
                                Button::new("markdown-source-view")
                                    .ghost()
                                    .small()
                                    .selected(source)
                                    .accessibility_label(tr!("Markdown source"))
                                    .tooltip(tr!("Markdown source"))
                                    .child(IconName::TextCursorInput)
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.set_markdown_view(
                                            markdown_editor::EditView::Source,
                                            cx,
                                        )
                                    })),
                            )
                        },
                    )
                    .child(div().flex_1())
                    .child(
                        self.command(
                            "reader-search",
                            IconName::Search,
                            tr!("Search document"),
                            &SearchDocument,
                            cx,
                        )
                        .selected(self.search_open),
                    )
                    .child(
                        self.command(
                            "reader-star",
                            IconName::Bookmark,
                            tr!("Favorites"),
                            &Favorite,
                            cx,
                        )
                        .icon(document_icons::bookmark_icon(
                            self.books[self.active].favorite,
                        )),
                    )
                    .when(!self.document_panel.open, |bar| {
                        bar.child(self.command(
                            "document-panel-toggle",
                            IconName::PanelRight,
                            tr!("Document sidebar"),
                            &ToggleDocumentPanel,
                            cx,
                        ))
                    }),
            )
            .child(self.reflow_content(window, cx))
            .child(
                row()
                    .id("reflow-status")
                    .test_support()
                    .h(px(32.))
                    .flex_shrink_0()
                    .px_2()
                    .gap_1()
                    .border_t_1()
                    .border_color(p.line)
                    .bg(p.bg)
                    // Page turns stay on the keyboard; the status bar starts
                    // with the reading position.
                    .when(
                        self.books[self.active].markdown_editing
                            && self.books[self.active].markdown_edit.is_some(),
                        |bar| {
                            let status = self.books[self.active]
                                .markdown_edit
                                .as_ref()
                                .map(|editor| editor.read(cx).status());
                            bar.child(
                                div()
                                    .id("markdown-editor-status")
                                    .test_support()
                                    .text_size(px(11.))
                                    .text_color(p.muted)
                                    .when_some(status, |item, status| item.child(status)),
                            )
                        },
                    )
                    .child({
                        let position = self.reflow_position_text();
                        div()
                            .id("reflow-position")
                            .test_support()
                            .min_w(px(56.))
                            .text_size(px(12.))
                            // The visible text is a page count; name it for
                            // assistive technology and for the tests that pin it.
                            .aria_label(position.clone())
                            .child(position)
                    })
                    .when(!epub, |bar| match self.markdown_characters(cx) {
                        Some(characters) => {
                            // The accessibility label repeats the visible text
                            // so tests and assistive technology read one value.
                            let text: SharedString =
                                tf!("{} characters", grouped_characters(characters)).into();
                            bar.child(
                                div()
                                    .id("markdown-characters")
                                    .test_support()
                                    .flex_shrink_0()
                                    .text_size(px(11.))
                                    .text_color(p.muted)
                                    .aria_label(text.clone())
                                    .tooltip(|window, cx| {
                                        Tooltip::new(tr!("Characters in the Markdown text"))
                                            .build(window, cx)
                                    })
                                    .child(text),
                            )
                        }
                        None => bar,
                    })
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(50.))
                            .overflow_hidden()
                            .text_size(px(11.))
                            .text_color(p.muted)
                            .when_some(self.reflow_notice(), |area, notice| {
                                area.child(
                                    div()
                                        .id("reflow-status-notice")
                                        .test_support()
                                        .child(notice),
                                )
                            }),
                    )
                    .when(epub, |bar| {
                        // Two-page reading is a view state: the button keeps the
                        // pressed look while the spread is on.
                        let spread = self.books[self.active].reflow.spread;
                        bar.child(
                            Button::new("reflow-spread")
                                .ghost()
                                .small()
                                .icon(IconName::Columns2)
                                // The pressed look and the toggle state are
                                // separate in Kit: one paints, one is announced.
                                .selected(spread)
                                .toggled(spread)
                                .accessibility_label(if spread {
                                    tr!("Single-page reading")
                                } else {
                                    tr!("Two-page reading")
                                })
                                .tooltip(if spread {
                                    tr!("Single-page reading")
                                } else {
                                    tr!("Two-page reading")
                                })
                                .on_click(
                                    cx.listener(|this, _, _, cx| this.toggle_reflow_spread(cx)),
                                ),
                        )
                    })
                    .when(!epub, |bar| {
                        let full_width = self.books[self.active].reflow.full_width;
                        bar.child(
                            Button::new("reflow-width")
                                .ghost()
                                .small()
                                .icon(if full_width {
                                    IconName::ChevronsRightLeft
                                } else {
                                    IconName::ChevronsLeftRight
                                })
                                .selected(full_width)
                                .accessibility_label(if full_width {
                                    tr!("Use the maximum reading width")
                                } else {
                                    tr!("Use the full window width")
                                })
                                .tooltip(if full_width {
                                    tr!("Use the maximum reading width")
                                } else {
                                    tr!("Use the full window width")
                                })
                                .on_click(cx.listener(|this, _, _, cx| {
                                    let full = &mut this.books[this.active].reflow.full_width;
                                    *full = !*full;
                                    this.reflow.list.remeasure();
                                    cx.notify();
                                })),
                        )
                    })
                    .child(
                        Button::new("text-smaller")
                            .ghost()
                            .small()
                            .icon(IconName::Minus)
                            .accessibility_label(tr!("Decrease text size"))
                            .tooltip(tr!("Decrease text size"))
                            .disabled(self.books[self.active].reflow.font_size <= 12)
                            .on_click(
                                cx.listener(|this, _, _, cx| this.resize_reflow_text(-1, cx)),
                            ),
                    )
                    .child(
                        div()
                            .id("text-size")
                            .test_support()
                            .min_w(px(48.))
                            .text_center()
                            .text_size(px(12.))
                            .child(format!("{} px", self.books[self.active].reflow.font_size)),
                    )
                    .child(
                        Button::new("text-larger")
                            .ghost()
                            .small()
                            .icon(IconName::Plus)
                            .accessibility_label(tr!("Increase text size"))
                            .tooltip(tr!("Increase text size"))
                            .disabled(self.books[self.active].reflow.font_size >= 32)
                            .on_click(cx.listener(|this, _, _, cx| this.resize_reflow_text(1, cx))),
                    )
                    .child(self.fullscreen_button(cx)),
            );
        let center = if self.sidebar {
            h_resizable("reader-split")
                .with_state(&self.navigation_split)
                .with_handle_appearance(reader::navigation_handle_appearance(p))
                .child(
                    resizable_panel()
                        .size(px(self.navigation_width))
                        .size_range(
                            px(reader::NAVIGATION_MIN_WIDTH)..px(reader::NAVIGATION_MAX_WIDTH),
                        )
                        .child(self.reflow_navigation(cx)),
                )
                .child(
                    resizable_panel()
                        .size_range(
                            px(if self.is_split() { 160. } else { 300. })..gpui_kit::Pixels::MAX,
                        )
                        .child(main),
                )
                .into_any_element()
        } else {
            main.into_any_element()
        };
        row()
            .flex_1()
            .min_w(px(if self.is_split() {
                self.navigation_width * f32::from(self.sidebar) + self.document_panel.width() + 160.
            } else {
                0.
            }))
            .min_h_0()
            .items_stretch()
            .child(div().size_full().min_w_0().min_h_0().child(center))
            .when(self.document_panel.open, |row| {
                row.child(self.document_sidebar(cx))
            })
    }

    /// The page the reader is on, as `current / total`.
    ///
    /// EPUB is paginated one chapter at a time — laying out the whole book to
    /// number its pages would cost what the chapter-local design exists to avoid
    /// — so the count is the page within the open chapter. Markdown has no pages
    /// and keeps reporting its reading progress.
    /// Characters the Markdown reading surface shows, or `None` for EPUB.
    ///
    /// An open draft reports its own projection, so the count follows the text
    /// on screen while it is edited. The loaded document supplies the count
    /// otherwise, which is the same rendered-text measure the document details
    /// panel already reports.
    fn markdown_characters(&self, cx: &App) -> Option<usize> {
        let book = &self.books[self.active];
        book.markdown_editing
            .then_some(book.markdown_edit.as_ref())
            .flatten()
            .map(|editor| editor.read(cx).characters())
            .or_else(|| {
                self.reflow
                    .markdown
                    .as_ref()
                    .map(|document| document.characters)
            })
    }

    fn reflow_position_text(&self) -> String {
        if let Some(frame) = &self.reflow.frame {
            let left = frame.chapter_page + 1;
            let right = (frame.chapter_page + frame.columns).min(frame.chapter_pages);
            return if right > left {
                format!("{left}\u{2013}{right} / {}", frame.chapter_pages)
            } else {
                format!("{left} / {}", frame.chapter_pages)
            };
        }
        format!("{:.0}%", self.books[self.active].reflow.progress * 100.)
    }

    /// The one status notice the reflow reader shows, or `None` when there is
    /// nothing to report. A healthy document leaves the area empty instead of
    /// announcing that nothing is wrong.
    fn reflow_notice(&self) -> Option<String> {
        if self.loading {
            Some(tr!("Reading…").into())
        } else if !self.message.is_empty() {
            Some(self.message.clone())
        } else {
            None
        }
    }

    /// Which outline row the reader is inside of.
    ///
    /// EPUB rows match the open chapter; Markdown rows are the last heading at
    /// or before the block at the top of the view.
    fn reflow_active_outline(&self) -> Option<usize> {
        let entries = &self.reflow.outline;
        if entries.is_empty() {
            return None;
        }
        match self.books[self.active].format() {
            Some(DocumentFormat::Epub) => {
                let frame = self.reflow.frame.as_ref()?;
                let chapter = &frame.metadata.chapters[frame.chapter].href;
                entries
                    .iter()
                    .position(|entry| entry.href.split('#').next() == Some(chapter.as_str()))
                    .or(Some(0))
            }
            _ => entries
                .iter()
                .rposition(|entry| entry.block <= self.reflow.active_block),
        }
    }

    fn reflow_outline_search_open(&self) -> bool {
        self.reflow.outline_search.is_some()
    }

    fn reflow_toggle_outline_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(search) = self.reflow.outline_search.take() {
            let mut scroll = self.reflow.outline_scroll.0.borrow_mut();
            scroll.deferred_scroll_to_item = None;
            scroll.base_handle.set_offset(search.saved_offset);
            window.focus(&self.focus, cx);
        } else {
            let input =
                cx.new(|cx| InputState::new(window, cx).placeholder(tr!("Search outline titles")));
            let subscription = cx.subscribe(&input, |this, input, event, cx| {
                if matches!(event, InputEvent::Change)
                    && let Some(search) = &mut this.reflow.outline_search
                    && search.input == input
                {
                    search.collapsed = if input.read(cx).value().trim().is_empty() {
                        this.reflow.outline_folds.clone()
                    } else {
                        HashSet::new()
                    };
                    this.reflow
                        .outline_scroll
                        .scroll_to_item(0, ScrollStrategy::Top);
                    cx.notify();
                }
            });
            input.update(cx, |input, cx| input.focus(window, cx));
            self.reflow.outline_search = Some(OutlineSearch {
                input,
                collapsed: self.reflow.outline_folds.clone(),
                saved_offset: self.reflow.outline_scroll.0.borrow().base_handle.offset(),
                _subscription: subscription,
            });
        }
        cx.notify();
    }

    fn reflow_outline_search_bar(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let search = self.reflow.outline_search.as_ref()?;
        Some(
            search::search_row(self.p(cx).line)
                .id("outline-search-bar")
                .test_support()
                .w_full()
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .child(Input::new(&search.input).small()),
                )
                .child(
                    Button::new("close-outline-search")
                        .ghost()
                        .small()
                        .icon(IconName::X)
                        .accessibility_label(tr!("Close outline search"))
                        .tooltip(tr!("Close outline search"))
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.reflow_toggle_outline_search(window, cx)
                        })),
                )
                .on_action(cx.listener(
                    |this, _: &gpui_kit::component::input::Escape, window, cx| {
                        this.reflow_toggle_outline_search(window, cx);
                    },
                ))
                .into_any_element(),
        )
    }

    fn reflow_outline_menu(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let view = cx.entity();
        Button::new("outline-more")
            .ghost()
            .small()
            .icon(IconName::EllipsisVertical)
            .accessibility_label(tr!("More outline actions"))
            .tooltip(tr!("More outline actions"))
            .dropdown_menu(move |menu, window, _| {
                menu.item(
                    PopupMenuItem::new(tr!("Expand all"))
                        .icon(IconName::ListChevronsUpDown)
                        .on_click(window.listener_for(&view, |this, _, _, cx| {
                            this.reflow_set_outline_expanded(true, cx);
                        })),
                )
                .item(
                    PopupMenuItem::new(tr!("Collapse all"))
                        .icon(IconName::ListChevronsDownUp)
                        .on_click(window.listener_for(&view, |this, _, _, cx| {
                            this.reflow_set_outline_expanded(false, cx);
                        })),
                )
            })
    }

    fn reflow_set_outline_expanded(&mut self, expanded: bool, cx: &mut Context<Self>) {
        let outline = self.reflow.outline.clone();
        self.reflow.outline_folds.clear();
        if !expanded {
            self.reflow
                .outline_folds
                .extend((0..outline.len()).filter(|&index| outline_has_children(&outline, index)));
        }
        self.reflow
            .outline_scroll
            .scroll_to_item(0, ScrollStrategy::Top);
        cx.notify();
    }

    fn reflow_toggle_outline(&mut self, index: usize, cx: &mut Context<Self>) {
        if !self.reflow.outline_folds.insert(index) {
            self.reflow.outline_folds.remove(&index);
        }
        cx.notify();
    }

    fn jump_reflow_outline(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(entry) = self.reflow.outline.get(index).cloned() else {
            return;
        };
        if self.books[self.active].format() == Some(DocumentFormat::Epub) {
            self.jump_reflow(entry.href, 0, cx);
        } else {
            self.jump_reflow(String::new(), entry.block, cx);
        }
    }

    fn reflow_navigation(&self, cx: &mut Context<Self>) -> AnyElement {
        let p = self.p(cx);
        let query = self
            .reflow
            .outline_search
            .as_ref()
            .map(|search| search.input.read(cx).value().trim().to_lowercase())
            .unwrap_or_default();
        let nested = (0..self.reflow.outline.len())
            .any(|index| outline_has_children(&self.reflow.outline, index));
        let visible = visible_outline(&self.reflow.outline, self.reflow.outline_folds(), &query);
        column()
            .size_full()
            .min_w_0()
            .min_h_0()
            .bg(p.sidebar)
            .child(
                row()
                    .id("navigation-header")
                    .test_support()
                    .h(px(style::TOOLBAR_HEIGHT))
                    .flex_shrink_0()
                    .px(px(8.))
                    .border_b_1()
                    .border_color(p.line)
                    // The leading slot pins the collapse control to the header's
                    // left edge, the way the PDF reader's panel header does, and
                    // the trailing slot right-aligns the outline search.
                    .child(
                        row()
                            .id("navigation-header-leading")
                            .test_support()
                            .flex_1()
                            .min_w_0()
                            .justify_start()
                            .child(
                                self.command(
                                    "collapse",
                                    IconName::PanelLeft,
                                    tr!("Navigation panel"),
                                    &ToggleSidebar,
                                    cx,
                                )
                                .selected(true),
                            ),
                    )
                    .child(
                        TabBar::new("reflow-nav-tabs")
                            .segmented()
                            .small()
                            .flex_shrink_0()
                            .selected_index(0)
                            .child(
                                Tab::new()
                                    .icon(IconName::TableOfContents)
                                    .aria_label(tr!("Outline"))
                                    .tooltip(|window, cx| {
                                        Tooltip::new(tr!("Outline")).build(window, cx)
                                    }),
                            ),
                    )
                    .child(
                        row()
                            .id("navigation-header-trailing")
                            .test_support()
                            .flex_1()
                            .min_w_0()
                            .justify_end()
                            .child(
                                row()
                                    .gap(px(4.))
                                    .flex_shrink_0()
                                    .child(
                                        Button::new("outline-search")
                                            .ghost()
                                            .small()
                                            .icon(IconName::Search)
                                            .selected(self.reflow_outline_search_open())
                                            .accessibility_label(tr!("Search outline titles"))
                                            .tooltip(tr!("Search outline titles"))
                                            .on_click(cx.listener(|this, _, window, cx| {
                                                this.reflow_toggle_outline_search(window, cx);
                                            })),
                                    )
                                    .when(nested, |actions| {
                                        actions.child(self.reflow_outline_menu(cx))
                                    }),
                            ),
                    ),
            )
            .children(self.reflow_outline_search_bar(cx))
            .child(
                column()
                    .id("navigation-content")
                    .test_support()
                    .flex_1()
                    .min_h_0()
                    .child(self.reflow_outline_view(visible, cx)),
            )
            .into_any_element()
    }

    fn reflow_outline_view(&self, visible: Vec<usize>, cx: &mut Context<Self>) -> AnyElement {
        if visible.is_empty() {
            return div()
                .id("outline-no-results")
                .test_support()
                .p_2()
                .text_color(self.p(cx).muted)
                .child(tr!("No matching outline titles"))
                .into_any_element();
        }
        let outline = self.reflow.outline.clone();
        let nested = (0..outline.len()).any(|index| outline_has_children(&outline, index));
        let active = self.reflow_active_outline();
        let active = visible_active_outline(&visible, active);
        let folds = self.reflow.outline_folds();
        let folds = folds.clone();
        div()
            .size_full()
            .pt_1()
            .child(
                // A distinct id keeps this list from inheriting the PDF
                // outline element's cached measurements when a library switches
                // between a PDF and a reflowable document.
                uniform_list(
                    "reflow-outline-list",
                    visible.len(),
                    cx.processor(move |_this, range: std::ops::Range<usize>, _, cx| {
                        range
                            .map(|row_index| {
                                let index = visible[row_index];
                                let item = &outline[index];
                                let has_children = outline_has_children(&outline, index);
                                let collapsed = folds.contains(&index);
                                row()
                                    .w_full()
                                    .h(px(24.))
                                    .px_2()
                                    .pl(px(8. + item.depth.min(12) as f32 * 10.))
                                    .when(nested, |row| {
                                        row.child(if has_children {
                                            Button::new(("outline-toggle", index))
                                                .ghost()
                                                .small()
                                                .size(px(24.))
                                                .flex_shrink_0()
                                                .icon(if collapsed {
                                                    IconName::ChevronRight
                                                } else {
                                                    IconName::ChevronDown
                                                })
                                                .accessibility_label(format!(
                                                    "{}：{}",
                                                    if collapsed {
                                                        tr!("Expand")
                                                    } else {
                                                        tr!("Collapse")
                                                    },
                                                    item.title
                                                ))
                                                .tooltip(if collapsed {
                                                    tr!("Expand child entries")
                                                } else {
                                                    tr!("Collapse child entries")
                                                })
                                                .on_click(cx.listener(move |this, _, _, cx| {
                                                    this.reflow_toggle_outline(index, cx)
                                                }))
                                                .into_any_element()
                                        } else {
                                            div().w(px(24.)).flex_shrink_0().into_any_element()
                                        })
                                    })
                                    .child(
                                        Button::new(("outline", index))
                                            .ghost()
                                            .small()
                                            .h(px(24.))
                                            .flex_1()
                                            .min_w_0()
                                            .px_1()
                                            .accessibility_label(item.title.clone())
                                            .child(
                                                row().w_full().child(
                                                    div()
                                                        .flex_1()
                                                        .min_w_0()
                                                        .text_ellipsis()
                                                        .text_left()
                                                        .child(item.title.clone()),
                                                ),
                                            )
                                            .selected(active == Some(index))
                                            .tooltip(item.title.clone())
                                            .on_click(cx.listener(move |this, _, _, cx| {
                                                this.jump_reflow_outline(index, cx)
                                            })),
                                    )
                            })
                            .collect::<Vec<_>>()
                    }),
                )
                .track_scroll(&self.reflow.outline_scroll)
                .size_full(),
            )
            .into_any_element()
    }

    /// Reading and editing are two views of the same tab. The draft session is
    /// created on first entry and kept while the tab stays open, so switching
    /// tabs or returning to reading never discards it.
    pub(super) fn toggle_markdown_editing(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.books[self.active].is_markdown() {
            return;
        }
        if self.books[self.active].markdown_editing {
            self.books[self.active].markdown_editing = false;
            cx.notify();
            return;
        }
        if self.books[self.active].markdown_edit.is_none() {
            let Source::File(path) = self.books[self.active].source.clone() else {
                self.message = tr!("Editing needs a Markdown file on disk").into();
                cx.notify();
                return;
            };
            let editor = cx.new(|cx| markdown_editor::MarkdownEditor::open(path, cx));
            self.books[self.active].markdown_edit = Some(editor);
        }
        self.books[self.active].markdown_editing = true;
        if let Some(editor) = self.books[self.active].markdown_edit.clone() {
            let focus = editor.read(cx).focus_handle();
            window.focus(&focus, cx);
        }
        cx.notify();
    }

    pub(super) fn set_markdown_view(
        &mut self,
        view: markdown_editor::EditView,
        cx: &mut Context<Self>,
    ) {
        if let Some(editor) = self.books[self.active].markdown_edit.clone() {
            editor.update(cx, |editor, cx| editor.set_view(view, cx));
            cx.notify();
        }
    }

    /// Maps a pointer in a laid-out page image to normalized page coordinates.
    fn reflow_page_point(
        &self,
        page: usize,
        position: Point<gpui_kit::Pixels>,
    ) -> Option<[f32; 2]> {
        let bounds = *self.reflow.page_bounds.get(&page)?;
        let raster = *self.reflow.page_sizes.get(page)?;
        let display = contained_page_bounds(bounds, raster);
        if display.size.width <= px(0.) || display.size.height <= px(0.) {
            return None;
        }
        Some([
            (f32::from(position.x - display.origin.x) / f32::from(display.size.width))
                .clamp(0., 1.),
            (f32::from(position.y - display.origin.y) / f32::from(display.size.height))
                .clamp(0., 1.),
        ])
    }

    /// The laid-out page under a pointer, or the nearest page when the pointer
    /// is in the gutter or padding.
    fn reflow_page_at(&self, position: Point<gpui_kit::Pixels>) -> Option<usize> {
        self.reflow
            .page_bounds
            .iter()
            .min_by(|(page_a, a), (page_b, b)| {
                let a_display = contained_page_bounds(
                    **a,
                    self.reflow
                        .page_sizes
                        .get(**page_a)
                        .copied()
                        .unwrap_or((1., 1.)),
                );
                let b_display = contained_page_bounds(
                    **b,
                    self.reflow
                        .page_sizes
                        .get(**page_b)
                        .copied()
                        .unwrap_or((1., 1.)),
                );
                page_bounds_distance(a_display, position)
                    .total_cmp(&page_bounds_distance(b_display, position))
                    .then(page_a.cmp(page_b))
            })
            .map(|(page, _)| *page)
    }

    fn begin_reflow_text_selection(
        &mut self,
        page: usize,
        position: Point<gpui_kit::Pixels>,
        cx: &mut Context<Self>,
    ) {
        if self.books[self.active].state.mode != Mode::Read {
            return;
        }
        let caret = self.reflow_page_point(page, position).and_then(|point| {
            self.reflow
                .frame
                .as_ref()?
                .text_pages
                .get(page)?
                .caret(point)
        });
        let Some(caret) = caret else {
            return;
        };
        self.text_selection = None;
        self.selection.clear();
        self.selected_text.clear();
        self.reflow.text_selection = Some(ReflowTextSelection {
            anchor: (page, caret),
            focus: (page, caret),
        });
        self.reflow.selection_active = true;
        cx.notify();
    }

    fn update_reflow_text_selection(
        &mut self,
        page: usize,
        position: Point<gpui_kit::Pixels>,
        cx: &mut Context<Self>,
    ) {
        if !self.reflow.selection_active {
            return;
        }
        let caret = self.reflow_page_point(page, position).and_then(|point| {
            self.reflow
                .frame
                .as_ref()?
                .text_pages
                .get(page)?
                .caret(point)
        });
        let Some(caret) = caret else {
            return;
        };
        if let Some(selection) = &mut self.reflow.text_selection {
            selection.focus = (page, caret);
            self.refresh_reflow_text_selection();
            cx.notify();
        }
    }

    fn end_reflow_text_selection(&mut self, cx: &mut Context<Self>) {
        if !self.reflow.selection_active {
            return;
        }
        self.reflow.selection_active = false;
        if self
            .reflow
            .text_selection
            .is_some_and(ReflowTextSelection::collapsed)
        {
            self.clear_reflow_text_selection();
            self.message.clear();
        } else {
            self.refresh_reflow_text_selection();
            self.message = tf!(
                "{} characters selected · ⌘/Ctrl+C to copy",
                self.selected_text.chars().count()
            );
        }
        cx.notify();
    }

    fn refresh_reflow_text_selection(&mut self) {
        let Some(selection) = self.reflow.text_selection else {
            return;
        };
        let Some(frame) = self.reflow.frame.as_ref() else {
            return;
        };
        let mut selected = Vec::new();
        let mut rectangles = Vec::new();
        for (page, text) in frame.text_pages.iter().enumerate() {
            let range = selection.on_page(page, text.glyphs.len());
            if range.is_empty() {
                continue;
            }
            let piece = text.text(range.clone());
            if !piece.is_empty() {
                selected.push(piece);
                rectangles.push((page, text.rects(range)));
            }
        }
        self.selected_text = selected.join("\n");
        self.reflow.selection = rectangles;
    }

    fn clear_reflow_text_selection(&mut self) {
        self.reflow.text_selection = None;
        self.reflow.selection.clear();
        self.reflow.selection_active = false;
        self.selected_text.clear();
    }

    fn reflow_content(&mut self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let p = self.p(cx);
        let epub = self.books[self.active].format() == Some(DocumentFormat::Epub);
        let scale = window.scale_factor();
        let view = cx.entity().downgrade();
        let mut content = div()
            .id("reflow-viewport")
            .test_support()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .overflow_hidden()
            .bg(if epub { rgb(0xffffff).into() } else { p.bg })
            .on_prepaint(move |bounds, _, cx| {
                let _ = view.update(cx, |this, cx| {
                    let viewport = (
                        f32::from(bounds.size.width).max(240.).round(),
                        f32::from(bounds.size.height).max(200.).round(),
                        scale,
                    );
                    if this.reflow.viewport != viewport {
                        this.reflow.viewport = viewport;
                        if epub && !this.reflow.queued {
                            this.reflow.queued = true;
                            let epoch = this.reflow.epoch;
                            let queued_view = view.clone();
                            cx.defer(move |cx| {
                                let _ = queued_view.update(cx, |this, cx| {
                                    this.reflow.queued = false;
                                    if this.reflow.epoch == epoch {
                                        this.request_epub(0, 0., None, cx);
                                    }
                                });
                            });
                        }
                    }
                });
            });
        if epub {
            content = content.on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, _, cx| {
                if !this.reflow.selection_active {
                    return;
                }
                if let Some(page) = this.reflow_page_at(event.position) {
                    this.update_reflow_text_selection(page, event.position, cx);
                }
            }));
            content =
                content.on_scroll_wheel(cx.listener(|this, event: &ScrollWheelEvent, _, cx| {
                    // A trackpad emits dozens of wheel events for one swipe.
                    // Requesting a render for each of them re-laid out and
                    // superseded the page continuously, so the accumulator only
                    // turns a page when a whole viewport has been crossed.
                    let delta = event.delta.pixel_delta(px(20.));
                    // A third of a viewport per page: a mouse notch turns a page
                    // after a few clicks, and a trackpad flick crosses one or two
                    // without ever launching a render per event.
                    let page = (this.reflow.viewport.1 / 3.).max(120.);
                    this.reflow.scroll_accum -= f32::from(delta.y);
                    let mut turn = 0i8;
                    while this.reflow.scroll_accum >= page {
                        this.reflow.scroll_accum -= page;
                        turn = turn.saturating_add(1);
                    }
                    while this.reflow.scroll_accum <= -page {
                        this.reflow.scroll_accum += page;
                        turn = turn.saturating_sub(1);
                    }
                    if turn != 0 {
                        this.request_epub(turn.clamp(-8, 8), 0., None, cx);
                    }
                }));
            if !self.reflow.page_images.is_empty() {
                let spread = self
                    .reflow
                    .frame
                    .as_ref()
                    .is_some_and(|frame| frame.columns == 2);
                let images = self.reflow.page_images.clone();
                let page_sizes = self.reflow.page_sizes.clone();
                let selections = (0..images.len())
                    .map(|page| {
                        self.reflow
                            .selection
                            .iter()
                            .find(|(candidate, _)| *candidate == page)
                            .map(|(_, rects)| rects.clone())
                            .unwrap_or_default()
                    })
                    .collect::<Vec<_>>();
                let mut pages = row()
                    .id("reflow-pages")
                    .size_full()
                    .min_w_0()
                    .items_center()
                    .justify_center();
                for (index, image) in images.iter().enumerate() {
                    let page_size = page_sizes.get(index).copied().unwrap_or((1., 1.));
                    let rects = selections.get(index).cloned().unwrap_or_default();
                    let view = cx.entity();
                    pages = pages.child(
                        div()
                            .id(("reflow-page", index))
                            .test_support()
                            .relative()
                            .h_full()
                            .min_w_0()
                            .flex_1()
                            .cursor(CursorStyle::IBeam)
                            .on_prepaint(move |bounds, _, cx| {
                                view.update(cx, |this, _| {
                                    this.reflow.page_bounds.insert(index, bounds);
                                });
                            })
                            .capture_any_mouse_down(cx.listener(
                                move |this, event: &MouseDownEvent, _, cx| {
                                    this.begin_reflow_text_selection(index, event.position, cx);
                                },
                            ))
                            .capture_any_mouse_up(cx.listener(
                                move |this, _: &MouseUpEvent, _, cx| {
                                    this.end_reflow_text_selection(cx);
                                },
                            ))
                            .child(
                                img(image.clone())
                                    .size_full()
                                    .object_fit(ObjectFit::Contain),
                            )
                            .child(
                                canvas(
                                    move |_, _, _| rects,
                                    move |bounds, rects, window, _| {
                                        let display = contained_page_bounds(bounds, page_size);
                                        for rect in rects {
                                            let x = f32::from(display.origin.x)
                                                + rect[0] * f32::from(display.size.width);
                                            let y = f32::from(display.origin.y)
                                                + rect[1] * f32::from(display.size.height);
                                            window.paint_quad(fill(
                                                Bounds {
                                                    origin: point(px(x), px(y)),
                                                    size: size(
                                                        px((rect[2] - rect[0])
                                                            * f32::from(display.size.width)),
                                                        px((rect[3] - rect[1])
                                                            * f32::from(display.size.height)),
                                                    ),
                                                },
                                                Rgba {
                                                    a: 0.28,
                                                    ..rgb(annotation_editing::SELECTION_COLOR)
                                                },
                                            ));
                                        }
                                    },
                                )
                                .absolute()
                                .top_0()
                                .left_0()
                                .size_full(),
                            ),
                    );
                }
                // Keep an odd final spread centered at the same half-width as
                // its preceding pairs instead of enlarging its only page.
                if spread && images.len() == 1 {
                    pages = pages.child(div().h_full().flex_1().min_w_0());
                }
                content = content.child(pages);
            }
        } else if let Some(editor) = self.books[self.active]
            .markdown_editing
            .then(|| self.books[self.active].markdown_edit.clone())
            .flatten()
        {
            // Editing replaces the read-only text inside the same centred
            // measure; the reader's chrome and navigation stay untouched.
            let full_width = self.books[self.active].reflow.full_width;
            content = content.child(
                div()
                    .id("markdown-column")
                    .test_support()
                    .flex()
                    .size_full()
                    .min_h_0()
                    .justify_center()
                    .child(
                        div()
                            .id("markdown-measure")
                            .test_support()
                            .h_full()
                            .min_h_0()
                            .w_full()
                            .when(!full_width, |column| column.max_w(px(MARKDOWN_MAX_WIDTH)))
                            .child(editor),
                    ),
            );
        } else if self.reflow.markdown.is_some() {
            // Markdown reads at a comfortable measure by default. The status bar
            // toggles the full window width; the text column stays centred when
            // the maximum applies.
            let full_width = self.books[self.active].reflow.full_width;
            content = content.child(
                div()
                    .id("markdown-column")
                    .test_support()
                    .flex()
                    .size_full()
                    .min_h_0()
                    .justify_center()
                    .child(
                        div()
                            .id("markdown-measure")
                            .test_support()
                            .h_full()
                            .min_h_0()
                            .w_full()
                            .when(!full_width, |column| column.max_w(px(MARKDOWN_MAX_WIDTH)))
                            .child(
                                list(
                                    self.reflow.list.clone(),
                                    cx.processor(|this, index: usize, _, cx| {
                                        this.markdown_block(index, cx)
                                    }),
                                )
                                .with_sizing_behavior(ListSizingBehavior::Auto)
                                .size_full(),
                            ),
                    ),
            );
        }
        if self.loading || (self.reflow.frame.is_none() && self.reflow.markdown.is_none()) {
            content = content.child(div().p_4().text_color(p.muted).child(if self.loading {
                tr!("Reading…").into()
            } else {
                self.message.clone()
            }));
        }
        content.into_any_element()
    }

    fn markdown_block(&mut self, index: usize, cx: &mut Context<Self>) -> AnyElement {
        let Some(document) = self.reflow.markdown.clone() else {
            return div().into_any_element();
        };
        let Some(block) = document.blocks.get(index) else {
            return div().into_any_element();
        };
        let state = if let Some(position) = self.reflow.blocks.iter().position(|b| b.index == index)
        {
            let block = self
                .reflow
                .blocks
                .remove(position)
                .unwrap_or_else(|| unreachable!());
            let state = block.state.clone();
            self.reflow.blocks.push_back(block);
            state
        } else {
            let state = cx.new(|cx| TextViewState::html(&block.html, cx).selectable(true));
            let list = self.reflow.list.clone();
            let view = cx.entity().downgrade();
            let subscription = cx.observe(&state, move |_, _, cx| {
                // Remeasuring touches the list's own borrow, and this observer can
                // fire from inside a list layout pass because the text element
                // updates its state while it is being laid out. Deferring the
                // remeasure keeps the two borrows apart; the notification is for
                // the reader, never for the observed text state, which would
                // re-enter this observer.
                let list = list.clone();
                let view = view.clone();
                cx.defer(move |cx| {
                    list.remeasure_items(index..index + 1);
                    let _ = view.update(cx, |_, cx| cx.notify());
                });
            });
            while self.reflow.blocks.len() >= 96 {
                self.reflow.blocks.pop_front();
            }
            self.reflow.blocks.push_back(TextBlock {
                index,
                state: state.clone(),
                _subscription: subscription,
            });
            state
        };
        let view = cx.entity().downgrade();
        let source = self.books[self.active].source.clone();
        let images = cx.entity().downgrade();
        div()
            .id(("markdown-block", index))
            .test_support()
            .w_full()
            .px_4()
            .py_1()
            .min_h(px(20.))
            .text_size(px(self.books[self.active].reflow.font_size as f32))
            .child(
                TextView::new(&state)
                    .selectable(true)
                    .image_source(move |uri| {
                        reflow_images::source(images.clone(), source.clone(), uri.to_string())
                    })
                    .on_link_click(move |href, _, _, cx| {
                        let _ =
                            view.update(cx, |this, cx| this.follow_reflow_link(href.as_ref(), cx));
                    }),
            )
            .into_any_element()
    }

    fn follow_reflow_link(&mut self, href: &str, cx: &mut Context<Self>) {
        if let Some(anchor) = href.strip_prefix('#') {
            if let Some(block) = self
                .reflow
                .markdown
                .as_ref()
                .and_then(|doc| doc.anchor_block(anchor))
            {
                self.jump_reflow(String::new(), block, cx);
            }
        } else if href.starts_with("https://")
            || href.starts_with("http://")
            || href.starts_with("mailto:")
        {
            cx.open_url(href);
        } else if self.books[self.active].format() == Some(DocumentFormat::Epub) {
            self.jump_reflow(href.to_owned(), 0, cx);
        } else if !href.contains(':')
            && let Source::File(path) = &self.books[self.active].source
            && let Some(parent) = path.parent()
        {
            self.open_path(parent.join(href.split('#').next().unwrap_or(href)), cx);
        }
    }
}
/// Formats an EPUB result without exposing the spine filename when navigation
/// has no title for that chapter.
fn reflow_search_result_text(title: &str, href: &str, snippet: &str) -> String {
    if title.is_empty() || title == href {
        snippet.to_owned()
    } else {
        format!("{title} · {snippet}")
    }
}

/// Distance from a point to page bounds; zero inside them.
fn page_bounds_distance(
    bounds: Bounds<gpui_kit::Pixels>,
    position: gpui_kit::Point<gpui_kit::Pixels>,
) -> f32 {
    let dx = f32::from(
        (bounds.left() - position.x)
            .max(position.x - bounds.right())
            .max(px(0.)),
    );
    let dy = f32::from(
        (bounds.top() - position.y)
            .max(position.y - bounds.bottom())
            .max(px(0.)),
    );
    dx * dx + dy * dy
}

/// The image rectangle inside a laid-out page cell when `Contain` scales it.
fn contained_page_bounds(
    bounds: Bounds<gpui_kit::Pixels>,
    raster: (f32, f32),
) -> Bounds<gpui_kit::Pixels> {
    let scale = (f32::from(bounds.size.width) / raster.0)
        .min(f32::from(bounds.size.height) / raster.1)
        .max(0.);
    let width = raster.0 * scale;
    let height = raster.1 * scale;
    Bounds {
        origin: point(
            bounds.origin.x + px((f32::from(bounds.size.width) - width) / 2.),
            bounds.origin.y + px((f32::from(bounds.size.height) - height) / 2.),
        ),
        size: size(px(width), px(height)),
    }
}

/// Whether the row owns nested rows, the same test the PDF outline uses.
fn outline_has_children(outline: &[OutlineEntry], index: usize) -> bool {
    outline
        .get(index)
        .zip(outline.get(index + 1))
        .is_some_and(|(item, next)| next.depth > item.depth)
}

/// Outline rows to show, honouring folds and an optional title search. A row is
/// kept when it matches or when an ancestor matches, exactly like the PDF
/// outline filter.
fn visible_outline(
    outline: &[OutlineEntry],
    collapsed: &HashSet<usize>,
    query: &str,
) -> Vec<usize> {
    let visible = visible_outline_rows(outline, collapsed);
    if query.is_empty() {
        return visible;
    }
    let mut ancestors: Vec<usize> = Vec::new();
    let mut parents = Vec::with_capacity(outline.len());
    let mut included = Vec::with_capacity(outline.len());
    for (index, item) in outline.iter().enumerate() {
        while ancestors
            .last()
            .is_some_and(|&parent| outline[parent].depth >= item.depth)
        {
            ancestors.pop();
        }
        parents.push(ancestors.last().copied());
        included.push(item.title.to_lowercase().contains(query));
        ancestors.push(index);
    }
    for index in (0..outline.len()).rev() {
        if included[index]
            && let Some(parent) = parents[index]
        {
            included[parent] = true;
        }
    }
    visible
        .into_iter()
        .filter(|&index| included[index])
        .collect()
}

fn visible_outline_rows(outline: &[OutlineEntry], collapsed: &HashSet<usize>) -> Vec<usize> {
    let mut hidden_below = None;
    outline
        .iter()
        .enumerate()
        .filter_map(|(index, item)| {
            if hidden_below.is_some_and(|depth| item.depth > depth) {
                return None;
            }
            hidden_below = collapsed.contains(&index).then_some(item.depth);
            Some(index)
        })
        .collect()
}

/// Group a character count so a long document stays readable at a glance.
fn grouped_characters(count: usize) -> String {
    let digits = count.to_string();
    let mut grouped = String::with_capacity(digits.len() + digits.len() / 3);
    for (index, digit) in digits.char_indices() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            grouped.push(',');
        }
        grouped.push(digit);
    }
    grouped
}

/// A hidden active row belongs to the last visible ancestor preceding it.
fn visible_active_outline(visible: &[usize], active: Option<usize>) -> Option<usize> {
    active.and_then(|active| {
        visible
            .iter()
            .rev()
            .find(|&&index| index <= active)
            .copied()
    })
}

#[cfg(test)]
mod tests {
    use super::{
        ReflowTextSelection, contained_page_bounds, grouped_characters, reflow_search_result_text,
    };
    use crate::{Mode, Source, texture};
    use breeze_reflow::epub::{TextGlyph, TextLayer, TextLine};
    use gpui_kit::AppContext;
    use gpui_kit::TestAppContext;
    use gpui_kit::test::TestWindowExt;
    use gpui_kit::{Bounds, point, px, size};
    use std::sync::Arc;

    /// A two-line text layer with one glyph per character, laid out left to
    /// right at 10 px per glyph.
    fn text_layer(lines: &[&str]) -> TextLayer {
        let mut layer = TextLayer::default();
        for (row, text) in lines.iter().enumerate() {
            let start = layer.glyphs.len();
            for (column, character) in text.chars().enumerate() {
                let x = column as f32 / 10.;
                let y = row as f32 / 2.;
                layer.glyphs.push(TextGlyph {
                    character,
                    bounds: [x, y, x + 0.1, y + 0.1],
                });
            }
            layer.lines.push(TextLine {
                glyphs: start..layer.glyphs.len(),
                bounds: [
                    0.,
                    row as f32 / 2.,
                    text.chars().count() as f32 / 10.,
                    row as f32 / 2. + 0.1,
                ],
            });
        }
        layer
    }

    #[test]
    fn epub_selection_maps_carets_across_lines() {
        let text = text_layer(&["hello", "world"]);
        let first = text.caret([0.55, 0.05]).unwrap();
        let last = text.caret([0.45, 0.55]).unwrap();
        assert_eq!(first, 5, "a point past the line end lands at its caret end");
        assert_eq!(
            last, 10,
            "a point past the last glyph lands at the line end"
        );
        assert_eq!(text.text(first..last), "world");
        let rects = text.rects(first..last);
        assert_eq!(rects.len(), 1);
        assert!(rects[0][2] > rects[0][0]);
    }

    #[test]
    fn epub_selection_spans_visible_pages() {
        let selection = ReflowTextSelection {
            anchor: (0, 3),
            focus: (1, 2),
        };
        assert_eq!(selection.on_page(0, 5), 3..5);
        assert_eq!(selection.on_page(1, 5), 0..2);
        assert_eq!(selection.on_page(2, 5), 0..0);
    }

    #[test]
    fn contained_image_bounds_centre_the_page() {
        let bounds = Bounds {
            origin: point(px(10.), px(20.)),
            size: size(px(400.), px(400.)),
        };
        let display = contained_page_bounds(bounds, (200., 100.));
        assert_eq!(display.origin.x, px(10.));
        assert_eq!(display.origin.y, px(120.));
        assert_eq!(display.size.width, px(400.));
        assert_eq!(display.size.height, px(200.));
    }

    #[test]
    fn search_result_hides_spine_filenames() {
        assert_eq!(
            reflow_search_result_text("index_split_004.html", "index_split_004.html", "…命中…"),
            "…命中…"
        );
        assert_eq!(
            reflow_search_result_text("鸣谢", "index_split_004.html", "…命中…"),
            "鸣谢 · …命中…"
        );
    }

    #[gpui_kit::test]
    fn epub_pointer_drag_selects_text_and_paints_rects(cx: &mut TestAppContext) {
        use breeze_reflow::epub::{self, TextGlyph, TextLine};

        fn layer(lines: &[(&str, f32)]) -> epub::TextLayer {
            let mut layer = epub::TextLayer::default();
            for (text, y) in lines {
                let start = layer.glyphs.len();
                for (column, character) in text.chars().enumerate() {
                    let x = 0.1 + column as f32 * 0.06;
                    layer.glyphs.push(TextGlyph {
                        character,
                        bounds: [x, *y, x + 0.05, *y + 0.05],
                    });
                }
                layer.lines.push(TextLine {
                    glyphs: start..layer.glyphs.len(),
                    bounds: [0.1, *y, 0.1 + text.chars().count() as f32 * 0.06, *y + 0.05],
                });
            }
            layer
        }

        let (handle, view) = crate::ui_tests::app(cx);
        let image = texture(breeze_pdf::Pixels {
            width: 10,
            height: 10,
            bgra: vec![255; 10 * 10 * 4],
        })
        .unwrap();
        cx.update_window(handle, |_, window, cx| {
            view.update(cx, |view, cx| {
                view.books[0].source = Source::File("/tmp/select.epub".into());
                view.books[0].state.mode = Mode::Read;
                view.reflow.frame = Some(epub::Frame {
                    metadata: Arc::new(epub::Metadata {
                        title: "Selectable".into(),
                        author: String::new(),
                        chapters: vec![epub::Chapter {
                            href: "select.xhtml".into(),
                            title: "Select".into(),
                        }],
                        outline: Vec::new(),
                        fingerprint: "test".into(),
                    }),
                    location: Default::default(),
                    chapter: 0,
                    chapter_page: 0,
                    chapter_pages: 1,
                    columns: 1,
                    width: 100,
                    height: 100,
                    pages: Vec::new(),
                    text_pages: vec![layer(&[("hello", 0.2), ("world", 0.4)])],
                    text: String::new(),
                    progress: 0.,
                    links: Vec::new(),
                    scroll_offset: 0.,
                    scroll_extent: 0.,
                });
                view.reflow.page_images = vec![image.clone()];
                view.reflow.page_sizes = vec![(100., 100.)];
                view.loading = false;
                cx.notify();
            });
            window.render_frame(cx);
        })
        .unwrap();
        let bounds = cx
            .update_window(handle, |_, window, _| {
                window.find(("reflow-page", 0usize)).bounds()
            })
            .unwrap();
        let start = point(
            bounds.origin.x + bounds.size.width * 0.2,
            bounds.origin.y + bounds.size.height * 0.25,
        );
        let end = point(
            bounds.origin.x + bounds.size.width * 0.8,
            bounds.origin.y + bounds.size.height * 0.45,
        );
        cx.update_window(handle, |_, window, cx| {
            window.drag(start, end, cx);
        })
        .unwrap();
        cx.update_window(handle, |_, _, cx| {
            let view = view.read(cx);
            assert!(!view.selected_text.is_empty(), "drag did not select text");
            assert!(
                view.reflow
                    .selection
                    .iter()
                    .any(|(_, rects)| !rects.is_empty()),
                "selection did not paint any rectangles"
            );
        })
        .unwrap();
    }

    #[test]
    fn character_counts_group_by_thousands() {
        assert_eq!(grouped_characters(0), "0");
        assert_eq!(grouped_characters(22), "22");
        assert_eq!(grouped_characters(999), "999");
        assert_eq!(grouped_characters(1_000), "1,000");
        assert_eq!(grouped_characters(47_025), "47,025");
        assert_eq!(grouped_characters(1_234_567), "1,234,567");
    }
}
