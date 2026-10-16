//! P2 projection coverage: hidden markers, mapping and reveal rules.
use breeze_reflow::markdown::{Affinity, BlockKind, Projection, RunStyle};

fn live(source: &str, caret: usize) -> Projection {
    Projection::live(source, caret..caret)
}

#[test]
fn source_mode_passes_every_byte_through() {
    let source = "# Title\r\n\r\n- item **bold**\r\n";
    let projection = Projection::source(source);
    assert_eq!(projection.text, source);
    assert_eq!(projection.line_count(), 3);
    assert!(
        projection
            .lines
            .iter()
            .all(|line| line.kind == BlockKind::Paragraph)
    );
    for line in &projection.lines {
        assert!(line.runs.is_empty());
    }
    // Identity mapping in source mode.
    for offset in 0..=source.len() {
        if source.is_char_boundary(offset) {
            assert_eq!(projection.source_to_display(offset), offset);
            assert_eq!(
                projection.display_to_source(offset, Affinity::Before),
                offset
            );
            assert_eq!(
                projection.display_to_source(offset, Affinity::After),
                offset
            );
        }
    }
}

#[test]
fn headings_hide_and_reveal_their_markers() {
    let source = "# Title\nbody\n";
    let hidden = live(source, 7);
    assert_eq!(hidden.lines[0].kind, BlockKind::Heading(1));
    assert!(!hidden.lines[0].revealed);
    assert_eq!(&hidden.text[..5], "Title");
    assert_eq!(
        hidden.source_to_display(2),
        0,
        "the `# ` collapses onto display 0"
    );
    assert_eq!(hidden.display_to_source(0, Affinity::Before), 0);
    assert_eq!(hidden.display_to_source(0, Affinity::After), 2);
    assert_eq!(hidden.display_to_source(5, Affinity::After), 7);
    assert_eq!(hidden.lines[1].kind, BlockKind::Paragraph);

    // A caret inside the marker reveals it for editing.
    let revealed = live(source, 1);
    assert!(revealed.lines[0].revealed);
    assert_eq!(&revealed.text[..7], "# Title");
    assert_eq!(revealed.source_to_display(3), 3);

    // Deeper levels and a heading with no text at all.
    let deep = live("### Deep\n", 0);
    assert_eq!(deep.lines[0].kind, BlockKind::Heading(3));
    assert!(deep.lines[0].revealed);
    let bare = live("##\n", 2);
    assert_eq!(bare.lines[0].kind, BlockKind::Heading(2));
    assert!(bare.lines[0].revealed);
    assert_eq!(bare.text, "##\n");
    let bare_hidden = live("##\n", 3);
    assert!(!bare_hidden.lines[0].revealed);
    assert_eq!(bare_hidden.text, "\n");
}

#[test]
fn inline_markers_hide_until_the_selection_touches_them() {
    let source = "a **bold** c\n";
    let hidden = live(source, 0);
    assert_eq!(hidden.text, "a bold c\n");
    let bold = hidden.lines[0]
        .runs
        .iter()
        .find(|run| {
            run.style
                == RunStyle {
                    bold: true,
                    ..Default::default()
                }
        })
        .expect("bold run");
    assert_eq!(&hidden.text[bold.range.clone()], "bold");
    assert_eq!(hidden.source_to_display(2), 2);
    assert_eq!(
        hidden.source_to_display(4),
        2,
        "content starts where `**` collapsed"
    );
    assert_eq!(hidden.source_to_display(5), 3);
    assert_eq!(hidden.display_to_source(2, Affinity::Before), 2);
    assert_eq!(hidden.display_to_source(2, Affinity::After), 4);
    // The trailing `**` collapses onto the end of `bold`.
    assert_eq!(hidden.display_to_source(6, Affinity::Before), 8);
    assert_eq!(hidden.display_to_source(6, Affinity::After), 10);
    assert_eq!(hidden.display_to_source(7, Affinity::After), 11);
    assert_eq!(hidden.text.len(), 9);

    let revealed = live(source, 4);
    assert_eq!(revealed.text, source);

    // Italic, strike, code and a link label.
    let mixed_source = "*i* ~~s~~ `c` [label](https://example.com)\n";
    let mixed = live(mixed_source, mixed_source.len());
    assert_eq!(mixed.text, "i s c label\n");
    let styles: Vec<RunStyle> = mixed.lines[0].runs.iter().map(|run| run.style).collect();
    assert!(styles.iter().any(|style| style.italic));
    assert!(styles.iter().any(|style| style.strike));
    assert!(styles.iter().any(|style| style.code));
    assert!(styles.iter().any(|style| style.link));

    // Escaped delimiters are literal text and never start a span.
    let escaped_source = "a \\*b\\* c\n";
    let escaped = live(escaped_source, escaped_source.len());
    assert!(escaped.lines[0].runs.iter().all(|run| !run.style.italic));
    assert_eq!(escaped.text, escaped_source);
}

