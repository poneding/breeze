use crate::*;
use gpui_kit::component::{
    h_resizable,
    menu::{DropdownMenu, PopupMenuItem},
    resizable_panel,
    tab::{Tab, TabBar},
    tooltip::Tooltip,
};

pub(super) const NAVIGATION_MIN_WIDTH: f32 = 192.;

impl Breeze {
    pub(super) fn reader(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Div {
        let p = self.p();
        let mode = self.books[self.active].state.mode;
        let main = column()
            .size_full()
            .min_w_0()
            .min_h_0()
            .child(
                row()
                    .id("reader-toolbar")
                    .test_support()
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
                            .small()
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
                                Tooltip::new("标注模式")
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
                            .small()
                            .icon(IconName::Info)
                            .accessibility_label("原型说明")
                            .tooltip(format!(
                                "批注保存在旁置文件；原 PDF 不变。原文编辑仍未开放{}",
                                self.preview
                                    .as_ref()
                                    .map(|p| format!(" · 本页后台准备 {:.0} ms", p.elapsed_ms))
                                    .unwrap_or_default()
                                    + &self
                                        .preview
                                        .as_ref()
                                        .and_then(|p| p.text_warning.clone())
                                        .map(|w| format!(" · {w}"))
                                        .unwrap_or_default()
                            )),
                    )
                    .child(
                        self.command("reader-star", IconName::Bookmark, "收藏", &Favorite, cx)
                            .selected(self.books[self.active].favorite),
                    ),
            )
            .when(self.search_open, |d| d.child(self.search_bar(cx)))
            .child(
                row()
                    .flex_1()
                    .min_h_0()
                    .items_stretch()
                    .child(self.pages_view(window, cx))
                    .when(mode != Mode::Read, |d| d.child(self.inspector(mode, cx))),
            )
            .child(self.status(window, cx));
        if self.sidebar {
            div().flex_1().min_h_0().child(
                h_resizable("reader-split")
                    .with_state(&self.navigation_split)
                    .child(
                        resizable_panel()
                            .size(px(self.navigation_width))
                            .size_range(px(NAVIGATION_MIN_WIDTH)..px(360.))
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
                (IconName::Search, "搜索"),
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
                    .small()
                    .icon(icon)
                    .accessibility_label(label)
                    .tooltip(label)
                    .selected(self.tool == i)
                    .on_click(cx.listener(move |this, _, window, cx| {
                        if mode == Mode::Read && i == 1 {
                            this.execute(&SearchDocument, window, cx);
                            return;
                        }
                        this.gesture.clear();
                        this.tool = i;
                        cx.notify()
                    }))
            }))
    }
    pub(super) fn navigation_outline(&self) -> Option<Arc<Vec<breeze_pdf::Outline>>> {
        self.preview
            .as_ref()
            .map(|preview| preview.outline.clone())
            .or_else(|| self.reading.outline())
    }

    fn has_outline(&self) -> bool {
        self.navigation_outline()
            .is_some_and(|outline| !outline.is_empty())
    }

    pub(super) fn show_thumbnails(&self) -> bool {
        self.thumbs || !self.has_outline()
    }

