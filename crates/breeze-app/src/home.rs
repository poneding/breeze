use crate::*;
use gpui_kit::component::{ElementExt, Icon, button::ButtonCustomVariant};
use std::ops::Range;
const CATEGORY_WIDTH: f32 = 144.;
const POSITION_WIDTH: f32 = 100.;
const ACTIONS_WIDTH: f32 = 80.;

impl Breeze {
    pub(super) fn home(&self, cx: &mut Context<Self>) -> Div {
        let p = self.p();
        let mut filtered: Vec<_> = self
            .books
            .iter()
            .enumerate()
            .filter(|(_, b)| {
                b.title
                    .to_lowercase()
                    .contains(&self.library_input.read(cx).value().to_lowercase())
            })
            .filter(|(_, b)| match self.library_filter {
                1 => b.favorite,
                3 => b.categories.is_empty(),
                n if n > 3 => b.categories.contains(&(n - 3)),
                _ => true,
            })
            .collect();
        filtered.sort_by_key(|(_, b)| std::cmp::Reverse(b.last_opened));
        row()
            .flex_1()
            .min_h_0()
            .items_stretch()
            .child(
                column()
                    .w(px(168.))
                    .flex_shrink_0()
                    .id("library-sidebar")
                    .overflow_y_scroll()
                    .py_2()
                    .px_2()
                    .gap_1()
                    .bg(p.panel)
                    .border_r_1()
                    .border_color(p.line)
                    .children(
                        [
                            (IconName::LibraryBig, "全部文档", 2),
                            (IconName::Clock3, "最近阅读", 0),
                            (IconName::Bookmark, "收藏", 1),
                        ]
                        .into_iter()
                        .map(|(icon, label, index)| {
                            categories::left_button(("library-nav", index), icon, label)
                                .selected(self.library_filter == index)
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.library_filter = index;
                                    cx.notify()
                                }))
                        }),
                    )
                    .child(self.category_sidebar(cx)),
            )
            .child(
                column()
                    .id("home-content")
                    .flex_1()
                    .min_w_0()
                    .min_h_0()
                    .px_5()
                    .py_3()
                    .gap_3()
                    .child(
                        row()
                            .gap_2()
                            .child(
                                div()
                                    .text_size(px(16.))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(match self.library_filter {
                                        1 => "收藏".to_owned(),
                                        2 => "全部文档".to_owned(),
                                        n if n >= 3 => self.category_name(n - 3),
                                        _ => "最近阅读".to_owned(),
                                    }),
                            )
                            .child(
                                div()
                                    .text_size(px(12.))
                                    .text_color(p.muted)
                                    .child(format!("{} 份", filtered.len())),
                            )
                            .child(div().flex_1())
                            .child(
                                div()
                                    .w(px(240.))
                                    .child(Input::new(&self.library_input).small()),
                            )
                            .children(
                                [
                                    (false, IconName::List, "列表视图"),
                                    (true, IconName::LayoutGrid, "网格视图"),
                                ]
                                .into_iter()
                                .map(|(grid, icon, label)| {
                                    Button::new(if grid { "library-grid" } else { "library-list" })
                                        .ghost()
                                        .small()
                                        .icon(icon)
                                        .selected(self.library_grid == grid)
                                        .accessibility_label(label)
                                        .tooltip(label)
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            this.library_grid = grid;
                                            this.covers.cancel();
                                            cx.notify();
                                        }))
                                }),
                            )
                            .when(self.library_filter == 2, |d| {
                                d.child(self.command(
                                    "home-open",
                                    IconName::FilePlusCorner,
                                    "打开文档",
                                    &Open,
                                    cx,
                                ))
                            }),
                    )
                    .when(!self.library_grid, |d| {
                        d.child(
                            row()
                                .h(px(24.))
                                .px_2()
                                .border_b_1()
                                .border_color(p.line)
                                .text_size(px(11.))
                                .text_color(p.muted)
                                .gap_2()
                                .child(div().flex_1().min_w_0().child("文档"))
                                .child(
                                    div()
                                        .id("category-heading")
                                        .test_support()
                                        .w(px(CATEGORY_WIDTH))
                                        .flex_shrink_0()
                                        .px_2()
                                        .child("分类"),
                                )
                                .child(
                                    div()
                                        .id("position-heading")
                                        .test_support()
                                        .w(px(POSITION_WIDTH))
                                        .flex_shrink_0()
                                        .child("位置"),
                                )
                                .child(div().w(px(ACTIONS_WIDTH)).flex_shrink_0()),
                        )
                    })
                    .child(
                        self.library_documents(
                            filtered.iter().map(|(index, _)| *index).collect(),
                            cx,
                        ),
                    )
                    .when(filtered.is_empty(), |d| {
                        d.child(div().py_6().text_color(p.muted).child("暂无文档"))
                    })
                    .when(!self.message.is_empty(), |d| {
                        d.child(div().text_color(rgb(0xba6544)).child(self.message.clone()))
                    })
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(p.muted)
                            .child("收藏、分类、外观与阅读状态自动保存在本机。"),
                    ),
            )
    }

    fn library_row(&self, index: usize, cx: &mut Context<Self>) -> Button {
        let p = self.p();
        let b = &self.books[index];
        Button::new(("library-open", index))
            .custom(ButtonCustomVariant::new(cx).hover(p.tint).active(p.tint))
            .group("library-row")
            .group_hover("library-row", move |s| s.bg(p.tint))
            .small()
            .w_full()
            .accessibility_label(format!("阅读文档：{}", b.title))
            .tooltip(self.cover_description(index))
            .on_click(cx.listener(move |this, _, _, cx| this.open_book(index, cx)))
            .h(px(48.))
            .px_2()
            .gap_2()
            .rounded_md()
            .child(
                row()
                    .w_full()
                    .gap_2()
                    .child(
                        row()
                            .flex_1()
                            .min_w_0()
                            .gap_2()
                            .child(self.library_cover(index, false, cx))
                            .child(
                                column()
                                    .flex_1()
                                    .min_w_0()
                                    .items_start()
                                    .child(
                                        div()
                                            .w_full()
                                            .text_left()
                                            .text_ellipsis()
                                            .child(b.title.clone()),
                                    )
                                    .child(
                                        div().text_size(px(11.)).text_color(p.muted).child("PDF"),
                                    ),
                            ),
                    )
                    .child(self.category_assignment(index, cx))
                    .child(
                        div()
                            .id(("position", index))
                            .test_support()
                            .w(px(POSITION_WIDTH))
                            .flex_shrink_0()
                            .text_left()
                            .text_size(px(12.))
                            .text_color(p.muted)
                            .child(if b.opened {
                                format!("第 {} 页", b.state.navigation.page() + 1)
                            } else {
                                "尚未打开".into()
                            }),
                    )
                    .child(self.library_actions(index, cx)),
            )
    }
    fn library_actions(&self, index: usize, cx: &mut Context<Self>) -> Div {
        let p = self.p();
        let b = &self.books[index];
        row()
            .w(px(ACTIONS_WIDTH))
            .flex_shrink_0()
            .justify_end()
            .gap_1()
            .child(
                Button::new(("favorite", index))
                    .custom(ButtonCustomVariant::new(cx).hover(p.line).active(p.line))
                    .small()
                    .icon(IconName::Bookmark)
                    .selected(b.favorite)
                    .accessibility_label("收藏")
                    .tooltip("收藏 / 取消收藏")
                    .on_click(cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        this.books[index].favorite = !this.books[index].favorite;
                        cx.notify()
                    })),
            )
            .child(
                Button::new(("open-item", index))
                    .custom(ButtonCustomVariant::new(cx).hover(p.line).active(p.line))
                    .small()
                    .icon(IconName::ArrowUpRight)
                    .accessibility_label("阅读文档")
                    .tooltip("阅读文档")
                    .on_click(cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        this.open_book(index, cx)
                    })),
            )
            .child(
                Button::new(("forget", index))
                    .custom(ButtonCustomVariant::new(cx).hover(p.line).active(p.line))
                    .small()
                    .icon(IconName::X)
                    .accessibility_label("从资料库移除")
                    .tooltip("移除条目，不删除文件")
                    .on_click(cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        this.forget_book(index, cx)
                    })),
            )
    }
    fn cover_description(&self, index: usize) -> String {
        match self.covers.image(&self.books[index].source) {
            Some(Err(error)) => error.clone(),
            _ => self.books[index].title.clone(),
        }
    }
    fn library_cover(&self, index: usize, grid: bool, _cx: &App) -> Div {
        let p = self.p();
        div()
            .w(px(if grid { 120. } else { 28. }))
            .h(px(if grid { 156. } else { 38. }))
            .flex_shrink_0()
            .flex()
            .items_center()
            .justify_center()
            .bg(p.canvas)
            .rounded_sm()
            .overflow_hidden()
            .map(|d| match self.covers.image(&self.books[index].source) {
                Some(Ok(image)) => d.child(
                    img(image.clone())
                        .size_full()
                        .object_fit(ObjectFit::Contain),
                ),
                Some(Err(_)) => d.child(
                    Icon::new(IconName::TriangleAlert)
                        .small()
                        .text_color(p.muted),
                ),
                None => d.child(Icon::new(IconName::FileText).small().text_color(p.muted)),
            })
    }
    fn library_card(&self, index: usize, cx: &mut Context<Self>) -> Button {
        let p = self.p();
        let book = &self.books[index];
        Button::new(("library-open", index))
            .custom(ButtonCustomVariant::new(cx).hover(p.tint).active(p.tint))
            .group("library-row")
            .group_hover("library-row", move |s| s.bg(p.tint))
            .small()
            .w_full()
            .h(px(264.))
            .p_2()
            .accessibility_label(format!("阅读文档：{}", book.title))
            .tooltip(self.cover_description(index))
            .on_click(cx.listener(move |this, _, _, cx| this.open_book(index, cx)))
            .child(
                column()
                    .w_full()
                    .min_w_0()
                    .gap_1()
                    .child(
                        div()
                            .w_full()
                            .flex()
                            .justify_center()
                            .child(self.library_cover(index, true, cx)),
                    )
                    .child(
                        div()
                            .w_full()
                            .text_left()
                            .text_ellipsis()
                            .child(book.title.clone()),
                    )
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(p.muted)
                            .text_left()
                            .child(format!(
                                "PDF · {}",
                                if book.last_opened > 0 {
                                    format!("第 {} 页", book.state.navigation.page() + 1)
                                } else {
                                    "尚未打开".into()
                                }
                            )),
                    )
                    .child(self.category_assignment(index, cx))
                    .child(
                        row()
                            .w_full()
                            .justify_end()
                            .child(self.library_actions(index, cx)),
                    ),
            )
    }
    fn library_documents(&self, indices: Vec<usize>, cx: &mut Context<Self>) -> Div {
        let grid = self.library_grid;
        let columns = if grid {
            grid_columns(self.library_width)
        } else {
            1
        };
        let count = indices.len().div_ceil(columns);
        let list = uniform_list(
            "library-documents",
            count,
            cx.processor(move |this, range: Range<usize>, _, cx| {
                let first = range.start;
                let sources: Vec<_> = range
                    .clone()
                    .flat_map(|row| (row * columns)..((row + 1) * columns).min(indices.len()))
                    .map(|i| this.books[indices[i]].source.clone())
                    .collect();
                range
                    .map(|row_index| {
                        let mut content = row().w_full().items_start().gap_3().pb(px(if grid {
                            12.
                        } else {
                            4.
                        }));
                        for col in 0..columns {
                            let i = row_index * columns + col;
                            content = content.child(div().flex_1().min_w_0().when(
                                i < indices.len(),
                                |d| {
                                    d.child(if grid {
                                        this.library_card(indices[i], cx)
                                    } else {
                                        this.library_row(indices[i], cx)
                                    })
                                },
                            ));
                        }
                        if row_index == first {
                            let view = cx.entity();
                            let sources = sources.clone();
                            content = content.on_prepaint(move |_, _, cx| {
                                view.update(cx, |this, cx| this.defer_covers(sources, cx));
                            });
                        }
                        content
                    })
                    .collect()
            }),
        )
        .size_full();
        let view = cx.entity();
        div()
            .flex_1()
            .min_h_0()
            .w_full()
            .child(list)
            .on_prepaint(move |bounds, _, cx| {
                let width = f32::from(bounds.size.width);
                view.update(cx, |this, cx| {
                    if grid_columns(width) != grid_columns(this.library_width) {
                        this.library_width = width;
                        cx.notify();
                    }
                });
            })
    }
}
fn grid_columns(width: f32) -> usize {
    ((width + 12.) / 188.).floor().clamp(1., 8.) as usize
}
