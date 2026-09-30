//! Native overlays and explicit, conflict-checked sidecar saves; no PDF content writes.
use crate::*;
use breeze_core::annotations::{Annotation, History, Kind, PageGeometry, Point as DocPoint, Rect};
use breeze_storage::AnnotationFile;
use gpui_kit::component::ElementExt;

#[derive(Default)]
pub(super) struct DocumentAnnotations {
    pub history: History,
    pub autosave: Option<Task<()>>,
    pub saved: Vec<Annotation>,
    pub fingerprint: String,
    pub revision: Option<String>,
    pub path: Option<PathBuf>,
    pub ready: bool,
    pub source_conflict: bool,
    pub saving: bool,
    pub error: Option<String>,
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
        if book.annotations.dirty() {
            book.annotations.source_conflict = true;
            book.annotations.error = Some("源文档已变化，保留未保存批注并暂停写入".into());
            self.message = "源文档已变化：旧批注已隐藏，写入已暂停".into();
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
        let work = cx
            .background_spawn(async move { breeze_storage::load_annotations(&path, &fingerprint) });
        cx.spawn(async move |this, cx| {
            let result = work.await;
            let _ = this.update(cx, |this, cx| {
                let Some(book) = this.books.iter_mut().find(|b| b.source == source) else {
                    return;
                };
                if book.annotations.fingerprint != expected_fingerprint
                    || book.annotations.path.as_ref() != Some(&expected_path)
                {
                    return;
                }
                match result {
                    Ok(AnnotationFile {
                        annotations,
                        revision,
                    }) => {
                        if annotations
                            .iter()
                            .any(|a| a.page >= book.state.navigation.total())
                        {
                            book.annotations.error = Some("批注页码超出文档范围，未载入".into());
                            cx.notify();
                            return;
                        }
                        book.annotations.saved = annotations.clone();
                        book.annotations.history.items = annotations;
                        book.annotations.revision = revision;
                        book.annotations.ready = true;
                    }
                    Err(e) => book.annotations.error = Some(format!("无法加载批注：{e:#}")),
                }
                cx.notify();
            });
        })
        .detach();
    }
    pub(super) fn save_annotations(&mut self, cx: &mut Context<Self>) {
        self.save_annotations_for(self.active, cx);
    }
    fn save_annotations_for(&mut self, index: usize, cx: &mut Context<Self>) {
        let book = &mut self.books[index];
        let a = &mut book.annotations;
        if !a.ready || a.saving || a.error.is_some() {
            self.message = "批注未就绪或存在冲突，不能保存".into();
            cx.notify();
            return;
        }
        let Some(path) = a.path.clone() else {
            return;
        };
        let source = book.source.clone();
        let fingerprint = a.fingerprint.clone();
        let revision = a.revision.clone();
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
                if book.annotations.fingerprint != saved_fingerprint
                    || book.annotations.path.as_ref() != Some(&saved_path)
                {
                    return;
                }
                book.annotations.saving = false;
                match result {
                    Ok(revision) => {
                        book.annotations.revision = Some(revision);
                        book.annotations.saved = saved;
                        this.message = format!("批注已保存：{destination}");
                    }
                    Err(e) => {
                        book.annotations.error = Some(format!("保存失败（未覆盖原文件）：{e:#}"));
                        this.message = format!("批注保存失败：{e:#}");
                    }
                }
                if let Some(index) = this.books.iter().position(|b| b.source == source)
                    && this.books[index].annotations.dirty()
                    && this.books[index].annotations.error.is_none()
                {
                    this.schedule_annotation_save(index, cx);
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
    pub(super) fn schedule_annotation_save(&mut self, index: usize, cx: &mut Context<Self>) {
        if !self.books[index].annotations.ready
            || self.books[index].annotations.path.is_none()
            || self.books[index].annotations.error.is_some()
        {
            return;
        }
        let source = self.books[index].source.clone();
        let timer = cx
            .background_executor()
            .timer(std::time::Duration::from_millis(500));
        let task = cx.spawn(async move |this, cx| {
            timer.await;
            let _ = this.update(cx, |this, cx| {
                if let Some(index) = this.books.iter().position(|b| b.source == source)
                    && this.books[index].annotations.dirty()
                    && !this.books[index].annotations.saving
                {
                    this.save_annotations_for(index, cx);
                }
            });
        });
        self.books[index].annotations.autosave = Some(task);
    }
    pub(super) fn save_annotations_as(&mut self, cx: &mut Context<Self>) {
        if self.books[self.active].annotations.fingerprint.is_empty() {
            self.message = "文档指纹尚未就绪".into();
            cx.notify();
            return;
        }
        let source = self.books[self.active].source.clone();
        let directory = self.data_dir.clone().unwrap_or_else(std::env::temp_dir);
        let receiver = cx.prompt_for_new_path(&directory, Some("annotations.breeze.json"));
        cx.spawn(async move |this, cx| {
            if let Ok(Ok(Some(path))) = receiver.await {
                let _ = this.update(cx, |this, cx| {
                    if let Some(index) = this.books.iter().position(|b| b.source == source) {
                        let a = &mut this.books[index].annotations;
                        if a.saving {
                            return;
                        }
                        a.path = Some(path);
                        a.revision = None;
                        a.error = None;
                        a.ready = true;
                        this.save_annotations_for(index, cx);
                    }
                });
            }
        })
        .detach();
    }
    pub(super) fn has_unsaved_annotations(&self) -> bool {
        self.books
            .iter()
            .any(|b| b.annotations.dirty() || b.annotations.saving)
    }
    pub(super) fn allow_close(&mut self, cx: &mut Context<Self>) -> bool {
        if self.export_busy {
            self.message = "请等待 PDF 导出完成或取消文件选择".into();
            cx.notify();
            return false;
        }
        if self.has_unsaved_annotations() {
            self.message = "存在未保存批注，请切换到对应文档并按 ⌘/Ctrl+S 保存后关闭".into();
            cx.notify();
            return false;
        }
        true
    }
    fn document_pointer(&self, position: gpui_kit::Point<gpui_kit::Pixels>) -> Option<DocPoint> {
        let bounds = self.surface_bounds?;
        let preview = self.preview.as_ref()?;
        let (w, h) = preview.points;
        let x = f32::from(position.x - bounds.origin.x) / f32::from(bounds.size.width) * w;
        let y = f32::from(position.y - bounds.origin.y) / f32::from(bounds.size.height) * h;
        Some(
            preview
                .geometry
                .document(DocPoint::new(x.clamp(0., w), y.clamp(0., h))),
        )
    }
    fn pointer_down(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.loading || self.books[self.active].state.mode == Mode::Edit {
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
                self.message = "批注文件未就绪；请检查存储或冲突提示".into();
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
                    self.schedule_annotation_save(self.active, cx);
                }
                cx.notify();
                return;
            }
        }
        self.gesture = vec![point];
        cx.notify();
    }
    fn pointer_move(&mut self, event: &MouseMoveEvent, _: &mut Window, cx: &mut Context<Self>) {
        if self.gesture.is_empty() {
            return;
        }
        let Some(point) = self.document_pointer(event.position) else {
            return;
        };
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
        let hex = [0x365f48, 0xd9b657, 0xcc8167, 0x829fbd][self.color];
        Some(Annotation {
            page: self.books[self.active].state.navigation.page(),
            kind,
            points: if self.gesture.len() == 1 {
                vec![first, last]
            } else {
                self.gesture.clone()
            },
            quads,
            color: [
                ((hex >> 16) & 255) as f32 / 255.,
                ((hex >> 8) & 255) as f32 / 255.,
                (hex & 255) as f32 / 255.,
            ],
            opacity: if matches!(kind, Kind::Area | Kind::Highlight) {
                0.28 * (self.opacity as f32 / 100.)
            } else {
                self.opacity as f32 / 100.
            },
            width: (self.font_size as f32 / 6.).clamp(0.5, 10.),
            text: String::new(),
        })
    }
    fn pointer_up(&mut self, _: &MouseUpEvent, _: &mut Window, cx: &mut Context<Self>) {
        if self.gesture.is_empty() {
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
            self.message = format!(
                "已选择 {} 个字符 · ⌘/Ctrl+C 复制",
                self.selected_text.chars().count()
            );
        } else if let Some(mut annotation) = self.draft() {
            if annotation.kind == Kind::Note {
                annotation.text = self.text_input.read(cx).value().to_string();
            }
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
                        self.schedule_annotation_save(self.active, cx);
                        self.message = "批注已修改，正在自动保存…".into();
                    } else {
                        self.message = "当前文档批注数量已达上限".into();
                    }
                }
                Err(e) => self.message = format!("未创建批注：{e}"),
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
        let geometry = preview.geometry;
        let mut annotations: Vec<_> = self.books[self.active]
            .annotations
            .history
            .items
            .iter()
            .filter(|a| a.page == current && !self.books[self.active].annotations.source_conflict)
            .cloned()
            .collect();
        if let Some(draft) = self.draft().filter(|a| a.page == current) {
            annotations.push(draft);
        }
        let notes = annotations
            .iter()
            .filter(|a| a.kind == Kind::Note)
            .cloned()
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
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event, window, cx| {
                    this.books[this.active].state.navigation.go_to(current);
                    this.preview = Some(page_preview.clone());
                    this.surface_bounds = bounds_cell.get();
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
            .child(
                img(preview.image.clone())
                    .size_full()
                    .object_fit(ObjectFit::Contain),
            )
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
            .children(notes.into_iter().filter_map(|a| {
                let position = geometry.display(*a.points.first()?);
                let (w, h) = geometry.size();
                Some(
                    div()
                        .absolute()
                        .left(px(position.x / w * width))
                        .top(px(position.y / h * height))
                        .max_w(px(220.))
                        .p_1()
                        .bg(rgb(0xfff0b3))
                        .text_color(rgb(0x342d19))
                        .text_size(px(13.))
                        .child(a.text),
                )
            }))
            .into_any_element()
    }
    pub(super) fn annotation_inspector(&self, cx: &mut Context<Self>) -> Div {
        let p = self.p();
        let data = &self.books[self.active].annotations;
        let page = self.books[self.active].state.navigation.page();
        column()
            .w(px(232.))
            .flex_shrink_0()
            .bg(p.bg)
            .border_l_1()
            .border_color(p.line)
            .child(
                column()
                    .id("annotation-properties")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .p_3()
                    .gap_3()
                    .child(
                        row()
                            .gap_1()
                            .child(
                                div()
                                    .flex_1()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child("批注"),
                            )
                            .child(
                                self.command(
                                    "undo",
                                    IconName::Undo2,
                                    "撤销批注",
                                    &UndoAnnotation,
                                    cx,
                                )
                                .disabled(!data.history.can_undo()),
                            )
                            .child(
                                self.command(
                                    "redo",
                                    IconName::Redo2,
                                    "重做批注",
                                    &RedoAnnotation,
                                    cx,
                                )
                                .disabled(!data.history.can_redo()),
                            )
                            .child(
                                self.command(
                                    "save-sidecar",
                                    IconName::Save,
                                    "保存批注",
                                    &SaveAnnotations,
                                    cx,
                                )
                                .disabled(!data.ready || data.saving || data.error.is_some()),
                            ),
                    )
                    .child(
                        div()
                            .text_size(px(12.))
                            .text_color(p.muted)
                            .child(if data.saving {
                                "正在保存…"
                            } else if data.dirty() {
                                "有未保存批注"
                            } else if data.ready {
                                "批注已载入"
                            } else {
                                "批注尚未就绪"
                            }),
                    )
                    .child(self.export_controls(cx))
                    .child(self.command(
                        "save-sidecar-as",
                        IconName::FolderOpen,
                        "选择新的批注保存位置",
                        &SaveAnnotationsAs,
                        cx,
                    ))
                    .when_some(data.error.as_ref(), |d, e| {
                        d.child(div().text_color(rgb(0xb46a40)).child(e.clone()))
                    })
                    .child(
                        row().gap_2().children(
                            [0x365f48, 0xd9b657, 0xcc8167, 0x829fbd]
                                .into_iter()
                                .enumerate()
                                .map(|(i, color)| {
                                    Button::new(("ink-color", i))
                                        .small()
                                        .ghost()
                                        .label("●")
                                        .text_color(rgb(color))
                                        .selected(self.color == i)
                                        .tooltip("批注颜色")
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            this.color = i;
                                            cx.notify();
                                        }))
                                }),
                        ),
                    )
                    .child(
                        Button::new("ink-width")
                            .small()
                            .label(format!("线宽 {} pt", self.font_size / 6))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.font_size = if this.font_size >= 30 {
                                    6
                                } else {
                                    this.font_size + 6
                                };
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new("ink-opacity")
                            .small()
                            .label(format!("透明度 {}%", self.opacity))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.opacity = if this.opacity <= 25 {
                                    100
                                } else {
                                    this.opacity - 25
                                };
                                cx.notify();
                            })),
                    )
                    .when(self.tool == 6, |d| {
                        d.child(Input::new(&self.text_input).small()).child(
                            div()
                                .text_size(px(11.))
                                .text_color(p.muted)
                                .child("输入便笺内容后点击页面放置"),
                        )
                    })
                    .child(div().text_size(px(11.)).text_color(p.muted).child(
                        "拖动创建批注；文字标记只作用于选框内识别到的字形。橡皮擦只删除笔迹。",
                    ))
                    .children(
                        data.history
                            .items
                            .iter()
                            .enumerate()
                            .filter(|(_, a)| a.page == page)
                            .map(|(i, a)| {
                                row()
                                    .gap_1()
                                    .child(div().flex_1().text_size(px(12.)).child(format!(
                                        "{} · {:?}",
                                        i + 1,
                                        a.kind
                                    )))
                                    .child(
                                        Button::new(("remove-annotation", i))
                                            .ghost()
                                            .small()
                                            .icon(IconName::Trash)
                                            .accessibility_label("删除此批注")
                                            .tooltip("只删除批注，不修改原文")
                                            .on_click(cx.listener(move |this, _, _, cx| {
                                                let h = &mut this.books[this.active]
                                                    .annotations
                                                    .history;
                                                let mut items = h.items.clone();
                                                items.remove(i);
                                                h.replace(items);
                                                this.schedule_annotation_save(this.active, cx);
                                                cx.notify();
                                            })),
                                    )
                            }),
                    )
                    .child(div().text_size(px(11.)).text_color(p.muted).child(
                        "自动保存到旁置 .breeze.json；PDF 导出写新副本。原文编辑仍未开放。",
                    )),
            )
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
    geometry: PageGeometry,
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
