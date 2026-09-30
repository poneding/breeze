//! Device-resolution page tiles. Layout/measurement does not submit raster work.
use crate::*;
use breeze_core::annotations::PageGeometry;
use breeze_pdf::DeviceTile;
use std::collections::{HashMap, VecDeque};

// Including both gutters, fit a 512px atlas cell. A 516px texture wastes
// three quarters of GPUI Metal's default 1024px atlas allocation.
const EDGE: u32 = 508;
const GUTTER: u32 = 2;
const BUDGET: usize = 56 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct Key {
    page: usize,
    tile: DeviceTile,
}
#[derive(Default)]
pub(super) struct RasterView {
    cache: VecDeque<(Key, Arc<RenderImage>, usize)>,
    wanted: Vec<Key>,
    pending: Option<Vec<Key>>,
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
        self.errors.clear();
    }
    pub fn clear(&mut self, cx: &mut App) {
        self.cancel();
        for (_, image, _) in self.cache.drain(..) {
            cx.defer(move |cx| cx.drop_image(image, None));
        }
    }
    fn discard_other_scales(&mut self, wanted: &[Key], cx: &mut App) {
        // A different zoom cannot reuse these pixels. Release it before allocating
        // the next viewport, while retaining offscreen tiles at the current scale.
        self.cache.retain(|(key, image, _)| {
            let visible_page = wanted.iter().any(|next| next.page == key.page);
            let same_scale = wanted
                .iter()
                .any(|next| next.page == key.page && next.tile.scale() == key.tile.scale());
            if visible_page && !same_scale {
                let image = image.clone();
                cx.defer(move |cx| cx.drop_image(image, None));
                false
            } else {
                true
            }
        });
    }
    pub fn busy(&self) -> bool {
        self.busy
    }
    fn get(&self, key: Key) -> Option<Arc<RenderImage>> {
        self.cache.iter().find(|e| e.0 == key).map(|e| e.1.clone())
    }
    fn insert(&mut self, key: Key, pixels: Pixels, cx: &mut App) -> Result<()> {
        let bytes = pixels.bgra.len();
        anyhow::ensure!(bytes <= BUDGET, "页面分块超过内存预算");
        while self.cache.iter().map(|e| e.2).sum::<usize>() + bytes > BUDGET {
            let index = self
                .cache
                .iter()
                .position(|e| !self.wanted.contains(&e.0))
                .ok_or_else(|| anyhow::anyhow!("当前可见区域超过清晰显示内存预算"))?;
            if let Some((_, image, _)) = self.cache.remove(index) {
                cx.defer(move |cx| cx.drop_image(image, None));
            }
        }
        self.cache.push_back((key, texture(pixels)?, bytes));
        Ok(())
    }
}

fn tiles(
    page: usize,
    points: (f32, f32),
    width: f32,
    dpr: f32,
    visible: [f32; 4],
) -> Result<Vec<Key>> {
    let scale = width / points.0 * dpr;
    anyhow::ensure!(
        scale.is_finite() && scale > 0. && scale <= 64.,
        "不支持的页面缩放"
    );
    let w = (points.0 * scale).ceil() as u32;
    let h = (points.1 * scale).ceil() as u32;
    let [left, top, right, bottom] = visible;
    let x0 = (left.max(0.) * dpr).floor() as u32 / EDGE;
    let y0 = (top.max(0.) * dpr).floor() as u32 / EDGE;
    let x1 = ((right.max(0.) * dpr).ceil() as u32).min(w).div_ceil(EDGE);
    let y1 = ((bottom.max(0.) * dpr).ceil() as u32).min(h).div_ceil(EDGE);
    anyhow::ensure!(
        u64::from(x1.saturating_sub(x0)) * u64::from(y1.saturating_sub(y0)) <= 128,
        "可见区域超过分块数量预算"
    );
    let mut keys = Vec::new();
    for row in y0..y1 {
        for col in x0..x1 {
            let x = (col * EDGE).saturating_sub(GUTTER);
            let y = (row * EDGE).saturating_sub(GUTTER);
            let right = ((col + 1) * EDGE + GUTTER).min(w);
            let bottom = ((row + 1) * EDGE + GUTTER).min(h);
            keys.push(Key {
                page,
                tile: DeviceTile::new(scale, x, y, right - x, bottom - y)?,
            });
        }
    }
    Ok(keys)
}

