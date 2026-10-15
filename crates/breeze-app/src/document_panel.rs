use crate::*;
use breeze_core::annotations::{Annotation, Kind};
use gpui_kit::component::{
    Icon, Theme,
    menu::{DropdownMenu, PopupMenuItem},
    switch::Switch,
    tooltip::Tooltip,
};

const WIDTH: f32 = 256.;
const KINDS: [Kind; 11] = [
    Kind::Highlight,
    Kind::Area,
    Kind::Underline,
    Kind::Strikeout,
    Kind::Ink,
    Kind::Note,
    Kind::Ellipse,
    Kind::Rectangle,
    Kind::Line,
    Kind::Squiggle,
    Kind::Arrow,
];

#[derive(Default)]
pub(super) struct DocumentPanel {
    pub open: bool,
    tab: usize,
    kind: Option<Kind>,
    current_page_only: bool,
    scroll: UniformListScrollHandle,
}

impl DocumentPanel {
    pub fn width(&self) -> f32 {
        if self.open { WIDTH } else { 0. }
    }

    pub fn reset_document(&mut self) {
        self.tab = 0;
        self.kind = None;
        self.current_page_only = false;
        self.scroll = UniformListScrollHandle::new();
    }
    pub fn show_annotations(&mut self) {
        self.open = true;
        self.tab = 1;
    }
}

fn kind_presentation(kind: Kind) -> (IconName, &'static str) {
    match kind {
        Kind::Highlight => (IconName::Highlighter, tr!("Text highlight")),
        Kind::Area => (IconName::Scan, tr!("Area highlight")),
        Kind::Underline => (IconName::Underline, tr!("Underline")),
        Kind::Strikeout => (IconName::Strikethrough, tr!("Strikethrough")),
        Kind::Ink => (IconName::Pencil, tr!("Pen")),
        Kind::Note => (IconName::Type, tr!("Text note")),
        Kind::Ellipse => (IconName::Circle, tr!("Ellipse")),
        Kind::Rectangle => (IconName::Square, tr!("Rectangle")),
        Kind::Line => (IconName::Minus, tr!("Line")),
        Kind::Squiggle => (IconName::LineSquiggle, tr!("Wavy line")),
        Kind::Arrow => (IconName::ArrowUpRight, tr!("Arrow")),
    }
}

fn filtered_indices(items: &[Annotation], kind: Option<Kind>, page: Option<usize>) -> Vec<usize> {
    let mut indices: Vec<_> = items
        .iter()
        .enumerate()
        .filter(|(_, item)| kind.is_none_or(|kind| item.kind == kind))
        .filter(|(_, item)| page.is_none_or(|page| item.page == page))
        .map(|(index, _)| index)
        .collect();
    indices.sort_by_key(|&index| items[index].page);
    indices
}

