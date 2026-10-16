//! A single optional peer reader. The outer view owns library metadata and the
//! state store; a moved Book has exactly one owner for annotations and drafts.
use crate::*;
use gpui_kit::component::{h_resizable, resizable_panel};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum Side {
    Left,
    #[default]
    Right,
}

#[derive(Default)]
pub(super) struct Workspace {
    pub parent: Option<WeakEntity<Breeze>>,
    pub secondary: Option<Entity<Breeze>>,
    /// Sources whose live Book belongs to the peer, not the library entry.
    pub secondary_sources: Vec<Source>,
    pub side: Side,
    pub order: Vec<Source>,
    split: Option<Entity<ResizableState>>,
    observation: Option<Subscription>,
    resize_observation: Option<Subscription>,
}

#[derive(Clone)]
pub(super) struct TabDrag {
    pub owner: WeakEntity<Breeze>,
    pub source: Source,
    pub title: String,
}
impl Render for TabDrag {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .px_3()
            .py_1()
            .rounded_md()
            .shadow_md()
            .bg(Theme::global(cx).background)
            .text_color(Theme::global(cx).foreground)
            .max_w(px(228.))
            .text_ellipsis()
            .child(self.title.clone())
    }
}

#[derive(Clone, Copy)]
pub(super) enum Destination {
    Primary,
    Secondary,
    Split(Side),
}

impl Book {
    /// A library record for a session owned by the peer. Never clone histories,
    /// in-flight save identities, or editable drafts into the outer reader.
    fn library_record(&self) -> Self {
        Self {
            source: self.source.clone(),
            title: self.title.clone(),
            favorite: self.favorite,
            categories: self.categories.clone(),
            state: self.state.clone(),
            reflow: self.reflow.clone(),
            opened: self.opened,
            last_opened: self.last_opened,
            annotations: DocumentAnnotations {
                path: self.annotations.path.clone(),
                ..Default::default()
            },
            markdown_edit: None,
            markdown_editing: false,
        }
    }
}

impl Breeze {
    pub(super) fn focus_pane(&self, window: &mut Window, cx: &mut Context<Self>) {
        let peer_focused = self
            .workspace
            .secondary
            .as_ref()
            .is_some_and(|peer| peer.read(cx).focus.contains_focused(window, cx));
        if peer_focused || !self.focus.contains_focused(window, cx) {
            window.focus(&self.focus, cx);
        }
    }
    pub(super) fn toggle_favorite(&mut self, index: usize, cx: &mut Context<Self>) {
        if let Some((peer, remote)) = self.secondary_book(&self.books[index].source, cx) {
            peer.update(cx, |peer, cx| peer.toggle_favorite(remote, cx));
            self.sync_secondary(cx);
        } else {
            self.books[index].favorite = !self.books[index].favorite;
        }
        cx.notify();
    }
    pub(super) fn is_split(&self) -> bool {
        self.workspace.parent.is_some() || self.workspace.secondary.is_some()
    }

    pub(super) fn pane_tabs(&self) -> Vec<usize> {
        let mut tabs: Vec<_> = self
            .books
            .iter()
            .enumerate()
            .filter(|(_, book)| {
                book.opened && !self.workspace.secondary_sources.contains(&book.source)
            })
            .map(|(index, _)| index)
            .collect();
        tabs.sort_by_key(|index| {
            self.workspace
                .order
                .iter()
                .position(|source| *source == self.books[*index].source)
                .unwrap_or(usize::MAX)
        });
        tabs
    }

    pub(super) fn secondary_book(
        &self,
        source: &Source,
        cx: &App,
    ) -> Option<(Entity<Breeze>, usize)> {
        let peer = self.workspace.secondary.as_ref()?;
        let index = peer
            .read(cx)
            .books
            .iter()
            .position(|book| book.source == *source && book.opened)?;
        Some((peer.clone(), index))
    }

    pub(super) fn queue_tab_drop(
        &self,
        drag: &TabDrag,
        destination: Destination,
        before: Option<Source>,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        let root = self
            .workspace
            .parent
            .clone()
            .unwrap_or_else(|| cx.weak_entity());
        let drag = drag.clone();
        // A peer callback must finish before its owner transfers the Book.
        let handle = window.window_handle();
        cx.defer(move |cx| {
            let _ = handle.update(cx, |_, window, cx| {
                let _ = root.update(cx, |root, cx| {
                    root.drop_tab(&drag, destination, before, window, cx)
                });
            });
        });
    }

