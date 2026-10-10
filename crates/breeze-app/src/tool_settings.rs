use crate::*;
use breeze_core::library::CategoryColor;
use gpui_kit::component::{
    Theme,
    hover_card::HoverCard,
    input::NumberInput,
    popover::Popover,
    slider::{Slider, SliderEvent, SliderState},
};
use std::time::Duration;

actions!(breeze_tool_settings, [CloseToolSettings]);

pub(super) const ANNOTATION_TOOLS: [(IconName, &str); 12] = [
    (IconName::Highlighter, "文本高亮"),
    (IconName::Scan, "区域高亮"),
    (IconName::Underline, "下划线"),
    (IconName::Strikethrough, "删除线"),
    (IconName::Pencil, "画笔"),
    (IconName::Eraser, "橡皮擦"),
    (IconName::Type, "文本批注"),
    (IconName::Circle, "圆形"),
    (IconName::Square, "矩形"),
    (IconName::Minus, "直线"),
    (IconName::LineSquiggle, "波浪线"),
    (IconName::ArrowUpRight, "箭头"),
];
const EDIT_TOOLS: [(IconName, &str); 4] = [
    (IconName::TextCursorInput, "文字"),
    (IconName::Image, "图片"),
    (IconName::Type, "插入文字"),
    (IconName::ImagePlus, "插入图片"),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ToolKey {
    mode: Mode,
    index: usize,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct AnnotationStyle {
    pub color: CategoryColor,
    pub width: f32,
    pub opacity: u16,
}

impl Default for AnnotationStyle {
    fn default() -> Self {
        Self {
            color: CategoryColor::Red,
            width: 2.,
            opacity: 100,
        }
    }
}

#[derive(Clone, Copy)]
struct TextStyle {
    color: CategoryColor,
    background: Option<CategoryColor>,
    size: u16,
    opacity: u16,
    bold: bool,
    italic: bool,
    alignment: usize,
}

impl Default for TextStyle {
    fn default() -> Self {
        Self {
            color: CategoryColor::Blue,
            background: None,
            size: 18,
            opacity: 100,
            bold: false,
            italic: false,
            alignment: 0,
        }
    }
}

pub(super) struct ToolSettings {
    pub annotation: [AnnotationStyle; 12],
    text: [TextStyle; 2],
    keyboard_open: Option<ToolKey>,
    epoch: usize,
    controls: Option<ToolControls>,
}

struct ToolControls {
    key: ToolKey,
    width: Entity<InputState>,
    opacity: Entity<SliderState>,
    _subscriptions: Vec<Subscription>,
}

impl Default for ToolSettings {
    fn default() -> Self {
        let mut annotation = [AnnotationStyle::default(); 12];
        for index in [0, 1] {
            annotation[index].color = CategoryColor::Amber;
            annotation[index].opacity = 40;
        }
        Self {
            annotation,
            text: [TextStyle::default(); 2],
            keyboard_open: None,
            epoch: 0,
            controls: None,
        }
    }
}

impl ToolSettings {
    pub fn dismiss(&mut self) {
        self.keyboard_open = None;
        self.controls = None;
        self.epoch = self.epoch.wrapping_add(1);
    }
}

pub(super) fn bind(cx: &mut App) {
    cx.bind_keys([KeyBinding::new(
        "escape",
        CloseToolSettings,
        Some("ToolSettings"),
    )]);
}

impl Breeze {
    pub(super) fn tools(&self, mode: Mode, cx: &mut Context<Self>) -> Stateful<Div> {
        let tools = match mode {
            Mode::Read => vec![(IconName::MousePointer2, "浏览")],
            Mode::Annotate => ANNOTATION_TOOLS.to_vec(),
            Mode::Edit => EDIT_TOOLS.to_vec(),
        };
        row()
            .id("tools")
            .flex_1()
            .min_w_0()
            .overflow_x_scroll()
            .gap_1()
            .when(mode == Mode::Annotate, |tools| {
                tools.child(
                    Button::new(("tool", annotation_editing::SELECT_TOOL))
                        .ghost()
                        .small()
                        .icon(IconName::MousePointer2)
                        .selected(self.tool == annotation_editing::SELECT_TOOL)
                        .accessibility_label("选择标注")
                        .tooltip("选择标注以修改、移动或删除")
                        .on_click(cx.listener(|this, _, _, cx| {
                            if !this.annotation_navigation_allowed(cx) {
                                return;
                            }
                            this.tool = annotation_editing::SELECT_TOOL;
                            this.gesture.clear();
                            cx.notify();
                        })),
                )
            })
            .children(tools.into_iter().enumerate().map(|(index, (icon, label))| {
                let key = ToolKey { mode, index };
                let button = Button::new(("tool", index))
                    .ghost()
                    .small()
                    .icon(icon)
                    .accessibility_label(label)
                    .selected(self.tool == index)
                    .on_click(cx.listener(move |this, event, _, cx| {
                        if !this.annotation_navigation_allowed(cx) {
                            return;
                        }
                        this.gesture.clear();
                        this.tool = index;
                        if mode != Mode::Read && matches!(event, ClickEvent::Keyboard(_)) {
                            this.tool_settings.keyboard_open = Some(key);
                        }
                        cx.notify();
                    }));
                if mode == Mode::Read {
                    return button.tooltip(label).into_any_element();
                }
                let id = SharedString::from(format!(
                    "tool-settings-{}-{}-{index}-{}",
                    self.tool_settings.epoch,
                    mode.index(),
                    if self.tool_settings.keyboard_open == Some(key) {
                        "keyboard"
                    } else {
                        "hover"
                    }
                ));
                let view = cx.entity().downgrade();
                // Keyboard activation pins a native popover with Escape and focus restoration.
                // Mouse users keep a hover card that stays open while its controls are hovered.
                if self.tool_settings.keyboard_open == Some(key) {
                    return Popover::new(id)
                        .anchor(Anchor::TopLeft)
                        .open(true)
                        .trigger(button)
                        .on_open_change(cx.listener(|this, open, _, cx| {
                            if !open {
                                this.tool_settings.dismiss();
                                cx.notify();
                            }
                        }))
                        .content(move |_, window, cx| {
                            view.update(cx, |this, cx| this.tool_settings_content(key, window, cx))
                                .unwrap_or_else(|_| div().into_any_element())
                        })
                        .into_any_element();
                }
                HoverCard::new(id)
                    .anchor(Anchor::TopLeft)
                    .open_delay(Duration::from_millis(350))
                    .close_delay(Duration::from_millis(300))
                    .trigger(button)
                    .content(move |_, window, cx| {
                        view.update(cx, |this, cx| this.tool_settings_content(key, window, cx))
                            .unwrap_or_else(|_| div().into_any_element())
                    })
                    .into_any_element()
            }))
    }

    fn tool_settings_content(
        &mut self,
        key: ToolKey,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.prepare_tool_controls(key, window, cx);
        let label = if key.mode == Mode::Annotate {
            ANNOTATION_TOOLS[key.index].1
        } else {
            EDIT_TOOLS[key.index].1
        };
        column()
            .id("tool-settings-content")
            .test_support()
            .w(px(256.))
            .max_h((window.viewport_size().height - px(120.)).max(px(160.)))
            .overflow_y_scroll()
            .key_context("ToolSettings")
            .gap_3()
            .on_action(cx.listener(|this, _: &CloseToolSettings, window, cx| {
                this.tool_settings.dismiss();
                window.focus(&this.focus, cx);
                cx.notify();
            }))
            .child(div().font_weight(FontWeight::SEMIBOLD).child(label))
            .child(if key.mode == Mode::Annotate {
                self.annotation_tool_settings(key, cx)
            } else {
                self.edit_tool_settings(key, cx)
            })
            .into_any_element()
    }

    fn annotation_tool_settings(&self, key: ToolKey, cx: &mut Context<Self>) -> Div {
        let muted = Theme::global(cx).muted_foreground;
        if key.index == 5 {
            return div()
                .text_size(px(12.))
                .text_color(muted)
                .child("点击笔迹可擦除整条笔画，原文和其他标注保持不变。");
        }
        column()
            .gap_3()
            .child(self.tool_color_choices(key, false, cx))
            .when(!matches!(key.index, 0 | 1 | 6), |content| {
                content.child(
                    column()
                        .gap_1()
                        .child(setting_label("线宽（0.25–100 pt）", muted))
                        .when_some(self.tool_settings.controls.as_ref(), |content, controls| {
                            content.child(
                                div()
                                    .id("ink-width")
                                    .test_support()
                                    .w_full()
                                    .child(NumberInput::new(&controls.width).small().suffix("pt")),
                            )
                        }),
                )
            })
            .child(self.tool_opacity_choices(key, cx))
            .when(key.index == 6, |content| {
                content.child(
                    div().text_size(px(11.)).text_color(muted).child(
                        "点击页面输入文本，Enter 完成，Esc 取消。导出为可打开全文的便笺图标。",
                    ),
                )
            })
    }

    fn tool_color_choices(&self, key: ToolKey, background: bool, cx: &mut Context<Self>) -> Div {
        let theme = Theme::global(cx);
        let foreground = theme.foreground;
        let selected = if key.mode == Mode::Annotate {
            Some(self.tool_settings.annotation[key.index].color)
        } else if background {
            self.tool_settings.text[key.index / 2].background
        } else {
            Some(self.tool_settings.text[key.index / 2].color)
        };
        column()
            .gap_1()
            .child(setting_label(
                if background { "背景色" } else { "颜色" },
                theme.muted_foreground,
            ))
            .child(
                row()
                    .gap_1()
                    .children(
                        CategoryColor::ALL
                            .into_iter()
                            .enumerate()
                            .map(|(index, color)| {
                                Button::new((
                                    if background {
                                        "tool-background"
                                    } else {
                                        "tool-color"
                                    },
                                    index,
                                ))
                                .ghost()
                                .small()
                                .size(px(28.))
                                .p_0()
                                .accessibility_label(format!(
                                    "{}：{}",
                                    if background { "背景色" } else { "颜色" },
                                    colors::label(color)
                                ))
                                .child(
                                    div()
                                        .size(px(18.))
                                        .flex_shrink_0()
                                        .rounded_full()
                                        .bg(rgb(colors::hex(color)))
                                        .when(selected == Some(color), |swatch| {
                                            swatch.border_2().border_color(foreground)
                                        }),
                                )
                                .on_click(cx.listener(
                                    move |this, _, _, cx| {
                                        if key.mode == Mode::Annotate {
                                            this.tool_settings.annotation[key.index].color = color;
                                        } else if background {
                                            this.tool_settings.text[key.index / 2].background =
                                                Some(color);
                                        } else {
                                            this.tool_settings.text[key.index / 2].color = color;
                                        }
                                        cx.notify();
                                    },
                                ))
                            }),
                    ),
            )
            .when(background, |content| {
                content.child(
                    Button::new("clear-text-background")
                        .ghost()
                        .small()
                        .label("无背景")
                        .selected(selected.is_none())
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.tool_settings.text[key.index / 2].background = None;
                            cx.notify();
                        })),
                )
            })
    }

    fn tool_opacity_choices(&self, key: ToolKey, cx: &mut Context<Self>) -> Div {
        let opacity = if key.mode == Mode::Annotate {
            self.tool_settings.annotation[key.index].opacity
        } else {
            self.tool_settings.text[key.index / 2].opacity
        };
        column()
            .gap_1()
            .child(
                row()
                    .justify_between()
                    .child(setting_label(
                        "不透明度",
                        Theme::global(cx).muted_foreground,
                    ))
                    .child(
                        div()
                            .id("tool-opacity-value")
                            .test_support()
                            .text_size(px(11.))
                            .child(format!("{opacity}%")),
                    ),
            )
            .when_some(self.tool_settings.controls.as_ref(), |content, controls| {
                content.child(
                    div()
                        .id("tool-opacity")
                        .test_support()
                        .w_full()
                        .px_2()
                        .child(Slider::new(&controls.opacity)),
                )
            })
    }

    fn prepare_tool_controls(&mut self, key: ToolKey, window: &mut Window, cx: &mut Context<Self>) {
        if self
            .tool_settings
            .controls
            .as_ref()
            .is_some_and(|controls| controls.key == key)
        {
            return;
        }
        let width_value = self
            .tool_settings
            .annotation
            .get(key.index)
            .map_or(2., |style| style.width);
        let opacity_value = if key.mode == Mode::Annotate {
            self.tool_settings.annotation[key.index].opacity
        } else {
            self.tool_settings.text[key.index / 2].opacity
        };
        let width = cx.new(|cx| {
            InputState::new(window, cx)
                .default_value(width_value.to_string())
                .step(0.25)
                .min(0.25)
                .max(100.)
        });
        let opacity = cx.new(|_| {
            SliderState::new()
                .min(0.)
                .max(100.)
                .step(1.)
                .default_value(opacity_value as f32)
        });
        let width_subscription =
            cx.subscribe_in(&width, window, move |this, input, event, window, cx| {
                if !this
                    .tool_settings
                    .controls
                    .as_ref()
                    .is_some_and(|controls| controls.width == *input && controls.key == key)
                {
                    return;
                }
                if key.mode != Mode::Annotate {
                    return;
                }
                let value = input
                    .read(cx)
                    .value()
                    .trim()
                    .parse::<f32>()
                    .ok()
                    .filter(|value| value.is_finite() && (0.25..=100.).contains(value));
                if matches!(
                    event,
                    InputEvent::Change | InputEvent::PressEnter { .. } | InputEvent::Blur
                ) {
                    if let Some(value) = value {
                        this.tool_settings.annotation[key.index].width = value;
                    } else if !matches!(event, InputEvent::Change) {
                        let previous = this.tool_settings.annotation[key.index].width;
                        input.update(cx, |input, cx| {
                            input.set_value(previous.to_string(), window, cx)
                        });
                    }
                    cx.notify();
                }
            });
        let opacity_subscription = cx.subscribe(&opacity, move |this, slider, event, cx| {
            if !this
                .tool_settings
                .controls
                .as_ref()
                .is_some_and(|controls| controls.opacity == slider && controls.key == key)
            {
                return;
            }
            if let SliderEvent::Change(value) = event {
                let value = value.end().round().clamp(0., 100.) as u16;
                if key.mode == Mode::Annotate {
                    this.tool_settings.annotation[key.index].opacity = value;
                } else {
                    this.tool_settings.text[key.index / 2].opacity = value;
                }
                cx.notify();
            }
        });
        self.tool_settings.controls = Some(ToolControls {
            key,
            width,
            opacity,
            _subscriptions: vec![width_subscription, opacity_subscription],
        });
    }

    fn edit_tool_settings(&self, key: ToolKey, cx: &mut Context<Self>) -> Div {
        let muted = Theme::global(cx).muted_foreground;
        if key.index == 1 || key.index == 3 {
            return column()
                .gap_2()
                .child(setting_label("原图编辑尚未开放", muted))
                .children(
                    [
                        (IconName::Scaling, "缩放"),
                        (IconName::RotateCw, "旋转"),
                        (IconName::Trash, "删除"),
                    ]
                    .into_iter()
                    .map(|(icon, label)| {
                        Button::new(SharedString::from(format!("image-{label}")))
                            .small()
                            .icon(icon)
                            .label(label)
                            .disabled(true)
                    }),
                );
        }
        let style = self.tool_settings.text[key.index / 2];
        column()
            .gap_3()
            .child(setting_label("独立样式预览 · 原文编辑尚未开放", muted))
            .child(Input::new(&self.font_input).small())
            .child(
                row()
                    .gap_1()
                    .child(
                        Button::new("font-smaller")
                            .ghost()
                            .small()
                            .icon(IconName::Minus)
                            .accessibility_label("减小预览字号")
                            .on_click(cx.listener(move |this, _, _, cx| {
                                let style = &mut this.tool_settings.text[key.index / 2];
                                style.size = style.size.saturating_sub(2).max(8);
                                cx.notify();
                            })),
                    )
                    .child(
                        div()
                            .w(px(48.))
                            .text_center()
                            .child(format!("{} pt", style.size)),
                    )
                    .child(
                        Button::new("font-larger")
                            .ghost()
                            .small()
                            .icon(IconName::Plus)
                            .accessibility_label("增大预览字号")
                            .on_click(cx.listener(move |this, _, _, cx| {
                                let style = &mut this.tool_settings.text[key.index / 2];
                                style.size = (style.size + 2).min(72);
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new("bold")
                            .ghost()
                            .small()
                            .icon(IconName::Bold)
                            .accessibility_label("粗体预览")
                            .selected(style.bold)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                let style = &mut this.tool_settings.text[key.index / 2];
                                style.bold = !style.bold;
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new("italic")
                            .ghost()
                            .small()
                            .icon(IconName::Italic)
                            .accessibility_label("斜体预览")
                            .selected(style.italic)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                let style = &mut this.tool_settings.text[key.index / 2];
                                style.italic = !style.italic;
                                cx.notify();
                            })),
                    ),
            )
            .child(self.tool_color_choices(key, false, cx))
            .child(self.tool_color_choices(key, true, cx))
            .child(self.tool_opacity_choices(key, cx))
            .child(
                row().gap_1().children(
                    [
                        (IconName::TextAlignStart, "左对齐"),
                        (IconName::TextAlignCenter, "居中"),
                        (IconName::TextAlignEnd, "右对齐"),
                    ]
                    .into_iter()
                    .enumerate()
                    .map(|(index, (icon, label))| {
                        Button::new(("align", index))
                            .ghost()
                            .small()
                            .icon(icon)
                            .accessibility_label(label)
                            .selected(style.alignment == index)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.tool_settings.text[key.index / 2].alignment = index;
                                cx.notify();
                            }))
                    }),
                ),
            )
            .child(Input::new(&self.edit_text_input).small())
            .child(
                div()
                    .p_3()
                    .rounded_md()
                    .bg(style
                        .background
                        .map(colors::hex)
                        .map(rgb)
                        .unwrap_or(rgb(0xffffff)))
                    .text_size(px(style.size as f32))
                    .text_color(rgb(colors::hex(style.color)))
                    .opacity(style.opacity as f32 / 100.)
                    .font_weight(if style.bold {
                        FontWeight::BOLD
                    } else {
                        FontWeight::NORMAL
                    })
                    .when(style.italic, |text| text.italic())
                    .when(style.alignment == 1, |text| text.text_center())
                    .when(style.alignment == 2, |text| text.text_right())
                    .child(self.edit_text_input.read(cx).value()),
            )
            .child(setting_label(
                "字体名称仅供输入预览，暂不切换实际字体。",
                muted,
            ))
    }
}

fn setting_label(label: &'static str, muted: Hsla) -> Div {
    div().text_size(px(11.)).text_color(muted).child(label)
}

#[cfg(test)]
mod tests;
