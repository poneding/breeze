use super::*;
use breeze_core::Rotation;
use gpui_kit::InputEvent as _;
use gpui_kit::test::TestWindowExt;

pub(super) fn app(cx: &mut TestAppContext) -> (AnyWindowHandle, Entity<Breeze>) {
    // This integration fixture owns a real PDF worker thread. Allow its wakeups
    // instead of treating them as deterministic scheduler violations.
    cx.background_executor.allow_parking();
    cx.update(|cx| {
        gpui_kit::init(cx);
        appearance::init(cx).unwrap();
        commands::bind(cx);
        let config = ThemePreference::System
            .config(WindowAppearance::Light, cx)
            .unwrap();
        style::apply(&config, cx);
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
                    view.books.push(Book {
                        source: Source::Sample,
                        title: "Field notes".into(),
                        favorite: false,
                        categories: Vec::new(),
                        state: ReaderState::default(),
                        opened: false,
                        last_opened: 0,
                        annotations: DocumentAnnotations::default(),
                    });
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
        assert!(split.read(cx).sizes()[0] >= px(reader::NAVIGATION_MIN_WIDTH));
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
fn titlebar_and_theme_dropdown_stay_in_sync(cx: &mut TestAppContext) {
    let (handle, view) = app(cx);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.click("settings", cx);
        assert!(view.read(cx).screen == Screen::Settings);
        window.render_frame(cx);
        window.click("theme-select", cx);
        window.press("down", cx);
        window.press("down", cx);
        window.press("enter", cx);
    })
    .unwrap();
    // Select confirmation is deferred until after the enclosing App update.
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        let selected = ThemePreference::Theme("Aurora Light".into());
        assert_eq!(view.read(cx).theme_preference, selected);
        assert_eq!(
            view.read(cx).theme_select.read(cx).selected_value(),
            Some(&selected)
        );
        assert!(!Theme::global(cx).is_dark());
        let config = selected.config(WindowAppearance::Dark, cx).unwrap();
        let mut expected = Theme::default();
        expected.apply_config(&config);
        assert_eq!(
            Theme::global(cx).colors,
            expected.colors,
            "Breeze must not override built-in colors"
        );
        assert_eq!(
            Theme::global(cx).tokens,
            expected.tokens,
            "upstream gradients must remain intact"
        );
        let p = view.read(cx).p(cx);
        assert_eq!(p.bg, expected.background);
        assert_eq!(p.panel, expected.title_bar);
        assert_eq!(p.sidebar, expected.sidebar);
        assert_eq!(p.accent, expected.primary);
        assert_eq!(Theme::global(cx).font_size, px(15.));

        view.update(cx, |view, cx| {
            view.set_appearance(ThemePreference::Theme("Twilight".into()), window, cx);
        });
        window.render_frame(cx);
        window.click("theme", cx);
        window.render_frame(cx);
        assert_eq!(view.read(cx).theme_preference, ThemePreference::System);
        assert_eq!(
            view.read(cx).theme_select.read(cx).selected_value(),
            Some(&ThemePreference::System)
        );
        window.focus(&view.read(cx).focus.clone(), cx);
        window.press("secondary-shift-t", cx);
        window.render_frame(cx);
        assert_eq!(Theme::global(cx).theme_name().as_ref(), "Default Light");
        assert!(!Theme::global(cx).is_dark());
        assert_eq!(
            view.read(cx).theme_select.read(cx).selected_value(),
            Some(&ThemePreference::Theme("Default Light".into()))
        );
        window.click("home-tab", cx);
        assert!(view.read(cx).screen == Screen::Home);
    })
    .unwrap();
}

#[gpui_kit::test]
fn named_theme_restores_with_unchanged_document_state(cx: &mut TestAppContext) {
    let (handle, view) = app(cx);
    cx.update_window(handle, |_, window, cx| {
        let before = view.read(cx).snapshot();
        view.update(cx, |view, cx| {
            view.set_appearance(
                ThemePreference::Theme("Catppuccin Mocha".into()),
                window,
                cx,
            );
        });
        let saved = view.read(cx).snapshot();
        assert_eq!(saved.books, before.books);
        let restored: SavedState =
            serde_json::from_slice(&serde_json::to_vec(&saved).unwrap()).unwrap();
        view.update(cx, |view, cx| {
            view.set_appearance(ThemePreference::System, window, cx);
            view.restore(restored, cx);
        });
        window.render_frame(cx);
        assert_eq!(view.read(cx).snapshot(), saved);
        assert_eq!(
            view.read(cx).theme_select.read(cx).selected_value(),
            Some(&saved.preference)
        );
        assert_eq!(Theme::global(cx).theme_name().as_ref(), "Catppuccin Mocha");
    })
    .unwrap();
}