impl Breeze {
    pub(super) fn document_sidebar(&self, cx: &mut Context<Self>) -> AnyElement {
        if !self.books[self.active].is_pdf() {
            return self.reflow_sidebar(cx);
        }
        let theme = Theme::global(cx);
        column()
            .id("document-panel")
            .test_support()
            .w(px(WIDTH))
            .h_full()
            .min_h_0()
            .flex_shrink_0()
            .border_l_1()
            .border_color(theme.border)
            .bg(theme.sidebar)
            .child(
                row()
                    .id("document-panel-header")
                    .test_support()
                    .h(px(style::TOOLBAR_HEIGHT))
                    .flex_shrink_0()
                    .px_2()
                    .border_b_1()
                    .border_color(theme.border)
                    .when(self.search_open, |header| header.child(self.search_bar(cx)))
                    .when(!self.search_open, |header| {
                        header
                            .child(
                                Button::new("document-panel-information")
                                    .ghost()
                                    .small()
                                    .icon(IconName::Info)
                                    .selected(!self.search_open && self.document_panel.tab == 0)
                                    .accessibility_label(tr!("Document information"))
                                    .tooltip(tr!("Document information"))
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.cancel_search();
                                        this.search_open = false;
                                        this.search_restore_document_panel_open = None;
                                        this.document_panel.open = true;
                                        this.document_panel.tab = 0;
                                        cx.notify();
                                    })),
                            )
                            .child(div().w(px(4.)).flex_shrink_0())
                            .child(
                                Button::new("document-panel-annotations")
                                    .ghost()
                                    .small()
                                    .icon(IconName::Highlighter)
                                    .selected(!self.search_open && self.document_panel.tab == 1)
                                    .accessibility_label(tr!("Document annotations"))
                                    .tooltip(tr!("Document annotations"))
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.cancel_search();
                                        this.search_open = false;
                                        this.search_restore_document_panel_open = None;
                                        this.document_panel.open = true;
                                        this.document_panel.tab = 1;
                                        cx.notify();
                                    })),
                            )
                            // Push the panel toggle to the header's right edge.
                            .child(div().flex_1())
                    })
                    .child(
                        self.command(
                            "document-panel-toggle",
                            IconName::PanelRight,
                            tr!("Document sidebar"),
                            &ToggleDocumentPanel,
                            cx,
                        )
                        .selected(true),
                    ),
            )
            .child(if self.search_open {
                self.search_results(cx).into_any_element()
            } else if self.document_panel.tab == 0 {
                self.document_information(cx).into_any_element()
            } else {
                self.document_annotations(cx).into_any_element()
            })
            .into_any_element()
    }

    fn document_information(&self, cx: &mut Context<Self>) -> AnyElement {
        let book = &self.books[self.active];
        let muted = Theme::global(cx).muted_foreground;
        let path = match &book.source {
            Source::Sample => tr!("Bundled sample document").to_owned(),
            Source::File(path) => path.display().to_string(),
        };
        let categories = book
            .categories
            .iter()
            .filter_map(|id| {
                self.categories
                    .iter()
                    .find(|category| category.id == *id)
                    .map(|category| category.name.as_str())
            })
            .collect::<Vec<_>>();
        column()
            .id("document-information")
            .test_support()
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .p_3()
            .gap_3()
            .child(info_field(tr!("Name"), book.title.clone(), muted))
            .child(info_field(tr!("Format"), "PDF".into(), muted))
            .child(info_field(
                tr!("Pages"),
                if self.reading.sizes.is_empty() {
                    tr!("Reading…").into()
                } else {
                    book.state.navigation.total().to_string()
                },
                muted,
            ))
            .when_some(
                self.reading.sizes.get(book.state.navigation.page()),
                |content, size| {
                    content.child(info_field(
                        tr!("Current page size"),
                        format!("{:.0} × {:.0} pt", size.0, size.1),
                        muted,
                    ))
                },
            )
            .child(info_field(
                tr!("Categories"),
                if categories.is_empty() {
                    tr!("Unassigned").into()
                } else {
                    categories.join("、")
                },
                muted,
            ))
            .child(info_field(
                tr!("Annotations"),
                tf!("{} items", book.annotations.history.items.len()),
                muted,
            ))
            .child(info_field(tr!("File location"), path, muted))
            .when_some(
                match &book.source {
                    Source::File(path) => Some(path.clone()),
                    _ => None,
                },
                |content, path| {
                    content.child(
                        Button::new("reveal-document")
                            .ghost()
                            .small()
                            .icon(IconName::FolderOpen)
                            .label(tr!("Show in folder"))
                            .on_click(move |_, _, cx| cx.reveal_path(&path)),
                    )
                },
            )
            .when_some(book.annotations.path.as_ref(), |content, path| {
                content.child(info_field(
                    tr!("Annotation location"),
                    path.display().to_string(),
                    muted,
                ))
            })
            .child(div().text_size(px(11.)).text_color(muted).child(tr!(
                "Annotations are saved separately. The source PDF is not modified."
            )))
            .when_some(
                self.preview
                    .as_ref()
                    .and_then(|preview| preview.text_warning.clone()),
                |content, warning| {
                    content.child(div().text_size(px(11.)).text_color(muted).child(warning))
                },
            )
            .into_any_element()
    }

    fn document_annotations(&self, cx: &mut Context<Self>) -> Div {
        let data = &self.books[self.active].annotations;
        let muted = Theme::global(cx).muted_foreground;
        let border = Theme::global(cx).border;
        let indices = filtered_indices(
            &data.history.items,
            self.document_panel.kind,
            self.document_panel
                .current_page_only
                .then_some(self.books[self.active].state.navigation.page()),
        );
        let count = indices.len();
        let total = data.history.items.len();
        column()
            .flex_1()
            .h_full()
            .min_h_0()
            .child(
                column()
                    .flex_shrink_0()
                    .p_2()
                    .gap_2()
                    .child(
                        row().gap_2().child(self.annotation_kind_filter(cx)).child(
                            div()
                                .text_size(px(11.))
                                .text_color(muted)
                                .child(format!("{count} / {total}")),
                        ),
                    )
                    .child(
                        row()
                            .gap_2()
                            .child(
                                Switch::new("annotations-current-page")
                                    .small()
                                    .checked(self.document_panel.current_page_only)
                                    .on_change(cx.listener(|this, value, _, cx| {
                                        this.document_panel.current_page_only = *value;
                                        this.document_panel
                                            .scroll
                                            .scroll_to_item(0, ScrollStrategy::Top);
                                        cx.notify();
                                    })),
                            )
                            .child(
                                div()
                                    .text_size(px(11.))
                                    .text_color(muted)
                                    .child(tr!("Current page only")),
                            ),
                    ),
            )
            .child(if indices.is_empty() {
                div()
                    .id("annotations-empty")
                    .test_support()
                    .flex_1()
                    .min_h_0()
                    .p_3()
                    .text_size(px(12.))
                    .text_color(muted)
                    .child(if !data.ready && data.error.is_none() {
                        tr!("Loading annotations…")
                    } else if total == 0 {
                        tr!("No annotations")
                    } else {
                        tr!("No matching annotations")
                    })
                    .into_any_element()
            } else {
                uniform_list(
                    "document-annotation-list",
                    indices.len(),
                    cx.processor(move |this, range: std::ops::Range<usize>, _, cx| {
                        indices[range]
                            .iter()
                            .filter_map(|&index| this.annotation_list_item(index, cx))
                            .collect::<Vec<_>>()
                    }),
                )
                .track_scroll(&self.document_panel.scroll)
                .w_full()
                .flex_1()
                .min_h_0()
                .into_any_element()
            })
            .child(
                column()
                    .p_2()
                    .gap_2()
                    .flex_shrink_0()
                    .border_t_1()
                    .border_color(border)
                    .child(
                        row()
                            .gap_1()
                            .child(
                                self.command(
                                    "undo",
                                    IconName::Undo2,
                                    tr!("Undo annotation"),
                                    &UndoAnnotation,
                                    cx,
                                )
                                .disabled(!data.history.can_undo()),
                            )
                            .child(
                                self.command(
                                    "redo",
                                    IconName::Redo2,
                                    tr!("Redo annotation"),
                                    &RedoAnnotation,
                                    cx,
                                )
                                .disabled(!data.history.can_redo()),
                            )
                            .child(
                                self.command(
                                    "save-sidecar",
                                    IconName::Save,
                                    tr!("Save annotations"),
                                    &SaveAnnotations,
                                    cx,
                                )
                                .disabled(!data.ready || data.saving || data.error.is_some()),
                            )
                            .child(self.command(
                                "save-sidecar-as",
                                IconName::FolderOpen,
                                tr!("Save annotations as"),
                                &SaveAnnotationsAs,
                                cx,
                            )),
                    )
                    .when_some(
                        if data.saving {
                            Some(tr!("Saving…"))
                        } else if data.dirty() {
                            Some(tr!("Unsaved annotations"))
                        } else if data.ready {
                            // A loaded, saved sidecar needs no announcement.
                            None
                        } else {
                            Some(tr!("Annotations are not ready"))
                        },
                        |content, status| {
                            content.child(
                                div()
                                    .id("annotation-status")
                                    .test_support()
                                    .text_size(px(11.))
                                    .text_color(muted)
                                    .child(status),
                            )
                        },
                    )
                    .when_some(data.error.as_ref(), |content, error| {
                        content.child(
                            div()
                                .text_size(px(11.))
                                .text_color(rgb(0xdc2626))
                                .child(error.clone()),
                        )
                    })
                    .child(self.annotation_recovery_controls(cx))
                    .child(self.export_controls(cx)),
            )
    }

    fn annotation_kind_filter(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let view = cx.entity();
        let selected = self.document_panel.kind;
        let source = self.books[self.active].source.clone();
        Button::new("annotation-kind-filter")
            .small()
            .icon(IconName::ListFilter)
            .label(
                selected
                    .map(|kind| kind_presentation(kind).1)
                    .unwrap_or(tr!("All types")),
            )
            .accessibility_label(tr!("Filter by annotation type"))
            .dropdown_menu(move |mut menu, window, _| {
                for kind in std::iter::once(None).chain(KINDS.into_iter().map(Some)) {
                    let source = source.clone();
                    let (icon, label) = kind
                        .map(kind_presentation)
                        .unwrap_or((IconName::List, tr!("All types")));
                    menu = menu.item(
                        PopupMenuItem::new(label)
                            .icon(icon)
                            .checked(kind == selected)
                            .on_click(window.listener_for(&view, move |this, _, _, cx| {
                                if this.books[this.active].source != source {
                                    return;
                                }
                                this.document_panel.kind = kind;
                                this.document_panel
                                    .scroll
                                    .scroll_to_item(0, ScrollStrategy::Top);
                                cx.notify();
                            })),
                    );
                }
                menu
            })
    }

    fn annotation_list_item(&self, index: usize, cx: &mut Context<Self>) -> Option<Div> {
        let book = &self.books[self.active];
        let annotation = book.annotations.history.items.get(index)?;
        let source = book.source.clone();
        let expected = annotation.clone();
        let remove_source = source.clone();
        let remove_expected = expected.clone();
        let (icon, label) = kind_presentation(annotation.kind);
        let disabled = !book.annotations.ready || book.annotations.source_conflict;
        let muted = Theme::global(cx).muted_foreground;
        let color: Hsla = gpui_kit::Rgba {
            r: annotation.color[0],
            g: annotation.color[1],
            b: annotation.color[2],
            a: 1.,
        }
        .into();
        Some(
            row()
                .w_full()
                .min_w_0()
                .h(px(48.))
                .px_2()
                .gap_1()
                .child(
                    Button::new(("annotation-item", index))
                        .ghost()
                        .small()
                        .h_full()
                        .flex_1()
                        .min_w_0()
                        .disabled(disabled)
                        .accessibility_label(tf!("{} · Page {}", label, annotation.page + 1))
                        .tooltip(if annotation.text.is_empty() {
                            tf!("{} · Page {}", label, annotation.page + 1)
                        } else {
                            annotation.text.clone()
                        })
                        .child(
                            row()
                                .w_full()
                                .gap_2()
                                .child(Icon::new(icon).small().text_color(color))
                                .child(
                                    column()
                                        .flex_1()
                                        .min_w_0()
                                        .items_start()
                                        .child(div().w_full().text_left().text_ellipsis().child(
                                            if annotation.text.is_empty() {
                                                label.to_owned()
                                            } else {
                                                annotation.text.clone()
                                            },
                                        ))
                                        .child(div().text_size(px(11.)).text_color(muted).child(
                                            tf!("Page {} · {}", annotation.page + 1, label),
                                        )),
                                ),
                        )
                        .on_click(cx.listener(move |this, _, _, cx| {
                            let book = &mut this.books[this.active];
                            if !book.annotations.ready
                                || book.annotations.source_conflict
                                || book.source != source
                                || book.annotations.history.items.get(index) != Some(&expected)
                            {
                                return;
                            }
                            this.go_to_annotation_page(expected.page, cx);
                        })),
                )
                .child(
                    Button::new(("remove-annotation", index))
                        .ghost()
                        .small()
                        .icon(IconName::Trash)
                        .disabled(disabled)
                        .accessibility_label(tr!("Delete this annotation"))
                        .tooltip(tr!("Delete this annotation"))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if !this.annotation_navigation_allowed(cx) {
                                return;
                            }
                            let book = &mut this.books[this.active];
                            if !book.annotations.ready
                                || book.annotations.source_conflict
                                || book.source != remove_source
                                || book.annotations.history.items.get(index)
                                    != Some(&remove_expected)
                            {
                                return;
                            }
                            let mut items = book.annotations.history.items.clone();
                            items.remove(index);
                            book.annotations.history.replace(items);
                            cx.notify();
                        })),
                ),
        )
    }
}

fn info_field(label: &'static str, value: String, muted: Hsla) -> Div {
    column()
        .w_full()
        .min_w_0()
        .gap_1()
        .child(div().text_size(px(11.)).text_color(muted).child(label))
        .child(
            div()
                .id(SharedString::from(label))
                .w_full()
                .min_w_0()
                .text_size(px(12.))
                .text_ellipsis()
                .child(value.clone())
                .tooltip(move |window, cx| Tooltip::new(value.clone()).build(window, cx)),
        )
}

#[cfg(test)]
mod tests;
