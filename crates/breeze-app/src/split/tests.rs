use super::*;
use gpui_kit::test::{TestAppContextExt, TestWindowExt};

fn fixture(cx: &mut TestAppContext) -> (AnyWindowHandle, Entity<Breeze>) {
    let (handle, view) = crate::ui_tests::app(cx);
    view.update(cx, |view, _| {
        let mut second = view.books[0].library_record();
        second.source = Source::File(
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets/samples/field-notes.pdf"),
        );
        second.title = "Second PDF".into();
        view.books.push(second);
    });
    (handle, view)
}

fn split(view: &Entity<Breeze>, side: Side, window: &mut Window, cx: &mut App) -> Entity<Breeze> {
    view.update(cx, |view, cx| {
        let drag = TabDrag {
            owner: cx.weak_entity(),
            source: view.books[1].source.clone(),
            title: view.books[1].title.clone(),
        };
        view.drop_tab(&drag, Destination::Split(side), None, window, cx);
    });
    view.read(cx)
        .workspace
        .secondary
        .clone()
        .expect("second reader")
}

#[gpui_kit::test]
fn native_drag_creates_only_two_independent_readers(cx: &mut TestAppContext) {
    let (handle, view) = fixture(cx);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        let from = window.find(("document", 1usize)).bounds().center();
        let body = window.find("primary-pane").bounds();
        window.drag(
            from,
            point(body.right() - px(80.), body.top() + px(120.)),
            cx,
        );
    })
    .unwrap();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        let peer = view
            .read(cx)
            .workspace
            .secondary
            .clone()
            .expect("native drag must split");
        assert_eq!(view.read(cx).pane_tabs(), vec![0]);
        assert_eq!(peer.read(cx).pane_tabs(), vec![0]);
        assert_ne!(
            view.read(cx).books[view.read(cx).active].source,
            peer.read(cx).books[peer.read(cx).active].source
        );
        for tabs in ["primary-tabs", "secondary-tabs"] {
            assert!(
                window
                    .within(tabs)
                    .find("document-tabs")
                    .bounds()
                    .size
                    .width
                    >= px(40.)
            );
        }
        let left = window.find("primary-pane").bounds();
        let right = window.find("secondary-pane").bounds();
        assert!(left.right() <= right.left() + px(1.));
        for pane in ["primary-pane", "secondary-pane"] {
            assert_eq!(
                window
                    .within(pane)
                    .find("reader-toolbar")
                    .bounds()
                    .size
                    .height,
                px(style::TOOLBAR_HEIGHT)
            );
            assert_eq!(
                window
                    .within(pane)
                    .find("status-scroll")
                    .bounds()
                    .size
                    .height,
                px(32.)
            );
        }
        window.within("secondary-pane").click("collapse", cx);
        assert!(!peer.read(cx).sidebar);
        assert!(view.read(cx).sidebar);
        window.press("secondary-b", cx);
        assert!(peer.read(cx).sidebar);
        assert!(view.read(cx).sidebar);
        window
            .within("secondary-pane")
            .click("document-panel-toggle", cx);
        assert!(peer.read(cx).document_panel.open);
        assert!(!view.read(cx).document_panel.open);
        window.within("primary-pane").click("collapse", cx);
        assert!(!view.read(cx).sidebar);
        window.press("secondary-b", cx);
        assert!(view.read(cx).sidebar);
        assert!(peer.read(cx).sidebar);
        view.update(cx, |view, cx| {
            let drag = TabDrag {
                owner: cx.weak_entity(),
                source: view.books[0].source.clone(),
                title: "PDF".into(),
            };
            view.drop_tab(&drag, Destination::Split(Side::Left), None, window, cx);
        });
        assert_eq!(view.read(cx).workspace.secondary.as_ref(), Some(&peer));
        assert!(peer.read(cx).workspace.secondary.is_none());
    })
    .unwrap();
}