#[gpui_kit::test]
fn every_gallery_theme_applies_without_changing_documents(cx: &mut TestAppContext) {
    let (handle, view) = app(cx);
    cx.update_window(handle, |_, window, cx| {
        let before = view.read(cx).snapshot().books;
        for preference in ThemePreference::options(cx) {
            let config = preference.config(window.appearance(), cx).unwrap();
            let mut expected = Theme::default();
            expected.apply_config(&config);
            view.update(cx, |view, cx| {
                view.set_appearance(preference.clone(), window, cx)
            });
            assert_eq!(Theme::global(cx).theme_name(), &config.name);
            assert_eq!(Theme::global(cx).mode, config.mode);
            assert_eq!(Theme::global(cx).colors, expected.colors, "{}", config.name);
            assert_eq!(Theme::global(cx).tokens, expected.tokens, "{}", config.name);
            assert_eq!(view.read(cx).snapshot().books, before);
        }
    })
    .unwrap();
}

#[gpui_kit::test]
fn theme_search_selects_a_dark_gallery_entry(cx: &mut TestAppContext) {
    let (handle, view) = app(cx);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.click("settings", cx);
        window.render_frame(cx);
        window.click("theme-select", cx);
        window.render_frame(cx);
        window.input("Tokyo Night", cx);
    })
    .unwrap();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.press("enter", cx);
    })
    .unwrap();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        let selected = ThemePreference::Theme("Tokyo Night".into());
        assert_eq!(view.read(cx).theme_preference, selected);
        assert_eq!(
            view.read(cx).theme_select.read(cx).selected_value(),
            Some(&selected)
        );
        assert_eq!(
            view.read(cx).theme_select.read(cx).selected_index(cx),
            Some(gpui_kit::component::IndexPath::new(23).section(2))
        );
        assert!(Theme::global(cx).is_dark());
        // Reopening and cancelling must keep the committed theme, not a filtered row index.
        window.click("theme-select", cx);
        window.render_frame(cx);
        window.press("escape", cx);
    })
    .unwrap();
    cx.update_window(handle, |_, _, cx| {
        assert_eq!(
            view.read(cx).theme_preference,
            ThemePreference::Theme("Tokyo Night".into())
        );
    })
    .unwrap();
}

