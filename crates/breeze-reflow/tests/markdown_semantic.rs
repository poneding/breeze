//! P3 coverage: the parser's semantics drive what the projection displays.
use breeze_reflow::markdown::{Affinity, BlockKind, Projection, ProjectionMode, Semantic};

fn live(source: &str, caret: usize) -> Projection {
    let semantic = Semantic::parse(source).unwrap();
    Projection::build_semantic(source, caret..caret, ProjectionMode::Live, None, &semantic)
}

#[test]
fn resolved_reference_links_render_as_their_label() {
    let source = "see [label][ref] now\n\n[ref]: https://example.com\n";
    let projection = live(source, source.len());
    assert_eq!(
        &projection.text[..projection.lines[0].display.end],
        "see label now"
    );
    let link = projection.lines[0]
        .runs
        .iter()
        .find(|run| run.style.link)
        .expect("the resolved reference link is styled");
    assert_eq!(
        &projection.text[link.range.clone()],
        "label",
        "only the label is visible"
    );
    // Revealing the whole construct restores its source spelling.
    let revealed = live(source, 5);
    assert!(revealed.text.starts_with("see [label][ref] now"));
}

#[test]
fn nested_emphasis_composes_instead_of_overlapping() {
    let source = "x **bold *and* ital** y\n";
    let projection = live(source, source.len());
    let line = &projection.lines[0];
    assert_eq!(&projection.text[line.display.clone()], "x bold and ital y");
    // Runs must be sorted, disjoint and cover the whole styled text.
    let mut cursor = 0;
    for run in &line.runs {
        assert!(
            run.range.start >= cursor,
            "runs overlap: {cursor} > {}",
            run.range.start
        );
        cursor = run.range.end;
    }
    let bold: Vec<&str> = line
        .runs
        .iter()
        .filter(|run| run.style.bold && !run.style.italic)
        .map(|run| &projection.text[run.range.clone()])
        .collect();
    assert_eq!(bold, vec!["bold ", " ital"]);
    let both: Vec<&str> = line
        .runs
        .iter()
        .filter(|run| run.style.bold && run.style.italic)
        .map(|run| &projection.text[run.range.clone()])
        .collect();
    assert_eq!(both, vec!["and"], "the inner emphasis composes with strong");
}

#[test]
fn character_entities_show_their_decoded_text() {
    let source = "a &amp; b &lt;c&gt;\n";
    let projection = live(source, source.len());
    let line = &projection.lines[0];
    assert_eq!(&projection.text[line.display.clone()], "a & b <c>");
    // The decoded range maps back to the entity's whole source spelling.
    let display = projection.source_range_to_display(2..7);
    assert_eq!(projection.display_range_to_source(display), 2..7);
    // Editing right after the entity still lands on the source boundary.
    assert_eq!(projection.source_to_display(7), 3);
}

#[test]
fn setext_headings_hide_their_underline() {
    let source = "Title\n=====\n\nbody\n";
    let projection = live(source, source.len());
    assert_eq!(projection.lines[0].kind, BlockKind::Heading(1));
    assert_eq!(
        &projection.text[projection.lines[0].display.clone()],
        "Title"
    );
    assert!(
        projection.lines[1].display.is_empty(),
        "the underline has no display row: {:?}",
        projection.lines[1].display
    );
    assert_eq!(projection.lines[1].kind, BlockKind::Heading(1));
    // The underline's bytes are still reachable from the mapping.
    let underline_end = projection.lines[1].display.start;
    assert_eq!(
        projection.display_to_source(underline_end, Affinity::After),
        11,
        "the mapping still reaches the hidden underline"
    );
    assert_eq!(projection.lines[2].kind, BlockKind::Paragraph);
}

