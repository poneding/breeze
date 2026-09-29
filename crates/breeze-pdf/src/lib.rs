//! A bounded, single-worker read-only PDF adapter. No editing API is exposed.
use anyhow::{Context, Result, bail};
use futures::channel::oneshot;
use pdf_oxide::{
    PdfDocument,
    outline::{Destination, OutlineItem},
    rendering::{PageRenderer, RenderOptions},
};
use std::{
    path::PathBuf,
    sync::{Arc, Condvar, Mutex},
    thread,
    time::Instant,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Source {
    Sample,
    File(PathBuf),
}
#[derive(Clone, Debug)]
pub struct Outline {
    pub title: String,
    pub page: Option<usize>,
    pub depth: usize,
}
pub struct Pixels {
    pub width: u32,
    pub height: u32,
    pub bgra: Vec<u8>,
}
pub struct Page {
    pub pixels: Pixels,
    pub page: usize,
    pub total: usize,
    pub points: (f32, f32),
    pub outline: Vec<Outline>,
    pub thumbnails: Vec<(usize, Pixels)>,
    pub elapsed_ms: f64,
}
struct Job {
    source: Source,
    page: usize,
    reply: oneshot::Sender<Result<Page>>,
}
#[derive(Default)]
struct Pending {
    job: Option<Job>,
    stop: bool,
}
type Queue = Arc<(Mutex<Pending>, Condvar)>;
pub struct PdfWorker {
    queue: Queue,
}
impl PdfWorker {
    pub fn new() -> Result<Self> {
        let queue: Queue = Arc::new((Mutex::new(Pending::default()), Condvar::new()));
        let q = queue.clone();
        thread::Builder::new()
            .name("breeze-pdf".into())
            .spawn(move || {
                let mut loaded: Option<(Source, PdfDocument)> = None;
                let mut options = RenderOptions::with_dpi(144).as_raw();
                options.max_output_pixels = 4_000_000;
                let mut renderer = PageRenderer::new(options);
                let mut thumbnails = PageRenderer::new(RenderOptions::with_dpi(24).as_raw());
                loop {
                    let (lock, wake) = &*q;
                    let Ok(mut pending) = lock.lock() else { return };
                    while pending.job.is_none() && !pending.stop {
                        pending = match wake.wait(pending) {
                            Ok(p) => p,
                            Err(_) => return,
                        };
                    }
                    if pending.stop {
                        return;
                    }
                    let Some(job) = pending.job.take() else {
                        continue;
                    };
                    drop(pending);
                    let result = render(&job, &mut loaded, &mut renderer, &mut thumbnails);
                    // Receiver may have been dropped after navigation or window close.
                    let _ = job.reply.send(result);
                }
            })?;
        Ok(Self { queue })
    }
    pub fn request(&self, source: Source, page: usize) -> Result<oneshot::Receiver<Result<Page>>> {
        let (reply, receive) = oneshot::channel();
        let mut pending = self
            .queue
            .0
            .lock()
            .map_err(|_| anyhow::anyhow!("PDF worker unavailable"))?;
        // Replacing the sole pending job cancels its receiver; there is no unbounded queue.
        pending.job = Some(Job {
            source,
            page,
            reply,
        });
        self.queue.1.notify_one();
        Ok(receive)
    }
}
impl Drop for PdfWorker {
    fn drop(&mut self) {
        if let Ok(mut p) = self.queue.0.lock() {
            p.stop = true;
            p.job = None;
            self.queue.1.notify_one();
        }
    }
}
fn render(
    job: &Job,
    loaded: &mut Option<(Source, PdfDocument)>,
    renderer: &mut PageRenderer,
    thumb: &mut PageRenderer,
) -> Result<Page> {
    let time = Instant::now();
    if loaded.as_ref().is_none_or(|(s, _)| s != &job.source) {
        let doc = match &job.source {
            Source::Sample => PdfDocument::from_bytes(
                include_bytes!("../../../assets/samples/field-notes.pdf").to_vec(),
            )?,
            Source::File(path) => {
                if !path
                    .extension()
                    .is_some_and(|s| s.eq_ignore_ascii_case("pdf"))
                {
                    bail!("当前原型仅接入 PDF；其他格式尚未接入")
                }
                PdfDocument::open(path).with_context(|| format!("无法打开 {}", path.display()))?
            }
        };
        if doc.is_encrypted() {
            bail!("当前阶段不支持加密/密码 PDF")
        }
        *loaded = Some((job.source.clone(), doc));
    }
    let (_, doc) = loaded.as_ref().context("PDF not loaded")?;
    let total = doc.page_count()?;
    if total == 0 {
        bail!("文档没有可阅读的页面")
    }
    let page = job.page.min(total - 1);
    let info = doc.get_page_info(page)?;
    let bounds = if let Some(crop) = info.crop_box {
        let x = crop.x.max(info.media_box.x);
        let y = crop.y.max(info.media_box.y);
        let right = (crop.x + crop.width).min(info.media_box.x + info.media_box.width);
        let top = (crop.y + crop.height).min(info.media_box.y + info.media_box.height);
        pdf_oxide::geometry::Rect::new(x, y, right - x, top - y)
    } else {
        info.media_box
    };
    if !bounds.width.is_finite()
        || !bounds.height.is_finite()
        || bounds.width <= 0.0
        || bounds.height <= 0.0
    {
        bail!("页面尺寸无效，无法安全显示")
    }
    let points = if info.rotation.rem_euclid(180) == 90 {
        (bounds.height, bounds.width)
    } else {
        (bounds.width, bounds.height)
    };
    let pixels = convert(renderer.render_page(doc, page)?);
    let mut outline = Vec::new();
    if let Some(items) = doc.get_outline()? {
        flatten(&items, 0, &mut outline);
    }
    // Only a small visible neighborhood: never allocate one thumbnail per PDF page.
    let start = page.saturating_sub(1);
    let end = (start + 4).min(total);
    let mut thumbnails = Vec::new();
    for index in start..end {
        thumbnails.push((index, convert(thumb.render_page(doc, index)?)));
    }
    Ok(Page {
        pixels,
        page,
        total,
        points,
        outline,
        thumbnails,
        elapsed_ms: time.elapsed().as_secs_f64() * 1000.,
    })
}
fn flatten(items: &[OutlineItem], depth: usize, out: &mut Vec<Outline>) {
    for item in items {
        out.push(Outline {
            title: item.title.clone(),
            page: match item.dest {
                Some(Destination::PageIndex(n)) => Some(n),
                _ => None,
            },
            depth,
        });
        flatten(&item.children, depth + 1, out);
    }
}
fn convert(image: pdf_oxide::rendering::RenderedImage) -> Pixels {
    let mut bgra = image.data;
    for pixel in bgra.chunks_exact_mut(4) {
        pixel.swap(0, 2);
    }
    Pixels {
        width: image.width,
        height: image.height,
        bgra,
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn real_sample_renders_off_thread() {
        let worker = PdfWorker::new().unwrap();
        let result = futures::executor::block_on(worker.request(Source::Sample, 0).unwrap())
            .unwrap()
            .unwrap();
        assert_eq!(result.total, 8);
        assert_eq!(
            result.pixels.bgra.len(),
            (result.pixels.width * result.pixels.height * 4) as usize
        );
        assert_eq!(result.outline[0].page, Some(0));
        assert!(result.thumbnails.len() <= 4);
    }
    #[test]
    fn wrong_extension_is_explicit() {
        let worker = PdfWorker::new().unwrap();
        let result =
            futures::executor::block_on(worker.request(Source::File("x.epub".into()), 0).unwrap())
                .unwrap();
        assert!(result.is_err());
    }
}

#[cfg(test)]
mod scheduling_tests {
    use super::*;
    #[test]
    fn replacing_pending_job_cancels_old_request() {
        let (first, mut old) = oneshot::channel();
        let (second, _) = oneshot::channel();
        let mut pending = Pending {
            job: Some(Job {
                source: Source::Sample,
                page: 0,
                reply: first,
            }),
            stop: false,
        };
        pending.job = Some(Job {
            source: Source::Sample,
            page: 7,
            reply: second,
        });
        assert!(old.try_recv().is_err());
        assert_eq!(pending.job.as_ref().unwrap().page, 7);
    }
    #[test]
    fn pixel_conversion_preserves_alpha_and_swaps_channels() {
        let pixels = convert(pdf_oxide::rendering::RenderedImage {
            data: vec![10, 20, 30, 255],
            width: 1,
            height: 1,
            format: pdf_oxide::rendering::ImageFormat::RawRgba8,
        });
        assert_eq!(pixels.bgra, vec![30, 20, 10, 255]);
    }
}
