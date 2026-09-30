use super::*;
use gpui_kit::InputEvent as _;
use gpui_kit::test::TestWindowExt;

pub(super) fn app(cx: &mut TestAppContext) -> (AnyWindowHandle, Entity<Breeze>) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        commands::bind(cx);
        style::apply(false, ColorScheme::Neutral, cx);
        gpui_kit::open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds {
                    origin: point(px(0.), px(0.)),
                    size: size(px(1000.), px(700.)),
                })),
                ..TitleBar::window_options()
            },
            cx,
            |window, cx| {
                let view = cx.new(|cx| Breeze::new(PdfWorker::new().unwrap(), window, cx));
                view.update(cx, |view, cx| {
                    view.screen = Screen::Reader;
                    view.books[0].opened = true;
                    view.books[0].state.navigation.set_total(8);
                    window.focus(&view.focus, cx);
                });
                view
            },
        )
        .unwrap()
    })
}
#[gpui_kit::test]
fn commands_and_input_keys_work(cx: &mut TestAppContext) {
    let (handle, view) = app(cx);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert!(
            gpui_kit::component::kbd::Kbd::binding_for_action(
                &ToggleSidebar,
                Some("Breeze"),
                window
            )
            .is_some()
        );
        assert!(
            gpui_kit::component::kbd::Kbd::binding_for_action(&ZoomIn, Some("Breeze"), window)
                .is_some()
        );
        window.press("secondary-b", cx);
        assert!(!view.read(cx).sidebar);
        window.press("secondary-b", cx);
        assert!(view.read(cx).sidebar);
        window.press("secondary-2", cx);
        assert_eq!(view.read(cx).books[0].state.mode, Mode::Annotate);
        window.press("secondary-3", cx);
        assert_eq!(view.read(cx).books[0].state.mode, Mode::Edit);
        window.press("secondary-g", cx);
        assert!(
            view.read(cx)
                .page_input
                .read(cx)
                .focus_handle(cx)
                .is_focused(window)
        );
        // Zoom shortcuts must not steal keys from the focused text input.
        let zoom = view.read(cx).books[0].state.zoom;
        window.press("secondary-=", cx);
        assert_eq!(view.read(cx).books[0].state.zoom, zoom);
        window.input("4", cx);
        assert_eq!(view.read(cx).page_input.read(cx).value().as_ref(), "4");
        window.press("enter", cx);
    })
    .unwrap();
    // Entity events are delivered after the enclosing App update, not inside it.
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert_eq!(view.read(cx).books[0].state.navigation.page(), 3);
        window.press("secondary-=", cx);
        assert_eq!(view.read(cx).books[0].state.zoom, Zoom::Percent(125));
        window.press("secondary-,", cx);
        assert!(view.read(cx).screen == Screen::Settings);
        window.press("secondary-shift-h", cx);
        assert!(view.read(cx).screen == Screen::Home);
    })
    .unwrap();
}
#[gpui_kit::test]
fn resizing_respects_bounds_and_keeps_width_on_collapse(cx: &mut TestAppContext) {
    let (handle, view) = app(cx);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        let split = view.read(cx).navigation_split.clone();
        let before = split.read(cx).sizes()[0];
        window.drag(
            point(before, px(200.)),
            point(before + px(60.), px(200.)),
            cx,
        );
        window.render_frame(cx);
        assert!(
            split.read(cx).sizes()[0] > before + px(20.),
            "native pointer drag must resize sidebar"
        );
        split.update(cx, |state, cx| state.resize_panel(0, px(900.), window, cx));
        window.render_frame(cx);
        assert!(split.read(cx).sizes()[0] <= px(360.));
        split.update(cx, |state, cx| state.resize_panel(0, px(10.), window, cx));
        window.render_frame(cx);
        assert!(split.read(cx).sizes()[0] >= px(168.));
        split.update(cx, |state, cx| state.resize_panel(0, px(280.), window, cx));
        window.render_frame(cx);
        window.press("secondary-b", cx);
        window.press("secondary-b", cx);
        window.render_frame(cx);
        assert!((f32::from(split.read(cx).sizes()[0]) - 280.).abs() < 2.);
    })
    .unwrap();
}
#[gpui_kit::test]
fn titlebar_commands_and_theme_slots(cx: &mut TestAppContext) {
    let (handle, view) = app(cx);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.click("settings", cx);
        assert!(view.read(cx).screen == Screen::Settings);
        view.update(cx, |view, cx| {
            view.set_appearance(ThemePreference::Dark, window, cx)
        });
        window.render_frame(cx);
        window.click(("dark-palette", 3_usize), cx);
        assert_eq!(view.read(cx).dark_scheme, ColorScheme::Forest);
        assert_eq!(view.read(cx).light_scheme, ColorScheme::Neutral);
        window.click(("light-palette", 2_usize), cx);
        assert_eq!(view.read(cx).light_scheme, ColorScheme::Blue);
        assert_eq!(view.read(cx).dark_scheme, ColorScheme::Forest);
        view.update(cx, |view, cx| {
            view.set_appearance(ThemePreference::Light, window, cx)
        });
        window.render_frame(cx);
        assert!(!view.read(cx).dark);
        window.click("home-tab", cx);
        assert!(view.read(cx).screen == Screen::Home);
    })
    .unwrap();
}

