//! Markdown source editing with syntax highlighting.
//!
//! The reading surface is the rendered preview; this surface is the editable
//! Markdown source. Editing uses GPUI Kit's code editor, which owns text,
//! selection, undo/redo, IME composition, soft wrap and the tree-sitter
//! Markdown highlighter. Breeze supplies the file, font size, dirty tracking
//! for the close guards and the status line.
//!
//! Markdown syntax highlighting comes from Kit's `tree-sitter-markdown`
//! feature; see `Cargo.toml` for the pinned component release.
use crate::reflow::MARKDOWN_HORIZONTAL_PADDING;
use crate::*;
use breeze_reflow::markdown::SourceBuffer;
use breeze_storage::markdown::{self, LoadedSource, Target};
use gpui_kit::base::{Scrollbar, ScrollbarHandle};
use gpui_kit::component::input::{
    BufferPoint, Editor, EditorState, InputEvent, Redo, RopeExt as _, Undo,
};
use std::{
    cell::Cell,
    path::{Path, PathBuf},
    rc::Rc,
};

/// The editing session behind the reader's content area.
pub(super) struct MarkdownEditor {
    state: Entity<EditorState>,
    focus: FocusHandle,
    /// The source file this draft belongs to.
    path: PathBuf,
    /// Fingerprint of the bytes the draft was loaded from or last saved to.
    fingerprint: Option<String>,
    /// A publication is in flight; further saves wait for it to finish.
    saving: bool,
    /// The last publication failure, shown in the editor until it succeeds.
    save_error: Option<SharedString>,
    /// Whether the current clean state came from a successful save.
    saved_successfully: bool,
    /// The reader's body size; the editor reflows when it changes.
    font_size: u8,
    /// Whether the source column spans the complete content panel.
    full_width: bool,
    /// Width of the full-height editing viewport from the previous frame.
    measured_width: Rc<Cell<f32>>,
    /// A source offset to bring into view after the editor has laid out.
    pending_reveal: Option<usize>,
    /// The outer scrollbar mirrors the editor's private scroll handle.
    scrollbar: EditorScrollbarBridge,
    /// Text as loaded, so undoing back to it makes the draft clean again.
    saved: String,
    /// Cached state, refreshed from the editor's text when it is read.
    dirty: Cell<bool>,
    characters: Cell<usize>,
    synced_len: Cell<usize>,
    notice: Option<SharedString>,
    /// Keeps the cached dirty state exact after every text change.
    _subscription: Subscription,
}

#[derive(Clone, Default)]
struct EditorScrollbarBridge {
    offset: Rc<Cell<f32>>,
    content_height: Rc<Cell<f32>>,
    pending_offset: Rc<Cell<Option<f32>>>,
}

impl ScrollbarHandle for EditorScrollbarBridge {
    fn viewport_bounds(&self) -> gpui_kit::Bounds<gpui_kit::Pixels> {
        gpui_kit::Bounds::default()
    }

    fn offset(&self) -> gpui_kit::Point<gpui_kit::Pixels> {
        gpui_kit::point(px(0.), px(self.offset.get()))
    }

    fn set_offset(&self, offset: gpui_kit::Point<gpui_kit::Pixels>) {
        self.offset.set(f32::from(offset.y));
        self.pending_offset.set(Some(f32::from(offset.y)));
    }

    fn content_size(&self) -> gpui_kit::Size<gpui_kit::Pixels> {
        gpui_kit::size(px(0.), px(self.content_height.get()))
    }
}

