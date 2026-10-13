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

pub(super) fn annotation_tools() -> [(IconName, &'static str); 12] {
    [
        (IconName::Highlighter, tr!("文本高亮")),
        (IconName::Scan, tr!("区域高亮")),
        (IconName::Underline, tr!("下划线")),
        (IconName::Strikethrough, tr!("删除线")),
        (IconName::Pencil, tr!("画笔")),
        (IconName::Eraser, tr!("橡皮擦")),
        (IconName::Type, tr!("文本批注")),
        (IconName::Circle, tr!("圆形")),
        (IconName::Square, tr!("矩形")),
        (IconName::Minus, tr!("直线")),
        (IconName::LineSquiggle, tr!("波浪线")),
        (IconName::ArrowUpRight, tr!("箭头")),
    ]
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ToolKey {
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

pub(super) struct ToolSettings {
    pub annotation: [AnnotationStyle; 12],
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
            Mode::Read => vec![(IconName::MousePointer2, tr!("浏览"))],
            Mode::Annotate => annotation_tools().to_vec(),
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
                        .accessibility_label(tr!("选择标注"))
                        .tooltip(tr!("选择标注以修改、移动或删除"))
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
                let key = ToolKey { index };
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
        let label = annotation_tools()[key.index].1;
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
            .child(self.annotation_tool_settings(key, cx))
            .into_any_element()
    }

    fn annotation_tool_settings(&self, key: ToolKey, cx: &mut Context<Self>) -> Div {
        let muted = Theme::global(cx).muted_foreground;
        if key.index == 5 {
            return div()
                .text_size(px(12.))
                .text_color(muted)
                .child(tr!("点击笔迹可擦除整条笔画，原文和其他标注保持不变。"));
        }
        column()
            .gap_3()
            .child(self.tool_color_choices(key, cx))
            .when(!matches!(key.index, 0 | 1 | 6), |content| {
                content.child(
                    column()
                        .gap_1()
                        .child(setting_label(tr!("线宽（0.25–100 pt）"), muted))
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
                content.child(div().text_size(px(11.)).text_color(muted).child(tr!(
                    "点击页面输入文本，Enter 完成，Esc 取消。导出为可打开全文的便笺图标。"
                )))
            })
    }

    fn tool_color_choices(&self, key: ToolKey, cx: &mut Context<Self>) -> Div {
        let theme = Theme::global(cx);
        let foreground = theme.foreground;
        let selected = self.tool_settings.annotation[key.index].color;
        column()
            .gap_1()
            .child(setting_label(tr!("颜色"), theme.muted_foreground))
            .child(
                row()
                    .gap_1()
                    .children(
                        CategoryColor::ALL
                            .into_iter()
                            .enumerate()
                            .map(|(index, color)| {
                                Button::new(("tool-color", index))
                                    .ghost()
                                    .small()
                                    .size(px(28.))
                                    .p_0()
                                    .accessibility_label(format!(
                                        "{}：{}",
                                        tr!("颜色"),
                                        colors::label(color)
                                    ))
                                    .child(
                                        div()
                                            .size(px(18.))
                                            .flex_shrink_0()
                                            .rounded_full()
                                            .bg(rgb(colors::hex(color)))
                                            .when(selected == color, |swatch| {
                                                swatch.border_2().border_color(foreground)
                                            }),
                                    )
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.tool_settings.annotation[key.index].color = color;
                                        cx.notify();
                                    }))
                            }),
                    ),
            )
    }

    fn tool_opacity_choices(&self, key: ToolKey, cx: &mut Context<Self>) -> Div {
        let opacity = self.tool_settings.annotation[key.index].opacity;
        column()
            .gap_1()
            .child(
                row()
                    .justify_between()
                    .child(setting_label(
                        tr!("不透明度"),
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
        let opacity_value = self.tool_settings.annotation[key.index].opacity;
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
                this.tool_settings.annotation[key.index].opacity = value;
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
}

fn setting_label(label: &'static str, muted: Hsla) -> Div {
    div().text_size(px(11.)).text_color(muted).child(label)
}

#[cfg(test)]
mod tests;
