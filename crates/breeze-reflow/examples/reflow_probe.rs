//! Release adapter measurements; excludes GPUI layout, GPU upload and presentation.
#[path = "../tests/common/mod.rs"]
mod common;
use breeze_core::document::EpubLocation;
use breeze_reflow::{
    epub::{Book, Request},
    markdown::Document,
};
use std::time::Instant;

fn report(name: &str, mut values: Vec<f64>) {
    values.sort_by(f64::total_cmp);
    println!(
        "{name}: n={} p50={:.3}ms p95={:.3}ms",
        values.len(),
        values[values.len() / 2],
        values[(values.len() * 95).div_ceil(100).saturating_sub(1)]
    );
}

fn main() -> anyhow::Result<()> {
    let root = std::path::PathBuf::from("target/reflow-probe");
    std::fs::create_dir_all(&root)?;
    let path = root.join("authored.epub");
    let bytes = common::publication(100, 50);
    std::fs::write(&path, &bytes)?;
    println!(
        "EPUB archive={} bytes, 100 chapters × 50 paragraphs, viewport=600×800 scale=2, font=18",
        bytes.len()
    );
    let request = Request {
        path: path.clone(),
        location: EpubLocation::default(),
        width: 600.,
        height: 800.,
        font_size: 18.,
        scale: 2.,
        turn: 0,
        scroll: 0.,
        find: None,
        spread: false,
    };
    let start = Instant::now();
    let mut book = Book::open(&path)?;
    let frame = book.render(&request)?;
    println!(
        "process-first EPUB open+first page={:.3}ms sha256={}",
        start.elapsed().as_secs_f64() * 1000.,
        frame.metadata.fingerprint
    );
    let mut render = Vec::new();
    for _ in 0..20 {
        let start = Instant::now();
        let _ = book.render(&request)?;
        render.push(start.elapsed().as_secs_f64() * 1000.);
    }
    report("warm EPUB page (same page, cached)", render);
    // Density, so a sparse layout is visible in the numbers: characters the
    // layout engine fits into one viewport-height page.
    let mut density = Vec::new();
    for expression in 1..=8u32 {
        let mut probe = request.clone();
        probe.location.progression = (expression - 1) as f32 / 7.;
        let frame = book.render(&probe)?;
        density.push((frame.text.chars().count(), frame.chapter_pages));
    }
    let characters: usize = density.iter().map(|(chars, _)| chars).sum();
    println!(
        "EPUB chapter: {} pages, {:.0} characters per page over 8 sampled pages",
        density[0].1,
        characters as f64 / density.len() as f64
    );
    // A page turn: a different progression, which is what the reader requests
    // when the user swipes or presses Next.
    let mut turn = request.clone();
    let mut turns = Vec::new();
    let mut text_bytes = 0usize;
    for step in 1..=20usize {
        turn.location.progression = (step % 8) as f32 / 8.;
        let start = Instant::now();
        let frame = book.render(&turn)?;
        turns.push(start.elapsed().as_secs_f64() * 1000.);
        text_bytes += frame.text.len();
    }
    report("EPUB page turn (chapter already laid out)", turns);
    std::hint::black_box(text_bytes);
    let start = Instant::now();
    let mut jump = request.clone();
    jump.location.href = book.metadata.chapters.last().unwrap().href.clone();
    book.render(&jump)?;
    println!(
        "first jump to chapter 100={:.3}ms",
        start.elapsed().as_secs_f64() * 1000.
    );
    // End-to-end page turns through the real worker: a turn the reader has
    // prefetched must not wait for a layout, and one it has not must.
    let worker = breeze_reflow::EpubWorker::new()?;
    let mut first = request.clone();
    first.scale = 1.;
    let frame = futures::executor::block_on(worker.request(first.clone())?)?;
    std::hint::black_box(&frame);
    let mut cold = Vec::new();
    let mut warm = Vec::new();
    for step in 1..=10usize {
        let mut next = first.clone();
        next.location.progression = (step % 8) as f32 / 8.;
        // Cold: ask for the page the worker has not seen.
        let start = Instant::now();
        let frame = futures::executor::block_on(worker.request(next.clone())?)?;
        cold.push(start.elapsed().as_secs_f64() * 1000.);
        std::hint::black_box(&frame);
        // Prefetch the following page the way the reader does, then turn to it.
        let mut upcoming = first.clone();
        upcoming.location.progression = ((step + 1) % 8) as f32 / 8.;
        worker.prefetch(upcoming.clone())?;
        std::thread::sleep(std::time::Duration::from_millis(120));
        let start = Instant::now();
        let frame = futures::executor::block_on(worker.request(upcoming)?)?;
        warm.push(start.elapsed().as_secs_f64() * 1000.);
        std::hint::black_box(&frame);
    }
    report("worker page turn, nothing prefetched", cold);
    report("worker page turn, prefetched", warm);

    let source = (0..10_000).map(|i| format!("## Heading {i}\n\nParagraph **bold** and [link](https://example.com) with 中文 text.\n\n")).collect::<String>();
    std::fs::write(root.join("authored.md"), &source)?;
    println!("Markdown source={} bytes, 20,000 blocks", source.len());
    let mut parse = Vec::new();
    for _ in 0..20 {
        let start = Instant::now();
        let doc = Document::parse(&source)?;
        std::hint::black_box(doc);
        parse.push(start.elapsed().as_secs_f64() * 1000.);
    }
    report("warm Markdown parse+block HTML", parse);
    Ok(())
}
