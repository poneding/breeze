//! Visible-range scheduling. At most one UI request is awaiting the bounded PDF worker.
use crate::*;
use gpui_kit::component::ElementExt;
use gpui_kit::component::{
    VirtualListScrollHandle,
    menu::{DropdownMenu, PopupMenuItem},
    v_virtual_list,
};
use std::{
    collections::{HashMap, VecDeque},
    ops::Range,
    rc::Rc,
};
const PAGE_BUDGET: usize = 64 * 1024 * 1024;
const THUMB_BUDGET: usize = 8 * 1024 * 1024;

struct Resident<T> {
    page: usize,
    value: T,
    bytes: usize,
}
struct Cache<T> {
    entries: VecDeque<Resident<T>>,
    budget: usize,
    count: usize,
}
impl<T: Clone> Cache<T> {
    fn new(budget: usize, count: usize) -> Self {
        Self {
            entries: VecDeque::new(),
            budget,
            count,
        }
    }
    fn get(&self, page: usize) -> Option<T> {
        self.entries
            .iter()
            .find(|e| e.page == page)
            .map(|e| e.value.clone())
    }
    fn insert(&mut self, page: usize, value: T, bytes: usize, protected: Range<usize>) -> bool {
        self.entries.retain(|e| e.page != page);
        if bytes > self.budget {
            return false;
        }
        while self.entries.len() >= self.count
            || self.entries.iter().map(|e| e.bytes).sum::<usize>() + bytes > self.budget
        {
            if let Some(index) = self
                .entries
                .iter()
                .position(|e| !protected.contains(&e.page))
            {
                self.entries.remove(index);
            } else {
                return false;
            }
        }
        self.entries.push_back(Resident { page, value, bytes });
        true
    }
}
struct PageMetrics {
    width: f32,
    height: f32,
    zoom: Zoom,
    items: Rc<Vec<Size<gpui_kit::Pixels>>>,
    widest: usize,
}
pub(super) struct ReadingView {
    pub task: Option<Task<()>>,
    pub source: Option<Source>,
    pub fingerprint: String,
    pub sizes: Arc<Vec<(f32, f32)>>,
    pages: Cache<Arc<Preview>>,
    thumbnails: Cache<Arc<RenderImage>>,
    errors: HashMap<(bool, usize), String>,
    wanted_pages: Range<usize>,
    pending_pages: Option<(u64, Range<usize>)>,
    pending_thumbs: Option<(u64, Range<usize>)>,
    wanted_thumbs: Range<usize>,
    busy: bool,
    pub pages_scroll: VirtualListScrollHandle,
    pub thumbs_scroll: UniformListScrollHandle,
    metrics: Option<PageMetrics>,
}
impl Default for ReadingView {
    fn default() -> Self {
        Self {
            task: None,
            source: None,
            fingerprint: String::new(),
            sizes: Arc::new(Vec::new()),
            pages: Cache::new(PAGE_BUDGET, 8),
            thumbnails: Cache::new(THUMB_BUDGET, 64),
            errors: HashMap::new(),
            wanted_pages: 0..0,
            pending_pages: None,
            pending_thumbs: None,
            wanted_thumbs: 0..0,
            busy: false,
            pages_scroll: VirtualListScrollHandle::new(),
            thumbs_scroll: UniformListScrollHandle::new(),
            metrics: None,
        }
    }
}
impl ReadingView {
    pub fn cancel(&mut self) {
        self.task = None;
        self.busy = false;
        self.pending_pages = None;
        self.pending_thumbs = None;
    }
    pub fn page(&self, page: usize) -> Option<Arc<Preview>> {
        self.pages.get(page)
    }
    pub fn insert(&mut self, page: usize, preview: Arc<Preview>) -> bool {
        self.pages.insert(
            page,
            preview.clone(),
            preview.bytes,
            self.wanted_pages.clone(),
        )
    }
}
impl Preview {
    pub(super) fn from_page(page: breeze_pdf::Page) -> Result<Arc<Self>> {
        let bytes =
            page.pixels.bgra.len() + page.glyphs.len() * std::mem::size_of::<breeze_pdf::Glyph>();
        Ok(Arc::new(Self {
            page: page.page,
            geometry: page.geometry,
            glyphs: page.glyphs,
            text_warning: page.text_warning,
            image: texture(page.pixels)?,
            points: page.points,
            outline: page.outline,
            elapsed_ms: page.elapsed_ms,
            bytes,
        }))
    }
}
fn page_scale(points: (f32, f32), width: f32, height: f32, zoom: Zoom) -> f32 {
    match zoom {
        Zoom::FitPage => (width / points.0).min(height / points.1),
        Zoom::FitWidth => width / points.0,
        Zoom::Percent(n) => n as f32 / 100. * 96. / 72.,
    }
    .max(0.01)
}
impl Breeze {
    pub(super) fn defer_visible(
        &mut self,
        range: Range<usize>,
        thumbnail: bool,
        cx: &mut Context<Self>,
    ) {
        let stable = if thumbnail {
            self.reading.pending_thumbs.is_none() && self.reading.wanted_thumbs == range
        } else {
            self.reading.pending_pages.is_none() && self.reading.wanted_pages == range
        };
        if stable {
            self.pump_visible(cx);
            return;
        }
        let pending = if thumbnail {
            &mut self.reading.pending_thumbs
        } else {
            &mut self.reading.pending_pages
        };
        let queued = pending.is_some();
        *pending = Some((self.generation, range));
        if queued {
            return;
        }
        let view = cx.entity();
        // Commit only the last range observed in a frame. List measurement never
        // runs the prepaint callbacks that feed this scheduler.
        cx.defer(move |cx| {
            view.update(cx, |this, cx| {
                let pending = if thumbnail {
                    this.reading.pending_thumbs.take()
                } else {
                    this.reading.pending_pages.take()
                };
                if let Some((generation, range)) = pending
                    && this.screen == Screen::Reader
                    && this.generation == generation
                {
                    this.schedule_visible(range, thumbnail, cx);
                }
            })
        });
    }
    pub(super) fn schedule_visible(
        &mut self,
        range: Range<usize>,
        thumbnail: bool,
        cx: &mut Context<Self>,
    ) {
        let total = self.books[self.active].state.navigation.total();
        let range = range.start.min(total)..range.end.min(total);
        let old_page = self.books[self.active].state.navigation.page();
        if thumbnail {
            if self.reading.wanted_thumbs != range {
                self.reading.errors.retain(|(thumb, _), _| !*thumb);
            }
            self.reading.wanted_thumbs = range;
        } else {
            if self.reading.wanted_pages != range {
                self.reading.errors.retain(|(thumb, _), _| *thumb);
            }
            if self.books[self.active].state.layout == ReadingLayout::Continuous
                && !range.is_empty()
                && self.gesture.is_empty()
                && !self.loading
            {
                self.books[self.active].state.navigation.go_to(range.start);
                if let Some(preview) = self.reading.page(range.start) {
                    self.preview = Some(preview);
                }
            }
            self.reading.wanted_pages = range;
        }
        self.reading.errors.retain(|(thumb, page), _| {
            if *thumb {
                self.reading.wanted_thumbs.contains(page)
            } else {
                self.reading.wanted_pages.contains(page)
            }
        });
        if old_page != self.books[self.active].state.navigation.page() {
            cx.notify();
        }
        self.pump_visible(cx);
    }
    pub(super) fn pump_visible(&mut self, cx: &mut Context<Self>) {
        if self.loading
            || self.reading.busy
            || self.screen != Screen::Reader
            || self.closing
            || self.reading.sizes.is_empty()
        {
            return;
        }
        let next_page = self
            .reading
            .wanted_pages
            .clone()
            .take(8)
            .find(|p| {
                self.reading.page(*p).is_none() && !self.reading.errors.contains_key(&(false, *p))
            })
            .map(|p| (false, p));
        let next = next_page.or_else(|| {
            if !self.sidebar || !self.thumbs || self.search_open {
                return None;
            }
            self.reading
                .wanted_thumbs
                .clone()
                .take(32)
                .find(|p| {
                    self.reading.thumbnails.get(*p).is_none()
                        && !self.reading.errors.contains_key(&(true, *p))
                })
                .map(|p| (true, p))
        });
        let Some((thumbnail, page_index)) = next else {
            return;
        };
        let source = self.books[self.active].source.clone();
        let generation = self.generation;
        let request = if thumbnail {
            self.worker.request_thumbnail(source, page_index)
        } else {
            self.worker.request(source, page_index)
        };
        let receiver = match request {
            Ok(r) => r,
            Err(e) => {
                self.reading
                    .errors
                    .insert((thumbnail, page_index), format!("{e:#}"));
                cx.notify();
                return;
            }
        };
        self.reading.busy = true;
        self.reading.task = Some(cx.spawn(async move |this, cx| {
            let result = receiver.await;
            let _ = this.update(cx, |this, cx| {
                if generation != this.generation {
                    return;
                }
                this.reading.busy = false;
                let result = (|| -> Result<()> {
                    let page = result.map_err(|e| anyhow::anyhow!("页面请求取消：{e}"))??;
                    anyhow::ensure!(
                        page.fingerprint == this.reading.fingerprint,
                        "文档在外部发生变化，请关闭后重新打开；未修改原文件"
                    );
                    if !(if thumbnail {
                        &this.reading.wanted_thumbs
                    } else {
                        &this.reading.wanted_pages
                    })
                    .contains(&page_index)
                    {
                        return Ok(());
                    }
                    if thumbnail {
                        let bytes = page.pixels.bgra.len();
                        anyhow::ensure!(
                            this.reading.thumbnails.insert(
                                page_index,
                                texture(page.pixels)?,
                                bytes,
                                this.reading.wanted_thumbs.clone()
                            ),
                            "可见缩略图超出内存预算"
                        );
                    } else {
                        let preview = Preview::from_page(page)?;
                        anyhow::ensure!(
                            this.reading.insert(page_index, preview.clone()),
                            "可见页面超出 64 MiB 内存预算，请提高缩放或切换单页"
                        );
                        if page_index == this.books[this.active].state.navigation.page()
                            && this.gesture.is_empty()
                        {
                            this.preview = Some(preview);
                        }
                    }
                    Ok(())
                })();
                if let Err(e) = result {
                    this.reading
                        .errors
                        .insert((thumbnail, page_index), format!("{e:#}"));
                }
                this.pump_visible(cx);
                cx.notify();
            });
        }));
    }
    pub(super) fn set_layout(&mut self, layout: ReadingLayout, cx: &mut Context<Self>) {
        self.books[self.active].state.layout = layout;
        self.load(cx);
    }
    pub(super) fn layout_button(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let view = cx.entity();
        let selected = self.books[self.active].state.layout;
        Button::new("layout")
            .ghost()
            .small()
            .icon(IconName::PanelsTopLeft)
            .label(self.books[self.active].state.layout.label())
            .tooltip_with_action(
                "切换阅读布局：单页 / 连续 / 双页",
                &CycleLayout,
                Some("Breeze"),
            )
            .dropdown_menu(move |mut menu, window, _| {
                for layout in ReadingLayout::ALL {
                    menu = menu.item(
                        PopupMenuItem::new(layout.label())
                            .checked(selected == layout)
                            .on_click(window.listener_for(&view, move |this, _, _, cx| {
                                this.set_layout(layout, cx)
                            })),
                    );
                }
                menu
            })
    }
    pub(super) fn outline_view(
        &self,
        outline: Arc<Vec<breeze_pdf::Outline>>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if outline.is_empty() {
            return div()
                .p_2()
                .text_color(self.p().muted)
                .child("此文档没有内嵌目录。可切换缩略图导航。")
                .into_any_element();
        }
        uniform_list(
            "outline-list",
            outline.len(),
            cx.processor(move |this, range: Range<usize>, _, cx| {
                range
                    .map(|index| {
                        let item = &outline[index];
                        let target = item.page;
                        div()
                            .h(px(28.))
                            .px_2()
                            .pl(px(8. + item.depth.min(12) as f32 * 10.))
                            .child(
                                categories::left_button(
                                    ("outline", index),
                                    IconName::ChevronRight,
                                    item.title.clone(),
                                )
                                .selected(
                                    target == Some(this.books[this.active].state.navigation.page()),
                                )
                                .disabled(target.is_none())
                                .tooltip(if target.is_some() {
                                    item.title.clone()
                                } else {
                                    "命名目的地尚未解析".into()
                                })
                                .on_click(cx.listener(
                                    move |this, _, _, cx| {
                                        if let Some(page) = target {
                                            this.books[this.active].state.navigation.go_to(page);
                                            this.load(cx);
                                        }
                                    },
                                )),
                            )
                    })
                    .collect()
            }),
        )
        .size_full()
        .into_any_element()
    }
    pub(super) fn thumbnails_view(&self, cx: &mut Context<Self>) -> AnyElement {
        let total = self.books[self.active].state.navigation.total();
        let width = (self.navigation_width - 32.).min(176.);
        uniform_list(
            "thumbnail-list",
            total,
            cx.processor(move |this, range: Range<usize>, _, cx| {
                let wanted = range.clone();
                let generation = this.generation;
                let p = this.p();
                let current = this.books[this.active].state.navigation.page();
                range
                    .map(|page| {
                        Button::new(("thumbnail", page))
                            .ghost()
                            .small()
                            .w_full()
                            .accessibility_label(format!("第 {} 页缩略图", page + 1))
                            .h(px(width * 1.42 + 40.))
                            .px_2()
                            .py_1()
                            .items_center()
                            .child(
                                column()
                                    .when(page == wanted.start, |d| {
                                        let view = cx.entity();
                                        let wanted = wanted.clone();
                                        d.on_prepaint(move |_, _, cx| {
                                            view.update(cx, |this, cx| {
                                                if generation == this.generation {
                                                    this.defer_visible(wanted, true, cx);
                                                }
                                            })
                                        })
                                    })
                                    .w_full()
                                    .h_full()
                                    .items_center()
                                    .gap_1()
                                    .p_2()
                                    .rounded_md()
                                    .border_1()
                                    .border_color(if page == current { p.accent } else { p.line })
                                    .cursor_pointer()
                                    .child(
                                        div()
                                            .flex_1()
                                            .min_h_0()
                                            .w(px(width))
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .map(|d| {
                                                if let Some(image) =
                                                    this.reading.thumbnails.get(page)
                                                {
                                                    d.child(
                                                        img(image)
                                                            .size_full()
                                                            .object_fit(ObjectFit::Contain),
                                                    )
                                                } else {
                                                    d.child(
                                                        div()
                                                            .text_size(px(12.))
                                                            .text_color(p.muted)
                                                            .child(
                                                                this.reading
                                                                    .errors
                                                                    .get(&(true, page))
                                                                    .cloned()
                                                                    .unwrap_or_else(|| {
                                                                        "正在准备…".into()
                                                                    }),
                                                            ),
                                                    )
                                                }
                                            }),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(12.))
                                            .text_color(p.muted)
                                            .child(format!("{}", page + 1)),
                                    ),
                            )
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.books[this.active].state.navigation.go_to(page);
                                this.load(cx);
                            }))
                    })
                    .collect()
            }),
        )
        .track_scroll(&self.reading.thumbs_scroll)
        .size_full()
        .into_any_element()
    }
    fn page_item(
        &self,
        page: usize,
        width: f32,
        height: f32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if let Some(preview) = self
            .reading
            .page(page)
            .or_else(|| self.preview.clone().filter(|preview| preview.page == page))
        {
            div()
                .id(("page-frame", page))
                .flex_shrink_0()
                .child(self.page_surface(page, &preview, width, height, cx))
                .into_any_element()
        } else {
            let p = self.p();
            div()
                .w(px(width))
                .h(px(height))
                .flex_shrink_0()
                .bg(rgb(0xffffff))
                .flex()
                .items_center()
                .justify_center()
                .child(
                    div().text_color(p.muted).child(
                        self.reading
                            .errors
                            .get(&(false, page))
                            .cloned()
                            .unwrap_or_else(|| {
                                if self.reading.wanted_pages.contains(&page)
                                    && page >= self.reading.wanted_pages.start.saturating_add(8)
                                {
                                    "可见页面超过 8 页预算，请提高缩放或切换单页".into()
                                } else {
                                    format!("第 {} 页 · 正在准备…", page + 1)
                                }
                            }),
                    ),
                )
                .into_any_element()
        }
    }
    pub(super) fn pages_view(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let state = &self.books[self.active].state;
        let page = state.navigation.page();
        let layout = state.layout;
        let zoom = state.zoom;
        let width = (f32::from(window.viewport_size().width)
            - if self.sidebar {
                self.navigation_width
            } else {
                0.
            }
            - if state.mode == Mode::Read { 0. } else { 232. })
        .max(100.);
        let height = (f32::from(window.viewport_size().height)
            - 34.
            - 38.
            - 32.
            - if self.search_open { 34. } else { 0. })
        .max(100.);
        let p = self.p();
        let viewport = div()
            .id("document-viewport")
            .test_support()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .bg(p.canvas);
        if self.preview.is_none() && self.reading.sizes.is_empty() {
            return viewport
                .flex()
                .items_center()
                .justify_center()
                .child(div().text_color(p.muted).child(if self.loading {
                    "正在准备页面…".into()
                } else {
                    self.message.clone()
                }))
                .into_any_element();
        }
        let sizes = if self.reading.sizes.is_empty() {
            Arc::new(vec![
                self.preview
                    .as_ref()
                    .map(|p| p.points)
                    .unwrap_or((612., 792.));
                state.navigation.total()
            ])
        } else {
            self.reading.sizes.clone()
        };
        if layout == ReadingLayout::Continuous {
            let metrics = &mut self.reading.metrics;
            if metrics
                .as_ref()
                .is_none_or(|m| m.width != width || m.height != height || m.zoom != zoom)
            {
                let items: Vec<_> = sizes
                    .iter()
                    .map(|points| {
                        let scale = page_scale(*points, width - 24., height - 24., zoom);
                        size(
                            px((points.0 * scale + 24.).max(width)),
                            px(points.1 * scale + 24.),
                        )
                    })
                    .collect();
                let widest = items
                    .iter()
                    .enumerate()
                    .max_by(|(_, a), (_, b)| a.width.cmp(&b.width))
                    .map(|(i, _)| i)
                    .unwrap_or(0);
                *metrics = Some(PageMetrics {
                    width,
                    height,
                    zoom,
                    items: Rc::new(items),
                    widest,
                });
                self.reading
                    .pages_scroll
                    .scroll_to_item(page, ScrollStrategy::Top);
            }
            let items = metrics
                .as_ref()
                .map(|m| m.items.clone())
                .unwrap_or_default();
            let widest = metrics.as_ref().map(|m| m.widest).unwrap_or(0);
            let item_sizes = items.clone();
            return viewport
                .child(
                    v_virtual_list(
                        cx.entity(),
                        "continuous-pages",
                        items,
                        move |this, range, _, cx| {
                            let wanted = range.clone();
                            let generation = this.generation;
                            range
                                .map(|index| {
                                    let (pw, ph) = sizes[index];
                                    let scale =
                                        page_scale((pw, ph), width - 24., height - 24., zoom);
                                    row()
                                        .id(("continuous-page", index))
                                        .when(index == wanted.start, |d| {
                                            let view = cx.entity();
                                            let wanted = wanted.clone();
                                            d.on_prepaint(move |_, _, cx| {
                                                view.update(cx, |this, cx| {
                                                    if generation == this.generation {
                                                        this.defer_visible(wanted, false, cx);
                                                    }
                                                })
                                            })
                                        })
                                        .w(item_sizes[widest].width)
                                        .h(item_sizes[index].height)
                                        .justify_center()
                                        .items_start()
                                        .py_3()
                                        .child(this.page_item(index, pw * scale, ph * scale, cx))
                                })
                                .collect()
                        },
                    )
                    .with_item_to_measure_index(widest)
                    .track_scroll(&self.reading.pages_scroll)
                    .size_full(),
                )
                .into_any_element();
        }
        let range = layout.pages(page, sizes.len());
        let wanted = range.clone();
        self.defer_visible(wanted, false, cx);
        let spread_width = range.clone().map(|i| sizes[i].0).sum::<f32>();
        let spread_height = range.clone().map(|i| sizes[i].1).fold(0., f32::max);
        let gap = if range.len() == 2 { 12. } else { 0. };
        let scale = page_scale(
            (spread_width, spread_height),
            width - 24. - gap,
            height - 24.,
            zoom,
        );
        viewport
            .overflow_scroll()
            .p_3()
            .child(
                row()
                    .min_w_full()
                    .w(px(spread_width * scale + gap))
                    .gap_3()
                    .items_start()
                    .justify_center()
                    .children(
                        range
                            .map(|i| self.page_item(i, sizes[i].0 * scale, sizes[i].1 * scale, cx)),
                    ),
            )
            .into_any_element()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn caches_bound_bytes_and_count() {
        let mut cache = Cache::new(10, 2);
        cache.insert(0, "a", 4, 0..0);
        cache.insert(1, "b", 4, 0..0);
        cache.insert(2, "c", 4, 0..0);
        assert_eq!(cache.get(0), None);
        assert_eq!(cache.get(2), Some("c"));
        cache.insert(3, "oversize", 11, 0..0);
        assert_eq!(cache.get(3), None);
        cache.insert(4, "large", 9, 0..0);
        assert_eq!(cache.entries.len(), 1);
        assert!(!cache.insert(5, "protected overflow", 2, 4..5));
        assert_eq!(cache.get(4), Some("large"));
        assert_eq!(cache.entries.len(), 1);
    }
}