fn rendered_sample() -> breeze_pdf::Page {
    let worker = PdfWorker::new().unwrap();
    futures::executor::block_on(worker.request(Source::Sample, 0).unwrap())
        .unwrap()
        .unwrap()
}
#[gpui_kit::test]
fn annotation_gestures_history_and_close_guard(cx: &mut TestAppContext) {
    let page = rendered_sample();
    let (handle, view) = app(cx);
    cx.update_window(handle, |_, window, cx| {
        view.update(cx, |view, _| {
            view.preview = Some(Arc::new(Preview {
                page: page.page,
                points: page.points,
                geometry: page.geometry,
                glyphs: page.glyphs,
                text_warning: page.text_warning,
                outline: page.outline,
                bytes: 0,
                elapsed_ms: 0.,
            }));
            view.books[0].annotations.ready = true;
            view.books[0].state.mode = Mode::Annotate;
            view.tool = 4;
        });
        window.render_frame(cx);
        let bounds = window.find("page-surface").bounds();
        let overlay = window
            .within("page-surface")
            .find("annotation-overlay")
            .bounds();
        assert_eq!(
            overlay, bounds,
            "Annotation overlay must cover the page, not follow it in normal flow"
        );
        let start = point(bounds.origin.x + px(40.), bounds.origin.y + px(100.));
        let end = point(start.x + px(120.), start.y + px(40.));
        window.drag(start, end, cx);
        assert_eq!(view.read(cx).books[0].annotations.history.items.len(), 1);
        assert_eq!(
            view.read(cx).books[0].annotations.history.items[0].kind,
            breeze_core::annotations::Kind::Ink
        );
        assert!(view.read(cx).books[0].annotations.dirty());
        view.update(cx, |view, cx| {
            assert!(!view.allow_close(cx));
        });
        window.press("secondary-z", cx);
        assert!(view.read(cx).books[0].annotations.history.items.is_empty());
        window.press("secondary-shift-z", cx);
        assert_eq!(view.read(cx).books[0].annotations.history.items.len(), 1);
        view.update(cx, |view, _| view.tool = 5);
        window.render_frame(cx);
        // The eraser hits the stroke's first segment, not the page background.
        window.click_at("page-surface", point(px(40.), px(100.)), cx);
        assert!(view.read(cx).books[0].annotations.history.items.is_empty());
    })
    .unwrap();
}
#[gpui_kit::test]
fn text_selection_uses_extracted_geometry(cx: &mut TestAppContext) {
    let page = rendered_sample();
    let (handle, view) = app(cx);
    cx.update_window(handle, |_, window, cx| {
        view.update(cx, |view, _| {
            view.preview = Some(Arc::new(Preview {
                page: page.page,
                points: page.points,
                geometry: page.geometry,
                glyphs: page.glyphs,
                text_warning: page.text_warning,
                outline: page.outline,
                bytes: 0,
                elapsed_ms: 0.,
            }));
        });
        window.render_frame(cx);
        let bounds = window.find("page-surface").bounds();
        let overlay = window
            .within("page-surface")
            .find("annotation-overlay")
            .bounds();
        assert_eq!(
            overlay, bounds,
            "Annotation overlay must cover the page, not follow it in normal flow"
        );
        window.drag(
            point(bounds.origin.x + px(1.), bounds.origin.y + px(1.)),
            point(
                bounds.origin.x + bounds.size.width - px(2.),
                bounds.origin.y + bounds.size.height - px(2.),
            ),
            cx,
        );
        assert!(view.read(cx).selected_text.contains("quiet art of focus"));
        view.update(cx, |view, _| {
            view.books[0].annotations.ready = true;
            view.books[0].state.mode = Mode::Annotate;
            view.tool = 0;
        });
        window.render_frame(cx);
        let bounds = window.find("page-surface").bounds();
        let overlay = window
            .within("page-surface")
            .find("annotation-overlay")
            .bounds();
        assert_eq!(
            overlay, bounds,
            "Annotation overlay must cover the page, not follow it in normal flow"
        );
        window.drag(
            point(bounds.origin.x + px(1.), bounds.origin.y + px(1.)),
            point(
                bounds.origin.x + bounds.size.width - px(2.),
                bounds.origin.y + bounds.size.height - px(2.),
            ),
            cx,
        );
        let history = &view.read(cx).books[0].annotations.history;
        assert_eq!(history.items.len(), 1);
        assert!(!history.items[0].quads.is_empty());
        assert!(history.items[0].validate().is_ok());
    })
    .unwrap();
}
#[gpui_kit::test]
fn restoration_flags_are_independent(cx: &mut TestAppContext) {
    let (handle, view) = app(cx);
    cx.update_window(handle, |_, _, cx| {
        view.update(cx, |view, cx| {
            view.books[0].state.navigation.go_to(4);
            view.books[0].favorite = true;
            view.remember_files = false;
            view.remember_position = true;
            let saved = view.snapshot();
            assert!(!saved.books[0].opened);
            assert_eq!(saved.books[0].page, Some(4));
            view.restore(saved, cx);
            assert!(!view.books[0].opened);
            assert_eq!(view.books[0].state.navigation.page(), 4);
            assert!(view.books[0].favorite);
            view.books[0].opened = true;
            view.remember_files = true;
            view.remember_position = false;
            let saved = view.snapshot();
            assert!(saved.books[0].opened);
            assert_eq!(saved.books[0].page, None);
            view.restore(saved, cx);
            assert!(view.books[0].opened);
            assert_eq!(view.books[0].state.navigation.page(), 0);
        });
    })
    .unwrap();
}