#[test]
fn lists_quotes_and_code_fences_keep_editable_source() {
    let list = live("- one\n- two\n\n1. first\n", 0);
    assert_eq!(list.text, "\u{2022} one\n\u{2022} two\n\n1. first\n");
    assert_eq!(
        list.lines[0].kind,
        BlockKind::List {
            ordered: false,
            depth: 0
        }
    );
    assert_eq!(list.lines[2].kind, BlockKind::Paragraph);
    assert_eq!(
        list.lines[3].kind,
        BlockKind::List {
            ordered: true,
            depth: 0
        }
    );
    // The dash is replaced by the bullet, so editing the visible text cannot
    // leave half a marker behind.
    assert_eq!(list.display_to_source(0, Affinity::Before), 0);
    assert_eq!(list.display_to_source(1, Affinity::After), 2);
    assert_eq!(list.source_to_display(0), 0);
    // Inside the substituted bullet the source collapses to the display end,
    // so deleting the visible bullet cannot leave the source dash behind.
    assert_eq!(list.source_to_display(1), 4);
    assert_eq!(list.display_range_to_source(0..4), 0..2);

    let nested = live("  - child\n", 0);
    assert_eq!(
        nested.lines[0].kind,
        BlockKind::List {
            ordered: false,
            depth: 0
        }
    );
    assert_eq!(nested.text, "  \u{2022} child\n");
    assert_eq!(
        nested.source_to_display(2),
        2,
        "indentation is copied verbatim"
    );

    let quote = live("> quoted\n", 5);
    assert_eq!(quote.lines[0].kind, BlockKind::Quote { depth: 1 });
    assert_eq!(quote.text, "quoted\n");
    assert_eq!(quote.display_to_source(0, Affinity::After), 2);

    let code = live("```rs\nlet x = *y;\n```\n", 0);
    assert_eq!(code.lines[0].kind, BlockKind::Fence);
    assert_eq!(code.lines[1].kind, BlockKind::Code);
    assert_eq!(code.lines[2].kind, BlockKind::Fence);
    assert!(code.lines[1].runs.is_empty(), "code content stays literal");
    assert_eq!(code.text, "```rs\nlet x = *y;\n```\n");

    // After the closing fence the document is ordinary Markdown again.
    let after = live("```rs\nlet x = 1;\n```\n# again\n", 0);
    assert_eq!(after.lines[3].kind, BlockKind::Heading(1));
}

#[test]
fn display_and_source_ranges_round_trip_per_line() {
    let source = "# H\n\n- a **b**\n\n> q `c`\n\ntext\n";
    let projection = Projection::live(source, 0..0);
    for line in &projection.lines {
        let display = projection.source_range_to_display(line.source.clone());
        let back = projection.display_range_to_source(display);
        assert_eq!(
            back, line.source,
            "line {line:?} did not round trip through the display mapping"
        );
    }
    // A whole-document range maps back to the whole document.
    let display = projection.source_range_to_display(0..source.len());
    assert_eq!(
        projection.display_range_to_source(display.clone()),
        0..source.len()
    );
    assert_eq!(projection.text.len(), display.len());
}

#[test]
fn crlf_and_missing_terminators_keep_their_bytes() {
    let source = "# H\r\n\r\ntail";
    let projection = Projection::live(source, 100..100);
    assert_eq!(projection.text, "H\r\n\r\ntail");
    assert_eq!(projection.line_count(), 3);
    let tail = &projection.lines[2];
    assert_eq!(tail.source, 7..11);
    assert_eq!(projection.source_to_display(11), 9);
}

#[test]
fn trailing_and_repeated_delimiters_do_not_hang_or_over_consume() {
    for source in [
        "**unterminated\n",
        "`x\n",
        "[label](\n",
        "***\n",
        "~~~~\n",
        "- \n",
        ">\n",
        "```\n",
    ] {
        let projection = Projection::live(source, 0..0);
        assert!(!projection.text.is_empty() || source.is_empty());
        let line = &projection.lines[0];
        assert!(line.display.end <= projection.text.len());
    }
}

#[test]
fn composition_literal_range_keeps_half_typed_markdown_visible() {
    use breeze_reflow::markdown::ProjectionMode;
    let source = "a **bo";
    let literal = 2..source.len();
    let projection =
        Projection::build_with_literal(source, 0..0, ProjectionMode::Live, Some(literal));
    assert_eq!(projection.text, source, "the composing range stays literal");
    assert!(
        projection.lines[0].runs.is_empty(),
        "half-typed delimiters must not start a style run"
    );
    // Other lines keep their live projection; only the composing line is
    // literal, so a half-typed delimiter cannot restyle its neighbours.
    let settled =
        Projection::build_with_literal("x **b**\na **bo", 0..0, ProjectionMode::Live, Some(10..14));
    assert_eq!(settled.text, "x b\na **bo");
}
