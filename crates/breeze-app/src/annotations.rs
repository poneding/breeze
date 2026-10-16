//! Native overlays and explicit, conflict-checked sidecar saves; no PDF content writes.
use crate::*;
use breeze_core::annotations::{Annotation, History, Kind, Point as DocPoint, Rect};
use breeze_core::annotations::{ViewGeometry, curves};
use breeze_storage::{AnnotationConflict, AnnotationFile};
use gpui_kit::component::ElementExt;
use lyon_path::{LineCap, LineJoin};

mod recovery;
#[cfg(test)]
mod tests;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum AnnotationFailure {
    Io,
    SidecarConflict,
    SourceConflict,
    InvalidData,
}

/// Distance from the reading viewport's edge at which a drag starts scrolling,
/// and the scroll speed there and at the edge itself, in logical pixels.
const AUTOSCROLL_MARGIN: f32 = 24.;
const AUTOSCROLL_MIN: f32 = 2.;
const AUTOSCROLL_MAX: f32 = 18.;
const AUTOSCROLL_INTERVAL: std::time::Duration = std::time::Duration::from_millis(16);

/// Distance from a point to a page surface, zero inside it: the page a pointer
/// in a gap, a margin, or a spread's middle belongs to.
fn page_distance(
    bounds: Bounds<gpui_kit::Pixels>,
    position: gpui_kit::Point<gpui_kit::Pixels>,
) -> f32 {
    let dx = f32::from(
        (bounds.left() - position.x)
            .max(position.x - bounds.right())
            .max(px(0.)),
    );
    let dy = f32::from(
        (bounds.top() - position.y)
            .max(position.y - bounds.bottom())
            .max(px(0.)),
    );
    dx * dx + dy * dy
}

/// A text selection: the caret it started at and the caret it has reached, each
/// a page and a glyph index in that page's reading order. Pages between them are
/// selected whole, so a drag that crosses page surfaces keeps working.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct TextSelection {
    anchor: (usize, usize),
    focus: (usize, usize),
}

impl TextSelection {
    fn collapsed(page: usize, caret: usize) -> Self {
        Self {
            anchor: (page, caret),
            focus: (page, caret),
        }
    }

    /// Anchor and focus in reading order; dragging backwards selects the same
    /// text as dragging forwards.
    fn ordered(self) -> ((usize, usize), (usize, usize)) {
        if self.anchor <= self.focus {
            (self.anchor, self.focus)
        } else {
            (self.focus, self.anchor)
        }
    }

    fn collapsed_to_page(&self) -> bool {
        self.anchor.0 == self.focus.0 && self.anchor.1 == self.focus.1
    }

    /// The glyphs selected on one page, clamped to what that page holds.
    fn on_page(self, page: usize, glyphs: usize) -> Option<std::ops::Range<usize>> {
        let (start, end) = self.ordered();
        if page < start.0 || page > end.0 {
            return None;
        }
        let low = if page == start.0 { start.1 } else { 0 }.min(glyphs);
        let high = if page == end.0 { end.1 } else { glyphs }.min(glyphs);
        Some(low..high.max(low))
    }
}

