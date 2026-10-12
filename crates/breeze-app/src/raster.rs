//! Visible-first page scheduling with bounded lookahead and retained presentation.
use crate::*;
use breeze_core::Rotation;
use breeze_core::annotations::ViewGeometry;
use breeze_pdf::DeviceTile;
use std::collections::{HashMap, VecDeque};

// Gutters fit inside a 512px atlas cell, avoiding 1024px allocations per tile.
const EDGE: u32 = 508;
const GUTTER: u32 = 2;
const BUDGET: usize = 56 * 1024 * 1024;
const PREFETCH_BUDGET: usize = 16 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct Key {
    page: usize,
    // Part of the identity: the same tile of a turned page holds other pixels.
    rotation: Rotation,
    tile: DeviceTile,
}
impl Key {
    fn bytes(self) -> usize {
        self.tile.width as usize * self.tile.height as usize * 4
    }
    fn rect(self) -> [f32; 4] {
        let x = self.tile.x.div_ceil(EDGE) * EDGE;
        let y = self.tile.y.div_ceil(EDGE) * EDGE;
        let scale = self.tile.scale();
        [
            x as f32 / scale,
            y as f32 / scale,
            (x + EDGE).min(self.tile.x + self.tile.width) as f32 / scale,
            (y + EDGE).min(self.tile.y + self.tile.height) as f32 / scale,
        ]
    }
}
fn intersection(a: [f32; 4], b: [f32; 4]) -> Option<[f32; 4]> {
    let r = [
        a[0].max(b[0]),
        a[1].max(b[1]),
        a[2].min(b[2]),
        a[3].min(b[3]),
    ];
    (r[2] > r[0] && r[3] > r[1]).then_some(r)
}
#[derive(Clone, Copy)]
struct Viewport {
    page: usize,
    rotation: Rotation,
    points: (f32, f32),
    scale: f32,
    rect: [f32; 4],
}