#[gpui_kit::test]
fn annotation_tool_matrix_creates_real_geometry(cx: &mut TestAppContext) {
    use breeze_core::annotations::Kind;
    let page = rendered_sample();
    let (handle, view) = app(cx);
    cx.update_window(handle, |_, window, cx| {
        view.update(cx, |view, _| {
            view.preview = Some(Arc::new(Preview {
                page: page.page,
                points: page.points,
                geometry: page.geometry,
                glyphs: page.glyphs,
                text_warning: page.text_warning,
                outline: page.outline,
                bytes: 0,
                elapsed_ms: 0.,
            }));
            view.books[0].state.mode = Mode::Annotate;
            view.books[0].annotations.ready = true;
        });
        for (tool, kind) in [
            (0, Kind::Highlight),
            (1, Kind::Area),
            (2, Kind::Underline),
            (3, Kind::Strikeout),
            (4, Kind::Ink),
            (6, Kind::Note),
            (7, Kind::Ellipse),
            (8, Kind::Rectangle),
            (9, Kind::Line),
            (10, Kind::Squiggle),
            (11, Kind::Arrow),
        ] {
            view.update(cx, |view, _| view.tool = tool);
            window.render_frame(cx);
            let bounds = window.find("page-surface").bounds();
            window.drag(
                point(bounds.origin.x + px(2.), bounds.origin.y + px(2.)),
                point(
                    bounds.origin.x + bounds.size.width - px(3.),
                    bounds.origin.y + bounds.size.height - px(3.),
                ),
                cx,
            );
            let annotations = &view.read(cx).books[0].annotations.history.items;
            assert_eq!(annotations.last().unwrap().kind, kind);
            assert!(annotations.last().unwrap().validate().is_ok());
        }
        assert_eq!(view.read(cx).books[0].annotations.history.items.len(), 11);
    })
    .unwrap();
}

