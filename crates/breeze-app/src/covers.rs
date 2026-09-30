//! First-page previews share the bounded PDF worker and never open reader tabs.
use crate::*;
use std::collections::VecDeque;

const COVER_BUDGET: usize = 16 * 1024 * 1024;
const COVER_COUNT: usize = 64;
struct Cover {
    source: Source,
    image: Result<Arc<RenderImage>, String>,
    bytes: usize,
}
#[derive(Default)]
pub(super) struct LibraryCovers {
    entries: VecDeque<Cover>,
    wanted: Vec<Source>,
    pending: Option<Vec<Source>>,
    task: Option<Task<()>>,
    busy: bool,
    epoch: u64,
}
impl LibraryCovers {
    pub fn cancel(&mut self) {
        self.epoch += 1;
        self.task = None;
        self.busy = false;
        self.pending = None;
        self.wanted.clear();
    }
    pub fn image(&self, source: &Source) -> Option<&Result<Arc<RenderImage>, String>> {
        self.entries
            .iter()
            .find(|e| &e.source == source)
            .map(|e| &e.image)
    }
    pub fn clear(&mut self, cx: &mut App) {
        self.cancel();
        for entry in self.entries.drain(..) {
            if let Ok(image) = entry.image {
                cx.defer(move |cx| cx.drop_image(image, None));
            }
        }
    }
    fn insert(&mut self, cover: Cover, cx: &mut App) {
        while self.entries.len() >= COVER_COUNT
            || self.entries.iter().map(|e| e.bytes).sum::<usize>() + cover.bytes > COVER_BUDGET
        {
            if let Some(entry) = self.entries.pop_front() {
                if let Ok(image) = entry.image {
                    cx.defer(move |cx| cx.drop_image(image, None));
                }
            } else {
                return;
            }
        }
        self.entries.push_back(cover);
    }
}
impl Breeze {
    pub(super) fn defer_covers(&mut self, sources: Vec<Source>, cx: &mut Context<Self>) {
        if self.screen != Screen::Home {
            return;
        }
        // One thumbnail is bounded to 120,000 BGRA pixels in the PDF worker.
        // Thirty-two visible covers therefore fit within the 16 MiB image budget.
        let sources: Vec<_> = sources.into_iter().take(32).collect();
        if self.covers.pending.is_none() && self.covers.wanted == sources {
            self.pump_covers(cx);
            return;
        }
        let queued = self.covers.pending.is_some();
        self.covers.pending = Some(sources);
        if queued {
            return;
        }
        let epoch = self.covers.epoch;
        let view = cx.entity();
        cx.defer(move |cx| {
            view.update(cx, |this, cx| {
                if this.screen == Screen::Home
                    && this.covers.epoch == epoch
                    && let Some(sources) = this.covers.pending.take()
                {
                    // Cap work as well as memory for unusually large native windows.
                    this.covers.wanted = sources;
                    this.pump_covers(cx);
                }
            })
        });
    }
    fn pump_covers(&mut self, cx: &mut Context<Self>) {
        if self.screen != Screen::Home || self.covers.busy {
            return;
        }
        let Some(source) = self
            .covers
            .wanted
            .iter()
            .find(|source| self.covers.image(source).is_none())
            .cloned()
        else {
            return;
        };
        let request = match self.worker.request_thumbnail(source.clone(), 0) {
            Ok(request) => request,
            Err(error) => {
                self.covers.insert(
                    Cover {
                        source,
                        image: Err(format!("封面不可用：{error:#}")),
                        bytes: 0,
                    },
                    cx,
                );
                cx.notify();
                return;
            }
        };
        let epoch = self.covers.epoch;
        self.covers.busy = true;
        self.covers.task = Some(cx.spawn(async move |this, cx| {
            let result = request.await;
            let _ = this.update(cx, |this, cx| {
                if this.screen != Screen::Home || this.covers.epoch != epoch {
                    return;
                }
                this.covers.busy = false;
                if this.covers.wanted.contains(&source) {
                    let result = result
                        .map_err(anyhow::Error::from)
                        .and_then(|page| page)
                        .and_then(|page| {
                            page.pixels.ok_or_else(|| anyhow::anyhow!("封面像素不可用"))
                        })
                        .and_then(|pixels| {
                            let bytes = pixels.bgra.len();
                            Ok((texture(pixels)?, bytes))
                        });
                    let (image, bytes) = match result {
                        Ok((image, bytes)) => (Ok(image), bytes),
                        Err(error) => (Err(format!("封面不可用：{error:#}")), 0),
                    };
                    this.covers.insert(
                        Cover {
                            source,
                            image,
                            bytes,
                        },
                        cx,
                    );
                }
                this.pump_covers(cx);
                cx.notify();
            });
        }));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui_kit::test::TestWindowExt;
    #[gpui_kit::test]
    async fn visible_covers_complete_without_opening_documents(cx: &mut TestAppContext) {
        use gpui_kit::test::TestAppContextExt;
        let (handle, view) = crate::ui_tests::app(cx);
        let before = view.read_with(cx, |view, _| view.snapshot().books);
        cx.update_window(handle, |_, window, cx| {
            view.update(cx, |view, cx| view.show_home(cx));
            window.render_frame(cx);
        })
        .unwrap();
        cx.wait_for(handle, std::time::Duration::from_secs(10), |_, cx| {
            view.read(cx).covers.image(&Source::Sample).is_some()
        })
        .await;
        view.read_with(cx, |view, _| {
            assert!(view.covers.image(&Source::Sample).unwrap().is_ok());
            assert!(view.screen == Screen::Home);
            assert_eq!(view.snapshot().books, before);
        });
    }
    #[gpui_kit::test]
    fn library_cover_and_grid_preserve_reader_state(cx: &mut TestAppContext) {
        let worker = PdfWorker::new().unwrap();
        let page =
            futures::executor::block_on(worker.request_thumbnail(Source::Sample, 0).unwrap())
                .unwrap()
                .unwrap();
        let pixels = page.pixels.unwrap();
        let bytes = pixels.bgra.len();
        let image = texture(pixels).unwrap();
        let (handle, view) = crate::ui_tests::app(cx);
        cx.update_window(handle, |_, window, cx| {
            view.update(cx, |view, cx| {
                view.show_home(cx);
                view.covers.insert(
                    Cover {
                        source: Source::Sample,
                        image: Ok(image),
                        bytes,
                    },
                    cx,
                );
            });
            let before = view.read(cx).snapshot().books;
            window.render_frame(cx);
            assert!(view.read(cx).covers.image(&Source::Sample).unwrap().is_ok());
            assert_eq!(view.read(cx).snapshot().books, before);
            window.click("library-grid", cx);
            window.render_frame(cx);
            assert!(view.read(cx).library_grid);
            assert!(window.find(("library-open", 0usize)).bounds().size.height > px(200.));
            window.hover(("library-open", 0usize), cx);
            window.click(("favorite", 0usize), cx);
            assert!(view.read(cx).screen == Screen::Home);
            view.update(cx, |view, cx| {
                let saved = view.snapshot();
                view.library_grid = false;
                view.restore(saved, cx);
                assert!(view.library_grid);
                view.defer_covers(vec![Source::File("never-read.pdf".into())], cx);
                view.covers.cancel();
                view.screen = Screen::Settings;
            });
        })
        .unwrap();
        cx.update_window(handle, |_, _, cx| {
            assert!(view.read(cx).covers.pending.is_none());
            assert!(!view.read(cx).covers.busy);
        })
        .unwrap();
    }
    #[gpui_kit::test]
    fn cover_cache_bounds_bytes_and_count(cx: &mut TestAppContext) {
        cx.update(|cx| {
            let mut cache = LibraryCovers::default();
            for index in 0..100 {
                cache.insert(
                    Cover {
                        source: Source::File(format!("{index}.pdf").into()),
                        image: Err("test".into()),
                        bytes: 480_000,
                    },
                    cx,
                );
            }
            assert!(cache.entries.iter().map(|e| e.bytes).sum::<usize>() <= COVER_BUDGET);
            for index in 100..200 {
                cache.insert(
                    Cover {
                        source: Source::File(format!("{index}.pdf").into()),
                        image: Err("test".into()),
                        bytes: 0,
                    },
                    cx,
                );
            }
            assert!(cache.entries.len() <= COVER_COUNT);
            cache.clear(cx);
            assert!(cache.entries.is_empty());
        });
    }
}
