//! Visible-range scheduling. At most one UI request is awaiting the bounded PDF worker.
use crate::*;
use gpui_kit::component::ElementExt;
use gpui_kit::component::{VirtualListScrollHandle, v_virtual_list};
use std::{
    collections::{HashMap, VecDeque},
    ops::Range,
    rc::Rc,
};
const PAGE_BUDGET: usize = 8 * 1024 * 1024;
const THUMB_BUDGET: usize = 8 * 1024 * 1024;

#[derive(Clone, Copy, PartialEq)]
struct ThumbnailGrid {
    columns: usize,
    card_width: f32,
}
impl ThumbnailGrid {
    fn new(panel_width: f32) -> Self {
        // Reserve the panel border, 8 px side insets and 8 px between cards.
        let width = (panel_width - 17.).max(1.);
        let columns = ((width + 8.) / 112.).floor().max(1.) as usize;
        let card_width = ((width - (columns - 1) as f32 * 8.) / columns as f32)
            .floor()
            .min(144.);
        Self {
            columns,
            card_width,
        }
    }

    fn card_height(self) -> f32 {
        // Integral row heights keep the scroll anchor stable after layout rounding.
        (((self.card_width - 18.) * 1.42 + 38.) / 4.).ceil() * 4.
    }

    fn row_height(self) -> f32 {
        self.card_height() + 8.
    }