fn rendered_sample() -> breeze_pdf::Page {
    let worker = PdfWorker::new().unwrap();
    futures::executor::block_on(worker.request(Source::Sample, 0).unwrap())
        .unwrap()
        .unwrap()
}
/// Seeds one ready page the way a completed load leaves the reader.
fn seed_rendered_page(view: &mut Breeze, page: breeze_pdf::Page) {
    view.reading.source = Some(Source::Sample);
    view.reading.fingerprint = page.fingerprint.clone();
    view.reading.sizes = page.page_sizes.clone();
    let preview = Preview::from_page(page).unwrap();
    view.reading.insert(0, preview.clone());
    view.preview = Some(preview);
    view.books[0].state.layout = ReadingLayout::Single;
    view.books[0].state.zoom = Zoom::Percent(100);
    view.loading = true;
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
            (7, Kind::Ellipse),
            (8, Kind::Rectangle),
            (9, Kind::Line),
            (10, Kind::Squiggle),
            (11, Kind::Arrow),
        ] {
            view.update(cx, |view, _| {
                view.tool = tool;
                view.books[0].annotations.history.items.clear();
                view.annotation_editing.clear();
            });
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
        assert_eq!(view.read(cx).books[0].annotations.history.items.len(), 1);
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
            assert_eq!(view.books[0].categories, Vec::<usize>::new());
            view.category_form = true;
            view.category_input
                .update(cx, |input, cx| input.set_value("  Research  ", window, cx));
            view.save_category(cx);
            let id = view.categories[0].id;
            assert_eq!(view.categories[0].name, "Research");
            view.books[0].categories = vec![id];
            view.books[0].state.layout = ReadingLayout::Spread;
            let saved = view.snapshot();
            let saved = serde_json::from_slice(&serde_json::to_vec(&saved).unwrap()).unwrap();
            view.restore(saved, cx);
            assert_eq!(view.books[0].state.layout, ReadingLayout::Spread);
            assert_eq!(view.books[0].categories, vec![id]);
            view.category_edit = Some(id);
            view.category_input
                .update(cx, |input, cx| input.set_value("Reading", window, cx));
            view.save_category(cx);
            assert_eq!(view.category_name(id), "Reading");
            let next = view.next_category_id;
            view.delete_category(id, cx);
            assert!(view.categories.is_empty());
            assert_eq!(view.books[0].categories, Vec::<usize>::new());
            assert_eq!(view.next_category_id, next);
            assert_eq!(view.books.len(), 1);
            let mut legacy = view.snapshot();
            legacy.version = 1;
            legacy.books[0].category = 2;
            view.restore(legacy, cx);
            assert_eq!(view.categories.len(), 1);
            assert_eq!(view.categories[0].id, 2);
            assert_eq!(view.books[0].categories, vec![2]);
            assert_eq!(view.snapshot().version, 7);
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
fn navigation_tabs_follow_the_current_outline(cx: &mut TestAppContext) {
    let mut page = rendered_sample();
    assert!(!page.outline.is_empty());
    let outlined = Preview::from_page(page.clone()).unwrap();
    page.outline = Arc::new(Vec::new());
    let without_outline = Preview::from_page(page.clone()).unwrap();
    let (handle, view) = app(cx);
    cx.update_window(handle, |_, window, cx| {
        view.update(cx, |view, _| {
            view.reading.sizes = page.page_sizes;
            view.preview = Some(outlined.clone());
            view.loading = true;
        });
        window.render_frame(cx);
        assert_eq!(window.within("nav-tabs").find(0usize).label(), Some("目录"));
        assert_eq!(
            window.within("nav-tabs").find(1usize).label(),
            Some("缩略图")
        );
        assert_eq!(
            window.find("navigation-header").bounds().size.height,
            px(38.)
        );

        view.update(cx, |view, _| {
            view.reading.insert(0, outlined.clone());
            view.preview = None;
        });
        window.render_frame(cx);
        assert_eq!(window.within("nav-tabs").find(0usize).label(), Some("目录"));
        assert!(window.find(("outline", 0usize)).visible());

        view.update(cx, |view, _| view.preview = Some(without_outline.clone()));
        window.render_frame(cx);
        assert_eq!(
            window.within("nav-tabs").find(0usize).label(),
            Some("缩略图")
        );
        assert_eq!(
            window.within("nav-tabs").find(0usize).selected(),
            Some(true)
        );
        assert!(window.within("nav-tabs").try_find(1usize).is_none());
        assert!(window.find(("thumbnail", 0usize)).visible());
        assert!(
            !view.read(cx).thumbs,
            "automatic fallback preserves the preference"
        );
        window.press("secondary-shift-o", cx);
        assert!(window.find(("thumbnail", 0usize)).visible());
        window.within("nav-tabs").click(0usize, cx);
        assert!(view.read(cx).thumbs);

        view.update(cx, |view, _| view.preview = Some(outlined.clone()));
        window.render_frame(cx);
        assert_eq!(
            window.within("nav-tabs").find(1usize).selected(),
            Some(true)
        );
        window.within("nav-tabs").click(0usize, cx);
        assert!(window.find(("outline", 0usize)).visible());
        view.update(cx, |view, _| view.preview = Some(without_outline));
        window.render_frame(cx);
        view.update(cx, |view, _| view.preview = Some(outlined));
        window.render_frame(cx);
        assert_eq!(
            window.within("nav-tabs").find(0usize).selected(),
            Some(true)
        );
        assert!(window.find(("outline", 0usize)).visible());
    })
    .unwrap();
}

#[gpui_kit::test]
fn thumbnail_grid_reflows_keeps_its_anchor_and_reaches_the_last_page(cx: &mut TestAppContext) {
    let page = rendered_sample();
    let (handle, view) = app(cx);
    cx.update_window(handle, |_, window, cx| {
        view.update(cx, |view, _| {
            view.reading.source = Some(Source::Sample);
            view.reading.sizes = Arc::new(vec![page.points; 121]);
            view.preview = Some(Preview::from_page(page).unwrap());
            view.books[0].state.navigation.set_total(121);
            view.thumbs = true;
            // Geometry and navigation are deterministic; async loading has its own regression.
            view.loading = true;
        });
        window.render_frame(cx);
    })
    .unwrap();
    for (width, columns) in [(168., 1usize), (208., 1), (280., 2), (360., 3)] {
        cx.update_window(handle, |_, window, cx| {
            let split = view.read(cx).navigation_split.clone();
            split.update(cx, |state, cx| state.resize_panel(0, px(width), window, cx));
        })
        .unwrap();
        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            let panel = window.find("navigation-content").bounds();
            let first = window.find(("thumbnail", 0usize)).bounds();
            assert!(first.size.width <= px(144.));
            for page in 0..columns {
                let card = window.find(("thumbnail", page)).bounds();
                let image = window.find(("thumbnail-image", page)).bounds();
                assert_eq!(card.top(), first.top());
                assert!(card.left() >= panel.left() && card.right() <= panel.right());
                assert!(image.left() > card.left() && image.right() < card.right());
                assert!(image.top() > card.top() && image.bottom() < card.bottom());
                if page > 0 {
                    assert!(
                        card.left()
                            >= window.find(("thumbnail", page - 1)).bounds().right() + px(8.)
                    );
                }
            }
            assert!(window.find(("thumbnail", columns)).bounds().top() > first.bottom());
            assert!(window.try_find(("thumbnail", 120usize)).is_none());
            assert_eq!(view.read(cx).books[0].state.navigation.page(), 0);
        })
        .unwrap();
    }
    cx.update_window(handle, |_, window, cx| {
        view.read(cx).reading.thumbs_scroll.scroll_to_bottom();
        window.render_frame(cx);
        let last = window.find(("thumbnail", 120usize)).bounds();
        let previous_row = window.find(("thumbnail", 117usize)).bounds();
        assert!(window.find(("thumbnail", 120usize)).visible());
        assert!(window.try_find(("thumbnail", 121usize)).is_none());
        assert_eq!(last.left(), previous_row.left());
        assert_eq!(last.size, previous_row.size);
        view.read(cx).scroll_thumbnail_to_page(60);
        window.render_frame(cx);
        assert!(window.find(("thumbnail", 60usize)).visible());
        view.read(cx)
            .reading
            .thumbs_scroll
            .scroll_to_item_strict(20, ScrollStrategy::Top);
        window.render_frame(cx);
    })
    .unwrap();
    for width in [168., 280., 360.] {
        cx.update_window(handle, |_, window, cx| {
            let split = view.read(cx).navigation_split.clone();
            split.update(cx, |state, cx| state.resize_panel(0, px(width), window, cx));
        })
        .unwrap();
        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            let card = window.find(("thumbnail", 60usize)).bounds();
            let panel = window.find("navigation-content").bounds();
            assert!(
                (card.top() - panel.top() - px(4.)).abs() < px(1.),
                "page 61 moved during resize to {width}: card {card:?}, panel {panel:?}"
            );
            assert_eq!(view.read(cx).books[0].state.navigation.page(), 0);
        })
        .unwrap();
    }
    cx.update_window(handle, |_, window, cx| {
        window.click(("thumbnail", 61usize), cx);
        assert_eq!(view.read(cx).books[0].state.navigation.page(), 61);
        assert_eq!(
            view.read(cx)
                .reading
                .thumbs_scroll
                .logical_scroll_top_index(),
            20
        );
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
fn tab_close_is_hover_only_and_does_not_activate_its_document(cx: &mut TestAppContext) {
    let (handle, view) = app(cx);
    cx.update_window(handle, |_, window, cx| {
        view.update(cx, |view, _| {
            view.books.push(Book {
                source: Source::Sample,
                title: "Second".into(),
                favorite: false,
                categories: Vec::new(),
                state: ReaderState::default(),
                opened: true,
                last_opened: 0,
                annotations: DocumentAnnotations::default(),
            });
        });
        window.render_frame(cx);
        window.hover("home-tab", cx);
        assert!(!window.find(("close", 1usize)).visible());
        let title = window.find(("document-tab-title", 1usize)).bounds();
        window.hover(("document", 1usize), cx);
        assert!(window.find(("close", 1usize)).visible());
        assert_eq!(window.find(("document-tab-title", 1usize)).bounds(), title);
        let before = window.find(("document", 1usize)).bounds();
        let home = window.find("home-tab").bounds();
        let close = window.find(("close", 1usize)).bounds();
        assert_eq!(before.size.height, home.size.height);
        assert_eq!(before.origin.y, home.origin.y);
        assert!(close.right() <= before.right());
        assert!(before.right() - close.right() <= px(4.));
        assert!(before.right() - close.right() >= px(2.));
        assert_eq!(close.size.width, close.size.height);
        assert!(close.size.height < before.size.height);
        assert!(close.origin.y > before.origin.y);
        assert!(close.bottom() < before.bottom());
        assert!(title.right() > close.left());
        let overlay = window.find(("document-tab-close-overlay", 1usize)).bounds();
        assert!(overlay.left() < close.left());
        assert!(overlay.right() <= before.right());
        window.hover(("close", 1usize), cx);
        assert!(window.find(("close", 1usize)).visible());
        window.click(("close", 1usize), cx);
        assert!(!view.read(cx).books[1].opened);
        assert_eq!(view.read(cx).active, 0);
        assert!(before.size.width > px(0.));
    })
    .unwrap();
}

#[gpui_kit::test]
fn document_tabs_compress_without_covering_titlebar_actions(cx: &mut TestAppContext) {
    let (handle, view) = app(cx);
    for count in [1usize, 3, 16, 60] {
        cx.update_window(handle, |_, _, cx| {
            view.update(cx, |view, _| {
                view.books.truncate(1);
                view.books.extend((1..count).map(|index| Book {
                    source: Source::Sample,
                    title: format!("Long document title number {index}"),
                    favorite: false,
                    categories: Vec::new(),
                    state: ReaderState::default(),
                    opened: true,
                    last_opened: 0,
                    annotations: DocumentAnnotations::default(),
                }));
            });
        })
        .unwrap();
        let mut previous_width = None;
        // The intermediate sizes also remove the 102 px occupied by Kit's
        // three Windows/Linux control buttons from the available content width.
        for width in [1000., 898., 640., 538., 400., 298.] {
            cx.simulate_window_resize(handle, size(px(width), px(700.)));
            cx.update_window(handle, |_, window, cx| {
                window.render_frame(cx);
                let content = window.find("titlebar-content").bounds();
                let tabs = window.find("document-tabs").bounds();
                let actions = window.find("titlebar-actions").bounds();
                let add = window.find("document-tab-add").bounds();
                let plus = window.find("add-document");
                let divider = window.find("document-tab-divider").bounds();
                let first = window.find(("document", 0usize)).bounds();
                assert!(content.right() <= px(width), "{count} tabs at {width}px");
                assert!(tabs.right() <= actions.left());
                assert!(actions.right() <= content.right());
                assert!(tabs.right() <= add.left());
                assert!(add.left() - tabs.right() <= px(4.));
                assert!(add.right() <= actions.left());
                assert!(plus.visible());
                assert!(divider.right() <= plus.bounds().left());
                assert!(plus.bounds().right() <= add.right());
                assert!(window.try_find("open").is_none());
                for id in ["theme", "settings"] {
                    let button = window.find(id);
                    assert!(button.visible());
                    assert!(button.bounds().left() >= actions.left());
                    assert!(button.bounds().right() <= actions.right());
                }
                if (count <= 3 && width >= 640.) || (count == 16 && width >= 898.) {
                    let last = window.find(("document", count - 1)).bounds();
                    assert!(last.right() <= tabs.right() + px(1.));
                    assert!((first.size.width - last.size.width).abs() < px(1.));
                }
                assert!(first.size.width >= px(40.));
                if let Some(previous) = previous_width {
                    assert!(first.size.width <= previous);
                }
                previous_width = Some(first.size.width);
                window.click("settings", cx);
                assert!(view.read(cx).screen == Screen::Settings);
            })
            .unwrap();
        }
    }
    cx.update_window(handle, |_, window, cx| {
        view.update(cx, |view, _| {
            for book in &mut view.books {
                book.opened = false;
            }
        });
        window.render_frame(cx);
        assert!(window.find("add-document").visible());
        assert!(window.find("document-tabs").bounds().size.width <= px(4.));
        window.click("add-document", cx);
    })
    .unwrap();
    assert!(cx.did_prompt_for_paths());
    cx.simulate_path_prompt_response(|_| None);
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
fn rotation_controls_turn_the_page_a_quarter_turn_per_click(cx: &mut TestAppContext) {
    let page = rendered_sample();
    let (handle, view) = app(cx);
    let mut upright = size(px(0.), px(0.));
    cx.update_window(handle, |_, window, cx| {
        view.update(cx, |view, _| seed_rendered_page(view, page));
        window.render_frame(cx);
        upright = window.find("page-surface").bounds().size;
        window.click("rotate-right", cx);
    })
    .unwrap();
    cx.update_window(handle, |_, window, cx| {
        assert_eq!(view.read(cx).books[0].state.rotation, Rotation::Right);
        window.render_frame(cx);
        // One quarter turn exchanges the displayed width and height.
        let turned = window.find("page-surface").bounds().size;
        assert!((f32::from(turned.width) - f32::from(upright.height)).abs() < 1.);
        assert!((f32::from(turned.height) - f32::from(upright.width)).abs() < 1.);
        window.click("rotate-left", cx);
    })
    .unwrap();
    cx.update_window(handle, |_, window, cx| {
        assert_eq!(view.read(cx).books[0].state.rotation, Rotation::None);
        window.render_frame(cx);
        let restored = window.find("page-surface").bounds().size;
        assert!((f32::from(restored.width) - f32::from(upright.width)).abs() < 1.);
        assert!((f32::from(restored.height) - f32::from(upright.height)).abs() < 1.);
        // Three left turns equal one right turn, and the rotation is per document.
        for _ in 0..3 {
            window.click("rotate-left", cx);
        }
    })
    .unwrap();
    cx.update_window(handle, |_, window, cx| {
        assert_eq!(view.read(cx).books[0].state.rotation, Rotation::Right);
        view.update(cx, |view, cx| {
            view.remember_position = false;
            let saved: SavedState =
                serde_json::from_slice(&serde_json::to_vec(&view.snapshot()).unwrap()).unwrap();
            assert_eq!(saved.books[0].rotation, Rotation::Right);
            view.books[0].state = ReaderState::default();
            view.restore(saved, cx);
            assert_eq!(view.books[0].state.rotation, Rotation::Right);
        });
        window.render_frame(cx);
        assert_eq!(view.read(cx).books[0].state.rotation, Rotation::Right);
    })
    .unwrap();
}

#[gpui_kit::test]
fn fullscreen_shows_only_the_document_and_leaves_on_escape_or_f11(cx: &mut TestAppContext) {
    let page = rendered_sample();
    let (handle, view) = app(cx);
    cx.update_window(handle, |_, window, cx| {
        view.update(cx, |view, _| seed_rendered_page(view, page));
        window.render_frame(cx);
        window.click("rotate-right", cx);
    })
    .unwrap();
    let mut turned = size(px(0.), px(0.));
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        turned = window.find("page-surface").bounds().size;
        window.click("fullscreen", cx);
    })
    .unwrap();
    cx.update_window(handle, |_, window, cx| {
        assert!(view.read(cx).full_screen);
        assert!(window.is_fullscreen());
        window.render_frame(cx);
        // Immersive reading hides the title bar, both panels, the toolbar and the status bar.
        for id in [
            "titlebar-content",
            "reader-toolbar",
            "navigation-header",
            "document-tabs",
            "status-scroll",
        ] {
            assert!(window.try_find(id).is_none(), "{id} must be hidden");
        }
        // The turned document keeps its orientation without any status control.
        let surface = window.find("page-surface").bounds().size;
        assert!((f32::from(surface.width) - f32::from(turned.width)).abs() < 1.);
        assert!((f32::from(surface.height) - f32::from(turned.height)).abs() < 1.);
        assert!(window.try_find("rotate-right").is_none());
        window.press("f11", cx);
    })
    .unwrap();
    cx.update_window(handle, |_, window, cx| {
        assert!(!view.read(cx).full_screen);
        assert!(!window.is_fullscreen());
        window.render_frame(cx);
        assert!(window.find("status-scroll").visible());
        assert!(window.find("titlebar-content").visible());
        assert_eq!(view.read(cx).books[0].state.rotation, Rotation::Right);
        window.click("fullscreen", cx);
    })
    .unwrap();
    cx.update_window(handle, |_, window, cx| {
        assert!(view.read(cx).full_screen);
        window.render_frame(cx);
        assert!(window.try_find("status-scroll").is_none());
        // Escape leaves immersive reading when no text annotation is being edited.
        window.press("escape", cx);
    })
    .unwrap();
    cx.update_window(handle, |_, window, cx| {
        assert!(!view.read(cx).full_screen);
        window.render_frame(cx);
        assert!(window.find("status-scroll").visible());
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

#[gpui_kit::test]
fn library_columns_align_and_empty_state_stays_empty(cx: &mut TestAppContext) {
    let (handle, view) = app(cx);
    cx.update_window(handle, |_, window, cx| {
        view.update(cx, |view, cx| view.show_home(cx));
        window.render_frame(cx);
        for (heading, cell) in [
            ("category-heading", "category"),
            ("position-heading", "position"),
        ] {
            let header = window.find(heading).bounds();
            let row = window.find((cell, 0usize)).bounds();
            assert!((header.origin.x - row.origin.x).abs() < px(1.));
            assert!((header.size.width - row.size.width).abs() < px(1.));
        }
        window.click(("forget", 0usize), cx);
        assert!(view.read(cx).books.is_empty());
        assert!(view.read(cx).screen == Screen::Home);
        view.update(cx, |view, cx| {
            view.restore(view.snapshot(), cx);
            assert!(view.books.is_empty());
        });
        window.render_frame(cx);
        let fresh = cx.new(|cx| Breeze::new(PdfWorker::new().unwrap(), window, cx));
        assert!(fresh.read(cx).books.is_empty());
    })
    .unwrap();
}

#[gpui_kit::test]
fn multiple_categories_survive_restore_and_independent_deletion(cx: &mut TestAppContext) {
    let (handle, view) = app(cx);
    cx.update_window(handle, |_, window, cx| {
        view.update(cx, |view, cx| {
            for name in ["Research", "Work"] {
                view.category_input
                    .update(cx, |input, cx| input.set_value(name, window, cx));
                view.save_category(cx);
            }
            view.toggle_category(0, 1, cx);
            view.toggle_category(0, 2, cx);
            assert_eq!(view.books[0].categories, vec![1, 2]);
            let state =
                serde_json::from_slice(&serde_json::to_vec(&view.snapshot()).unwrap()).unwrap();
            view.restore(state, cx);
            assert_eq!(view.books[0].categories, vec![1, 2]);
            view.delete_category(1, cx);
            assert_eq!(view.books[0].categories, vec![2]);
            view.show_home(cx);
            view.library_filter = 5;
        });
        window.render_frame(cx);
        assert!(window.find(("library-open", 0usize)).bounds().size.width > px(0.));
        view.update(cx, |view, cx| {
            view.toggle_category(0, 1, cx);
            assert_eq!(view.books[0].categories, vec![2]);
            for version in 2..=5 {
                let mut state = view.snapshot();
                state.version = version;
                state.books[0].category = 2;
                state.books[0].categories.clear();
                view.restore(state, cx);
                assert_eq!(view.books[0].categories, vec![2]);
            }
            view.toggle_category(0, 2, cx);
            assert!(view.books[0].categories.is_empty());
            view.toggle_category(0, 2, cx);
            view.toggle_category(0, 0, cx);
            assert!(view.books[0].categories.is_empty());
        });
    })
    .unwrap();
}

#[gpui_kit::test]
fn category_colors_and_inline_hover_actions_work(cx: &mut TestAppContext) {
    use breeze_core::library::CategoryColor;
    let (handle, view) = app(cx);
    cx.update_window(handle, |_, window, cx| {
        view.update(cx, |view, cx| view.show_home(cx));
        window.render_frame(cx);
        window.click("new-category", cx);
        window.render_frame(cx);
        for color in CategoryColor::ALL {
            window.click(("category-color", color as usize), cx);
            assert_eq!(view.read(cx).category_color, color);
        }
        view.update(cx, |view, cx| {
            view.category_input
                .update(cx, |input, cx| input.set_value("Colored tag", window, cx));
        });
        window.render_frame(cx);
        window.click("save-category", cx);
        assert_eq!(view.read(cx).categories[0].color, CategoryColor::Pink);
        window.render_frame(cx);
        window.hover("home-tab", cx);
        assert!(!window.find(("rename-category", 1usize)).visible());
        assert!(!window.find(("delete-category", 1usize)).visible());
        window.hover(("category-row", 1usize), cx);
        assert!(window.find(("rename-category", 1usize)).visible());
        assert!(window.find(("delete-category", 1usize)).visible());
        let row = window.find(("category-row", 1usize)).bounds();
        let rename = window.find(("rename-category", 1usize)).bounds();
        let delete = window.find(("delete-category", 1usize)).bounds();
        assert_eq!(rename.origin.y, delete.origin.y);
        assert!(rename.origin.y >= row.origin.y && delete.bottom() <= row.bottom());
        assert!(delete.right() <= row.right());
        window.click(("rename-category", 1usize), cx);
        assert_eq!(view.read(cx).category_color, CategoryColor::Pink);
        view.update(cx, |view, cx| {
            view.category_input
                .update(cx, |input, cx| input.set_value("Renamed", window, cx));
        });
        window.render_frame(cx);
        window.click("save-category", cx);
        view.update(cx, |view, cx| {
            let state =
                serde_json::from_slice(&serde_json::to_vec(&view.snapshot()).unwrap()).unwrap();
            view.restore(state, cx);
            assert_eq!(view.categories[0].name, "Renamed");
            assert_eq!(view.categories[0].color, CategoryColor::Pink);
        });
        window.render_frame(cx);
        window.hover(("category-row", 1usize), cx);
        window.click(("delete-category", 1usize), cx);
        assert!(view.read(cx).categories.is_empty());
        assert_eq!(view.read(cx).books.len(), 1);
    })
    .unwrap();
}

#[gpui_kit::test]
fn category_popover_dismisses_and_keeps_invalid_input_open(cx: &mut TestAppContext) {
    let (handle, view) = app(cx);
    cx.update_window(handle, |_, window, cx| {
        view.update(cx, |view, cx| view.show_home(cx));
        window.render_frame(cx);
        let before = window.find("unassigned-category").bounds();
        window.click("new-category", cx);
        window.render_frame(cx);
        assert!(view.read(cx).category_form);
        assert!(window.find("category-editor").visible());
        assert_eq!(window.find("unassigned-category").bounds(), before);
        window.press("escape", cx);
        window.render_frame(cx);
        assert!(!view.read(cx).category_form);
        window.click("new-category", cx);
        window.render_frame(cx);
        window.click("save-category", cx);
        assert!(view.read(cx).category_form);
        assert!(!view.read(cx).message.is_empty());
        window.render_frame(cx);
        window.click("cancel-category", cx);
        assert!(!view.read(cx).category_form);
    })
    .unwrap();
}

#[gpui_kit::test]
fn category_secondary_actions_do_not_select_the_row(cx: &mut TestAppContext) {
    let (handle, view) = app(cx);
    cx.update_window(handle, |_, window, cx| {
        view.update(cx, |view, cx| {
            for name in ["Research", "Work"] {
                view.category_input
                    .update(cx, |input, cx| input.set_value(name, window, cx));
                view.save_category(cx);
            }
            view.show_home(cx);
            view.library_filter = 0;
        });
        window.render_frame(cx);
        window.hover(("category-row", 1usize), cx);
        window.click(("rename-category", 1usize), cx);
        window.render_frame(cx);
        assert!(view.read(cx).category_form);
        assert_eq!(view.read(cx).library_filter, 0);
        window.press("escape", cx);
        window.render_frame(cx);
        window.hover(("category-row", 1usize), cx);
        window.click(("delete-category", 1usize), cx);
        assert_eq!(view.read(cx).library_filter, 0);
        assert_eq!(view.read(cx).categories.len(), 1);
        window.render_frame(cx);
        window.click(("category-nav", 2usize), cx);
        assert_eq!(view.read(cx).library_filter, 5);
    })
    .unwrap();
}

#[gpui_kit::test]
fn grid_metadata_is_centered_and_actions_only_show_on_hover(cx: &mut TestAppContext) {
    let (handle, view) = app(cx);
    cx.update_window(handle, |_, window, cx| {
        view.update(cx, |view, cx| {
            view.show_home(cx);
            view.library_grid = true;
        });
        window.render_frame(cx);
        let card = window.find(("library-open", 0usize)).bounds();
        for name in ["grid-title", "grid-position", "category"] {
            let metadata = window.find((name, 0usize)).bounds();
            assert!((metadata.center().x - card.center().x).abs() < px(1.));
        }
        window.hover("home-tab", cx);
        assert!(!window.find(("favorite", 0usize)).visible());
        window.hover(("library-open", 0usize), cx);
        assert!(window.find(("favorite", 0usize)).visible());
        window.click(("favorite", 0usize), cx);
        assert!(view.read(cx).books[0].favorite);
        assert!(view.read(cx).screen == Screen::Home);
        window.hover("home-tab", cx);
        assert!(!window.find(("favorite", 0usize)).visible());
        assert_eq!(window.find(("library-open", 0usize)).bounds(), card);
    })
    .unwrap();
}

#[gpui_kit::test]
fn library_empty_component_is_centered_in_both_views(cx: &mut TestAppContext) {
    let (handle, view) = app(cx);
    cx.update_window(handle, |_, window, cx| {
        let books = view.update(cx, |view, cx| {
            view.show_home(cx);
            std::mem::take(&mut view.books)
        });
        for grid in [false, true] {
            for filter in [0, 1, 2, 3] {
                view.update(cx, |view, _| {
                    view.library_grid = grid;
                    view.library_filter = filter;
                });
                window.render_frame(cx);
                let empty = window.find("library-empty");
                assert!(empty.visible());
                let content = window.find("home-content").bounds();
                assert!((empty.bounds().center().x - content.center().x).abs() < px(1.));
                assert!(empty.bounds().center().y > content.origin.y + px(100.));
            }
        }
        view.update(cx, |view, cx| {
            view.books = books;
            view.library_input.update(cx, |input, cx| {
                input.set_value("no matching document", window, cx)
            });
        });
        for grid in [false, true] {
            view.update(cx, |view, _| view.library_grid = grid);
            window.render_frame(cx);
            assert!(window.find("library-empty").visible());
            assert_eq!(view.read(cx).books.len(), 1);
        }
        view.update(cx, |view, cx| {
            view.library_input
                .update(cx, |input, cx| input.set_value("", window, cx));
        });
        window.render_frame(cx);
        assert!(window.find(("library-open", 0usize)).visible());
    })
    .unwrap();
}

#[gpui_kit::test]
fn category_color_tags_stay_on_one_row(cx: &mut TestAppContext) {
    let (handle, view) = app(cx);
    cx.update_window(handle, |_, window, cx| {
        view.update(cx, |view, cx| view.show_home(cx));
        window.render_frame(cx);
        window.click("new-category", cx);
        window.render_frame(cx);
        let first = window.find(("category-color", 0usize)).bounds();
        for color in 1..8 {
            let bounds = window.find(("category-color", color as usize)).bounds();
            assert_eq!(bounds.origin.y, first.origin.y);
            assert!(bounds.origin.x > first.origin.x);
        }
        assert!(window.find("category-editor").bounds().size.width >= px(228.));
    })
    .unwrap();
}

#[gpui_kit::test]
fn category_editor_actions_are_right_aligned_and_ordered(cx: &mut TestAppContext) {
    let (handle, view) = app(cx);
    cx.update_window(handle, |_, window, cx| {
        view.update(cx, |view, cx| view.show_home(cx));
        window.render_frame(cx);
        window.click("new-category", cx);
        window.render_frame(cx);
        let editor = window.find("category-editor").bounds();
        let cancel = window.find("cancel-category").bounds();
        let save = window.find("save-category").bounds();
        assert_eq!(cancel.origin.y, save.origin.y);
        assert!(cancel.origin.x < save.origin.x);
        assert!(save.right() <= editor.right());
        assert!(editor.right() - save.right() <= px(2.));
        assert!(cancel.origin.x > editor.origin.x);
    })
    .unwrap();
}
