//! Page scheduling: whole-page rasters at the density the reader's surface
//! needs, with the pages a reader is about to reach prepared ahead.
//!
//! A page is rendered as one raster because the engine exposes no way to
//! rasterize a sub-region; the trade is that a page's cost grows with its size
//! and zoom rather than being bounded by the viewport. The visible page is
//! therefore never refused and never evicted, while the cache ceiling applies to
//! the pages that are only being predicted.
use crate::*;
use breeze_core::Rotation;
use breeze_core::annotations::ViewGeometry;
use std::collections::{HashMap, VecDeque};

// Ceiling on rasters whose pages are not currently visible. The visible page is
// always resident: a deep zoom costs memory rather than being refused.
const BUDGET: usize = 96 * 1024 * 1024;

/// Identity of one rendered page: the same page turned the other way, or asked
/// for at another density, is another raster.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct Key {
    page: usize,
    rotation: Rotation,
    scale_bits: u32,
}
impl Key {
    fn new(page: usize, rotation: Rotation, scale: f32) -> Self {
        Self {
            page,
            rotation,
            scale_bits: scale.to_bits(),
        }
    }
    fn scale(self) -> f32 {
        f32::from_bits(self.scale_bits)
    }
    /// Whole-DPI density to ask the engine for. It takes an integer DPI, so the
    /// request rounds up: the result carries at or just above the density the
    /// surface asked for, and painting scales that small difference back out.
    fn requested_dpi(self) -> u32 {
        (f64::from(self.scale()) * 72.).ceil() as u32
    }
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
    pub fn page_failed(&self, page: usize) -> bool {
        self.errors.keys().any(|key| key.page == page)
    }
    #[cfg(test)]
    pub fn requested_scales(&self) -> Vec<f32> {
        self.wanted.iter().map(|k| k.scale()).collect()
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
    /// Whether a raster of another density is the only thing a visible page has
    /// to show while its exact density is being prepared. Those pixels stay as a
    /// bridge; they are never presented as the final resolution.
    fn bridges(&self, key: Key) -> bool {
        self.views.iter().any(|view| {
            view.page == key.page
                && view.rotation == key.rotation
                && view.scale != key.scale()
                && !self.contains(Key::new(view.page, view.rotation, view.scale))
                && !self.errors.contains_key(&key)
        })
    }
    /// Drop rasters that are neither wanted, predicted, nor bridging a visible
    /// page. Prediction has to be kept here: a prepared neighbor is not visible
    /// yet, and dropping it on arrival would request it again every frame.
    fn retire_obsolete(&mut self, cx: &mut App) {
        let retired: Vec<_> = self
            .cache
            .iter()
            .map(|(key, _, _)| *key)
            .filter(|key| {
                !self.wanted.contains(key) && !self.prefetch.contains(key) && !self.bridges(*key)
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
            let key = Key::new(view.page, view.rotation, view.scale);
            if !wanted.contains(&key) {
                wanted.push(key);
            }
        }
        // Prediction is bounded by how many pages lookahead asks for, not by a
        // byte estimate: the ceiling is applied when a raster is resident, and
        // the visible page is never subject to it.
        let mut prefetch = Vec::new();
        for key in demand.prefetch {
            if !wanted.contains(&key) && !prefetch.contains(&key) {
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
        self.retire_obsolete(cx);
        Ok(())
    }
    fn insert(&mut self, key: Key, pixels: Pixels, cx: &mut App) -> Result<()> {
        let bytes = pixels.bgra.len();
        let image = texture(pixels)?;
        self.retire_obsolete(cx);
        // Evict predicted or obsolete rasters before this one, oldest first, and
        // never the visible page. When there is nothing left to evict the cache
        // grows instead of refusing the page the reader is looking at.
        while self.cache.iter().map(|e| e.2).sum::<usize>() + bytes > BUDGET {
            match self
                .cache
                .iter()
                .position(|e| !self.wanted.contains(&e.0) && !self.bridges(e.0))
            {
                Some(index) => {
                    if let Some((_, image, _)) = self.cache.remove(index) {
                        cx.defer(move |cx| cx.drop_image(image, None));
                    }
                }
                None => break,
            }
        }
        self.cache.retain(|(existing, _, _)| *existing != key);
        self.cache.push_back((key, image, bytes));
        // The density this one replaces has just stopped being the only picture a
        // visible page has, so it is retired in the same update instead of waiting
        // for the next scheduling pass.
        self.retire_obsolete(cx);
        Ok(())
    }
    fn next_key(&self, prefetch: bool) -> Option<Key> {
        if prefetch && !self.visible_ready() {
            return None;
        }
        let missing = |key: &Key| !self.contains(*key) && !self.errors.contains_key(key);
        let candidates = if prefetch {
            &self.prefetch
        } else {
            &self.wanted
        };
        candidates.iter().find(|key| missing(key)).copied()
    }
    /// Record a failed page so the same raster is not requested again until the
    /// demand changes, and so the reader can say the page did not render.
    fn fail(&mut self, key: Key, message: String) {
        if !self.contains(key) {
            self.errors.insert(key, message);
        }
    }
    fn images(&self, view: Viewport) -> Vec<(Key, Arc<RenderImage>)> {
        let mut images = Vec::new();
        // A raster of another density stays under the new one while it is being
        // prepared. This is presentation only: the final pixels still match the
        // density the surface asked for.
        for exact in [false, true] {
            images.extend(
                self.cache
                    .iter()
                    .filter(|(key, _, _)| {
                        key.page == view.page
                            && key.rotation == view.rotation
                            && (key.scale() == view.scale) == exact
                            && !self.errors.contains_key(key)
                    })
                    .map(|(key, image, _)| (*key, image.clone())),
            );
        }
        images
    }
}

/// One raster per visible page at the density the surface asked for.
fn keys(view: Viewport) -> Result<Vec<Key>> {
    anyhow::ensure!(
        view.scale.is_finite() && view.scale > 0. && view.scale <= 64.,
        tr!("Unsupported page zoom")
    );
    Ok(vec![Key::new(view.page, view.rotation, view.scale)])
}
impl Breeze {
    fn lookahead(&self, view: Viewport, display: f32, window: &Window) -> Result<Vec<Key>> {
        let margin = f32::from(window.viewport_size().height) / display;
        let mut expanded = view;
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
        // Predict forward along the reading direction. The chain only starts when
        // the visible region already reaches the page end, so a stale reading
        // position cannot keep predicting, and it stops after two pages.
        if view.rect[3] + margin >= view.points.1 {
            let mut predicted = 0;
            for step in 1..=2 {
                if predicted >= 2 {
                    break;
                }
                let Some(next) = row.checked_add(step) else {
                    break;
                };
                let first = next.saturating_mul(layout.columns());
                if first >= sizes.len() {
                    break;
                }
                for page in layout.pages(first, sizes.len()) {
                    let points = rotation.apply(sizes[page]);
                    let height = f32::from(window.viewport_size().height)
                        / self.page_display_scale(page, window);
                    result.extend(keys(Viewport {
                        page,
                        rotation,
                        points,
                        scale: self.page_raster_scale(page, points, window),
                        rect: [0., 0., points.0, height.min(points.1)],
                    })?);
                    predicted += 1;
                }
            }
        }
        if view.rect[1] - margin <= 0.
            && let Some(previous) = row.checked_sub(1)
        {
            let first = previous.saturating_mul(layout.columns());
            if first < sizes.len() {
                for page in layout.pages(first, sizes.len()) {
                    let points = rotation.apply(sizes[page]);
                    let height = f32::from(window.viewport_size().height)
                        / self.page_display_scale(page, window);
                    let top = (points.1 - height).max(0.);
                    result.extend(keys(Viewport {
                        page,
                        rotation,
                        points,
                        scale: self.page_raster_scale(page, points, window),
                        rect: [0., top, points.0, (top + height).min(points.1)],
                    })?);
                }
            }
        }
        result.sort_by(|a, b| {
            // One raster covers a whole page, so the only useful order is by page:
            // nearest first, in the direction the reader is already moving.
            let score = |key: &Key| {
                let forward = key.page >= view.page;
                let distance = key.page.abs_diff(view.page) as f32;
                distance * if (direction > 0.) == forward { 1. } else { 2. }
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
        // One question decides whether this surface has anything to report, so the
        // status bar and the surface cannot disagree about a page having failed.
        let notice = if self.raster.page_failed(page) {
            self.raster
                .errors
                .iter()
                .find(|(k, _)| k.page == page)
                .map(|(_, e)| e.clone())
        } else {
            None
        };
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
                        // The raster covers the whole page, including when it was
                        // prepared at another density, so it is painted across the
                        // page surface and the density difference becomes a scale.
                        for (key, image) in images {
                            if let Err(error) = window.paint_image(
                                bounds,
                                bounds,
                                Corners::default(),
                                image,
                                0,
                                false,
                            ) {
                                let view = paint_view.clone();
                                let message =
                                    tf!("Page texture upload failed: {}", format!("{:#}", error));
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
                        .id(("page-render-error", page))
                        .test_support()
                        .absolute()
                        .top_2()
                        .left_2()
                        .right_2()
                        .p_2()
                        .text_size(px(11.))
                        .bg(rgb(0xffffff))
                        .text_color(rgb(0xb56b3c))
                        .child(tr!(
                            "Page rendering failed; some content could not be displayed"
                        ))
                        .child(div().child(message.chars().take(512).collect::<String>())),
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
        let receiver = match self.worker.request_page(
            self.books[self.active].source.clone(),
            key.page,
            key.rotation,
            key.scale(),
        ) {
            Ok(receiver) => receiver,
            Err(error) => {
                self.raster.fail(key, format!("{error:#}"));
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
                        let page = result
                            .map_err(|e| anyhow::anyhow!(tf!("Page request cancelled: {}", e)))??;
                        anyhow::ensure!(
                            page.fingerprint == this.reading.fingerprint,
                            tr!("The document changed externally. Reopen it.")
                        );
                        let pixels = page
                            .pixels
                            .ok_or_else(|| anyhow::anyhow!(tr!("Page pixels are missing")))?;
                        // The engine can round the density up to whole DPI, and can
                        // also reduce it to fit its own budget. Only the first is
                        // acceptable: the surface asked for this density and the
                        // raster is cached under it.
                        let resolution =
                            tr!("The renderer did not return the requested resolution");
                        // The engine rounds the density up to whole DPI, and can also
                        // reduce it to fit its own budget. Both sides compare that
                        // DPI integer, so only rounding up is accepted here.
                        let dpi = key.requested_dpi();
                        anyhow::ensure!(page.rendered_dpi == Some(dpi), resolution);
                        // The surface asked for this page at this density in this
                        // orientation, so the returned raster has to cover exactly
                        // that. One pixel of slack absorbs the engine's rounding.
                        let rendered = dpi as f32 / 72.;
                        let view = key.rotation.apply(this.reading.sizes[key.page]);
                        let expected = (
                            (f64::from(view.0) * f64::from(rendered)).ceil() as u32,
                            (f64::from(view.1) * f64::from(rendered)).ceil() as u32,
                        );
                        anyhow::ensure!(
                            pixels.width.abs_diff(expected.0) <= 1
                                && pixels.height.abs_diff(expected.1) <= 1,
                            resolution
                        );
                        this.raster.insert(key, pixels, cx)
                    })();
                    if let Err(error) = result {
                        this.raster.fail(key, format!("{error:#}"));
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
    fn a_viewport_maps_to_one_raster_at_the_density_it_asks_for() {
        let view = Viewport {
            page: 4,
            rotation: Rotation::None,
            points: (600., 800.),
            scale: 2.3466,
            rect: [0., 0., 600., 300.],
        };
        let once = keys(view).unwrap();
        assert_eq!(once.len(), 1, "one raster covers the visible page");
        assert_eq!(once[0].page, 4);
        assert_eq!(once[0].scale(), 2.3466);
        // Whole DPI only: the engine rounds the density up, never down, and the
        // surface caches the raster under the density it asked for.
        assert_eq!(once[0].requested_dpi(), (2.3466f64 * 72.).ceil() as u32);
        assert!(once[0].requested_dpi() as f32 / 72. >= once[0].scale());
        // Orientation is part of the identity: the same page turned holds other
        // pixels, so it is another raster.
        let turned = keys(Viewport {
            rotation: Rotation::Right,
            ..view
        })
        .unwrap();
        assert_ne!(turned[0], once[0]);
        assert!(keys(Viewport { scale: 0., ..view }).is_err());
        assert!(keys(Viewport { scale: 1e6, ..view }).is_err());
    }

    #[test]
    fn a_page_key_is_distinct_per_page_density_and_orientation() {
        let base = Key::new(2, Rotation::None, 2.);
        assert_eq!(base, Key::new(2, Rotation::None, 2.));
        assert_ne!(base, Key::new(2, Rotation::Right, 2.));
        assert_ne!(base, Key::new(2, Rotation::None, 2.5));
        assert_ne!(base, Key::new(3, Rotation::None, 2.));
    }
}

#[cfg(test)]
mod page_tests;

#[cfg(test)]
mod atlas_tests {
    use super::*;
    use gpui_kit::test::TestWindowExt;

    #[gpui_kit::test]
    fn source_clear_releases_uploaded_gpu_rasters_after_the_window_update(cx: &mut TestAppContext) {
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
                            width: 4,
                            height: 4,
                            bgra: vec![255; 64],
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
    /// A tiny stand-in raster. These tests assert scheduling, residency and
    /// eviction, so the pixels themselves only have to be a valid image.
    fn pixels(_key: Key) -> Pixels {
        Pixels {
            width: 2,
            height: 2,
            bgra: vec![255; 16],
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
                            .all(|key| (key.scale() - 167. / 75. * 2.).abs() < 0.00001)
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
                scale = view.raster.wanted[0].scale();
                assert!(
                    view.raster.prefetch_pages().contains(&1),
                    "neighbor page should be queued before it becomes visible"
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
                    assert!(view.raster.wanted.iter().all(|key| key.scale() == scale));
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
    fn a_density_change_bridges_with_the_previous_raster_until_the_new_one_arrives(
        cx: &mut TestAppContext,
    ) {
        cx.update(|cx| {
            let marker = texture(Pixels {
                width: 1,
                height: 1,
                bgra: vec![255; 4],
            })
            .unwrap();
            for old_scale in [1., 2., 4.] {
                for ratio in [0.5, 0.9, 1.01, 1.1, 2.] {
                    let scale = old_scale * ratio;
                    let old = Viewport {
                        page: 0,
                        rotation: Rotation::None,
                        points: (600., 800.),
                        scale: old_scale,
                        rect: [0., 0., 600., 800.],
                    };
                    let new = Viewport { scale, ..old };
                    let old_key = Key::new(0, Rotation::None, old_scale);
                    let mut raster = RasterView {
                        cache: [(old_key, marker.clone(), 4)].into_iter().collect(),
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
                    assert!(
                        !raster.images(new).is_empty(),
                        "{old_scale} -> {scale}: the previous density must bridge the change"
                    );
                    let fresh = raster
                        .next_key(false)
                        .expect("the new density is requested");
                    raster.insert(fresh, pixels(fresh), cx).unwrap();
                    assert!(raster.visible_ready());
                    assert!(
                        raster
                            .images(new)
                            .iter()
                            .all(|(key, _)| key.scale() == scale),
                        "once the requested density is resident it is the only picture"
                    );
                    assert!(
                        raster
                            .cache
                            .iter()
                            .all(|entry| entry.0.scale() <= scale.max(old_scale))
                    );
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
                assert!(
                    view.raster.wanted.iter().all(|key| key.scale()
                        >= view.page_display_scale(0, window) * window.scale_factor())
                );
                warm(&mut view.raster, cx);
                view.sidebar = false;
            });
            window.render_frame(cx);
        })
        .unwrap();
        cx.update_window(handle, |_, window, cx| {
            view.update(cx, |view, cx| {
                assert!(!view.raster.images(view.raster.views[0]).is_empty());
                assert!(
                    view.raster.wanted.iter().all(|key| key.scale()
                        >= view.page_display_scale(0, window) * window.scale_factor())
                );
                warm(&mut view.raster, cx);
                assert!(view.raster.visible_ready());
            });
        })
        .unwrap();
    }
}

#[cfg(test)]
mod integrity_tests;