#[derive(Default)]
struct Demand {
    views: Vec<Viewport>,
    prefetch: Vec<Key>,
}
#[derive(Default)]
pub(super) struct RasterView {
    cache: VecDeque<(Key, Arc<RenderImage>, usize)>,
    wanted: Vec<Key>,
    prefetch: Vec<Key>,
    views: Vec<Viewport>,
    directions: HashMap<usize, f32>,
    pending: Option<Demand>,
    errors: HashMap<Key, String>,
    task: Option<Task<()>>,
    busy: bool,
}
impl RasterView {
    #[cfg(test)]
    pub fn requested_scales(&self) -> Vec<f32> {
        self.wanted.iter().map(|k| k.tile.scale()).collect()
    }
    pub fn cancel(&mut self) {
        self.task = None;
        self.busy = false;
        self.pending = None;
        self.wanted.clear();
        self.prefetch.clear();
        self.views.clear();
        self.directions.clear();
        self.errors.clear();
    }
    pub fn clear(&mut self, cx: &mut App) {
        self.cancel();
        for (_, image, _) in self.cache.drain(..) {
            cx.defer(move |cx| cx.drop_image(image, None));
        }
    }
    pub fn busy(&self) -> bool {
        self.busy
    }
    pub fn prefetch_pages(&self) -> Vec<usize> {
        let mut pages = Vec::new();
        for key in &self.prefetch {
            if !pages.contains(&key.page) {
                pages.push(key.page);
            }
        }
        pages
    }
    fn contains(&self, key: Key) -> bool {
        self.cache.iter().any(|entry| entry.0 == key)
    }
    pub fn visible_ready(&self) -> bool {
        self.wanted.iter().all(|key| self.contains(*key))
    }
    #[cfg(test)]
    fn get(&self, key: Key) -> Option<Arc<RenderImage>> {
        self.cache.iter().find(|e| e.0 == key).map(|e| e.1.clone())
    }
    fn requested(&self, key: Key) -> bool {
        self.wanted.contains(&key) || self.prefetch.contains(&key)
    }
    fn fallback_needed(&self, key: Key, incoming: Option<Key>) -> bool {
        self.views.iter().any(|view| {
            if key.page != view.page
                || key.rotation != view.rotation
                || key.tile.scale() == view.scale
            {
                return false;
            }
            let Some(area) = intersection(key.rect(), view.rect) else {
                return false;
            };
            self.wanted.iter().any(|next| {
                next.page == key.page
                    && intersection(next.rect(), area).is_some()
                    && incoming != Some(*next)
                    && !self.contains(*next)
            })
        })
    }
    fn retire_covered(&mut self, incoming: Option<Key>, cx: &mut App) {
        let retired: Vec<_> = self
            .cache
            .iter()
            .filter_map(|(key, _, _)| {
                let obsolete = self.views.iter().any(|v| {
                    v.page == key.page
                        && (v.rotation != key.rotation || v.scale != key.tile.scale())
                });
                (obsolete && !self.fallback_needed(*key, incoming)).then_some(*key)
            })
            .collect();
        self.cache.retain(|(key, image, _)| {
            if retired.contains(key) {
                let image = image.clone();
                cx.defer(move |cx| cx.drop_image(image, None));
                false
            } else {
                true
            }
        });
    }
    fn commit(&mut self, demand: Demand, cx: &mut App) -> Result<()> {
        let mut wanted = Vec::new();
        for view in &demand.views {
            for key in keys(*view)? {
                if !wanted.contains(&key) {
                    wanted.push(key);
                }
            }
        }
        let allowance = BUDGET
            .saturating_sub(wanted.iter().map(|key| key.bytes()).sum::<usize>())
            .min(PREFETCH_BUDGET);
        let mut used = 0;
        let mut prefetch = Vec::new();
        for key in demand.prefetch {
            if !wanted.contains(&key) && !prefetch.contains(&key) && used + key.bytes() <= allowance
            {
                used += key.bytes();
                prefetch.push(key);
            }
        }
        if self.wanted != wanted || self.prefetch != prefetch {
            self.errors.clear();
        }
        self.wanted = wanted;
        self.prefetch = prefetch;
        for next in &demand.views {
            if let Some(old) = self.views.iter().find(|old| old.page == next.page) {
                let delta = next.rect[1] - old.rect[1];
                if delta.abs() > 0.01 {
                    self.directions.insert(next.page, delta.signum());
                }
            }
        }
        self.views = demand.views;
        self.retire_covered(None, cx);
        Ok(())
    }
    fn insert(&mut self, key: Key, pixels: Pixels, cx: &mut App) -> Result<()> {
        let bytes = pixels.bgra.len();
        anyhow::ensure!(bytes <= BUDGET, tr!("页面分块超过内存预算"));
        // Only retire a previous-scale tile once its visible area is covered.
        // The incoming tile is installed in this same update before the next paint.
        let image = texture(pixels)?;
        self.retire_covered(Some(key), cx);
        while self.cache.iter().map(|e| e.2).sum::<usize>() + bytes > BUDGET {
            let index = self
                .cache
                .iter()
                .position(|e| !self.wanted.contains(&e.0) && !self.fallback_needed(e.0, Some(key)))
                .ok_or_else(|| anyhow::anyhow!(tr!("当前可见区域超过清晰显示内存预算")))?;
            if let Some((_, image, _)) = self.cache.remove(index) {
                cx.defer(move |cx| cx.drop_image(image, None));
            }
        }
        self.cache.push_back((key, image, bytes));
        Ok(())
    }
    fn next_key(&self, prefetch: bool) -> Option<Key> {
        if prefetch && !self.visible_ready() {
            return None;
        }
        let missing = |key: &Key| !self.contains(*key) && !self.errors.contains_key(key);
        if !prefetch {
            let mut completion: Option<Vec<Key>> = None;
            for (old, _, _) in &self.cache {
                if !self.fallback_needed(*old, None) {
                    continue;
                }
                let Some(view) = self.views.iter().find(|view| view.page == old.page) else {
                    continue;
                };
                let Some(area) = intersection(old.rect(), view.rect) else {
                    continue;
                };
                let remaining: Vec<_> = self
                    .wanted
                    .iter()
                    .filter(|key| {
                        key.page == old.page
                            && intersection(key.rect(), area).is_some()
                            && missing(key)
                    })
                    .copied()
                    .collect();
                if !remaining.is_empty()
                    && completion
                        .as_ref()
                        .is_none_or(|best| remaining.len() < best.len())
                {
                    completion = Some(remaining);
                }
            }
            if let Some(keys) = completion {
                return keys.first().copied();
            }
        }
        let candidates = if prefetch {
            &self.prefetch
        } else {
            &self.wanted
        };
        candidates.iter().find(|key| missing(key)).copied()
    }
    fn images(&self, view: Viewport) -> Vec<(Key, Arc<RenderImage>)> {
        let mut images = Vec::new();
        // Previous pixels remain under the new resolution during preparation.
        // This is presentation only; final pixels still match the requested scale.
        for exact in [false, true] {
            images.extend(
                self.cache
                    .iter()
                    .filter(|(key, _, _)| {
                        key.page == view.page
                            && key.rotation == view.rotation
                            && (key.tile.scale() == view.scale) == exact
                            && !self.errors.contains_key(key)
                            && intersection(key.rect(), view.rect).is_some()
                    })
                    .map(|(key, image, _)| (*key, image.clone())),
            );
        }
        images
    }
}