impl Breeze {
    pub(super) fn raster_surface(
        &self,
        page: usize,
        geometry: PageGeometry,
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
                        let dpr = window.scale_factor();
                        let visible = [
                            f32::from(clip.left() - bounds.left()),
                            f32::from(clip.top() - bounds.top()),
                            f32::from(clip.right() - bounds.left()),
                            f32::from(clip.bottom() - bounds.top()),
                        ];
                        let keys = if clip.size.width > px(0.) && clip.size.height > px(0.) {
                            tiles(
                                page,
                                geometry.size(),
                                f32::from(bounds.size.width),
                                dpr,
                                visible,
                            )
                        } else {
                            Ok(Vec::new())
                        };
                        view.update(cx, |this, cx| {
                            if generation != this.generation {
                                return (dpr, Vec::new());
                            }
                            let keys = match keys {
                                Ok(keys) => keys,
                                Err(error) => {
                                    this.message = format!("{error:#}");
                                    return (dpr, Vec::new());
                                }
                            };
                            let images = keys
                                .iter()
                                .filter_map(|key| {
                                    this.raster
                                        .get(*key)
                                        .filter(|_| !this.raster.errors.contains_key(key))
                                        .map(|image| (key.tile, image))
                                })
                                .collect();
                            let queued = this.raster.pending.is_some();
                            this.raster
                                .pending
                                .get_or_insert_with(Vec::new)
                                .extend(keys);
                            if !queued {
                                let view = cx.entity();
                                cx.defer(move |cx| {
                                    view.update(cx, |this, cx| {
                                        if generation != this.generation
                                            || this.screen != Screen::Reader
                                        {
                                            return;
                                        }
                                        let Some(wanted) = this.raster.pending.take() else {
                                            return;
                                        };
                                        if this.raster.wanted != wanted {
                                            this.raster.discard_other_scales(&wanted, cx);
                                            this.raster.errors.clear();
                                            this.raster.wanted = wanted;
                                        }
                                        this.pump_visible(cx);
                                    });
                                });
                            }
                            (dpr, images)
                        })
                    },
                    move |bounds, (dpr, images), window, cx| {
                        for (tile, image) in images {
                            let image_bounds = Bounds::new(
                                bounds.origin
                                    + point(px(tile.x as f32 / dpr), px(tile.y as f32 / dpr)),
                                size(px(tile.width as f32 / dpr), px(tile.height as f32 / dpr)),
                            );
                            // Overlap only supplies the texture filter's neighbors; each
                            // pixel is composited once in the non-overlapping tile interior.
                            let left = tile.x.div_ceil(EDGE) * EDGE;
                            let top = tile.y.div_ceil(EDGE) * EDGE;
                            let clip = Bounds::new(
                                bounds.origin + point(px(left as f32 / dpr), px(top as f32 / dpr)),
                                size(px(EDGE as f32 / dpr), px(EDGE as f32 / dpr)),
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
                                // GPU upload failures must not be reported as successful draws.
                                let view = paint_view.clone();
                                let message = format!("页面纹理上传失败：{error:#}");
                                cx.defer(move |cx| {
                                    view.update(cx, |this, cx| {
                                        if generation == this.generation {
                                            this.raster.errors.insert(Key { page, tile }, message);
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

    pub(super) fn pump_raster(&mut self, cx: &mut Context<Self>) -> bool {
        if self.raster.busy {
            return true;
        }
        let Some(key) =
            self.raster.wanted.iter().copied().find(|key| {
                self.raster.get(*key).is_none() && !self.raster.errors.contains_key(key)
            })
        else {
            return false;
        };
        let generation = self.generation;
        let request =
            self.worker
                .request_tile(self.books[self.active].source.clone(), key.page, key.tile);
        let receiver = match request {
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
                if this.raster.wanted.contains(&key) {
                    let result = (|| -> Result<()> {
                        let page = result.map_err(|e| anyhow::anyhow!("页面请求取消：{e}"))??;
                        anyhow::ensure!(
                            page.fingerprint == this.reading.fingerprint,
                            "文档已在外部改变，请重新打开"
                        );
                        anyhow::ensure!(page.tile == Some(key.tile), "页面分块不匹配");
                        let pixels = page.pixels.ok_or_else(|| anyhow::anyhow!("缺少页面像素"))?;
                        anyhow::ensure!(
                            pixels.width == key.tile.width && pixels.height == key.tile.height,
                            "渲染器未返回所需清晰度"
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
