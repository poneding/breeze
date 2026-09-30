//! Explicit category management and assignment; deleting a category never removes a file.
use crate::*;
use breeze_core::library::{Category, CategoryColor, validate_name};
use gpui_kit::component::{
    Icon, Theme,
    menu::{DropdownMenu, PopupMenuItem},
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
        let p = self.p();
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
                        Button::new("new-category")
                            .ghost()
                            .small()
                            .icon(IconName::Plus)
                            .accessibility_label("新建分类")
                            .tooltip("新建分类")
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.category_edit = None;
                                this.category_color = CategoryColor::default();
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
                        left_button_icon(
                            ("category-nav", id),
                            Icon::new(IconName::Tag)
                                .small()
                                .text_color(category_tint(category.color, cx)),
                            category.name.clone(),
                        )
                        .flex_1()
                        .min_w_0()
                        .selected(self.library_filter == id + 3)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.library_filter = id + 3;
                            cx.notify();
                        })),
                    )
                    .child(
                        row()
                            .w(px(44.))
                            .flex_shrink_0()
                            .gap_1()
                            .invisible()
                            .group_hover("category-row", |s| s.visible())
                            .child(
                                Button::new(("rename-category", id))
                                    .ghost()
                                    .small()
                                    .size(px(20.))
                                    .icon(IconName::Pencil)
                                    .accessibility_label(format!("重命名分类：{}", category.name))
                                    .tooltip("重命名分类")
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.category_form = true;
                                        this.category_edit = Some(id);
                                        if let Some(category) = this
                                            .categories
                                            .iter()
                                            .find(|category| category.id == id)
                                        {
                                            this.category_color = category.color;
                                        }
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
                                Button::new(("delete-category", id))
                                    .ghost()
                                    .small()
                                    .size(px(20.))
                                    .icon(IconName::Trash)
                                    .accessibility_label(format!("删除分类：{}", category.name))
                                    .tooltip("删除分类；保留文档及其他分类")
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.delete_category(id, cx)
                                    })),
                            ),
                    )
            }))
            .when(self.category_form, |d| {
                d.child(Input::new(&self.category_input).small())
                    .children(CategoryColor::ALL.chunks(4).map(|colors| {
                        row().gap_1().children(colors.iter().copied().map(|color| {
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
                                .accessibility_label(format!("分类颜色：{}", color_label(color)))
                                .tooltip(color_label(color))
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.category_color = color;
                                    cx.notify();
                                }))
                        }))
                    }))
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
            })
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
        Button::new(("category", index))
            .ghost()
            .small()
            .w(px(144.))
            .flex_shrink_0()
            .accessibility_label(format!("分类（可多选）：{label}"))
            .tooltip(format!("分类（可多选）：{label}"))
            .child(div().w_full().text_left().text_ellipsis().child(label))
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

fn color_label(color: CategoryColor) -> &'static str {
    match color {
        CategoryColor::Red => "红色",
        CategoryColor::Orange => "橙色",
        CategoryColor::Amber => "琥珀色",
        CategoryColor::Green => "绿色",
        CategoryColor::Teal => "青色",
        CategoryColor::Blue => "蓝色",
        CategoryColor::Violet => "紫色",
        CategoryColor::Pink => "粉色",
    }
}
fn category_tint(color: CategoryColor, cx: &App) -> Hsla {
    let (light, dark) = match color {
        CategoryColor::Red => (0xdc2626, 0xf87171),
        CategoryColor::Orange => (0xc2410c, 0xfb923c),
        CategoryColor::Amber => (0xa16207, 0xfbbf24),
        CategoryColor::Green => (0x15803d, 0x4ade80),
        CategoryColor::Teal => (0x0f766e, 0x2dd4bf),
        CategoryColor::Blue => (0x2563eb, 0x60a5fa),
        CategoryColor::Violet => (0x7c3aed, 0xa78bfa),
        CategoryColor::Pink => (0xbe185d, 0xf472b6),
    };
    rgb(if Theme::global(cx).is_dark() {
        dark
    } else {
        light
    })
    .into()
}