fn keys(view: Viewport) -> Result<Vec<Key>> {
    let Viewport {
        page,
        rotation,
        points,
        scale,
        rect: [left, top, right, bottom],
    } = view;
    anyhow::ensure!(
        scale.is_finite() && scale > 0. && scale <= 64.,
        tr!("不支持的页面缩放")
    );
    let w = (f64::from(points.0) * f64::from(scale)).ceil() as u32;
    let h = (f64::from(points.1) * f64::from(scale)).ceil() as u32;
    let x0 = ((left.max(0.) * scale).floor() as u32).min(w) / EDGE;
    let y0 = ((top.max(0.) * scale).floor() as u32).min(h) / EDGE;
    let x1 = ((right.max(0.) * scale).ceil() as u32)
        .min(w)
        .div_ceil(EDGE);
    let y1 = ((bottom.max(0.) * scale).ceil() as u32)
        .min(h)
        .div_ceil(EDGE);
    anyhow::ensure!(
        u64::from(x1.saturating_sub(x0)) * u64::from(y1.saturating_sub(y0)) <= 512,
        tr!("可见区域超过分块数量预算")
    );
    let mut result = Vec::new();
    for row in y0..y1 {
        for col in x0..x1 {
            let x = (col * EDGE).saturating_sub(GUTTER);
            let y = (row * EDGE).saturating_sub(GUTTER);
            let right = ((col + 1) * EDGE + GUTTER).min(w);
            let bottom = ((row + 1) * EDGE + GUTTER).min(h);
            result.push(Key {
                page,
                rotation,
                tile: DeviceTile::new(scale, x, y, right - x, bottom - y)?,
            });
        }
    }
    // Compact clusters finish covering old tiles sooner than a whole scanline.
    result.sort_by_key(|k| {
        let x = k.tile.x.div_ceil(EDGE);
        let y = k.tile.y.div_ceil(EDGE);
        (y / 2, x / 2, y % 2, x % 2)
    });
    Ok(result)
}
#[cfg(test)]
fn tiles(
    page: usize,
    points: (f32, f32),
    width: f32,
    dpr: f32,
    visible: [f32; 4],
) -> Result<Vec<Key>> {
    let display = width / points.0;
    keys(Viewport {
        page,
        rotation: Rotation::None,
        points,
        scale: display * dpr,
        rect: visible.map(|v| v / display),
    })
}