    fn pages(self, rows: Range<usize>, total: usize) -> Range<usize> {
        (rows.start * self.columns).min(total)..(rows.end * self.columns).min(total)
    }
}

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
        self.insert_with_eviction(page, value, bytes, protected, |_| {})
    }
    fn insert_with_eviction(
        &mut self,
        page: usize,
        value: T,
        bytes: usize,
        protected: Range<usize>,
        mut evict: impl FnMut(T),
    ) -> bool {
        self.entries.retain(|e| {
            if e.page == page {
                evict(e.value.clone());
                false
            } else {
                true
            }
        });
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
                if let Some(entry) = self.entries.remove(index) {
                    evict(entry.value);
                }
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
    layout: ReadingLayout,
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
    pub zoom_remainder: f32,
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
            zoom_remainder: 0.,
        }
    }
}
impl ReadingView {
    pub fn release_images(&mut self, cx: &mut App) {
        for entry in self.thumbnails.entries.drain(..) {
            cx.defer(move |cx| cx.drop_image(entry.value, None));
        }
    }
    pub fn cancel(&mut self) {
        self.task = None;
        self.busy = false;
        self.pending_pages = None;
        self.pending_thumbs = None;
    }
    pub fn page(&self, page: usize) -> Option<Arc<Preview>> {
        self.pages.get(page)
    }
    pub fn outline(&self) -> Option<Arc<Vec<breeze_pdf::Outline>>> {
        // Cached pages share document metadata even while the current page is loading.
        self.pages
            .entries
            .back()
            .map(|entry| entry.value.outline.clone())
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
        let bytes = page.glyphs.len() * std::mem::size_of::<breeze_pdf::Glyph>();
        Ok(Arc::new(Self {
            page: page.page,
            geometry: page.geometry,
            glyphs: page.glyphs,
            text_warning: page.text_warning,
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
        Zoom::ActualSize => 96. / 72.,
        Zoom::Percent(n) => n as f32 / 100. * 96. / 72.,
    }
    .max(0.01)
}
fn row_geometry(
    sizes: &[(f32, f32)],
    pages: Range<usize>,
    width: f32,
    height: f32,
    zoom: Zoom,
) -> (f32, f32, f32) {
    let row_width = pages.clone().map(|i| sizes[i].0).sum::<f32>();
    let row_height = pages.clone().map(|i| sizes[i].1).fold(0., f32::max);
    let gap = if pages.len() == 2 { 12. } else { 0. };
    let scale = page_scale(
        (row_width, row_height),
        width - 24. - gap,
        height - 24.,
        zoom,
    );
    (row_width * scale + gap, row_height * scale, scale)
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
            if self.books[self.active].state.layout.is_continuous()
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
            || self.raster.busy()
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
        if next_page.is_none() && self.pump_raster(cx) {
            return;
        }
        let next = next_page.or_else(|| {
            if !self.sidebar || !self.show_thumbnails() || self.search_open {
                return None;
            }
            self.reading
                .wanted_thumbs
                .clone()
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
            self.worker.request_metadata(source, page_index)
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
                        let pixels = page
                            .pixels
                            .ok_or_else(|| anyhow::anyhow!("缺少缩略图像素"))?;
                        let bytes = pixels.bgra.len();
                        anyhow::ensure!(
                            this.reading.thumbnails.insert_with_eviction(
                                page_index,
                                texture(pixels)?,
                                bytes,
                                this.reading.wanted_thumbs.clone(),
                                |image| cx.defer(move |cx| cx.drop_image(image, None))
                            ),
                            "可见缩略图超出内存预算"
                        );
                    } else {
                        let preview = Preview::from_page(page)?;
                        anyhow::ensure!(
                            this.reading.insert(page_index, preview.clone()),
                            "可见页面文字数据超出 8 MiB 内存预算，请切换单页"
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
        self.reading_preferences.layout = layout;
        self.reading.metrics = None;
        self.load(cx);
    }
    pub(super) fn outline_view(
        &self,
        outline: Arc<Vec<breeze_pdf::Outline>>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
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
    pub(super) fn resize_navigation(&mut self, width: f32) {
        let old = ThumbnailGrid::new(self.navigation_width);
        let new = ThumbnailGrid::new(width);
        self.navigation_width = width;
        if old == new {
            return;
        }
        let mut scroll = self.reading.thumbs_scroll.0.borrow_mut();
        let offset = scroll.base_handle.offset();
        let row = (-f32::from(offset.y) / old.row_height()).max(0.);
        let page = row.floor() as usize * old.columns;
        // Retain the top visible page and its fractional row offset on reflow.
        scroll.base_handle.set_offset(point(
            offset.x,
            px(-((page / new.columns) as f32 + row.fract()) * new.row_height()),
        ));
        if let Some(pending) = scroll.deferred_scroll_to_item.as_mut() {
            pending.item_index = pending.item_index.saturating_mul(old.columns) / new.columns;
        }
    }

    pub(super) fn scroll_thumbnail_to_page(&self, page: usize) {
        let grid = ThumbnailGrid::new(self.navigation_width);
        self.reading
            .thumbs_scroll
            .scroll_to_item(page / grid.columns, ScrollStrategy::Center);
    }

    fn thumbnail_card(&self, page: usize, cx: &mut Context<Self>) -> Button {
        let p = self.p();
        let current = self.books[self.active].state.navigation.page();
        Button::new(("thumbnail", page))
            .ghost()
            .small()
            .size_full()
            .p_0()
            .border_0()
            .accessibility_label(format!("第 {} 页缩略图", page + 1))
            .child(
                column()
                    .size_full()
                    .items_center()
                    .gap(px(4.))
                    .p(px(8.))
                    .rounded_md()
                    .border_1()
                    .border_color(if page == current { p.accent } else { p.line })
                    .child(
                        div()
                            .id(("thumbnail-image", page))
                            .test_support()
                            .flex_1()
                            .min_h_0()
                            .w_full()
                            .flex()
                            .items_center()
                            .justify_center()
                            .overflow_hidden()
                            .map(|d| {
                                if let Some(image) = self.reading.thumbnails.get(page) {
                                    d.child(img(image).size_full().object_fit(ObjectFit::Contain))
                                } else {
                                    d.child(
                                        div()
                                            .w_full()
                                            .text_center()
                                            .text_size(px(12.))
                                            .text_color(p.muted)
                                            .child(
                                                self.reading
                                                    .errors
                                                    .get(&(true, page))
                                                    .cloned()
                                                    .unwrap_or_else(|| "正在准备…".into()),
                                            ),
                                    )
                                }
                            }),
                    )
                    .child(
                        div()
                            .h(px(16.))
                            .flex_shrink_0()
                            .line_height(px(16.))
                            .text_size(px(12.))
                            .text_color(p.muted)
                            .child(format!("{}", page + 1)),
                    ),
            )
            .on_click(cx.listener(move |this, _, _, cx| {
                this.books[this.active].state.navigation.go_to(page);
                this.load(cx);
            }))
    }

    pub(super) fn thumbnails_view(&self, cx: &mut Context<Self>) -> AnyElement {
        let total = self.books[self.active].state.navigation.total();
        let grid = ThumbnailGrid::new(self.navigation_width);
        uniform_list(
            "thumbnail-list",
            total.div_ceil(grid.columns),
            cx.processor(move |this, range: Range<usize>, _, cx| {
                let first = range.start;
                let wanted = grid.pages(range.clone(), total);
                let generation = this.generation;
                range
                    .map(|index| {
                        row()
                            .id(("thumbnail-row", index))
                            .test_support()
                            .w_full()
                            .h(px(grid.row_height()))
                            .px(px(8.))
                            .py(px(4.))
                            .gap(px(8.))
                            .justify_center()
                            .when(index == first, |d| {
                                let view = cx.entity();
                                let wanted = wanted.clone();
                                d.on_prepaint(move |_, _, cx| {
                                    view.update(cx, |this, cx| {
                                        if generation == this.generation
                                            && grid == ThumbnailGrid::new(this.navigation_width)
                                        {
                                            this.defer_visible(wanted, true, cx);
                                        }
                                    })
                                })
                            })
                            .children((0..grid.columns).map(|column| {
                                let page = index * grid.columns + column;
                                div()
                                    .w(px(grid.card_width))
                                    .h(px(grid.card_height()))
                                    .flex_shrink_0()
                                    .when(page < total, |d| d.child(this.thumbnail_card(page, cx)))
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
                .test_support()
                .flex_shrink_0()
                .child(self.page_surface(page, &preview, width, height, cx))
                .into_any_element()
        } else {
            let p = self.p();
            div()
                .id(("page-frame", page))
                .test_support()
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
    fn reading_viewport_size(&self, window: &Window) -> (f32, f32) {
        let state = &self.books[self.active].state;
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
        (width, height)
    }
    pub(super) fn display_zoom(&self, window: &Window) -> f32 {
        let state = &self.books[self.active].state;
        match state.zoom {
            Zoom::Percent(n) => return n as f32,
            Zoom::ActualSize => return 100.,
            _ => {}
        }
        let (width, height) = self.reading_viewport_size(window);
        let sizes = &self.reading.sizes;
        if sizes.is_empty() {
            return 100.;
        }
        let range = state.layout.pages(state.navigation.page(), sizes.len());
        row_geometry(sizes, range, width, height, state.zoom).2 * 75.
    }
    pub(super) fn pages_view(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let state = &self.books[self.active].state;
        let page = state.navigation.page();
        let layout = state.layout;
        let zoom = state.zoom;
        let (width, height) = self.reading_viewport_size(window);
        let p = self.p();
        let view = cx.entity();
        let viewport = div()
            .id("document-viewport")
            .test_support()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .bg(p.canvas)
            .child(
                canvas(
                    |bounds, window, _| window.insert_hitbox(bounds, HitboxBehavior::Normal),
                    move |_, hitbox, window, _| {
                        window.on_mouse_event(
                            move |event: &ScrollWheelEvent, phase, window, cx| {
                                // Capture before either the virtual list or the single-page scroller.
                                if phase == DispatchPhase::Capture
                                    && hitbox.should_handle_scroll(window)
                                    && event.modifiers.secondary()
                                    && !event.modifiers.alt
                                    && !event.modifiers.shift
                                {
                                    cx.stop_propagation();
                                    view.update(cx, |this, cx| this.scroll_zoom(event, window, cx));
                                }
                            },
                        );
                    },
                )
                .absolute()
                .size_full(),
            );
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
        if layout.is_continuous() {
            let metrics = &mut self.reading.metrics;
            if metrics.as_ref().is_none_or(|m| {
                m.width != width || m.height != height || m.zoom != zoom || m.layout != layout
            }) {
                let items: Vec<_> = (0..sizes.len().div_ceil(layout.columns()))
                    .map(|row| {
                        let range = layout.pages(row * layout.columns(), sizes.len());
                        let (w, h, _) = row_geometry(&sizes, range, width, height, zoom);
                        size(px((w + 24.).max(width)), px(h + 24.))
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
                    layout,
                    items: Rc::new(items),
                    widest,
                });
                self.reading
                    .pages_scroll
                    .scroll_to_item(layout.row(page), ScrollStrategy::Top);
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
                            let wanted = range.start * layout.columns()
                                ..(range.end * layout.columns()).min(sizes.len());
                            let first_row = range.start;
                            let generation = this.generation;
                            range
                                .map(|index| {
                                    let pages = layout.pages(index * layout.columns(), sizes.len());
                                    let (_, _, scale) =
                                        row_geometry(&sizes, pages.clone(), width, height, zoom);
                                    row()
                                        .id(("continuous-page", index))
                                        .when(index == first_row, |d| {
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
                                        .gap_3()
                                        .children(pages.map(|page| {
                                            this.page_item(
                                                page,
                                                sizes[page].0 * scale,
                                                sizes[page].1 * scale,
                                                cx,
                                            )
                                        }))
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
        let (row_width, _, scale) = row_geometry(&sizes, range.clone(), width, height, zoom);
        viewport
            .overflow_scroll()
            .p_3()
            .child(
                row()
                    .min_w_full()
                    .w(px(row_width))
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

    #[gpui_kit::test]
    async fn thumbnails_without_outline_load_every_visible_grid_column(cx: &mut TestAppContext) {
        use gpui_kit::test::TestWindowExt;
        let worker = PdfWorker::new().unwrap();
        let mut page =
            futures::executor::block_on(worker.request_metadata(Source::Sample, 0).unwrap())
                .unwrap()
                .unwrap();
        page.outline = Arc::new(Vec::new());
        let total = page.total;
        let (handle, view) = crate::ui_tests::app(cx);
        cx.update_window(handle, |_, window, cx| {
            view.update(cx, |view, _| {
                view.reading.source = Some(Source::Sample);
                view.reading.fingerprint = page.fingerprint.clone();
                view.reading.sizes = page.page_sizes.clone();
                let preview = Preview::from_page(page).unwrap();
                view.reading.insert(0, preview.clone());
                view.preview = Some(preview);
                view.navigation_width = 360.;
            });
            window.render_frame(cx);
        })
        .unwrap();
        cx.condition(&view, |view, _| {
            (0..total).all(|page| view.reading.thumbnails.get(page).is_some())
        })
        .await;
        view.update(cx, |view, _| {
            assert!(!view.thumbs);
            assert_eq!(view.reading.wanted_thumbs, 0..total);
            assert_eq!(view.reading.thumbnails.entries.len(), total);
            assert!(
                view.reading
                    .thumbnails
                    .entries
                    .iter()
                    .map(|entry| entry.bytes)
                    .sum::<usize>()
                    <= THUMB_BUDGET
            );
            assert_eq!(view.books[0].state.navigation.page(), 0);
        });
    }

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