fn exportable_annotation() -> breeze_core::annotations::Annotation {
    use breeze_core::annotations::{Annotation, Kind, Point};
    Annotation {
        page: 0,
        kind: Kind::Ink,
        points: vec![Point::new(50., 50.), Point::new(90., 90.)],
        quads: Vec::new(),
        color: [0.1, 0.5, 0.2],
        opacity: 1.,
        width: 2.,
        text: String::new(),
    }
}
#[gpui_kit::test]
async fn export_uses_snapshot_and_never_acknowledges_sidecar_save(cx: &mut TestAppContext) {
    use gpui_kit::test::TestAppContextExt;
    let folder = tempfile::tempdir().unwrap();
    let destination = folder.path().join("annotated.pdf");
    let fingerprint = rendered_sample().fingerprint;
    let (handle, view) = app(cx);
    cx.update_window(handle, |_, window, cx| {
        view.update(cx, |view, cx| {
            let a = &mut view.books[0].annotations;
            a.ready = true;
            a.fingerprint = fingerprint;
            a.history.items = vec![exportable_annotation()];
            let request = view.export_snapshot(destination.clone()).unwrap();
            view.start_pdf_export(request, cx);
            assert!(!view.can_export_pdf());
            assert!(!view.allow_close(cx));
            view.books[0]
                .annotations
                .history
                .add(exportable_annotation());
        });
        window.render_frame(cx);
    })
    .unwrap();
    cx.wait_for(handle, std::time::Duration::from_secs(10), |_, cx| {
        !view.read(cx).export_busy
    })
    .await;
    cx.update_window(handle, |_, _, cx| {
        let view = view.read(cx);
        assert_eq!(view.last_export.as_ref(), Some(&destination));
        assert!(view.books[0].annotations.dirty());
        assert!(view.books[0].annotations.saved.is_empty());
        assert_eq!(view.books[0].annotations.history.items.len(), 2);
    })
    .unwrap();
    assert!(destination.exists());
    cx.update_window(handle, |_, _, cx| {
        assert!(
            view.read(cx)
                .export_notice
                .as_ref()
                .unwrap()
                .contains("1 条批注")
        )
    })
    .unwrap();
}
#[gpui_kit::test]
async fn failed_export_preserves_destination_and_releases_busy_state(cx: &mut TestAppContext) {
    use gpui_kit::test::TestAppContextExt;
    let folder = tempfile::tempdir().unwrap();
    let destination = folder.path().join("existing.pdf");
    std::fs::write(&destination, b"original destination").unwrap();
    let fingerprint = rendered_sample().fingerprint;
    let (handle, view) = app(cx);
    cx.update_window(handle, |_, _, cx| {
        view.update(cx, |view, cx| {
            let a = &mut view.books[0].annotations;
            a.ready = true;
            a.fingerprint = fingerprint;
            a.history.items = vec![exportable_annotation()];
            let request = view.export_snapshot(destination.clone()).unwrap();
            view.start_pdf_export(request, cx);
        })
    })
    .unwrap();
    cx.wait_for(handle, std::time::Duration::from_secs(10), |_, cx| {
        !view.read(cx).export_busy
    })
    .await;
    cx.update_window(handle, |_, _, cx| {
        assert!(view.read(cx).last_export.is_none());
        assert!(
            view.read(cx)
                .export_notice
                .as_ref()
                .unwrap()
                .contains("导出失败")
        );
        assert!(view.read(cx).can_export_pdf());
    })
    .unwrap();
    assert_eq!(std::fs::read(destination).unwrap(), b"original destination");
}
#[gpui_kit::test]
fn opening_policy_runs_once_and_does_not_change_zoom(cx: &mut TestAppContext) {
    let page = rendered_sample();
    let (handle, view) = app(cx);
    cx.update_window(handle, |_, _, cx| {
        view.update(cx, |view, cx| {
            view.preview = Some(Arc::new(Preview {
                page: page.page,
                points: page.points,
                geometry: page.geometry,
                glyphs: page.glyphs,
                text_warning: page.text_warning,
                outline: page.outline,
                bytes: 0,
                elapsed_ms: 0.,
            }));
            view.window_policy = 2;
            view.pending_window_policy = true;
            view.books[0].state.zoom = Zoom::Percent(150);
            view.apply_opening_policy(cx);
            assert!(!view.pending_window_policy);
            assert_eq!(view.books[0].state.zoom, Zoom::Percent(150));
        })
    })
    .unwrap();
    cx.update_window(handle, |_, _, cx| {
        view.update(cx, |view, cx| {
            view.pending_window_policy = true;
            view.show_home(cx);
            assert!(!view.pending_window_policy);
        })
    })
    .unwrap();
}

#[gpui_kit::test]
fn categories_are_user_owned_and_layout_roundtrips(cx: &mut TestAppContext) {
    let (handle, view) = app(cx);
    cx.update_window(handle, |_, window, cx| {
        view.update(cx, |view, cx| {
            assert!(view.categories.is_empty());
            assert_eq!(view.books[0].category, 0);
            view.category_form = true;
            view.category_input
                .update(cx, |input, cx| input.set_value("  Research  ", window, cx));
            view.save_category(cx);
            let id = view.categories[0].id;
            assert_eq!(view.categories[0].name, "Research");
            view.books[0].category = id;
            view.books[0].state.layout = ReadingLayout::Spread;
            let saved = view.snapshot();
            let saved = serde_json::from_slice(&serde_json::to_vec(&saved).unwrap()).unwrap();
            view.restore(saved, cx);
            assert_eq!(view.books[0].state.layout, ReadingLayout::Spread);
            assert_eq!(view.books[0].category, id);
            view.category_edit = Some(id);
            view.category_input
                .update(cx, |input, cx| input.set_value("Reading", window, cx));
            view.save_category(cx);
            assert_eq!(view.category_name(id), "Reading");
            let next = view.next_category_id;
            view.delete_category(id, cx);
            assert!(view.categories.is_empty());
            assert_eq!(view.books[0].category, 0);
            assert_eq!(view.next_category_id, next);
            assert_eq!(view.books.len(), 1);
            let mut legacy = view.snapshot();
            legacy.version = 1;
            legacy.books[0].category = 2;
            view.restore(legacy, cx);
            assert_eq!(view.categories.len(), 1);
            assert_eq!(view.categories[0].id, 2);
            assert_eq!(view.books[0].category, 2);
            assert_eq!(view.snapshot().version, 5);
        });
    })
    .unwrap();
}