impl Breeze {
    fn lookahead(&self, view: Viewport, display: f32, window: &Window) -> Result<Vec<Key>> {
        let margin = f32::from(window.viewport_size().height) / display;
        let mut expanded = view;
        expanded.rect[0] -= EDGE as f32 / view.scale;
        expanded.rect[2] += EDGE as f32 / view.scale;
        expanded.rect[1] -= margin;
        expanded.rect[3] += margin;
        let mut result = keys(expanded)?;
        let previous = self
            .raster
            .directions
            .get(&view.page)
            .copied()
            .unwrap_or(1.);
        let direction = self
            .raster
            .views
            .iter()
            .find(|old| old.page == view.page)
            .map_or(previous, |old| {
                let delta = view.rect[1] - old.rect[1];
                if delta.abs() > 0.01 {
                    delta.signum()
                } else {
                    previous
                }
            });
        let sizes = &self.reading.sizes;
        let state = &self.books[self.active].state;
        let layout = state.layout;
        let rotation = state.rotation;
        let row = layout.row(view.page);
        for (neighbor, at_end) in [
            (
                row.checked_add(1)
                    .filter(|_| view.rect[3] + margin >= view.points.1),
                false,
            ),
            (
                row.checked_sub(1).filter(|_| view.rect[1] - margin <= 0.),
                true,
            ),
        ] {
            let Some(row) = neighbor else {
                continue;
            };
            let first = row.saturating_mul(layout.columns());
            if first >= sizes.len() {
                continue;
            }
            for page in layout.pages(first, sizes.len()) {
                let points = rotation.apply(sizes[page]);
                let height = f32::from(window.viewport_size().height)
                    / self.page_display_scale(page, window);
                let top = if at_end {
                    (points.1 - height).max(0.)
                } else {
                    0.
                };
                result.extend(keys(Viewport {
                    page,
                    rotation,
                    points,
                    scale: self.page_raster_scale(page, points, window),
                    rect: [0., top, points.0, (top + height).min(points.1)],
                })?);
            }
        }
        result.sort_by(|a, b| {
            let score = |key: &Key| {
                let r = key.rect();
                if key.page > view.page {
                    ((view.points.1 - view.rect[3]).max(0.) + r[1])
                        * if direction > 0. { 1. } else { 2. }
                } else if key.page < view.page {
                    (view.rect[1] + (rotation.apply(sizes[key.page]).1 - r[3]).max(0.))
                        * if direction < 0. { 1. } else { 2. }
                } else if r[1] >= view.rect[3] {
                    (r[1] - view.rect[3]) * if direction > 0. { 1. } else { 2. }
                } else if r[3] <= view.rect[1] {
                    (view.rect[1] - r[3]) * if direction < 0. { 1. } else { 2. }
                } else {
                    0.
                }
            };
            score(a).total_cmp(&score(b))
        });
        Ok(result)
    }
    pub(super) fn raster_surface(
        &self,
        page: usize,
        geometry: ViewGeometry,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let view = cx.entity();
        let paint_view = view.clone();
        let generation = self.generation;
        let notice = self
            .raster
            .errors
            .iter()
            .find(|(k, _)| k.page == page)
            .map(|(_, e)| e.clone());
        div()
            .absolute()
            .inset_0()
            .bg(rgb(0xffffff))
            .child(
                canvas(
                    move |bounds, window, cx| {
                        let clip = bounds.intersect(&window.content_mask().bounds);
                        // View-space points: the page surface already carries the view rotation.
                        let points = geometry.size();
                        let display = f32::from(bounds.size.width) / points.0;
                        if clip.size.width <= px(0.) || clip.size.height <= px(0.) || display <= 0.
                        {
                            return Vec::new();
                        }
                        let rect = [
                            f32::from(clip.left() - bounds.left()) / display,
                            f32::from(clip.top() - bounds.top()) / display,
                            f32::from(clip.right() - bounds.left()) / display,
                            f32::from(clip.bottom() - bounds.top()) / display,
                        ];
                        view.update(cx, |this, cx| {
                            if generation != this.generation {
                                return Vec::new();
                            }
                            let viewport = Viewport {
                                page,
                                rotation: geometry.rotation,
                                points,
                                scale: this.page_raster_scale(page, points, window),
                                rect,
                            };
                            let images = this.raster.images(viewport);
                            let prefetch = match this.lookahead(viewport, display, window) {
                                Ok(keys) => keys,
                                Err(error) => {
                                    this.message = format!("{error:#}");
                                    Vec::new()
                                }
                            };
                            let queued = this.raster.pending.is_some();
                            let pending = this.raster.pending.get_or_insert_with(Demand::default);
                            pending.views.push(viewport);
                            pending.prefetch.extend(prefetch);
                            if !queued {
                                let view = cx.entity();
                                cx.defer(move |cx| {
                                    view.update(cx, |this, cx| {
                                        if generation != this.generation
                                            || this.screen != Screen::Reader
                                        {
                                            return;
                                        }
                                        if let Some(demand) = this.raster.pending.take() {
                                            if let Err(error) = this.raster.commit(demand, cx) {
                                                this.message = format!("{error:#}");
                                            }
                                            this.pump_visible(cx);
                                        }
                                    });
                                });
                            }
                            images
                        })
                    },
                    move |bounds, images, window, cx| {
                        let display = f32::from(bounds.size.width) / geometry.size().0;
                        for (key, image) in images {
                            let tile = key.tile;
                            let factor = display / tile.scale();
                            let image_bounds = Bounds::new(
                                bounds.origin
                                    + point(px(tile.x as f32 * factor), px(tile.y as f32 * factor)),
                                size(
                                    px(tile.width as f32 * factor),
                                    px(tile.height as f32 * factor),
                                ),
                            );
                            let rect = key.rect();
                            let clip = Bounds::new(
                                bounds.origin + point(px(rect[0] * display), px(rect[1] * display)),
                                size(
                                    px((rect[2] - rect[0]) * display),
                                    px((rect[3] - rect[1]) * display),
                                ),
                            )
                            .intersect(&bounds);
                            if let Err(error) = window.paint_image(
                                clip,
                                image_bounds,
                                Corners::default(),
                                image,
                                0,
                                false,
                            ) {
                                let view = paint_view.clone();
                                let message = tf!("页面纹理上传失败：{}", format!("{:#}", error));
                                cx.defer(move |cx| {
                                    view.update(cx, |this, cx| {
                                        if generation == this.generation {
                                            this.raster.errors.insert(key, message);
                                            cx.notify();
                                        }
                                    });
                                });
                            }
                        }
                    },
                )
                .size_full(),
            )
            .when_some(notice, |d, message| {
                d.child(
                    div()
                        .absolute()
                        .top_2()
                        .left_2()
                        .bg(rgb(0xffffff))
                        .text_color(rgb(0xb56b3c))
                        .child(message),
                )
            })
            .into_any_element()
    }
    pub(super) fn pump_raster(&mut self, prefetch: bool, cx: &mut Context<Self>) -> bool {
        if self.raster.busy {
            return true;
        }
        let Some(key) = self.raster.next_key(prefetch) else {
            return false;
        };
        let generation = self.generation;
        let receiver = match self.worker.request_tile(
            self.books[self.active].source.clone(),
            key.page,
            key.tile,
            key.rotation,
        ) {
            Ok(receiver) => receiver,
            Err(error) => {
                self.raster.errors.insert(key, format!("{error:#}"));
                cx.notify();
                return false;
            }
        };
        self.raster.busy = true;
        self.raster.task = Some(cx.spawn(async move |this, cx| {
            let result = receiver.await;
            let _ = this.update(cx, |this, cx| {
                if generation != this.generation {
                    return;
                }
                this.raster.busy = false;
                if this.raster.requested(key) {
                    let result = (|| -> Result<()> {
                        let page =
                            result.map_err(|e| anyhow::anyhow!(tf!("页面请求取消：{}", e)))??;
                        anyhow::ensure!(
                            page.fingerprint == this.reading.fingerprint,
                            tr!("文档已在外部改变，请重新打开")
                        );
                        anyhow::ensure!(page.tile == Some(key.tile), tr!("页面分块不匹配"));
                        let pixels = page
                            .pixels
                            .ok_or_else(|| anyhow::anyhow!(tr!("缺少页面像素")))?;
                        anyhow::ensure!(
                            pixels.width == key.tile.width && pixels.height == key.tile.height,
                            tr!("渲染器未返回所需清晰度")
                        );
                        this.raster.insert(key, pixels, cx)
                    })();
                    if let Err(error) = result {
                        this.raster.errors.insert(key, format!("{error:#}"));
                    }
                }
                this.pump_visible(cx);
                cx.notify();
            });
        }));
        true
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn physical_resolution_tracks_zoom_and_dpr_without_a_full_page_allocation() {
        let visible = [0., 0., 800., 600.];
        let normal = tiles(0, (600., 800.), 800., 2., visible).unwrap();
        let zoomed = tiles(0, (600., 800.), 3200., 2., visible).unwrap();
        assert_eq!(normal.len(), zoomed.len());
        assert!(
            normal
                .iter()
                .all(|k| k.tile.width <= 512 && k.tile.height <= 512)
        );
        assert_eq!(zoomed[0].tile.scale(), normal[0].tile.scale() * 4.);
        let retina = tiles(0, (600., 800.), 800., 2., visible).unwrap();
        let standard = tiles(0, (600., 800.), 800., 1., visible).unwrap();
        assert_eq!(retina[0].tile.scale(), standard[0].tile.scale() * 2.);
        assert_ne!(retina[0], standard[0]);
    }

    #[test]
    fn panning_reuses_overlapping_keys_and_clips_the_last_tile() {
        let first = tiles(7, (600., 800.), 600., 2., [0., 0., 600., 400.]).unwrap();
        let next = tiles(7, (600., 800.), 600., 2., [0., 100., 600., 500.]).unwrap();
        assert!(next.iter().any(|k| first.contains(k)));
        assert!(next.iter().all(|k| k.tile.x + k.tile.width <= 1200));
        assert!(next.iter().all(|k| k.page == 7));
        assert!(
            tiles(0, (600., 800.), 600., 2., [0., 0., 0., 0.])
                .unwrap()
                .is_empty()
        );
    }
}

#[cfg(test)]
mod atlas_tests {
    use super::*;
    use gpui_kit::test::TestWindowExt;

