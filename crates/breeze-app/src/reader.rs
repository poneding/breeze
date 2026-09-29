use crate::*;
use gpui_kit::component::{
    Icon, h_resizable, resizable_panel,
    tab::{Tab, TabBar},
    tooltip::Tooltip,
};
impl Breeze {
    pub(super) fn reader(&self, window: &mut Window, cx: &mut Context<Self>) -> Div {
        let p = self.p();
        let mode = self.books[self.active].state.mode;
        let main = column()
            .size_full()
            .min_w_0()
            .min_h_0()
            .child(
                row()
                    .h(px(38.))
                    .flex_shrink_0()
                    .px_2()
                    .gap_2()
                    .border_b_1()
                    .border_color(p.line)
                    .child(self.command(
                        "collapse",
                        IconName::PanelLeft,
                        "导航面板",
                        &ToggleSidebar,
                        cx,
                    ))
                    .child(
                        TabBar::new("reader-modes")
                            .segmented()
                            .xsmall()
                            .selected_index(mode.index())
                            .on_click(cx.listener(|this, index, _, cx| {
                                this.books[this.active].state.mode = Mode::from_index(*index);
                                this.tool = 0;
                                cx.notify();
                            }))
                            .child(Tab::new().label("阅读").tooltip(|window, cx| {
                                Tooltip::new("阅读模式")
                                    .action(&ReadMode, Some("Breeze"))
                                    .build(window, cx)
                            }))
                            .child(Tab::new().label("标注").tooltip(|window, cx| {
                                Tooltip::new("标注模式 · 工具预览")
                                    .action(&AnnotateMode, Some("Breeze"))
                                    .build(window, cx)
                            }))
                            .child(Tab::new().label("编辑").tooltip(|window, cx| {
                                Tooltip::new("编辑模式 · 工具预览")
                                    .action(&EditMode, Some("Breeze"))
                                    .build(window, cx)
                            })),
                    )
                    .child(div().h(px(18.)).w(px(1.)).bg(p.line))
                    .child(self.tools(mode, cx))
                    .child(
                        Button::new("preview-info")
                            .ghost()
                            .xsmall()
                            .icon(IconName::Info)
                            .accessibility_label("原型说明")
                            .tooltip(format!(
                                "只读原型：标注/编辑仅预览，不修改PDF{}",
                                self.preview
                                    .as_ref()
                                    .map(|p| format!(
                                        " · 本页后台准备 {:.0} ms（含缩略图）",
                                        p.elapsed_ms
                                    ))
                                    .unwrap_or_default()
                            )),
                    )
                    .child(
                        self.command("reader-star", IconName::Bookmark, "收藏", &Favorite, cx)
                            .selected(self.books[self.active].favorite),
                    ),
            )
            .child(
                row()
                    .flex_1()
                    .min_h_0()
                    .items_stretch()
                    .child(self.page_view(window, cx))
                    .when(mode != Mode::Read, |d| d.child(self.inspector(mode, cx))),
            )
            .child(self.status(cx));
        if self.sidebar {
            div().flex_1().min_h_0().child(
                h_resizable("reader-split")
                    .with_state(&self.navigation_split)
                    .child(
                        resizable_panel()
                            .size(px(208.))
                            .size_range(px(168.)..px(360.))
                            .child(self.navigation(cx)),
                    )
                    .child(
                        resizable_panel()
                            .size_range(px(430.)..gpui_kit::Pixels::MAX)
                            .child(main),
                    ),
            )
        } else {
            div().flex_1().min_h_0().child(main)
        }
    }
    fn tools(&self, mode: Mode, cx: &mut Context<Self>) -> Stateful<Div> {
        let tools: Vec<(IconName, &str)> = match mode {
            Mode::Read => vec![
                (IconName::MousePointer2, "浏览"),
                (IconName::Search, "搜索 · 待实现"),
            ],
            Mode::Annotate => vec![
                (IconName::Highlighter, "文本高亮"),
                (IconName::Scan, "区域高亮"),
                (IconName::Underline, "下划线"),
                (IconName::Strikethrough, "删除线"),
                (IconName::Pencil, "画笔"),
                (IconName::Eraser, "橡皮擦"),
                (IconName::StickyNote, "文本批注"),
                (IconName::Circle, "圆形"),
                (IconName::Square, "矩形"),
                (IconName::Minus, "直线"),
                (IconName::LineSquiggle, "波浪线"),
                (IconName::ArrowUpRight, "箭头"),
            ],
            Mode::Edit => vec![
                (IconName::TextCursorInput, "文字"),
                (IconName::Image, "图片"),
                (IconName::Type, "插入文字"),
                (IconName::ImagePlus, "插入图片"),
            ],
        };
        row()
            .id("tools")
            .flex_1()
            .min_w_0()
            .overflow_x_scroll()
            .gap_1()
            .children(tools.into_iter().enumerate().map(|(i, (icon, label))| {
                Button::new(("tool", i))
                    .ghost()
                    .xsmall()
                    .icon(icon)
                    .accessibility_label(label)
                    .tooltip(label)
                    .selected(self.tool == i)
                    .disabled(mode == Mode::Read && i == 1)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.tool = i;
                        cx.notify()
                    }))
            }))
    }
    fn navigation(&self, cx: &mut Context<Self>) -> Div {
        let p = self.p();
        let current = self.books[self.active].state.navigation.page();
        column()
            .size_full()
            .min_w_0()
            .min_h_0()
            .flex_shrink_0()
            .bg(p.panel)
            .border_r_1()
            .border_color(p.line)
            .child(
                row()
                    .h(px(38.))
                    .justify_center()
                    .flex_shrink_0()
                    .px_2()
                    .border_b_1()
                    .border_color(p.line)
                    .child(
                        TabBar::new("nav-tabs")
                            .segmented()
                            .xsmall()
                            .selected_index(usize::from(self.thumbs))
                            .on_click(cx.listener(|this, index, _, cx| {
                                this.thumbs = *index == 1;
                                cx.notify()
                            }))
                            .child(Tab::new().label("目录").tooltip(|window, cx| {
                                Tooltip::new("目录")
                                    .action(&ShowOutline, Some("Breeze"))
                                    .build(window, cx)
                            }))
                            .child(Tab::new().label("缩略图").tooltip(|window, cx| {
                                Tooltip::new("缩略图")
                                    .action(&ShowThumbnails, Some("Breeze"))
                                    .build(window, cx)
                            })),
                    ),
            )
            .child(
                column()
                    .id("navigation-content")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .p_2()
                    .gap_2()
                    .when_some(self.preview.as_ref(), |this, preview| {
                        if self.thumbs {
                            this.children(preview.thumbnails.iter().map(|(page, image)| {
                                column()
                                    .id(("thumbnail", *page))
                                    .items_center()
                                    .gap_1()
                                    .p_2()
                                    .rounded_md()
                                    .border_1()
                                    .border_color(if *page == current { p.accent } else { p.line })
                                    .cursor_pointer()
                                    .child(
                                        img(image.clone())
                                            .w(px((self.navigation_width - 32.).min(176.)))
                                            .h(px((self.navigation_width - 32.).min(176.) * 1.294))
                                            .object_fit(ObjectFit::Contain),
                                    )
                                    .child(
                                        row()
                                            .justify_center()
                                            .text_size(px(11.))
                                            .text_color(p.muted)
                                            .child(format!("{:02}", page + 1)),
                                    )
                                    .on_click(cx.listener({
                                        let page = *page;
                                        move |this, _, _, cx| {
                                            this.books[this.active].state.navigation.go_to(page);
                                            this.load(cx)
                                        }
                                    }))
                            }))
                        } else {
                            this.when(preview.outline.len() > 300, |this| {
                                this.child(
                                    div()
                                        .text_size(px(10.))
                                        .text_color(p.muted)
                                        .child("原型仅显示前300条目录，完整虚拟树待实现"),
                                )
                            })
                            .when(preview.outline.is_empty(), |this| {
                                this.child(
                                    div()
                                        .text_size(px(12.))
                                        .text_color(p.muted)
                                        .child("此文档没有内嵌目录。可切换缩略图导航。"),
                                )
                            })
                            .children(
                                preview
                                    .outline
                                    .iter()
                                    .take(300)
                                    .enumerate()
                                    .map(|(i, item)| {
                                        let page = item.page;
                                        column().pl(px((item.depth.min(6) * 10) as f32)).child(
                                            Button::new(("outline", i))
                                                .ghost()
                                                .xsmall()
                                                .max_w(px((self.navigation_width
                                                    - 28.
                                                    - (item.depth.min(6) * 10) as f32)
                                                    .max(96.)))
                                                .overflow_hidden()
                                                .justify_start()
                                                .label(item.title.clone())
                                                .icon(IconName::ChevronRight)
                                                .disabled(page.is_none())
                                                .selected(page == Some(current))
                                                .tooltip(if page.is_none() {
                                                    "命名目的地尚未解析"
                                                } else {
                                                    "跳转到目录目标页"
                                                })
                                                .on_click(cx.listener(move |this, _, _, cx| {
                                                    if let Some(page) = page {
                                                        this.books[this.active]
                                                            .state
                                                            .navigation
                                                            .go_to(page);
                                                        this.load(cx)
                                                    }
                                                })),
                                        )
                                    }),
                            )
                        }
                    })
                    .when(self.loading, |this| {
                        this.child(div().text_color(p.muted).child("正在读取…"))
                    }),
            )
    }
    fn page_view(&self, window: &mut Window, _cx: &mut Context<Self>) -> Stateful<Div> {
        let p = self.p();
        let state = &self.books[self.active].state;
        let mut view = column()
            .id("document-viewport")
            .flex_1()
            .min_w_0()
            .min_h_0()
            .overflow_scroll()
            .bg(p.canvas)
            .p_3();
        if let Some(preview) = &self.preview {
            let width = f32::from(window.viewport_size().width)
                - (if self.sidebar {
                    self.navigation_width
                } else {
                    0.
                })
                - (if state.mode == Mode::Read { 0. } else { 232. })
                - 24.;
            let height = f32::from(window.viewport_size().height) - 34. - 38. - 32. - 24.;
            let (pw, ph) = preview.points;
            let scale = match state.zoom {
                Zoom::FitPage => (width / pw).min(height / ph),
                Zoom::FitWidth => width / pw,
                Zoom::Percent(n) => n as f32 / 100. * 96. / 72.,
            }
            .max(0.1);
            view = view.child(
                row().min_w_full().justify_center().child(
                    img(preview.image.clone())
                        .w(px(pw * scale))
                        .h(px(ph * scale))
                        .flex_shrink_0()
                        .object_fit(ObjectFit::Contain)
                        .border_1()
                        .border_color(p.line)
                        .shadow_md(),
                ),
            );
        } else {
            view = view.child(
                column()
                    .flex_1()
                    .items_center()
                    .justify_center()
                    .gap_3()
                    .text_color(p.muted)
                    .child(Icon::new(if self.loading {
                        IconName::BookOpen
                    } else {
                        IconName::TriangleAlert
                    }))
                    .child(if self.loading {
                        "正在准备页面…".into()
                    } else if self.message.is_empty() {
                        "选择一个文档开始阅读".into()
                    } else {
                        self.message.clone()
                    })
                    .child(div().text_size(px(11.)).child(if self.loading {
                        "后台解析与渲染，界面仍可操作"
                    } else {
                        "原文件没有被修改"
                    })),
            );
        }
        view
    }
    fn status(&self, cx: &mut Context<Self>) -> Stateful<Div> {
        let p = self.p();
        let s = &self.books[self.active].state;
        row()
            .id("status-scroll")
            .overflow_x_scroll()
            .h(px(32.))
            .flex_shrink_0()
            .px_2()
            .gap_1()
            .border_t_1()
            .border_color(p.line)
            .bg(p.bg)
            .child(
                self.command(
                    "previous",
                    IconName::ChevronLeft,
                    "上一页",
                    &PreviousPage,
                    cx,
                )
                .disabled(s.navigation.page() == 0),
            )
            .child(div().text_size(px(11.)).child(format!(
                "{} / {}",
                s.navigation.page() + 1,
                s.navigation.total()
            )))
            .child(
                self.command("next", IconName::ChevronRight, "下一页", &NextPage, cx)
                    .disabled(s.navigation.page() + 1 == s.navigation.total()),
            )
            .child(
                div()
                    .w(px(42.))
                    .child(Input::new(&self.page_input).xsmall()),
            )
            .child(
                Button::new("jump")
                    .ghost()
                    .xsmall()
                    .icon(IconName::CornerDownLeft)
                    .accessibility_label("跳转到输入页码")
                    .tooltip_with_action(
                        "跳转到输入页码；快捷键聚焦页码框",
                        &FocusPage,
                        Some("Breeze"),
                    )
                    .on_click(cx.listener(|this, _, _, cx| this.submit_page(cx))),
            )
            .child(
                div()
                    .flex_1()
                    .min_w(px(50.))
                    .overflow_hidden()
                    .text_size(px(10.))
                    .text_color(p.muted)
                    .child(if !self.message.is_empty() {
                        self.message.clone()
                    } else {
                        "只读预览".into()
                    }),
            )
            .child(
                Button::new("layout")
                    .ghost()
                    .xsmall()
                    .icon(IconName::PanelsTopLeft)
                    .tooltip("单页 · 连续/双页待实现")
                    .disabled(true),
            )
            .child(
                self.command("fit-page", IconName::Maximize, "适合页面", &FitPage, cx)
                    .selected(s.zoom == Zoom::FitPage),
            )
            .child(
                self.command(
                    "fit-width",
                    IconName::MoveHorizontal,
                    "适合页宽",
                    &FitWidth,
                    cx,
                )
                .selected(s.zoom == Zoom::FitWidth),
            )
            .child(
                self.command("actual-size", IconName::Scan, "实际大小", &ActualSize, cx)
                    .selected(s.zoom == Zoom::Percent(100)),
            )
            .child(self.command("zoom-out", IconName::Minus, "缩小", &ZoomOut, cx))
            .child(
                div()
                    .min_w(px(56.))
                    .text_center()
                    .text_size(px(10.))
                    .text_color(p.muted)
                    .child(s.zoom.label()),
            )
            .child(self.command("zoom-in", IconName::Plus, "放大", &ZoomIn, cx))
    }
    pub(super) fn submit_page(&mut self, cx: &mut Context<Self>) {
        let value = self.page_input.read(cx).value();
        match self.books[self.active].state.navigation.jump(&value) {
            Ok(()) => {
                self.message.clear();
                self.load(cx);
            }
            Err(error) => {
                self.message = error.into();
                cx.notify();
            }
        }
    }
    fn inspector(&self, mode: Mode, cx: &mut Context<Self>) -> Div {
        let p = self.p();
        let image_mode = mode == Mode::Edit && (self.tool == 1 || self.tool == 3);
        let text_mode =
            (mode == Mode::Edit && !image_mode) || (mode == Mode::Annotate && self.tool == 6);
        column()
            .w(px(232.))
            .flex_shrink_0()
            .border_l_1()
            .border_color(p.line)
            .bg(p.bg)
            .child(
                column()
                    .id("properties")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .p_3()
                    .gap_3()
                    .child(
                        row()
                            .justify_between()
                            .child(div().font_weight(FontWeight::SEMIBOLD).child(
                                if mode == Mode::Annotate {
                                    "标注属性"
                                } else if image_mode {
                                    "图片属性"
                                } else {
                                    "文字属性"
                                },
                            ))
                            .child(div().text_size(px(9.)).text_color(p.accent).child("预览")),
                    )
                    .when(text_mode, |this| {
                        this.child(
                            column()
                                .gap_2()
                                .child(style::eyebrow("字体", p))
                                .child(Input::new(&self.font_input).xsmall()),
                        )
                        .child(
                            row()
                                .gap_2()
                                .child(
                                    Button::new("font-size")
                                        .xsmall()
                                        .label(format!("{} pt", self.font_size))
                                        .tooltip("点击切换示例字号")
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.font_size = if this.font_size >= 28 {
                                                12
                                            } else {
                                                this.font_size + 2
                                            };
                                            cx.notify()
                                        })),
                                )
                                .child(
                                    Button::new("bold")
                                        .ghost()
                                        .xsmall()
                                        .icon(IconName::Bold)
                                        .selected(self.bold)
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.bold = !this.bold;
                                            cx.notify()
                                        })),
                                )
                                .child(
                                    Button::new("italic")
                                        .ghost()
                                        .xsmall()
                                        .icon(IconName::Italic)
                                        .selected(self.italic)
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.italic = !this.italic;
                                            cx.notify()
                                        })),
                                ),
                        )
                    })
                    .when(image_mode, |this| {
                        this.child(
                            div().text_color(p.muted).text_size(px(12.)).child(
                                "选择原图的绘制实例，而不是覆盖原图。共享资源需按实例隔离。",
                            ),
                        )
                        .child(
                            Button::new("image-scale")
                                .xsmall()
                                .label("缩放 · 未接入")
                                .disabled(true),
                        )
                        .child(
                            Button::new("image-rotate")
                                .xsmall()
                                .icon(IconName::RotateCw)
                                .label("旋转 · 未接入")
                                .disabled(true),
                        )
                        .child(
                            Button::new("image-delete")
                                .xsmall()
                                .icon(IconName::Trash)
                                .label("删除 · 未接入")
                                .disabled(true),
                        )
                    })
                    .when(mode == Mode::Annotate && !text_mode, |this| {
                        this.child(div().text_size(px(12.)).text_color(p.muted).child(
                            "只预览工具参数，当前不在页面绘制。橡皮擦最终只处理笔迹，不修改原文。",
                        ))
                        .child(
                            Button::new("stroke-width")
                                .xsmall()
                                .label(format!("线宽   {} px", self.font_size / 6))
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.font_size = if this.font_size >= 30 {
                                        6
                                    } else {
                                        this.font_size + 6
                                    };
                                    cx.notify()
                                })),
                        )
                    })
                    .child(style::eyebrow("颜色", p))
                    .child(
                        row().gap_2().children(
                            [0x365f48, 0xd9b657, 0xcc8167, 0x829fbd]
                                .into_iter()
                                .enumerate()
                                .map(|(i, color)| {
                                    div()
                                        .id(("swatch", i))
                                        .size(px(27.))
                                        .rounded_full()
                                        .bg(rgb(color))
                                        .border_2()
                                        .border_color(if self.color == i { p.text } else { p.bg })
                                        .cursor_pointer()
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            this.color = i;
                                            cx.notify()
                                        }))
                                }),
                        ),
                    )
                    .child(
                        Button::new("opacity")
                            .xsmall()
                            .label(format!("透明度   {}%", self.opacity))
                            .tooltip("点击切换 25 / 50 / 75 / 100%")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.opacity = if this.opacity <= 25 {
                                    100
                                } else {
                                    this.opacity - 25
                                };
                                cx.notify()
                            })),
                    )
                    .when(text_mode, |this| {
                        this.child(
                            row().gap_2().children(
                                [
                                    IconName::TextAlignStart,
                                    IconName::TextAlignCenter,
                                    IconName::TextAlignEnd,
                                ]
                                .into_iter()
                                .enumerate()
                                .map(|(i, icon)| {
                                    Button::new(("align", i))
                                        .ghost()
                                        .xsmall()
                                        .icon(icon)
                                        .selected(self.alignment == i)
                                        .tooltip(["左对齐", "居中", "右对齐"][i])
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            this.alignment = i;
                                            cx.notify()
                                        }))
                                }),
                            ),
                        )
                        .child(
                            Button::new("text-background")
                                .xsmall()
                                .label(format!(
                                    "背景色   {}",
                                    ["浅松绿", "画布色", "面板色"][self.background]
                                ))
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.background = (this.background + 1) % 3;
                                    cx.notify()
                                })),
                        )
                        .child(style::eyebrow("独立样式预览 · 非页面编辑", p))
                        .child(Input::new(&self.text_input).xsmall())
                        .child(
                            div()
                                .p_4()
                                .rounded_md()
                                .bg(match self.background {
                                    1 => p.bg,
                                    2 => p.panel,
                                    _ => p.tint,
                                })
                                .text_size(px(self.font_size as f32))
                                .text_color(rgb(
                                    [0x365f48, 0xd9b657, 0xcc8167, 0x829fbd][self.color]
                                ))
                                .opacity(self.opacity as f32 / 100.)
                                .font_weight(if self.bold {
                                    FontWeight::BOLD
                                } else {
                                    FontWeight::NORMAL
                                })
                                .when(self.italic, |d| d.italic())
                                .when(self.alignment == 1, |d| d.text_center())
                                .when(self.alignment == 2, |d| d.text_right())
                                .child(self.text_input.read(cx).value()),
                        )
                        .child(
                            div()
                                .text_size(px(11.))
                                .text_color(p.muted)
                                .child("字体名称为输入交互，暂不切换实际字体。"),
                        )
                    })
                    .child(div().flex_1())
                    .child(
                        Button::new("apply-edit")
                            .primary()
                            .label("应用到文档 · 尚未开放")
                            .disabled(true),
                    )
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(p.muted)
                            .child("不会遮盖原文，不会栅格化替换。当前无文件写入。"),
                    ),
            )
    }
}
