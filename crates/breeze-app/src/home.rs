use crate::*;
use gpui_kit::component::Icon;
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
                n if n >= 3 => b.category == n - 3,
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
                    .overflow_y_scroll()
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
                            .child(self.command(
                                "home-open",
                                IconName::FolderOpen,
                                "打开文档",
                                &Open,
                                cx,
                            )),
                    )
                    .child(
                        row()
                            .h(px(24.))
                            .px_2()
                            .border_b_1()
                            .border_color(p.line)
                            .text_size(px(11.))
                            .text_color(p.muted)
                            .child(div().flex_1().child("文档"))
                            .child(div().w(px(118.)).child("分类"))
                            .child(div().w(px(100.)).child("位置"))
                            .child(div().w(px(54.))),
                    )
                    .child(column().gap_1().children(filtered.iter().map(|(index, b)| {
                        let index = *index;
                        row()
                            .h(px(48.))
                            .px_2()
                            .gap_2()
                            .rounded_md()
                            .hover(move |s| s.bg(p.panel))
                            .child(Icon::new(IconName::FileText).small().text_color(p.muted))
                            .child(
                                column()
                                    .flex_1()
                                    .min_w_0()
                                    .items_start()
                                    .child(
                                        Button::new(("library-open", index))
                                            .ghost()
                                            .small()
                                            .justify_start()
                                            .label(b.title.clone())
                                            .on_click(cx.listener(move |this, _, _, cx| {
                                                this.open_book(index, cx)
                                            })),
                                    )
                                    .child(div().text_size(px(11.)).text_color(p.muted).child(
                                        if b.source == Source::Sample {
                                            "PDF · 内置示例"
                                        } else {
                                            "PDF · 本地文件"
                                        },
                                    )),
                            )
                            .child(self.category_assignment(index, cx))
                            .child(
                                div()
                                    .w(px(100.))
                                    .text_size(px(12.))
                                    .text_color(p.muted)
                                    .child(if b.opened {
                                        format!("第 {} 页", b.state.navigation.page() + 1)
                                    } else {
                                        "尚未打开".into()
                                    }),
                            )
                            .child(
                                Button::new(("favorite", index))
                                    .ghost()
                                    .small()
                                    .icon(IconName::Bookmark)
                                    .selected(b.favorite)
                                    .accessibility_label("收藏")
                                    .tooltip("收藏 / 取消收藏")
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.books[index].favorite = !this.books[index].favorite;
                                        cx.notify()
                                    })),
                            )
                            .child(
                                Button::new(("open-item", index))
                                    .ghost()
                                    .small()
                                    .icon(IconName::ArrowUpRight)
                                    .accessibility_label("阅读文档")
                                    .tooltip("阅读文档")
                                    .on_click(
                                        cx.listener(move |this, _, _, cx| {
                                            this.open_book(index, cx)
                                        }),
                                    ),
                            )
                            .when(b.source != Source::Sample, |d| {
                                d.child(
                                    Button::new(("forget", index))
                                        .ghost()
                                        .small()
                                        .icon(IconName::X)
                                        .accessibility_label("从资料库移除")
                                        .tooltip("移除条目，不删除文件")
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            this.forget_book(index, cx)
                                        })),
                                )
                            })
                    })))
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
}
