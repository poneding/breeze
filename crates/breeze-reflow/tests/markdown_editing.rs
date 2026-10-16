//! P1 exit coverage: source preservation, transactions, history and positions.
use breeze_reflow::markdown::{
    Affinity, EditKind, EditLimits, EditSession, Position, Replacement, Selection, SourceBuffer,
};
use std::{
    ops::Range,
    sync::Arc,
    time::{Duration, Instant},
};

fn session(text: &str) -> EditSession {
    EditSession::new(SourceBuffer::from_bytes(text.as_bytes()).unwrap()).unwrap()
}

fn text_of(session: &EditSession) -> String {
    session.source().text(0..session.source().len()).unwrap()
}

fn apply(session: &mut EditSession, range: Range<usize>, insert: &str, caret: usize) {
    let revision = session.revision();
    session
        .apply(
            revision,
            vec![Replacement::new(range, insert)],
            Selection::caret(caret),
            EditKind::Atomic,
        )
        .unwrap();
}

const PRESERVED_DOCUMENTS: &[&str] = &[
    "",
    "\u{feff}# Title\r\n\r\nBody without a final newline",
    "# Front matter\n\n---\nkey: value\n---\n",
    "line one\rline two\nline three\r\nline four",
    "hard break  \nnext\ttabbed\n\n<!-- comment -->\n",
    "> quote\n\n    indented code\n\n[ref]: https://example.com\n",
    "# 标题 😀\n\n- 项目\n\n```md\n# not a heading\n```\n",
];

#[test]
fn documents_survive_load_and_noop_edits_exactly() {
    for original in PRESERVED_DOCUMENTS {
        let mut editor = session(original);
        assert_eq!(text_of(&editor), *original, "load changed {original:?}");
        assert!(!editor.dirty());

        let revision = editor.revision();
        let unchanged = editor
            .apply(
                revision,
                vec![Replacement::new(0..0, "")],
                Selection::caret(0),
                EditKind::Atomic,
            )
            .unwrap();
        assert!(!unchanged, "an empty replacement is not an edit");
        assert_eq!(
            editor.revision(),
            revision,
            "a no-op must not advance the revision"
        );
        assert_eq!(text_of(&editor), *original);
    }
}

#[test]
fn invalid_utf8_is_rejected_explicitly() {
    let error = SourceBuffer::from_bytes(b"ok \xff\xfe").unwrap_err();
    assert!(error.to_string().contains("UTF-8"), "{error:#}");
    let oversized = vec![b'a'; 16 * 1024 * 1024 + 1];
    assert!(SourceBuffer::from_bytes(&oversized).is_err());
}

#[test]
fn replacements_are_atomic_ordered_and_bounds_checked() {
    let mut editor = session("alpha beta gamma");
    let revision = editor.revision();
    let changed = editor
        .apply(
            revision,
            vec![
                Replacement::new(0..5, "ALPHA"),
                Replacement::new(11..16, "GAMMA"),
            ],
            Selection::caret(16),
            EditKind::Atomic,
        )
        .unwrap();
    assert!(changed);
    assert_eq!(text_of(&editor), "ALPHA beta GAMMA");
    assert_eq!(editor.revision(), revision + 1);

    // A stale revision, overlapping ranges and an interior UTF-8 offset all
    // leave the source and history untouched.
    let before = text_of(&editor);
    let undo_before = editor.undo_count();
    assert!(
        editor
            .apply(
                revision,
                vec![Replacement::new(0..1, "x")],
                Selection::caret(1),
                EditKind::Atomic,
            )
            .is_err()
    );
    let current = editor.revision();
    assert!(
        editor
            .apply(
                current,
                vec![Replacement::new(0..2, "x"), Replacement::new(1..3, "y"),],
                Selection::caret(1),
                EditKind::Atomic,
            )
            .is_err()
    );
    assert!(
        editor
            .apply(
                current,
                vec![Replacement::new(0..2, "x")],
                Selection::caret(1),
                EditKind::Atomic,
            )
            .is_ok()
    );
    // "AL" is two bytes, so offset 1 is inside a multi-byte character only for
    // non-ASCII text; verify on a CJK document too.
    assert_eq!(text_of(&editor), "xPHA beta GAMMA");
    assert!(editor.undo().unwrap());
    assert_eq!(text_of(&editor), before);
    assert_eq!(editor.undo_count(), undo_before);

    let mut unicode = session("你好世界");
    let revision = unicode.revision();
    assert!(
        unicode
            .apply(
                revision,
                vec![Replacement::new(1..3, "x")],
                Selection::caret(1),
                EditKind::Atomic,
            )
            .is_err(),
        "an offset inside a multi-byte character must be rejected"
    );
    assert_eq!(text_of(&unicode), "你好世界");
}