    fn new_secondary(&mut self, side: Side, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if self.workspace.secondary.is_some() {
            return true;
        }
        if self.workspace.parent.is_some() {
            return false;
        }
        let worker = match PdfWorker::new() {
            Ok(worker) => worker,
            Err(error) => {
                self.message = errors::error_text(&error);
                cx.notify();
                return false;
            }
        };
        let parent = cx.weak_entity();
        let peer = cx.new(|cx| {
            let mut peer = Breeze::new(worker, window, cx);
            peer.workspace.parent = Some(parent);
            peer.language = self.language;
            peer.theme_preference = self.theme_preference.clone();
            peer.sidebar = self.sidebar;
            peer.navigation_width = self.navigation_width;
            peer.document_panel.open = self.document_panel.open;
            peer.reading_preferences = self.reading_preferences;
            peer.data_dir = self.data_dir.clone();
            peer
        });
        i18n::set_language(i18n::LanguagePreference::from_saved(self.language).resolve());
        self.workspace.observation = Some(cx.observe(&peer, |this, _, cx| {
            this.sync_secondary(cx);
            this.normalize_split(cx);
            cx.notify();
        }));
        let split = cx.new(|_| ResizableState::default());
        self.workspace.resize_observation = Some(cx.observe(&split, |_, _, cx| cx.notify()));
        self.workspace.split = Some(split);
        self.workspace.side = side;
        self.workspace.secondary = Some(peer);
        true
    }

    /// Keep the library and its one background state writer up to date. Sessions
    /// return intact when closed; moved-out slots stay in place so pending
    /// callbacks cannot accidentally address a different document by index.
    pub(super) fn sync_secondary(&mut self, cx: &mut Context<Self>) {
        let Some(peer) = self.workspace.secondary.clone() else {
            return;
        };
        peer.update(cx, |peer, _| {
            for book in &mut peer.books {
                if !book.opened && !self.workspace.secondary_sources.contains(&book.source) {
                    continue;
                }
                let record = book.library_record();
                let replacement = if book.opened {
                    record
                } else {
                    std::mem::replace(book, record)
                };
                if replacement.opened
                    && !self
                        .workspace
                        .secondary_sources
                        .contains(&replacement.source)
                {
                    self.workspace
                        .secondary_sources
                        .push(replacement.source.clone());
                } else if !replacement.opened {
                    self.workspace
                        .secondary_sources
                        .retain(|source| *source != replacement.source);
                }
                if let Some(index) = self
                    .books
                    .iter()
                    .position(|b| b.source == replacement.source)
                {
                    self.books[index] = replacement;
                } else {
                    self.books.push(replacement);
                }
            }
        });
    }

    fn transferable(&mut self, cx: &mut Context<Self>) -> bool {
        if self.export_busy || self.books.iter().any(|book| book.annotations.saving) {
            self.message = tr!("Wait for saving or export before moving a tab").into();
            cx.notify();
            return false;
        }
        self.annotation_navigation_allowed(cx)
    }