    #[gpui_kit::test]
    fn source_clear_releases_uploaded_gpu_tiles_after_the_window_update(cx: &mut TestAppContext) {
        let worker = PdfWorker::new().unwrap();
        let metadata =
            futures::executor::block_on(worker.request_metadata(Source::Sample, 0).unwrap())
                .unwrap()
                .unwrap();
        let (handle, view) = crate::ui_tests::app(cx);
        cx.update_window(handle, |_, window, cx| {
            view.update(cx, |view, _| {
                view.reading.sizes = metadata.page_sizes.clone();
                view.preview = Some(Preview::from_page(metadata).unwrap());
                view.books[0].state.zoom = Zoom::Percent(100);
                view.loading = true;
            });
            window.render_frame(cx);
        })
        .unwrap();
        let mut uploaded = None;
        cx.update_window(handle, |_, window, cx| {
            view.update(cx, |view, cx| {
                let key = view.raster.wanted[0];
                view.raster
                    .insert(
                        key,
                        Pixels {
                            width: key.tile.width,
                            height: key.tile.height,
                            bgra: vec![255; (key.tile.width * key.tile.height * 4) as usize],
                        },
                        cx,
                    )
                    .unwrap();
                uploaded = view.raster.get(key);
            });
            window.render_frame(cx);
            assert!(window.has_image_atlas_entry(uploaded.as_ref().unwrap()));
            view.update(cx, |view, cx| view.raster.clear(cx));
        })
        .unwrap();
        cx.update_window(handle, |_, window, _| {
            assert!(
                !window.has_image_atlas_entry(uploaded.as_ref().unwrap()),
                "CPU eviction must also release the GPU atlas entry"
            );
        })
        .unwrap();
    }
}

#[cfg(test)]
mod continuity_tests {
    use super::*;
    use gpui_kit::test::TestWindowExt;

