//! Release probe for the editor's parse and projection costs.
//!
//! Run with `cargo test --release -p breeze-reflow --test markdown_performance
//! -- --ignored --nocapture`. The numbers are library costs, not editor frame
//! times: they say how much of the input budget one snapshot would consume if
//! it ran on the UI thread, which is what decides whether a provisional path
//! must exist at all.
use breeze_reflow::markdown::{Projection, ProjectionMode, Semantic};
use std::time::{Duration, Instant};

/// A document with the shapes real Markdown actually contains.
fn corpus(target_bytes: usize) -> String {
    let mut out = String::with_capacity(target_bytes + 4096);
    let mut index = 0usize;
    while out.len() < target_bytes {
        out.push_str(&format!(
            "## Section {index}\n\n\
             A paragraph with **bold**, *italic*, `code`, [a link](https://example.com/{index}) and 中文文本，\
             以及一些标点符号。Another sentence keeps the line long enough to wrap in a narrow window.\n\n\
             - list item one\n- list item two with `code`\n  - nested item\n\n\
             > a quotation that spans\n> two source lines\n\n\
             | Column A | Column B |\n|---|---|\n| value {index} | 值 |\n\n\
             ```rust\nfn sample_{index}() -> usize {{ {index} }}\n```\n\n"
        ));
        index += 1;
    }
    out
}

fn percentile(mut samples: Vec<Duration>, p: f64) -> Duration {
    samples.sort_unstable();
    let index = ((samples.len() as f64 - 1.) * p).round() as usize;
    samples[index.min(samples.len() - 1)]
}

fn measure(samples: usize, mut run: impl FnMut()) -> (Duration, Duration) {
    let mut timings = Vec::with_capacity(samples);
    for _ in 0..samples {
        let started = Instant::now();
        run();
        timings.push(started.elapsed());
    }
    (percentile(timings.clone(), 0.5), percentile(timings, 0.95))
}

#[test]
#[ignore = "Release measurement; run explicitly with --release"]
fn parse_and_projection_cost_by_document_size() {
    println!(
        "{:>10}  {:>8}  {:>16}  {:>16}  {:>16}",
        "source", "lines", "parse p50/p95", "semantic p50/p95", "provisional p50/p95"
    );
    let samples = if cfg!(debug_assertions) { 3 } else { 20 };
    for size in [10 * 1024, 100 * 1024, 1024 * 1024, 16 * 1024 * 1024] {
        let source = corpus(size);
        let lines = source.lines().count();
        let (parse_p50, parse_p95) = measure(samples, || {
            let semantic = Semantic::parse(&source).unwrap();
            std::hint::black_box(semantic.lines.len());
        });
        let semantic = Semantic::parse(&source).unwrap();
        let (build_p50, build_p95) = measure(samples, || {
            let projection = Projection::build_semantic(
                &source,
                source.len()..source.len(),
                ProjectionMode::Live,
                None,
                &semantic,
            );
            std::hint::black_box(projection.text.len());
        });
        let (provisional_p50, provisional_p95) = measure(samples, || {
            let projection = Projection::build_with_literal(
                &source,
                source.len()..source.len(),
                ProjectionMode::Live,
                None,
            );
            std::hint::black_box(projection.text.len());
        });
        println!(
            "{:>10}  {:>8}  {:>7.1}/{:<6.1}ms  {:>7.1}/{:<6.1}ms  {:>7.1}/{:<6.1}ms",
            size,
            lines,
            parse_p50.as_secs_f64() * 1e3,
            parse_p95.as_secs_f64() * 1e3,
            build_p50.as_secs_f64() * 1e3,
            build_p95.as_secs_f64() * 1e3,
            provisional_p50.as_secs_f64() * 1e3,
            provisional_p95.as_secs_f64() * 1e3,
        );
    }
}

#[test]
#[ignore = "Release measurement; run explicitly with --release"]
fn one_megabyte_snapshot_materialization() {
    let source = corpus(1024 * 1024);
    let buffer = breeze_reflow::markdown::SourceBuffer::from_bytes(source.as_bytes()).unwrap();
    let (p50, p95) = measure(50, || {
        let text = buffer.text(0..buffer.len()).unwrap();
        std::hint::black_box(text.len());
    });
    println!(
        "1 MiB rope materialization: p50 {:.2}ms p95 {:.2}ms",
        p50.as_secs_f64() * 1e3,
        p95.as_secs_f64() * 1e3
    );
}