#[test]
fn undo_and_redo_restore_bytes_and_selection() {
    let mut editor = session("one\ntwo\nthree\n");
    editor
        .set_selection(Selection {
            anchor: Position {
                offset: 4,
                affinity: Affinity::After,
            },
            head: Position {
                offset: 7,
                affinity: Affinity::Before,
            },
            desired_x: Some(9.5),
        })
        .unwrap();
    let revision = editor.revision();
    editor
        .apply(
            revision,
            vec![Replacement::new(4..7, "TWO")],
            Selection {
                anchor: Position {
                    offset: 4,
                    affinity: Affinity::After,
                },
                head: Position {
                    offset: 7,
                    affinity: Affinity::Before,
                },
                desired_x: Some(12.5),
            },
            EditKind::Atomic,
        )
        .unwrap();
    assert_eq!(text_of(&editor), "one\nTWO\nthree\n");
    assert_eq!(editor.selection().range(), 4..7);
    assert_eq!(editor.selection().desired_x, Some(12.5));

    assert!(editor.undo().unwrap());
    assert_eq!(text_of(&editor), "one\ntwo\nthree\n");
    assert_eq!(
        editor.selection().range(),
        4..7,
        "undo restores the prior selection"
    );
    assert_eq!(editor.selection().desired_x, Some(9.5));
    assert_eq!(editor.undo_count(), 0);
    assert_eq!(editor.redo_count(), 1);

    assert!(editor.redo().unwrap());
    assert_eq!(text_of(&editor), "one\nTWO\nthree\n");
    assert_eq!(editor.selection().range(), 4..7);
    assert_eq!(editor.redo_count(), 0);

    // A new edit after undo clears forward history.
    assert!(editor.undo().unwrap());
    apply(&mut editor, 0..0, "# ", 2);
    assert_eq!(editor.redo_count(), 0);
    assert_eq!(text_of(&editor), "# one\ntwo\nthree\n");
}

#[test]
fn typing_coalesces_within_the_window_and_breaks_on_selection() {
    let start = Instant::now();
    let mut editor = session("");
    editor
        .apply_at(
            0,
            vec![Replacement::new(0..0, "你")],
            Selection::caret(3),
            EditKind::Typing,
            start,
        )
        .unwrap();
    editor
        .apply_at(
            editor.revision(),
            vec![Replacement::new(3..3, "好")],
            Selection::caret(6),
            EditKind::Typing,
            start + Duration::from_millis(200),
        )
        .unwrap();
    assert_eq!(text_of(&editor), "你好");
    assert_eq!(
        editor.undo_count(),
        1,
        "adjacent typing merges into one entry"
    );
    assert!(editor.undo().unwrap());
    assert_eq!(text_of(&editor), "");
    assert!(editor.redo().unwrap());
    assert_eq!(text_of(&editor), "你好");
    assert_eq!(editor.undo_count(), 1);

    // Slow typing in a fresh range starts a new entry.
    editor.set_selection(Selection::caret(0)).unwrap();
    editor
        .apply_at(
            editor.revision(),
            vec![Replacement::new(0..0, "a")],
            Selection::caret(1),
            EditKind::Typing,
            start + Duration::from_millis(2_000),
        )
        .unwrap();
    assert_eq!(editor.undo_count(), 2, "the coalescing window expired");

    // Moving the caret terminates the group.
    editor.set_selection(Selection::caret(0)).unwrap();
    editor
        .apply_at(
            editor.revision(),
            vec![Replacement::new(0..0, "b")],
            Selection::caret(1),
            EditKind::Typing,
            start + Duration::from_millis(2_100),
        )
        .unwrap();
    assert_eq!(editor.undo_count(), 3);

    // A non-typing transaction never merges into typing.
    let revision = editor.revision();
    editor
        .apply_at(
            revision,
            vec![Replacement::new(0..1, "B")],
            Selection::caret(1),
            EditKind::Typing,
            start + Duration::from_millis(2_150),
        )
        .unwrap();
    editor
        .apply_at(
            editor.revision(),
            vec![Replacement::new(0..0, "# ")],
            Selection::caret(2),
            EditKind::Atomic,
            start + Duration::from_millis(2_160),
        )
        .unwrap();
    assert_eq!(editor.undo_count(), 5);
}

