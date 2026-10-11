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
        let query = self.search_input.read(cx).value().to_string();
        if query.trim().is_empty() {
            return;
        }
        if self.search_busy {
            self.message = tr!("请先取消正在进行的搜索").into();
            cx.notify();
            return;
        }
        self.search_generation += 1;
        let generation = self.search_generation;
        self.search_hits.clear();
        self.search_busy = true;
        self.search_notice = tr!("正在搜索…").into();
        let token = Arc::new(AtomicBool::new(false));
        self.search_cancel = Some(token.clone());
        let source = self.books[self.active].source.clone();
        let work = cx.background_spawn(async move { breeze_pdf::search(source, query, token) });
        cx.spawn(async move |this, cx| {
            let result = work.await;
            let _ = this.update(cx, |this, cx| {
                this.search_busy = false;
                if generation != this.search_generation {
                    cx.notify();
                    return;
                }
                match result {
                    Ok(hits) => {
                        this.search_notice = tf!(
                            "{} 处匹配{}",
                            hits.len(),
                            if hits.len() == 300 {
                                tr!("（达到上限）")
                            } else {
                                ""
                            }
                        );
                        this.search_hits = hits;
                    }
                    Err(e) => this.search_notice = format!("{e:#}"),
                };
                this.search_cancel = None;
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
    pub(super) fn cancel_search(&mut self) {
        if let Some(token) = &self.search_cancel {
            token.store(true, Ordering::Relaxed);
        }
        self.search_generation += 1;
        self.search_hits.clear();
        self.search_notice = tr!("搜索已取消").into();
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
                    .h(px(38.))
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
                        Button::new("start-search")
                            .small()
                            .label(tr!("搜索"))
                            .disabled(self.search_busy)
                            .on_click(cx.listener(|this, _, _, cx| this.run_search(cx))),
                    )
                    .child(
                        Button::new("cancel-search")
                            .ghost()
                            .small()
                            .icon(IconName::X)
                            .tooltip(tr!("取消并关闭搜索"))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.close_search(cx);
                            })),
                    ),
            )
            .into_any_element()
    }
    pub(super) fn search_results(&self, cx: &mut Context<Self>) -> Div {
        let p = self.p(cx);
        column()
            .flex_1()
            .min_h_0()
            .bg(p.panel)
            .p_2()
            .gap_2()
            .child(
                div()
                    .text_size(px(12.))
                    .text_color(p.muted)
                    .child(self.search_notice.clone()),
            )
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
