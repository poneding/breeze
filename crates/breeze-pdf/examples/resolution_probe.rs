//! Focused MuPDF page/region probe; not an application performance guarantee.
use anyhow::{Context, Result, ensure};
use mupdf::{Colorspace, Cookie, Device, Matrix, Pixmap, pdf::PdfDocument};
use std::{hint::black_box, path::Path, time::Instant};

const SAMPLE: &[u8] = include_bytes!("../../../assets/samples/field-notes.pdf");
const COUNT: usize = 20;

fn main() -> Result<()> {
    ensure!(!cfg!(debug_assertions), "Run this probe with --release");
    let mut args = std::env::args().skip(1);
    let dpi: u32 = args
        .next()
        .context("Expected DPI: 144, 192, or 384")?
        .parse()?;
    ensure!([144, 192, 384].contains(&dpi), "Unsupported DPI");
    let mode = args.next().context("Expected mode: page or region")?;
    let path = args.next();
    ensure!(args.next().is_none(), "Unexpected extra arguments");
    ensure!(
        ["page", "region"].contains(&mode.as_str()),
        "Unsupported mode"
    );
    let started = Instant::now();
    let doc = if let Some(path) = &path {
        PdfDocument::open(breeze_pdf::mupdf_path(Path::new(path))?)?
    } else {
        PdfDocument::from_bytes(SAMPLE)?
    };
    let page = doc.load_pdf_page(0)?;
    let list = page.to_display_list(true)?;
    let bounds = page.bounds()?;
    let dpi_scale = dpi as f32 / 72.;
    let matrix = Matrix::new_scale(dpi_scale, dpi_scale);
    let initialization_ms = started.elapsed().as_secs_f64() * 1000.;
    let colorspace = Colorspace::device_rgb();
    let mut samples = Vec::with_capacity(COUNT);
    let mut first_ms = 0.;
    let mut dimensions = (0, 0);
    let mut returned_bytes = 0;
    for iteration in 0..=COUNT {
        let start = Instant::now();
        let pixmap = if mode == "page" {
            page.to_pixmap(&matrix, &colorspace, false, true)?
        } else {
            let mut clip = bounds;
            clip.x1 = clip.x0 + clip.width() * 0.5;
            let device_bounds = clip.transform(&matrix).round();
            let mut pixmap = Pixmap::new_with_rect(&colorspace, device_bounds, false)?;
            pixmap.clear_with(255)?;
            let cookie = Cookie::new()?;
            {
                let device = Device::from_pixmap_with_clip(&pixmap, device_bounds)?;
                list.run_with_cookie(&device, &matrix, clip, &cookie)?;
            }
            anyhow::ensure!(cookie.errors() == 0 && !cookie.incomplete());
            pixmap
        };
        let elapsed = start.elapsed().as_secs_f64() * 1000.;
        dimensions = (pixmap.width(), pixmap.height());
        returned_bytes = pixmap.samples().len();
        black_box(pixmap.samples());
        if iteration == 0 {
            first_ms = elapsed;
        } else {
            samples.push(elapsed);
        }
    }
    samples.sort_by(f64::total_cmp);
    let fingerprint = if path.is_some() {
        "measured-separately".to_owned()
    } else {
        breeze_storage::bytes_fingerprint(SAMPLE)
    };
    println!(
        "sample={} page=0 sha256={} profile=release dpi={dpi} mode={mode} n={COUNT} initialization_ms={initialization_ms:.2} first_render_ms={first_ms:.2} warm_uncached_p50_ms={:.2} warm_uncached_p95_ms={:.2} dimensions={dimensions:?} returned_bytes={returned_bytes} pixel_buffer_bytes={} disk_io={} gpu=false",
        path.as_deref().unwrap_or("field-notes.pdf"),
        fingerprint,
        samples[9],
        samples[18],
        u64::from(dimensions.0) * u64::from(dimensions.1) * 3,
        path.is_some(),
    );
    Ok(())
}