#[gpui_kit::test]
fn chrome_heights_and_left_navigation_are_preserved(cx: &mut TestAppContext) {
    let (handle, view) = app(cx);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert_eq!(
            window.find("titlebar-content").bounds().size.height,
            px(33.) // The native 34 px title bar includes its 1 px bottom border.
        );
        assert_eq!(window.find("reader-toolbar").bounds().size.height, px(38.));
        assert_eq!(window.find("status-scroll").bounds().size.height, px(32.));
        assert_eq!(window.find("home-tab").bounds().size.height, px(22.5));
        view.update(cx, |view, cx| view.show_home(cx));
        window.render_frame(cx);
        let button = window.find(("library-nav", 0usize)).bounds();
        let content = window
            .within(("library-nav", 0usize))
            .find("left-label-row")
            .bounds();
        assert!(content.size.width > button.size.width - px(20.));
        assert!(content.origin.x - button.origin.x < px(12.));
    })
    .unwrap();
}

#[gpui_kit::test]
fn spread_second_page_gesture_targets_its_own_surface(cx: &mut TestAppContext) {
    let worker = PdfWorker::new().unwrap();
    let first = futures::executor::block_on(worker.request(Source::Sample, 0).unwrap())
        .unwrap()
        .unwrap();
    let second = futures::executor::block_on(worker.request(Source::Sample, 1).unwrap())
        .unwrap()
        .unwrap();
    let (handle, view) = app(cx);
    cx.update_window(handle, |_, window, cx| {
        view.update(cx, |view, _| {
            view.reading.sizes = first.page_sizes.clone();
            let first = Preview::from_page(first).unwrap();
            let second = Preview::from_page(second).unwrap();
            view.reading.insert(0, first.clone());
            view.reading.insert(1, second);
            view.preview = Some(first);
            view.books[0].state.layout = ReadingLayout::Spread;
            view.books[0].state.mode = Mode::Annotate;
            view.books[0].annotations.ready = true;
            view.tool = 4;
        });
        window.render_frame(cx);
        let left = window
            .within(("page-frame", 0usize))
            .find("page-surface")
            .bounds();
        let right = window
            .within(("page-frame", 1usize))
            .find("page-surface")
            .bounds();
        assert!(right.origin.x >= left.origin.x + left.size.width);
        window.drag(
            right.origin + point(px(20.), px(20.)),
            right.origin + point(px(65.), px(50.)),
            cx,
        );
        let history = &view.read(cx).books[0].annotations.history;
        assert_eq!(history.items.len(), 1);
        assert_eq!(history.items[0].page, 1);
        assert!(history.items[0].validate().is_ok());
    })
    .unwrap();
}

#[gpui_kit::test]
fn virtual_lists_reach_last_page_without_eager_page_images(cx: &mut TestAppContext) {
    let page = rendered_sample();
    let (handle, view) = app(cx);
    cx.update_window(handle, |_, window, cx| {
        view.update(cx, |view, _| {
            view.reading.sizes = Arc::new(vec![page.points; 120]);
            let preview = Preview::from_page(page).unwrap();
            view.reading.insert(0, preview.clone());
            view.preview = Some(preview);
            view.books[0].state.navigation.set_total(120);
            view.books[0].state.layout = ReadingLayout::Continuous;
            view.thumbs = true;
            // Keep real background jobs out of this deterministic layout/scroll test.
            view.loading = true;
        });
        window.render_frame(cx);
        view.update(cx, |view, _| {
            view.reading
                .thumbs_scroll
                .scroll_to_item(119, ScrollStrategy::Top);
            view.reading
                .pages_scroll
                .scroll_to_item(119, ScrollStrategy::Top);
        });
        window.render_frame(cx);
        assert!(window.find(("thumbnail", 119usize)).visible());
        assert!(view.read(cx).reading.pages_scroll.offset().y < -px(1000.));
        window.click(("thumbnail", 119usize), cx);
        assert_eq!(view.read(cx).books[0].state.navigation.page(), 119);
    })
    .unwrap();
}

#[gpui_kit::test]
fn virtual_measurement_is_coalesced_and_stale_ranges_are_ignored(cx: &mut TestAppContext) {
    let (handle, view) = app(cx);
    cx.update_window(handle, |_, _, cx| {
        view.update(cx, |view, cx| {
            view.books[0].state.layout = ReadingLayout::Continuous;
            view.books[0].state.navigation.set_total(120);
            view.defer_visible(0..1, false, cx);
            view.defer_visible(99..102, false, cx);
        });
    })
    .unwrap();
    cx.update_window(handle, |_, _, cx| {
        assert_eq!(view.read(cx).books[0].state.navigation.page(), 99);
        view.update(cx, |view, cx| {
            view.defer_visible(0..1, false, cx);
            view.generation += 1;
        });
    })
    .unwrap();
    cx.update_window(handle, |_, _, cx| {
        assert_eq!(view.read(cx).books[0].state.navigation.page(), 99);
    })
    .unwrap();
}