impl MarkdownEditor {
    pub(super) fn open(
        path: PathBuf,
        font_size: u8,
        full_width: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let (text, notice, fingerprint) = load_text(&path);
        let saved = text.clone();
        let characters = text.chars().count();
        let state = cx.new(|cx| {
            EditorState::new(window, cx)
                // Markdown highlighting, soft wrapped like prose. Line numbers
                // and folding belong to code; a note should read as a page.
                .language("markdown")
                .soft_wrap(true)
                .line_number(false)
                .searchable(true)
                // A prose editor should not reserve half a viewport of empty
                // scroll range beyond the last line.
                .scroll_beyond_last_line(Some(0))
                .default_value(text)
        });
        let focus = state.read(cx).focus_handle(cx);
        let subscription = cx.subscribe(&state, |this, _, event, cx| {
            if matches!(event, InputEvent::Change) {
                this.sync(cx, true);
                this.saved_successfully = false;
                cx.notify();
            }
        });
        let synced_len = saved.len();
        Self {
            state,
            focus,
            path,
            fingerprint,
            saving: false,
            save_error: None,
            saved_successfully: false,
            font_size,
            full_width,
            measured_width: Rc::new(Cell::new(0.)),
            pending_reveal: None,
            scrollbar: EditorScrollbarBridge::default(),
            saved,
            dirty: Cell::new(false),
            characters: Cell::new(characters),
            synced_len: Cell::new(synced_len),
            notice: notice.map(Into::into),
            _subscription: subscription,
        }
    }

    /// Follow the reader's font size.
    pub(super) fn set_font_size(&mut self, font_size: u8, cx: &mut Context<Self>) {
        if self.font_size == font_size {
            return;
        }
        self.font_size = font_size;
        cx.notify();
    }

    pub(super) fn set_full_width(&mut self, full_width: bool, cx: &mut Context<Self>) {
        if self.full_width == full_width {
            return;
        }
        self.full_width = full_width;
        cx.notify();
    }

    /// Bring `offset` to the top of the editor after its next layout.
    pub(super) fn reveal_source_offset(
        &mut self,
        offset: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let position = {
            let state = self.state.read(cx);
            state
                .text()
                .offset_to_position(offset.min(state.text().len()))
        };
        self.state.update(cx, |state, cx| {
            state.set_cursor_position(position, window, cx)
        });
        self.pending_reveal = Some(offset);
        cx.notify();
    }

    /// The source line at the top of the editor's visible viewport.
    pub(super) fn source_offset_at_viewport(&self, cx: &App) -> usize {
        let state = self.state.read(cx);
        let text = state.text();
        let Some(line_height) = state.line_height() else {
            return 0;
        };
        if let Some(visible) = state.visible_row_range() {
            return text.line_start_offset(visible.start).min(text.len());
        }
        let y = (-state.scroll_offset().y).max(px(0.));
        let requested_row = (y / line_height.max(px(1.))) as usize;
        let display_map = state.display_map();
        let row = requested_row.min(display_map.display_row_count().saturating_sub(1));
        let line = display_map.display_row_to_buffer_line(row);
        text.line_start_offset(line).min(text.len())
    }

    #[cfg(test)]
    pub(super) fn text_width(&self, cx: &App) -> Option<f32> {
        self.state
            .read(cx)
            .text_bounds()
            .map(|bounds| f32::from(bounds.size.width))
    }

    pub(super) fn focus_handle(&self) -> FocusHandle {
        self.focus.clone()
    }

    /// Whether the draft differs from the text as loaded.
    ///
    /// A composition and a committed keystroke both change the editor's text;
    /// reading the text here rather than tracking events keeps the answer
    /// correct while an input method is still composing. The check costs one
    /// byte-length read, plus one content comparison at equal lengths.
    pub(super) fn dirty(&self, cx: &App) -> bool {
        self.sync(cx, true);
        self.dirty.get()
    }

    /// The draft's text, for the reading preview.
    pub(super) fn source_text(&self, cx: &App) -> String {
        self.state.read(cx).value().to_string()
    }

    /// Characters in the draft, refreshed when its byte length changed.
    pub(super) fn characters(&self, cx: &App) -> usize {
        self.sync(cx, false);
        self.characters.get()
    }

    pub(super) fn status(&self, cx: &App) -> SharedString {
        self.sync(cx, false);
        let mut status = tr!("Markdown source").to_string();
        if self.saving {
            status.push_str(" · ");
            status.push_str(tr!("saving"));
        } else if self.dirty.get() {
            status.push_str(" · ");
            status.push_str(tr!("modified"));
        } else if self.saved_successfully {
            status.push_str(" · ");
            status.push_str(tr!("saved"));
        }
        status.into()
    }

