//! Native reflow readers share chrome, not PDF navigation or annotation state.
use crate::*;
use breeze_reflow::{EpubWorker, epub, markdown};
use gpui_kit::base::{TextView, TextViewState};
use gpui_kit::component::{ElementExt, h_resizable, resizable_panel};
use std::collections::VecDeque;

struct TextBlock {
    index: usize,
    state: Entity<TextViewState>,
    _subscription: Subscription,
}

pub(super) struct ReflowView {
    pub source: Option<Source>,
    pub markdown: Option<Arc<markdown::Document>>,
    pub frame: Option<epub::Frame>,
    image: Option<Arc<RenderImage>>,
    worker: Option<EpubWorker>,
    blocks: VecDeque<TextBlock>,
    pub list: ListState,
    task: Option<Task<()>>,
    request: Option<epub::Request>,
    viewport: (f32, f32, f32),
    epoch: u64,
    queued: bool,
    pub hits: Vec<(String, usize, String)>,
    pub images: reflow_images::Images,
}

impl Default for ReflowView {
    fn default() -> Self {
        Self {
            source: None,
            markdown: None,
            frame: None,
            image: None,
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
        }
    }
}

impl ReflowView {
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
        self.worker = None;
        self.images.clear(cx);
        if let Some(image) = self.image.take() {
            cx.defer(move |cx| cx.drop_image(image, None));
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
                        .map(|hit| (hit.href, 0, format!("{} · {}", hit.title, hit.snippet)))
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
                    .h(px(38.))
                    .flex_shrink_0()
                    .px_2()
                    .border_b_1()
                    .border_color(p.line)
                    .when(self.search_open, |bar| bar.child(self.search_bar(cx)))
                    .when(!self.search_open, |bar| {
                        bar.child(
                            div()
                                .flex_1()
                                .text_size(px(12.))
                                .child(tr!("Document information")),
                        )
                    })
                    .child(self.command(
                        "document-panel-toggle",
                        IconName::PanelRight,
                        tr!("Document sidebar"),
                        &ToggleDocumentPanel,
                        cx,
                    )),
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
                                list.scroll_to(ListOffset {
                                    item_ix: document.block_at_offset(position.markdown_offset),
                                    offset_in_item: px(position.scroll_offset),
                                });
                                let view = cx.entity().downgrade();
                                list.set_scroll_handler(move |_, _, cx| {
                                    let _ = view
                                        .update(cx, |this, cx| this.remember_markdown_position(cx));
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
        let request = epub::Request {
            path: path.clone(),
            location: self.books[self.active].reflow.epub.clone(),
            width,
            height,
            scale,
            font_size: self.books[self.active].reflow.font_size as f32,
            turn,
            scroll,
            find,
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
                    Ok(frame) => {
                        match texture(Pixels {
                            width: frame.width,
                            height: frame.height,
                            bgra: frame.bgra.as_ref().clone(),
                        }) {
                            Ok(image) => {
                                if let Some(old) = this.reflow.image.replace(image) {
                                    cx.defer(move |cx| cx.drop_image(old, None));
                                }
                                this.books[this.active].reflow.epub = frame.location.clone();
                                this.books[this.active].reflow.progress = frame.progress;
                                this.reflow.frame = Some(frame);
                                this.message.clear();
                            }
                            Err(error) => this.message = format!("{error:#}"),
                        }
                    }
                    Err(error) => this.message = format!("{error:#}"),
                }
                cx.notify();
            });
        }));
    }

    fn remember_markdown_position(&mut self, cx: &mut Context<Self>) {
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
        let top = self.reflow.list.logical_scroll_top();
        let Some(block) = document.blocks.get(top.item_ix) else {
            return;
        };
        let position = &mut self.books[self.active].reflow;
        position.markdown_offset = block.source.start;
        position.scroll_offset = f32::from(top.offset_in_item).max(0.);
        position.progress = if self.reflow.list.is_scrolled_to_end() == Some(true) {
            1.
        } else {
            block.source.start as f32 / document.source_bytes.max(1) as f32
        };
        cx.notify();
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
            self.reflow.list.scroll_to(ListOffset {
                item_ix: block,
                offset_in_item: px(0.),
            });
            self.remember_markdown_position(cx);
        }
        cx.notify();
    }

    fn change_chapter(&mut self, delta: i32, cx: &mut Context<Self>) {
        let Some(frame) = &self.reflow.frame else {
            return;
        };
        let index = (frame.chapter as i32 + delta)
            .clamp(0, frame.metadata.chapters.len() as i32 - 1) as usize;
        self.jump_reflow(frame.metadata.chapters[index].href.clone(), 0, cx);
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
                    .h(px(38.))
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
                    .when(epub, |bar| {
                        bar.child(
                            Button::new("previous-chapter")
                                .ghost()
                                .small()
                                .icon(IconName::ChevronLeft)
                                .accessibility_label(tr!("Previous chapter"))
                                .tooltip(tr!("Previous chapter"))
                                .on_click(
                                    cx.listener(|this, _, _, cx| this.change_chapter(-1, cx)),
                                ),
                        )
                        .child(
                            Button::new("next-chapter")
                                .ghost()
                                .small()
                                .icon(IconName::ChevronRight)
                                .accessibility_label(tr!("Next chapter"))
                                .tooltip(tr!("Next chapter"))
                                .on_click(cx.listener(|this, _, _, cx| this.change_chapter(1, cx))),
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
                    .child(div().w(px(1.)).h(px(18.)).mx_1().bg(p.line))
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
                    .child(div().flex_1())
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
                    .gap_2()
                    .border_t_1()
                    .border_color(p.line)
                    .text_size(px(11.))
                    .text_color(p.muted)
                    .child(self.books[self.active].format_label())
                    .child(self.reflow_status_text())
                    .child(div().flex_1())
                    .child(format!(
                        "{:.0}%",
                        self.books[self.active].reflow.progress * 100.
                    ))
                    .child(self.command(
                        "reflow-previous",
                        IconName::ChevronLeft,
                        tr!("Previous"),
                        &PreviousPage,
                        cx,
                    ))
                    .child(self.command(
                        "reflow-next",
                        IconName::ChevronRight,
                        tr!("Next"),
                        &NextPage,
                        cx,
                    )),
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
                        .size_range(px(300.)..gpui_kit::Pixels::MAX)
                        .child(main),
                )
                .into_any_element()
        } else {
            main.into_any_element()
        };
        row()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .items_stretch()
            .child(div().size_full().min_w_0().min_h_0().child(center))
            .when(self.document_panel.open, |row| {
                row.child(self.document_sidebar(cx))
            })
    }

    fn reflow_status_text(&self) -> String {
        if self.loading {
            return tr!("Reading…").into();
        }
        if !self.message.is_empty() {
            return self.message.clone();
        }
        if let Some(frame) = &self.reflow.frame {
            return tf!(
                "Chapter {} / {}",
                frame.chapter + 1,
                frame.metadata.chapters.len()
            );
        }
        self.reflow
            .markdown
            .as_ref()
            .map(|doc| tf!("{} characters", doc.characters))
            .unwrap_or_default()
    }

    fn reflow_navigation(&self, cx: &mut Context<Self>) -> AnyElement {
        let p = self.p(cx);
        let entries: Vec<(String, usize, String, usize)> = if let Some(doc) = &self.reflow.markdown
        {
            doc.headings
                .iter()
                .map(|h| {
                    (
                        h.title.clone(),
                        h.level.saturating_sub(1),
                        String::new(),
                        h.block,
                    )
                })
                .collect()
        } else if let Some(frame) = &self.reflow.frame {
            frame
                .metadata
                .outline
                .iter()
                .map(|h| (h.title.clone(), h.depth, h.href.clone(), 0))
                .collect()
        } else {
            Vec::new()
        };
        let entries = Arc::new(entries);
        column()
            .size_full()
            .min_w_0()
            .min_h_0()
            .bg(p.sidebar)
            .child(
                row()
                    .id("navigation-header")
                    .test_support()
                    .h(px(38.))
                    .flex_shrink_0()
                    .px_2()
                    .border_b_1()
                    .border_color(p.line)
                    .child(div().flex_1().text_size(px(12.)).child(tr!("Outline")))
                    .child(self.command(
                        "collapse",
                        IconName::PanelLeft,
                        tr!("Navigation panel"),
                        &ToggleSidebar,
                        cx,
                    )),
            )
            .child(
                div()
                    .id("navigation-content")
                    .test_support()
                    .flex_1()
                    .min_h_0()
                    .when(entries.is_empty(), |panel| {
                        panel.child(
                            div()
                                .p_3()
                                .text_size(px(12.))
                                .text_color(p.muted)
                                .child(tr!("No outline entries")),
                        )
                    })
                    .child(
                        uniform_list(
                            "reflow-outline",
                            entries.len(),
                            cx.processor(move |_, range: std::ops::Range<usize>, _, cx| {
                                range
                                    .map(|index| {
                                        let (title, depth, href, block) = entries[index].clone();
                                        let label = title.clone();
                                        Button::new(("reflow-heading", index))
                                            .ghost()
                                            .small()
                                            .w_full()
                                            .justify_start()
                                            .accessibility_label(label)
                                            .tooltip(title.clone())
                                            .child(
                                                div()
                                                    .w_full()
                                                    .pl(px((depth.min(8) * 12) as f32))
                                                    .text_left()
                                                    .text_ellipsis()
                                                    .child(title),
                                            )
                                            .on_click(cx.listener(move |this, _, _, cx| {
                                                this.jump_reflow(href.clone(), block, cx)
                                            }))
                                    })
                                    .collect::<Vec<_>>()
                            }),
                        )
                        .size_full(),
                    ),
            )
            .into_any_element()
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
            content =
                content.on_scroll_wheel(cx.listener(|this, event: &ScrollWheelEvent, _, cx| {
                    let delta = event.delta.pixel_delta(px(20.));
                    this.request_epub(0, -f32::from(delta.y), None, cx);
                }));
            if let Some(image) = &self.reflow.image {
                content = content.child(
                    img(image.clone())
                        .size_full()
                        .object_fit(ObjectFit::Contain),
                );
            }
        } else if self.reflow.markdown.is_some() {
            content = content.child(
                list(
                    self.reflow.list.clone(),
                    cx.processor(|this, index: usize, _, cx| this.markdown_block(index, cx)),
                )
                .with_sizing_behavior(ListSizingBehavior::Auto)
                .size_full(),
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
            let subscription = cx.observe(&state, move |_, _, cx| {
                list.remeasure_items(index..index + 1);
                cx.notify();
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
