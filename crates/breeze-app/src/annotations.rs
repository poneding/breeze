//! Native overlays and explicit, conflict-checked sidecar saves; no PDF content writes.
use crate::*;
use breeze_core::annotations::ViewGeometry;
use breeze_core::annotations::{Annotation, History, Kind, Point as DocPoint, Rect};
use breeze_storage::{AnnotationConflict, AnnotationFile};
use gpui_kit::component::ElementExt;

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
    fn document_pointer(&self, position: gpui_kit::Point<gpui_kit::Pixels>) -> Option<DocPoint> {
        let bounds = self.surface_bounds?;
        let preview = self.preview.as_ref()?;
        let geometry = self.view_geometry(preview.geometry);
        let (w, h) = geometry.size();
        let x = f32::from(position.x - bounds.origin.x) / f32::from(bounds.size.width) * w;
        let y = f32::from(position.y - bounds.origin.y) / f32::from(bounds.size.height) * h;
        Some(geometry.document(DocPoint::new(x.clamp(0., w), y.clamp(0., h))))
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
        self.selected_text.clear();
        if self.books[self.active].state.mode == Mode::Annotate {
            let data = &self.books[self.active].annotations;
            if !data.ready || data.error.is_some() {
                self.message =
                    tr!("Annotation file is not ready; check storage or conflict messages").into();
                cx.notify();
                return;
            }
            if self.tool == 5 {
                let current = self.books[self.active].state.navigation.page();
                let data = &mut self.books[self.active].annotations;
                let target = data.history.items.iter().rposition(|a| {
                    a.page == current
                        && a.kind == Kind::Ink
                        && a.points
                            .windows(2)
                            .any(|s| distance_to_segment(point, s[0], s[1]) < 8.)
                });
                if let Some(index) = target {
                    let mut items = data.history.items.clone();
                    items.remove(index);
                    data.history.replace(items);
                }
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
                6 => Kind::Note,
                7 => Kind::Ellipse,
                8 => Kind::Rectangle,
                9 => Kind::Line,
                10 => Kind::Squiggle,
                11 => Kind::Arrow,
                _ => return None,
            }
        };
        let region = Rect::between(first, last);
        let quads = if kind.text_markup() {
            self.preview
                .as_ref()?
                .glyphs
                .iter()
                .filter(|g| {
                    region.contains(DocPoint::new(
                        g.bounds.x + g.bounds.width / 2.,
                        g.bounds.y + g.bounds.height / 2.,
                    ))
                })
                .map(|g| g.bounds)
                .collect()
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
        if self.books[self.active].state.mode == Mode::Annotate && self.tool == 6 {
            let point = self.gesture[0];
            self.begin_text_annotation(point, window, cx);
            return;
        }
        let region = Rect::between(
            self.gesture[0],
            *self.gesture.last().unwrap_or(&self.gesture[0]),
        );
        if self.books[self.active].state.mode == Mode::Read {
            if let Some(preview) = &self.preview {
                self.selected_text = preview
                    .glyphs
                    .iter()
                    .filter(|g| {
                        region.contains(DocPoint::new(
                            g.bounds.x + g.bounds.width / 2.,
                            g.bounds.y + g.bounds.height / 2.,
                        ))
                    })
                    .map(|g| g.character)
                    .collect();
            }
            self.message = tf!(
                "{} characters selected · ⌘/Ctrl+C to copy",
                self.selected_text.chars().count()
            );
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
        div()
            .id("page-surface")
            .test_support()
            .relative()
            .w(px(width))
            .h(px(height))
            .flex_shrink_0()
            .overflow_hidden()
            .on_prepaint(move |bounds, _, cx| {
                capture_bounds.set(Some(bounds));
                view.update(cx, |this, _| {
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
            .on_mouse_move(cx.listener(move |this, event, window, cx| {
                if current == this.books[this.active].state.navigation.page() {
                    this.pointer_move(event, window, cx);
                }
            }))
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
                            Theme::global(cx).primary
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
fn distance_to_segment(p: DocPoint, a: DocPoint, b: DocPoint) -> f32 {
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    let t = (((p.x - a.x) * dx + (p.y - a.y) * dy) / (dx * dx + dy * dy).max(0.001)).clamp(0., 1.);
    ((p.x - a.x - t * dx).powi(2) + (p.y - a.y - t * dy).powi(2)).sqrt()
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
        if points.is_empty() {
            return;
        }
        let mut path = PathBuilder::stroke(px((a.width * sx).max(0.5)));
        path.move_to(map(points[0]));
        for p in points.iter().skip(1) {
            path.line_to(map(*p));
        }
        if closed {
            path.close();
        }
        if let Ok(path) = path.build() {
            window.paint_path(path, color);
        }
    };
    let fill_rect = |r: Rect, window: &mut Window| {
        let start = map(DocPoint::new(r.x, r.y));
        let end = map(DocPoint::new(r.x + r.width, r.y + r.height));
        let origin = point(start.x.min(end.x), start.y.min(end.y));
        window.paint_quad(fill(
            Bounds {
                origin,
                size: size((start.x - end.x).abs(), (start.y - end.y).abs()),
            },
            color,
        ));
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
        Kind::Ellipse => stroke(
            (0..48)
                .map(|i| {
                    let angle = i as f32 / 48. * std::f32::consts::TAU;
                    DocPoint::new(
                        r.x + r.width / 2. + angle.cos() * r.width / 2.,
                        r.y + r.height / 2. + angle.sin() * r.height / 2.,
                    )
                })
                .collect(),
            true,
            window,
        ),
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
        Kind::Squiggle => {
            let dx = last.x - first.x;
            let dy = last.y - first.y;
            let length = (dx * dx + dy * dy).sqrt().max(1.);
            let steps = (length / 2.).ceil().clamp(2., 2000.) as usize;
            stroke(
                (0..=steps)
                    .map(|i| {
                        let t = i as f32 / steps as f32;
                        let wave = (t * length / 8. * std::f32::consts::TAU).sin() * a.width;
                        DocPoint::new(
                            first.x + dx * t - dy / length * wave,
                            first.y + dy * t + dx / length * wave,
                        )
                    })
                    .collect(),
                false,
                window,
            );
        }
        Kind::Note => {}
    }
}