#[gpui_kit::test]
fn left_split_moves_back_and_collapses_without_cloning_annotations(cx: &mut TestAppContext) {
    let (handle, view) = fixture(cx);
    cx.update_window(handle, |_, window, cx| {
        let identity = view.read(cx).books[1].annotations.identity.clone();
        let peer = split(&view, Side::Left, window, cx);
        window.render_frame(cx);
        assert!(
            window.find("secondary-pane").bounds().right()
                <= window.find("primary-pane").bounds().left() + px(1.)
        );
        assert!(Arc::ptr_eq(
            &peer.read(cx).books[0].annotations.identity,
            &identity
        ));
        assert!(!Arc::ptr_eq(
            &view.read(cx).books[1].annotations.identity,
            &identity
        ));
        let drag = TabDrag {
            owner: peer.downgrade(),
            source: peer.read(cx).books[0].source.clone(),
            title: "PDF".into(),
        };
        view.update(cx, |view, cx| {
            view.drop_tab(&drag, Destination::Primary, None, window, cx)
        });
        assert!(view.read(cx).workspace.secondary.is_none());
        assert!(Arc::ptr_eq(
            &view.read(cx).books[1].annotations.identity,
            &identity
        ));
        assert_eq!(view.read(cx).pane_tabs().len(), 2);
        window.render_frame(cx);
        assert!(window.try_find("secondary-pane").is_none());
    })
    .unwrap();
}

#[gpui_kit::test]
fn closing_last_primary_returns_peer_and_keeps_library_state(cx: &mut TestAppContext) {
    let (handle, view) = fixture(cx);
    cx.update_window(handle, |_, window, cx| {
        let peer = split(&view, Side::Right, window, cx);
        peer.update(cx, |peer, _| {
            peer.books[0].state.zoom = Zoom::Percent(150);
            peer.books[0].favorite = true;
        });
        view.update(cx, |view, cx| view.close_book(0, cx));
        assert!(view.read(cx).workspace.secondary.is_none());
        assert_eq!(view.read(cx).active, 1);
        assert_eq!(view.read(cx).books[1].state.zoom, Zoom::Percent(150));
        assert!(view.read(cx).books[1].favorite);
        assert!(!view.read(cx).books[0].opened);
        assert!(view.read(cx).snapshot().books[1].opened);
    })
    .unwrap();
}

#[gpui_kit::test]
fn tab_reordering_keeps_close_and_reading_identity(cx: &mut TestAppContext) {
    let (handle, view) = fixture(cx);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.drag_to(("document", 1usize), ("document", 0usize), cx);
    })
    .unwrap();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert_eq!(view.read(cx).pane_tabs(), vec![1, 0]);
        assert!(view.read(cx).workspace.secondary.is_none());
        assert_eq!(view.read(cx).active, 0);
        window.hover(("document", 1usize), cx);
        window.click(("close", 1usize), cx);
        assert!(!view.read(cx).books[1].opened);
        assert!(view.read(cx).books[0].opened);
    })
    .unwrap();
}

#[gpui_kit::test]
async fn both_workers_render_and_navigation_and_search_stay_local(cx: &mut TestAppContext) {
    let (handle, view) = fixture(cx);
    let peer = cx
        .update_window(handle, |_, window, cx| {
            view.update(cx, |view, cx| {
                for book in &mut view.books {
                    book.state.zoom = Zoom::FitPage;
                }
                view.load(cx);
            });
            split(&view, Side::Right, window, cx)
        })
        .unwrap();
    cx.wait_for(handle, std::time::Duration::from_secs(15), |_, cx| {
        view.read(cx).preview.is_some()
            && peer.read(cx).preview.is_some()
            && view.read(cx).raster.visible_ready()
            && peer.read(cx).raster.visible_ready()
    })
    .await;
    cx.update_window(handle, |_, window, cx| {
        for reader in [&view, &peer] {
            let reader = reader.read(cx);
            let viewport = reader.viewport_bounds.unwrap();
            let page = reader.page_bounds[&0];
            assert!(page.left() >= viewport.left() && page.right() <= viewport.right() + px(1.));
            assert!(viewport.size.width < window.viewport_size().width / 2.);
        }
        peer.update(cx, |peer, cx| {
            peer.next_page(1, cx);
            peer.execute(&SearchDocument, window, cx);
        });
        assert_eq!(view.read(cx).books[0].state.navigation.page(), 0);
        assert_eq!(peer.read(cx).books[0].state.navigation.page(), 1);
        assert!(peer.read(cx).search_open);
        assert!(!view.read(cx).search_open);
        view.update(cx, |view, cx| view.sync_secondary(cx));
        let saved = view.read(cx).snapshot();
        assert_eq!(saved.books[0].page, Some(0));
        assert_eq!(saved.books[1].page, Some(1));
    })
    .unwrap();
}