    /// Whether Save can attempt a publication.
    ///
    /// The draft may be clean: the handler performs the exact comparison when
    /// it is invoked, so a stale toolbar render cannot prevent a real save.
    pub(super) fn can_save(&self) -> bool {
        self.notice.is_none() && self.fingerprint.is_some() && !self.saving
    }

    #[cfg(test)]
    pub(super) fn is_saving(&self) -> bool {
        self.saving
    }

    /// Capture the draft for publication and mark the session busy.
    pub(super) fn begin_save(&mut self, cx: &mut Context<Self>) -> Option<MarkdownSaveRequest> {
        if !self.can_save() {
            return None;
        }
        let fingerprint = self.fingerprint.clone()?;
        self.sync(cx, true);
        if !self.dirty.get() {
            return None;
        }
        let text: String = self.state.read(cx).value().to_string();
        if text.len() > markdown::MAX_SOURCE_BYTES {
            self.save_error = Some(tr!("This draft exceeds the 16 MiB Markdown save limit").into());
            cx.notify();
            return None;
        }
        self.saving = true;
        self.save_error = None;
        self.saved_successfully = false;
        cx.notify();
        Some(MarkdownSaveRequest {
            path: self.path.clone(),
            bytes: text.clone().into_bytes(),
            checkpoint: text,
            fingerprint,
        })
    }

    /// Acknowledge exactly the checkpoint that was published.
    ///
    /// Text typed while the write was in flight remains dirty, so a receipt
    /// never marks newer content as saved.
    pub(super) fn mark_saved(
        &mut self,
        checkpoint: String,
        fingerprint: String,
        cx: &mut Context<Self>,
    ) {
        self.saving = false;
        self.saved = checkpoint;
        self.fingerprint = Some(fingerprint);
        self.saved_successfully = true;
        self.save_error = None;
        // Force the cached length/character count to be rebuilt against the
        // new checkpoint even when the text length did not change.
        self.synced_len.set(usize::MAX);
        self.sync(cx, true);
        cx.notify();
    }

    pub(super) fn mark_save_failed(&mut self, error: SharedString, cx: &mut Context<Self>) {
        self.saving = false;
        self.saved_successfully = false;
        self.save_error = Some(error);
        cx.notify();
    }

    /// Refresh the cached count and dirtiness from the editor's text.
    ///
    /// `deep` additionally compares contents when the byte length is unchanged,
    /// which is what recognizes an equal-length replacement and an undo back to
    /// the saved checkpoint.
    fn sync(&self, cx: &App, deep: bool) {
        let len = {
            let text = self.state.read(cx).text();
            text.len()
        };
        if len == self.synced_len.get() {
            if deep {
                let text = self.state.read(cx).value();
                let dirty = len != self.saved.len() || text.as_ref() != self.saved;
                self.dirty.set(dirty);
            }
            return;
        }
        let text = self.state.read(cx).value();
        self.synced_len.set(len);
        self.characters.set(text.chars().count());
        self.dirty
            .set(len != self.saved.len() || text.as_ref() != self.saved);
    }
}

/// The snapshot and destination of one Markdown save.
pub(super) struct MarkdownSaveRequest {
    pub path: PathBuf,
    pub bytes: Vec<u8>,
    /// The text this snapshot represents, acknowledged only on success.
    pub checkpoint: String,
    /// Fingerprint the destination had when the snapshot was captured.
    pub fingerprint: String,
}

