//! Bounded PDF reading and explicit annotation-copy export; no source-content editing.
pub mod export;
mod integrity;
mod raster;
use anyhow::{Context, Result, bail};
use breeze_core::{
    Rotation,
    annotations::{PageGeometry, Rect},
};
use futures::channel::oneshot;
use mupdf::{
    Colorspace, IRect, Matrix, Outline as MuPdfOutline, Pixmap, TextPageFlags,
    cookie::Cookie,
    device::Device,
    pdf::{PdfDocument, PdfObject, PdfPage},
};
use raster::rotate_region;
use std::{
    cell::RefCell,
    collections::VecDeque,
    path::PathBuf,
    sync::atomic::{AtomicBool, Ordering},
    sync::{Arc, Condvar, Mutex},
    thread,
    time::{Instant, SystemTime},
};

/// Output budget for a page rendered at the reader's requested density.
///
/// This is not a quality limit. The engine takes integer DPI and reduces the
/// scale when a request exceeds its budget, so this has to sit above any page the
/// reader may ask for, and a reduced result is refused rather than displayed. It
/// exists only so a pathological page box cannot ask the host for an unbounded
/// allocation: 268 Mpx is 1 GiB of RGBA, above a large-format sheet at 2x display
/// scale and far above an ordinary page.
const MAX_PAGE_PIXELS: u64 = 268_435_456;
const MAX_REGION_PIXELS: u64 = 12_000_000;

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
    /// View-space page rectangle covered by `pixels` when this is a clipped raster.
    pub region: Option<Rect>,
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
    region: Option<Rect>,
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
                    let result = render(&job, &mut loaded, &mut cache);
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
        self.request_kind(source, page, false, false, None, None, Rotation::None)
    }
    pub fn request_thumbnail(
        &self,
        source: Source,
        page: usize,
        rotation: Rotation,
    ) -> Result<oneshot::Receiver<Result<Page>>> {
        self.request_kind(source, page, true, false, None, None, rotation)
    }
    pub fn request_metadata(
        &self,
        source: Source,
        page: usize,
    ) -> Result<oneshot::Receiver<Result<Page>>> {
        self.request_kind(source, page, false, true, None, None, Rotation::None)
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
        self.request_kind(source, page, false, false, Some(scale), None, rotation)
    }

    /// Replay a clipped view-space page rectangle at the requested device scale.
    pub fn request_region(
        &self,
        source: Source,
        page: usize,
        rotation: Rotation,
        scale: f32,
        region: Rect,
    ) -> Result<oneshot::Receiver<Result<Page>>> {
        anyhow::ensure!(
            scale.is_finite()
                && (0.01..=64.).contains(&scale)
                && region.valid()
                && region.width > 0.
                && region.height > 0.,
            "页面区域或缩放无效"
        );
        self.request_kind(
            source,
            page,
            false,
            false,
            Some(scale),
            Some(region),
            rotation,
        )
    }
    #[allow(clippy::too_many_arguments)]
    fn request_kind(
        &self,
        source: Source,
        page: usize,
        thumbnail: bool,
        metadata_only: bool,
        scale: Option<f32>,
        region: Option<Rect>,
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
            region,
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
    stamp: Option<SourceStamp>,
    geometries: Vec<PageGeometry>,
    sizes: Arc<Vec<(f32, f32)>>,
    outline: Arc<Vec<Outline>>,
    /// Damage verdicts for embedded images, reused across page requests.
    integrity: RefCell<integrity::IntegrityCache>,
}
const MAX_PDF_BYTES: u64 = 256 * 1024 * 1024;

/// Cheap identity of a source file.
///
/// Size and modification time alone cannot see a same-size rewrite that
/// preserves the timestamp, which is exactly what an in-place editor or a
/// coarse-timestamp filesystem can produce. The device and inode (or the
/// Windows file index) change when a file is replaced by rename, so a reload is
/// triggered for that case too. A deliberate same-inode, same-size,
/// same-timestamp in-place edit still needs a full fingerprint, which is why
/// saving a sidecar and exporting re-hash the source before publishing.
#[derive(PartialEq, Eq)]
struct SourceStamp {
    len: u64,
    modified: SystemTime,
    #[cfg(unix)]
    device: u64,
    #[cfg(unix)]
    inode: u64,
    #[cfg(windows)]
    volume: u32,
    #[cfg(windows)]
    index: u64,
}

