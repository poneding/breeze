use crate::*;
use gpui_kit::component::{
    ElementExt, Icon,
    button::ButtonCustomVariant,
    empty::{Empty, EmptyHeader, EmptyMedia, EmptyTitle},
};
use std::ops::Range;
const CATEGORY_WIDTH: f32 = 144.;
const POSITION_WIDTH: f32 = 100.;
const ACTIONS_WIDTH: f32 = 80.;
/// Global Home pages stay readable instead of stretching across a wide window.
pub(super) const PAGE_MAX_WIDTH: f32 = 720.;

impl Breeze {
    pub(super) fn home(&self, window: &Window, cx: &mut Context<Self>) -> Div {
        row()
            .flex_1()
            .min_h_0()
            .items_stretch()
            .child(self.library_sidebar(cx))
            .child(self.home_content(window, cx))
    }

    /// Library entries stay scrollable; global pages sit at the bottom edge.
    fn library_sidebar(&self, cx: &mut Context<Self>) -> Div {
        let p = self.p(cx);
        column()
            .w(px(168.))
            .flex_shrink_0()
            .bg(p.sidebar)
            .border_r_1()
            .border_color(p.line)
            .child(
                column()
                    .id("library-sidebar")
                    .test_support()
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .py_2()
                    .px_2()
                    .gap_1()
                    .children(
                        [
                            (IconName::LibraryBig, tr!("All documents"), 2),
                            (IconName::Clock3, tr!("Recent"), 0),
                            (IconName::Bookmark, tr!("Favorites"), 1),
                        ]
                        .into_iter()
                        .map(|(icon, label, index)| {
                            categories::left_button(("library-nav", index), icon, label)
                                .selected(
                                    self.home_page == HomePage::Library
                                        && self.library_filter == index,
                                )
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.home_page = HomePage::Library;
                                    this.library_filter = index;
                                    cx.notify()
                                }))
                        }),
                    )
                    .child(self.category_sidebar(cx)),
            )
            .child(
                column()
                    .id("home-footer")
                    .test_support()
                    .gap_1()
                    .px_2()
                    .py_2()
                    .border_t_1()
                    .border_color(p.line)
                    .child(self.home_page_button(
                        "home-about",
                        IconName::Info,
                        tr!("About"),
                        HomePage::About,
                        &ShowAbout,
                        cx,
                    ))
                    .child(self.home_page_button(
                        "home-shortcuts",
                        IconName::Keyboard,
                        tr!("Shortcuts"),
                        HomePage::Shortcuts,
                        &ShowShortcuts,
                        cx,
                    ))
                    .child(self.home_page_button(
                        "home-settings",
                        IconName::Settings,
                        tr!("Settings"),
                        HomePage::Settings,
                        &Preferences,
                        cx,
                    )),
            )
    }

    /// A sidebar entry for one global page. The tooltip shows the real binding.
    fn home_page_button(
        &self,
        id: &'static str,
        icon: IconName,
        label: &'static str,
        page: HomePage,
        action: &dyn Action,
        cx: &mut Context<Self>,
    ) -> Button {
        let action = action.boxed_clone();
        let click_action = action.boxed_clone();
        Button::new(id)
            .ghost()
            .small()
            .w_full()
            .selected(self.home_page == page)
            .accessibility_label(label)
            .tooltip_with_action(label, action.as_ref(), Some("Breeze"))
            .on_click(cx.listener(move |this, _, window, cx| {
                this.execute(click_action.as_ref(), window, cx);
            }))
            .child(
                row().w_full().gap_2().child(Icon::new(icon).small()).child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_ellipsis()
                        .text_left()
                        .child(label),
                ),
            )
    }

    fn home_content(&self, window: &Window, cx: &mut Context<Self>) -> impl IntoElement {
        column()
            .id("home-content")
            .test_support()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .px_5()
            .py_3()
            .gap_3()
            .child(match self.home_page {
                HomePage::Library => self.library_page(cx).into_any_element(),
                HomePage::About => self.about_page(cx).into_any_element(),
                HomePage::Shortcuts => self.shortcuts_page(window, cx).into_any_element(),
                HomePage::Settings => self.settings_page(cx).into_any_element(),
            })
    }

    /// A global page: full height, capped width, and centered when the window is wider.
    pub(super) fn page_frame(id: &'static str, content: impl IntoElement) -> impl IntoElement {
        column().flex_1().min_h_0().w_full().items_center().child(
            column()
                .id(id)
                .test_support()
                .w_full()
                .max_w(px(PAGE_MAX_WIDTH))
                .flex_1()
                .min_h_0()
                // Keep a little room so an active control border is never clipped.
                .pr_1()
                .overflow_y_scroll()
                .child(content),
        )
    }

    fn library_page(&self, cx: &mut Context<Self>) -> Div {
        let p = self.p(cx);
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
        column()
            .flex_1()
            .min_h_0()
            .gap_3()
            .child(
                row()
                    .gap_2()
                    .child(
                        div()
                            .text_size(px(16.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(match self.library_filter {
                                1 => tr!("Favorites").to_owned(),
                                2 => tr!("All documents").to_owned(),
                                n if n >= 3 => self.category_name(n - 3),
                                _ => tr!("Recent").to_owned(),
                            }),
                    )
                    .child(
                        div()
                            .text_size(px(12.))
                            .text_color(p.muted)
                            .child(tf!("{} documents", filtered.len())),
                    )
                    .child(div().flex_1())
                    .child(
                        div()
                            .w(px(240.))
                            .child(Input::new(&self.library_input).small()),
                    )
                    .children(
                        [
                            (false, IconName::List, tr!("List view")),
                            (true, IconName::LayoutGrid, tr!("Grid view")),
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
                            tr!("Open document"),
                            &Open,
                            cx,
                        ))
                    }),
            )
            .when(!self.library_grid && !filtered.is_empty(), |d| {
                d.child(
                    row()
                        .h(px(24.))
                        .px_2()
                        .border_b_1()
                        .border_color(p.line)
                        .text_size(px(11.))
                        .text_color(p.muted)
                        .gap_2()
                        .child(div().flex_1().min_w_0().child(tr!("Document")))
                        .child(
                            div()
                                .id("category-heading")
                                .test_support()
                                .w(px(CATEGORY_WIDTH))
                                .flex_shrink_0()
                                .px_2()
                                .child(tr!("Categories")),
                        )
                        .child(
                            div()
                                .id("position-heading")
                                .test_support()
                                .w(px(POSITION_WIDTH))
                                .flex_shrink_0()
                                .child(tr!("Location")),
                        )
                        .child(div().w(px(ACTIONS_WIDTH)).flex_shrink_0()),
                )
            })
            .child(self.library_documents(filtered.iter().map(|(index, _)| *index).collect(), cx))
            .when(!self.message.is_empty(), |d| {
                d.child(div().text_color(rgb(0xba6544)).child(self.message.clone()))
            })
    }

    fn library_row(&self, index: usize, cx: &mut Context<Self>) -> Button {
        let p = self.p(cx);
        let hover = style::accent_hover(cx);
        let b = &self.books[index];
        Button::new(("library-open", index))
            .custom(ButtonCustomVariant::new(cx).hover(hover).active(hover))
            .group("library-row")
            .group_hover("library-row", move |s| s.bg(hover))
            .small()
            .w_full()
            .accessibility_label(tf!("Read document: {}", b.title))
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
                                        div()
                                            .text_size(px(11.))
                                            .text_color(p.muted)
                                            .child(b.format_label()),
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
                                b.position_label()
                            } else {
                                tr!("Not opened").into()
                            }),
                    )
                    .child(self.library_actions(index, cx)),
            )
    }
    fn library_actions(&self, index: usize, cx: &mut Context<Self>) -> Div {
        let p = self.p(cx);
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
                    .icon(document_icons::bookmark_icon(b.favorite))
                    .accessibility_label(if b.favorite {
                        tr!("Remove favorite")
                    } else {
                        tr!("Favorites")
                    })
                    .tooltip(tr!("Toggle favorite"))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        this.books[index].favorite = !this.books[index].favorite;
                        cx.notify()
                    })),
            )
            .when(matches!(&b.source, Source::File(_)), |d| {
                d.child(
                    Button::new(("reveal-item", index))
                        .custom(ButtonCustomVariant::new(cx).hover(p.line).active(p.line))
                        .small()
                        .icon(IconName::FolderOpen)
                        .accessibility_label(tr!(reveal::label()))
                        .tooltip(tr!(reveal::label()))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            cx.stop_propagation();
                            this.reveal_book(index, cx)
                        })),
                )
            })
            .child(
                Button::new(("forget", index))
                    .custom(ButtonCustomVariant::new(cx).hover(p.line).active(p.line))
                    .small()
                    .icon(IconName::X)
                    .accessibility_label(tr!("Remove from library"))
                    .tooltip(tr!("Remove entry; keep file"))
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
    fn library_cover(&self, index: usize, grid: bool, cx: &App) -> Div {
        let p = self.p(cx);
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
                Some(Err(_)) | None => {
                    d.child(document_icons::file_icon(&self.books[index].source, cx))
                }
            })
    }
    fn library_card(&self, index: usize, cx: &mut Context<Self>) -> Button {
        let p = self.p(cx);
        let hover = style::accent_hover(cx);
        let book = &self.books[index];
        Button::new(("library-open", index))
            .custom(ButtonCustomVariant::new(cx).hover(hover).active(hover))
            .group("library-row")
            .group_hover("library-row", move |s| s.bg(hover))
            .small()
            .w_full()
            .h(px(264.))
            .p_2()
            .accessibility_label(tf!("Read document: {}", book.title))
            .tooltip(self.cover_description(index))
            .on_click(cx.listener(move |this, _, _, cx| this.open_book(index, cx)))
            .child(
                column()
                    .w_full()
                    .min_w_0()
                    .items_center()
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
                            .id(("grid-title", index))
                            .test_support()
                            .text_center()
                            .text_ellipsis()
                            .child(book.title.clone()),
                    )
                    .child(
                        div()
                            .id(("grid-position", index))
                            .test_support()
                            .w_full()
                            .text_size(px(11.))
                            .text_color(p.muted)
                            .text_center()
                            .child(format!(
                                "{} · {}",
                                book.format_label(),
                                if book.last_opened > 0 {
                                    book.position_label()
                                } else {
                                    tr!("Not opened").into()
                                }
                            )),
                    )
                    .child(self.category_assignment(index, cx))
                    .child(
                        row()
                            .w_full()
                            .justify_center()
                            .invisible()
                            .group_hover("library-row", |s| s.visible())
                            .child(self.library_actions(index, cx)),
                    ),
            )
    }
    fn library_documents(&self, indices: Vec<usize>, cx: &mut Context<Self>) -> Div {
        if indices.is_empty() {
            let p = self.p(cx);
            let view = cx.entity();
            return div()
                .flex_1()
                .min_h_0()
                .w_full()
                .flex()
                .child(
                    Empty::new().header(
                        EmptyHeader::new()
                            .media(
                                EmptyMedia::new().child(
                                    Icon::new(IconName::LibraryBig)
                                        .size(px(24.))
                                        .text_color(p.muted),
                                ),
                            )
                            .title(
                                EmptyTitle::new()
                                    .text_size(px(13.))
                                    .font_weight(FontWeight::NORMAL)
                                    .text_color(p.muted)
                                    .child(
                                        div()
                                            .id("library-empty")
                                            .test_support()
                                            .child(tr!("No documents")),
                                    ),
                            ),
                    ),
                )
                .on_prepaint(move |_, _, cx| {
                    view.update(cx, |this, cx| this.defer_covers(Vec::new(), cx))
                });
        }
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
