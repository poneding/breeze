//! Visible-range scheduling. At most one UI request is awaiting the bounded PDF worker.
use crate::*;
use breeze_core::Rotation;
use breeze_core::annotations::{PageGeometry, ViewGeometry};
use gpui_kit::component::ElementExt;
use gpui_kit::component::{VirtualListScrollHandle, v_virtual_list};
use std::{
    collections::{HashMap, HashSet, VecDeque},
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
    rotation: Rotation,
    items: Rc<Vec<Size<gpui_kit::Pixels>>>,
    widest: usize,
}
struct OutlineSearch {
    input: Entity<InputState>,
    collapsed: HashSet<usize>,
    saved_offset: Point<gpui_kit::Pixels>,
    _subscription: Subscription,
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
    collapsed_outline: HashSet<usize>,
    outline_search: Option<OutlineSearch>,
    outline_scroll: UniformListScrollHandle,
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
            collapsed_outline: HashSet::new(),
            outline_search: None,
            outline_scroll: UniformListScrollHandle::new(),
        }
    }
}
impl ReadingView {
    fn outline_folds(&self) -> &HashSet<usize> {
        self.outline_search
            .as_ref()
            .map(|search| &search.collapsed)
            .unwrap_or(&self.collapsed_outline)
    }

    fn outline_folds_mut(&mut self) -> &mut HashSet<usize> {
        match &mut self.outline_search {
            Some(search) => &mut search.collapsed,
            None => &mut self.collapsed_outline,
        }
    }

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
    /// Presentation geometry must be recomputed after the window chrome changes.
    pub fn invalidate_metrics(&mut self) {
        self.metrics = None;
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
        let bytes = page.text.bytes();
        Ok(Arc::new(Self {
            page: page.page,
            geometry: page.geometry,
            text: page.text,
            text_warning: page.text_warning,
            points: page.points,
            outline: page.outline,
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
/// Scales one row of pages. `sizes` holds the pages' own sizes; the reader's
/// view rotation is applied here so nothing downstream sees a stale aspect.
fn row_geometry(
    sizes: &[(f32, f32)],
    pages: Range<usize>,
    width: f32,
    height: f32,
    zoom: Zoom,
    rotation: Rotation,
) -> (f32, f32, f32) {
    let view = |index: usize| rotation.apply(sizes[index]);
    let row_width = pages.clone().map(|i| view(i).0).sum::<f32>();
    let row_height = pages.clone().map(|i| view(i).1).fold(0., f32::max);
    let gap = if pages.len() == 2 { 12. } else { 0. };
    let scale = page_scale(
        (row_width, row_height),
        width - 24. - gap,
        height - 24.,
        zoom,
    );
    (row_width * scale + gap, row_height * scale, scale)
}
fn preserve_scroll_anchor(
    old: &[Size<gpui_kit::Pixels>],
    new: &[Size<gpui_kit::Pixels>],
    offset: Point<gpui_kit::Pixels>,
) -> Point<gpui_kit::Pixels> {
    if old.len() == new.len() && old.iter().zip(new).all(|(a, b)| a.height == b.height) {
        return offset;
    }
    let top = -f32::from(offset.y);
    let mut before = 0.;
    let mut row = 0;
    while row + 1 < old.len() && before + f32::from(old[row].height) <= top {
        before += f32::from(old[row].height);
        row += 1;
    }
    let Some((old, new_row)) = old.get(row).zip(new.get(row)) else {
        return offset;
    };
    let fraction = ((top - before) / f32::from(old.height).max(1.)).clamp(0., 1.);
    let new_top = new[..row].iter().map(|s| f32::from(s.height)).sum::<f32>()
        + fraction * f32::from(new_row.height);
    point(offset.x, px(-new_top))
}
pub(super) fn outline_has_children(outline: &[breeze_pdf::Outline], index: usize) -> bool {
    outline
        .get(index)
        .zip(outline.get(index + 1))
        .is_some_and(|(item, next)| next.depth > item.depth)
}

fn filtered_outline(
    outline: &[breeze_pdf::Outline],
    collapsed: &HashSet<usize>,
    query: &str,
) -> Vec<usize> {
    let visible = visible_outline(outline, collapsed);
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
    // Propagate each match to its ancestors without scanning entire subtrees.
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

fn visible_outline(outline: &[breeze_pdf::Outline], collapsed: &HashSet<usize>) -> Vec<usize> {
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

// A hidden active entry belongs to the last visible ancestor preceding it.
fn visible_active_outline(visible: &[usize], active: Option<usize>) -> Option<usize> {
    active.and_then(|active| {
        visible
            .iter()
            .rev()
            .find(|&&index| index <= active)
            .copied()
    })
}

// Destinations currently carry page numbers only. Prefer the closest preceding
// chapter, then the deepest entry, keeping the first peer for same-page ties.
fn active_outline(outline: &[breeze_pdf::Outline], page: usize) -> Option<usize> {
    outline
        .iter()
        .enumerate()
        .filter_map(|(index, item)| {
            item.page
                .filter(|target| *target <= page)
                .map(|target| ((target, item.depth, std::cmp::Reverse(index)), index))
        })
        .max_by_key(|(key, _)| *key)
        .map(|(_, index)| index)
}

impl Breeze {
    pub(super) fn scroll_reading(&mut self, distance: gpui_kit::Pixels, cx: &mut Context<Self>) {
        let book = &self.books[self.active];
        match book.format() {
            Some(DocumentFormat::Markdown) => self.scroll_markdown(distance, cx),
            Some(DocumentFormat::Pdf) if book.state.layout.is_continuous() => {
                let scroll = &self.reading.pages_scroll;
                let mut offset = scroll.offset();
                offset.y = (offset.y - distance).clamp(-scroll.max_offset().y, px(0.));
                scroll.set_offset(offset);
                // The virtual list's normal prepaint updates visible pages and navigation.
                cx.notify();
            }
            _ => {}
        }
    }

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
                && !self.annotation_editing.interacting()
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
        if next_page.is_none() && self.pump_raster(false, cx) {
            return;
        }
        let nearby = if next_page.is_none() && self.raster.visible_ready() {
            self.raster
                .prefetch_pages()
                .into_iter()
                .find(|page| {
                    self.reading.page(*page).is_none()
                        && !self.reading.errors.contains_key(&(false, *page))
                })
                .map(|page| (false, page))
        } else {
            None
        };
        if next_page.is_none() && nearby.is_none() && self.pump_raster(true, cx) {
            return;
        }
        let next = next_page.or(nearby).or_else(|| {
            if !self.sidebar || !self.show_thumbnails() {
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
        let rotation = self.books[self.active].state.rotation;
        let request = if thumbnail {
            self.worker.request_thumbnail(source, page_index, rotation)
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
                    let page =
                        result.map_err(|e| anyhow::anyhow!(tf!("Page request cancelled: {}", e)))??;
                    anyhow::ensure!(
                        page.fingerprint == this.reading.fingerprint,
                        tr!("The document changed externally. Close and reopen it; the source is unchanged.")
                    );
                    if !(if thumbnail {
                        &this.reading.wanted_thumbs
                    } else {
                        &this.reading.wanted_pages
                    })
                    .contains(&page_index)
                        && (thumbnail || !this.raster.prefetch_pages().contains(&page_index))
                    {
                        return Ok(());
                    }
                    if thumbnail {
                        let pixels = page
                            .pixels
                            .ok_or_else(|| anyhow::anyhow!(tr!("Thumbnail pixels are missing")))?;
                        let bytes = pixels.bgra.len();
                        anyhow::ensure!(
                            this.reading.thumbnails.insert_with_eviction(
                                page_index,
                                texture(pixels)?,
                                bytes,
                                this.reading.wanted_thumbs.clone(),
                                |image| cx.defer(move |cx| cx.drop_image(image, None))
                            ),
                            tr!("Visible thumbnails exceed the memory budget")
                        );
                    } else {
                        let preview = Preview::from_page(page)?;
                        anyhow::ensure!(
                            this.reading.insert(page_index, preview.clone()),
                            tr!("Visible page text exceeds the 8 MiB budget. Switch to single page.")
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
        if !self.annotation_navigation_allowed(cx) {
            return;
        }
        self.books[self.active].state.layout = layout;
        self.reading_preferences.layout = layout;
        self.reading.metrics = None;
        self.load(cx);
    }
    /// Turns the current document a quarter turn without touching the source.
    /// Cached rasters and thumbnails belong to the previous orientation, and
    /// page text stays valid because it never depends on the view rotation.
    pub(super) fn rotate(&mut self, clockwise: bool, cx: &mut Context<Self>) {
        if !self.annotation_navigation_allowed(cx) {
            return;
        }
        let rotation = &mut self.books[self.active].state.rotation;
        *rotation = if clockwise {
            rotation.clockwise()
        } else {
            rotation.counterclockwise()
        };
        self.reading.release_images(cx);
        self.raster.clear(cx);
        self.reading.metrics = None;
        // Reloading discards in-flight results for the old orientation.
        self.load(cx);
    }
    pub(super) fn rotation(&self) -> Rotation {
        self.books[self.active].state.rotation
    }
    /// The page's display geometry as the reader currently shows it.
    pub(super) fn view_geometry(&self, page: PageGeometry) -> ViewGeometry {
        page.view(self.rotation())
    }
    fn toggle_outline(&mut self, index: usize, cx: &mut Context<Self>) {
        let collapsed = self.reading.outline_folds_mut();
        if !collapsed.insert(index) {
            collapsed.remove(&index);
        }
        cx.notify();
    }

    pub(super) fn set_outline_expanded(&mut self, expanded: bool, cx: &mut Context<Self>) {
        let Some(outline) = self.navigation_outline() else {
            return;
        };
        let collapsed = self.reading.outline_folds_mut();
        collapsed.clear();
        if !expanded {
            collapsed
                .extend((0..outline.len()).filter(|&index| outline_has_children(&outline, index)));
        }
        self.reading
            .outline_scroll
            .scroll_to_item(0, ScrollStrategy::Top);
        cx.notify();
    }

    pub(super) fn toggle_outline_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(search) = self.reading.outline_search.take() {
            let mut scroll = self.reading.outline_scroll.0.borrow_mut();
            scroll.deferred_scroll_to_item = None;
            scroll.base_handle.set_offset(search.saved_offset);
            window.focus(&self.focus, cx);
        } else {
            let input =
                cx.new(|cx| InputState::new(window, cx).placeholder(tr!("Search outline titles")));
            let subscription = cx.subscribe(&input, |this, input, event, cx| {
                if matches!(event, InputEvent::Change)
                    && let Some(search) = &mut this.reading.outline_search
                    && search.input == input
                {
                    search.collapsed = if input.read(cx).value().trim().is_empty() {
                        this.reading.collapsed_outline.clone()
                    } else {
                        HashSet::new()
                    };
                    this.reading
                        .outline_scroll
                        .scroll_to_item(0, ScrollStrategy::Top);
                    cx.notify();
                }
            });
            input.update(cx, |input, cx| input.focus(window, cx));
            self.reading.outline_search = Some(OutlineSearch {
                input,
                collapsed: self.reading.collapsed_outline.clone(),
                saved_offset: self.reading.outline_scroll.0.borrow().base_handle.offset(),
                _subscription: subscription,
            });
        }
        cx.notify();
    }

    pub(super) fn outline_search_open(&self) -> bool {
        self.reading.outline_search.is_some()
    }

    pub(super) fn outline_search_bar(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let search = self.reading.outline_search.as_ref()?;
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
                            this.toggle_outline_search(window, cx)
                        })),
                )
                .on_action(cx.listener(
                    |this, _: &gpui_kit::component::input::Escape, window, cx| {
                        this.toggle_outline_search(window, cx);
                    },
                ))
                .into_any_element(),
        )
    }

    pub(super) fn outline_view(
        &self,
        outline: Arc<Vec<breeze_pdf::Outline>>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let query = self
            .reading
            .outline_search
            .as_ref()
            .map(|search| search.input.read(cx).value().trim().to_lowercase())
            .unwrap_or_default();
        let visible = filtered_outline(&outline, self.reading.outline_folds(), &query);
        let nested = (0..outline.len()).any(|index| outline_has_children(&outline, index));
        if visible.is_empty() {
            return div()
                .id("outline-no-results")
                .test_support()
                .p_2()
                .text_color(self.p(cx).muted)
                .child(tr!("No matching outline titles"))
                .into_any_element();
        }
        let current = active_outline(&outline, self.books[self.active].state.navigation.page());
        let active = visible_active_outline(&visible, current);
        let active = active.filter(|index| query.is_empty() || current == Some(*index));
        div()
            .size_full()
            .pt_1()
            .child(
                uniform_list(
                    "outline-list",
                    visible.len(),
                    cx.processor(move |this, range: Range<usize>, _, cx| {
                        range
                            .map(|row_index| {
                                let index = visible[row_index];
                                let item = &outline[index];
                                let has_children = outline_has_children(&outline, index);
                                let collapsed = this.reading.outline_folds().contains(&index);
                                let target = item.page;
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
                                                    this.toggle_outline(index, cx)
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
                                            .disabled(target.is_none())
                                            .tooltip(if target.is_some() {
                                                item.title.clone()
                                            } else {
                                                tr!("Named destination is unresolved").into()
                                            })
                                            .on_click(cx.listener(move |this, _, _, cx| {
                                                if let Some(page) = target {
                                                    this.go_to_annotation_page(page, cx);
                                                }
                                            })),
                                    )
                            })
                            .collect()
                    }),
                )
                .track_scroll(&self.reading.outline_scroll)
                .size_full(),
            )
            .into_any_element()
    }
    /// Adopt the width the navigation panel is laid out with.
    ///
    /// Kit's divider redistributes both panels proportionally when the window
    /// changes size, which can report a sidebar wider than the panel is allowed
    /// to show; the thumbnail grid has to follow the panel, so cap the reported
    /// width here and let [`Self::adopt_navigation_width`] correct the rest.
    pub(super) fn resize_navigation(&mut self, width: f32) {
        let width = width.min(reader::NAVIGATION_MAX_WIDTH);
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
        let p = self.p(cx);
        let current = self.books[self.active].state.navigation.page();
        Button::new(("thumbnail", page))
            .ghost()
            .small()
            .size_full()
            .p_0()
            .border_0()
            .accessibility_label(tf!("Page {} thumbnail", page + 1))
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
                                                    .unwrap_or_else(|| tr!("Preparing…").into()),
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
                this.go_to_annotation_page(page, cx);
            }))
    }

    /// Adopt the width the navigation panel was really laid out with.
    ///
    /// Kit's divider widths are preferences: a window that is tight beside the
    /// document sidebar leaves the navigation panel less room than the divider
    /// asked for, and the panel decides. The frame that measures a different
    /// width therefore defers the reflow the measured width needs instead of
    /// rebuilding rows that were already laid out with the older one.
    fn adopt_navigation_width(&mut self, measured: f32, frame_width: f32, cx: &mut Context<Self>) {
        let view = cx.entity();
        cx.defer(move |cx| {
            view.update(cx, |this, cx| {
                // A drag or a window resize that landed after this frame owns
                // the width now; only the width this frame was built with is
                // stale.
                if (this.navigation_width - frame_width).abs() <= 0.5 {
                    this.resize_navigation(measured);
                    cx.notify();
                }
            })
        });
    }

    pub(super) fn thumbnails_view(&self, cx: &mut Context<Self>) -> AnyElement {
        let total = self.books[self.active].state.navigation.total();
        let panel_width = self.navigation_width;
        let grid = ThumbnailGrid::new(panel_width);
        let list = uniform_list(
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
        .size_full();
        div()
            .size_full()
            .on_prepaint({
                let view = cx.entity();
                move |bounds, _, cx| {
                    let measured = f32::from(bounds.size.width);
                    if (measured - panel_width).abs() > 0.5 {
                        view.update(cx, |this, cx| {
                            this.adopt_navigation_width(measured, panel_width, cx)
                        });
                    }
                }
            })
            .child(list)
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
            let p = self.p(cx);
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
                                    tr!("More than 8 pages are visible. Zoom in or switch to single page.").into()
                                } else {
                                    tf!("Page {} · Preparing…", page + 1)
                                }
                            }),
                    ),
                )
                .into_any_element()
        }
    }
    fn reading_viewport_size(&self, window: &Window) -> (f32, f32) {
        let viewport = window.viewport_size();
        // Immersive reading hides every chrome row, so the document owns the window.
        if self.full_screen {
            return (
                f32::from(viewport.width).max(100.),
                f32::from(viewport.height).max(100.),
            );
        }
        let width = (f32::from(viewport.width)
            - if self.sidebar {
                self.navigation_width
            } else {
                0.
            }
            - self.document_panel.width())
        .max(100.);
        let height = (f32::from(viewport.height) - 34. - style::TOOLBAR_HEIGHT - 32.).max(100.);
        (width, height)
    }
    pub(super) fn page_display_scale(&self, page: usize, window: &Window) -> f32 {
        let (width, height) = self.reading_viewport_size(window);
        let sizes = &self.reading.sizes;
        let state = &self.books[self.active].state;
        if sizes.is_empty() {
            return 1.;
        }
        row_geometry(
            sizes,
            state.layout.pages(page, sizes.len()),
            width,
            height,
            state.zoom,
            state.rotation,
        )
        .2
    }
    pub(super) fn page_raster_scale(
        &self,
        page: usize,
        points: (f32, f32),
        window: &Window,
    ) -> f32 {
        let state = &self.books[self.active].state;
        let (mut width, height) = self.reading_viewport_size(window);
        // Reserve the density needed with the navigation panel collapsed. Its
        // resizing changes presentation geometry, not the page's pixel identity.
        if self.sidebar && !self.full_screen {
            width += self.navigation_width;
        }
        let sizes = &self.reading.sizes;
        let scale = if sizes.is_empty() {
            page_scale(points, width - 24., height - 24., state.zoom)
        } else {
            row_geometry(
                sizes,
                state.layout.pages(page, sizes.len()),
                width,
                height,
                state.zoom,
                state.rotation,
            )
            .2
        };
        let (visible_width, visible_height) = self.reading_viewport_size(window);
        let actual = if sizes.is_empty() {
            page_scale(
                points,
                visible_width - 24.,
                visible_height - 24.,
                state.zoom,
            )
        } else {
            self.page_display_scale(page, window)
        };
        let oversampling = scale / actual;
        let estimated_bytes =
            visible_width * visible_height * (window.scale_factor() * oversampling).powi(2) * 4.;
        // Keep room for tile rounding and pending replacement in large windows.
        // If oversampling would exhaust it, retain the previous picture while
        // preparing the exact native density instead of reducing final quality.
        let scale = if scale > actual && estimated_bytes > 44. * 1024. * 1024. {
            actual
        } else {
            scale
        };
        scale * window.scale_factor()
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
        row_geometry(sizes, range, width, height, state.zoom, state.rotation).2 * 75.
    }
    pub(super) fn pages_view(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let state = &self.books[self.active].state;
        let page = state.navigation.page();
        let layout = state.layout;
        let zoom = state.zoom;
        let (width, height) = self.reading_viewport_size(window);
        let p = self.p(cx);
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
                    tr!("Preparing page…").into()
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
        let rotation = state.rotation;
        if layout.is_continuous() {
            let metrics = &mut self.reading.metrics;
            if metrics.as_ref().is_none_or(|m| {
                m.width != width
                    || m.height != height
                    || m.zoom != zoom
                    || m.layout != layout
                    || m.rotation != rotation
            }) {
                let items: Vec<_> = (0..sizes.len().div_ceil(layout.columns()))
                    .map(|row| {
                        let range = layout.pages(row * layout.columns(), sizes.len());
                        let (w, h, _) = row_geometry(&sizes, range, width, height, zoom, rotation);
                        size(px((w + 24.).max(width)), px(h + 24.))
                    })
                    .collect();
                let widest = items
                    .iter()
                    .enumerate()
                    .max_by(|(_, a), (_, b)| a.width.cmp(&b.width))
                    .map(|(i, _)| i)
                    .unwrap_or(0);
                let offset = metrics.as_ref().map(|old| {
                    preserve_scroll_anchor(&old.items, &items, self.reading.pages_scroll.offset())
                });
                *metrics = Some(PageMetrics {
                    width,
                    height,
                    zoom,
                    layout,
                    rotation,
                    items: Rc::new(items),
                    widest,
                });
                if let Some(offset) = offset {
                    self.reading.pages_scroll.base_handle().set_offset(offset);
                } else {
                    self.reading
                        .pages_scroll
                        .scroll_to_item(layout.row(page), ScrollStrategy::Top);
                }
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
                                    let (_, _, scale) = row_geometry(
                                        &sizes,
                                        pages.clone(),
                                        width,
                                        height,
                                        zoom,
                                        rotation,
                                    );
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
                                            let points = rotation.apply(sizes[page]);
                                            this.page_item(
                                                page,
                                                points.0 * scale,
                                                points.1 * scale,
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
        let (row_width, _, scale) =
            row_geometry(&sizes, range.clone(), width, height, zoom, rotation);
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
                    .children(range.map(|i| {
                        let points = rotation.apply(sizes[i]);
                        self.page_item(i, points.0 * scale, points.1 * scale, cx)
                    })),
            )
            .into_any_element()
    }
}

impl Breeze {
    pub(super) fn refresh_outline_language(&self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(state) = &self.reading.outline_search {
            state.input.update(cx, |input, cx| {
                input.set_placeholder(tr!("Search outline titles"), window, cx)
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A reader showing the bundled sample with the thumbnail tab selected.
    /// Geometry is deterministic because the asynchronous loader waits.
    fn thumbnail_layout_fixture(cx: &mut TestAppContext) -> (AnyWindowHandle, Entity<Breeze>) {
        use gpui_kit::test::TestWindowExt;
        let worker = PdfWorker::new().unwrap();
        let mut page =
            futures::executor::block_on(worker.request_metadata(Source::Sample, 0).unwrap())
                .unwrap()
                .unwrap();
        page.outline = Arc::new(Vec::new());
        let (handle, view) = crate::ui_tests::app(cx);
        cx.update_window(handle, |_, window, cx| {
            view.update(cx, |view, _| {
                view.reading.source = Some(Source::Sample);
                view.reading.fingerprint = page.fingerprint.clone();
                view.reading.sizes = page.page_sizes.clone();
                let preview = Preview::from_page(page).unwrap();
                view.reading.insert(0, preview.clone());
                view.preview = Some(preview);
                view.loading = true;
            });
            window.render_frame(cx);
        })
        .unwrap();
        (handle, view)
    }

    /// The divider's number is not the width the panel ends up with: growing
    /// the window redistributes the splitter proportionally, and a window that
    /// is tight beside the document sidebar leaves the panel less room than it
    /// asked for. The grid has to follow the panel either way, or cards are
    /// clipped at both edges of a narrower panel.
    #[gpui_kit::test]
    async fn thumbnail_grid_follows_the_panel_the_reader_lays_out(cx: &mut TestAppContext) {
        use gpui_kit::test::TestWindowExt;
        let (handle, view) = thumbnail_layout_fixture(cx);
        let drag_to_maximum = |cx: &mut TestAppContext| {
            cx.update_window(handle, |_, window, cx| {
                let split = view.read(cx).navigation_split.clone();
                split.update(cx, |state, cx| {
                    state.resize_panel(0, px(reader::NAVIGATION_MAX_WIDTH), window, cx)
                });
            })
            .unwrap();
        };
        let assert_row_fits = |cx: &mut TestAppContext, case: &str| {
            cx.update_window(handle, |_, window, cx| {
                // Widths travel through the splitter's settling frame, so let
                // the reader reach the layout the user would see.
                window.render_frame(cx);
                window.render_frame(cx);
                window.render_frame(cx);
                let width = view.read(cx).navigation_width;
                let grid = ThumbnailGrid::new(width);
                let panel = window.find("navigation-content").bounds();
                assert!(
                    (f32::from(panel.size.width) - width).abs() <= 0.5,
                    "{case}: grid is laid out for {width} px, panel is {} px",
                    f32::from(panel.size.width)
                );
                for column in 0..grid.columns {
                    let card = window.find(("thumbnail", column)).bounds();
                    assert!(
                        card.left() >= panel.left() && card.right() <= panel.right(),
                        "{case}: card {column} {card:?} leaves the panel {panel:?}"
                    );
                }
                assert_eq!(view.read(cx).books[0].state.navigation.page(), 0);
            })
            .unwrap();
        };
        drag_to_maximum(cx);
        cx.simulate_window_resize(handle, size(px(1600.), px(900.)));
        assert_row_fits(cx, "window grown after the drag");
        // The document sidebar takes its 256 px from the same row, so the
        // navigation panel is squeezed below its own minimum preference.
        cx.update_window(handle, |_, window, cx| {
            view.update(cx, |view, _| view.document_panel.open = true);
            window.render_frame(cx);
        })
        .unwrap();
        cx.simulate_window_resize(handle, size(px(900.), px(650.)));
        assert_row_fits(cx, "document sidebar at the minimum window width");
        cx.update_window(handle, |_, window, cx| {
            view.update(cx, |view, _| view.document_panel.open = false);
            window.render_frame(cx);
        })
        .unwrap();
        assert_row_fits(cx, "document sidebar closed again");
    }

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
            });
            // Widen the panel itself: the grid follows the panel the reader
            // lays out, not a width assigned to the view field.
            let split = view.read(cx).navigation_split.clone();
            split.update(cx, |state, cx| {
                state.resize_panel(0, px(reader::NAVIGATION_MAX_WIDTH), window, cx)
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

    fn nested_outline() -> Vec<breeze_pdf::Outline> {
        [
            (None, 0),
            (None, 1),
            (Some(0), 2),
            (Some(2), 1),
            (Some(5), 0),
        ]
        .into_iter()
        .enumerate()
        .map(|(index, (page, depth))| breeze_pdf::Outline {
            title: format!("Section {index}"),
            page,
            depth,
        })
        .collect()
    }

    fn outline_page(outline: Vec<breeze_pdf::Outline>) -> breeze_pdf::Page {
        let worker = PdfWorker::new().unwrap();
        let mut page =
            futures::executor::block_on(worker.request_metadata(Source::Sample, 0).unwrap())
                .unwrap()
                .unwrap();
        page.outline = Arc::new(outline);
        page
    }

    fn seed_outline(view: &mut Breeze, page: breeze_pdf::Page) {
        view.reading.source = Some(Source::Sample);
        view.reading.sizes = page.page_sizes.clone();
        let preview = Preview::from_page(page).unwrap();
        view.reading.insert(0, preview.clone());
        view.preview = Some(preview);
        view.loading = true;
    }

    #[test]
    fn outline_filter_preserves_matching_ancestors_and_original_ids() {
        let mut outline = nested_outline();
        outline[2].title = "CHILD 指南".into();
        outline[4].title = "child overview".into();
        assert_eq!(
            filtered_outline(&outline, &HashSet::new(), "child"),
            vec![0, 1, 2, 4]
        );
        assert_eq!(
            filtered_outline(&outline, &HashSet::new(), "指南"),
            vec![0, 1, 2]
        );
        assert_eq!(
            filtered_outline(&outline, &HashSet::from([1]), "child"),
            vec![0, 1, 4]
        );
        assert!(filtered_outline(&outline, &HashSet::new(), "missing").is_empty());
        assert_eq!(
            filtered_outline(&outline, &HashSet::from([0]), ""),
            vec![0, 4]
        );
    }

    #[gpui_kit::test]
    fn outline_header_fits_minimum_width_and_flat_titles_have_no_icon_slot(
        cx: &mut TestAppContext,
    ) {
        use gpui_kit::test::TestWindowExt;
        let page = outline_page(nested_outline());
        let (handle, view) = crate::ui_tests::app(cx);
        cx.update_window(handle, |_, window, cx| {
            view.update(cx, |view, cx| {
                seed_outline(view, page);
                let mut saved = view.snapshot();
                saved.width = 168.;
                view.restore(saved, cx);
                assert_eq!(view.navigation_width, reader::NAVIGATION_MIN_WIDTH);
                assert_eq!(view.snapshot().width, reader::NAVIGATION_MIN_WIDTH);
            });
            window.render_frame(cx);
            let split = view.read(cx).navigation_split.clone();
            split.update(cx, |state, cx| state.resize_panel(0, px(1.), window, cx));
        })
        .unwrap();
        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            let header = window.find("navigation-header").bounds();
            let first_tab = window.within("nav-tabs").find(0usize).bounds();
            let last_tab = window.within("nav-tabs").find(1usize).bounds();
            let search = window.find("outline-search").bounds();
            let more = window.find("outline-more").bounds();
            let collapse = window.find("collapse").bounds();
            assert_eq!(header.size.height, px(34.));
            assert!((view.read(cx).navigation_width - reader::NAVIGATION_MIN_WIDTH).abs() < 1.);
            assert!(header.right() <= window.find("navigation-content").bounds().right());
            assert!(collapse.left() - header.left() < px(12.));
            // Two equal flex slots keep the icon tabs centered in the header.
            let tabs_center = (first_tab.left() + last_tab.right()) * 0.5;
            assert!(
                (tabs_center - header.center().x).abs() <= px(1.),
                "tabs {first_tab:?}..{last_tab:?}, header {header:?}"
            );
            assert!(
                last_tab.right() + px(4.) <= search.left(),
                "tabs {last_tab:?}, search {search:?}"
            );
            assert!(search.right() + px(4.) <= more.left());
            assert!(
                more.right() <= header.right() - px(8.),
                "header {header:?}, more {more:?}"
            );
            window.within("nav-tabs").click(1usize, cx);
            assert!(window.try_find("outline-search").is_none());
            assert!(window.try_find("outline-more").is_none());
        })
        .unwrap();
        let mut outline = nested_outline();
        for item in &mut outline {
            item.depth = 0;
        }
        let page = outline_page(outline);
        let empty = Preview::from_page(outline_page(Vec::new())).unwrap();
        cx.update_window(handle, |_, window, cx| {
            view.update(cx, |view, _| {
                seed_outline(view, page);
                view.thumbs = false;
            });
            window.render_frame(cx);
            assert!(window.find("outline-search").visible());
            assert!(window.try_find("outline-more").is_none());
            let panel = window.find("navigation-content").bounds();
            for index in 0..5usize {
                let title = window.find(("outline", index)).bounds();
                assert!(
                    title.left() - panel.left() < px(9.),
                    "flat title {title:?}, panel {panel:?}"
                );
                assert!(window.try_find(("outline-toggle", index)).is_none());
            }
            view.update(cx, |view, _| view.preview = Some(empty));
            window.render_frame(cx);
            assert!(window.try_find("outline-search").is_none());
            assert!(window.try_find("outline-more").is_none());
            let only_tab = window.within("nav-tabs").find(0usize).bounds();
            let header = window.find("navigation-header").bounds();
            assert!(
                (only_tab.center().x - header.center().x).abs() <= px(1.),
                "tab {only_tab:?}, header {header:?}"
            );
        })
        .unwrap();
    }

    #[gpui_kit::test]
    fn outline_menu_expands_and_collapses_all_levels_without_navigation(cx: &mut TestAppContext) {
        use gpui_kit::test::TestWindowExt;
        let page = outline_page(nested_outline());
        let (handle, view) = crate::ui_tests::app(cx);
        cx.update_window(handle, |_, window, cx| {
            view.update(cx, |view, _| seed_outline(view, page));
            window.render_frame(cx);
            window.click("outline-more", cx);
            window.press("down", cx);
            window.press("down", cx);
            window.press("enter", cx);
        })
        .unwrap();
        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            assert_eq!(
                view.read(cx).reading.collapsed_outline,
                HashSet::from([0, 1])
            );
            assert!(window.find(("outline", 0usize)).visible());
            assert!(window.find(("outline", 4usize)).visible());
            assert!(window.try_find(("outline", 1usize)).is_none());
            assert_eq!(view.read(cx).books[0].state.navigation.page(), 0);
            window.click("outline-more", cx);
            window.press("down", cx);
            window.press("enter", cx);
        })
        .unwrap();
        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            assert!(view.read(cx).reading.collapsed_outline.is_empty());
            for index in 0..5usize {
                assert!(window.find(("outline", index)).visible());
            }
            assert_eq!(view.read(cx).books[0].state.navigation.page(), 0);
        })
        .unwrap();
    }

    #[gpui_kit::test]
    fn outline_search_restores_scroll_after_no_matches(cx: &mut TestAppContext) {
        use gpui_kit::test::TestWindowExt;
        let page = outline_page(
            (0..80)
                .map(|index| breeze_pdf::Outline {
                    title: format!("Section {index}"),
                    page: Some(0),
                    depth: 0,
                })
                .collect(),
        );
        let (handle, view) = crate::ui_tests::app(cx);
        let mut saved = point(px(0.), px(0.));
        cx.update_window(handle, |_, window, cx| {
            view.update(cx, |view, _| seed_outline(view, page));
            window.render_frame(cx);
            view.read(cx)
                .reading
                .outline_scroll
                .scroll_to_item_strict(40, ScrollStrategy::Top);
            window.render_frame(cx);
            saved = view
                .read(cx)
                .reading
                .outline_scroll
                .0
                .borrow()
                .base_handle
                .offset();
            assert!(saved.y < -px(500.));
            window.click("outline-search", cx);
            window.input("no match", cx);
        })
        .unwrap();
        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            assert!(window.find("outline-no-results").visible());
            window.click("close-outline-search", cx);
            assert_eq!(
                view.read(cx)
                    .reading
                    .outline_scroll
                    .0
                    .borrow()
                    .base_handle
                    .offset(),
                saved
            );
            assert!(window.find(("outline", 40usize)).visible());
        })
        .unwrap();
    }

    #[gpui_kit::test]
    fn outline_search_reveals_collapsed_titles_and_restores_folds(cx: &mut TestAppContext) {
        use gpui_kit::test::TestWindowExt;
        let mut outline = nested_outline();
        outline[2].title = "CHILD 指南".into();
        outline[2].page = Some(2);
        let page = outline_page(outline);
        let (handle, view) = crate::ui_tests::app(cx);
        cx.update_window(handle, |_, window, cx| {
            view.update(cx, |view, _| {
                seed_outline(view, page);
                view.reading.collapsed_outline.insert(0);
            });
            window.render_frame(cx);
            window.click("outline-search", cx);
            assert!(window.find("outline-search-bar").visible());
            window.input("child", cx);
        })
        .unwrap();
        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            for index in 0..3usize {
                assert!(window.find(("outline", index)).visible());
            }
            assert!(window.try_find(("outline", 3usize)).is_none());
            assert!(window.try_find(("outline", 4usize)).is_none());
            assert_eq!(view.read(cx).reading.collapsed_outline, HashSet::from([0]));
            let zoom = view.read(cx).books[0].state.zoom;
            window.press("secondary-=", cx);
            assert_eq!(view.read(cx).books[0].state.zoom, zoom);
            window.press("secondary-a", cx);
            window.input("not-found", cx);
        })
        .unwrap();
        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            assert!(window.find("outline-no-results").visible());
            assert!(window.try_find(("outline", 0usize)).is_none());
            window.press("escape", cx);
            assert!(window.try_find("outline-search-bar").is_none());
            assert!(!view.read(cx).outline_search_open());
            assert!(window.find(("outline", 0usize)).visible());
            assert!(window.try_find(("outline", 1usize)).is_none());
            assert_eq!(view.read(cx).books[0].state.navigation.page(), 0);
            window.click("outline-search", cx);
            window.input("指南", cx);
        })
        .unwrap();
        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            window.click(("outline", 2usize), cx);
            assert_eq!(view.read(cx).books[0].state.navigation.page(), 2);
            assert_eq!(view.read(cx).reading.collapsed_outline, HashSet::from([0]));
            window.click("close-outline-search", cx);
            assert!(window.try_find("outline-search-bar").is_none());
        })
        .unwrap();
    }

    #[test]
    fn collapsed_outline_hides_descendants_and_preserves_nested_state() {
        let outline = nested_outline();
        let mut collapsed = HashSet::new();
        assert_eq!(visible_outline(&outline, &collapsed), vec![0, 1, 2, 3, 4]);
        collapsed.insert(1);
        assert_eq!(visible_outline(&outline, &collapsed), vec![0, 1, 3, 4]);
        collapsed.insert(0);
        let visible = visible_outline(&outline, &collapsed);
        assert_eq!(visible, vec![0, 4]);
        assert_eq!(visible_active_outline(&visible, Some(2)), Some(0));
        assert_eq!(visible_active_outline(&visible, Some(4)), Some(4));
        assert_eq!(visible_active_outline(&visible, None), None);
        collapsed.remove(&0);
        assert_eq!(visible_outline(&outline, &collapsed), vec![0, 1, 3, 4]);
        collapsed.remove(&1);
        assert_eq!(visible_outline(&outline, &collapsed), vec![0, 1, 2, 3, 4]);
        assert!(outline_has_children(&outline, 0));
        assert!(outline_has_children(&outline, 1));
        assert!(!outline_has_children(&outline, 2));
        assert!(!outline_has_children(&outline, 4));
    }

    #[gpui_kit::test]
    fn outline_arrows_toggle_while_titles_only_navigate(cx: &mut TestAppContext) {
        use gpui_kit::test::TestWindowExt;
        let worker = PdfWorker::new().unwrap();
        let mut page =
            futures::executor::block_on(worker.request_metadata(Source::Sample, 0).unwrap())
                .unwrap()
                .unwrap();
        let mut outline = nested_outline();
        outline[1].page = Some(0);
        page.outline = Arc::new(outline);
        let (handle, view) = crate::ui_tests::app(cx);
        cx.update_window(handle, |_, window, cx| {
            view.update(cx, |view, _| {
                view.reading.source = Some(Source::Sample);
                view.reading.sizes = page.page_sizes.clone();
                let preview = Preview::from_page(page).unwrap();
                view.reading.insert(0, preview.clone());
                view.preview = Some(preview);
                view.loading = true;
            });
            window.render_frame(cx);
            assert!(window.find(("outline", 2usize)).visible());
            assert!(window.find(("outline", 0usize)).bounds().size.width > px(140.));
            assert!(window.try_find(("outline-toggle", 2usize)).is_none());
            window.click(("outline", 1usize), cx);
            assert!(window.find(("outline", 2usize)).visible());
            assert!(view.read(cx).reading.collapsed_outline.is_empty());
            window.click(("outline-toggle", 1usize), cx);
            assert!(window.try_find(("outline", 2usize)).is_none());
            window.click(("outline", 1usize), cx);
            assert!(window.try_find(("outline", 2usize)).is_none());
            assert!(view.read(cx).reading.collapsed_outline.contains(&1));
            assert_eq!(view.read(cx).books[0].state.navigation.page(), 0);
            // An unresolved title is disabled, but its disclosure button remains usable.
            window.click(("outline", 0usize), cx);
            assert!(window.find(("outline", 1usize)).visible());
            window.click(("outline-toggle", 0usize), cx);
            assert!(window.try_find(("outline", 1usize)).is_none());
            assert!(window.try_find(("outline", 3usize)).is_none());
            assert!(window.find(("outline", 4usize)).visible());
            window.click(("outline-toggle", 0usize), cx);
            assert!(window.find(("outline", 1usize)).visible());
            assert!(window.try_find(("outline", 2usize)).is_none());
            window.click(("outline-toggle", 1usize), cx);
            assert!(window.find(("outline", 2usize)).visible());
            window.click(("outline", 3usize), cx);
            assert_eq!(view.read(cx).books[0].state.navigation.page(), 2);
        })
        .unwrap();
    }

    #[test]
    fn outline_selection_is_unique_and_tracks_sections() {
        let outline: Vec<_> = [
            (Some(2), 0),
            (Some(2), 1),
            (Some(2), 1),
            (None, 2),
            (Some(7), 0),
            (Some(5), 0),
        ]
        .into_iter()
        .map(|(page, depth)| breeze_pdf::Outline {
            title: String::new(),
            page,
            depth,
        })
        .collect();
        assert_eq!(active_outline(&outline, 0), None);
        assert_eq!(active_outline(&outline, 2), Some(1));
        assert_eq!(active_outline(&outline, 4), Some(1));
        assert_eq!(active_outline(&outline, 5), Some(5));
        assert_eq!(active_outline(&outline, 8), Some(4));
        assert_eq!(active_outline(&outline, 2), Some(1));
        assert_eq!(active_outline(&[], 2), None);
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