#[test]
fn undo_to_the_saved_checkpoint_makes_the_document_clean() {
    let mut editor = session("draft\n");
    apply(&mut editor, 6..6, "more\n", 11);
    assert!(editor.dirty());

    let saved = editor.save_snapshot();
    assert!(editor.acknowledge_saved(saved.clone()));
    // Saving is not an edit: the revision and content are unchanged.
    assert_eq!(text_of(&editor), "draft\nmore\n");
    assert!(!editor.dirty());

    apply(&mut editor, 0..0, "# ", 2);
    assert!(editor.dirty());
    assert!(editor.undo().unwrap());
    assert_eq!(text_of(&editor), "draft\nmore\n");
    assert!(
        !editor.dirty(),
        "undoing to the saved checkpoint clears dirtiness"
    );

    // A stale acknowledgement cannot mark a newer revision clean.
    let stale = saved;
    apply(&mut editor, 0..0, "x", 1);
    assert!(editor.acknowledge_saved(stale));
    assert!(
        editor.dirty(),
        "an older receipt must not acknowledge newer content"
    );
}

#[test]
fn equal_content_reached_by_another_path_can_clear_dirtiness() {
    let mut editor = session("abc");
    let saved = editor.save_snapshot();
    assert!(editor.acknowledge_saved(saved.clone()));
    assert!(!editor.dirty());

    apply(&mut editor, 3..3, "d", 4);
    apply(&mut editor, 3..4, "", 3);
    assert_eq!(text_of(&editor), "abc");
    assert!(
        editor.dirty(),
        "a different history path stays dirty until proven equal"
    );

    let current = editor.snapshot();
    let equal = current.source().same_content(saved.source());
    assert!(editor.acknowledge_equality(&current, &saved, equal));
    assert!(!editor.dirty());

    // A stale equality result is rejected.
    apply(&mut editor, 0..0, "z", 1);
    assert!(editor.dirty());
    assert!(!editor.acknowledge_equality(&current, &saved, true));
    assert!(editor.dirty());
}

