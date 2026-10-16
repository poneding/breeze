//! P2 native Markdown live-editing surface.
//!
//! This is the native feasibility harness for work package 25 phase P2: one
//! document session, one projected surface and one `EntityInputHandler`. It is
//! reachable only through the `BREEZE_MARKDOWN_EDITOR_REVIEW` development route
//! until P4 adds the save workflow and P5 enables normal entry, so the library
//! UI never exposes a partially implemented editor.
//!
//! Limits recorded for this phase: the projection is rebuilt on the UI thread
//! from a materialized source snapshot, and the whole document is laid out.
//! P3 replaces both with bounded background parsing and virtualized layout.
use crate::*;
// `crate::*` re-exports the PDF geometry `Pixels`; the surface uses GPUI's.
use breeze_reflow::markdown::{
    Affinity, BlockKind, EditSession, Position, Projection, ProjectionMode, Replacement, Selection,
    SourceBuffer,
};
use gpui_kit::{Bounds, Pixels, Point, point, px, relative, size};
use std::{
    ops::Range,
    path::{Path, PathBuf},
    rc::Rc,
};

const BODY_SIZE: f32 = 16.;
const CODE_SIZE: f32 = 13.;
const MEASURE: f32 = 760.;
const PAD_X: f32 = 24.;
const PAD_Y: f32 = 16.;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum EditView {
    Live,
    Source,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct LayoutKey {
    revision: u64,
    view: EditView,
    width: u32,
}

struct Wrapped {
    display: Range<usize>,
    shaped: ShapedLine,
    x: f32,
}

struct VisualLine {
    kind: BlockKind,
    top: f32,
    height: f32,
    wrapped: Vec<Wrapped>,
}

struct Layout {
    bounds: Bounds<Pixels>,
    key: LayoutKey,
    scroll: f32,
    content_height: f32,
    lines: Vec<VisualLine>,
}

/// The document editing session behind the native surface.
pub(super) struct MarkdownEditor {
    session: EditSession,
    view: EditView,
    focus: FocusHandle,
    scroll: f32,
    projection: Arc<Projection>,
    layout: Option<Rc<Layout>>,
    notice: Option<SharedString>,
}

impl MarkdownEditor {
    pub(super) fn view(&self) -> EditView {
        self.view
    }

    pub(super) fn set_view(&mut self, view: EditView, cx: &mut Context<Self>) {
        self.toggle_view(view, cx);
    }

    pub(super) fn focus_handle(&self) -> FocusHandle {
        self.focus.clone()
    }

    pub(super) fn dirty(&self) -> bool {
        self.session.dirty()
    }

    pub(super) fn open(path: PathBuf, cx: &mut Context<Self>) -> Self {
        let (session, notice) = load_session(&path);
        let mut editor = Self {
            session,
            view: EditView::Live,
            focus: cx.focus_handle(),
            scroll: 0.,
            projection: Arc::new(Projection::live("", 0..0)),
            layout: None,
            notice: notice.map(Into::into),
        };
        editor.refresh_projection();
        editor
    }

    fn source_text(&self) -> String {
        let source = self.session.source();
        source.text(0..source.len()).unwrap_or_default()
    }

    /// Rebuild the projection from the current source, selection and mode.
    ///
    /// P2 materializes the document and rebuilds the whole projection on every
    /// edit; P3 replaces this with background parsing and incremental
    /// invalidation.
    fn refresh_projection(&mut self) {
        let source = self.source_text();
        let selection = self.session.selection().range();
        let literal = self.session.composition_range();
        self.projection = Arc::new(Projection::build_with_literal(
            &source,
            selection,
            match self.view {
                EditView::Live => ProjectionMode::Live,
                EditView::Source => ProjectionMode::Source,
            },
            literal,
        ));
        self.layout = None;
    }

    /// Commit an in-flight composition using the text currently in the source.
    ///
    /// The platform normally unmarks text before another edit arrives. Some
    /// paths (focus changes, a new composition starting elsewhere) do not, so
    /// closing the open composition here keeps one history entry per composed
    /// run instead of merging unrelated edits into it.
    fn finish_composition(&mut self) -> bool {
        let Some(range) = self.session.composition_range() else {
            return false;
        };
        let text = self.session.source().text(range).unwrap_or_default();
        let caret = self.session.selection().head.offset;
        self.session
            .commit_composition(&text, Selection::caret(caret))
            .unwrap_or(false)
    }

    fn display_selection(&self) -> Range<usize> {
        self.projection
            .source_range_to_display(self.session.selection().range())
    }

    fn update_selection(&mut self, selection: Selection, cx: &mut Context<Self>) {
        if self.session.set_selection(selection).is_ok() {
            self.refresh_projection();
            cx.notify();
        }
    }

    fn set_caret(&mut self, source_offset: usize, extend: bool, cx: &mut Context<Self>) {
        self.finish_composition();

        let current = self.session.selection();
        let selection = if extend {
            Selection {
                anchor: current.anchor,
                head: Position {
                    offset: source_offset,
                    affinity: Affinity::After,
                },
                desired_x: current.desired_x,
            }
        } else {
            Selection::caret(source_offset)
        };
        self.update_selection(selection, cx);
    }

    fn toggle_view(&mut self, view: EditView, cx: &mut Context<Self>) {
        self.finish_composition();

        if self.view == view {
            return;
        }
        self.view = view;
        // Switching projection is not an edit: bytes, history and selection
        // are unchanged.
        self.refresh_projection();
        cx.notify();
    }

    fn undo(&mut self, cx: &mut Context<Self>) {
        self.finish_composition();
        if self.session.undo().is_ok() {
            self.refresh_projection();
            cx.notify();
        }
    }

    fn redo(&mut self, cx: &mut Context<Self>) {
        self.finish_composition();
        if self.session.redo().is_ok() {
            self.refresh_projection();
            cx.notify();
        }
    }

    /// Move the caret within the source text.
    fn move_caret(&mut self, motion: Motion, extend: bool, cx: &mut Context<Self>) {
        self.finish_composition();
        let selection = self.session.selection();
        let offset = selection.head.offset;
        let source = self.session.source();
        let target = match motion {
            Motion::Left => source.previous_grapheme(offset).unwrap_or(offset),
            Motion::Right => source.next_grapheme(offset).unwrap_or(offset),
            Motion::Up | Motion::Down => match self.vertical_target(motion == Motion::Up) {
                Some(offset) => offset,
                None => return,
            },
            Motion::Home => source
                .line_range(source.line_at(offset).unwrap_or(0))
                .map(|range| range.start)
                .unwrap_or(offset),
            Motion::End => source
                .line_range(source.line_at(offset).unwrap_or(0))
                .map(|range| {
                    // Keep the line terminator out of the caret position.
                    let mut end = range.end;
                    let text = source.text(range).unwrap_or_default();
                    if text.ends_with("\r\n") {
                        end -= 2;
                    } else if text.ends_with('\n') || text.ends_with('\r') {
                        end -= 1;
                    }
                    end
                })
                .unwrap_or(offset),
        };
        self.set_caret(target, extend, cx);
    }

    /// The source offset one visual line above or below the caret.
    fn vertical_target(&self, up: bool) -> Option<usize> {
        let layout = self.layout.as_ref()?;
        let display = self
            .projection
            .source_to_display(self.session.selection().head.offset);
        let (line_index, wrap_index, x) = locate_display(layout, display)?;
        let x = f32::from(x);
        if up {
            if wrap_index > 0 {
                let wrapped = &layout.lines[line_index].wrapped[wrap_index - 1];
                return Some(self.display_hit(wrapped, x));
            }
            let previous = layout.lines.get(line_index.checked_sub(1)?)?;
            let wrapped = previous.wrapped.last()?;
            return Some(self.display_hit(wrapped, x));
        }
        let line = &layout.lines[line_index];
        if wrap_index + 1 < line.wrapped.len() {
            let wrapped = &line.wrapped[wrap_index + 1];
            return Some(self.display_hit(wrapped, x));
        }
        let next = layout.lines.get(line_index + 1)?;
        let wrapped = next.wrapped.first()?;
        Some(self.display_hit(wrapped, x))
    }

    fn display_hit(&self, wrapped: &Wrapped, x: f32) -> usize {
        let local = wrapped.shaped.closest_index_for_x(px(x - wrapped.x));
        self.projection
            .display_to_source(wrapped.display.start + local, Affinity::After)
    }

    fn scroll_by(&mut self, delta: f32, cx: &mut Context<Self>) {
        let content = self
            .layout
            .as_ref()
            .map(|layout| layout.content_height)
            .unwrap_or(0.);
        let viewport = self
            .layout
            .as_ref()
            .map(|layout| f32::from(layout.bounds.size.height))
            .unwrap_or(0.);
        self.scroll = (self.scroll + delta).clamp(0., (content - viewport).max(0.));
        cx.notify();
    }

    /// Returns whether the key was part of document editing. Unhandled keys
    /// keep bubbling so reading shortcuts are unaffected outside the editor.
    fn key_down(&mut self, event: &KeyDownEvent, _: &mut Window, cx: &mut Context<Self>) -> bool {
        let shift = event.keystroke.modifiers.shift;
        let command = event.keystroke.modifiers.secondary();
        let key = event.keystroke.key.as_str();
        match (key, command) {
            ("left", _) if !command => self.move_caret(Motion::Left, shift, cx),
            ("right", _) if !command => self.move_caret(Motion::Right, shift, cx),
            ("up", _) if !command => self.move_caret(Motion::Up, shift, cx),
            ("down", _) if !command => self.move_caret(Motion::Down, shift, cx),
            ("home", _) => self.move_caret(Motion::Home, shift, cx),
            ("end", _) => self.move_caret(Motion::End, shift, cx),
            ("z", true) if shift => self.redo(cx),
            ("z", true) => self.undo(cx),
            ("backspace", _) => self.delete_backwards(cx),
            ("delete", _) => self.delete_forwards(cx),
            ("enter", _) => self.insert_line_break(cx),
            _ => return false,
        }
        cx.stop_propagation();
        true
    }

    fn delete_backwards(&mut self, cx: &mut Context<Self>) {
        let selection = self.session.selection();
        if selection.is_empty() {
            let offset = selection.head.offset;
            let Some(previous) = self.session.source().previous_grapheme(offset).ok() else {
                return;
            };
            if previous == offset {
                return;
            }
            self.replace_source(previous..offset, "", previous, cx);
            return;
        }
        let range = selection.range();
        self.replace_source(range.clone(), "", range.start, cx);
    }

    fn delete_forwards(&mut self, cx: &mut Context<Self>) {
        let selection = self.session.selection();
        if selection.is_empty() {
            let offset = selection.head.offset;
            let Some(next) = self.session.source().next_grapheme(offset).ok() else {
                return;
            };
            if next == offset {
                return;
            }
            self.replace_source(offset..next, "", offset, cx);
            return;
        }
        let range = selection.range();
        self.replace_source(range.clone(), "", range.start, cx);
    }

    fn insert_line_break(&mut self, cx: &mut Context<Self>) {
        let selection = self.session.selection();
        let range = selection.range();
        let source = self.session.source();
        let newline = source.newline_at(range.start).unwrap_or("\n");
        let newline = newline.to_owned();
        self.replace_source(range.clone(), &newline, range.start + newline.len(), cx);
    }

    /// Apply a source replacement and restore the caret.
    fn replace_source(
        &mut self,
        range: Range<usize>,
        text: &str,
        caret: usize,
        cx: &mut Context<Self>,
    ) {
        let revision = self.session.revision();
        if self
            .session
            .apply(
                revision,
                vec![Replacement::new(range, text)],
                Selection::caret(caret),
                breeze_reflow::markdown::EditKind::Atomic,
            )
            .is_err()
        {
            self.notice = Some(tr!("This edit exceeds the document limit").into());
        }
        self.refresh_projection();
        cx.notify();
    }

    /// Draft state reported in the reader's shared status bar.
    pub(super) fn status(&self) -> SharedString {
        let mode = match self.view {
            EditView::Live => tr!("Live view"),
            EditView::Source => tr!("Source view"),
        };
        let mut status = format!("{} · {}", mode, tf!("{} undo", self.session.undo_count()));
        if self.session.dirty() {
            status.push_str(" · ");
            status.push_str(tr!("modified"));
        }
        if self.session.composition_range().is_some() {
            status.push_str(" · ");
            status.push_str(tr!("composing"));
        }
        status.into()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Motion {
    Left,
    Right,
    Up,
    Down,
    Home,
    End,
}

fn load_session(path: &Path) -> (EditSession, Option<String>) {
    match std::fs::read(path) {
        Ok(bytes) => match SourceBuffer::from_bytes(&bytes).and_then(EditSession::new) {
            Ok(session) => (session, None),
            Err(error) => (
                EditSession::empty(),
                Some(tf!(
                    "Cannot edit {}: {}",
                    path.display(),
                    format!("{error:#}")
                )),
            ),
        },
        Err(error) => (
            EditSession::empty(),
            Some(tf!(
                "Cannot edit {}: {}",
                path.display(),
                format!("{error:#}")
            )),
        ),
    }
}

/// The visual line and wrapped segment containing a display offset.
fn locate_display(layout: &Layout, display: usize) -> Option<(usize, usize, Pixels)> {
    for (line_index, line) in layout.lines.iter().enumerate() {
        for (wrap_index, wrapped) in line.wrapped.iter().enumerate() {
            if wrapped.display.start <= display && display <= wrapped.display.end {
                let x = wrapped.shaped.x_for_index(display - wrapped.display.start);
                return Some((line_index, wrap_index, x + px(wrapped.x)));
            }
        }
    }
    None
}

impl EntityInputHandler for MarkdownEditor {
    fn text_for_range(
        &mut self,
        range: Range<usize>,
        adjusted_range: &mut Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        let text = &self.projection.text;
        let start = utf16_to_byte(text, range.start);
        let end = utf16_to_byte(text, range.end).max(start);
        *adjusted_range = Some(range);
        Some(text[start..end].to_string())
    }

    fn selected_text_range(
        &mut self,
        _: bool,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        let display = self.display_selection();
        let text = &self.projection.text;
        Some(UTF16Selection {
            range: byte_to_utf16(text, display.start)..byte_to_utf16(text, display.end),
            reversed: self.session.selection().reversed(),
        })
    }

    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        let range = self.session.composition_range()?;
        let display = self.projection.source_range_to_display(range);
        let text = &self.projection.text;
        Some(byte_to_utf16(text, display.start)..byte_to_utf16(text, display.end))
    }

    fn unmark_text(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        // The platform marks text as no longer composing. An empty preview is a
        // cancellation; otherwise the visible preview becomes one edit.
        let Some(range) = self.session.composition_range() else {
            return;
        };
        let text = self.session.source().text(range).unwrap_or_default();
        if text.is_empty() {
            self.session.cancel_composition();
        } else {
            let caret = self.session.selection().head.offset;
            let _ = self
                .session
                .commit_composition(&text, Selection::caret(caret));
        }
        self.refresh_projection();
        cx.notify();
    }

    fn replace_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.session.composition_range().is_some() {
            if text.is_empty() {
                // An empty final replacement is the platform cancelling the
                // marked text; restore instead of deleting what it replaced.
                self.session.cancel_composition();
            } else {
                let caret = self
                    .session
                    .composition_range()
                    .map_or(0, |range| range.start)
                    + text.len();
                let _ = self
                    .session
                    .commit_composition(text, Selection::caret(caret));
            }
            self.refresh_projection();
            cx.notify();
            return;
        }
        let display = match range {
            Some(range) => self.display_range_from_utf16(range),
            None => self.display_selection(),
        };
        let source = self.projection.display_range_to_source(display);
        let caret = source.start + text.len();
        self.replace_source(source, text, caret, cx);
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        new_text: &str,
        new_selected_range: Option<Range<usize>>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let incoming = match range.as_ref() {
            Some(range) => self.display_range_from_utf16(range.clone()),
            None => self.display_selection(),
        };
        let incoming = self.projection.display_range_to_source(incoming);
        if let Some(active) = self.session.composition_range()
            && (incoming.start < active.start || incoming.end > active.end)
        {
            // The platform started a new composition without unmarking the old
            // one; close the previous run so it keeps its own history entry.
            self.finish_composition();
        }
        if self.session.composition_range().is_none() {
            let _ = self
                .session
                .set_selection(Selection::between(incoming.start, incoming.end));
            if self.session.begin_composition().is_err() {
                return;
            }
            self.refresh_projection();
        }
        let Some(current) = self.session.composition_range() else {
            return;
        };
        let selected = new_selected_range
            .as_ref()
            .map(|range| {
                let text = new_text;
                utf16_to_byte(text, range.start)..utf16_to_byte(text, range.end)
            })
            .unwrap_or(new_text.len()..new_text.len());
        let caret = current.start + selected.end.min(new_text.len());
        let _ = self
            .session
            .apply_composition(new_text, Selection::caret(caret));
        self.refresh_projection();
        cx.notify();
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        element_bounds: Bounds<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let layout = self.layout.clone()?;
        let display = utf16_to_byte(&self.projection.text, range_utf16.start);
        let (line_index, _, x) = locate_display(&layout, display)?;
        let line = &layout.lines[line_index];
        let origin = point(
            element_bounds.origin.x + px(PAD_X) + x,
            element_bounds.origin.y + px(PAD_Y + line.top - layout.scroll),
        );
        Some(Bounds {
            origin,
            size: size(px(1.), px(line.height)),
        })
    }

    fn character_index_for_point(
        &mut self,
        point: Point<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<usize> {
        let layout = self.layout.clone()?;
        let display = hit_test(&layout, point)?;
        Some(byte_to_utf16(&self.projection.text, display))
    }

    fn text_length_utf16(&mut self, _: &mut Window, _: &mut Context<Self>) -> Option<usize> {
        Some(self.projection.text.encode_utf16().count())
    }

    fn set_selected_text_range(
        &mut self,
        range: Range<usize>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let display = self.display_range_from_utf16(range);
        let source = self.projection.display_range_to_source(display);
        self.update_selection(Selection::between(source.start, source.end), cx);
    }

    fn accepts_text_input(&self, _: &mut Window, _: &mut Context<Self>) -> bool {
        true
    }
}

impl MarkdownEditor {
    fn display_range_from_utf16(&self, range: Range<usize>) -> Range<usize> {
        let text = &self.projection.text;
        let start = utf16_to_byte(text, range.start);
        let end = utf16_to_byte(text, range.end).max(start);
        start..end
    }

    /// Build and cache the shaped visual lines for the current surface size.
    fn build_layout(
        &mut self,
        bounds: Bounds<Pixels>,
        window: &mut Window,
        cx: &mut App,
    ) -> Rc<Layout> {
        let key = LayoutKey {
            revision: self.session.revision(),
            view: self.view,
            width: f32::from(bounds.size.width).to_bits(),
        };
        if let Some(layout) = self.layout.as_ref()
            && layout.key == key
            && layout.bounds.size == bounds.size
        {
            return layout.clone();
        }
        let theme = Theme::global(cx);
        let body_font = font(theme.font_family.clone());
        let mono_font = font(theme.mono_font_family.clone());
        let width = f32::from(bounds.size.width) - PAD_X * 2.;
        let measure = width.clamp(80., MEASURE);
        let mut lines = Vec::with_capacity(self.projection.lines.len());
        let mut top = 0.;
        for line in self.projection.lines.iter() {
            let (font_size, indent, base_font) = match line.kind {
                BlockKind::Heading(level) => {
                    let size = match level {
                        1 => 28.,
                        2 => 23.,
                        3 => 20.,
                        _ => BODY_SIZE,
                    };
                    (size, 0., body_font.clone())
                }
                BlockKind::Quote { depth } => (BODY_SIZE, 16. * depth as f32, body_font.clone()),
                BlockKind::List { depth, .. } => (BODY_SIZE, 16. * depth as f32, body_font.clone()),
                BlockKind::Fence | BlockKind::Code => (CODE_SIZE, 12., mono_font.clone()),
                BlockKind::Paragraph => (BODY_SIZE, 0., body_font.clone()),
            };
            let text = &self.projection.text[line.display.clone()];
            let runs = build_runs(theme, text, &line.runs, base_font.clone(), font_size);
            let shaped = window.text_system().shape_line(
                text.to_string().into(),
                px(font_size),
                &runs,
                None,
            );
            let wrapped = wrap_line(&shaped, px(measure - indent));
            let mut wrapped = wrapped
                .into_iter()
                .map(|(fragment, shaped)| Wrapped {
                    x: indent + fragment.start_offset,
                    // Wrapping works in line-local bytes; selection, hit tests
                    // and the mapping all use document display offsets.
                    display: line.display.start + fragment.range.start
                        ..line.display.start + fragment.range.end,
                    shaped,
                })
                .collect::<Vec<_>>();
            if wrapped.is_empty() {
                // An empty display line still needs a caret position.
                let empty = window
                    .text_system()
                    .shape_line("".into(), px(font_size), &runs, None);
                wrapped.push(Wrapped {
                    x: indent,
                    display: line.display.start..line.display.start,
                    shaped: empty,
                });
            }
            let height = (font_size * 1.55).max(font_size + 6.);
            lines.push(VisualLine {
                kind: line.kind.clone(),
                top,
                height,
                wrapped,
            });
            top += height;
        }
        let content_height = top + PAD_Y * 2.;
        let viewport = f32::from(bounds.size.height);
        let scroll = self.scroll.clamp(0., (content_height - viewport).max(0.));
        let layout = Rc::new(Layout {
            bounds,
            key,
            scroll,
            content_height,
            lines,
        });
        self.scroll = scroll;
        self.layout = Some(layout.clone());
        layout
    }

    fn paint(&mut self, bounds: Bounds<Pixels>, window: &mut Window, cx: &mut Context<Self>) {
        let Some(layout) = self.layout.clone() else {
            return;
        };
        let theme = Theme::global(cx).clone();
        let text_color = theme.foreground;
        let selection_color = theme.selection;
        let caret_color = theme.primary;
        let entity = cx.entity();
        let focus = self.focus.clone();
        let selection = self.display_selection();
        let display_text = self.projection.text.clone();

        for line in &layout.lines {
            let y = bounds.origin.y + px(PAD_Y + line.top - layout.scroll);
            if y + px(line.height) < bounds.origin.y || y > bounds.bottom() {
                continue;
            }
            // Block decoration: a quote bar or a list baseline keeps the
            // structure visible without falling back to a second renderer.
            match line.kind {
                BlockKind::Quote { .. } => {
                    window.paint_quad(fill(
                        Bounds {
                            origin: point(bounds.origin.x + px(8.), y),
                            size: size(px(3.), px(line.height - 2.)),
                        },
                        theme.border,
                    ));
                }
                BlockKind::Code | BlockKind::Fence => {
                    window.paint_quad(fill(
                        Bounds {
                            origin: point(bounds.origin.x + px(PAD_X) - px(6.), y),
                            size: size(px(measure_width(bounds) + 12.), px(line.height)),
                        },
                        theme.muted.opacity(0.35),
                    ));
                }
                _ => {}
            }
            for wrapped in &line.wrapped {
                let origin = point(bounds.origin.x + px(PAD_X + wrapped.x), y);
                let start = wrapped.display.start.max(selection.start);
                let end = wrapped.display.end.min(selection.end);
                if start < end {
                    let x1 = wrapped.shaped.x_for_index(start - wrapped.display.start);
                    let x2 = wrapped.shaped.x_for_index(end - wrapped.display.start);
                    window.paint_quad(fill(
                        Bounds {
                            origin: point(origin.x + x1, y),
                            size: size(x2 - x1, px(line.height)),
                        },
                        selection_color,
                    ));
                }
                let _ = wrapped.shaped.paint(
                    origin,
                    px(line.height),
                    TextAlign::Left,
                    None,
                    window,
                    cx,
                );
            }
            if selection.is_empty() {
                let caret = selection.start;
                for wrapped in &line.wrapped {
                    if wrapped.display.start <= caret && caret <= wrapped.display.end {
                        let x = wrapped.shaped.x_for_index(caret - wrapped.display.start);
                        window.paint_quad(fill(
                            Bounds {
                                origin: point(bounds.origin.x + px(PAD_X + wrapped.x) + x, y),
                                size: size(px(1.5), px(line.height)),
                            },
                            caret_color,
                        ));
                    }
                }
            }
        }
        let _ = (text_color, display_text);

        window.handle_input(
            &self.focus,
            ElementInputHandler::new(bounds, entity.clone()),
            cx,
        );
        window.on_mouse_event({
            let entity = entity.clone();
            let focus = focus.clone();
            move |event: &MouseDownEvent, phase, window, cx| {
                if phase != DispatchPhase::Bubble || !bounds.contains(&event.position) {
                    return;
                }
                window.focus(&focus, cx);
                let position = event.position;
                let extend = event.modifiers.shift;
                entity.update(cx, |editor, cx| editor.pointer_down(position, extend, cx));
            }
        });
        window.on_mouse_event({
            let entity = entity.clone();
            move |event: &MouseMoveEvent, phase, _, cx| {
                if phase != DispatchPhase::Bubble || !event.dragging() {
                    return;
                }
                let position = event.position;
                entity.update(cx, |editor, cx| editor.pointer_drag(position, cx));
            }
        });
        window.on_mouse_event(move |event: &ScrollWheelEvent, phase, _, cx| {
            if phase != DispatchPhase::Bubble || !bounds.contains(&event.position) {
                return;
            }
            let delta = match event.delta {
                ScrollDelta::Pixels(delta) => f32::from(delta.y),
                ScrollDelta::Lines(delta) => delta.y * 40.,
            };
            entity.update(cx, |editor, cx| editor.scroll_by(delta, cx));
        });
    }

    fn pointer_down(&mut self, position: Point<Pixels>, extend: bool, cx: &mut Context<Self>) {
        self.finish_composition();
        let Some(layout) = self.layout.clone() else {
            return;
        };
        let Some(display) = hit_test(&layout, position) else {
            return;
        };
        let source = self.projection.display_to_source(display, Affinity::After);
        self.set_caret(source, extend, cx);
    }

    fn pointer_drag(&mut self, position: Point<Pixels>, cx: &mut Context<Self>) {
        let Some(layout) = self.layout.clone() else {
            return;
        };
        let Some(display) = hit_test(&layout, position) else {
            return;
        };
        let source = self.projection.display_to_source(display, Affinity::After);
        self.set_caret(source, true, cx);
    }
}

fn measure_width(bounds: Bounds<Pixels>) -> f32 {
    (f32::from(bounds.size.width) - PAD_X * 2.).clamp(80., MEASURE)
}

/// The display offset nearest a window point inside the surface.
fn hit_test(layout: &Layout, position: Point<Pixels>) -> Option<usize> {
    let y = position.y - layout.bounds.origin.y - px(PAD_Y) + px(layout.scroll);
    let x = position.x - layout.bounds.origin.x - px(PAD_X);
    let line = layout
        .lines
        .iter()
        .find(|line| px(line.top) <= y && y <= px(line.top + line.height))
        .or_else(|| {
            // Outside the text: clamp to the closest line, not the first one.
            layout.lines.iter().min_by(|a, b| {
                let da = distance_to_span(y, a.top, a.top + a.height);
                let db = distance_to_span(y, b.top, b.top + b.height);
                da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
            })
        })?;
    let wrapped = line
        .wrapped
        .iter()
        .find(|wrapped| {
            let start = px(wrapped.x);
            let end = start + wrapped.shaped.width();
            start <= x && x <= end
        })
        .or_else(|| {
            // Beyond the last fragment, or before the first: clamp to the
            // nearest end of the line.
            line.wrapped.iter().min_by(|a, b| {
                let da = distance_to_span(x, a.x, a.x + f32::from(a.shaped.width()));
                let db = distance_to_span(x, b.x, b.x + f32::from(b.shaped.width()));
                da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
            })
        })?;
    let local = wrapped.shaped.closest_index_for_x(x - px(wrapped.x));
    Some(wrapped.display.start + local)
}

/// Distance from a coordinate to a one-dimensional span, zero when inside.
fn distance_to_span(value: Pixels, start: f32, end: f32) -> f32 {
    let value = f32::from(value);
    if value < start {
        start - value
    } else if value > end {
        value - end
    } else {
        0.
    }
}

fn build_runs(
    theme: &Theme,
    text: &str,
    styles: &[breeze_reflow::markdown::DisplayRun],
    base_font: Font,
    font_size: f32,
) -> Vec<TextRun> {
    use breeze_reflow::markdown::RunStyle;
    let mut runs = Vec::new();
    let mut cursor = 0usize;
    let push = |runs: &mut Vec<TextRun>, range: Range<usize>, style: RunStyle| {
        if range.is_empty() {
            return;
        }
        let mut run_font = base_font.clone();
        let mut color = theme.foreground;
        if style.bold {
            run_font.weight = FontWeight::BOLD;
        }
        if style.italic {
            run_font.style = FontStyle::Italic;
        }
        if style.code {
            run_font = font(theme.mono_font_family.clone());
            color = theme.muted_foreground;
        }
        if style.link {
            color = theme.link;
        }
        runs.push(TextRun {
            len: range.len(),
            font: run_font,
            color,
            background_color: None,
            underline: style.link.then_some(UnderlineStyle {
                color: Some(theme.link),
                thickness: px(1.),
                wavy: false,
            }),
            strikethrough: style.strike.then_some(StrikethroughStyle {
                color: Some(theme.foreground),
                thickness: px(1.),
            }),
        });
    };
    for run in styles {
        let start = run.range.start.min(text.len());
        let end = run.range.end.min(text.len());
        if cursor < start {
            push(&mut runs, cursor..start, RunStyle::default());
        }
        push(&mut runs, start..end, run.style);
        cursor = cursor.max(end);
        let _ = font_size;
    }
    if cursor < text.len() {
        push(&mut runs, cursor..text.len(), RunStyle::default());
    }
    if runs.is_empty() {
        runs.push(TextRun {
            len: text.len(),
            font: base_font,
            color: theme.foreground,
            background_color: None,
            underline: None,
            strikethrough: None,
        });
    }
    runs
}

struct Fragment {
    range: Range<usize>,
    start_offset: f32,
}

/// Greedy wrapping over the shaped line. P3 replaces this with the bounded
/// layout pipeline; P2 only needs correct caret geometry for the harness.
fn wrap_line(shaped: &ShapedLine, max_width: Pixels) -> Vec<(Fragment, ShapedLine)> {
    let mut out = Vec::new();
    if f32::from(max_width) <= 0. {
        out.push((
            Fragment {
                range: 0..shaped.text.len(),
                start_offset: 0.,
            },
            shaped.clone(),
        ));
        return out;
    }
    let mut remaining = shaped.clone();
    let mut base = 0usize;
    let mut offset = 0.;
    loop {
        if remaining.width() <= max_width || remaining.text.is_empty() {
            out.push((
                Fragment {
                    range: base..base + remaining.text.len(),
                    start_offset: offset,
                },
                remaining,
            ));
            break;
        }
        let mut cut = remaining.closest_index_for_x(max_width);
        if let Some(space) = remaining.text[..cut.min(remaining.text.len())].rfind(' ') {
            cut = space + 1;
        }
        if cut == 0 {
            cut = remaining
                .text
                .char_indices()
                .nth(1)
                .map(|(index, _)| index)
                .unwrap_or(remaining.text.len());
        }
        let cut = cut.min(remaining.text.len());
        let (prefix, suffix) = remaining.split_at(cut);
        let next_offset = offset + f32::from(prefix.width());
        out.push((
            Fragment {
                range: base..base + cut,
                start_offset: offset,
            },
            prefix,
        ));
        base += cut;
        offset = next_offset;
        remaining = suffix;
    }
    out
}

fn utf16_to_byte(text: &str, unit: usize) -> usize {
    let mut seen = 0usize;
    for (index, character) in text.char_indices() {
        if seen >= unit {
            return index;
        }
        seen += character.len_utf16();
    }
    text.len()
}

fn byte_to_utf16(text: &str, byte: usize) -> usize {
    let mut index = byte.min(text.len());
    while index > 0 && !text.is_char_boundary(index) {
        index -= 1;
    }
    text[..index].encode_utf16().count()
}

impl IntoElement for MarkdownSurface {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

/// The document body element: it paints the shaped projection and owns the
/// platform input handler and pointer listeners.
pub(super) struct MarkdownSurface {
    editor: Entity<MarkdownEditor>,
}

impl MarkdownSurface {
    pub(super) fn new(editor: Entity<MarkdownEditor>) -> Self {
        Self { editor }
    }
}

impl Element for MarkdownSurface {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        Some("markdown-surface".into())
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&gpui_kit::InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let mut style = Style::default();
        style.size.width = relative(1.).into();
        style.size.height = relative(1.).into();
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&gpui_kit::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        self.editor
            .update(cx, |editor, cx| editor.build_layout(bounds, window, cx));
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&gpui_kit::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut Self::RequestLayoutState,
        _: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        self.editor
            .update(cx, |editor, cx| editor.paint(bounds, window, cx));
    }
}

impl Render for MarkdownEditor {
    /// The editor renders only the document surface. Chrome, tabs, the
    /// navigation panel, the toolbar and the status bar stay owned by the
    /// reader, so editing happens in the existing content area.
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = Theme::global(cx).clone();
        let notice = self.notice.clone();
        div()
            .id("markdown-editor")
            .test_support()
            .size_full()
            .flex()
            .flex_col()
            .track_focus(&self.focus)
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                if this.key_down(event, window, cx) {
                    cx.stop_propagation();
                }
            }))
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
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .overflow_hidden()
                    .child(MarkdownSurface::new(cx.entity())),
            )
    }
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;
    use gpui_kit::test::TestWindowExt;
    use gpui_kit::{AnyWindowHandle, EntityInputHandler as _, TestAppContext, WindowAppearance};

    /// A real window whose root view is the review editor.
    pub(super) fn editor_window(
        cx: &mut TestAppContext,
        name: &str,
        markdown: &str,
    ) -> (AnyWindowHandle, Entity<MarkdownEditor>) {
        // A temporary file per test keeps fixtures out of the working tree;
        // `open` reads the bytes immediately, so dropping it afterwards is safe.
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join(name);
        std::fs::write(&path, markdown).unwrap();
        cx.update(|cx| {
            gpui_kit::init(cx);
            appearance::init(cx).unwrap();
            let config = ThemePreference::System
                .config(WindowAppearance::Light, cx)
                .unwrap();
            style::apply(&config, cx);
            gpui_kit::open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(Bounds {
                        origin: point(px(0.), px(0.)),
                        size: size(px(900.), px(600.)),
                    })),
                    ..TitleBar::window_options()
                },
                cx,
                move |window, cx| {
                    let editor = cx.new(|cx| MarkdownEditor::open(path.clone(), cx));
                    let focus = editor.read(cx).focus.clone();
                    window.focus(&focus, cx);
                    editor
                },
            )
            .unwrap()
        })
    }

    #[gpui_kit::test]
    fn live_and_source_views_share_bytes_history_and_selection(cx: &mut TestAppContext) {
        let (handle, editor) = editor_window(cx, "views.md", "# Title\n\nBody **bold** text\n");
        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            editor.update(cx, |editor, cx| {
                assert_eq!(editor.view, EditView::Live);
                // The initial caret sits in the heading marker, so the live view
                // reveals it; moving the caret away hides it again.
                assert!(editor.projection.text.starts_with("# Title"));
                let layout = editor.layout.as_ref().expect("the surface laid out");
                assert_eq!(layout.lines.len(), 3);
                assert!(layout.content_height > 0.);
                // Moving the caret out of the marker hides it and invalidates
                // the cached layout.
                editor.set_caret(6, false, cx);
                assert!(
                    editor.projection.text.starts_with("Title"),
                    "live view hid the heading marker: {:?}",
                    editor.projection.text
                );
                assert!(editor.layout.is_none(), "a new projection needs new layout");
            });
            window.render_frame(cx);
            editor.update(cx, |editor, cx| {
                assert!(editor.layout.is_some(), "the surface laid out again");
                let bytes = editor.source_text();
                let selection = editor.session.selection();
                let revision = editor.session.revision();
                editor.toggle_view(EditView::Source, cx);
                assert_eq!(editor.view, EditView::Source);
                assert_eq!(editor.source_text(), bytes, "toggling must not edit");
                assert_eq!(editor.session.revision(), revision);
                assert_eq!(editor.session.selection(), selection);
                assert!(editor.projection.text.contains("# Title"));
                assert_eq!(editor.session.undo_count(), 0);

                editor.toggle_view(EditView::Live, cx);
                assert_eq!(editor.source_text(), bytes);
                assert!(editor.projection.text.starts_with("Title"));
            });
        })
        .unwrap();
    }

    #[gpui_kit::test]
    fn composition_previews_commit_once_and_cancel_restores(cx: &mut TestAppContext) {
        let (handle, editor) = editor_window(cx, "composition.md", "say here\n");
        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            editor.update(cx, |editor, cx| {
                editor.set_caret(4, false, cx);
                assert!(editor.session.undo_count() == 0);
                editor.replace_and_mark_text_in_range(None, "ni", None, window, cx);
                editor.replace_and_mark_text_in_range(None, "nihao", None, window, cx);
                assert_eq!(editor.source_text(), "say nihaohere\n");
                assert_eq!(editor.session.undo_count(), 0, "previews are not history");
                assert!(editor.marked_text_range(window, cx).is_some());
                assert_eq!(
                    editor
                        .text_for_range(0..2, &mut None, window, cx)
                        .as_deref(),
                    Some("sa")
                );

                editor.unmark_text(window, cx);
                assert_eq!(editor.source_text(), "say nihaohere\n");
                assert_eq!(editor.session.undo_count(), 1);
                assert!(editor.marked_text_range(window, cx).is_none());

                // A cancelled composition restores the original bytes.
                editor.set_caret(4, false, cx);
                editor.replace_and_mark_text_in_range(None, "xie", None, window, cx);
                assert_eq!(editor.source_text(), "say xienihaohere\n");
                editor.replace_text_in_range(None, "", window, cx);
                assert_eq!(editor.source_text(), "say nihaohere\n");
                assert_eq!(editor.session.undo_count(), 1, "cancel adds no entry");

                editor.undo(cx);
                assert_eq!(editor.source_text(), "say here\n");
            });
        })
        .unwrap();
    }

    #[gpui_kit::test]
    fn caret_movement_and_bounds_cover_graphemes_and_cjk(cx: &mut TestAppContext) {
        let (handle, editor) = editor_window(cx, "caret.md", "a😀b 中文\n");
        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            editor.update(cx, |editor, cx| {
                editor.set_caret(0, false, cx);
                editor.move_caret(Motion::Right, false, cx);
                assert_eq!(editor.session.selection().head.offset, 1);
                editor.move_caret(Motion::Right, false, cx);
                assert_eq!(
                    editor.session.selection().head.offset,
                    5,
                    "one arrow step crosses the whole emoji"
                );
                editor.move_caret(Motion::End, false, cx);
                assert_eq!(
                    editor.session.selection().head.offset,
                    13,
                    "End stops before the line terminator"
                );
                editor.move_caret(Motion::Home, false, cx);
                assert_eq!(editor.session.selection().head.offset, 0);

                // Source 7 is the start of 中, UTF-16 offset 5.
                editor.set_caret(7, false, cx);
            });
            window.render_frame(cx);
            editor.update(cx, |editor, cx| {
                let bounds = editor
                    .bounds_for_range(
                        5..5,
                        Bounds {
                            origin: point(px(0.), px(0.)),
                            size: size(px(900.), px(600.)),
                        },
                        window,
                        cx,
                    )
                    .expect("the caret has bounds");
                let index = editor
                    .character_index_for_point(bounds.origin, window, cx)
                    .expect("the point maps back");
                assert_eq!(index, 5, "UTF-16 caret round trip");
                let selected = editor
                    .selected_text_range(false, window, cx)
                    .expect("a selection exists");
                assert_eq!(selected.range, 5..5);

                // Shift-extension keeps the anchor and moves the head.
                editor.set_caret(1, false, cx);
                editor.move_caret(Motion::Right, true, cx);
                let selection = editor.session.selection();
                assert_eq!(selection.range(), 1..5);
                assert!(!selection.is_empty());
            });
        })
        .unwrap();
    }

    #[gpui_kit::test]
    fn typing_and_undo_stay_inside_the_session(cx: &mut TestAppContext) {
        let (handle, editor) = editor_window(cx, "typing.md", "- one\n");
        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            editor.update(cx, |editor, cx| {
                assert_eq!(editor.projection.text, "\u{2022} one\n");
                editor.set_caret(5, false, cx);
                editor.replace_text_in_range(None, "!", window, cx);
                assert_eq!(editor.source_text(), "- one!\n");
                assert_eq!(editor.session.undo_count(), 1);
                assert!(editor.projection.text.contains('\u{2022}'));
                editor.undo(cx);
                assert_eq!(editor.source_text(), "- one\n");
                editor.redo(cx);
                assert_eq!(editor.source_text(), "- one!\n");
            });
        })
        .unwrap();
    }
}

