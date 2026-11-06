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
    BufferPoint, Editor, EditorState, InputEvent, Redo, Rope, RopeExt as _, Undo,
};
use std::{
    cell::Cell,
    path::{Path, PathBuf},
    rc::Rc,
};

/// Which way a history request moves the editor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum HistoryStep {
    Undo,
    Redo,
}

impl HistoryStep {
    fn opposite(self) -> Self {
        match self {
            HistoryStep::Undo => HistoryStep::Redo,
            HistoryStep::Redo => HistoryStep::Undo,
        }
    }
}

/// Bounded number of text states the shadow history remembers.
const MAX_HISTORY_STATES: usize = 256;
/// Cap on full text comparisons for one history step.
const MAX_HISTORY_COMPARISONS: usize = 32;

/// A shadow of the states Kit's undo history can restore.
///
/// GPUI Kit exposes the Undo and Redo actions but not the depth behind them, so
/// Breeze records the text after every change and resolves a completed history
/// step by finding the recorded state the editor now shows. Resolution matters
/// where a counter cannot work: Kit coalesces typing into one transaction, so
/// the number of changes is not the number of steps, and it reuses old text, so
/// comparing against the document as loaded cannot prove the history is
/// exhausted either. A step that changes nothing proves the direction is
/// finished and clamps the cursor to that end.
///
/// States share Rope structure, so remembering recent revisions costs a path per
/// edit rather than a copy per revision. The window is bounded; once it drops
/// its oldest state, undo stays available until a dispatch proves otherwise.
#[derive(Debug)]
struct HistoryModel {
    states: Vec<Rope>,
    /// Index of the state the editor currently shows.
    cursor: usize,
    /// Older states were dropped from the window.
    truncated: bool,
}

impl HistoryModel {
    fn new(text: &Rope) -> Self {
        Self {
            states: vec![text.clone()],
            cursor: 0,
            truncated: false,
        }
    }

    fn can_undo(&self) -> bool {
        self.truncated || self.cursor > 0
    }

    fn can_redo(&self) -> bool {
        self.cursor + 1 < self.states.len()
    }

    /// Record the text a change produced, dropping the redo branch it replaced.
    fn record(&mut self, text: &Rope) {
        self.states.truncate(self.cursor + 1);
        self.states.push(text.clone());
        self.cursor = self.states.len() - 1;
        if self.states.len() > MAX_HISTORY_STATES {
            self.states.remove(0);
            self.cursor -= 1;
            self.truncated = true;
        }
    }

    /// Resolve a completed history step against the text now on screen.
    fn settle(&mut self, step: HistoryStep, before: &Rope, text: &Rope) {
        if before == text {
            // The editor did not move, so the direction had nothing left.
            match step {
                HistoryStep::Undo => {
                    self.cursor = 0;
                    self.truncated = false;
                }
                HistoryStep::Redo => self.cursor = self.states.len().saturating_sub(1),
            }
            return;
        }
        if let Some(index) = self
            .nearest_match(text, step)
            .or_else(|| self.nearest_match(text, step.opposite()))
        {
            self.cursor = index;
            return;
        }
        // The state is outside the remembered window; move one step in the
        // direction the editor was asked to take.
        match step {
            HistoryStep::Undo => {
                if self.cursor > 0 {
                    self.cursor -= 1;
                } else {
                    self.truncated = true;
                }
            }
            HistoryStep::Redo => {
                if self.cursor + 1 < self.states.len() {
                    self.cursor += 1;
                }
            }
        }
    }

    /// The recorded state matching `text` nearest the cursor in one direction.
    fn nearest_match(&self, text: &Rope, step: HistoryStep) -> Option<usize> {
        let mut comparisons = 0;
        for distance in 1..self.states.len() {
            let index = match step {
                HistoryStep::Undo => self.cursor.checked_sub(distance)?,
                HistoryStep::Redo => self.cursor + distance,
            };
            if index >= self.states.len() {
                return None;
            }
            let state = &self.states[index];
            // Length rejects almost every candidate for free; only equal-length
            // states are worth a content comparison.
            if state.len() != text.len() {
                continue;
            }
            comparisons += 1;
            if comparisons > MAX_HISTORY_COMPARISONS {
                return None;
            }
            if state == text {
                return Some(index);
            }
        }
        None
    }
}