impl Breeze {
    /// Save the active Markdown draft back to its source file.
    ///
    /// The publication runs on the background executor and is conflict-checked
    /// against the bytes the draft was loaded from or last saved to. A receipt
    /// is applied to the editor entity that produced the snapshot, not to the
    /// tab index, so moving or closing tabs cannot acknowledge the wrong draft.
    pub(super) fn save_markdown(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let book = &self.books[self.active];
        if !book.markdown_editing {
            return;
        }
        let Some(editor) = book.markdown_edit.clone() else {
            return;
        };
        let focus = editor.read(cx).focus_handle();
        window.focus(&focus, cx);
        // A save attempt cancels an earlier discard warning; a failed save
        // leaves the draft dirty, so the next close warns again.
        self.pending_markdown_discard = None;
        let Some(request) = editor.update(cx, |editor, cx| editor.begin_save(cx)) else {
            cx.notify();
            return;
        };
        cx.notify();
        let MarkdownSaveRequest {
            path,
            bytes,
            checkpoint,
            fingerprint,
        } = request;
        let destination = path.clone();
        let work = cx.background_spawn(async move {
            markdown::publish(&destination, &bytes, &Target::Existing { fingerprint })
        });
        cx.spawn(async move |this, cx| {
            match work.await {
                Ok(fingerprint) => editor.update(cx, |editor, cx| {
                    editor.mark_saved(checkpoint, fingerprint, cx);
                }),
                Err(error) => editor.update(cx, |editor, cx| {
                    let message: SharedString =
                        tf!("Cannot save {}: {}", path.display(), format!("{error:#}")).into();
                    editor.mark_save_failed(message, cx);
                }),
            }
            let _ = this.update(cx, |_, cx| cx.notify());
        })
        .detach();
    }

    /// Undo or redo inside the focused Markdown editor.
    ///
    /// Kit owns the history; its Undo/Redo actions are handled by the editor's
    /// focused element. The toolbar button only routes them there.
    pub(super) fn markdown_history(
        &mut self,
        redo: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(editor) = self.books[self.active].markdown_edit.clone() else {
            return;
        };
        let focus = editor.read(cx).focus_handle();
        window.focus(&focus, cx);
        if redo {
            window.dispatch_action(Box::new(Redo), cx);
        } else {
            window.dispatch_action(Box::new(Undo), cx);
        }
        cx.notify();
    }
}

/// Read the draft, refusing the same limits the source model always had.
fn load_text(path: &Path) -> (String, Option<String>, Option<String>) {
    let failure = |error: anyhow::Error| {
        (
            String::new(),
            Some(tf!(
                "Cannot edit {}: {}",
                path.display(),
                format!("{error:#}")
            )),
            None,
        )
    };
    let LoadedSource { bytes, fingerprint } = match markdown::load(path) {
        Ok(loaded) => loaded,
        Err(error) => return failure(error),
    };
    match SourceBuffer::from_bytes(&bytes).and_then(|buffer| {
        let range = 0..buffer.len();
        buffer.text(range)
    }) {
        Ok(text) => (text, None, Some(fingerprint)),
        Err(error) => failure(error),
    }
}

