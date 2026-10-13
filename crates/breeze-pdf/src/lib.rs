//! Bounded PDF reading and explicit annotation-copy export; no source-content editing.
pub mod export;
mod raster;
use anyhow::{Context, Result, bail};
use breeze_core::{
    Rotation,
    annotations::{PageGeometry, Rect},
};
use futures::channel::oneshot;
use pdf_oxide::{
    PdfDocument,
    outline::{Destination, OutlineItem},
    rendering::{PageRenderer, RenderOptions},
};
use raster::rotate_region;
use std::{
    collections::VecDeque,
    path::PathBuf,
    sync::atomic::{AtomicBool, Ordering},
    sync::{Arc, Condvar, Mutex},
    thread,
    time::{Instant, SystemTime},
};

/// Output budget for the default-density renderer, which serves the fixed
/// 144 dpi convenience request and the 24 dpi thumbnail path.
const DEFAULT_DENSITY_MAX_PIXELS: u64 = 4_000_000;

/// Output budget for a page rendered at the reader's requested density.
///
/// This is not a quality limit. The engine takes integer DPI and reduces the
/// scale when a request exceeds its budget, so this has to sit above any page the
/// reader may ask for, and a reduced result is refused rather than displayed. It
/// exists only so a pathological page box cannot ask the host for an unbounded
/// allocation: 268 Mpx is 1 GiB of RGBA, above a large-format sheet at 2x display
/// scale and far above an ordinary page.
const MAX_PAGE_PIXELS: u64 = 268_435_456;

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
#[derive(Clone)]
pub struct Pixels {
    pub width: u32,
    pub height: u32,
    pub bgra: Vec<u8>,
}
#[derive(Clone)]
pub struct Page {
    pub fingerprint: String,
    pub geometry: PageGeometry,
    pub glyphs: Vec<Glyph>,
    pub text_warning: Option<String>,
    pub pixels: Option<Pixels>,
    /// Whole-DPI density the raster above was produced at, when one was
    /// requested. The engine takes an integer DPI, so a caller asks in device
    /// pixels per point and is answered in the DPI that was actually used; both
    /// sides compare that integer rather than a derived float.
    pub rendered_dpi: Option<u32>,
    pub page: usize,
    pub total: usize,
    pub points: (f32, f32),
    pub outline: Arc<Vec<Outline>>,
    pub thumbnail: bool,
    pub page_sizes: Arc<Vec<(f32, f32)>>,
    pub elapsed_ms: f64,
}
struct Job {
    source: Source,
    page: usize,
    thumbnail: bool,
    metadata_only: bool,
    /// Device pixels per PDF point to rasterize the whole page at, for a page
    /// request. `None` means the default-density or thumbnail path.
    scale: Option<f32>,
    /// View rotation applied to the rendered pixels after rasterization.
    rotation: Rotation,
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
                let trace = std::env::var_os("BREEZE_TRACE_RENDER").is_some();
                let mut loaded: Option<Loaded> = None;
                // Cached entries are rotation-specific, so the view rotation is part of the key.
                let mut cache: VecDeque<(Rotation, Page)> = VecDeque::new();
                let mut options = RenderOptions::with_dpi(144).as_raw();
                // Only the default-density path uses this renderer; page requests
                // build one per request around the requested density.
                options.max_output_pixels = DEFAULT_DENSITY_MAX_PIXELS;
                let mut renderer = PageRenderer::new(options);
                let mut thumbnail_options = RenderOptions::with_dpi(24).as_raw();
                thumbnail_options.max_output_pixels = 120_000;
                let mut thumbnails = PageRenderer::new(thumbnail_options);
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
                    let result = render(
                        &job,
                        &mut loaded,
                        &mut renderer,
                        &mut thumbnails,
                        &mut cache,
                    );
                    if trace && let Ok(page) = &result {
                        eprintln!(
                            "breeze-render page={} metadata={} scale={:?} elapsed_ms={:.2} bytes={}",
                            page.page,
                            job.metadata_only,
                            job.scale,
                            page.elapsed_ms,
                            page_bytes(page)
                        );
                    }
                    // Receiver may have been dropped after navigation or window close.
                    let _ = job.reply.send(result);
                }
            })?;
        Ok(Self { queue })
    }
    pub fn request(&self, source: Source, page: usize) -> Result<oneshot::Receiver<Result<Page>>> {
        self.request_kind(source, page, false, false, None, Rotation::None)
    }
    pub fn request_thumbnail(
        &self,
        source: Source,
        page: usize,
        rotation: Rotation,
    ) -> Result<oneshot::Receiver<Result<Page>>> {
        self.request_kind(source, page, true, false, None, rotation)
    }
    pub fn request_metadata(
        &self,
        source: Source,
        page: usize,
    ) -> Result<oneshot::Receiver<Result<Page>>> {
        self.request_kind(source, page, false, true, None, Rotation::None)
    }
    /// Rasterize the whole page at `scale` device pixels per PDF point, rotated
    /// for the current view.
    ///
    /// The engine exposes an integer DPI only, so the request rounds up: the
    /// result carries at or slightly above the requested density, never below.
    /// A result the engine quietly reduced to fit its own budget is refused
    /// instead of returned, because a caller caches by the density it asked for.
    pub fn request_page(
        &self,
        source: Source,
        page: usize,
        rotation: Rotation,
        scale: f32,
    ) -> Result<oneshot::Receiver<Result<Page>>> {
        self.request_kind(source, page, false, false, Some(scale), rotation)
    }
    fn request_kind(
        &self,
        source: Source,
        page: usize,
        thumbnail: bool,
        metadata_only: bool,
        scale: Option<f32>,
        rotation: Rotation,
    ) -> Result<oneshot::Receiver<Result<Page>>> {
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
            thumbnail,
            metadata_only,
            scale,
            rotation,
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
#[derive(Clone, Debug)]
pub struct Glyph {
    pub character: char,
    pub bounds: Rect,
}
struct Loaded {
    source: Source,
    doc: PdfDocument,
    fingerprint: String,
    stamp: Option<(u64, SystemTime)>,
    geometries: Vec<PageGeometry>,
    sizes: Arc<Vec<(f32, f32)>>,
    outline: Arc<Vec<Outline>>,
}
fn read_source(source: &Source) -> Result<Vec<u8>> {
    match source {
        Source::Sample => Ok(include_bytes!("../../../assets/samples/field-notes.pdf").to_vec()),
        Source::File(path) => {
            use std::io::Read;
            if !path
                .extension()
                .is_some_and(|s| s.eq_ignore_ascii_case("pdf"))
            {
                bail!("当前仅支持 PDF；其他格式尚未接入");
            }
            let file = std::fs::File::open(path)
                .with_context(|| format!("无法打开 {}", path.display()))?;
            let mut data = Vec::new();
            file.take(256 * 1024 * 1024 + 1).read_to_end(&mut data)?;
            anyhow::ensure!(
                data.len() <= 256 * 1024 * 1024,
                "当前版本限制 PDF 文件为 256 MiB"
            );
            Ok(data)
        }
    }
}
fn render(
    job: &Job,
    loaded: &mut Option<Loaded>,
    renderer: &mut PageRenderer,
    thumb: &mut PageRenderer,
    cache: &mut VecDeque<(Rotation, Page)>,
) -> Result<Page> {
    let time = Instant::now();
    let stamp = match &job.source {
        Source::Sample => None,
        Source::File(path) => {
            let m = std::fs::metadata(path)?;
            Some((m.len(), m.modified()?))
        }
    };
    if loaded
        .as_ref()
        .is_none_or(|l| l.source != job.source || l.stamp != stamp)
    {
        let bytes = read_source(&job.source)?;
        let fingerprint = breeze_storage::bytes_fingerprint(&bytes);
        let doc = PdfDocument::from_bytes(bytes)?;
        if doc.is_encrypted() {
            bail!("当前不支持加密/密码 PDF");
        }
        let total = doc.page_count()?;
        anyhow::ensure!(total > 0 && total <= 20_000, "当前支持 1–20000 页文档");
        let geometries = (0..total)
            .map(|page| page_geometry(&doc, page))
            .collect::<Result<Vec<_>>>()?;
        let sizes = Arc::new(geometries.iter().map(|g| g.size()).collect());
        let mut outline = Vec::new();
        if let Some(items) = doc.get_outline()? {
            flatten(&items, 0, &mut outline);
        }
        *loaded = Some(Loaded {
            source: job.source.clone(),
            geometries,
            sizes,
            outline: Arc::new(outline),
            doc,
            fingerprint,
            stamp,
        });
        cache.clear();
    }
    let loaded = loaded.as_ref().context("PDF not loaded")?;
    let doc = &loaded.doc;
    let total = doc.page_count()?;
    if total == 0 {
        bail!("文档没有可阅读的页面");
    }
    let page = job.page.min(total - 1);
    if let Some(index) = cache.iter().position(|(rotation, p)| {
        job.scale.is_none()
            && *rotation == job.rotation
            && p.page == page
            && p.thumbnail == job.thumbnail
            && p.pixels.is_none() == job.metadata_only
    }) {
        let (_, mut hit) = cache.remove(index).context("Missing cache entry")?;
        hit.elapsed_ms = time.elapsed().as_secs_f64() * 1000.;
        cache.push_back((job.rotation, hit.clone()));
        return Ok(hit);
    }
    let geometry = loaded.geometries[page];
    let points = geometry.size();
    let (pixels, rendered_dpi) = if job.metadata_only {
        (None, None)
    } else if let Some(scale) = job.scale {
        // Whole page at the density the reader's surface actually needs. The
        // engine exposes an integer DPI, so round up: a raster at or just above
        // the requested density is sharp, and painting scales it back down by the
        // same fraction instead of stretching it.
        let dpi = (f64::from(scale) * 72.).ceil();
        anyhow::ensure!(
            dpi.is_finite() && (1. ..=4_608.).contains(&dpi),
            "不支持的页面缩放"
        );
        let mut options = RenderOptions::with_dpi(dpi as u32).as_raw();
        options.max_output_pixels = MAX_PAGE_PIXELS;
        let mut page_renderer = PageRenderer::new(options);
        let rendered = page_renderer
            .render_page(doc, page)
            .with_context(|| format!("第 {} 页渲染失败，无法完整显示", page + 1))?;
        let rendered_scale = dpi as f32 / 72.;
        let image = convert(rendered);
        // The engine reduces the scale to fit its own output budget instead of
        // failing. A reduced raster is indistinguishable from a sharp one until
        // it is painted, so refuse it here rather than present a blurry page as
        // the density that was requested.
        // `points` is the page's own size with its /Rotate applied, which is what
        // the engine renders; the viewer's rotation is applied to the raster
        // afterwards. One pixel of slack absorbs the engine's own rounding of a
        // fractional extent, while a reduced scale moves the extent far more.
        let expected = (
            (f64::from(points.0) * f64::from(rendered_scale)).ceil() as u32,
            (f64::from(points.1) * f64::from(rendered_scale)).ceil() as u32,
        );
        anyhow::ensure!(
            image.width.abs_diff(expected.0) <= 1 && image.height.abs_diff(expected.1) <= 1,
            "渲染器未按请求的清晰度返回页面（需要 {}x{}，得到 {}x{}）",
            expected.0,
            expected.1,
            image.width,
            image.height
        );
        (Some(rotate_region(image, job.rotation)), Some(dpi as u32))
    } else {
        let rendered = if job.thumbnail {
            thumb.render_page(doc, page)
        } else {
            renderer.render_page(doc, page)
        }
        .with_context(|| format!("第 {} 页渲染失败，无法完整显示", page + 1))?;
        let density = if job.thumbnail { 24 } else { 144 };
        (
            Some(rotate_region(convert(rendered), job.rotation)),
            Some(density),
        )
    };
    let full_page_request = job.thumbnail || job.scale.is_some();
    let outline = if full_page_request {
        Arc::new(Vec::new())
    } else {
        loaded.outline.clone()
    };
    let (glyphs, text_warning) = if full_page_request {
        (Vec::new(), None)
    } else {
        match doc.extract_chars(page) {
            Ok(chars) => {
                if chars.len() > 50_000 {
                    (Vec::new(), Some("当前页字符数量超出文本选择预算".into()))
                } else {
                    let glyphs: Vec<_> = chars
                        .into_iter()
                        .filter(|c| c.rotation_degrees.abs() < 0.01)
                        .map(|c| Glyph {
                            character: c.char,
                            bounds: Rect {
                                x: c.bbox.x,
                                y: c.bbox.y,
                                width: c.bbox.width,
                                height: c.bbox.height,
                            },
                        })
                        .filter(|g| g.bounds.valid())
                        .collect();
                    (glyphs, None)
                }
            }
            Err(e) => (Vec::new(), Some(format!("文字提取失败：{e}"))),
        }
    };
    let result = Page {
        pixels,
        rendered_dpi,
        page,
        total,
        points,
        geometry,
        glyphs,
        text_warning,
        fingerprint: loaded.fingerprint.clone(),
        outline,
        thumbnail: job.thumbnail,
        page_sizes: loaded.sizes.clone(),
        elapsed_ms: time.elapsed().as_secs_f64() * 1000.,
    };
    // The viewer owns page-raster residency: a whole-page texture is far larger
    // than this worker cache, and the viewer already keeps the density it needs
    // for the visible pages.
    if job.scale.is_some() {
        return Ok(result);
    }
    let cost = page_bytes(&result);
    while cache
        .iter()
        .map(|(_, page)| page_bytes(page))
        .sum::<usize>()
        + cost
        > 32 * 1024 * 1024
    {
        if cache.pop_front().is_none() {
            break;
        }
    }
    if cost <= 32 * 1024 * 1024 {
        cache.push_back((job.rotation, result.clone()));
    }
    Ok(result)
}
fn page_geometry(doc: &PdfDocument, page: usize) -> Result<PageGeometry> {
    let info = doc.get_page_info(page)?;
    let bounds = if let Some(crop) = info.crop_box {
        let x = crop.x.max(info.media_box.x);
        let y = crop.y.max(info.media_box.y);
        let r = (crop.x + crop.width).min(info.media_box.x + info.media_box.width);
        let t = (crop.y + crop.height).min(info.media_box.y + info.media_box.height);
        Rect {
            x,
            y,
            width: r - x,
            height: t - y,
        }
    } else {
        Rect {
            x: info.media_box.x,
            y: info.media_box.y,
            width: info.media_box.width,
            height: info.media_box.height,
        }
    };
    anyhow::ensure!(
        bounds.valid() && bounds.width > 0. && bounds.height > 0.,
        "页面尺寸无效"
    );
    Ok(PageGeometry {
        crop: bounds,
        rotation: info.rotation,
    })
}
fn page_bytes(page: &Page) -> usize {
    page.pixels.as_ref().map_or(0, |p| p.bgra.len())
        + page.glyphs.len() * std::mem::size_of::<Glyph>()
        + page
            .outline
            .iter()
            .map(|o| o.title.len() + std::mem::size_of::<Outline>())
            .sum::<usize>()
}
#[derive(Clone, Debug)]
pub struct SearchHit {
    pub page: usize,
    pub snippet: String,
}
pub fn search(source: Source, query: String, cancel: Arc<AtomicBool>) -> Result<Vec<SearchHit>> {
    anyhow::ensure!(
        !query.trim().is_empty() && query.len() <= 256,
        "请输入 1–256 字节的搜索词"
    );
    let doc = PdfDocument::from_bytes(read_source(&source)?)?;
    if doc.is_encrypted() {
        bail!("不支持加密文档搜索");
    }
    let needle = query.to_lowercase();
    let mut hits = Vec::new();
    for page in 0..doc.page_count()? {
        if cancel.load(Ordering::Relaxed) {
            bail!("搜索已取消");
        }
        let text = doc.extract_text(page)?;
        for line in text.lines() {
            if line.to_lowercase().contains(&needle) {
                hits.push(SearchHit {
                    page,
                    snippet: line.chars().take(160).collect(),
                });
                if hits.len() >= 300 {
                    return Ok(hits);
                }
            }
        }
    }
    Ok(hits)
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
            result.pixels.as_ref().unwrap().bgra.len(),
            (result.pixels.as_ref().unwrap().width * result.pixels.as_ref().unwrap().height * 4)
                as usize
        );
        assert_eq!(result.outline[0].page, Some(0));
        assert_eq!(result.page_sizes.len(), result.total);
        assert!(!result.thumbnail);
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
                thumbnail: false,
                metadata_only: false,
                scale: None,
                rotation: Rotation::None,
                reply: first,
            }),
            stop: false,
        };
        pending.job = Some(Job {
            source: Source::Sample,
            page: 7,
            thumbnail: false,
            metadata_only: false,
            scale: None,
            rotation: Rotation::None,
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

#[cfg(test)]
mod reader_tests {
    use super::*;
    #[test]
    fn search_returns_real_page_destinations() {
        let result = search(
            Source::Sample,
            "quiet art of focus".into(),
            Arc::new(AtomicBool::new(false)),
        )
        .unwrap();
        assert_eq!(result[0].page, 0);
        assert!(
            result[0]
                .snippet
                .to_lowercase()
                .contains("quiet art of focus")
        );
    }
    #[test]
    fn search_can_be_cancelled() {
        assert!(
            search(
                Source::Sample,
                "focus".into(),
                Arc::new(AtomicBool::new(true))
            )
            .is_err()
        );
    }
    #[test]
    fn text_bounds_and_cache_are_page_local() {
        let worker = PdfWorker::new().unwrap();
        let first = futures::executor::block_on(worker.request(Source::Sample, 0).unwrap())
            .unwrap()
            .unwrap();
        assert!(!first.glyphs.is_empty());
        assert!(first.glyphs.iter().all(|g| g.bounds.valid()));
        let second = futures::executor::block_on(worker.request(Source::Sample, 0).unwrap())
            .unwrap()
            .unwrap();
        assert_eq!(first.fingerprint, second.fingerprint);
        assert_eq!(
            first.pixels.as_ref().unwrap().bgra,
            second.pixels.as_ref().unwrap().bgra
        );
        assert!(page_bytes(&second) <= 32 * 1024 * 1024);
    }
}

#[cfg(test)]
mod performance_probe {
    use super::*;
    #[test]
    #[ignore = "Run explicitly in Release; synthetic sample only, not a fidelity or release gate"]
    fn report_sample_render_and_cache_cost() {
        let worker = PdfWorker::new().unwrap();
        let started = Instant::now();
        let first = futures::executor::block_on(worker.request(Source::Sample, 0).unwrap())
            .unwrap()
            .unwrap();
        let first_ms = started.elapsed().as_secs_f64() * 1000.;
        let mut samples = Vec::new();
        for _ in 0..20 {
            let start = Instant::now();
            let page = futures::executor::block_on(worker.request(Source::Sample, 0).unwrap())
                .unwrap()
                .unwrap();
            assert_eq!(page.fingerprint, first.fingerprint);
            samples.push(start.elapsed().as_secs_f64() * 1000.);
        }
        samples.sort_by(f64::total_cmp);
        println!(
            "profile={} sample=field-notes pages={} dpi=144 output_bytes={} first_ms={first_ms:.2} cache_roundtrip_p50_ms={:.2} cache_roundtrip_p95_ms={:.2} n=20 excludes_gpu=true",
            if cfg!(debug_assertions) {
                "debug"
            } else {
                "release"
            },
            first.total,
            page_bytes(&first),
            samples[9],
            samples[18]
        );
    }
}

#[cfg(test)]
mod visible_request_tests {
    use super::*;
    #[test]
    fn thumbnails_reach_last_page_without_full_page_or_glyph_work() {
        let worker = PdfWorker::new().unwrap();
        let thumbnail = futures::executor::block_on(
            worker
                .request_thumbnail(Source::Sample, 7, Rotation::None)
                .unwrap(),
        )
        .unwrap()
        .unwrap();
        assert_eq!(thumbnail.page, 7);
        assert!(thumbnail.thumbnail);
        assert!(thumbnail.glyphs.is_empty());
        assert!(thumbnail.outline.is_empty());
        assert!(
            thumbnail.pixels.as_ref().unwrap().width * thumbnail.pixels.as_ref().unwrap().height
                <= 120_000
        );
        assert_eq!(thumbnail.page_sizes.len(), 8);
        let full = futures::executor::block_on(worker.request(Source::Sample, 7).unwrap())
            .unwrap()
            .unwrap();
        assert!(!full.thumbnail);
        assert!(!full.glyphs.is_empty());
        assert!(full.pixels.as_ref().unwrap().width > thumbnail.pixels.as_ref().unwrap().width);
        assert_eq!(full.fingerprint, thumbnail.fingerprint);
        assert!(Arc::ptr_eq(&full.page_sizes, &thumbnail.page_sizes));
    }
    #[test]
    #[ignore = "Run in Release; reports worker latency and accounted result bytes, not total RSS or GPU"]
    fn report_visible_request_costs() {
        for thumbnail in [false, true] {
            let mut cold = Vec::new();
            let mut warm = Vec::new();
            let mut dimensions = (0, 0);
            let mut bytes = 0;
            for sample in 0..20 {
                let worker = PdfWorker::new().unwrap();
                for samples in [&mut cold, &mut warm] {
                    let start = Instant::now();
                    let receiver = if thumbnail {
                        worker.request_thumbnail(Source::Sample, sample % 8, Rotation::None)
                    } else {
                        worker.request(Source::Sample, sample % 8)
                    }
                    .unwrap();
                    let result = futures::executor::block_on(receiver).unwrap().unwrap();
                    samples.push(start.elapsed().as_secs_f64() * 1000.);
                    dimensions = (
                        result.pixels.as_ref().unwrap().width,
                        result.pixels.as_ref().unwrap().height,
                    );
                    bytes = page_bytes(&result);
                }
            }
            cold.sort_by(f64::total_cmp);
            warm.sort_by(f64::total_cmp);
            println!(
                "profile={} thumbnail={thumbnail} n=20 document_cold_p50_ms={:.2} document_cold_p95_ms={:.2} warm_p50_ms={:.2} warm_p95_ms={:.2} dimensions={dimensions:?} last_result_bytes={bytes} excludes_gpu_and_engine_allocations=true",
                if cfg!(debug_assertions) {
                    "debug"
                } else {
                    "release"
                },
                cold[9],
                cold[18],
                warm[9],
                warm[18]
            );
        }
    }
}