/// The editing session behind the reader's content area.
pub(super) struct MarkdownEditor {
    state: Entity<EditorState>,
    focus: FocusHandle,
    /// The states Kit's undo history can restore, shadowed for the toolbar.
    history: HistoryModel,
    /// The source file this draft belongs to.
    path: PathBuf,
    /// Fingerprint of the bytes the draft was loaded from or last saved to.
    fingerprint: Option<String>,
    /// A publication is in flight; further saves wait for it to finish.
    saving: bool,
    /// The last publication failure, shown in the editor until it succeeds.
    save_error: Option<SharedString>,
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
    /// Lines the draft occupies, read from the rope on every refresh.
    lines: Cell<usize>,
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
        let initial = state.read(cx).text().clone();
        let subscription = cx.subscribe(&state, |this, _, event, cx| {
            if matches!(event, InputEvent::Change) {
                // Kit replays history without emitting Change, so every change
                // is new work: it becomes the newest reachable state and drops
                // the redo branch it replaced.
                let text = this.state.read(cx).text().clone();
                this.history.record(&text);
                this.sync(cx, true);
                cx.notify();
            }
        });
        let synced_len = saved.len();
        let lines = draft_lines(&initial);
        Self {
            state,
            focus,
            history: HistoryModel::new(&initial),
            path,
            fingerprint,
            saving: false,
            save_error: None,
            font_size,
            full_width,
            measured_width: Rc::new(Cell::new(0.)),
            pending_reveal: None,
            scrollbar: EditorScrollbarBridge::default(),
            saved,
            dirty: Cell::new(false),
            characters: Cell::new(characters),
            lines: Cell::new(lines),
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

    /// Lines in the draft, for the source status line.
    pub(super) fn lines(&self, cx: &App) -> usize {
        self.sync(cx, false);
        self.lines.get()
    }

    /// How much source the open draft holds.
    ///
    /// The reading surface counts rendered text while this surface counts
    /// source, so the two figures differ by the Markdown the preview strips;
    /// the status line reports lines rather than a second character count so
    /// the difference is not read as a defect.
    pub(super) fn status(&self, cx: &App) -> SharedString {
        tf!("{} lines", i18n::grouped_number(self.lines(cx))).into()
    }

    /// Whether the draft has a new revision worth publishing.
    ///
    /// A clean draft, a draft whose load failed and a draft already being
    /// published all leave Save disabled. The cached dirtiness is refreshed on
    /// every change, save and history step, so this read stays cheap enough for
    /// each frame that shows the toolbar.
    pub(super) fn can_save(&self, cx: &App) -> bool {
        self.sync(cx, false);
        self.notice.is_none() && self.fingerprint.is_some() && !self.saving && self.dirty.get()
    }

    /// Whether Kit has a step to take back.
    pub(super) fn can_undo(&self) -> bool {
        self.history.can_undo()
    }

    /// Whether an undone step can be replayed.
    pub(super) fn can_redo(&self) -> bool {
        self.history.can_redo()
    }

    #[cfg(test)]
    pub(super) fn is_saving(&self) -> bool {
        self.saving
    }

    /// Note a history request before Kit handles it.
    ///
    /// The text on screen is remembered so the completed step can be resolved
    /// afterwards; Kit performs the move itself.
    pub(super) fn note_history(&mut self, step: HistoryStep, cx: &mut Context<Self>) {
        let before = self.state.read(cx).text().clone();
        cx.spawn(async move |this, cx| {
            let _ = this.update(cx, |editor, cx| editor.settle_history(step, before, cx));
        })
        .detach();
    }

    /// Resolve a completed history step.
    ///
    /// A step emits no change event, so the draft's dirtiness has to be re-read
    /// here: undoing back to the checkpoint cleans the draft, and the toolbar's
    /// availability follows the state the editor landed on.
    fn settle_history(&mut self, step: HistoryStep, before: Rope, cx: &mut Context<Self>) {
        let text = self.state.read(cx).text().clone();
        self.history.settle(step, &before, &text);
        self.sync(cx, true);
        cx.notify();
    }

    /// Capture the draft for publication and mark the session busy.
    pub(super) fn begin_save(&mut self, cx: &mut Context<Self>) -> Option<MarkdownSaveRequest> {
        if !self.can_save(cx) {
            return None;
        }
        let fingerprint = self.fingerprint.clone()?;
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
        self.save_error = None;
        // Force the cached length/character count to be rebuilt against the
        // new checkpoint even when the text length did not change.
        self.synced_len.set(usize::MAX);
        self.sync(cx, true);
        cx.notify();
    }

    pub(super) fn mark_save_failed(&mut self, error: SharedString, cx: &mut Context<Self>) {
        self.saving = false;
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
            self.lines.set(draft_lines(text));
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

/// Lines the draft occupies, as a source editor counts them.
///
/// A final newline opens an empty line in the rope but not one a reader
/// counts, so the trailing line is dropped when it holds nothing. An empty
/// draft still reports its first line.
fn draft_lines(text: &Rope) -> usize {
    let lines = text.lines_len();
    if lines > 1 && text.line_len(lines - 1) == 0 {
        lines - 1
    } else {
        lines
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
        // Undo and Redo reach the editor as actions, whether they come from the
        // toolbar or from the platform's editing keys. Capturing them here sees
        // every step before Kit applies it, so the toolbar can report whether a
        // step is still available.
        let undo_view = cx.entity().downgrade();
        let redo_view = cx.entity().downgrade();
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
            .capture_action(move |_: &Undo, _, cx| {
                let _ = undo_view.update(cx, |editor, cx| {
                    editor.note_history(HistoryStep::Undo, cx);
                });
            })
            .capture_action(move |_: &Redo, _, cx| {
                let _ = redo_view.update(cx, |editor, cx| {
                    editor.note_history(HistoryStep::Redo, cx);
                });
            })
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

    fn rope(text: &str) -> Rope {
        Rope::from(text)
    }

    #[test]
    fn a_draft_reports_the_lines_it_occupies() {
        // A trailing newline is a line break, not a line the reader counts.
        assert_eq!(draft_lines(&rope("")), 1);
        assert_eq!(draft_lines(&rope("one")), 1);
        assert_eq!(draft_lines(&rope("one\n")), 1);
        assert_eq!(draft_lines(&rope("one\ntwo")), 2);
        assert_eq!(draft_lines(&rope("one\ntwo\n")), 2);
        assert_eq!(draft_lines(&rope("one\n\n")), 2);
        assert_eq!(draft_lines(&rope("\n")), 1);
    }

    #[test]
    fn history_resolves_the_state_kit_restored() {
        let mut history = HistoryModel::new(&rope("a"));
        assert!(!history.can_undo() && !history.can_redo());

        // Three edits, one of which returns the document to text it had before.
        // Resolving by text and then by distance keeps the repeated state apart
        // from the document's original text.
        history.record(&rope("ab"));
        history.record(&rope("a"));
        history.record(&rope("ac"));
        assert!(history.can_undo() && !history.can_redo());

        history.settle(HistoryStep::Undo, &rope("ac"), &rope("a"));
        assert_eq!(history.cursor, 2);
        assert!(history.can_undo() && history.can_redo());

        history.settle(HistoryStep::Undo, &rope("a"), &rope("ab"));
        assert_eq!(history.cursor, 1);
        history.settle(HistoryStep::Undo, &rope("ab"), &rope("a"));
        assert_eq!(history.cursor, 0);
        assert!(!history.can_undo() && history.can_redo());

        // A step with nothing left proves the direction is finished.
        history.settle(HistoryStep::Undo, &rope("a"), &rope("a"));
        assert!(!history.can_undo() && history.can_redo());
        history.settle(HistoryStep::Redo, &rope("a"), &rope("ab"));
        assert_eq!(history.cursor, 1);
        history.settle(HistoryStep::Redo, &rope("ab"), &rope("a"));
        assert_eq!(history.cursor, 2);
        history.settle(HistoryStep::Redo, &rope("a"), &rope("ac"));
        assert_eq!(history.cursor, 3);
        assert!(history.can_undo() && !history.can_redo());
        // Undoing everything then redoing to the tip also ends the redo branch.
        history.settle(HistoryStep::Redo, &rope("ac"), &rope("ac"));
        assert!(!history.can_redo());
    }

    #[test]
    fn a_new_edit_drops_the_redo_branch_it_replaced() {
        let mut history = HistoryModel::new(&rope("one"));
        history.record(&rope("one two"));
        history.settle(HistoryStep::Undo, &rope("one two"), &rope("one"));
        assert!(history.can_redo());
        history.record(&rope("one three"));
        assert!(history.can_undo() && !history.can_redo());
        assert_eq!(history.states.len(), 2);
    }

    #[test]
    fn the_bounded_window_keeps_undo_available_until_proven_otherwise() {
        let mut history = HistoryModel::new(&rope(""));
        for index in 0..MAX_HISTORY_STATES + 4 {
            history.record(&rope(&"x".repeat(index + 1)));
        }
        assert_eq!(history.states.len(), MAX_HISTORY_STATES);
        assert!(history.truncated && history.can_undo());
        // Walking past everything this window still remembers cannot disable a
        // direction the dropped states could still serve.
        let oldest = history.states[0].clone();
        history.settle(HistoryStep::Undo, &oldest, &oldest);
        assert!(!history.can_undo() && !history.truncated);
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