#[test]
fn history_budget_refuses_an_edit_before_mutating_the_source() {
    let mut editor = EditSession::with_limits(
        SourceBuffer::from_bytes(b"seed").unwrap(),
        EditLimits {
            source_bytes: 1024,
            history_bytes: 256,
            history_entries: 4,
        },
    )
    .unwrap();
    let before = text_of(&editor);
    // Fits the source limit but not the undo budget: refused before mutation.
    let payload = "x".repeat(600);
    let error = editor
        .apply(
            0,
            vec![Replacement::new(0..0, payload.clone())],
            Selection::caret(payload.len()),
            EditKind::Atomic,
        )
        .unwrap_err();
    assert!(error.to_string().contains("undo byte budget"), "{error:#}");
    assert_eq!(text_of(&editor), before);
    assert_eq!(editor.undo_count(), 0);
    assert_eq!(editor.revision(), 0);

    let oversized = "y".repeat(2048);
    let error = editor
        .apply(
            0,
            vec![Replacement::new(0..0, oversized.clone())],
            Selection::caret(oversized.len()),
            EditKind::Atomic,
        )
        .unwrap_err();
    assert!(error.to_string().contains("source limit"), "{error:#}");
    assert_eq!(text_of(&editor), before);

    // The entry limit evicts the oldest transaction but never the source.
    let mut editor = EditSession::with_limits(
        SourceBuffer::from_bytes(b"").unwrap(),
        EditLimits {
            source_bytes: 1024,
            history_bytes: 4096,
            history_entries: 2,
        },
    )
    .unwrap();
    for character in ["a", "b", "c"] {
        let offset = editor.source().len();
        editor
            .apply(
                editor.revision(),
                vec![Replacement::new(offset..offset, character)],
                Selection::caret(offset + 1),
                EditKind::Atomic,
            )
            .unwrap();
    }
    assert_eq!(text_of(&editor), "abc");
    assert_eq!(editor.undo_count(), 2);
    assert!(editor.undo().unwrap());
    assert!(editor.undo().unwrap());
    assert!(
        !editor.undo().unwrap(),
        "only the retained window is undoable"
    );
    assert_eq!(text_of(&editor), "a");
}

#[test]
fn utf16_and_grapheme_positions_round_trip() {
    let text = "a😀é🇨🇳e\u{301}👨‍👩‍👧‍👦z";
    let editor = session(text);
    let source = editor.source();
    assert_eq!(source.utf16_len(), text.encode_utf16().count());

    let mut boundaries = text
        .char_indices()
        .map(|(offset, _)| offset)
        .chain(std::iter::once(text.len()))
        .collect::<Vec<_>>();
    boundaries.dedup();
    for byte in boundaries.iter().copied() {
        let unit = source.byte_to_utf16(byte).unwrap();
        assert_eq!(source.utf16_to_byte(unit, Affinity::Before).unwrap(), byte);
        assert_eq!(source.utf16_to_byte(unit, Affinity::After).unwrap(), byte);
    }

    // A UTF-16 index inside a surrogate pair clips to its neighbouring boundary.
    let emoji_start = 1;
    let emoji_utf16 = source.byte_to_utf16(emoji_start).unwrap();
    assert_eq!(
        source
            .utf16_to_byte(emoji_utf16 + 1, Affinity::Before)
            .unwrap(),
        emoji_start
    );
    assert_eq!(
        source
            .utf16_to_byte(emoji_utf16 + 1, Affinity::After)
            .unwrap(),
        emoji_start + "😀".len()
    );

    // Grapheme movement walks whole clusters, not bytes or scalars.
    let mut offset = 0;
    let mut boundaries = vec![0];
    while offset < source.len() {
        offset = source.next_grapheme(offset).unwrap();
        boundaries.push(offset);
    }
    assert_eq!(
        boundaries,
        vec![0, 1, 5, 7, 15, 18, 43, 44],
        "grapheme clusters: a, emoji, e+acute, flag, e+combining, family, z"
    );
    for (index, boundary) in boundaries.iter().enumerate() {
        if index > 0 {
            assert_eq!(
                source.previous_grapheme(*boundary).unwrap(),
                boundaries[index - 1]
            );
        }
    }
    assert_eq!(source.previous_grapheme(0).unwrap(), 0);
    assert_eq!(source.next_grapheme(source.len()).unwrap(), source.len());
}

#[test]
fn line_endings_follow_surrounding_source() {
    let editor = session("a\r\nb\r\nc");
    let source = editor.source();
    assert_eq!(source.line_count(), 3);
    assert_eq!(source.newline_at(0).unwrap(), "\r\n");
    assert_eq!(source.newline_at(3).unwrap(), "\r\n");
    assert_eq!(source.newline_at(source.len()).unwrap(), "\r\n");

    let mixed = session("a\r\nb\nc");
    assert_eq!(mixed.source().newline_at(3).unwrap(), "\n");
    let lone = session("a");
    assert_eq!(lone.source().newline_at(0).unwrap(), "\n");
}