#[test]
fn a_table_line_keeps_its_own_kind_and_bytes() {
    let source = "| A | B |\n|---|---|\n| 1 | 2 |\n\npara\n";
    let projection = live(source, source.len());
    assert_eq!(projection.lines[0].kind, BlockKind::Table);
    assert_eq!(projection.lines[1].kind, BlockKind::Table);
    assert_eq!(projection.lines[2].kind, BlockKind::Table);
    assert_eq!(projection.lines[3].kind, BlockKind::Paragraph);
    assert_eq!(projection.lines[4].kind, BlockKind::Paragraph);
    assert!(
        projection
            .text
            .starts_with("| A | B |\n|---|---|\n| 1 | 2 |\n")
    );
}

#[test]
fn block_quotes_inside_lists_hide_every_marker_level() {
    let source = "> quote\n\n- item\n  - nested\n";
    let projection = live(source, source.len());
    assert_eq!(projection.lines[0].kind, BlockKind::Quote { depth: 1 });
    assert_eq!(
        &projection.text[projection.lines[0].display.clone()],
        "quote"
    );
    assert_eq!(
        projection.lines[2].kind,
        BlockKind::List {
            ordered: false,
            depth: 0
        }
    );
    assert_eq!(
        projection.lines[3].kind,
        BlockKind::List {
            ordered: false,
            depth: 1
        }
    );
    assert_eq!(
        &projection.text[projection.lines[3].display.clone()],
        "  \u{2022} nested"
    );
}

#[test]
fn fence_boundaries_and_code_content_follow_the_parser() {
    let source = "# H\n\n```rs\n# not a heading\nlet x = 1;\n```\n\n# after\n";
    let projection = live(source, source.len());
    let kinds: Vec<BlockKind> = projection
        .lines
        .iter()
        .map(|line| line.kind.clone())
        .collect();
    assert_eq!(
        kinds,
        vec![
            BlockKind::Heading(1),
            BlockKind::Paragraph,
            BlockKind::Fence,
            BlockKind::Code,
            BlockKind::Code,
            BlockKind::Fence,
            BlockKind::Paragraph,
            BlockKind::Heading(1),
        ],
        "# inside a fence is not a heading and the fence closes"
    );
    // The `#` inside the fence is literal text, not a heading marker.
    assert!(projection.text.contains("# not a heading"));
    assert!(
        projection.lines[4].runs.is_empty(),
        "code content is unstyled"
    );
}

#[test]
fn source_mode_ignores_the_semantic_layer() {
    let source = "# Title\n\n**bold** &amp;\n";
    let semantic = Semantic::parse(source).unwrap();
    let projection =
        Projection::build_semantic(source, 0..0, ProjectionMode::Source, None, &semantic);
    assert_eq!(projection.text, source);
    assert!(projection.lines.iter().all(|line| line.runs.is_empty()));
}

#[test]
fn a_mismatched_semantic_layer_is_refused() {
    let semantic = Semantic::parse("# Title\n").unwrap();
    // Builder must not index a layer parsed from a longer document.
    let projection =
        Projection::build_semantic("short", 0..0, ProjectionMode::Live, None, &semantic);
    assert!(
        !projection.text.contains("Title"),
        "a stale layer must not leak text: {:?}",
        projection.text
    );
    assert_eq!(projection.text.len(), 5);
}

#[test]
fn unfinished_delimiters_stay_visible_and_editable() {
    for source in [
        "**unterminated\n",
        "`x\n",
        "[label][missing]\n",
        "[label](\n",
        ">\n",
        "- \n",
        "```\n",
        "| A |\n",
    ] {
        let projection = live(source, source.len());
        assert!(
            !projection.text.is_empty() || source.is_empty(),
            "{source:?} produced no display text"
        );
        // Whatever the parser decided, the display text is never empty for a
        // non-empty source and always maps back to the whole document.
        let display = projection.source_range_to_display(0..source.len());
        assert_eq!(
            projection.display_range_to_source(display),
            0..source.len(),
            "{source:?} did not round trip: {:?}",
            projection.text
        );
    }
}