#[gpui_kit::test]
fn tab_close_is_hover_only_and_does_not_activate_its_document(cx: &mut TestAppContext) {
    let (handle, view) = app(cx);
    cx.update_window(handle, |_, window, cx| {
        view.update(cx, |view, _| {
            view.books.push(Book {
                source: Source::Sample,
                title: "Second".into(),
                favorite: false,
                category: 0,
                state: ReaderState::default(),
                opened: true,
                last_opened: 0,
                annotations: DocumentAnnotations::default(),
            });
        });
        window.render_frame(cx);
        window.hover("home-tab", cx);
        assert!(!window.find(("close", 1usize)).visible());
        window.hover(("document", 1usize), cx);
        assert!(window.find(("close", 1usize)).visible());
        let before = window.find(("document", 1usize)).bounds();
        let home = window.find("home-tab").bounds();
        let close = window.find(("close", 1usize)).bounds();
        assert_eq!(before.size.height, home.size.height);
        assert_eq!(before.origin.y, home.origin.y);
        assert!(close.right() <= before.right());
        assert!(before.right() - close.right() <= px(2.));
        window.click(("close", 1usize), cx);
        assert!(!view.read(cx).books[1].opened);
        assert_eq!(view.read(cx).active, 0);
        assert!(before.size.width > px(0.));
    })
    .unwrap();
}

#[gpui_kit::test]
fn remembered_page_number_does_not_allocate_placeholder_pages(cx: &mut TestAppContext) {
    let (handle, view) = app(cx);
    cx.update_window(handle, |_, window, cx| {
        view.update(cx, |view, _| {
            // The worker validates the actual document count asynchronously.
            view.books[0].state.layout = ReadingLayout::Spread;
            view.books[0].state.navigation.set_total(usize::MAX);
            view.books[0].state.navigation.go_to(usize::MAX - 1);
            view.loading = true;
        });
        window.render_frame(cx);
        assert!(view.read(cx).preview.is_none());
        assert!(view.read(cx).reading.sizes.is_empty());
    })
    .unwrap();
}

#[gpui_kit::test]
fn status_panels_keep_layout_axes_independent_and_restore_view_memory(cx: &mut TestAppContext) {
    let (handle, view) = app(cx);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert_eq!(
            view.read(cx).books[0].state.layout,
            ReadingLayout::Continuous
        );
        window.click("layout", cx);
        window.press("down", cx);
        window.press("down", cx);
        window.press("enter", cx);
    })
    .unwrap();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert_eq!(
            view.read(cx).books[0].state.layout,
            ReadingLayout::ContinuousSpread
        );
        window.click("layout", cx);
        window.press("down", cx);
        window.press("down", cx);
        window.press("down", cx);
        window.press("enter", cx);
    })
    .unwrap();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert_eq!(view.read(cx).books[0].state.layout, ReadingLayout::Spread);
        window.click("zoom", cx);
        window.press("down", cx);
        window.press("down", cx);
        window.press("enter", cx);
    })
    .unwrap();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert_eq!(view.read(cx).books[0].state.zoom, Zoom::FitWidth);
        window.focus(&view.read(cx).focus.clone(), cx);
        window.press("secondary-0", cx);
        window.press("secondary-+", cx);
        assert_eq!(view.read(cx).books[0].state.zoom, Zoom::Percent(125));
        window.press("secondary--", cx);
        window.press("secondary-shift-=", cx);
        assert_eq!(view.read(cx).books[0].state.zoom, Zoom::Percent(125));
        view.update(cx, |view, cx| {
            view.remember_position = false;
            let saved: SavedState =
                serde_json::from_slice(&serde_json::to_vec(&view.snapshot()).unwrap()).unwrap();
            view.books[0].state = ReaderState::default();
            view.restore(saved, cx);
            assert_eq!(view.books[0].state.layout, ReadingLayout::Spread);
            assert_eq!(view.books[0].state.zoom, Zoom::Percent(125));
            assert_eq!(
                view.reading_preferences,
                ReadingPreferences {
                    layout: ReadingLayout::Spread,
                    zoom: Zoom::Percent(125)
                }
            );
            view.open_canonical_path(PathBuf::from("/nonexistent/new-view-test.pdf"), cx);
            assert_eq!(view.books[1].state.layout, ReadingLayout::Spread);
            assert_eq!(view.books[1].state.zoom, Zoom::Percent(125));
        });
    })
    .unwrap();
}