#[gpui_kit::test]
fn native_peer_drop_merges_and_stale_owner_cannot_overwrite_a_session(cx: &mut TestAppContext) {
    let (handle, view) = fixture(cx);
    cx.update_window(handle, |_, window, cx| {
        let stale = TabDrag {
            owner: view.downgrade(),
            source: view.read(cx).books[1].source.clone(),
            title: "stale".into(),
        };
        let peer = split(&view, Side::Right, window, cx);
        let identity = peer.read(cx).books[0].annotations.identity.clone();
        view.update(cx, |view, cx| {
            view.drop_tab(&stale, Destination::Secondary, None, window, cx)
        });
        assert!(Arc::ptr_eq(
            &identity,
            &peer.read(cx).books[0].annotations.identity
        ));
        window.render_frame(cx);
        let from = window
            .within("secondary-tabs")
            .find(("document", 0usize))
            .bounds()
            .center();
        let to = window
            .within("primary-tabs")
            .find(("document", 0usize))
            .bounds()
            .center();
        window.drag(from, to, cx);
    })
    .unwrap();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert!(view.read(cx).workspace.secondary.is_none());
        assert_eq!(view.read(cx).pane_tabs(), vec![1, 0]);
    })
    .unwrap();
}

#[gpui_kit::test]
fn peer_annotations_keep_dirty_close_guards_and_library_actions(cx: &mut TestAppContext) {
    use breeze_core::annotations::{Annotation, Kind, Point};
    let (handle, view) = fixture(cx);
    cx.update_window(handle, |_, window, cx| {
        view.update(cx, |view, _| {
            view.categories.push(breeze_core::library::Category {
                id: 1,
                name: "Research".into(),
                color: Default::default(),
            });
            view.next_category_id = 2;
            view.books[1].annotations.ready = true;
            view.books[1].annotations.history.add(Annotation {
                page: 0,
                kind: Kind::Ink,
                points: vec![Point::new(30., 40.), Point::new(70., 80.)],
                quads: vec![],
                color: [1., 0., 0.],
                opacity: 1.,
                width: 2.,
                text: String::new(),
            });
        });
        let peer = split(&view, Side::Right, window, cx);
        assert!(peer.read(cx).books[0].annotations.dirty());
        view.update(cx, |view, cx| {
            view.close_book(1, cx);
            assert!(peer.read(cx).books[0].opened);
            assert!(!view.request_close(window, cx));
            view.toggle_favorite(1, cx);
            view.toggle_category(1, 1, cx);
            view.sync_secondary(cx);
            assert!(view.snapshot().books[1].favorite);
            assert_eq!(view.snapshot().books[1].categories, vec![1]);
            view.delete_category(1, cx);
        });
        assert!(peer.read(cx).books[0].favorite);
        assert!(peer.read(cx).books[0].categories.is_empty());
        let drag = TabDrag {
            owner: peer.downgrade(),
            source: peer.read(cx).books[0].source.clone(),
            title: "PDF".into(),
        };
        peer.update(cx, |peer, _| peer.books[0].annotations.saving = true);
        view.update(cx, |view, cx| {
            view.drop_tab(&drag, Destination::Primary, None, window, cx)
        });
        assert!(view.read(cx).workspace.secondary.is_some());
        peer.update(cx, |peer, _| peer.books[0].annotations.saving = false);
        view.update(cx, |view, cx| {
            view.drop_tab(&drag, Destination::Primary, None, window, cx)
        });
        assert!(view.read(cx).books[1].annotations.dirty());
        assert_eq!(view.read(cx).books[1].annotations.history.items.len(), 1);
    })
    .unwrap();
}

#[gpui_kit::test]
async fn moving_an_unfinished_sidecar_inspection_restarts_it_in_the_new_owner(
    cx: &mut TestAppContext,
) {
    let (handle, view) = fixture(cx);
    cx.update_window(handle, |_, _, cx| {
        view.update(cx, |view, cx| view.open_book(1, cx))
    })
    .unwrap();
    cx.wait_for(handle, std::time::Duration::from_secs(15), |_, cx| {
        view.read(cx).books[1].annotations.ready
    })
    .await;
    let peer = cx
        .update_window(handle, |_, window, cx| {
            view.update(cx, |view, _| view.books[1].annotations.ready = false);
            split(&view, Side::Right, window, cx)
        })
        .unwrap();
    cx.wait_for(handle, std::time::Duration::from_secs(15), |_, cx| {
        peer.read(cx).books[0].annotations.ready
    })
    .await;
}

