//! Explicit category management and assignment; deleting a category never removes a file.
use crate::*;
use breeze_core::library::{Category, validate_name};
use gpui_kit::component::{
    Icon,
    menu::{DropdownMenu, PopupMenuItem},
};

pub(super) fn left_button(
    id: impl Into<ElementId>,
    icon: IconName,
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
                .child(Icon::new(icon).small())
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
                    }
                } else if self.categories.len() < 100 && self.next_category_id < usize::MAX - 3 {
                    let id = self.next_category_id;
                    self.next_category_id += 1;
                    self.categories.push(Category { id, name });
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
            if book.category == id {
                book.category = 0;
            }
        }
        if self.library_filter == id + 3 {
            self.library_filter = 3;
        }
        self.category_form = false;
        self.category_edit = None;
        cx.notify();
    }
    pub(super) fn category_sidebar(&self, cx: &mut Context<Self>) -> Div {
        let p = self.p();
        column()
            .gap_1()
            .child(
                row()
                    .mt_2()
                    .child(style::eyebrow("分类", p).px_2())
                    .child(div().flex_1())
                    .child(
                        Button::new("new-category")
                            .ghost()
                            .small()
                            .icon(IconName::Plus)
                            .accessibility_label("新建分类")
                            .tooltip("新建分类")
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.category_edit = None;
                                this.category_form = true;
                                this.category_input.update(cx, |input, cx| {
                                    input.set_value("", window, cx);
                                    input.focus(window, cx);
                                });
                                cx.notify();
                            })),
                    ),
            )
            .child(
                left_button("unassigned-category", IconName::Folder, "未分类")
                    .selected(self.library_filter == 3)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.library_filter = 3;
                        cx.notify();
                    })),
            )
            .children(self.categories.iter().map(|category| {
                let id = category.id;
                left_button(
                    ("category-nav", id),
                    IconName::Folder,
                    category.name.clone(),
                )
                .selected(self.library_filter == id + 3)
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.library_filter = id + 3;
                    cx.notify();
                }))
            }))
            .when(self.library_filter > 3, |d| {
                let id = self.library_filter - 3;
                d.child(
                    row()
                        .gap_1()
                        .child(
                            Button::new("rename-category")
                                .ghost()
                                .small()
                                .label("重命名")
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.category_form = true;
                                    this.category_edit = Some(id);
                                    let name = this.category_name(id);
                                    this.category_input.update(cx, |input, cx| {
                                        input.set_value(name, window, cx);
                                        input.focus(window, cx);
                                        input.select_all(window, cx);
                                    });
                                    cx.notify();
                                })),
                        )
                        .child(
                            Button::new("delete-category")
                                .ghost()
                                .small()
                                .icon(IconName::Trash)
                                .accessibility_label("删除分类")
                                .tooltip("删除分类；文档移至未分类，不删除文件")
                                .on_click(
                                    cx.listener(move |this, _, _, cx| this.delete_category(id, cx)),
                                ),
                        ),
                )
            })
            .when(self.category_form, |d| {
                d.child(Input::new(&self.category_input).small()).child(
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
            })
    }
    pub(super) fn category_assignment(
        &self,
        index: usize,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let mut choices = vec![(0, "未分类".to_owned())];
        choices.extend(self.categories.iter().map(|c| (c.id, c.name.clone())));
        let view = cx.entity();
        Button::new(("category", index))
            .ghost()
            .small()
            .w(px(144.))
            .flex_shrink_0()
            .accessibility_label(format!(
                "分类：{}",
                self.category_name(self.books[index].category)
            ))
            .child(
                div()
                    .w_full()
                    .text_left()
                    .text_ellipsis()
                    .child(self.category_name(self.books[index].category)),
            )
            .on_click(|_, _, cx| cx.stop_propagation())
            .dropdown_menu(move |mut menu, window, _| {
                for (id, name) in &choices {
                    let id = *id;
                    menu = menu.item(PopupMenuItem::new(name.clone()).on_click(
                        window.listener_for(&view, move |this, _, _, cx| {
                            if let Some(book) = this.books.get_mut(index) {
                                book.category = id;
                            }
                            cx.notify();
                        }),
                    ));
                }
                menu
            })
    }
}
