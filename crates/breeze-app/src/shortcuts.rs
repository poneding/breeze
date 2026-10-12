//! Read-only shortcut reference. Bindings stay owned by `commands::bind`.
use crate::*;
use gpui_kit::component::kbd::Kbd;

/// One documented shortcut: a label and the actions it triggers.
pub(super) struct Shortcut {
    pub label: &'static str,
    pub actions: Vec<&'static dyn Action>,
}

/// The documented shortcuts, grouped by area. Nothing here invents a binding;
/// every entry resolves through the registered key bindings at render time.
pub(super) fn groups() -> Vec<(&'static str, Vec<Shortcut>)> {
    let row = |label, actions| Shortcut { label, actions };
    vec![
        (
            tr!("文件"),
            vec![
                row(tr!("打开文档"), vec![&Open]),
                row(tr!("关闭当前标签"), vec![&CloseTab]),
                row(tr!("切换标签"), vec![&NextTab, &PreviousTab]),
                row(tr!("退出 Breeze"), vec![&Quit]),
            ],
        ),
        (
            tr!("阅读"),
            vec![
                row(tr!("上一页"), vec![&PreviousPage]),
                row(tr!("下一页"), vec![&NextPage]),
                row(tr!("跳转到页"), vec![&FocusPage]),
                row(tr!("放大"), vec![&ZoomIn]),
                row(tr!("缩小"), vec![&ZoomOut]),
                row(tr!("实际大小"), vec![&ActualSize]),
                row(tr!("适合页面"), vec![&FitPage]),
                row(tr!("适合宽度"), vec![&FitWidth]),
                row(tr!("切换阅读布局"), vec![&CycleLayout]),
                row(tr!("向左旋转 90 度"), vec![&RotateLeft]),
                row(tr!("向右旋转 90 度"), vec![&RotateRight]),
                row(tr!("全屏阅读"), vec![&ToggleFullScreen]),
                row(tr!("搜索文档"), vec![&SearchDocument]),
                row(tr!("复制选中文字"), vec![&CopySelection]),
            ],
        ),
        (
            tr!("标注"),
            vec![
                row(tr!("保存标注"), vec![&SaveAnnotations]),
                row(tr!("标注另存为"), vec![&SaveAnnotationsAs]),
                row(tr!("撤销标注"), vec![&UndoAnnotation]),
                row(tr!("重做标注"), vec![&RedoAnnotation]),
                row(tr!("删除选中批注"), vec![&DeleteSelectedAnnotation]),
                row(tr!("取消编辑"), vec![&CancelAnnotationEdit]),
            ],
        ),
        (
            tr!("界面"),
            vec![
                row(tr!("主页"), vec![&Home]),
                row(tr!("设置"), vec![&Preferences]),
                row(tr!("展开 / 收起导航"), vec![&ToggleSidebar]),
                row(tr!("展开 / 收起文档侧栏"), vec![&ToggleDocumentPanel]),
                row(tr!("收藏 / 取消收藏"), vec![&Favorite]),
                row(tr!("切换主题"), vec![&CycleTheme]),
            ],
        ),
    ]
}

/// Shortcut chips for one row. A binding without a key context (`Quit`) stays global.
fn key_chips(window: &Window, actions: &[&dyn Action]) -> Vec<Kbd> {
    actions
        .iter()
        .filter_map(|action| {
            Kbd::binding_for_action(*action, Some("Breeze"), window)
                .or_else(|| Kbd::global_binding_for_action(*action, window))
        })
        .collect()
}

impl Breeze {
    pub(super) fn shortcuts_page(&self, window: &Window, cx: &Context<Self>) -> impl IntoElement {
        let p = self.p(cx);
        column()
            .id("shortcuts-body")
            .test_support()
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .gap_4()
            .child(
                div()
                    .text_size(px(16.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(tr!("快捷键")),
            )
            .children(groups().into_iter().filter_map(|(title, rows)| {
                // Actions without a registered binding are omitted, never invented.
                let rows: Vec<_> = rows
                    .into_iter()
                    .map(|shortcut| (shortcut.label, key_chips(window, &shortcut.actions)))
                    .filter(|(_, chips)| !chips.is_empty())
                    .collect();
                (!rows.is_empty()).then(|| {
                    column().gap_1().child(style::eyebrow(title, p)).children(
                        rows.into_iter()
                            .map(|(label, chips)| shortcut_row(label, chips)),
                    )
                })
            }))
            .child(
                div()
                    .text_size(px(11.))
                    .text_color(p.muted)
                    .child(tr!("暂不支持自定义快捷键。")),
            )
    }
}

fn shortcut_row(label: &str, keys: Vec<Kbd>) -> impl IntoElement {
    row()
        .id(format!("shortcut: {label}"))
        .test_support()
        .py_1()
        .gap_3()
        .child(
            div()
                .flex_1()
                .min_w_0()
                .text_size(px(12.))
                .child(label.to_string()),
        )
        .children(keys)
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui_kit::test::TestWindowExt;

    #[gpui_kit::test]
    fn every_documented_shortcut_resolves_to_a_binding(cx: &mut TestAppContext) {
        let (handle, view) = crate::ui_tests::app(cx);
        cx.update_window(handle, |_, window, cx| {
            view.update(cx, |view, cx| {
                view.show_home_page(HomePage::Shortcuts, cx);
            });
            window.render_frame(cx);
            assert!(window.find("shortcuts-body").visible());
            // Every documented row either renders with real chips or is dropped.
            let mut rows = 0;
            for (_, shortcuts) in groups() {
                for shortcut in shortcuts {
                    let chips = key_chips(window, &shortcut.actions);
                    let row = window.try_find(format!("shortcut: {}", shortcut.label));
                    assert_eq!(
                        !chips.is_empty(),
                        row.is_some(),
                        "{} must render exactly when it has a binding",
                        shortcut.label
                    );
                    rows += usize::from(row.is_some());
                }
            }
            assert!(rows > 15, "expected most documented shortcuts to be bound");
            assert!(window.find("shortcut: 打开文档").visible());
        })
        .unwrap();
    }
}
