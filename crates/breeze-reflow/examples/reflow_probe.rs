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
    report("warm EPUB page", render);
    let start = Instant::now();
    let mut jump = request;
    jump.location.href = book.metadata.chapters.last().unwrap().href.clone();
    book.render(&jump)?;
    println!(
        "first jump to chapter 100={:.3}ms",
        start.elapsed().as_secs_f64() * 1000.
    );
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
