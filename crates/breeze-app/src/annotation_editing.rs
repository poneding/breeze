use crate::*;
use breeze_core::annotations::{Annotation, Kind, PageGeometry, Point as DocPoint};
use gpui_kit::component::{
    Theme,
    input::NumberInput,
    slider::{Slider, SliderEvent, SliderState},
};

mod geometry;
#[cfg(test)]
mod tests;

pub(super) const SELECT_TOOL: usize = 12;

#[derive(Clone)]
struct Target {
    source: Source,
    fingerprint: String,
    index: usize,
    revision: u64,
    expected: Annotation,
}

impl Target {
    fn matches(&self, book: &Book) -> bool {
        editable(book)
            && book.source == self.source
            && book.annotations.fingerprint == self.fingerprint
            && book.annotations.history.revision() == self.revision
            && book.annotations.history.items.get(self.index) == Some(&self.expected)
    }
}

fn editable(book: &Book) -> bool {
    book.annotations.ready && !book.annotations.source_conflict && book.annotations.error.is_none()
}

struct Selection {
    target: Target,
    preview: Annotation,
    serial: u64,
    width: Entity<InputState>,
    opacity: Entity<SliderState>,
    _subscriptions: Vec<Subscription>,
}

struct TextEditor {
    source: Source,
    fingerprint: String,
    annotation: Annotation,
    original: Option<Target>,
    input: Entity<InputState>,
    _subscriptions: Vec<Subscription>,
}

struct MoveDraft {
    start: DocPoint,
    original: Annotation,
    geometry: PageGeometry,
}

#[derive(Default)]
pub(super) struct AnnotationEditing {
    selection: Option<Selection>,
    editor: Option<TextEditor>,
    moving: Option<MoveDraft>,
    serial: u64,
}

impl AnnotationEditing {
    pub fn pending_text(&self) -> bool {
        self.editor.is_some()
    }
    pub fn interacting(&self) -> bool {
        self.editor.is_some() || self.moving.is_some()
    }
    pub fn clear(&mut self) {
        self.selection = None;
        self.moving = None;
        self.serial = self.serial.wrapping_add(1);
    }
}

impl Breeze {
    pub(super) fn annotation_navigation_allowed(&mut self, cx: &mut Context<Self>) -> bool {
        if self.annotation_editing.editor.is_some() {
            self.message = "请按 Enter 完成文本标注，或按 Esc 取消，再切换位置".into();
            cx.notify();
            return false;
        }
        self.commit_selected_annotation(cx);
        self.annotation_editing.clear();
        true
    }

    pub(super) fn go_to_annotation_page(&mut self, page: usize, cx: &mut Context<Self>) {
        if !self.annotation_navigation_allowed(cx) {
            return;
        }
        self.books[self.active].state.navigation.go_to(page);
        self.load(cx);
    }