#[test]
fn generated_edit_sequences_restore_the_original_bytes() {
    let original = "# Title\n\n- item one\n- item two\n\n```rs\nfn main() {}\n```\n\n尾注\n";
    let mut editor = session(original);
    fn boundary(source: &SourceBuffer, candidate: usize, forward: bool) -> usize {
        let mut offset = candidate.min(source.len());
        while source.validate_offset(offset).is_err() {
            if forward {
                offset += 1;
            } else {
                offset -= 1;
            }
        }
        offset
    }

    let mut state = 0x2545_f491_4f6c_dd1du64;
    let mut applied = 0usize;
    for _ in 0..120 {
        state = state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        let length = editor.source().len();
        let start = boundary(editor.source(), (state as usize) % (length + 1), true);
        // Keep the document roughly stable: sometimes insert, sometimes delete a
        // short span, so the sequence keeps exercising both directions.
        let span = if state.is_multiple_of(3) {
            0
        } else {
            ((state >> 17) as usize % 6).min(length - start)
        };
        let end = boundary(editor.source(), start + span, false);
        let mut text = if end == start {
            "字"
        } else if state & 1 == 0 {
            ""
        } else {
            "字"
        };
        // Replacing text with an identical string is correctly a no-op; the
        // generator picks the other character so the sequence keeps moving.
        if editor.source().text(start..end).unwrap() == text {
            text = if text == "字" { "文" } else { "字" };
        }
        let outcome = editor
            .apply(
                editor.revision(),
                vec![Replacement::new(start..end, text)],
                Selection::caret(start + text.len()),
                EditKind::Atomic,
            )
            .unwrap();
        assert!(
            outcome,
            "generated edit {start}..{end} must change the source"
        );
        applied += 1;
    }
    assert_eq!(applied, 120);
    assert_ne!(text_of(&editor), original);
    let edited_revision = editor.revision();
    assert_eq!(edited_revision, applied as u64);

    while editor.undo().unwrap() {}
    assert_eq!(text_of(&editor), original, "undo restored every byte");
    assert_eq!(editor.revision(), edited_revision + applied as u64);

    while editor.redo().unwrap() {}
    assert_ne!(text_of(&editor), original);
    while editor.undo().unwrap() {}
    assert_eq!(text_of(&editor), original);
}

#[test]
fn snapshots_reject_results_from_another_session_or_revision() {
    let mut editor = session("alpha");
    let captured = editor.snapshot();
    assert!(editor.accepts(&captured));

    let mut other = session("alpha");
    assert_eq!(other.revision(), 0);
    assert!(
        !editor.accepts(&other.snapshot()),
        "sessions are distinguishable"
    );
    assert_ne!(editor.session_id(), other.session_id());

    apply(&mut other, 0..0, "x", 1);
    assert!(!editor.accepts(&other.snapshot()));

    apply(&mut editor, 0..0, "y", 1);
    assert!(
        !editor.accepts(&captured),
        "a later revision rejects the old snapshot"
    );
    assert!(editor.accepts(&editor.snapshot()));
}

#[test]
fn large_paste_within_budget_is_one_transaction() {
    let mut editor = session("start\n");
    let payload = format!("{}\n", "填充".repeat(4096));
    let revision = editor.revision();
    let caret = 0;
    let changed = editor
        .apply(
            revision,
            vec![Replacement::new(0..0, Arc::from(payload.as_str()))],
            Selection::caret(caret + payload.len()),
            EditKind::Atomic,
        )
        .unwrap();
    assert!(changed);
    assert_eq!(editor.source().len(), 6 + payload.len());
    assert_eq!(editor.undo_count(), 1);
    assert!(editor.undo().unwrap());
    assert_eq!(text_of(&editor), "start\n");
}