#[gpui_kit::test]
fn modified_wheel_zooms_without_scrolling_and_plain_wheel_still_scrolls(cx: &mut TestAppContext) {
    let page = rendered_sample();
    let (handle, view) = app(cx);
    cx.update_window(handle, |_, window, cx| {
        view.update(cx, |view, _| {
            view.reading.sizes = page.page_sizes.clone();
            view.preview = Some(Preview::from_page(page).unwrap());
            view.books[0].state.zoom = Zoom::Percent(100);
            view.loading = true;
        });
        window.render_frame(cx);
        window.hover("document-viewport", cx);
        let position = window.find("document-viewport").bounds().center();
        let primary = if cfg!(target_os = "macos") {
            Modifiers {
                platform: true,
                ..Default::default()
            }
        } else {
            Modifiers {
                control: true,
                ..Default::default()
            }
        };
        let before = view.read(cx).reading.pages_scroll.offset();
        window.dispatch_event(
            ScrollWheelEvent {
                position,
                delta: ScrollDelta::Lines(point(0., -1.)),
                modifiers: primary,
                ..Default::default()
            }
            .to_platform_input(),
            cx,
        );
        assert_eq!(view.read(cx).books[0].state.zoom, Zoom::Percent(75));
        assert_eq!(view.read(cx).reading.pages_scroll.offset(), before);
        window.render_frame(cx);
        let before = view.read(cx).reading.pages_scroll.offset();
        window.scroll("document-viewport", ScrollDelta::Lines(point(0., -6.)), cx);
        assert_eq!(view.read(cx).books[0].state.zoom, Zoom::Percent(75));
        assert!(
            view.read(cx).reading.pages_scroll.offset().y < before.y,
            "before {:?}, after {:?}, pages {}, viewport {:?}",
            before,
            view.read(cx).reading.pages_scroll.offset(),
            view.read(cx).reading.sizes.len(),
            window.find("document-viewport").bounds()
        );
        view.update(cx, |view, cx| view.set_zoom(Zoom::FitWidth, cx));
        window.render_frame(cx);
        let fitted = view.read(cx).display_zoom(window);
        window.focus(&view.read(cx).focus.clone(), cx);
        window.press("secondary--", cx);
        assert_eq!(
            view.read(cx).books[0].state.zoom,
            Zoom::step_percent(fitted.round(), false)
        );
    })
    .unwrap();
}

#[gpui_kit::test]
fn continuous_spreads_virtualize_pairs_and_reach_odd_final_page(cx: &mut TestAppContext) {
    let page = rendered_sample();
    let (handle, view) = app(cx);
    cx.update_window(handle, |_, window, cx| {
        view.update(cx, |view, _| {
            view.reading.sizes = Arc::new(vec![page.points; 121]);
            let preview = Preview::from_page(page).unwrap();
            view.reading.insert(0, preview.clone());
            view.preview = Some(preview);
            view.books[0].state.navigation.set_total(121);
            view.books[0].state.layout = ReadingLayout::ContinuousSpread;
            view.loading = true;
        });
        window.render_frame(cx);
        let left = window.find(("page-frame", 0usize)).bounds();
        let right = window.find(("page-frame", 1usize)).bounds();
        assert!(right.left() >= left.right());
        view.update(cx, |view, _| {
            view.reading
                .pages_scroll
                .scroll_to_item(60, ScrollStrategy::Top)
        });
        window.render_frame(cx);
        assert!(window.find(("page-frame", 120usize)).visible());
        view.update(cx, |view, cx| {
            view.loading = false;
            view.schedule_visible(120..121, false, cx);
        });
        assert_eq!(view.read(cx).books[0].state.navigation.page(), 120);
    })
    .unwrap();
}

#[gpui_kit::test]
fn actual_page_surfaces_request_new_pixels_after_zoom_and_display_changes(cx: &mut TestAppContext) {
    let page = rendered_sample();
    let (handle, view) = app(cx);
    cx.update_window(handle, |_, window, cx| {
        view.update(cx, |view, _| {
            view.reading.sizes = page.page_sizes.clone();
            view.preview = Some(Preview::from_page(page).unwrap());
            view.books[0].state.zoom = Zoom::Percent(100);
            view.loading = true;
        });
        window.render_frame(cx);
    })
    .unwrap();
    cx.update_window(handle, |_, window, cx| {
        let scales = view.read(cx).raster.requested_scales();
        assert!(!scales.is_empty());
        assert!(scales.iter().all(|s| (*s - 8. / 3.).abs() < 0.001));
        view.update(cx, |view, _| view.books[0].state.zoom = Zoom::Percent(200));
        window.render_frame(cx);
    })
    .unwrap();
    cx.update_window(handle, |_, _, cx| {
        assert!(
            view.read(cx)
                .raster
                .requested_scales()
                .iter()
                .all(|s| (*s - 16. / 3.).abs() < 0.001)
        );
    })
    .unwrap();
    cx.simulate_window_scale_factor_change(handle, 1.);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
    })
    .unwrap();
    cx.update_window(handle, |_, _, cx| {
        let scales = view.read(cx).raster.requested_scales();
        assert!(!scales.is_empty());
        assert!(scales.iter().all(|s| (*s - 8. / 3.).abs() < 0.001));
        view.update(cx, |view, cx| view.show_home(cx));
        assert!(view.read(cx).raster.requested_scales().is_empty());
    })
    .unwrap();
}