    pub(super) fn cancel_annotation_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.annotation_editing.editor = None;
        self.annotation_editing.clear();
        self.gesture.clear();
        self.message.clear();
        window.focus(&self.focus, cx);
        cx.notify();
    }

    pub(super) fn begin_text_annotation(
        &mut self,
        point: DocPoint,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.annotation_editing.editor.is_some() {
            self.focus_annotation_editor(window, cx);
            return;
        }
        let style = self.tool_settings.annotation[6];
        let annotation = Annotation {
            page: self.books[self.active].state.navigation.page(),
            kind: Kind::Note,
            points: vec![point],
            quads: Vec::new(),
            color: colors::components(style.color),
            opacity: style.opacity as f32 / 100.,
            width: style.width,
            text: String::new(),
        };
        self.open_annotation_text_editor(annotation, None, window, cx);
    }

    fn open_annotation_text_editor(
        &mut self,
        annotation: Annotation,
        original: Option<Target>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !editable(&self.books[self.active]) {
            return;
        }
        self.gesture.clear();
        self.tool_settings.dismiss();
        self.annotation_editing.clear();
        let input = cx.new(|cx| {
            InputState::new(window, cx)
                .default_value(annotation.text.clone())
                .placeholder("输入文本，Enter 完成")
                .validate(|text, _| text.len() <= 16_384)
        });
        let subscription = cx.subscribe_in(&input, window, |this, input, event, window, cx| {
            if !this
                .annotation_editing
                .editor
                .as_ref()
                .is_some_and(|editor| editor.input == *input)
            {
                return;
            }
            if matches!(
                event,
                InputEvent::PressEnter {
                    shift: false,
                    secondary: false
                }
            ) {
                // An IME confirmation belongs to the input method, not the annotation transaction.
                let composing = input.update(cx, |input, cx| {
                    input.marked_text_range(window, cx).is_some()
                });
                if !composing {
                    this.commit_text_annotation(window, cx);
                }
            }
        });
        let observer = cx.observe(&input, |_, _, cx| cx.notify());
        let book = &self.books[self.active];
        self.annotation_editing.editor = Some(TextEditor {
            source: book.source.clone(),
            fingerprint: book.annotations.fingerprint.clone(),
            annotation,
            original,
            input: input.clone(),
            _subscriptions: vec![subscription, observer],
        });
        input.update(cx, |input, cx| input.focus(window, cx));
        self.message = "Enter 完成文本标注 · Esc 取消".into();
        cx.notify();
    }

    pub(super) fn focus_annotation_editor(&self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(editor) = &self.annotation_editing.editor {
            let input = editor.input.clone();
            input.update(cx, |input, cx| input.focus(window, cx));
        }
    }

    fn commit_text_annotation(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(editor) = &self.annotation_editing.editor else {
            return;
        };
        let mut annotation = editor.annotation.clone();
        annotation.text = editor.input.read(cx).value().to_string();
        if let Err(error) = annotation.validate() {
            self.message = format!("无法完成文本标注：{error}");
            cx.notify();
            return;
        }
        let book = &mut self.books[self.active];
        if !editable(book)
            || book.source != editor.source
            || book.annotations.fingerprint != editor.fingerprint
            || editor
                .original
                .as_ref()
                .is_some_and(|target| !target.matches(book))
        {
            self.message = "标注或文档已变化，输入内容已保留；请复制内容后按 Esc 取消".into();
            cx.notify();
            return;
        }
        let index = if let Some(original) = &editor.original {
            let mut items = book.annotations.history.items.clone();
            if items[original.index] != annotation {
                items[original.index] = annotation;
                book.annotations.history.replace(items);
            }
            original.index
        } else {
            if !can_add(book, &annotation) {
                self.message = "当前文档标注数量已达上限，输入内容已保留".into();
                cx.notify();
                return;
            }
            let index = book.annotations.history.items.len();
            book.annotations.history.add(annotation);
            index
        };
        self.annotation_editing.editor = None;
        self.schedule_annotation_save(self.active, cx);
        self.message = "文本标注已完成".into();
        self.select_annotation(index, window, cx);
        window.focus(&self.focus, cx);
        cx.notify();
    }

    pub(super) fn select_annotation(
        &mut self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.annotation_editing.editor.is_some() {
            self.focus_annotation_editor(window, cx);
            return;
        }
        self.commit_selected_annotation(cx);
        let book = &self.books[self.active];
        let Some(annotation) = book
            .annotations
            .history
            .items
            .get(index)
            .filter(|_| editable(book))
            .cloned()
        else {
            return;
        };
        let target = Target {
            source: book.source.clone(),
            fingerprint: book.annotations.fingerprint.clone(),
            index,
            revision: book.annotations.history.revision(),
            expected: annotation.clone(),
        };
        self.annotation_editing.clear();
        let serial = self.annotation_editing.serial;
        let width = cx.new(|cx| {
            InputState::new(window, cx)
                .default_value(annotation.width.to_string())
                .step(0.25)
                .min(0.25)
                .max(100.)
        });
        let opacity = cx.new(|_| {
            SliderState::new()
                .min(0.)
                .max(100.)
                .step(1.)
                .default_value(annotation.opacity * 100.)
        });
        let width_subscription =
            cx.subscribe_in(&width, window, move |this, input, event, window, cx| {
                let Some(selected) = this
                    .annotation_editing
                    .selection
                    .as_mut()
                    .filter(|selected| selected.serial == serial && selected.width == *input)
                else {
                    return;
                };
                let value = input
                    .read(cx)
                    .value()
                    .trim()
                    .parse::<f32>()
                    .ok()
                    .filter(|value| value.is_finite() && (0.25..=100.).contains(value));
                if let Some(value) = value {
                    selected.preview.width = value;
                }
                if matches!(event, InputEvent::PressEnter { .. } | InputEvent::Blur) {
                    let value = selected.preview.width;
                    input.update(cx, |input, cx| {
                        input.set_value(value.to_string(), window, cx)
                    });
                    this.commit_selected_annotation(cx);
                }
                cx.notify();
            });
        let opacity_subscription = cx.subscribe(&opacity, move |this, slider, event, cx| {
            let Some(selected) = this
                .annotation_editing
                .selection
                .as_mut()
                .filter(|selected| selected.serial == serial && selected.opacity == slider)
            else {
                return;
            };
            match event {
                SliderEvent::Change(value) => {
                    selected.preview.opacity = value.end().clamp(0., 100.) / 100.
                }
                SliderEvent::Release(_) => {
                    this.commit_selected_annotation(cx);
                }
            }
            cx.notify();
        });
        self.annotation_editing.selection = Some(Selection {
            target,
            preview: annotation,
            serial,
            width,
            opacity,
            _subscriptions: vec![width_subscription, opacity_subscription],
        });
        self.tool_settings.dismiss();
        self.gesture.clear();
        self.selected_text.clear();
        window.focus(&self.focus, cx);
        cx.notify();
    }

    pub(super) fn commit_selected_annotation(&mut self, cx: &mut Context<Self>) {
        let Some(mut selected) = self.annotation_editing.selection.take() else {
            return;
        };
        let book = &mut self.books[self.active];
        if !selected.target.matches(book) {
            self.annotation_editing.clear();
            self.message = "标注已变化，请重新选择".into();
            cx.notify();
            return;
        }
        if selected.preview != selected.target.expected && selected.preview.validate().is_ok() {
            let mut items = book.annotations.history.items.clone();
            items[selected.target.index] = selected.preview.clone();
            book.annotations.history.replace(items);
            selected.target.revision = book.annotations.history.revision();
            selected.target.expected = selected.preview.clone();
            self.schedule_annotation_save(self.active, cx);
        }
        self.annotation_editing.selection = Some(selected);
    }

    pub(super) fn delete_selected_annotation(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.screen != Screen::Reader || self.annotation_editing.editor.is_some() {
            return;
        }
        let Some(selected) = self.annotation_editing.selection.take() else {
            return;
        };
        let book = &mut self.books[self.active];
        if selected.target.matches(book) {
            let mut items = book.annotations.history.items.clone();
            items.remove(selected.target.index);
            book.annotations.history.replace(items);
            self.schedule_annotation_save(self.active, cx);
            self.message = "标注已删除，可撤销".into();
        }
        self.annotation_editing.clear();
        window.focus(&self.focus, cx);
        cx.notify();
    }

    pub(super) fn pointer_annotation_selection(
        &mut self,
        point: DocPoint,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.annotation_editing.editor.is_some() {
            self.focus_annotation_editor(window, cx);
            return true;
        }
        if !editable(&self.books[self.active]) {
            return false;
        }
        let tolerance = self
            .surface_bounds
            .zip(self.preview.as_ref())
            .map_or(5., |(bounds, preview)| {
                5. * preview.points.0 / f32::from(bounds.size.width).max(1.)
            });
        let page = self.books[self.active].state.navigation.page();
        let target = self.books[self.active]
            .annotations
            .history
            .items
            .iter()
            .rposition(|a| a.page == page && geometry::hit(a, point, tolerance));
        if let Some(index) = target {
            self.select_annotation(index, window, cx);
            self.begin_annotation_move(point);
            true
        } else {
            self.commit_selected_annotation(cx);
            self.annotation_editing.clear();
            false
        }
    }

    pub(super) fn begin_annotation_move(&mut self, point: DocPoint) {
        if let (Some(selected), Some(preview)) = (&self.annotation_editing.selection, &self.preview)
            && !selected.preview.kind.text_markup()
        {
            self.annotation_editing.moving = Some(MoveDraft {
                start: point,
                original: selected.preview.clone(),
                geometry: preview.geometry,
            });
        }
    }

    pub(super) fn move_annotation_pointer(
        &mut self,
        point: DocPoint,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(moving) = &self.annotation_editing.moving else {
            return false;
        };
        if let Some(selected) = &mut self.annotation_editing.selection {
            let delta = DocPoint::new(point.x - moving.start.x, point.y - moving.start.y);
            if delta.x.hypot(delta.y) > 0.5 {
                selected.preview = geometry::translated(&moving.original, delta, moving.geometry);
                cx.notify();
            }
        }
        true
    }

    pub(super) fn finish_annotation_move(&mut self, cx: &mut Context<Self>) -> bool {
        if self.annotation_editing.moving.take().is_none() {
            return false;
        }
        self.commit_selected_annotation(cx);
        cx.notify();
        true
    }

    pub(super) fn displayed_annotation(&self, index: usize, annotation: &Annotation) -> Annotation {
        self.annotation_editing
            .selection
            .as_ref()
            .filter(|selected| {
                selected.target.index == index && selected.target.matches(&self.books[self.active])
            })
            .map_or_else(|| annotation.clone(), |selected| selected.preview.clone())
    }

    pub(super) fn text_annotation_being_edited(&self, index: usize) -> bool {
        self.annotation_editing
            .editor
            .as_ref()
            .and_then(|editor| editor.original.as_ref())
            .is_some_and(|target| target.index == index && target.matches(&self.books[self.active]))
    }

    pub(super) fn is_selected_annotation(&self, index: usize) -> bool {
        self.annotation_editing
            .selection
            .as_ref()
            .is_some_and(|selected| {
                selected.target.index == index && selected.target.matches(&self.books[self.active])
            })
    }

    pub(super) fn edit_selected_annotation_text(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.commit_selected_annotation(cx);
        if let Some(selected) = self
            .annotation_editing
            .selection
            .as_ref()
            .filter(|s| s.preview.kind == Kind::Note)
        {
            self.open_annotation_text_editor(
                selected.preview.clone(),
                Some(selected.target.clone()),
                window,
                cx,
            );
        }
    }

    pub(super) fn annotation_edit_layers(
        &self,
        page: usize,
        geometry: PageGeometry,
        width: f32,
        height: f32,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let mut layers = Vec::new();
        let (w, h) = geometry.size();
        if let Some(editor) = &self.annotation_editing.editor
            && editor.source == self.books[self.active].source
            && editor.annotation.page == page
        {
            let point = geometry.display(editor.annotation.points[0]);
            let input_width = (width - 8.).clamp(48., 256.);
            let x = (point.x / w * width).clamp(0., (width - input_width).max(0.));
            let y = (point.y / h * height).clamp(0., (height - 32.).max(0.));
            layers.push(
                div()
                    .id("annotation-text-editor")
                    .test_support()
                    .absolute()
                    .left(px(x))
                    .top(px(y))
                    .w(px(input_width))
                    .key_context("AnnotationText")
                    .occlude()
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .child(Input::new(&editor.input).small())
                    .into_any_element(),
            );
        }
        if let Some(selected) = &self.annotation_editing.selection
            && selected.target.matches(&self.books[self.active])
            && selected.preview.page == page
        {
            let bounds = geometry::display_bounds(&selected.preview, geometry);
            let x = bounds.x / w * width;
            let y = bounds.y / h * height;
            let sw = if selected.preview.kind == Kind::Note {
                80.
            } else {
                bounds.width / w * width
            };
            let sh = if selected.preview.kind == Kind::Note {
                22.
            } else {
                bounds.height / h * height
            };
            if selected.preview.kind != Kind::Note {
                layers.push(
                    div()
                        .absolute()
                        .left(px(x))
                        .top(px(y))
                        .w(px(sw))
                        .h(px(sh))
                        .border_1()
                        .border_color(Theme::global(cx).primary)
                        .into_any_element(),
                );
            }
            if self.annotation_editing.moving.is_none() {
                let panel_width = width.min(260.);
                let panel_x = x.clamp(0., (width - panel_width).max(0.));
                let panel_y = if y >= 212. {
                    y - 208.
                } else {
                    (y + sh + 6.).min((height - 208.).max(0.))
                };
                layers.push(
                    div()
                        .absolute()
                        .left(px(panel_x))
                        .top(px(panel_y))
                        .w(px(panel_width))
                        .child(self.selected_annotation_controls(selected, cx))
                        .into_any_element(),
                );
            }
        }
        layers
    }

    fn selected_annotation_controls(
        &self,
        selected: &Selection,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let serial = selected.serial;
        let theme = Theme::global(cx);
        column()
            .id("annotation-selection-controls")
            .test_support()
            .w_full()
            .p_2()
            .gap_2()
            .occlude()
            .bg(theme.popover)
            .border_1()
            .border_color(theme.border)
            .rounded_md()
            .shadow_sm()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(
                row()
                    .gap_1()
                    .child(div().flex_1().text_size(px(11.)).child("已选中标注"))
                    .when(selected.preview.kind == Kind::Note, |row| {
                        row.child(
                            Button::new("edit-annotation-text")
                                .ghost()
                                .small()
                                .icon(IconName::Type)
                                .accessibility_label("编辑标注文本")
                                .tooltip("编辑文本")
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.commit_selected_annotation(cx);
                                    if let Some(selected) = this
                                        .annotation_editing
                                        .selection
                                        .as_ref()
                                        .filter(|s| s.serial == serial)
                                    {
                                        this.open_annotation_text_editor(
                                            selected.preview.clone(),
                                            Some(selected.target.clone()),
                                            window,
                                            cx,
                                        );
                                    }
                                })),
                        )
                    })
                    .child(
                        Button::new("delete-page-annotation")
                            .ghost()
                            .small()
                            .icon(IconName::Trash)
                            .accessibility_label("删除选中标注")
                            .tooltip_with_action(
                                "删除选中标注",
                                &DeleteSelectedAnnotation,
                                Some("Breeze"),
                            )
                            .on_click(cx.listener(move |this, _, window, cx| {
                                if this
                                    .annotation_editing
                                    .selection
                                    .as_ref()
                                    .is_some_and(|s| s.serial == serial)
                                {
                                    this.delete_selected_annotation(window, cx);
                                }
                            })),
                    ),
            )
            .child(
                row().gap_1().children(
                    breeze_core::library::CategoryColor::ALL
                        .into_iter()
                        .enumerate()
                        .map(|(index, color)| {
                            Button::new(("selected-annotation-color", index))
                                .ghost()
                                .small()
                                .size(px(26.))
                                .p_0()
                                .accessibility_label(colors::label(color))
                                .child(
                                    div()
                                        .size(px(17.))
                                        .flex_shrink_0()
                                        .rounded_full()
                                        .bg(rgb(colors::hex(color))),
                                )
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    if let Some(selected) = this
                                        .annotation_editing
                                        .selection
                                        .as_mut()
                                        .filter(|s| s.serial == serial)
                                    {
                                        selected.preview.color = colors::components(color);
                                        this.commit_selected_annotation(cx);
                                        cx.notify();
                                    }
                                }))
                        }),
                ),
            )
            .when(
                !matches!(
                    selected.preview.kind,
                    Kind::Note | Kind::Area | Kind::Highlight
                ),
                |content| {
                    content.child(
                        row()
                            .gap_2()
                            .child(div().text_size(px(11.)).child("线宽"))
                            .child(
                                div()
                                    .id("selected-annotation-width")
                                    .test_support()
                                    .flex_1()
                                    .child(NumberInput::new(&selected.width).small().suffix("pt")),
                            ),
                    )
                },
            )
            .child(
                row()
                    .justify_between()
                    .text_size(px(11.))
                    .child("不透明度")
                    .child(format!("{:.0}%", selected.preview.opacity * 100.)),
            )
            .child(
                div()
                    .id("selected-annotation-opacity")
                    .test_support()
                    .px_2()
                    .child(Slider::new(&selected.opacity)),
            )
            .into_any_element()
    }
}

fn can_add(book: &Book, annotation: &Annotation) -> bool {
    book.annotations.history.items.len() < 5000
        && book
            .annotations
            .history
            .items
            .iter()
            .map(|a| a.points.len() + a.quads.len())
            .sum::<usize>()
            + annotation.points.len()
            + annotation.quads.len()
            <= 100_000
}
