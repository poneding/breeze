//! Temporary measurement: one viewport region versus the same cells rendered separately.
use anyhow::{Context, Result};
use pdf_oxide::{
    PdfDocument,
    rendering::{PageRenderer, RenderOptions},
};
use std::time::Instant;

const EDGE: u32 = 508;
const GUTTER: u32 = 2;

fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let path = std::path::PathBuf::from(args.next().context("pdf path")?);
    let page: usize = args.next().context("page")?.parse()?;
    let dpi: u32 = args.next().context("dpi")?.parse()?;
    let bytes = std::fs::read(path)?;
    let doc = PdfDocument::from_bytes(bytes)?;
    let mut options = RenderOptions::with_dpi(dpi).as_raw();
    options.max_output_pixels = breeze_pdf::MAX_REGION_PIXELS;
    let mut renderer = PageRenderer::new(options);
    let scale = dpi as f32 / 72.;
    // A 3x2 block of Breeze cells: one viewport's worth at 2x on a typical window.
    let cells: Vec<(u32, u32, u32, u32)> = [1, 2, 3]
        .into_iter()
        .flat_map(|col| {
            [1, 2]
                .into_iter()
                .map(move |row| (col * EDGE - GUTTER, row * EDGE - GUTTER, EDGE + 4, EDGE + 4))
        })
        .collect();
    let left = cells.iter().map(|c| c.0).min().unwrap();
    let top = cells.iter().map(|c| c.1).min().unwrap();
    let right = cells.iter().map(|c| c.0 + c.2).max().unwrap();
    let bottom = cells.iter().map(|c| c.1 + c.3).max().unwrap();
    let region = (left, top, right - left, bottom - top);
    // Warm both paths before timing.
    let warm = renderer.render_device_region(&doc, page, scale, region)?;
    for cell in &cells {
        renderer.render_device_region(&doc, page, scale, *cell)?;
    }
    let n = 20;
    let start = Instant::now();
    for _ in 0..n {
        renderer.render_device_region(&doc, page, scale, region)?;
    }
    let region_ms = start.elapsed().as_secs_f64() * 1000. / f64::from(n);
    let mut latencies = Vec::new();
    let start = Instant::now();
    for _ in 0..n {
        for cell in &cells {
            let now = Instant::now();
            renderer.render_device_region(&doc, page, scale, *cell)?;
            latencies.push(now.elapsed().as_secs_f64() * 1000.);
        }
    }
    let cells_ms = start.elapsed().as_secs_f64() * 1000. / f64::from(n);
    latencies.sort_by(f64::total_cmp);
    println!(
        "dpi={dpi} region={}x{} region_pixels={} cells={} region_ms={region_ms:.2} cells_total_ms={cells_ms:.2} cell_p50_ms={:.2} cell_p95_ms={:.2} region_bytes={}",
        region.2,
        region.3,
        u64::from(region.2) * u64::from(region.3),
        cells.len(),
        latencies[latencies.len() / 2],
        latencies[latencies.len() * 95 / 100],
        warm.data.len()
    );
    Ok(())
}