#[derive(Default)]
pub(super) struct DocumentAnnotations {
    pub history: History,
    pub saved: Vec<Annotation>,
    pub fingerprint: String,
    pub revision: Option<String>,
    pub path: Option<PathBuf>,
    pub ready: bool,
    pub source_conflict: bool,
    pub saving: bool,
    pub error: Option<String>,
    pub failure: Option<AnnotationFailure>,
    pub identity: Arc<()>,
    pub recovery_path: Option<PathBuf>,
    pub recovery_notice: Option<String>,
}
impl DocumentAnnotations {
    pub fn dirty(&self) -> bool {
        self.history.items != self.saved
    }
}
impl Breeze {
    pub(super) fn prepare_annotations(&mut self, fingerprint: String, cx: &mut Context<Self>) {
        let book = &mut self.books[self.active];
        if book.annotations.fingerprint == fingerprint {
            return;
        }
        if book.annotations.ready || book.annotations.saving || book.annotations.source_conflict {
            book.annotations.source_conflict = true;
            book.annotations.failure = Some(AnnotationFailure::SourceConflict);
            book.annotations.error = Some(
                tr!("Source changed; unsaved annotations are retained and writes are paused")
                    .into(),
            );
            self.message =
                tr!("Source changed: old annotations are hidden and writes are paused").into();
            return;
        }
        let path = if let Some(path) = book.annotations.path.clone() {
            path
        } else {
            match &book.source {
                Source::File(p) => breeze_storage::sidecar_path(p),
                Source::Sample => {
                    let Some(root) = &self.data_dir else {
                        return;
                    };
                    root.join("sample.pdf.breeze.json")
                }
            }
        };
        book.annotations = DocumentAnnotations {
            fingerprint: fingerprint.clone(),
            path: Some(path.clone()),
            ..Default::default()
        };
        let source = book.source.clone();
        let expected_fingerprint = fingerprint.clone();
        let expected_path = path.clone();
        let identity = book.annotations.identity.clone();
        let work = cx.background_spawn(async move { breeze_storage::inspect_annotations(&path) });
        cx.spawn(async move |this, cx| {
            let result = work.await;
            let _ = this.update(cx, |this, cx| {
                let Some(book) = this.books.iter_mut().find(|b| b.source == source) else {
                    return;
                };
                if !Arc::ptr_eq(&book.annotations.identity, &identity)
                    || book.annotations.fingerprint != expected_fingerprint
                    || book.annotations.path.as_ref() != Some(&expected_path)
                {
                    return;
                }
                match result {
                    Ok(snapshot) => {
                        let (
                            source_fingerprint,
                            AnnotationFile {
                                annotations,
                                revision,
                            },
                        ) = snapshot.map_or_else(
                            || (expected_fingerprint.clone(), AnnotationFile::default()),
                            |s| (s.fingerprint, s.file),
                        );
                        let source_conflict = source_fingerprint != expected_fingerprint;
                        if !source_conflict
                            && annotations
                                .iter()
                                .any(|a| a.page >= book.state.navigation.total())
                        {
                            book.annotations.error =
                                Some(tr!("Annotation page is out of range; annotations were not loaded").into());
                            book.annotations.failure = Some(AnnotationFailure::InvalidData);
                            cx.notify();
                            return;
                        }
                        book.annotations.saved = annotations.clone();
                        book.annotations.history.items = annotations;
                        book.annotations.revision = revision;
                        book.annotations.ready = true;
                        book.annotations.fingerprint = source_fingerprint;
                        book.annotations.source_conflict = source_conflict;
                        if source_conflict {
                            book.annotations.failure = Some(AnnotationFailure::SourceConflict);
                            book.annotations.error = Some(
                                tr!("Source and annotation fingerprints differ. Old annotations are retained and hidden; recover or reassociate them.")
                                    .into(),
                            );
                        }
                    }
                    Err(e) => {
                        book.annotations.error = Some(tf!("Cannot load annotations: {}", format!("{:#}", e)));
                        book.annotations.failure =
                            Some(if e.downcast_ref::<std::io::Error>().is_some() {
                                AnnotationFailure::Io
                            } else {
                                AnnotationFailure::InvalidData
                            });
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }
    pub(super) fn save_annotations(&mut self, cx: &mut Context<Self>) {
        if !self.books[self.active].is_pdf() {
            return;
        }
        if self.annotation_editing.pending_text() {
            self.message = tr!("Press Enter to finish the text note before saving").into();
            cx.notify();
            return;
        }
        self.commit_selected_annotation(cx);
        self.save_annotations_for(self.active, cx);
    }
    fn save_annotations_for(&mut self, index: usize, cx: &mut Context<Self>) {
        self.save_annotations_to(index, None, cx);
    }
    fn save_annotations_to(
        &mut self,
        index: usize,
        new_path: Option<PathBuf>,
        cx: &mut Context<Self>,
    ) {
        let book = &mut self.books[index];
        let a = &mut book.annotations;
        let save_as = new_path.is_some();
        if !a.ready || a.saving || a.source_conflict || a.fingerprint.is_empty() {
            self.message =
                tr!("Annotations are not ready or have a conflict; saving is unavailable").into();
            cx.notify();
            return;
        }
        let Some(path) = new_path.or_else(|| a.path.clone()) else {
            return;
        };
        let source = book.source.clone();
        let fingerprint = a.fingerprint.clone();
        let revision = if save_as { None } else { a.revision.clone() };
        let original_path = a.path.clone();
        let identity = a.identity.clone();
        let items = a.history.items.clone();
        let saved = items.clone();
        a.saving = true;
        let read_source = match &source {
            Source::File(p) => Some(p.clone()),
            Source::Sample => None,
        };
        let destination = path.display().to_string();
        let saved_path = path.clone();
        let saved_fingerprint = fingerprint.clone();
        let work = cx.background_spawn(async move {
            breeze_storage::save_annotations(
                &path,
                read_source.as_deref(),
                &fingerprint,
                revision.as_deref(),
                items,
            )
        });
        cx.spawn(async move |this, cx| {
            let result = work.await;
            let _ = this.update(cx, |this, cx| {
                let Some(book) = this.books.iter_mut().find(|b| b.source == source) else {
                    return;
                };
                if !Arc::ptr_eq(&book.annotations.identity, &identity)
                    || book.annotations.fingerprint != saved_fingerprint
                    || book.annotations.path != original_path
                {
                    return;
                }
                book.annotations.saving = false;
                match result {
                    Ok(revision) => {
                        // Publish the association and acknowledgement as one UI transaction.
                        // A failed Save As must leave the last good sidecar reachable.
                        book.annotations.path = Some(saved_path);
                        book.annotations.revision = Some(revision);
                        book.annotations.saved = saved;
                        if !book.annotations.source_conflict {
                            book.annotations.error = None;
                            book.annotations.failure = None;
                        }
                        this.message = tf!("Annotations saved: {}", destination);
                    }
                    Err(e) => {
                        let failure = match e.downcast_ref::<AnnotationConflict>() {
                            Some(AnnotationConflict::SourceChanged) => {
                                AnnotationFailure::SourceConflict
                            }
                            Some(AnnotationConflict::SidecarChanged) => {
                                AnnotationFailure::SidecarConflict
                            }
                            None => AnnotationFailure::Io,
                        };
                        if !save_as || failure == AnnotationFailure::SourceConflict {
                            book.annotations.failure = Some(failure);
                            if failure == AnnotationFailure::SourceConflict {
                                book.annotations.source_conflict = true;
                            }
                        }
                        if save_as {
                            this.message = tf!(
                                "Save As failed; the original save location is retained: {}",
                                format!("{:#}", e)
                            );
                            if failure == AnnotationFailure::SourceConflict {
                                book.annotations.error = Some(this.message.clone());
                            }
                        } else {
                            book.annotations.error =
                                Some(tf!("Save failed: {}", format!("{:#}", e)));
                            this.message = tf!("Annotation save failed: {}", format!("{:#}", e));
                        }
                    }
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
    pub(super) fn save_annotations_as(&mut self, cx: &mut Context<Self>) {
        if !self.books[self.active].is_pdf() {
            return;
        }
        if self.annotation_editing.pending_text() {
            self.message = tr!("Press Enter to finish the text note before Save As").into();
            cx.notify();
            return;
        }
        self.commit_selected_annotation(cx);
        let book = &mut self.books[self.active];
        let a = &mut book.annotations;
        if !a.ready || a.saving || a.source_conflict || a.fingerprint.is_empty() {
            self.message = tr!(
                "Annotations are loading, saving or have a source conflict; Save As is unavailable"
            )
            .into();
            cx.notify();
            return;
        }
        let source = book.source.clone();
        let fingerprint = a.fingerprint.clone();
        let original_path = a.path.clone();
        let identity = a.identity.clone();
        // A pending picker owns the same save slot as the subsequent write.
        // This prevents competing dialogs and clean-document closes.
        a.saving = true;
        let directory = self.data_dir.clone().unwrap_or_else(std::env::temp_dir);
        let receiver = cx.prompt_for_new_path(&directory, Some("annotations.breeze.json"));
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = receiver.await;
            let _ = this.update(cx, |this, cx| {
                let Some(index) = this.books.iter().position(|b| b.source == source) else {
                    return;
                };
                let a = &mut this.books[index].annotations;
                if !Arc::ptr_eq(&a.identity, &identity)
                    || a.fingerprint != fingerprint
                    || a.path != original_path
                {
                    return;
                }
                a.saving = false;
                let selected = match result {
                    Ok(Ok(Some(path))) => Some(path),
                    Ok(Ok(None)) => {
                        // Cancelling keeps the current location; nothing to report.
                        this.message.clear();
                        None
                    }
                    Ok(Err(e)) => {
                        this.message = tf!(
                            "Cannot choose annotation save location: {}",
                            format!("{:#}", e)
                        );
                        None
                    }
                    Err(e) => {
                        this.message = tf!("Annotation file dialog interrupted: {}", e);
                        None
                    }
                };
                if let Some(path) = selected {
                    this.save_annotations_to(index, Some(path), cx);
                }
                cx.notify();
            });
        })
        .detach();
    }
    pub(super) fn has_unsaved_annotations(&self) -> bool {
        self.annotation_editing.pending_text()
            || self
                .books
                .iter()
                .any(|b| b.annotations.dirty() || b.annotations.saving)
    }
    pub(super) fn allow_close(&mut self, cx: &mut Context<Self>) -> bool {
        if self.annotation_editing.pending_text() {
            self.message =
                tr!("Finish the text note with Enter or cancel with Esc before closing").into();
            cx.notify();
            return false;
        }
        self.commit_selected_annotation(cx);
        if self.export_busy {
            self.message = tr!("Wait for PDF export to finish or cancel the file dialog").into();
            cx.notify();
            return false;
        }
        if self.has_unsaved_annotations() {
            if let Some(index) = self.books.iter().position(|b| b.annotations.dirty()) {
                if index != self.active {
                    self.open_book(index, cx);
                }
                self.screen = Screen::Reader;
                self.cancel_search();
                self.search_open = false;
                self.search_restore_document_panel_open = None;
                self.document_panel.show_annotations();
            }
            self.message = tr!("Unsaved annotations remain. Save, keep a recovery copy or explicitly discard changes before closing.").into();
            cx.notify();
            return false;
        }
        true
    }
    pub(super) fn document_pointer(
        &self,
        position: gpui_kit::Point<gpui_kit::Pixels>,
    ) -> Option<DocPoint> {
        document_point(
            self.view_geometry(self.preview.as_ref()?.geometry),
            self.surface_bounds?,
            position,
        )
    }
    fn pointer_down(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.loading {
            return;
        }
        if self.annotation_editing.pending_text() {
            self.focus_annotation_editor(window, cx);
            return;
        }
        window.focus(&self.focus, cx);
        let Some(point) = self.document_pointer(event.position) else {
            return;
        };
        self.clear_text_selection();
        if self.books[self.active].state.mode == Mode::Annotate {
            let data = &self.books[self.active].annotations;
            if !data.ready || data.error.is_some() {
                self.message =
                    tr!("Annotation file is not ready; check storage or conflict messages").into();
                cx.notify();
                return;
            }
        }
        if self.pointer_annotation_selection(point, window, cx) {
            return;
        }
        let mode = self.books[self.active].state.mode;
        if mode == Mode::Annotate && self.tool == annotation_editing::SELECT_TOOL {
            cx.notify();
            return;
        }
        // Reading starts a caret-anchored selection; the drag extends it.
        if mode == Mode::Read {
            self.begin_text_selection(event.position);
        }
        self.gesture = vec![point];
        cx.notify();
    }
    fn pointer_move(&mut self, event: &MouseMoveEvent, _: &mut Window, cx: &mut Context<Self>) {
        let Some(point) = self.document_pointer(event.position) else {
            return;
        };
        if self.move_annotation_pointer(point, cx) {
            return;
        }
        if self.gesture.is_empty() {
            return;
        }
        if self.tool == 4 && self.books[self.active].state.mode == Mode::Annotate {
            if self.gesture.len() < 20_000
                && self
                    .gesture
                    .last()
                    .is_some_and(|p| (p.x - point.x).abs() + (p.y - point.y).abs() > 0.7)
            {
                self.gesture.push(point);
            }
        } else if self.gesture.len() == 1 {
            self.gesture.push(point);
        } else {
            self.gesture[1] = point;
        }
        cx.notify();
    }
    fn draft(&self) -> Option<Annotation> {
        let first = *self.gesture.first()?;
        let last = *self.gesture.last()?;
        let mode = self.books[self.active].state.mode;
        let kind = if mode == Mode::Read {
            Kind::Highlight
        } else {
            match self.tool {
                0 => Kind::Highlight,
                1 => Kind::Area,
                2 => Kind::Underline,
                3 => Kind::Strikeout,
                4 => Kind::Ink,
                5 => Kind::Note,
                6 => Kind::Ellipse,
                7 => Kind::Rectangle,
                8 => Kind::Line,
                9 => Kind::Squiggle,
                10 => Kind::Arrow,
                _ => return None,
            }
        };
        let quads = if kind.text_markup() {
            let text = &self.preview.as_ref()?.text;
            let range = text.select(first, last);
            if kind == Kind::Highlight {
                text.highlight_rects(range)
            } else {
                text.line_rects(range)
            }
        } else {
            Vec::new()
        };
        let style =
            self.tool_settings
                .annotation
                .get(if mode == Mode::Read { 0 } else { self.tool })?;
        Some(Annotation {
            page: self.books[self.active].state.navigation.page(),
            kind,
            points: if self.gesture.len() == 1 {
                vec![first, last]
            } else {
                self.gesture.clone()
            },
            quads,
            color: colors::components(style.color),
            opacity: style.opacity as f32 / 100.,
            width: style.width,
            text: String::new(),
        })
    }
    fn pointer_up(&mut self, _: &MouseUpEvent, window: &mut Window, cx: &mut Context<Self>) {
        if self.finish_annotation_move(cx) {
            return;
        }
        if self.gesture.is_empty() {
            return;
        }
        if self.books[self.active].state.mode == Mode::Annotate && self.tool == 5 {
            let point = self.gesture[0];
            self.begin_text_annotation(point, window, cx);
            return;
        }
        if self.books[self.active].state.mode == Mode::Read {
            self.commit_text_selection(cx);
        } else if let Some(annotation) = self.draft() {
            match annotation.validate() {
                Ok(()) => {
                    let history = &mut self.books[self.active].annotations.history;
                    let points = history
                        .items
                        .iter()
                        .map(|a| a.points.len() + a.quads.len())
                        .sum::<usize>()
                        + annotation.points.len()
                        + annotation.quads.len();
                    if history.items.len() < 5000 && points <= 100_000 {
                        history.add(annotation);
                        self.annotation_editing.clear();
                        // The edit stays in the session until an explicit save; the
                        // reader status area already reports unsaved annotations.
                        self.message.clear();
                    } else {
                        self.message =
                            tr!("The annotation limit for this document has been reached").into();
                    }
                }
                Err(e) => self.message = tf!("Annotation was not created: {}", e),
            }
        }
        self.gesture.clear();
        cx.notify();
    }
    /// Reading mode: the pointer starts a selection anchored at its caret.
    /// Returns whether a selection began.
    pub(super) fn begin_text_selection(
        &mut self,
        position: gpui_kit::Point<gpui_kit::Pixels>,
    ) -> bool {
        if self.books[self.active].state.mode != Mode::Read {
            return false;
        }
        let Some(preview) = self.preview.clone() else {
            return false;
        };
        let Some(caret) = self
            .document_pointer(position)
            .and_then(|point| preview.text.caret(point))
        else {
            return false;
        };
        self.text_selection = Some(TextSelection::collapsed(preview.page, caret));
        self.refresh_text_selection();
        true
    }

    /// The page's own preview, current or cached.
    pub(super) fn page_preview(&self, page: usize) -> Option<Arc<Preview>> {
        self.reading
            .page(page)
            .or_else(|| self.preview.clone().filter(|preview| preview.page == page))
    }

    /// The page surface under a pointer, or the nearest one when the pointer is
    /// in a gap between pages or in the reading viewport's own padding. Pages
    /// that are not laid out right now are not candidates.
    fn selection_page_at(
        &self,
        position: gpui_kit::Point<gpui_kit::Pixels>,
    ) -> Option<(usize, Bounds<gpui_kit::Pixels>)> {
        self.page_bounds
            .iter()
            .min_by(|(page_a, a), (page_b, b)| {
                page_distance(**a, position)
                    .total_cmp(&page_distance(**b, position))
                    // A pointer exactly between two pages stays with the earlier.
                    .then(page_a.cmp(page_b))
            })
            .map(|(page, bounds)| (*page, *bounds))
    }

    /// Extends the text selection with a pointer anywhere in the reading
    /// viewport: over a page, in the gap between two pages, or in the padding
    /// past the last one. Returns whether the selection took the pointer.
    pub(super) fn update_text_selection(
        &mut self,
        position: gpui_kit::Point<gpui_kit::Pixels>,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.books[self.active].state.mode != Mode::Read
            || self.gesture.is_empty()
            || self.text_selection.is_none()
        {
            return false;
        }
        self.selection_pointer = Some(position);
        let extended = self.extend_text_selection(position);
        self.update_selection_autoscroll(cx);
        extended
    }

    /// Points the selection's focus at one window position, resolving the page
    /// from the surfaces that are laid out.
    fn extend_text_selection(&mut self, position: gpui_kit::Point<gpui_kit::Pixels>) -> bool {
        if self.gesture.is_empty() || self.text_selection.is_none() {
            return false;
        }
        let Some((page, bounds)) = self.selection_page_at(position) else {
            return false;
        };
        let Some(preview) = self.page_preview(page) else {
            return false;
        };
        let Some(point) = document_point(self.view_geometry(preview.geometry), bounds, position)
        else {
            return false;
        };
        // A page whose text is not ready yet keeps the selection where it is;
        // it joins as soon as its text arrives.
        let Some(caret) = preview.text.caret(point) else {
            return false;
        };
        if let Some(selection) = &mut self.text_selection {
            selection.focus = (page, caret);
        }
        self.refresh_text_selection();
        true
    }

    /// A drag held past the viewport's edge scrolls the reader, the way a text
    /// editor scrolls past its last visible line.
    fn update_selection_autoscroll(&mut self, cx: &mut Context<Self>) {
        let speed = match (self.selection_pointer, self.viewport_bounds) {
            (Some(position), Some(bounds)) => {
                let margin = px(AUTOSCROLL_MARGIN);
                let over_bottom = f32::from(position.y - (bounds.bottom() - margin));
                let over_top = f32::from((bounds.top() + margin) - position.y);
                let overshoot = over_bottom.max(over_top).max(0.);
                let direction = if over_bottom > 0. {
                    1.
                } else if over_top > 0. {
                    -1.
                } else {
                    0.
                };
                if direction == 0. {
                    0.
                } else {
                    direction
                        * (AUTOSCROLL_MIN
                            + (AUTOSCROLL_MAX - AUTOSCROLL_MIN)
                                * (overshoot / AUTOSCROLL_MARGIN).min(1.))
                }
            }
            _ => 0.,
        };
        self.selection_scroll = (speed != 0.).then_some(speed);
        if self.selection_scroll.is_none() {
            self.autoscroll = None;
            return;
        }
        if self.autoscroll.is_some() {
            return;
        }
        let executor = cx.background_executor().clone();
        self.autoscroll = Some(cx.spawn(async move |this, cx| {
            loop {
                executor.timer(AUTOSCROLL_INTERVAL).await;
                let keep = this
                    .update(cx, |this, cx| this.autoscroll_step(cx))
                    .unwrap_or(false);
                if !keep {
                    break;
                }
            }
        }));
    }

    /// One auto-scroll step: scroll the reader, carry the page bounds with it,
    /// and extend the selection over the content it revealed. Returns whether
    /// another step is wanted.
    fn autoscroll_step(&mut self, cx: &mut Context<Self>) -> bool {
        if self.gesture.is_empty() || self.text_selection.is_none() {
            self.selection_scroll = None;
            return false;
        }
        let Some(speed) = self.selection_scroll else {
            return false;
        };
        let before = self.reading.pages_scroll.offset();
        self.scroll_reading(px(speed), cx);
        let scrolled = self.reading.pages_scroll.offset().y - before.y;
        if scrolled == px(0.) {
            // The document ends here; the next pointer move decides again.
            self.selection_scroll = None;
            return false;
        }
        // The surfaces moved with the content until the next frame reports them.
        for bounds in self.page_bounds.values_mut() {
            bounds.origin.y += scrolled;
        }
        if let Some(position) = self.selection_pointer {
            self.extend_text_selection(position);
        }
        cx.notify();
        true
    }

    /// Recomputes the copied text and the rectangles drawn for a selection.
    pub(super) fn refresh_text_selection(&mut self) {
        let Some(selection) = self.text_selection else {
            self.selected_text.clear();
            self.selection.clear();
            return;
        };
        let (start, end) = selection.ordered();
        let mut text = String::new();
        let mut rects: Vec<(usize, Vec<Rect>)> = Vec::new();
        for page in start.0..=end.0 {
            let Some(preview) = self.page_preview(page) else {
                continue;
            };
            let Some(range) = selection.on_page(page, preview.text.glyphs().len()) else {
                continue;
            };
            if range.is_empty() {
                continue;
            }
            if !text.is_empty() {
                text.push('\n');
            }
            text.push_str(&preview.text.text(range.clone()));
            let page_rects = preview.text.highlight_rects(range);
            if !page_rects.is_empty() {
                rects.push((page, page_rects));
            }
        }
        self.selected_text = text;
        self.selection = rects;
    }

    /// Drops the selection and everything derived from it.
    pub(super) fn clear_text_selection(&mut self) {
        self.text_selection = None;
        self.selected_text.clear();
        self.selection.clear();
        self.selection_pointer = None;
        self.selection_scroll = None;
        self.autoscroll = None;
    }

    /// Ends a text selection in flight: the release commits it, a page-arrival
    /// refresh leaves it alone.
    pub(super) fn end_text_selection(&mut self, cx: &mut Context<Self>) {
        if self.text_selection.is_none() {
            self.autoscroll = None;
            self.selection_scroll = None;
            return;
        }
        self.commit_text_selection(cx);
    }

    /// Retries the selection's pointer when a page it is waiting on gains its
    /// text, so a drag held over a page still being prepared catches up without
    /// another pointer move.
    pub(super) fn refresh_selection_page(&mut self, page: usize) {
        let Some(position) = self.selection_pointer else {
            return;
        };
        if self.selection_page_at(position).map(|(found, _)| found) != Some(page) {
            return;
        }
        self.extend_text_selection(position);
    }

    /// Reports the selection the release just committed. A press that never
    /// moved selects nothing.
    fn commit_text_selection(&mut self, cx: &mut Context<Self>) {
        self.selection_scroll = None;
        self.autoscroll = None;
        if self.text_selection.is_none() {
            return;
        }
        if self.text_selection.is_some_and(|s| s.collapsed_to_page()) {
            self.clear_text_selection();
            self.message.clear();
        } else {
            self.refresh_text_selection();
            self.message = tf!(
                "{} characters selected · ⌘/Ctrl+C to copy",
                self.selected_text.chars().count()
            );
        }
        cx.notify();
    }

    /// The selection rectangles to paint on the page being drawn, if any.
    fn selection_layer(&self, current: usize) -> Option<Vec<Rect>> {
        self.selection
            .iter()
            .find(|(page, _)| *page == current)
            .map(|(_, rects)| rects.clone())
    }

    pub(super) fn page_surface(
        &self,
        current: usize,
        preview: &Arc<Preview>,
        width: f32,
        height: f32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let geometry = self.view_geometry(preview.geometry);
        let mut annotations: Vec<_> = self.books[self.active]
            .annotations
            .history
            .items
            .iter()
            .enumerate()
            .filter(|(_, a)| {
                a.page == current && !self.books[self.active].annotations.source_conflict
            })
            .map(|(index, a)| self.displayed_annotation(index, a))
            .collect();
        if let Some(draft) = self.draft().filter(|a| a.page == current) {
            annotations.push(draft);
        }
        let notes = self.books[self.active]
            .annotations
            .history
            .items
            .iter()
            .enumerate()
            .filter(|(index, a)| {
                a.page == current
                    && a.kind == Kind::Note
                    && !self.text_annotation_being_edited(*index)
                    && !self.books[self.active].annotations.source_conflict
            })
            .map(|(index, a)| (index, self.displayed_annotation(index, a)))
            .collect::<Vec<_>>();
        let view = cx.entity();
        let bounds_cell = std::rc::Rc::new(std::cell::Cell::new(None));
        let capture_bounds = bounds_cell.clone();
        let page_preview = preview.clone();
        // Reading is a pointer mode: the pointer icon on the mode tab matches the
        // hand the page shows, while Annotate keeps the default arrow unless an
        // annotation's own resize handles claim the cursor.
        let cursor = if self.books[self.active].state.mode == Mode::Read {
            CursorStyle::PointingHand
        } else {
            CursorStyle::Arrow
        };
        div()
            .id("page-surface")
            .test_support()
            .relative()
            .w(px(width))
            .h(px(height))
            .flex_shrink_0()
            .overflow_hidden()
            .cursor(cursor)
            .on_prepaint(move |bounds, _, cx| {
                capture_bounds.set(Some(bounds));
                view.update(cx, |this, _| {
                    this.page_bounds.insert(current, bounds);
                    if current == this.books[this.active].state.navigation.page()
                        && this.gesture.is_empty()
                    {
                        this.surface_bounds = Some(bounds);
                    }
                });
            })
            .capture_any_mouse_down(cx.listener(move |this, _: &MouseDownEvent, _, _| {
                if this.annotation_editing.pending_text()
                    && current != this.books[this.active].state.navigation.page()
                {
                    return;
                }
                this.books[this.active].state.navigation.go_to(current);
                this.preview = Some(page_preview.clone());
                this.surface_bounds = bounds_cell.get();
            }))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event, window, cx| {
                    this.pointer_down(event, window, cx);
                }),
            )
            .on_mouse_move(
                cx.listener(move |this, event: &MouseMoveEvent, window, cx| {
                    if current == this.books[this.active].state.navigation.page() {
                        this.pointer_move(event, window, cx);
                    }
                }),
            )
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(move |this, event, window, cx| {
                    if current == this.books[this.active].state.navigation.page() {
                        this.pointer_up(event, window, cx);
                    }
                }),
            )
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(move |this, event, window, cx| {
                    if current == this.books[this.active].state.navigation.page() {
                        this.pointer_up(event, window, cx);
                    }
                }),
            )
            .child(self.raster_surface(current, geometry, cx))
            .children(self.selection_layer(current).map(|rects| {
                canvas(
                    move |_, _, _| rects,
                    move |bounds, rects, window, _| {
                        for rect in &rects {
                            paint_doc_rect(
                                *rect,
                                geometry,
                                bounds,
                                Rgba {
                                    a: 0.28,
                                    ..rgb(annotation_editing::SELECTION_COLOR)
                                },
                                window,
                            );
                        }
                    },
                )
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .into_any_element()
            }))
            .child(
                div()
                    .id("annotation-overlay")
                    .test_support()
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full()
                    .child(
                        canvas(
                            move |_, _, _| annotations,
                            move |bounds, items, window, _| {
                                for a in &items {
                                    paint_annotation(a, geometry, bounds, window);
                                }
                            },
                        )
                        .size_full(),
                    ),
            )
            .children(notes.into_iter().filter_map(|(index, a)| {
                let position = geometry.display(*a.points.first()?);
                let (w, h) = geometry.size();
                let expected = self.books[self.active].annotations.history.items[index].clone();
                let source = self.books[self.active].source.clone();
                let revision = self.books[self.active].annotations.history.revision();
                Some(
                    div()
                        .id(("page-annotation-text", index))
                        .test_support()
                        .absolute()
                        .left(px(position.x / w * width))
                        .top(px(position.y / h * height))
                        .max_w(px((220. * width / w).min(width)))
                        .text_color(Rgba {
                            r: a.color[0],
                            g: a.color[1],
                            b: a.color[2],
                            a: a.opacity,
                        })
                        .text_size(px(13. * width / w))
                        .border_1()
                        .border_color(if self.is_selected_annotation(index) {
                            rgb(annotation_editing::SELECTION_COLOR).into()
                        } else {
                            transparent_black()
                        })
                        .child(a.text)
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                                cx.stop_propagation();
                                let book = &this.books[this.active];
                                if book.source != source
                                    || book.annotations.history.revision() != revision
                                    || book.annotations.history.items.get(index) != Some(&expected)
                                {
                                    return;
                                }
                                this.select_annotation(index, window, cx);
                                if event.click_count >= 2 {
                                    this.edit_selected_annotation_text(window, cx);
                                } else if let Some(point) = this.document_pointer(event.position) {
                                    this.begin_annotation_move(point);
                                }
                            }),
                        ),
                )
            }))
            .children(self.annotation_edit_layers(current, geometry, width, height, cx))
            .into_any_element()
    }
}
fn annotation_stroke(width: f32, device_scale: f32) -> PathBuilder {
    PathBuilder::stroke(px(width)).with_style(PathStyle::Stroke(
        StrokeOptions::default()
            .with_line_width(width)
            .with_line_cap(LineCap::Round)
            .with_line_join(LineJoin::Round)
            // Tessellate after the page-to-window transform, at device-pixel precision.
            .with_tolerance(0.1 / device_scale.max(1.)),
    ))
}

fn annotation_polyline(
    points: &[DocPoint],
    closed: bool,
    map: impl Fn(DocPoint) -> Point<gpui_kit::Pixels>,
    width: f32,
    device_scale: f32,
) -> Result<gpui_kit::Path<gpui_kit::Pixels>> {
    let mut combined: Option<gpui_kit::Path<gpui_kit::Pixels>> = None;
    // GPUI's tessellator indexes with u16. Round joins on a permitted 20,000-point
    // pen stroke can exceed that limit. Overlap chunk endpoints and merge their
    // triangles into one painted path so opacity is applied only once.
    let stride = if closed { points.len().max(1) } else { 128 };
    for start in (0..points.len()).step_by(stride) {
        let chunk = &points[start..(start + stride + 1).min(points.len())];
        if chunk.len() < 2 {
            break;
        }
        let mut path = annotation_stroke(width, device_scale);
        path.move_to(map(chunk[0]));
        for p in &chunk[1..] {
            path.line_to(map(*p));
        }
        if closed {
            path.close();
        }
        let path = path.build()?;
        if let Some(combined) = &mut combined {
            combined.bounds = combined.bounds.union(&path.bounds);
            combined.vertices.extend(path.vertices);
        } else {
            combined = Some(path);
        }
    }
    Ok(combined.unwrap_or_else(|| gpui_kit::Path::new(Point::default())))
}

/// Maps a pointer position on one page surface back to unrotated PDF user space,
/// clamped to the page so a drag past its edge still selects its text.
fn document_point(
    geometry: ViewGeometry,
    bounds: Bounds<gpui_kit::Pixels>,
    position: gpui_kit::Point<gpui_kit::Pixels>,
) -> Option<DocPoint> {
    let (w, h) = geometry.size();
    let x = f32::from(position.x - bounds.origin.x) / f32::from(bounds.size.width) * w;
    let y = f32::from(position.y - bounds.origin.y) / f32::from(bounds.size.height) * h;
    Some(geometry.document(DocPoint::new(x.clamp(0., w), y.clamp(0., h))))
}

/// Fills one user-space rectangle where the page surface shows it, following the
/// page's own rotation and the reader's view rotation.
fn paint_doc_rect(
    rect: Rect,
    geometry: ViewGeometry,
    bounds: Bounds<gpui_kit::Pixels>,
    color: Rgba,
    window: &mut Window,
) {
    let (w, h) = geometry.size();
    let sx = f32::from(bounds.size.width) / w;
    let sy = f32::from(bounds.size.height) / h;
    let start = geometry.display(DocPoint::new(rect.x, rect.y));
    let end = geometry.display(DocPoint::new(rect.x + rect.width, rect.y + rect.height));
    let origin = point(
        bounds.origin.x + px(start.x.min(end.x) * sx),
        bounds.origin.y + px(start.y.min(end.y) * sy),
    );
    window.paint_quad(fill(
        Bounds {
            origin,
            size: size(
                px((start.x - end.x).abs() * sx),
                px((start.y - end.y).abs() * sy),
            ),
        },
        color,
    ));
}

fn paint_annotation(
    a: &Annotation,
    geometry: ViewGeometry,
    bounds: Bounds<gpui_kit::Pixels>,
    window: &mut Window,
) {
    let (w, h) = geometry.size();
    let sx = f32::from(bounds.size.width) / w;
    let sy = f32::from(bounds.size.height) / h;
    let map = |p: DocPoint| {
        let p = geometry.display(p);
        point(
            bounds.origin.x + px(p.x * sx),
            bounds.origin.y + px(p.y * sy),
        )
    };
    let color = Rgba {
        r: a.color[0],
        g: a.color[1],
        b: a.color[2],
        a: a.opacity,
    };
    let stroke = |points: Vec<DocPoint>, closed: bool, window: &mut Window| {
        if let Ok(path) = annotation_polyline(
            &points,
            closed,
            map,
            (a.width * sx).max(0.5),
            window.scale_factor(),
        ) {
            window.paint_path(path, color);
        }
    };
    let curve = |segments: Vec<curves::Cubic>, closed: bool, window: &mut Window| {
        let Some(first) = segments.first() else {
            return;
        };
        let mut path = annotation_stroke((a.width * sx).max(0.5), window.scale_factor());
        path.move_to(map(first.start));
        for segment in segments {
            path.cubic_bezier_to(
                map(segment.end),
                map(segment.control1),
                map(segment.control2),
            );
        }
        if closed {
            path.close();
        }
        if let Ok(path) = path.build() {
            window.paint_path(path, color);
        }
    };
    let fill_rect = |r: Rect, window: &mut Window| {
        paint_doc_rect(r, geometry, bounds, color, window);
    };
    let Some(first) = a.points.first().copied() else {
        return;
    };
    let last = a.points.last().copied().unwrap_or(first);
    let r = Rect::between(first, last);
    match a.kind {
        Kind::Highlight => {
            for q in &a.quads {
                fill_rect(*q, window);
            }
        }
        Kind::Underline | Kind::Strikeout => {
            for q in &a.quads {
                let y = q.y
                    + if a.kind == Kind::Strikeout {
                        q.height / 2.
                    } else {
                        0.
                    };
                stroke(
                    vec![DocPoint::new(q.x, y), DocPoint::new(q.x + q.width, y)],
                    false,
                    window,
                );
            }
        }
        Kind::Area => fill_rect(r, window),
        Kind::Rectangle => stroke(
            vec![
                DocPoint::new(r.x, r.y),
                DocPoint::new(r.x + r.width, r.y),
                DocPoint::new(r.x + r.width, r.y + r.height),
                DocPoint::new(r.x, r.y + r.height),
            ],
            true,
            window,
        ),
        Kind::Ellipse => curve(curves::ellipse(r).to_vec(), true, window),
        Kind::Ink => stroke(a.points.clone(), false, window),
        Kind::Line => stroke(vec![first, last], false, window),
        Kind::Arrow => {
            stroke(vec![first, last], false, window);
            let angle = (last.y - first.y).atan2(last.x - first.x);
            let length = (a.width * 5.).max(8.);
            for side in [-0.5, 0.5] {
                stroke(
                    vec![
                        last,
                        DocPoint::new(
                            last.x - length * (angle + side).cos(),
                            last.y - length * (angle + side).sin(),
                        ),
                    ],
                    false,
                    window,
                );
            }
        }
        Kind::Squiggle => curve(
            curves::squiggle(first, last, a.width).collect(),
            false,
            window,
        ),
        Kind::Note => {}
    }
}
