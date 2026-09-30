//! Local-file comparison/measurement. Output belongs under target/, never assets.
use anyhow::{Context, Result, ensure};
use pdf_oxide::{
    PdfDocument,
    rendering::{PageRenderer, RenderOptions},
};
use std::{path::PathBuf, time::Instant};

fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let path = PathBuf::from(args.next().context("Expected PDF path")?);
    let page: usize = args.next().context("Expected zero-based page")?.parse()?;
    let dpi: u32 = args.next().context("Expected DPI")?.parse()?;
    let output = PathBuf::from(args.next().context("Expected output directory")?);
    ensure!((72..=576).contains(&dpi), "DPI outside probe range");
    std::fs::create_dir_all(&output)?;
    let bytes = std::fs::read(path)?;
    println!(
        "source_sha256={} page={page} dpi={dpi} release={}",
        breeze_storage::bytes_fingerprint(&bytes),
        !cfg!(debug_assertions)
    );
    let start = Instant::now();
    let doc = PdfDocument::from_bytes(bytes)?;
    let mut options = RenderOptions::with_dpi(dpi).as_raw();
    options.max_output_pixels = 24_000_000;
    let mut renderer = PageRenderer::new(options);
    let full = renderer.render_page(&doc, page)?;
    println!(
        "first_full_ms={:.2} size={}x{} bytes={}",
        start.elapsed().as_secs_f64() * 1000.,
        full.width,
        full.height,
        full.data.len()
    );
    image::save_buffer(
        output.join("full.png"),
        &full.data,
        full.width,
        full.height,
        image::ColorType::Rgba8,
    )?;
    let mut assembled = vec![255; full.data.len()];
    let start = Instant::now();
    let mut latencies = Vec::new();
    for y in (0..full.height).step_by(508) {
        for x in (0..full.width).step_by(508) {
            let w = 508.min(full.width - x);
            let h = 508.min(full.height - y);
            let left = x.saturating_sub(2);
            let top = y.saturating_sub(2);
            let right = (x + w + 2).min(full.width);
            let bottom = (y + h + 2).min(full.height);
            let now = Instant::now();
            let tile = renderer.render_device_region(
                &doc,
                page,
                dpi as f32 / 72.,
                (left, top, right - left, bottom - top),
            )?;
            latencies.push(now.elapsed().as_secs_f64() * 1000.);
            for row in 0..h {
                let dst = (((y + row) * full.width + x) * 4) as usize;
                let src = (((row + y - top) * tile.width + x - left) * 4) as usize;
                assembled[dst..dst + (w * 4) as usize]
                    .copy_from_slice(&tile.data[src..src + (w * 4) as usize]);
            }
        }
    }
    latencies.sort_by(f64::total_cmp);
    println!(
        "all_tiles_ms={:.2} n={} tile_p50_ms={:.2} tile_p95_ms={:.2}",
        start.elapsed().as_secs_f64() * 1000.,
        latencies.len(),
        latencies[(latencies.len() - 1) / 2],
        latencies[(latencies.len() * 95).div_ceil(100) - 1]
    );
    let differences = assembled
        .iter()
        .zip(&full.data)
        .filter(|(a, b)| a.abs_diff(**b) > 2)
        .count();
    println!(
        "channels_differing_over_2={differences}/{}",
        assembled.len()
    );
    image::save_buffer(
        output.join("tiles.png"),
        &assembled,
        full.width,
        full.height,
        image::ColorType::Rgba8,
    )?;
    Ok(())
}