    fn navigation(&self, cx: &mut Context<Self>) -> Div {
        if self.search_open {
            return self.search_results(cx);
        }
        let has_outline = self.has_outline();
        let outline_selected = has_outline && !self.thumbs;
        let nested = self.navigation_outline().is_some_and(|outline| {
            (0..outline.len()).any(|index| reading::outline_has_children(&outline, index))
        });
        let p = self.p();
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
                    .id("navigation-header")
                    .test_support()
                    .h(px(38.))
                    .justify_start()
                    .flex_shrink_0()
                    .px(px(8.))
                    .border_b_1()
                    .border_color(p.line)
                    .child(
                        TabBar::new("nav-tabs")
                            .segmented()
                            .small()
                            .flex_shrink_0()
                            .selected_index(usize::from(has_outline && self.thumbs))
                            .on_click(cx.listener(move |this, index, window, cx| {
                                this.thumbs = !has_outline || *index == 1;
                                window.focus(&this.focus, cx);
                                cx.notify()
                            }))
                            .when(has_outline, |tabs| {
                                tabs.child(Tab::new().label("目录").tooltip(|window, cx| {
                                    Tooltip::new("目录")
                                        .action(&ShowOutline, Some("Breeze"))
                                        .build(window, cx)
                                }))
                            })
                            .child(Tab::new().label("缩略图").tooltip(|window, cx| {
                                Tooltip::new("缩略图")
                                    .action(&ShowThumbnails, Some("Breeze"))
                                    .build(window, cx)
                            })),
                    )
                    .child(div().flex_1().min_w(px(4.)))
                    .when(outline_selected, |header| {
                        header.child(
                            row()
                                .gap(px(4.))
                                .flex_shrink_0()
                                .child(
                                    Button::new("outline-search")
                                        .ghost()
                                        .small()
                                        .icon(IconName::Search)
                                        .selected(self.outline_search_open())
                                        .accessibility_label("搜索目录标题")
                                        .tooltip("搜索目录标题")
                                        .on_click(cx.listener(|this, _, window, cx| {
                                            this.toggle_outline_search(window, cx);
                                        })),
                                )
                                .when(nested, |actions| actions.child(self.outline_menu(cx))),
                        )
                    }),
            )
            .when(outline_selected, |panel| {
                panel.children(self.outline_search_bar(cx))
            })
            .child(
                column()
                    .id("navigation-content")
                    .test_support()
                    .flex_1()
                    .min_h_0()
                    .when_some(self.navigation_outline(), |this, outline| {
                        if self.show_thumbnails() {
                            this.child(self.thumbnails_view(cx))
                        } else {
                            this.child(self.outline_view(outline, cx))
                        }
                    }),
            )
    }
    fn outline_menu(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let view = cx.entity();
        Button::new("outline-more")
            .ghost()
            .small()
            .icon(IconName::EllipsisVertical)
            .accessibility_label("更多目录操作")
            .tooltip("更多目录操作")
            .dropdown_menu(move |menu, window, _| {
                menu.item(
                    PopupMenuItem::new("全部展开")
                        .icon(IconName::ListChevronsUpDown)
                        .on_click(window.listener_for(&view, |this, _, _, cx| {
                            this.set_outline_expanded(true, cx);
                        })),
                )
                .item(
                    PopupMenuItem::new("全部收起")
                        .icon(IconName::ListChevronsDownUp)
                        .on_click(window.listener_for(&view, |this, _, _, cx| {
                            this.set_outline_expanded(false, cx);
                        })),
                )
            })
    }
    fn status(&self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let p = self.p();
        let s = &self.books[self.active].state;
        let visible = s.layout.pages(s.navigation.page(), s.navigation.total());
        row()
            .id("status-scroll")
            .test_support()
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
                .disabled(visible.start == 0),
            )
            .child(div().text_size(px(12.)).child(format!(
                "{} / {}",
                s.navigation.page() + 1,
                s.navigation.total()
            )))
            .child(
                self.command("next", IconName::ChevronRight, "下一页", &NextPage, cx)
                    .disabled(visible.end == s.navigation.total()),
            )
            .child(div().w(px(42.)).child(Input::new(&self.page_input).small()))
            .child(
                Button::new("jump")
                    .ghost()
                    .small()
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
                    .text_size(px(11.))
                    .text_color(p.muted)
                    .child(if !self.message.is_empty() {
                        self.message.clone()
                    } else if self.books[self.active].annotations.dirty() {
                        "批注未保存".into()
                    } else {
                        "原件未修改".into()
                    }),
            )
            .child(self.layout_button(cx))
            .child(self.zoom_button(cx))
            .child(self.command("zoom-out", IconName::Minus, "缩小", &ZoomOut, cx))
            .child(
                div()
                    .id("zoom-percent")
                    .test_support()
                    .min_w(px(48.))
                    .text_center()
                    .text_size(px(12.))
                    .child(format!("{:.0}%", self.display_zoom(window))),
            )
            .child(self.command("zoom-in", IconName::Plus, "放大", &ZoomIn, cx))
            .into_any_element()
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
        if mode == Mode::Annotate {
            return self.annotation_inspector(cx);
        }
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
                                .child(Input::new(&self.font_input).small()),
                        )
                        .child(
                            row()
                                .gap_2()
                                .child(
                                    Button::new("font-size")
                                        .small()
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
                                        .small()
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
                                        .small()
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
                            div().text_color(p.muted).text_size(px(13.)).child(
                                "选择原图的绘制实例，而不是覆盖原图。共享资源需按实例隔离。",
                            ),
                        )
                        .child(
                            Button::new("image-scale")
                                .small()
                                .label("缩放 · 未接入")
                                .disabled(true),
                        )
                        .child(
                            Button::new("image-rotate")
                                .small()
                                .icon(IconName::RotateCw)
                                .label("旋转 · 未接入")
                                .disabled(true),
                        )
                        .child(
                            Button::new("image-delete")
                                .small()
                                .icon(IconName::Trash)
                                .label("删除 · 未接入")
                                .disabled(true),
                        )
                    })
                    .when(mode == Mode::Annotate && !text_mode, |this| {
                        this.child(div().text_size(px(13.)).text_color(p.muted).child(
                            "只预览工具参数，当前不在页面绘制。橡皮擦最终只处理笔迹，不修改原文。",
                        ))
                        .child(
                            Button::new("stroke-width")
                                .small()
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
                            .small()
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
                                        .small()
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
                                .small()
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
                        .child(Input::new(&self.text_input).small())
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
                                .text_size(px(12.))
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
                            .text_size(px(12.))
                            .text_color(p.muted)
                            .child("不会遮盖原文，不会栅格化替换。当前无文件写入。"),
                    ),
            )
    }
}