impl Render for MarkdownEditor {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = Theme::global(cx).clone();
        let notice = self.notice.clone();
        let save_error = self.save_error.clone();
        let body = f32::from(self.font_size);
        let measured_width = self.measured_width.get();
        let side_padding = if self.full_width {
            0.
        } else {
            ((measured_width - crate::reflow::MARKDOWN_MAX_WIDTH) / 2.).max(0.)
        };
        let content_height = {
            let state = self.state.read(cx);
            if self.scrollbar.pending_offset.get().is_none() {
                self.scrollbar
                    .offset
                    .set(f32::from(state.scroll_offset().y));
            }
            let line_height = state.line_height().unwrap_or(px(1.));
            let rows = state.display_map().display_row_count().max(1);
            f32::from(line_height) * rows as f32
        };
        self.scrollbar.content_height.set(content_height);
        let editor_scrollbar = self.scrollbar.clone();
        let scrollbar_cover_color = theme.background;
        let measured = self.measured_width.clone();
        let view = cx.entity().downgrade();
        div()
            .on_children_prepainted(move |bounds, _, cx| {
                let width = bounds
                    .iter()
                    .map(|bounds| f32::from(bounds.size.width))
                    .fold(0., f32::max);
                if width > 0. && (width - measured.get()).abs() > 0.5 {
                    measured.set(width);
                    let _ = view.update(cx, |editor, cx| {
                        editor.apply_pending_reveal(cx);
                        editor.apply_scrollbar_offset(cx);
                        cx.notify();
                    });
                } else {
                    let _ = view.update(cx, |editor, cx| {
                        if editor.apply_pending_reveal(cx) | editor.apply_scrollbar_offset(cx) {
                            cx.notify();
                        }
                    });
                }
            })
            .id("markdown-editor")
            .test_support()
            .size_full()
            .min_h_0()
            .flex()
            .flex_col()
            .when_some(notice, |root, notice| {
                root.child(
                    div()
                        .px_3()
                        .py_1()
                        .text_size(px(12.))
                        .text_color(theme.danger)
                        .child(notice),
                )
            })
            .when_some(save_error, |root, error| {
                root.child(
                    div()
                        .id("markdown-save-error")
                        .test_support()
                        .px_3()
                        .py_1()
                        .text_size(px(12.))
                        .text_color(theme.danger)
                        .child(error),
                )
            })
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .w_full()
                    .relative()
                    .child(
                        Editor::new(&self.state)
                            .appearance(false)
                            .bordered(false)
                            .text_size(px(body))
                            .px(px(MARKDOWN_HORIZONTAL_PADDING + side_padding))
                            .h(relative(1.)),
                    )
                    // Hide Kit's editor scrollbar where the text measure ends,
                    // then draw the controlling scrollbar on the panel edge.
                    .child(
                        div()
                            .id("markdown-editor-scrollbar-cover")
                            .test_support()
                            .absolute()
                            .top_0()
                            .right(px(MARKDOWN_HORIZONTAL_PADDING + side_padding))
                            .bottom_0()
                            .w(px(MARKDOWN_HORIZONTAL_PADDING))
                            .child(
                                canvas(
                                    |_, _, _| (),
                                    move |bounds, _, window, _| {
                                        window.paint_layer(bounds, |window| {
                                            window.paint_quad(fill(bounds, scrollbar_cover_color));
                                        });
                                    },
                                )
                                .size_full(),
                            ),
                    )
                    .child(
                        div()
                            .id("markdown-editor-scrollbar")
                            .test_support()
                            .absolute()
                            .top_0()
                            .right_0()
                            .bottom_0()
                            .w(px(MARKDOWN_HORIZONTAL_PADDING))
                            .child(
                                Scrollbar::vertical(&editor_scrollbar)
                                    .id("markdown-editor-scrollbar-control")
                                    .viewport_from_layout()
                                    .scroll_size(size(px(1.), px(content_height))),
                            ),
                    ),
            )
    }
}

impl MarkdownEditor {
    fn apply_pending_reveal(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(offset) = self.pending_reveal.take() else {
            return false;
        };
        let Some(line_height) = self.state.read(cx).line_height() else {
            self.pending_reveal = Some(offset);
            return false;
        };
        let row = {
            let state = self.state.read(cx);
            let text = state.text();
            let point = text.offset_to_point(offset.min(text.len()));
            let display = state
                .display_map()
                .buffer_pos_to_display_pos(BufferPoint::new(point.row, point.column));
            display.row
        };
        self.state.update(cx, |state, cx| {
            state.set_scroll_offset(point(px(0.), -(line_height * row as f32)), cx);
        });
        true
    }

    fn apply_scrollbar_offset(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(y) = self.scrollbar.pending_offset.take() else {
            return false;
        };
        self.state.update(cx, |state, cx| {
            let x = state.scroll_offset().x;
            state.set_scroll_offset(point(x, px(y)), cx);
        });
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(bytes: &[u8]) -> (tempfile::TempDir, PathBuf) {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("draft.md");
        std::fs::write(&path, bytes).unwrap();
        (directory, path)
    }

    #[test]
    fn a_readable_file_loads_without_a_notice() {
        let (_directory, path) = write("# Title\n\nbody\n".as_bytes());
        let (text, notice, fingerprint) = load_text(&path);
        assert_eq!(text, "# Title\n\nbody\n");
        assert!(notice.is_none());
        assert!(fingerprint.is_some(), "the loaded bytes have an identity");
    }

    #[test]
    fn invalid_utf8_is_reported_with_the_file() {
        let (_directory, path) = write(b"ok \xff");
        let (text, notice, fingerprint) = load_text(&path);
        assert!(text.is_empty(), "no partial text is edited");
        let notice = notice.expect("an explicit error");
        assert!(notice.contains("draft.md"), "{notice}");
        assert!(fingerprint.is_none(), "an unreadable draft cannot be saved");
    }
}