#[gpui_kit::test]
fn zoom_mode_is_separate_from_computed_percentage_and_snaps_to_grid(cx: &mut TestAppContext) {
    let (handle, view) = app(cx);
    cx.update_window(handle, |_, window, cx| {
        view.update(cx, |view, cx| {
            view.reading.sizes = Arc::new(vec![(612., 792.); 8]);
            view.loading = true;
            view.set_zoom(Zoom::FitWidth, cx);
        });
        window.render_frame(cx);
        let fit = view.read(cx).display_zoom(window);
        assert!((fit - 768. / 612. * 75.).abs() < 0.01);
        assert!(window.find("zoom").bounds().right() <= window.find("zoom-out").bounds().left());
        assert!(
            window.find("zoom-out").bounds().right() <= window.find("zoom-percent").bounds().left()
        );
        window.press("secondary-b", cx);
        window.render_frame(cx);
        assert_eq!(view.read(cx).books[0].state.zoom, Zoom::FitWidth);
        assert!(view.read(cx).display_zoom(window) > fit);
        window.press("secondary-0", cx);
        assert_eq!(view.read(cx).books[0].state.zoom, Zoom::ActualSize);
        assert_eq!(view.read(cx).display_zoom(window), 100.);
        window.click("zoom", cx);
        for _ in 0..4 {
            window.press("down", cx);
        }
        window.press("enter", cx);
    })
    .unwrap();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert_eq!(view.read(cx).books[0].state.zoom, Zoom::Percent(100));
        assert_eq!(view.read(cx).books[0].state.zoom.label(), "自定义比例");
        assert_eq!(view.read(cx).display_zoom(window), 100.);
        window.focus(&view.read(cx).focus.clone(), cx);
        for (key, expected) in [("secondary-+", 175), ("secondary--", 150)] {
            view.update(cx, |view, cx| view.set_zoom(Zoom::Percent(167), cx));
            window.press(key, cx);
            assert_eq!(view.read(cx).books[0].state.zoom, Zoom::Percent(expected));
            assert_eq!(
                view.read(cx).reading_preferences.zoom,
                Zoom::Percent(expected)
            );
        }
        window.press("secondary-0", cx);
        window.click("zoom-out", cx);
        assert_eq!(view.read(cx).books[0].state.zoom, Zoom::Percent(75));
        window.press("secondary-9", cx);
        assert_eq!(view.read(cx).books[0].state.zoom, Zoom::FitPage);
        window.click("zoom-in", cx);
        assert!(matches!(
            view.read(cx).books[0].state.zoom,
            Zoom::Percent(_)
        ));
    })
    .unwrap();
}

#[gpui_kit::test]
fn actual_size_and_custom_100_restore_as_distinct_modes(cx: &mut TestAppContext) {
    let (handle, view) = app(cx);
    cx.update_window(handle, |_, _, cx| {
        view.update(cx, |view, cx| {
            for zoom in [
                Zoom::FitPage,
                Zoom::FitWidth,
                Zoom::ActualSize,
                Zoom::Percent(100),
                Zoom::Percent(167),
            ] {
                view.set_zoom(zoom, cx);
                let state =
                    serde_json::from_slice(&serde_json::to_vec(&view.snapshot()).unwrap()).unwrap();
                view.set_zoom(Zoom::FitPage, cx);
                view.restore(state, cx);
                assert_eq!(view.books[0].state.zoom, zoom);
                assert_eq!(view.reading_preferences.zoom, zoom);
            }
            view.set_zoom(Zoom::Percent(100), cx);
            let mut legacy = view.snapshot();
            legacy.version = 4;
            view.restore(legacy, cx);
            assert_eq!(view.books[0].state.zoom, Zoom::ActualSize);
            assert_eq!(view.reading_preferences.zoom, Zoom::ActualSize);
        });
    })
    .unwrap();
}

#[gpui_kit::test]
fn library_row_opens_and_secondary_controls_do_not(cx: &mut TestAppContext) {
    let (handle, view) = app(cx);
    cx.update_window(handle, |_, window, cx| {
        view.update(cx, |view, cx| view.show_home(cx));
        window.render_frame(cx);
        window.click(("favorite", 0usize), cx);
        assert!(view.read(cx).books[0].favorite);
        assert!(view.read(cx).screen == Screen::Home);
        window.render_frame(cx);
        window.click(("category", 0usize), cx);
        assert!(view.read(cx).screen == Screen::Home);
        window.press("escape", cx);
        window.render_frame(cx);
        window.click(("library-open", 0usize), cx);
        assert!(view.read(cx).screen == Screen::Reader);
    })
    .unwrap();
}
