//! Focused renderer feasibility probe; not an application performance guarantee.
use anyhow::{Context, Result, bail, ensure};
use pdf_oxide::{
    PdfDocument,
    rendering::{PageRenderer, RenderOptions, render_page_region},
};
use std::{hint::black_box, time::Instant};

const SAMPLE: &[u8] = include_bytes!("../../../assets/samples/field-notes.pdf");
const COUNT: usize = 20;

fn main() -> Result<()> {
    ensure!(!cfg!(debug_assertions), "Run this probe with --release");
    let mut args = std::env::args().skip(1);
    let dpi: u32 = args
        .next()
        .context("Expected DPI: 144, 192, or 384")?
        .parse()?;
    ensure!([144, 192, 384].contains(&dpi), "Unsupported probe DPI");
    let mode = args
        .next()
        .context("Expected mode: page, region-raw, or region-png")?;
    ensure!(args.next().is_none(), "Unexpected extra arguments");
    ensure!(
        ["page", "region-raw", "region-png"].contains(&mode.as_str()),
        "Unsupported probe mode"
    );
    let mut options = RenderOptions::with_dpi(dpi);
    // Diagnostic only: allow an uncapped 384-DPI sample, never change the app budget.
    options.max_output_pixels = 16_000_000;
    if mode != "region-png" {
        options = options.as_raw();
    }
    let started = Instant::now();
    let doc = PdfDocument::from_bytes(SAMPLE.to_vec())?;
    let info = doc.get_page_info(0)?;
    let expected_pixels = (f64::from(info.media_box.width) * f64::from(dpi) / 72.).ceil()
        * (f64::from(info.media_box.height) * f64::from(dpi) / 72.).ceil();
    ensure!(
        expected_pixels <= options.max_output_pixels as f64,
        "Probe would downscale"
    );
    let mut renderer = (mode == "page").then(|| PageRenderer::new(options.clone()));
    let initialization_ms = started.elapsed().as_secs_f64() * 1000.;
    let side = 512. * 72. / dpi as f32;
    let mut samples = Vec::with_capacity(COUNT);
    let mut first_ms = 0.;
    let mut dimensions = (0, 0);
    let mut returned_bytes = 0;
    for iteration in 0..=COUNT {
        let start = Instant::now();
        let result = if let Some(renderer) = &mut renderer {
            renderer.render_page(&doc, 0)
        } else {
            render_page_region(&doc, 0, (0., 0., side, side), &options)
        };
        let elapsed = start.elapsed().as_secs_f64() * 1000.;
        let image = match result {
            Ok(image) => image,
            Err(error) => bail!("mode={mode} dpi={dpi} elapsed_ms={elapsed:.2}: {error}"),
        };
        dimensions = (image.width, image.height);
        returned_bytes = image.data.len();
        black_box(&image.data);
        if iteration == 0 {
            first_ms = elapsed;
        } else {
            samples.push(elapsed);
        }
    }
    samples.sort_by(f64::total_cmp);
    println!(
        "sample=field-notes page=0 sha256={} profile=release dpi={dpi} mode={mode} n={COUNT} initialization_ms={initialization_ms:.2} first_render_ms={first_ms:.2} warm_uncached_p50_ms={:.2} warm_uncached_p95_ms={:.2} dimensions={dimensions:?} returned_bytes={returned_bytes} pixel_buffer_bytes={} disk_io=false gpu=false",
        breeze_storage::bytes_fingerprint(SAMPLE),
        samples[9],
        samples[18],
        u64::from(dimensions.0) * u64::from(dimensions.1) * 4,
    );
    Ok(())
}