    pub(super) fn drop_tab(
        &mut self,
        drag: &TabDrag,
        destination: Destination,
        before: Option<Source>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let from_secondary = self
            .workspace
            .secondary
            .as_ref()
            .is_some_and(|peer| peer.entity_id() == drag.owner.entity_id());
        if !from_secondary && drag.owner.entity_id() != cx.entity_id() {
            return;
        }
        let owns_source = if from_secondary {
            self.workspace.secondary.as_ref().is_some_and(|peer| {
                peer.read(cx)
                    .books
                    .iter()
                    .any(|book| book.opened && book.source == drag.source)
            })
        } else {
            !self.workspace.secondary_sources.contains(&drag.source)
                && self.books.iter().any(|book| book.source == drag.source)
        };
        if !owns_source {
            return;
        }
        let target_secondary = match destination {
            Destination::Secondary => true,
            Destination::Primary => false,
            Destination::Split(side) => {
                if self.workspace.secondary.is_some() || self.pane_tabs().len() < 2 {
                    return;
                }
                if !self.transferable(cx) || !self.new_secondary(side, window, cx) {
                    return;
                }
                true
            }
        };
        if from_secondary == target_secondary {
            if from_secondary {
                if let Some(peer) = self.workspace.secondary.clone() {
                    peer.update(cx, |peer, cx| peer.reorder_tab(&drag.source, before, cx));
                }
            } else {
                self.reorder_tab(&drag.source, before, cx);
            }
            return;
        }
        let Some(peer) = self.workspace.secondary.clone() else {
            return;
        };
        if !self.transferable(cx) || !peer.update(cx, |peer, cx| peer.transferable(cx)) {
            return;
        }
        if target_secondary {
            let Some(index) = self
                .books
                .iter()
                .position(|book| book.source == drag.source)
            else {
                return;
            };
            let record = self.books[index].library_record();
            let mut book = std::mem::replace(&mut self.books[index], record);
            book.opened = true;
            self.workspace.secondary_sources.push(book.source.clone());
            peer.update(cx, |peer, cx| {
                let source = book.source.clone();
                let index = peer.receive_book(book);
                peer.open_book(index, cx);
                peer.reorder_tab(&source, before, cx);
                window.focus(&peer.focus, cx);
            });
            if self.active == index
                && let Some(next) = self.pane_tabs().first().copied()
            {
                self.open_book(next, cx);
            }
        } else {
            let returned = peer.update(cx, |peer, cx| {
                let index = peer
                    .books
                    .iter()
                    .position(|book| book.source == drag.source && book.opened)?;
                let mut record = peer.books[index].library_record();
                record.opened = false;
                let book = std::mem::replace(&mut peer.books[index], record);
                if peer.active == index {
                    if let Some(next) = peer.pane_tabs().first().copied() {
                        peer.open_book(next, cx);
                    } else {
                        peer.show_home(cx);
                    }
                }
                Some(book)
            });
            let Some(book) = returned else {
                return;
            };
            self.workspace
                .secondary_sources
                .retain(|source| *source != book.source);
            let index = self.receive_book(book);
            self.open_book(index, cx);
            self.reorder_tab(&drag.source, before, cx);
            window.focus(&self.focus, cx);
        }
        self.normalize_split(cx);
        cx.notify();
    }

    fn receive_book(&mut self, mut book: Book) -> usize {
        self.pending_markdown_discard = None;
        // An inspection still running in the old view cannot deliver into this
        // owner. Reissue it here instead of retaining a permanently unready state.
        if !book.annotations.ready
            && !book.annotations.dirty()
            && !book.annotations.saving
            && !book.annotations.source_conflict
        {
            book.annotations.fingerprint.clear();
        }
        if let Some(index) = self
            .books
            .iter()
            .position(|entry| entry.source == book.source)
        {
            self.books[index] = book;
            index
        } else {
            self.books.push(book);
            self.books.len() - 1
        }
    }

    fn reorder_tab(&mut self, source: &Source, before: Option<Source>, cx: &mut Context<Self>) {
        if before.as_ref() == Some(source) {
            return;
        }
        let mut order: Vec<_> = self
            .pane_tabs()
            .into_iter()
            .map(|i| self.books[i].source.clone())
            .filter(|entry| entry != source)
            .collect();
        let index = before
            .and_then(|before| order.iter().position(|entry| *entry == before))
            .unwrap_or(order.len());
        order.insert(index, source.clone());
        self.workspace.order = order;
        cx.notify();
    }

    pub(super) fn normalize_split(&mut self, cx: &mut Context<Self>) {
        let Some(peer) = self.workspace.secondary.clone() else {
            return;
        };
        let peer_empty = peer.read(cx).pane_tabs().is_empty();
        if !peer_empty && !self.pane_tabs().is_empty() {
            return;
        }
        // A pending native text editor stays mounted until it can be committed.
        if !peer_empty {
            let peer_state = peer.read(cx);
            if peer_state.annotation_editing.pending_text()
                || peer_state.export_busy
                || peer_state.books.iter().any(|book| book.annotations.saving)
            {
                return;
            }
            peer.update(cx, |peer, cx| {
                peer.annotation_navigation_allowed(cx);
            });
        }
        let active = peer
            .read(cx)
            .books
            .get(peer.read(cx).active)
            .map(|book| book.source.clone());
        peer.update(cx, |peer, cx| {
            peer.page_task = None;
            peer.reading.release_images(cx);
            peer.raster.clear(cx);
            peer.reflow.clear(cx);
            peer.cancel_search();
            for book in &mut peer.books {
                if !self.workspace.secondary_sources.contains(&book.source) {
                    continue;
                }
                let mut record = book.library_record();
                record.opened = false;
                let book = std::mem::replace(book, record);
                self.receive_book(book);
            }
        });
        self.workspace.secondary_sources.clear();
        self.workspace.secondary = None;
        self.workspace.observation = None;
        self.workspace.resize_observation = None;
        self.workspace.split = None;
        if !peer_empty
            && let Some(index) =
                active.and_then(|source| self.books.iter().position(|b| b.source == source))
        {
            self.open_book(index, cx);
        }
        let focus = self.focus.clone();
        let handle = self.window_handle;
        cx.defer(move |cx| {
            let _ = handle.update(cx, |_, window, cx| window.focus(&focus, cx));
        });
        cx.notify();
    }

