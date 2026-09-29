use crate::*;
use gpui_kit::component::Icon;
impl Breeze {
    pub(super) fn home(&self, cx: &mut Context<Self>) -> Div {
        let p = self.p();
        let filtered: Vec<_> = self
            .books
            .iter()
            .enumerate()
            .filter(|(_, b)| match self.library_filter {
                1 => b.favorite,
                3 => b.category == 1,
                4 => b.category == 2,
                _ => true,
            })
            .collect();
        row()
            .flex_1()
            .min_h_0()
            .items_stretch()
            .child(
                column()
                    .w(px(168.))
                    .flex_shrink_0()
                    .py_2()
                    .px_2()
                    .gap_1()
                    .bg(p.panel)
                    .border_r_1()
                    .border_color(p.line)
                    .children(
                        [
                            (IconName::Clock3, "最近阅读", 0),
                            (IconName::Bookmark, "收藏", 1),
                            (IconName::Files, "全部文档", 2),
                        ]
                        .into_iter()
                        .map(|(icon, label, index)| {
                            Button::new(("library-nav", index))
                                .ghost()
                                .xsmall()
                                .justify_start()
                                .icon(icon)
                                .label(label)
                                .selected(self.library_filter == index)
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.library_filter = index;
                                    cx.notify()
                                }))
                        }),
                    )
                    .child(style::eyebrow("分类", p).mt_3().mb_1().px_2())
                    .children([("灵感与设计", 3), ("工作与学习", 4)].into_iter().map(
                        |(label, index)| {
                            Button::new(("category-nav", index))
                                .ghost()
                                .xsmall()
                                .justify_start()
                                .icon(IconName::Folder)
                                .label(label)
                                .selected(self.library_filter == index)
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.library_filter = index;
                                    cx.notify()
                                }))
                        },
                    )),
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
                                        1 => "收藏",
                                        3 => "灵感与设计",
                                        4 => "工作与学习",
                                        _ => "最近阅读",
                                    }),
                            )
                            .child(
                                div()
                                    .text_size(px(11.))
                                    .text_color(p.muted)
                                    .child(format!("{} 份", filtered.len())),
                            )
                            .child(div().flex_1())
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
                            .text_size(px(10.))
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
                                            .xsmall()
                                            .justify_start()
                                            .label(b.title.clone())
                                            .on_click(cx.listener(move |this, _, _, cx| {
                                                this.open_book(index, cx)
                                            })),
                                    )
                                    .child(div().text_size(px(10.)).text_color(p.muted).child(
                                        if b.source == Source::Sample {
                                            "PDF · 内置示例"
                                        } else {
                                            "PDF · 本地文件"
                                        },
                                    )),
                            )
                            .child(
                                Button::new(("category", index))
                                    .ghost()
                                    .xsmall()
                                    .w(px(118.))
                                    .justify_start()
                                    .label(["未分类", "灵感与设计", "工作与学习"][b.category])
                                    .tooltip("切换分类 · 会话内预览")
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.books[index].category =
                                            (this.books[index].category + 1) % 3;
                                        cx.notify()
                                    })),
                            )
                            .child(
                                div()
                                    .w(px(100.))
                                    .text_size(px(11.))
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
                                    .xsmall()
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
                                    .xsmall()
                                    .icon(IconName::ArrowUpRight)
                                    .accessibility_label("阅读文档")
                                    .tooltip("阅读文档")
                                    .on_click(
                                        cx.listener(move |this, _, _, cx| {
                                            this.open_book(index, cx)
                                        }),
                                    ),
                            )
                    })))
                    .when(filtered.is_empty(), |d| {
                        d.child(div().py_6().text_color(p.muted).child("暂无文档"))
                    })
                    .when(!self.message.is_empty(), |d| {
                        d.child(div().text_color(rgb(0xba6544)).child(self.message.clone()))
                    })
                    .child(
                        div()
                            .text_size(px(10.))
                            .text_color(p.muted)
                            .child("原型 · 收藏、分类与阅读位置仅保留本次会话。"),
                    ),
            )
    }
}
