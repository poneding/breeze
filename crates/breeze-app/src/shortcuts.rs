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
            tr!("File"),
            vec![
                row(tr!("Open document"), vec![&Open]),
                row(tr!("Close current tab"), vec![&CloseTab]),
                row(tr!("Switch tabs"), vec![&NextTab, &PreviousTab]),
                row(tr!("Quit Breeze"), vec![&Quit]),
            ],
        ),
        (
            tr!("Read"),
            vec![
                row(tr!("Previous page"), vec![&PreviousPage]),
                row(tr!("Next page"), vec![&NextPage]),
                row(tr!("Go to page"), vec![&FocusPage]),
                row(tr!("Zoom in"), vec![&ZoomIn]),
                row(tr!("Zoom out"), vec![&ZoomOut]),
                row(tr!("Actual size"), vec![&ActualSize]),
                row(tr!("Fit page"), vec![&FitPage]),
                row(tr!("Fit width"), vec![&FitWidth]),
                row(tr!("Change reading layout"), vec![&CycleLayout]),
                row(tr!("Rotate 90° left"), vec![&RotateLeft]),
                row(tr!("Rotate 90° right"), vec![&RotateRight]),
                row(tr!("Fullscreen reading"), vec![&ToggleFullScreen]),
                row(tr!("Search document"), vec![&SearchDocument]),
                row(tr!("Copy selected text"), vec![&CopySelection]),
            ],
        ),
        (
            tr!("Annotations"),
            vec![
                row(tr!("Save annotations"), vec![&SaveAnnotations]),
                row(tr!("Save annotations as"), vec![&SaveAnnotationsAs]),
                row(tr!("Undo annotation"), vec![&UndoAnnotation]),
                row(tr!("Redo annotation"), vec![&RedoAnnotation]),
                row(
                    tr!("Delete selected annotation"),
                    vec![&DeleteSelectedAnnotation],
                ),
                row(tr!("Cancel editing"), vec![&CancelAnnotationEdit]),
            ],
        ),
        (
            tr!("Interface"),
            vec![
                row(tr!("Home"), vec![&Home]),
                row(tr!("Settings"), vec![&Preferences]),
                row(tr!("About"), vec![&ShowAbout]),
                row(tr!("Shortcuts"), vec![&ShowShortcuts]),
                row(tr!("Toggle navigation"), vec![&ToggleSidebar]),
                row(tr!("Toggle document sidebar"), vec![&ToggleDocumentPanel]),
                row(tr!("Toggle favorite"), vec![&Favorite]),
                row(tr!("Change theme"), vec![&CycleTheme]),
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
        Breeze::page_frame(
            "shortcuts-body",
            column()
                .w_full()
                .gap_4()
                .child(
                    div()
                        .text_size(px(16.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(tr!("Shortcuts")),
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
                        .child(tr!("Custom shortcuts are not supported yet.")),
                ),
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