    pub(super) fn forward_global(
        &self,
        action: &dyn Action,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(parent) = self.workspace.parent.clone() else {
            return false;
        };
        if !(action.as_any().is::<Home>()
            || action.as_any().is::<Preferences>()
            || action.as_any().is::<ShowAbout>()
            || action.as_any().is::<ShowShortcuts>()
            || action.as_any().is::<CycleTheme>()
            || action.as_any().is::<Quit>())
        {
            return false;
        }
        let action = action.boxed_clone();
        let handle = window.window_handle();
        cx.defer(move |cx| {
            let _ = handle.update(cx, |_, window, cx| {
                let _ = parent.update(cx, |parent, cx| {
                    window.focus(&parent.focus, cx);
                    if action.as_any().is::<Quit>() {
                        if parent.request_close(window, cx) {
                            window.remove_window();
                        }
                    } else {
                        parent.execute(action.as_ref(), window, cx);
                    }
                });
            });
        });
        true
    }

    pub(super) fn pane_surface(&self, body: AnyElement, cx: &mut Context<Self>) -> AnyElement {
        let secondary = self.workspace.parent.is_some();
        let destination = if secondary {
            Destination::Secondary
        } else {
            Destination::Primary
        };
        let can_split = !self.is_split() && self.pane_tabs().len() >= 2;
        let p = self.p(cx);
        let mut surface = column()
            .id(if secondary {
                "secondary-pane"
            } else {
                "primary-pane"
            })
            .test_support()
            .relative()
            .size_full()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .overflow_x_scroll()
            .capture_any_mouse_down(cx.listener(|this, _, window, cx| {
                this.focus_pane(window, cx);
            }))
            .on_drop(cx.listener(move |this, drag: &TabDrag, window, cx| {
                // A quick native drag can release before the preview's next
                // frame. Resolve its destination from the actual pointer too.
                let destination = if !this.is_split() && this.pane_tabs().len() >= 2 {
                    Destination::Split(
                        if window.mouse_position().x < window.viewport_size().width / 2. {
                            Side::Left
                        } else {
                            Side::Right
                        },
                    )
                } else {
                    destination
                };
                this.queue_tab_drop(drag, destination, None, window, cx);
            }))
            .child(body);
        if can_split && cx.has_active_drag() {
            surface = surface.child(row().absolute().inset_0().items_stretch().children(
                [Side::Left, Side::Right].into_iter().map(|side| {
                    div()
                        .id(if side == Side::Left {
                            "split-drop-left"
                        } else {
                            "split-drop-right"
                        })
                        .test_support()
                        .flex_1()
                        .h_full()
                        .border_2()
                        .border_color(p.line)
                        .bg(p.panel.opacity(0.78))
                        .flex()
                        .items_center()
                        .justify_center()
                        .drag_over::<TabDrag>(move |style, _, _, _| style.bg(p.line.opacity(0.85)))
                        .on_drop(cx.listener(move |this, drag: &TabDrag, window, cx| {
                            cx.stop_propagation();
                            this.queue_tab_drop(drag, Destination::Split(side), None, window, cx);
                        }))
                        .child(if side == Side::Left {
                            tr!("Split to the left")
                        } else {
                            tr!("Split to the right")
                        })
                }),
            ));
        }
        surface.into_any_element()
    }

    pub(super) fn workspace_body(
        &mut self,
        body: AnyElement,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let primary = self.pane_surface(body, cx);
        let Some(peer) = self.workspace.secondary.clone() else {
            return primary;
        };
        let p = self.p(cx);
        let mut split = h_resizable("document-split")
            .with_handle_appearance(reader::navigation_handle_appearance(p));
        if let Some(state) = &self.workspace.split {
            split = split.with_state(state);
        }
        let primary = resizable_panel()
            .size_range(px(280.)..gpui_kit::Pixels::MAX)
            .child(primary);
        let secondary = resizable_panel()
            .size_range(px(280.)..gpui_kit::Pixels::MAX)
            .child(peer);
        split = if self.workspace.side == Side::Left {
            split.child(secondary).child(primary)
        } else {
            split.child(primary).child(secondary)
        };
        split.into_any_element()
    }
}

#[cfg(test)]
mod tests;