#[gpui_kit::test]
fn single_document_and_cancelled_drags_do_not_split(cx: &mut TestAppContext) {
    let (handle, view) = fixture(cx);
    cx.update_window(handle, |_, window, cx| {
        view.update(cx, |view, cx| {
            view.books[1].opened = false;
            let drag = TabDrag {
                owner: cx.weak_entity(),
                source: view.books[0].source.clone(),
                title: "PDF".into(),
            };
            view.drop_tab(&drag, Destination::Split(Side::Right), None, window, cx);
            assert!(view.workspace.secondary.is_none());
            view.books[1].opened = true;
        });
        window.render_frame(cx);
        let from = window.find(("document", 1usize)).bounds().center();
        // Dropping on the titlebar's Home control is not a pane destination.
        let to = window.find("home-tab").bounds().center();
        window.drag(from, to, cx);
    })
    .unwrap();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert!(view.read(cx).workspace.secondary.is_none());
        assert_eq!(view.read(cx).pane_tabs(), vec![0, 1]);
        assert!(!cx.has_active_drag());
    })
    .unwrap();
}

#[gpui_kit::test]
fn middle_divider_resizes_both_panes(cx: &mut TestAppContext) {
    let (handle, view) = fixture(cx);
    cx.update_window(handle, |_, window, cx| {
        split(&view, Side::Right, window, cx);
        window.render_frame(cx);
        let before = window.find("primary-pane").bounds();
        window.drag(
            point(before.right(), px(300.)),
            point(before.right() + px(70.), px(300.)),
            cx,
        );
        window.render_frame(cx);
        assert!(window.find("primary-pane").bounds().size.width > before.size.width + px(20.));
        assert!(window.find("secondary-pane").bounds().size.width < before.size.width - px(20.));
    })
    .unwrap();
}

#[gpui_kit::test]
fn markdown_editor_entity_and_unsaved_draft_follow_a_moved_tab(cx: &mut TestAppContext) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("draft.md");
    std::fs::write(&path, "# Draft\n\nOriginal text\n").unwrap();
    let (handle, view) = fixture(cx);
    cx.update_window(handle, |_, window, cx| {
        view.update(cx, |view, cx| {
            view.books[1].source = Source::File(path.clone());
            view.books[1].title = "draft.md".into();
            view.open_book(1, cx);
            view.toggle_markdown_editing(window, cx);
        });
        window.render_frame(cx);
        window.input("Unsaved ", cx);
        let editor = view.read(cx).books[1].markdown_edit.clone().unwrap();
        assert!(editor.read(cx).dirty());
        let peer = split(&view, Side::Right, window, cx);
        assert_eq!(peer.read(cx).books[0].markdown_edit.as_ref(), Some(&editor));
        assert!(view.read(cx).books[1].markdown_edit.is_none());
        view.update(cx, |view, cx| assert!(!view.request_close(window, cx)));
        peer.update(cx, |peer, cx| peer.close_book(0, cx));
        assert!(
            peer.read(cx).books[0].opened,
            "first close must retain the unsaved draft"
        );
        let drag = TabDrag {
            owner: peer.downgrade(),
            source: peer.read(cx).books[0].source.clone(),
            title: "Draft".into(),
        };
        view.update(cx, |view, cx| {
            view.drop_tab(&drag, Destination::Primary, None, window, cx)
        });
        assert_eq!(view.read(cx).books[1].markdown_edit.as_ref(), Some(&editor));
        assert!(editor.read(cx).dirty());
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "# Draft\n\nOriginal text\n"
        );
    })
    .unwrap();
}

#[gpui_kit::test]
fn fast_drag_can_split_before_the_preview_has_painted(cx: &mut TestAppContext) {
    use gpui_kit::InputEvent as _;
    let (handle, view) = fixture(cx);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        let from = window.find(("document", 1usize)).bounds().center();
        let to = point(px(180.), px(230.));
        window.hover(("document", 1usize), cx);
        window.dispatch_event(
            MouseDownEvent {
                button: MouseButton::Left,
                position: from,
                modifiers: Default::default(),
                click_count: 1,
                first_mouse: false,
            }
            .to_platform_input(),
            cx,
        );
        window.render_frame(cx);
        window.dispatch_event(
            MouseMoveEvent {
                position: to,
                pressed_button: Some(MouseButton::Left),
                modifiers: Default::default(),
            }
            .to_platform_input(),
            cx,
        );
        assert!(cx.has_active_drag());
        // No frame between drag initiation and release: the root drop target
        // must work even though the left/right preview is not mounted yet.
        window.dispatch_event(
            MouseUpEvent {
                button: MouseButton::Left,
                position: to,
                modifiers: Default::default(),
                click_count: 1,
            }
            .to_platform_input(),
            cx,
        );
    })
    .unwrap();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert!(view.read(cx).workspace.secondary.is_some());
        assert_eq!(view.read(cx).workspace.side, Side::Left);
    })
    .unwrap();
}