    fn metadata() -> breeze_pdf::Page {
        futures::executor::block_on(
            PdfWorker::new()
                .unwrap()
                .request_metadata(Source::Sample, 0)
                .unwrap(),
        )
        .unwrap()
        .unwrap()
    }
    fn pixels(key: Key) -> Pixels {
        Pixels {
            width: key.tile.width,
            height: key.tile.height,
            bgra: vec![255; key.bytes()],
        }
    }
    fn warm(raster: &mut RasterView, cx: &mut App) {
        while let Some(key) = raster.next_key(false) {
            raster.insert(key, pixels(key), cx).unwrap();
        }
        while let Some(key) = raster.next_key(true) {
            raster.insert(key, pixels(key), cx).unwrap();
        }
    }
    fn seed(view: &mut Breeze, page: breeze_pdf::Page, zoom: Zoom) {
        view.reading.sizes = page.page_sizes.clone();
        view.reading.fingerprint = page.fingerprint.clone();
        let preview = Preview::from_page(page).unwrap();
        view.reading.insert(0, preview.clone());
        view.preview = Some(preview);
        view.books[0].state.zoom = zoom;
        view.books[0].state.layout = ReadingLayout::Single;
        view.loading = true;
    }
    #[gpui_kit::test]
    fn custom_zoom_keeps_pixels_when_sidebar_moves_after_a_zoom_change(cx: &mut TestAppContext) {
        let page = metadata();
        let (handle, view) = crate::ui_tests::app(cx);
        cx.update_window(handle, |_, window, cx| {
            view.update(cx, |view, _| seed(view, page, Zoom::Percent(100)));
            window.render_frame(cx);
        })
        .unwrap();
        cx.update_window(handle, |_, window, cx| {
            view.update(cx, |view, cx| {
                warm(&mut view.raster, cx);
                view.set_zoom(Zoom::Percent(167), cx);
            });
            window.render_frame(cx);
        })
        .unwrap();
        cx.update_window(handle, |_, window, cx| {
            view.update(cx, |view, cx| {
                let viewport = view.raster.views[0];
                assert!(
                    !view.raster.images(viewport).is_empty(),
                    "zoom preparation must retain the previous picture"
                );
                warm(&mut view.raster, cx);
                view.sidebar = false;
            });
            window.render_frame(cx);
        })
        .unwrap();
        for width in [360., 168., 280.] {
            cx.update_window(handle, |_, window, cx| {
                view.update(cx, |view, _| {
                    assert!(
                        view.raster.next_key(false).is_none(),
                        "sidebar changes must reuse the prepared pixels"
                    );
                    assert!(
                        view.raster
                            .wanted
                            .iter()
                            .all(|key| (key.tile.scale() - 167. / 75. * 2.).abs() < 0.00001)
                    );
                    view.sidebar = true;
                    view.navigation_width = width;
                });
                window.render_frame(cx);
            })
            .unwrap();
        }
    }
    #[gpui_kit::test]
    fn fit_width_reuses_reserved_density_and_prepares_the_next_page(cx: &mut TestAppContext) {
        let page = metadata();
        let (handle, view) = crate::ui_tests::app(cx);
        cx.update_window(handle, |_, window, cx| {
            view.update(cx, |view, _| seed(view, page, Zoom::FitWidth));
            window.render_frame(cx);
        })
        .unwrap();
        let mut scale = 0.;
        cx.update_window(handle, |_, window, cx| {
            view.update(cx, |view, cx| {
                scale = view.raster.wanted[0].tile.scale();
                assert!(
                    view.raster.prefetch_pages().contains(&1),
                    "neighbor page should be queued before it becomes visible"
                );
                assert!(
                    view.raster
                        .prefetch
                        .iter()
                        .map(|key| key.bytes())
                        .sum::<usize>()
                        <= PREFETCH_BUDGET
                );
                assert!(
                    view.raster
                        .prefetch
                        .iter()
                        .all(|key| !view.raster.wanted.contains(key))
                );
                warm(&mut view.raster, cx);
                view.sidebar = false;
                assert_eq!(
                    view.books[0].state.navigation.page(),
                    0,
                    "prefetch is not navigation"
                );
            });
            window.render_frame(cx);
        })
        .unwrap();
        for width in [360., 208., 168.] {
            cx.update_window(handle, |_, window, cx| {
                view.update(cx, |view, _| {
                    assert!(
                        view.raster
                            .wanted
                            .iter()
                            .all(|key| key.tile.scale() == scale)
                    );
                    assert!(view.raster.next_key(false).is_none());
                    view.sidebar = true;
                    view.navigation_width = width;
                });
                window.render_frame(cx);
            })
            .unwrap();
        }
    }
    #[gpui_kit::test]
    fn continuous_sidebar_resize_preserves_position_instead_of_jumping_to_page_top(
        cx: &mut TestAppContext,
    ) {
        let page = metadata();
        let (handle, view) = crate::ui_tests::app(cx);
        cx.update_window(handle, |_, window, cx| {
            view.update(cx, |view, _| {
                seed(view, page, Zoom::Percent(150));
                view.books[0].state.layout = ReadingLayout::Continuous;
            });
            window.render_frame(cx);
        })
        .unwrap();
        cx.update_window(handle, |_, window, cx| {
            view.update(cx, |view, _| {
                view.reading
                    .pages_scroll
                    .base_handle()
                    .set_offset(point(px(0.), px(-400.)))
            });
            window.render_frame(cx);
        })
        .unwrap();
        for open in [false, true, false, true] {
            cx.update_window(handle, |_, window, cx| {
                view.update(cx, |view, _| view.sidebar = open);
                window.render_frame(cx);
                assert_eq!(view.read(cx).reading.pages_scroll.offset().y, px(-400.));
            })
            .unwrap();
        }
    }
    #[gpui_kit::test]
    fn scale_transitions_finish_without_erasing_uncovered_pixels_or_exceeding_budget(
        cx: &mut TestAppContext,
    ) {
        cx.update(|cx| {
            let marker = texture(Pixels {
                width: 1,
                height: 1,
                bgra: vec![255; 4],
            })
            .unwrap();
            for old_scale in [4., 8.] {
                for ratio in [0.5, 0.9, 1.01, 1.1, 2.] {
                    let scale = old_scale * ratio;
                    let old = Viewport {
                        page: 0,
                        rotation: Rotation::None,
                        points: (1000., 1400.),
                        scale: old_scale,
                        rect: [0., 100., 4200. / old_scale, 100. + 2100. / old_scale],
                    };
                    let new = Viewport {
                        scale,
                        rect: [0., 100., 4200. / scale, 100. + 2100. / scale],
                        ..old
                    };
                    let mut raster = RasterView {
                        cache: keys(old)
                            .unwrap()
                            .into_iter()
                            .map(|key| (key, marker.clone(), key.bytes()))
                            .collect(),
                        ..RasterView::default()
                    };
                    raster
                        .commit(
                            Demand {
                                views: vec![new],
                                prefetch: Vec::new(),
                            },
                            cx,
                        )
                        .unwrap();
                    assert!(!raster.images(new).is_empty());
                    while let Some(key) = raster.next_key(false) {
                        raster
                            .insert(key, pixels(key), cx)
                            .unwrap_or_else(|e| panic!("{old_scale} -> {scale}: {e}"));
                        assert!(raster.cache.iter().map(|e| e.2).sum::<usize>() <= BUDGET);
                        for y in 0..9 {
                            for x in 0..9 {
                                let px = new.rect[2].min(old.rect[2]) * x as f32 / 9.;
                                let py =
                                    100. + (new.rect[3].min(old.rect[3]) - 100.) * y as f32 / 9.;
                                assert!(
                                    raster.images(new).iter().any(|(key, _)| {
                                        let r = key.rect();
                                        px >= r[0] && px < r[2] && py >= r[1] && py < r[3]
                                    }),
                                    "previously visible content must stay covered"
                                );
                            }
                        }
                    }
                    assert!(raster.cache.iter().all(|e| e.0.tile.scale() == scale));
                }
            }
        });
    }
    #[gpui_kit::test]
    async fn prefetch_completes_without_another_frame_or_scroll(cx: &mut TestAppContext) {
        let page = metadata();
        let (handle, view) = crate::ui_tests::app(cx);
        cx.update_window(handle, |_, window, cx| {
            view.update(cx, |view, _| {
                seed(view, page, Zoom::FitPage);
                view.loading = false;
            });
            window.render_frame(cx);
        })
        .unwrap();
        cx.condition(&view, |view, _| {
            !view.raster.wanted.is_empty()
                && view.raster.visible_ready()
                && !view.raster.prefetch.is_empty()
                && view
                    .raster
                    .prefetch
                    .iter()
                    .all(|key| view.raster.contains(*key))
                && view.reading.page(1).is_some()
        })
        .await;
        view.update(cx, |view, _| {
            assert_eq!(view.books[0].state.navigation.page(), 0);
            assert!(view.raster.next_key(false).is_none());
            assert!(view.raster.next_key(true).is_none());
            assert!(view.raster.cache.iter().map(|e| e.2).sum::<usize>() <= BUDGET);
        });
    }
    #[gpui_kit::test]
    fn large_fit_width_transition_keeps_native_quality_and_previous_picture(
        cx: &mut TestAppContext,
    ) {
        let page = metadata();
        let (handle, view) = crate::ui_tests::app(cx);
        cx.simulate_window_resize(handle, size(px(2304.), px(1217.)));
        cx.update_window(handle, |_, window, cx| {
            view.update(cx, |view, _| {
                seed(view, page, Zoom::FitWidth);
                view.navigation_width = 360.;
            });
            window.render_frame(cx);
        })
        .unwrap();
        cx.update_window(handle, |_, window, cx| {
            view.update(cx, |view, cx| {
                assert!(view.raster.wanted.iter().all(|key| key.tile.scale()
                    >= view.page_display_scale(0, window) * window.scale_factor()));
                warm(&mut view.raster, cx);
                view.sidebar = false;
            });
            window.render_frame(cx);
        })
        .unwrap();
        cx.update_window(handle, |_, window, cx| {
            view.update(cx, |view, cx| {
                assert!(!view.raster.images(view.raster.views[0]).is_empty());
                assert!(view.raster.wanted.iter().all(|key| key.tile.scale()
                    >= view.page_display_scale(0, window) * window.scale_factor()));
                warm(&mut view.raster, cx);
                assert!(view.raster.visible_ready());
                assert!(view.raster.cache.iter().map(|e| e.2).sum::<usize>() <= BUDGET);
            });
        })
        .unwrap();
    }
}