struct OpenedDocument {
    doc: PdfDocument,
    fingerprint: String,
    stamp: Option<SourceStamp>,
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

fn source_stamp(path: &std::path::Path) -> Result<SourceStamp> {
    let metadata =
        std::fs::metadata(path).with_context(|| format!("无法读取 {}", path.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        Ok(SourceStamp {
            len: metadata.len(),
            modified: metadata.modified()?,
            device: metadata.dev(),
            inode: metadata.ino(),
        })
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        Ok(SourceStamp {
            len: metadata.len(),
            modified: metadata.modified()?,
            volume: metadata.volume_serial_number().unwrap_or_default(),
            index: metadata.file_index().unwrap_or_default(),
        })
    }
    #[cfg(not(any(unix, windows)))]
    {
        Ok(SourceStamp {
            len: metadata.len(),
            modified: metadata.modified()?,
        })
    }
}

fn open_document(source: &Source) -> Result<OpenedDocument> {
    match source {
        Source::Sample => {
            let bytes = include_bytes!("../../../assets/samples/field-notes.pdf");
            let fingerprint = breeze_storage::bytes_fingerprint(bytes);
            let document = PdfDocument::from_bytes(bytes).context("Cannot open bundled PDF")?;
            Ok(OpenedDocument {
                doc: document,
                fingerprint,
                stamp: None,
            })
        }
        Source::File(path) => {
            if !path
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("pdf"))
            {
                bail!("当前仅支持 PDF；其他格式尚未接入");
            }
            let before = source_stamp(path)?;
            anyhow::ensure!(
                before.len <= MAX_PDF_BYTES,
                "当前版本限制 PDF 文件为 256 MiB"
            );
            let fingerprint = breeze_storage::fingerprint(path)
                .with_context(|| format!("无法校验 {}", path.display()))?;
            let document = PdfDocument::open(path.as_path())
                .with_context(|| format!("无法打开 {}", path.display()))?;
            let after = source_stamp(path)?;
            anyhow::ensure!(before == after, "PDF 在打开过程中发生变化，请重试");
            let opened_fingerprint = breeze_storage::fingerprint(path)
                .with_context(|| format!("无法复核 {}", path.display()))?;
            anyhow::ensure!(
                opened_fingerprint == fingerprint,
                "PDF 在打开过程中发生变化，请重试"
            );
            Ok(OpenedDocument {
                doc: document,
                fingerprint,
                stamp: Some(after),
            })
        }
    }
}

fn render(
    job: &Job,
    loaded: &mut Option<Loaded>,
    cache: &mut VecDeque<(Rotation, Page)>,
) -> Result<Page> {
    let time = Instant::now();
    let stamp = match &job.source {
        Source::Sample => None,
        Source::File(path) => Some(source_stamp(path)?),
    };
    if loaded
        .as_ref()
        .is_none_or(|l| l.source != job.source || l.stamp != stamp)
    {
        let OpenedDocument {
            doc,
            fingerprint,
            stamp,
        } = open_document(&job.source)?;
        anyhow::ensure!(
            !doc.needs_password()
                .context("Cannot inspect PDF password state")?,
            "当前不支持加密/密码 PDF"
        );
        let trailer = doc.trailer().context("Cannot inspect PDF trailer")?;
        anyhow::ensure!(
            trailer.get_dict("Encrypt")?.is_none(),
            "当前不支持加密/密码 PDF"
        );
        let total = usize::try_from(doc.page_count().context("Cannot read PDF page count")?)
            .context("Invalid PDF page count")?;
        anyhow::ensure!(total > 0 && total <= 20_000, "当前支持 1–20000 页文档");
        let geometries = (0..total)
            .map(|page| {
                page_geometry(&doc, page).with_context(|| format!("第 {} 页不支持阅读", page + 1))
            })
            .collect::<Result<Vec<_>>>()?;
        let sizes = Arc::new(geometries.iter().map(|g| g.size()).collect());
        let mut outline = Vec::new();
        flatten(
            &doc.outlines().context("Cannot read PDF outline")?,
            0,
            &mut outline,
        );
        *loaded = Some(Loaded {
            source: job.source.clone(),
            geometries,
            sizes,
            outline: Arc::new(outline),
            doc,
            fingerprint,
            stamp,
            integrity: RefCell::new(integrity::IntegrityCache::default()),
        });
        cache.clear();
    }
    let loaded = loaded.as_mut().context("PDF not loaded")?;
    let doc = &loaded.doc;
    let total = usize::try_from(doc.page_count().context("Cannot read PDF page count")?)
        .context("Invalid PDF page count")?;
    if total == 0 {
        bail!("文档没有可阅读的页面");
    }
    anyhow::ensure!(
        job.page < total,
        "请求的第 {} 页超出文档范围（共 {} 页）",
        job.page + 1,
        total
    );
    let page = job.page;
    if let Some(index) = cache.iter().position(|(rotation, p)| {
        job.scale.is_none()
            && job.region.is_none()
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
    } else {
        let requested_dpi = if let Some(scale) = job.scale {
            let dpi = (f64::from(scale) * 72.).ceil();
            anyhow::ensure!(
                dpi.is_finite() && (1. ..=4_608.).contains(&dpi),
                "不支持的页面缩放"
            );
            dpi as u32
        } else if job.thumbnail {
            24
        } else {
            144
        };
        let (mut dpi, pixel_budget) = if job.thumbnail {
            (requested_dpi, 120_000)
        } else if job.scale.is_some() || job.region.is_some() {
            (requested_dpi, MAX_PAGE_PIXELS)
        } else {
            (requested_dpi, 4_000_000)
        };
        let mut expected = job.region.map_or_else(
            || raster_dimensions(points, dpi),
            |region| region_dimensions(region, job.scale.unwrap_or(dpi as f32 / 72.)),
        );
        while job.thumbnail && u64::from(expected.0) * u64::from(expected.1) > pixel_budget {
            anyhow::ensure!(dpi > 1, "页面像素数超出缩略图预算");
            let pixel_ratio = (pixel_budget as f64
                / (u64::from(expected.0) * u64::from(expected.1)) as f64)
                .sqrt();
            dpi = ((f64::from(dpi) * pixel_ratio).floor() as u32)
                .min(dpi - 1)
                .max(1);
            expected = raster_dimensions(points, dpi);
        }
        if job.thumbnail {
            while u64::from(expected.0) * u64::from(expected.1) > pixel_budget {
                anyhow::ensure!(dpi > 1, "页面像素数超出缩略图预算");
                let ratio = (pixel_budget as f64
                    / (u64::from(expected.0) * u64::from(expected.1)) as f64)
                    .sqrt();
                dpi = ((f64::from(dpi) * ratio).floor() as u32)
                    .min(dpi - 1)
                    .max(1);
                expected = raster_dimensions(points, dpi);
            }
        } else if job.region.is_none() {
            anyhow::ensure!(
                u64::from(expected.0) * u64::from(expected.1) <= pixel_budget
                    || job.scale.is_some(),
                "页面像素数超出当前渲染预算"
            );
        }
        let page_handle = doc
            .load_pdf_page(i32::try_from(page)?)
            .with_context(|| format!("无法读取第 {} 页", page + 1))?;
        // MuPDF draws malformed content defensively and leaves the Cookie empty,
        // so referenced content is verified before its pixels are published.
        integrity::verify_page_content(doc, &page_handle, &mut loaded.integrity.borrow_mut())
            .with_context(|| format!("第 {} 页渲染失败，无法完整显示", page + 1))?;
        let image = (|| -> Result<Pixels> {
            if let Some(region) = job.region {
                render_page_region(
                    &page_handle,
                    geometry.view(job.rotation),
                    job.scale.context("Region request has no scale")?,
                    region,
                )
            } else {
                render_page_pixmap(&page_handle, dpi)
            }
        })()
        .with_context(|| format!("第 {} 页渲染失败，无法完整显示", page + 1))?;
        anyhow::ensure!(
            image.width.abs_diff(expected.0) <= 1 && image.height.abs_diff(expected.1) <= 1,
            "渲染器未按请求的清晰度返回页面（需要 {}x{}，得到 {}x{}）",
            expected.0,
            expected.1,
            image.width,
            image.height
        );
        (
            Some(if job.region.is_some() {
                image
            } else {
                rotate_region(image, job.rotation)
            }),
            Some(dpi),
        )
    };
    let full_page_request = job.thumbnail || job.scale.is_some() || job.region.is_some();
    let outline = if full_page_request {
        Arc::new(Vec::new())
    } else {
        loaded.outline.clone()
    };
    let (glyphs, text_warning) = if full_page_request {
        (Vec::new(), None)
    } else {
        match doc.load_pdf_page(i32::try_from(page)?) {
            Ok(page_handle) => match extract_glyphs(&page_handle) {
                Ok(glyphs) => glyphs,
                Err(error) => (Vec::new(), Some(format!("文字提取失败：{error:#}"))),
            },
            Err(error) => (Vec::new(), Some(format!("文字提取失败：{error}"))),
        }
    };
    let result = Page {
        pixels,
        region: job.region,
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
    if job.scale.is_some() || job.region.is_some() {
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
fn pdf_number(value: PdfObject) -> Result<f32> {
    let value = value.resolve()?.unwrap_or(value);
    anyhow::ensure!(value.is_number()?, "PDF page box contains a non-number");
    Ok(value.as_float()?)
}

fn pdf_box(page: &PdfPage, name: &str) -> Result<Rect> {
    let value = page
        .object()
        .get_dict_inheritable(name)?
        .with_context(|| format!("Page is missing /{name}"))?;
    let value = value.resolve()?.unwrap_or(value);
    anyhow::ensure!(value.is_array()?, "Page /{name} is not an array");
    anyhow::ensure!(value.len()? == 4, "Page /{name} must have four numbers");
    let mut coordinates = [0.; 4];
    for (index, coordinate) in coordinates.iter_mut().enumerate() {
        let component = value
            .get_array(index as i32)?
            .with_context(|| format!("Page /{name} is incomplete"))?;
        *coordinate = pdf_number(component)?;
    }
    let rect = Rect {
        x: coordinates[0],
        y: coordinates[1],
        width: coordinates[2] - coordinates[0],
        height: coordinates[3] - coordinates[1],
    };
    anyhow::ensure!(
        rect.valid() && rect.width > 0. && rect.height > 0.,
        "Page /{name} has invalid bounds"
    );
    Ok(rect)
}

fn page_geometry(doc: &PdfDocument, index: usize) -> Result<PageGeometry> {
    let page = doc.load_pdf_page(i32::try_from(index)?)?;
    let object = page.object();
    if let Some((_, value)) = object
        .dict_iter()?
        .find_map(|entry| match entry {
            Ok((key, value)) if key.as_name().is_ok_and(|name| name == b"UserUnit") => {
                Some(Ok((key, value)))
            }
            Ok(_) => None,
            Err(error) => Some(Err(error)),
        })
        .transpose()?
    {
        let unit = pdf_number(value)?;
        anyhow::ensure!(
            unit == 1.0,
            "页面使用非默认 UserUnit={unit}；当前阅读器尚不支持此页面单位"
        );
    }
    let media = pdf_box(&page, "MediaBox")?;
    let crop = match object.get_dict_inheritable("CropBox")? {
        Some(_) => pdf_box(&page, "CropBox")?,
        None => media,
    };
    let x = media.x.max(crop.x);
    let y = media.y.max(crop.y);
    let right = (media.x + media.width).min(crop.x + crop.width);
    let top = (media.y + media.height).min(crop.y + crop.height);
    let crop = Rect {
        x,
        y,
        width: right - x,
        height: top - y,
    };
    anyhow::ensure!(
        crop.valid() && crop.width > 0. && crop.height > 0.,
        "页面裁剪区域无效"
    );
    let rotation = page.rotation()?;
    anyhow::ensure!(
        rotation.rem_euclid(90) == 0,
        "当前仅支持 90 度倍数的页面旋转"
    );
    Ok(PageGeometry { crop, rotation })
}

fn raster_dimensions(points: (f32, f32), dpi: u32) -> (u32, u32) {
    let scale = f64::from(dpi) / 72.;
    (
        (f64::from(points.0) * scale).ceil() as u32,
        (f64::from(points.1) * scale).ceil() as u32,
    )
}

fn region_dimensions(region: Rect, scale: f32) -> (u32, u32) {
    (
        (f64::from(region.width) * f64::from(scale)).ceil() as u32,
        (f64::from(region.height) * f64::from(scale)).ceil() as u32,
    )
}

fn extract_glyphs(page: &PdfPage) -> Result<(Vec<Glyph>, Option<String>)> {
    let text = page.to_text_page(TextPageFlags::empty())?.structured();
    let inverse = page
        .ctm()?
        .invert()
        .context("Cannot invert PDF page transform")?;
    let mut glyphs = Vec::new();
    for block in text.blocks {
        let mupdf::text_page::TextBlockContent::Text { lines } = block.content else {
            continue;
        };
        for line in lines {
            for character in line.chars {
                glyphs.push(Glyph {
                    character: character.ch,
                    bounds: quad_bounds(character.quad, &inverse),
                });
                if glyphs.len() > 50_000 {
                    return Ok((Vec::new(), Some("当前页字符数量超出文本选择预算".into())));
                }
            }
        }
    }
    glyphs.retain(|glyph| glyph.bounds.valid());
    Ok((glyphs, None))
}

fn quad_bounds(quad: mupdf::Quad, transform: &Matrix) -> Rect {
    let points = [quad.ul, quad.ur, quad.ll, quad.lr].map(|point| {
        let (x, y) = transform.transform_xy(point.x, point.y);
        (x, y)
    });
    let left = points
        .iter()
        .map(|point| point.0)
        .fold(f32::INFINITY, f32::min);
    let right = points
        .iter()
        .map(|point| point.0)
        .fold(f32::NEG_INFINITY, f32::max);
    let bottom = points
        .iter()
        .map(|point| point.1)
        .fold(f32::INFINITY, f32::min);
    let top = points
        .iter()
        .map(|point| point.1)
        .fold(f32::NEG_INFINITY, f32::max);
    Rect {
        x: left,
        y: bottom,
        width: right - left,
        height: top - bottom,
    }
}
/// Bound on outline entries retained for one document. A malformed outline is an
/// auxiliary structure, so it is truncated instead of failing the whole document.
const MAX_OUTLINE_ENTRIES: usize = 10_000;
/// Bound on nesting depth, so a pathological tree cannot exhaust the stack here.
const MAX_OUTLINE_DEPTH: usize = 32;
/// Bound on a single retained title.
const MAX_OUTLINE_TITLE_CHARS: usize = 512;

fn flatten(items: &[MuPdfOutline], depth: usize, out: &mut Vec<Outline>) {
    if depth > MAX_OUTLINE_DEPTH {
        return;
    }
    for item in items {
        if out.len() >= MAX_OUTLINE_ENTRIES {
            return;
        }
        let page = item
            .dest
            .map(|destination| destination.loc.page_number as usize);
        out.push(Outline {
            title: item.title.chars().take(MAX_OUTLINE_TITLE_CHARS).collect(),
            page,
            depth,
        });
        flatten(&item.down, depth + 1, out);
    }
}

fn convert_pixmap(pixmap: &Pixmap) -> Result<Pixels> {
    anyhow::ensure!(
        pixmap.n() == 3 && !pixmap.alpha(),
        "MuPDF returned an unexpected pixel layout"
    );
    let width = pixmap.width();
    let height = pixmap.height();
    let stride = usize::try_from(pixmap.stride()).context("Invalid MuPDF pixmap stride")?;
    let row_bytes = usize::try_from(width)? * 3;
    anyhow::ensure!(
        stride >= row_bytes,
        "MuPDF pixmap stride is shorter than a row"
    );
    convert_rgb_to_bgra(width, height, stride, pixmap.samples())
}

fn render_page_pixmap(page: &PdfPage, dpi: u32) -> Result<Pixels> {
    let scale = dpi as f32 / 72.;
    let transform = Matrix::new_scale(scale, scale);
    let colorspace = Colorspace::device_rgb();
    let pixmap = page.to_pixmap(&transform, &colorspace, false, true)?;
    convert_pixmap(&pixmap)
}

fn view_region_to_page(
    geometry: breeze_core::annotations::ViewGeometry,
    region: Rect,
) -> Result<Rect> {
    let base_size = geometry.page.size();
    let view_size = geometry.size();
    anyhow::ensure!(region.valid(), "页面区域无效");
    let x0 = region.x.clamp(0., view_size.0);
    let y0 = region.y.clamp(0., view_size.1);
    let x1 = (region.x + region.width).clamp(0., view_size.0);
    let y1 = (region.y + region.height).clamp(0., view_size.1);
    anyhow::ensure!(x1 > x0 && y1 > y0, "页面区域超出文档边界");
    let corners = [(x0, y0), (x1, y0), (x0, y1), (x1, y1)].map(|(x, y)| {
        geometry
            .rotation
            .unmap(base_size, breeze_core::annotations::Point::new(x, y))
    });
    let left = corners.iter().map(|p| p.x).fold(f32::INFINITY, f32::min);
    let top = corners.iter().map(|p| p.y).fold(f32::INFINITY, f32::min);
    let right = corners
        .iter()
        .map(|p| p.x)
        .fold(f32::NEG_INFINITY, f32::max);
    let bottom = corners
        .iter()
        .map(|p| p.y)
        .fold(f32::NEG_INFINITY, f32::max);
    Ok(Rect {
        x: left,
        y: top,
        width: right - left,
        height: bottom - top,
    })
}

/// Split a visible page rectangle into stable, grid-aligned MuPDF tiles.
pub fn page_region_tiles(points: (f32, f32), scale: f32, visible: Rect) -> Result<Vec<Rect>> {
    const INTERIOR_DEVICE: f32 = 508.;
    const GUTTER_DEVICE: f32 = 2.;
    anyhow::ensure!(
        points.0.is_finite()
            && points.1.is_finite()
            && points.0 > 0.
            && points.1 > 0.
            && scale.is_finite()
            && scale > 0.
            && scale <= 64.
            && visible.valid(),
        "页面区域或缩放无效"
    );
    let left = visible.x.max(0.).min(points.0);
    let top = visible.y.max(0.).min(points.1);
    let right = (visible.x + visible.width).clamp(0., points.0);
    let bottom = (visible.y + visible.height).clamp(0., points.1);
    anyhow::ensure!(right > left && bottom > top, "页面区域超出文档边界");
    let tile_page_units = INTERIOR_DEVICE / scale;
    let gutter = GUTTER_DEVICE / scale;
    let first_column = (left / tile_page_units).floor() as usize;
    let last_column = (right / tile_page_units).ceil() as usize;
    let first_row = (top / tile_page_units).floor() as usize;
    let last_row = (bottom / tile_page_units).ceil() as usize;
    let mut regions = Vec::new();
    for row in first_row..last_row {
        for column in first_column..last_column {
            let tile_x = column as f32 * tile_page_units;
            let tile_y = row as f32 * tile_page_units;
            let x0 = (tile_x - gutter).max(0.);
            let y0 = (tile_y - gutter).max(0.);
            let x1 = (tile_x + tile_page_units + gutter).min(points.0);
            let y1 = (tile_y + tile_page_units + gutter).min(points.1);
            if x1 > left && x0 < right && y1 > top && y0 < bottom {
                regions.push(Rect {
                    x: x0,
                    y: y0,
                    width: x1 - x0,
                    height: y1 - y0,
                });
            }
        }
    }
    Ok(regions)
}

fn render_page_region(
    page: &PdfPage,
    geometry: breeze_core::annotations::ViewGeometry,
    scale: f32,
    region: Rect,
) -> Result<Pixels> {
    anyhow::ensure!(
        scale.is_finite() && scale > 0. && scale <= 64.,
        "不支持的页面缩放"
    );
    let clipped = view_region_to_page(geometry, region)?;
    let target = region_dimensions(clipped, scale);
    let pixels = u64::from(target.0) * u64::from(target.1);
    anyhow::ensure!(
        pixels > 0 && pixels <= MAX_REGION_PIXELS,
        "页面区域超出渲染预算"
    );
    let target_w = i32::try_from(target.0)?;
    let target_h = i32::try_from(target.1)?;
    let device_bounds = IRect::new(0, 0, target_w, target_h);
    let matrix = Matrix::new(scale, 0., 0., scale, -clipped.x * scale, -clipped.y * scale);
    let colorspace = Colorspace::device_rgb();
    let mut pixmap = Pixmap::new_with_rect(&colorspace, device_bounds, false)?;
    pixmap.clear_with(255)?;
    let cookie = Cookie::new()?;
    {
        let device = Device::from_pixmap_with_clip(&pixmap, device_bounds)?;
        page.run_with_cookie(&device, &matrix, &cookie)?;
    }
    anyhow::ensure!(
        cookie.errors() == 0 && !cookie.incomplete(),
        "MuPDF returned incomplete region pixels"
    );
    let pixels = convert_pixmap(&pixmap)?;
    Ok(rotate_region(pixels, geometry.rotation))
}

fn convert_rgb_to_bgra(width: u32, height: u32, stride: usize, samples: &[u8]) -> Result<Pixels> {
    anyhow::ensure!(width > 0 && height > 0, "MuPDF returned an empty pixmap");
    let row_bytes = usize::try_from(width)? * 3;
    anyhow::ensure!(
        stride >= row_bytes,
        "MuPDF pixmap stride is shorter than a row"
    );
    anyhow::ensure!(
        samples.len() >= stride * usize::try_from(height)?,
        "MuPDF returned incomplete pixmap storage"
    );
    let mut bgra = Vec::with_capacity(usize::try_from(width)? * usize::try_from(height)? * 4);
    for row in samples.chunks_exact(stride).take(usize::try_from(height)?) {
        let (rgb_pixels, remainder) = row[..row_bytes].as_chunks::<3>();
        anyhow::ensure!(
            remainder.is_empty(),
            "MuPDF returned an incomplete RGB pixel"
        );
        for rgb in rgb_pixels {
            bgra.extend_from_slice(&[rgb[2], rgb[1], rgb[0], 255]);
        }
    }
    anyhow::ensure!(
        bgra.len() == usize::try_from(width)? * usize::try_from(height)? * 4,
        "MuPDF returned incomplete pixmap rows"
    );
    Ok(Pixels {
        width,
        height,
        bgra,
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
    let OpenedDocument { doc, .. } = open_document(&source)?;
    anyhow::ensure!(!doc.needs_password()?, "不支持加密文档搜索");
    let trailer = doc.trailer()?;
    anyhow::ensure!(trailer.get_dict("Encrypt")?.is_none(), "不支持加密文档搜索");
    let needle = query.to_lowercase();
    let mut hits = Vec::new();
    let page_count = usize::try_from(doc.page_count()?)?;
    for page_index in 0..page_count {
        if cancel.load(Ordering::Relaxed) {
            bail!("搜索已取消");
        }
        let page = doc.load_pdf_page(i32::try_from(page_index)?)?;
        let text = page.text(mupdf::TextExtractOptions::default())?;
        for line in text.lines() {
            if line.to_lowercase().contains(&needle) {
                hits.push(SearchHit {
                    page: page_index,
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
                region: None,
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
            region: None,
            rotation: Rotation::None,
            reply: second,
        });
        assert!(old.try_recv().is_err());
        assert_eq!(pending.job.as_ref().unwrap().page, 7);
    }
    #[test]
    fn pixel_conversion_preserves_alpha_and_swaps_channels() {
        let pixels = convert_rgb_to_bgra(1, 1, 3, &[10, 20, 30]).unwrap();
        assert_eq!(pixels.bgra, vec![30, 20, 10, 255]);
    }

    #[test]
    fn pixel_conversion_skips_row_padding_without_reordering_rows() {
        let pixels = convert_rgb_to_bgra(1, 2, 4, &[10, 20, 30, 99, 40, 50, 60, 88]).unwrap();
        assert_eq!(pixels.bgra, vec![30, 20, 10, 255, 60, 50, 40, 255]);
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

    fn receive(receiver: oneshot::Receiver<Result<Page>>) -> Page {
        futures::executor::block_on(receiver).unwrap().unwrap()
    }

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
    fn tiled_regions_reconstruct_the_full_page_for_each_view_rotation() {
        let worker = PdfWorker::new().unwrap();
        let metadata = receive(worker.request_metadata(Source::Sample, 0).unwrap());
        let base_size = metadata.page_sizes[0];

        for rotation in [
            Rotation::None,
            Rotation::Right,
            Rotation::Half,
            Rotation::Left,
        ] {
            let points = rotation.apply(base_size);
            let whole = receive(
                worker
                    .request_page(Source::Sample, 0, rotation, 1.)
                    .unwrap(),
            );
            let expected = whole.pixels.unwrap();
            let visible = Rect {
                x: 0.,
                y: 0.,
                width: points.0,
                height: points.1,
            };
            let tiles = page_region_tiles(points, 1., visible).unwrap();
            assert!(
                tiles.len() > 1,
                "fixture should cross at least one tile edge"
            );

            let mut assembled = vec![0; expected.bgra.len()];
            let mut covered = vec![false; expected.width as usize * expected.height as usize];
            for tile in tiles {
                let page = receive(
                    worker
                        .request_region(Source::Sample, 0, rotation, 1., tile)
                        .unwrap(),
                );
                assert_eq!(page.region, Some(tile));
                let pixels = page.pixels.unwrap();
                let origin_x = (tile.x).round() as i32;
                let origin_y = (tile.y).round() as i32;
                for y in 0..pixels.height as i32 {
                    let target_y = origin_y + y;
                    if !(0..expected.height as i32).contains(&target_y) {
                        continue;
                    }
                    for x in 0..pixels.width as i32 {
                        let target_x = origin_x + x;
                        if !(0..expected.width as i32).contains(&target_x) {
                            continue;
                        }
                        let target =
                            target_y as usize * expected.width as usize + target_x as usize;
                        if covered[target] {
                            continue;
                        }
                        let source = y as usize * pixels.width as usize + x as usize;
                        assembled[target * 4..target * 4 + 4]
                            .copy_from_slice(&pixels.bgra[source * 4..source * 4 + 4]);
                        covered[target] = true;
                    }
                }
            }
            assert!(
                covered.into_iter().all(|pixel| pixel),
                "tile gap at {rotation:?}"
            );
            if let Some(index) =
                (0..expected.width as usize * expected.height as usize).find(|index| {
                    assembled[index * 4..index * 4 + 4] != expected.bgra[index * 4..index * 4 + 4]
                })
            {
                let x = index % expected.width as usize;
                let y = index / expected.width as usize;
                panic!(
                    "tile mismatch at {rotation:?}, ({x}, {y}): {:?} != {:?}",
                    &assembled[index * 4..index * 4 + 4],
                    &expected.bgra[index * 4..index * 4 + 4]
                );
            }
        }
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