#[cfg(test)]
mod hit_test_tests {
    use super::*;
    use gpui_kit::test::TestWindowExt;

    /// A pointer below the last line clamps to that line, not the first one.
    #[gpui_kit::test]
    fn clicks_outside_the_text_clamp_to_the_nearest_line(cx: &mut TestAppContext) {
        let (handle, editor) = super::tests::editor_window(cx, "clicks.md", "# A\n\nB\n\nC\n");
        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            editor.update(cx, |editor, _| {
                let layout = editor.layout.clone().expect("laid out");
                let bounds = layout.bounds;
                // Far below the content.
                let below = point(
                    bounds.origin.x + px(PAD_X + 4.),
                    bounds.origin.y + px(10_000.),
                );
                let display = hit_test(&layout, below).expect("clamped inside the document");
                let source = editor.projection.display_to_source(display, Affinity::After);
                assert!(
                    source >= 8,
                    "clicking below the text must reach the last paragraph, got {source} (display {display}, lines {}, text {:?})",
                    layout.lines.len(),
                    editor.projection.text,
                );
                // Far above the content.
                let above = point(
                    bounds.origin.x + px(PAD_X + 4.),
                    bounds.origin.y - px(500.),
                );
                let display = hit_test(&layout, above).expect("clamped inside the document");
                let source = editor.projection.display_to_source(display, Affinity::After);
                assert_eq!(source, 0, "clicking above the text reaches the first line");
            });
        })
        .unwrap();
    }
}
