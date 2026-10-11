//! Explicit category management and assignment; deleting a category never removes a file.
use crate::*;
use breeze_core::library::{Category, CategoryColor, validate_name};
use gpui_kit::component::{
    Icon,
    button::ButtonCustomVariant,
    menu::{DropdownMenu, PopupMenuItem},
    popover::Popover,
};

pub(super) fn left_button(
    id: impl Into<ElementId>,
    icon: IconName,
    label: impl Into<SharedString>,
) -> Button {
    left_button_icon(id, Icon::new(icon).small(), label)
}
fn left_button_icon(
    id: impl Into<ElementId>,
    icon: Icon,
    label: impl Into<SharedString>,
) -> Button {
    let label = label.into();
    Button::new(id)
        .ghost()
        .small()
        .w_full()
        .accessibility_label(label.clone())
        .tooltip(label.clone())
        .child(
            row()
                .id("left-label-row")
                .test_support()
                .w_full()
                .gap_2()
                .child(icon)
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_ellipsis()
                        .text_left()
                        .child(label),
                ),
        )
}
impl Breeze {
    pub(super) fn category_name(&self, id: usize) -> String {
        self.categories
            .iter()
            .find(|c| c.id == id)
            .map(|c| c.name.clone())
            .unwrap_or_else(|| "未分类".into())
    }
    pub(super) fn save_category(&mut self, cx: &mut Context<Self>) {
        let result = validate_name(
            &self.category_input.read(cx).value(),
            &self.categories,
            self.category_edit,
        );
        match result {
            Ok(name) => {
                if let Some(id) = self.category_edit {
                    if let Some(category) = self.categories.iter_mut().find(|c| c.id == id) {
                        category.name = name;
                        category.color = self.category_color;
                    }
                } else if self.categories.len() < 100 && self.next_category_id < usize::MAX - 3 {
                    let id = self.next_category_id;
                    self.next_category_id += 1;
                    self.categories.push(Category {
                        id,
                        name,
                        color: self.category_color,
                    });
                    self.library_filter = id + 3;
                } else {
                    self.message = "最多支持 100 个分类".into();
                    cx.notify();
                    return;
                }
                self.category_form = false;
                self.category_edit = None;
                self.message.clear();
            }
            Err(e) => self.message = e.into(),
        }
        cx.notify();
    }
    pub(super) fn delete_category(&mut self, id: usize, cx: &mut Context<Self>) {
        self.categories.retain(|c| c.id != id);
        for book in &mut self.books {
            book.categories.retain(|category| *category != id);
        }
        if self.library_filter == id + 3 {
            self.library_filter = 3;
        }
        self.category_form = false;
        self.category_edit = None;
        cx.notify();
    }
    pub(super) fn category_sidebar(&self, cx: &mut Context<Self>) -> Div {
        let p = self.p(cx);
        column()
            .gap_1()
            .child(
                row()
                    .mt_2()
                    .child(
                        row()
                            .px_2()
                            .gap_2()
                            .child(Icon::new(IconName::Tags).small().text_color(p.muted))
                            .child(style::eyebrow("分类", p)),
                    )
                    .child(div().flex_1())
                    .child(
                        self.category_popover(
                            Button::new("new-category")
                                .ghost()
                                .small()
                                .icon(IconName::Plus)
                                .accessibility_label("新建分类")
                                .tooltip("新建分类"),
                            None,
                            cx,
                        ),
                    ),
            )
            .child(
                left_button("unassigned-category", IconName::Tag, "未分类")
                    .selected(self.library_filter == 3)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.library_filter = 3;
                        cx.notify();
                    })),
            )
            .children(self.categories.iter().map(|category| {
                let id = category.id;
                row()
                    .id(("category-row", id))
                    .test_support()
                    .group("category-row")
                    .w_full()
                    .child(
                        Button::new(("category-nav", id))
                            .custom(ButtonCustomVariant::new(cx).hover(p.tint).active(p.tint))
                            .small()
                            .w_full()
                            .px_2()
                            .selected(self.library_filter == id + 3)
                            .when(self.library_filter == id + 3, |d| d.bg(p.tint))
                            .group_hover("category-row", move |s| s.bg(p.tint))
                            .accessibility_label(category.name.clone())
                            .tooltip(category.name.clone())
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.library_filter = id + 3;
                                cx.notify();
                            }))
                            .child(
                                row()
                                    .w_full()
                                    .gap_1()
                                    .child(
                                        row()
                                            .flex_1()
                                            .min_w_0()
                                            .gap_2()
                                            .child(
                                                Icon::new(IconName::Tag)
                                                    .small()
                                                    .text_color(category_tint(category.color, cx)),
                                            )
                                            .child(
                                                div()
                                                    .flex_1()
                                                    .min_w_0()
                                                    .text_left()
                                                    .text_ellipsis()
                                                    .child(category.name.clone()),
                                            ),
                                    )
                                    .child(
                                        row()
                                            .w(px(44.))
                                            .flex_shrink_0()
                                            .gap_1()
                                            .when(
                                                !(self.category_form
                                                    && self.category_edit == Some(id)),
                                                |d| {
                                                    d.invisible().group_hover("category-row", |s| {
                                                        s.visible()
                                                    })
                                                },
                                            )
                                            .child(
                                                self.category_popover(
                                                    Button::new(("rename-category", id))
                                                        .custom(
                                                            ButtonCustomVariant::new(cx)
                                                                .hover(p.line)
                                                                .active(p.line),
                                                        )
                                                        .small()
                                                        .size(px(20.))
                                                        .icon(IconName::Pencil)
                                                        .accessibility_label(format!(
                                                            "修改分类：{}",
                                                            category.name
                                                        ))
                                                        .tooltip("修改分类"),
                                                    Some(id),
                                                    cx,
                                                ),
                                            )
                                            .child(
                                                Button::new(("delete-category", id))
                                                    .custom(
                                                        ButtonCustomVariant::new(cx)
                                                            .hover(p.line)
                                                            .active(p.line),
                                                    )
                                                    .small()
                                                    .size(px(20.))
                                                    .icon(IconName::Trash)
                                                    .accessibility_label(format!(
                                                        "删除分类：{}",
                                                        category.name
                                                    ))
                                                    .tooltip("删除分类；保留文档及其他分类")
                                                    .on_click(cx.listener(
                                                        move |this, _, _, cx| {
                                                            cx.stop_propagation();
                                                            this.delete_category(id, cx)
                                                        },
                                                    )),
                                            ),
                                    ),
                            ),
                    )
            }))
    }
    fn category_popover(
        &self,
        trigger: Button,
        editing: Option<usize>,
        cx: &mut Context<Self>,
    ) -> Popover {
        let view = cx.entity();
        let editor = view.clone();
        Popover::new(("category-popover", editing.unwrap_or(0)))
            .open(self.category_form && self.category_edit == editing)
            .track_focus(&self.category_input.read(cx).focus_handle(cx))
            .trigger(trigger.on_click(|_, _, cx| cx.stop_propagation()))
            .on_open_change(move |open, window, cx| {
                view.update(cx, |this, cx| {
                    if *open {
                        this.category_edit = editing;
                        this.category_form = true;
                        this.category_color = editing
                            .and_then(|id| this.categories.iter().find(|c| c.id == id))
                            .map(|c| c.color)
                            .unwrap_or_default();
                        let name = editing.map(|id| this.category_name(id)).unwrap_or_default();
                        this.message.clear();
                        this.category_input.update(cx, |input, cx| {
                            input.set_value(name, window, cx);
                            input.select_all(window, cx);
                        });
                    } else if this.category_edit == editing {
                        this.category_form = false;
                    }
                    cx.notify();
                });
            })
            .content(move |_, _, cx| editor.update(cx, |this, cx| this.category_editor(cx)))
    }
    fn category_editor(&self, cx: &mut Context<Self>) -> AnyElement {
        column()
            .id("category-editor")
            .test_support()
            .w(px(232.))
            .gap_2()
            .child(Input::new(&self.category_input).small())
            .child(
                row()
                    .gap_1()
                    .children(CategoryColor::ALL.into_iter().map(|color| {
                        Button::new(("category-color", color as usize))
                            .ghost()
                            .small()
                            .size(px(24.))
                            .icon(
                                Icon::new(IconName::Tag)
                                    .small()
                                    .text_color(category_tint(color, cx)),
                            )
                            .selected(self.category_color == color)
                            .accessibility_label(format!("分类颜色：{}", colors::label(color)))
                            .tooltip(colors::label(color))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.category_color = color;
                                cx.notify();
                            }))
                    })),
            )
            .child(
                row()
                    .gap_1()
                    .child(
                        Button::new("save-category")
                            .small()
                            .label("保存")
                            .on_click(cx.listener(|this, _, _, cx| this.save_category(cx))),
                    )
                    .child(
                        Button::new("cancel-category")
                            .ghost()
                            .small()
                            .label("取消")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.category_form = false;
                                cx.notify();
                            })),
                    ),
            )
            .when(!self.message.is_empty(), |d| {
                d.child(div().text_size(px(12.)).child(self.message.clone()))
            })
            .into_any_element()
    }
    pub(super) fn toggle_category(&mut self, index: usize, id: usize, cx: &mut Context<Self>) {
        if id != 0 && !self.categories.iter().any(|category| category.id == id) {
            return;
        }
        if let Some(book) = self.books.get_mut(index) {
            if id == 0 {
                book.categories.clear();
            } else if book.categories.contains(&id) {
                book.categories.retain(|category| *category != id);
            } else {
                book.categories.push(id);
                book.categories.sort_unstable();
            }
        }
        cx.notify();
    }
    pub(super) fn category_assignment(
        &self,
        index: usize,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let selected = self.books[index].categories.clone();
        let names: Vec<_> = self
            .categories
            .iter()
            .filter(|c| selected.contains(&c.id))
            .map(|c| c.name.as_str())
            .collect();
        let label = if names.is_empty() {
            "未分类".to_owned()
        } else {
            names.join("、")
        };
        let mut choices = vec![(0, "清除分类".to_owned())];
        choices.extend(self.categories.iter().map(|c| (c.id, c.name.clone())));
        let view = cx.entity();
        let p = self.p(cx);
        Button::new(("category", index))
            .custom(ButtonCustomVariant::new(cx).hover(p.line).active(p.line))
            .small()
            .w(px(144.))
            .flex_shrink_0()
            .accessibility_label(format!("分类（可多选）：{label}"))
            .tooltip(format!("分类（可多选）：{label}"))
            .child(
                div()
                    .w_full()
                    .text_left()
                    .when(self.library_grid, |d| d.text_center())
                    .text_ellipsis()
                    .child(label),
            )
            .on_click(|_, _, cx| cx.stop_propagation())
            .dropdown_menu(move |mut menu, window, _| {
                for (id, name) in &choices {
                    let id = *id;
                    menu = menu.item(
                        PopupMenuItem::new(name.clone())
                            .checked(if id == 0 {
                                selected.is_empty()
                            } else {
                                selected.contains(&id)
                            })
                            .on_click(window.listener_for(&view, move |this, _, _, cx| {
                                this.toggle_category(index, id, cx);
                            })),
                    );
                }
                menu
            })
    }
}

fn category_tint(color: CategoryColor, _cx: &App) -> Hsla {
    rgb(colors::hex(color)).into()
}
