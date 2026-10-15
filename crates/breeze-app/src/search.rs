use crate::*;
use std::sync::atomic::{AtomicBool, Ordering};

pub(super) fn search_row(border: Hsla) -> Div {
    row()
        .h(px(34.))
        .flex_shrink_0()
        .px_2()
        .gap_2()
        .border_b_1()
        .border_color(border)
}

impl Breeze {
    pub(super) fn run_search(&mut self, cx: &mut Context<Self>) {
        if !self.books[self.active].is_pdf() {
            self.run_reflow_search(cx);
            return;
        }
        let query = self.search_input.read(cx).value().to_string();
        if query.trim().is_empty() {
            return;
        }
        if self.search_busy {
            self.message = tr!("Cancel the current search first").into();
            cx.notify();
            return;
        }
        self.search_generation += 1;
        let generation = self.search_generation;
        self.search_hits.clear();
        self.search_busy = true;
        self.search_notice = tr!("Searching…").into();
        let token = Arc::new(AtomicBool::new(false));
        self.search_cancel = Some(token.clone());
        let source = self.books[self.active].source.clone();
        // Retained text is keyed by source revision, so a cancelled query only
        // costs the extraction it was already doing.
        let cache = self.search_cache.clone();
        let work =
            cx.background_spawn(async move { breeze_pdf::search(source, query, token, cache) });
        cx.spawn(async move |this, cx| {
            let result = work.await;
            let _ = this.update(cx, |this, cx| {
                if generation != this.search_generation {
                    cx.notify();
                    return;
                }
                this.search_busy = false;
                match result {
                    Ok(outcome) => {
                        this.search_notice = tf!(
                            "{} matches{}",
                            outcome.hits.len(),
                            if outcome.hits.len() == 300 {
                                tr!(" (limit reached)")
                            } else {
                                ""
                            }
                        );
                        this.search_hits = outcome.hits;
                        this.search_cache = outcome.cache;
                    }
                    Err(e) => this.search_notice = errors::error_text(&e),
                };
                this.search_cancel = None;
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
    pub(super) fn cancel_search(&mut self) {
        self.reflow.hits.clear();
        self.search_busy = false;
        if let Some(token) = &self.search_cancel {
            token.store(true, Ordering::Relaxed);
        }
        self.search_generation += 1;
        self.search_hits.clear();
        self.search_notice.clear();
    }
    pub(super) fn close_search(&mut self, cx: &mut Context<Self>) {
        self.cancel_search();
        self.search_open = false;
        if self.search_restore_document_panel_open.take() == Some(false) {
            self.document_panel.open = false;
        }
        cx.notify();
    }
    pub(super) fn search_bar(&self, cx: &mut Context<Self>) -> AnyElement {
        div()
            .id("document-panel-search-toolbar")
            .test_support()
            .flex_1()
            .min_w_0()
            .w_full()
            .child(
                row()
                    .h(px(style::TOOLBAR_HEIGHT))
                    .w_full()
                    .flex_shrink_0()
                    .px_1()
                    .gap_1()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .w_full()
                            .child(Input::new(&self.search_input).small()),
                    )
                    .child(
                        Button::new("cancel-search")
                            .ghost()
                            .small()
                            .icon(IconName::X)
                            .tooltip(tr!("Cancel and close search"))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.close_search(cx);
                            })),
                    ),
            )
            .into_any_element()
    }
    pub(super) fn search_results(&self, cx: &mut Context<Self>) -> Div {
        if !self.books[self.active].is_pdf() {
            return self.reflow_search_results(cx);
        }
        let p = self.p(cx);
        column()
            .flex_1()
            .min_h_0()
            .bg(p.sidebar)
            .p_2()
            .gap_2()
            .when(!self.search_notice.is_empty(), |results| {
                results.child(
                    div()
                        .text_size(px(12.))
                        .text_color(p.muted)
                        .child(self.search_notice.clone()),
                )
            })
            .child(
                column()
                    .id("search-results")
                    .test_support()
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .gap_1()
                    .children(self.search_hits.iter().enumerate().map(|(i, hit)| {
                        Button::new(("search-result", i))
                            .ghost()
                            .small()
                            .justify_start()
                            .max_w(px((self.document_panel.width() - 16.).max(100.)))
                            .overflow_hidden()
                            .accessibility_label(format!("{} · {}", hit.page + 1, hit.snippet))
                            .child(div().w_full().text_left().text_ellipsis().child(format!(
                                "{} · {}",
                                hit.page + 1,
                                hit.snippet
                            )))
                            .tooltip(hit.snippet.clone())
                            .on_click(cx.listener({
                                let page = hit.page;
                                move |this, _, _, cx| {
                                    this.go_to_annotation_page(page, cx);
                                }
                            }))
                    })),
            )
    }
}
