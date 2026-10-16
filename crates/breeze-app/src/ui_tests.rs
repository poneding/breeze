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
                        reflow: ReflowState::default(),
                        source: Source::Sample,
                        title: "Field notes".into(),
                        favorite: false,
                        categories: Vec::new(),
                        state: ReaderState::default(),
                        opened: false,
                        last_opened: 0,
                        annotations: DocumentAnnotations::default(),
                        markdown_edit: None,
                        markdown_editing: false,
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
        assert!(view.read(cx).home_page == HomePage::Settings);
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
fn theme_selection_and_settings_dropdown_stay_in_sync(cx: &mut TestAppContext) {
    let (handle, view) = app(cx);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.press("secondary-,", cx);
        let view_state = view.read(cx);
        assert!(view_state.screen == Screen::Home && view_state.home_page == HomePage::Settings);
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
        window.focus(&view.read(cx).focus.clone(), cx);
        window.press("secondary-shift-t", cx);
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
fn arrow_keys_preview_the_theme_until_the_row_is_confirmed(cx: &mut TestAppContext) {
    let (handle, view) = app(cx);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.press("secondary-,", cx);
        window.render_frame(cx);
        window.click("theme-select", cx);
        // Two rows down from Follow System lands on Aurora Light.
        window.press("down", cx);
        window.press("down", cx);
    })
    .unwrap();
    // The preview is applied after Kit's own key handler moved the cursor.
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert_eq!(view.read(cx).theme_preference, ThemePreference::System);
        assert_eq!(
            view.read(cx).theme_select.read(cx).selected_value(),
            Some(&ThemePreference::System),
            "a preview must not move the committed selection"
        );
        assert_eq!(Theme::global(cx).theme_name().as_ref(), "Aurora Light");
        window.press("escape", cx);
    })
    .unwrap();
    // Dismissing the dropdown drops the preview without committing it.
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert_eq!(view.read(cx).theme_preference, ThemePreference::System);
        assert_eq!(Theme::global(cx).theme_name().as_ref(), "Default Light");
        window.click("theme-select", cx);
        window.press("down", cx);
        window.press("down", cx);
        window.press("enter", cx);
    })
    .unwrap();
    // Confirming the same row is what commits it.
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        let selected = ThemePreference::Theme("Aurora Light".into());
        assert_eq!(view.read(cx).theme_preference, selected);
        assert_eq!(Theme::global(cx).theme_name().as_ref(), "Aurora Light");
        // Reopen the dropdown to preview the row after the committed one.
        window.click("theme-select", cx);
        window.press("down", cx);
    })
    .unwrap();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert_eq!(Theme::global(cx).theme_name().as_ref(), "Ayu Light");
        // Clicking outside the dropdown ends the preview as well.
        window.click("home-settings", cx);
    })
    .unwrap();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert_eq!(
            view.read(cx).theme_preference,
            ThemePreference::Theme("Aurora Light".into())
        );
        assert_eq!(Theme::global(cx).theme_name().as_ref(), "Aurora Light");
    })
    .unwrap();
}

#[gpui_kit::test]
fn leaving_settings_drops_an_unconfirmed_theme_preview(cx: &mut TestAppContext) {
    let (handle, view) = app(cx);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.press("secondary-,", cx);
        window.render_frame(cx);
        window.click("theme-select", cx);
        window.press("down", cx);
        window.press("down", cx);
    })
    .unwrap();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert_eq!(Theme::global(cx).theme_name().as_ref(), "Aurora Light");
        // The Home command unmounts the Settings page without closing the dropdown.
        window.press("secondary-shift-h", cx);
        window.render_frame(cx);
    })
    .unwrap();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert_eq!(view.read(cx).screen, Screen::Home);
        assert_eq!(view.read(cx).theme_preference, ThemePreference::System);
        assert_eq!(view.read(cx).theme_preview, None);
        assert_eq!(Theme::global(cx).theme_name().as_ref(), "Default Light");
        // The cursor is back on the committed row, so the next open starts there.
        assert_eq!(
            view.read(cx).theme_select.read(cx).selected_index(cx),
            Some(gpui_kit::component::IndexPath::new(0))
        );
    })
    .unwrap();
}

#[gpui_kit::test]
fn home_navigation_selects_about_shortcuts_and_settings(cx: &mut TestAppContext) {
    let (handle, view) = app(cx);
    cx.update_window(handle, |_, window, cx| {
        view.update(cx, |view, cx| view.show_home(cx));
        window.render_frame(cx);
        assert_eq!(view.read(cx).home_page, HomePage::Library);
        assert!(window.find("library-sidebar").visible());
        assert!(window.find("home-footer").visible());
        assert!(window.find(("library-open", 0usize)).visible());

        window.click("home-about", cx);
        window.render_frame(cx);
        assert_eq!(view.read(cx).home_page, HomePage::About);
        assert!(window.find("about-body").visible());
        assert!(window.find("brand-tile").visible());
        assert!(window.find("check-updates").visible());
        assert!(window.find("open-repository").visible());
        // The library stays reachable while a global page is open.
        assert!(window.find("library-sidebar").visible());

        window.click("home-shortcuts", cx);
        window.render_frame(cx);
        assert_eq!(view.read(cx).home_page, HomePage::Shortcuts);
        assert!(window.find("shortcuts-body").visible());

        window.click("home-settings", cx);
        window.render_frame(cx);
        assert_eq!(view.read(cx).home_page, HomePage::Settings);
        assert!(window.find("theme-select").visible());
        // The dropdown wrapper that captures arrow keys must not change its box.
        let theme = window.find("theme-select").bounds();
        let language = window.find("language-select").bounds();
        assert_eq!(theme.size, language.size);
        assert_eq!(theme.size.width, px(220.));
        assert!(window.find("remember-files").visible());
        // The association row reports what the installed app declares instead of
        // claiming a disabled or unavailable feature.
        assert!(window.try_find("file-association").is_none());
        assert!(
            window
                .within("settings-body")
                .find("default-pdf-app")
                .visible()
        );
        // Row-by-row explanations are gone; the update status lives in About.
        assert!(window.try_find("update-status").is_none());
        assert!(
            window
                .within("settings-body")
                .try_find("check-updates")
                .is_none()
        );

        window.click(("library-nav", 0usize), cx);
        window.render_frame(cx);
        assert_eq!(view.read(cx).home_page, HomePage::Library);
        assert!(window.find(("library-open", 0usize)).visible());

        // The Settings shortcut activates the same sidebar entry.
        window.press("secondary-,", cx);
        window.render_frame(cx);
        let state = view.read(cx);
        assert_eq!(state.screen, Screen::Home);
        assert_eq!(state.home_page, HomePage::Settings);

        // Each sidebar entry is a real command whose tooltip shows the platform keys.
        #[cfg(target_os = "macos")]
        let modifier = "⌘";
        #[cfg(not(target_os = "macos"))]
        let modifier = "Ctrl+";
        for (id, action, key) in [
            ("home-about", &ShowAbout as &dyn Action, "I"),
            ("home-shortcuts", &ShowShortcuts, "K"),
            ("home-settings", &Preferences, ","),
        ] {
            assert!(window.find(id).visible());
            assert!(window.find(id).bounds().size.width >= px(120.));
            let binding = window
                .highest_precedence_binding_for_action_in_context(
                    action,
                    gpui_kit::KeyContext::parse("Breeze").unwrap(),
                )
                .unwrap_or_else(|| panic!("{id} must advertise its binding"));
            let stroke = binding.keystrokes()[0].as_keystroke();
            assert_eq!(
                gpui_kit::component::kbd::Kbd::format(stroke),
                format!("{modifier}{key}"),
                "{id} tooltip must show the requested shortcut"
            );
        }
        // The three entries stack vertically in the requested order.
        let about = window.find("home-about").bounds();
        let shortcuts = window.find("home-shortcuts").bounds();
        let settings = window.find("home-settings").bounds();
        assert!(about.bottom() <= shortcuts.top());
        assert!(shortcuts.bottom() <= settings.top());
    })
    .unwrap();
}

#[gpui_kit::test]
fn home_shortcut_commands_select_their_pages(cx: &mut TestAppContext) {
    let (handle, view) = app(cx);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.press("secondary-k", cx);
        window.render_frame(cx);
        assert_eq!(view.read(cx).home_page, HomePage::Shortcuts);
        window.press("secondary-i", cx);
        window.render_frame(cx);
        assert_eq!(view.read(cx).home_page, HomePage::About);
        window.press("secondary-,", cx);
        window.render_frame(cx);
        assert_eq!(view.read(cx).home_page, HomePage::Settings);
    })
    .unwrap();
}

#[gpui_kit::test]
fn global_pages_are_bounded_and_centered(cx: &mut TestAppContext) {
    let (handle, view) = app(cx);
    cx.simulate_window_resize(handle, size(px(1600.), px(900.)));
    cx.update_window(handle, |_, window, cx| {
        for page in [HomePage::Shortcuts, HomePage::Settings] {
            view.update(cx, |view, cx| view.show_home_page(page, cx));
            window.render_frame(cx);
            let content = window.find("home-content").bounds();
            assert!(content.size.width > px(home::PAGE_MAX_WIDTH));
            let page_bounds = if page == HomePage::Shortcuts {
                window.find("shortcuts-body").bounds()
            } else {
                window.find("settings-body").bounds()
            };
            assert!(page_bounds.size.width <= px(home::PAGE_MAX_WIDTH) + px(1.));
            let left_gap = page_bounds.left() - content.left();
            let right_gap = content.right() - page_bounds.right();
            assert!(
                (left_gap - right_gap).abs() < px(2.),
                "{page:?} must be centered: {left_gap:?} vs {right_gap:?}"
            );
            // Controls keep clear of the clip edge so active borders stay visible.
            assert!(content.right() - page_bounds.right() >= px(1.));
        }
        view.update(cx, |view, cx| view.show_home_page(HomePage::About, cx));
        window.render_frame(cx);
        let body = window.find("about-body").bounds();
        let tile = window.find("brand-tile").bounds();
        let author_row = window.find("about-credit-author").bounds();
        let source_row = window.find("about-credit-source").bounds();
        let author = window.find("about-author").bounds();
        let repo = window.find("open-repository").bounds();
        assert!((tile.center().x - body.center().x).abs() < px(2.));
        // Credits share one line per entry and align their values on the left.
        assert!((author_row.left() - source_row.left()).abs() < px(1.));
        assert!((author.left() - repo.left()).abs() < px(1.));
        assert!(author_row.size.height < px(24.) && source_row.size.height < px(24.));
        assert!(author_row.bottom() <= source_row.top());
        assert!(tile.center().y - body.top() > px(80.));
        assert!(body.bottom() - source_row.bottom() > px(80.));
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
        window.press("secondary-,", cx);
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
                text: page.text,
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
        view.update(cx, |view, _| view.tool = annotation_editing::SELECT_TOOL);
        window.render_frame(cx);
        window.click_at("page-surface", point(px(40.), px(100.)), cx);
        window.press("delete", cx);
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
                text: page.text,
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
        // Drag from the left margin beside the sample's "Keep the details"
        // line to partway across "Attention is the beginning": the selection is
        // that run of reading order, drawn as one rectangle per row. The
        // sample's own user-space coordinates are y-up, so mirror y here.
        let surface = |x: f32, y: f32| {
            point(
                bounds.origin.x + px(x / page.points.0 * f32::from(bounds.size.width)),
                bounds.origin.y
                    + px((page.points.1 - y) / page.points.1 * f32::from(bounds.size.height)),
            )
        };
        window.drag(surface(20., 265.), surface(300., 168.), cx);
        let snapshot = view.read(cx);
        assert!(
            snapshot
                .selected_text
                .contains("Keep the details that help you think")
        );
        assert!(snapshot.selected_text.contains("Mark a sentence"));
        assert!(
            snapshot
                .selected_text
                .contains("Attention is the beginning")
        );
        assert!(
            !snapshot.selected_text.contains("Good reading starts"),
            "a line dragged over entirely above the start must not be selected: {:?}",
            snapshot.selected_text
        );
        assert!(
            !snapshot
                .selected_text
                .contains("A native reading experience"),
            "the selection must end where the pointer is, not at the page margin: {:?}",
            snapshot.selected_text
        );
        let rects: Vec<_> = snapshot
            .selection
            .iter()
            .flat_map(|(_, rects)| rects.clone())
            .collect();
        assert_eq!(rects.len(), 3, "one rectangle per selected row: {rects:?}");
        for pair in rects.windows(2) {
            assert!(
                pair[0].y >= pair[1].y + pair[1].height,
                "highlight rows must not overlap: {pair:?}"
            );
        }
        assert_eq!(
            rects[0].y,
            rects[1].y + rects[1].height,
            "rows of one paragraph meet on a shared edge"
        );
        assert!(
            rects[2].y + rects[2].height < rects[1].y,
            "the paragraph break stays open: {rects:?}"
        );
        // A press starts a new selection instead of keeping the old highlight.
        window.click_at("page-surface", point(px(4.), px(4.)), cx);
        assert!(view.read(cx).selection.is_empty());
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
                text: page.text,
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
            (6, Kind::Ellipse),
            (7, Kind::Rectangle),
            (8, Kind::Line),
            (9, Kind::Squiggle),
            (10, Kind::Arrow),
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
                text: page.text,
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
            assert_eq!(view.snapshot().version, 10);
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
        assert_eq!(window.find("reader-toolbar").bounds().size.height, px(34.));
        assert_eq!(window.find("status-scroll").bounds().size.height, px(32.));
        assert_eq!(window.find("home-tab").bounds().size.height, px(22.5));
        assert!(
            window.try_find("previous").is_none(),
            "the PDF status bar still shows a previous-page button"
        );
        assert!(
            window.try_find("next").is_none(),
            "the PDF status bar still shows a next-page button"
        );
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
fn panel_toggles_live_in_their_expanded_headers(cx: &mut TestAppContext) {
    let (handle, view) = app(cx);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        // Navigation is open by default, so its toggle belongs to its header.
        assert!(view.read(cx).sidebar);
        let header = window.find("navigation-header").bounds();
        let collapse = window.find("collapse").bounds();
        assert!(collapse.left() - header.left() < px(12.));
        // The closed document panel keeps its toggle at the toolbar's right end.
        let toolbar = window.find("reader-toolbar").bounds();
        let toggle = window.find("document-panel-toggle").bounds();
        assert!(toggle.right() <= toolbar.right() - px(4.));
        assert!(window.try_find("document-panel").is_none());

        // Collapsing navigation returns its toggle to the toolbar's left end.
        window.click("collapse", cx);
        window.render_frame(cx);
        assert!(!view.read(cx).sidebar);
        assert!(window.try_find("navigation-header").is_none());
        let toolbar = window.find("reader-toolbar").bounds();
        let collapse = window.find("collapse").bounds();
        assert!(collapse.left() - toolbar.left() < px(12.));

        // Opening the document panel moves the toggle to that header's right.
        window.click("document-panel-toggle", cx);
        window.render_frame(cx);
        assert!(view.read(cx).document_panel.open);
        let panel_header = window.find("document-panel-header").bounds();
        let toggle = window.find("document-panel-toggle").bounds();
        // Pinned to the header's right edge, within fractional control rounding.
        let inset = panel_header.right() - toggle.right();
        assert!(
            inset >= px(7.) && inset <= px(9.),
            "toggle inset {inset:?}, header {panel_header:?}, toggle {toggle:?}"
        );
        assert!(window.find("reader-toolbar").visible());
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
            px(34.)
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
                reflow: ReflowState::default(),
                source: Source::Sample,
                title: "Second".into(),
                favorite: false,
                categories: Vec::new(),
                state: ReaderState::default(),
                opened: true,
                last_opened: 0,
                annotations: DocumentAnnotations::default(),
                markdown_edit: None,
                markdown_editing: false,
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
        assert_eq!(before.right() - close.right(), px(2.));
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
fn document_tabs_compress_without_covering_titlebar_controls(cx: &mut TestAppContext) {
    let (handle, view) = app(cx);
    for count in [1usize, 3, 16, 60] {
        cx.update_window(handle, |_, _, cx| {
            view.update(cx, |view, _| {
                view.books.truncate(1);
                view.books.extend((1..count).map(|index| Book {
                    reflow: ReflowState::default(),
                    source: Source::Sample,
                    title: format!("Long document title number {index}"),
                    favorite: false,
                    categories: Vec::new(),
                    state: ReaderState::default(),
                    opened: true,
                    last_opened: 0,
                    annotations: DocumentAnnotations::default(),
                    markdown_edit: None,
                    markdown_editing: false,
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
                let add = window.find("document-tab-add").bounds();
                let plus = window.find("add-document");
                let divider = window.find("document-tab-divider").bounds();
                let first = window.find(("document", 0usize)).bounds();
                assert!(content.right() <= px(width), "{count} tabs at {width}px");
                assert!(tabs.right() <= add.left());
                assert!(add.left() - tabs.right() <= px(4.));
                assert!(add.right() <= content.right());
                assert!(plus.visible());
                assert!(divider.right() <= plus.bounds().left());
                assert!(plus.bounds().right() <= add.right());
                assert!(window.try_find("open").is_none());
                // Tabs shrink to a 40 px floor inside a 10 px padded, 4 px gapped
                // strip, so fit is asserted where the measured strip can host that
                // floor. Window widths cannot express this: Kit's window buttons
                // take about 102 px on Windows and Linux and none on macOS, so one
                // width leaves a different strip on each platform.
                let minimum = px(40. * count as f32 + 4. * count.saturating_sub(1) as f32 + 10.);
                if tabs.size.width >= minimum {
                    let last = window.find(("document", count - 1)).bounds();
                    assert!(
                        last.right() <= tabs.right() + px(1.),
                        "{count} tabs fit at {width}px"
                    );
                    assert!(last.size.width <= px(228.));
                } else {
                    // A strip narrower than its floor scrolls instead of covering
                    // the Open button, and the overflow has to stay reachable:
                    // from the start, scrolling to the end must bring the last tab
                    // inside the strip.
                    for delta in [px(4000.), px(-4000.)] {
                        window.scroll(
                            "document-tabs",
                            ScrollDelta::Pixels(point(delta, px(0.))),
                            cx,
                        );
                        window.render_frame(cx);
                    }
                    let last = window.find(("document", count - 1)).bounds();
                    assert!(
                        last.right() <= tabs.right() + px(1.) && last.right() > tabs.left(),
                        "{count} tabs scroll into view at {width}px"
                    );
                    assert!(last.size.width <= px(228.));
                }
                assert!(first.size.width >= px(40.));
                if let Some(previous) = previous_width {
                    assert!(first.size.width <= previous);
                }
                previous_width = Some(first.size.width);
                window.press("secondary-,", cx);
                let state = view.read(cx);
                assert!(state.screen == Screen::Home && state.home_page == HomePage::Settings);
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
        assert!(window.find("document-tabs").bounds().size.width <= px(10.));
        window.click("add-document", cx);
    })
    .unwrap();
    assert!(cx.did_prompt_for_paths());
    cx.simulate_path_prompt_response(|_| None);
}

#[gpui_kit::test]
fn short_document_titles_use_content_width_and_leave_room_for_shadow(cx: &mut TestAppContext) {
    let (handle, view) = app(cx);
    let mut wide_widths = Vec::new();
    cx.update_window(handle, |_, window, cx| {
        view.update(cx, |view, _| {
            view.books[0].title = "A.pdf".into();
            for title in [
                "短.pdf",
                "A very long document name that needs truncation.pdf",
            ] {
                view.books.push(Book {
                    reflow: ReflowState::default(),
                    source: Source::Sample,
                    title: title.into(),
                    favorite: false,
                    categories: Vec::new(),
                    state: ReaderState::default(),
                    opened: true,
                    last_opened: 0,
                    annotations: DocumentAnnotations::default(),
                    markdown_edit: None,
                    markdown_editing: false,
                });
            }
            view.active = 2;
        });
        window.render_frame(cx);
        let short = window.find(("document", 0usize)).bounds();
        let unicode = window.find(("document", 1usize)).bounds();
        let long = window.find(("document", 2usize)).bounds();
        for (index, tab) in [(0usize, short), (1, unicode), (2, long)] {
            let content = window.find(("document-tab-content", index)).bounds();
            let title = window.find(("document-tab-title", index)).bounds();
            let padding = window.rem_size() * 0.5;
            assert!((content.left() - tab.left() - padding).abs() < px(0.5));
            assert!((tab.right() - title.right() - padding).abs() < px(0.5));
        }
        assert!(short.size.width >= px(40.) && short.size.width < px(128.));
        assert!(unicode.size.width >= px(40.) && unicode.size.width < px(128.));
        assert_eq!(long.size.width, px(228.));
        let divider = window.find("document-tab-divider").bounds();
        assert!(divider.left() - long.right() >= px(9.));
        window.hover(("document", 0usize), cx);
        assert_eq!(window.find(("document", 0usize)).bounds(), short);
        window.hover(("close", 0usize), cx);
        assert_eq!(window.find(("document", 0usize)).bounds(), short);
        wide_widths = vec![short.size.width, unicode.size.width, long.size.width];
    })
    .unwrap();
    cx.simulate_window_resize(handle, size(px(400.), px(700.)));
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        for (index, wide) in wide_widths.iter().enumerate() {
            let width = window.find(("document", index)).bounds().size.width;
            assert!(width >= px(40.) && width <= *wide);
        }
        assert!(window.find(("document", 2usize)).bounds().size.width < px(228.));
        let content = window.find("titlebar-content").bounds();
        assert!(window.find("document-tab-add").bounds().right() <= content.right());
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

/// A native fullscreen transition outside the Fullscreen command must not turn
/// on immersive reading. Breeze keeps its document tabs visible inside the
/// native Space; the status-bar Fullscreen still hides them for reading.
#[gpui_kit::test]
#[cfg(target_os = "macos")]
fn native_fullscreen_does_not_change_reader_layout(cx: &mut TestAppContext) {
    let page = rendered_sample();
    let (handle, view) = app(cx);
    cx.update_window(handle, |_, window, cx| {
        view.update(cx, |view, _| seed_rendered_page(view, page));
        window.render_frame(cx);
        assert!(window.find("titlebar-content").visible());
        // Simulate a native fullscreen transition that bypasses the app action.
        window.toggle_fullscreen();
    })
    .unwrap();
    cx.simulate_window_resize(handle, size(px(1512.), px(982.)));
    cx.update_window(handle, |_, window, cx| {
        assert!(window.is_fullscreen());
        assert!(!view.read(cx).full_screen);
        window.render_frame(cx);
        assert!(window.find("titlebar-content").visible());
        assert!(window.find("document-tabs").visible());
        assert!(window.find("reader-toolbar").visible());
        assert!(window.find("status-scroll").visible());

        // The reader's own Fullscreen can add/remove the document-only chrome
        // inside the native Space without leaving or creating another Space.
        window.click("fullscreen", cx);
        window.render_frame(cx);
        assert!(view.read(cx).full_screen);
        assert!(window.is_fullscreen());
        assert!(window.try_find("titlebar-content").is_none());
        assert!(window.try_find("reader-toolbar").is_none());
        assert!(window.try_find("status-scroll").is_none());
        window.press("escape", cx);
        window.render_frame(cx);
        assert!(!view.read(cx).full_screen);
        assert!(window.is_fullscreen());
        assert!(window.find("titlebar-content").visible());
        assert!(window.find("document-tabs").visible());
        assert!(window.find("reader-toolbar").visible());
        assert!(window.find("status-scroll").visible());
        // Leaving fullscreen outside the app restores the reader chrome.
        window.toggle_fullscreen();
    })
    .unwrap();
    cx.simulate_window_resize(handle, size(px(1000.), px(700.)));
    cx.update_window(handle, |_, window, cx| {
        assert!(!view.read(cx).full_screen);
        window.render_frame(cx);
        assert!(window.find("titlebar-content").visible());
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
fn library_reveal_follows_the_platform_and_replaces_the_open_button(cx: &mut TestAppContext) {
    let (handle, view) = app(cx);
    cx.update_window(handle, |_, window, cx| {
        view.update(cx, |view, cx| view.show_home(cx));
        window.render_frame(cx);
        // The row itself opens the document, so its explicit Open action is gone.
        assert!(window.try_find(("open-item", 0usize)).is_none());
        // The bundled sample lives inside the binary; there is no folder to show.
        assert!(matches!(view.read(cx).books[0].source, Source::Sample));
        assert!(window.try_find(("reveal-item", 0usize)).is_none());
        view.update(cx, |view, cx| {
            view.books[0].source = Source::File(PathBuf::from("breeze-reveal-fixture.pdf"));
            cx.notify();
        });
        window.render_frame(cx);
        let button = window.find(("reveal-item", 0usize));
        assert!(button.visible());
        assert_eq!(button.label(), Some(i18n::text(crate::reveal::label())));
    })
    .unwrap();
    // A path that no longer exists cannot be revealed, and the failure reaches
    // the library instead of disappearing silently.
    cx.update_window(handle, |_, _, cx| {
        view.update(cx, |view, cx| {
            let path = std::env::temp_dir().join("breeze-missing-reveal-fixture.pdf");
            let _ = std::fs::remove_file(&path);
            view.books[0].source = Source::File(path);
            view.books[0].title = "Removed.pdf".into();
            view.reveal_book(0, cx);
        });
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(handle, |_, _, cx| {
        let message = view.read(cx).message.clone();
        assert!(message.contains("Removed.pdf"), "{message}");
        assert!(message.contains("文件已不在保存的位置"), "{message}");
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
        // The delete action sits close to the panel edge instead of trailing the row.
        assert!(row.right() - delete.right() <= px(4.));
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
fn category_forms_confirm_with_enter(cx: &mut TestAppContext) {
    let (handle, view) = app(cx);
    cx.update_window(handle, |_, window, cx| {
        view.update(cx, |view, cx| view.show_home(cx));
        window.render_frame(cx);
        window.click("new-category", cx);
        window.render_frame(cx);
        window.input("Keyboard", cx);
        // The Popover's own Enter binding closes the popover before the input's
        // submit event, so the editor must confirm the form itself.
        window.press("enter", cx);
        assert_eq!(view.read(cx).categories.len(), 1);
        assert_eq!(view.read(cx).categories[0].name, "Keyboard");
        assert!(!view.read(cx).category_form);
        window.hover(("category-row", 1usize), cx);
        window.click(("rename-category", 1usize), cx);
        window.render_frame(cx);
        window.input("Renamed", cx);
        window.press("enter", cx);
        assert_eq!(view.read(cx).categories.len(), 1);
        assert_eq!(view.read(cx).categories[0].name, "Renamed");
        assert!(!view.read(cx).category_form);
        // An invalid name keeps the editor open with its message rather than
        // dismissing an unsaved form.
        window.hover(("category-row", 1usize), cx);
        window.click(("rename-category", 1usize), cx);
        window.render_frame(cx);
        view.update(cx, |view, cx| {
            view.category_input
                .update(cx, |input, cx| input.set_value("", window, cx));
        });
        window.render_frame(cx);
        window.press("enter", cx);
        assert!(view.read(cx).category_form);
        assert!(!view.read(cx).message.is_empty());
        assert_eq!(view.read(cx).categories[0].name, "Renamed");
    })
    .unwrap();
}

#[test]
fn category_action_hover_splits_the_action_row_at_its_buttons() {
    use categories::{CategoryAction, category_action_hover};
    let bounds = Bounds {
        origin: point(px(100.), px(50.)),
        size: size(px(44.), px(20.)),
    };
    let at = |x: f32, action| category_action_hover(bounds, point(px(x), px(60.)), action);
    // Each button owns one end of the row; the gap between them belongs to neither.
    assert!(at(101., CategoryAction::Edit));
    assert!(at(118., CategoryAction::Edit));
    assert!(!at(121., CategoryAction::Edit) && !at(121., CategoryAction::Delete));
    assert!(at(141., CategoryAction::Delete));
    assert!(!at(101., CategoryAction::Delete) && !at(141., CategoryAction::Edit));
    // A pointer outside the revealed row's vertical band or beside it matches nothing.
    assert!(!category_action_hover(
        bounds,
        point(px(101.), px(80.)),
        CategoryAction::Edit
    ));
    assert!(!category_action_hover(
        bounds,
        point(px(90.), px(60.)),
        CategoryAction::Delete
    ));
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

#[gpui_kit::test]
fn status_bar_reports_only_real_notices(cx: &mut TestAppContext) {
    let page = rendered_sample();
    let (handle, view) = app(cx);
    cx.update_window(handle, |_, window, cx| {
        view.update(cx, |view, _| seed_rendered_page(view, page));
        window.render_frame(cx);
        // A healthy document says nothing; the retired "source unchanged"
        // placeholder must not come back.
        assert!(window.try_find("reader-status").is_none());
        view.update(cx, |view, cx| {
            view.message = tr!("Unsaved annotations").into();
            cx.notify();
        });
        window.render_frame(cx);
        assert!(window.find("reader-status").visible());
        view.update(cx, |view, cx| {
            view.message.clear();
            cx.notify();
        });
        window.render_frame(cx);
        assert!(window.try_find("reader-status").is_none());
    })
    .unwrap();
}

/// The OS can ask for a document while the saved library is still loading.
/// Restore replaces the whole book list, so the request has to wait for it and
/// then open the file the user asked for.
#[gpui_kit::test]
fn requested_documents_wait_for_restore_and_then_open(cx: &mut TestAppContext) {
    let (handle, view) = app(cx);
    let sample = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../assets/samples/field-notes.pdf")
        .canonicalize()
        .unwrap();
    cx.update_window(handle, |_, _, cx| {
        view.update(cx, |view, cx| {
            view.restoring = true;
            view.open_paths(vec![sample.clone()], cx);
            assert_eq!(view.pending_opens.as_slice(), [sample.as_path()]);
            assert_eq!(view.books.len(), 1, "restore still owns the library");
            view.restoring = false;
            assert!(
                view.open_requested(Vec::new(), cx),
                "a queued document must not fall back to session restoration"
            );
            assert!(view.pending_opens.is_empty());
        });
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        let opened = view.read(cx);
        assert_eq!(opened.books.len(), 2);
        assert_eq!(
            opened.books.last().map(|book| book.source.clone()),
            Some(Source::File(sample.clone()))
        );
        assert_eq!(
            opened.books.last().map(|book| book.title.as_str()),
            Some("field-notes.pdf")
        );
    })
    .unwrap();
}

/// A document the OS opens while Breeze is already running must land in the
/// window, not just in the queue. This exercises the delivery path the macOS
/// `application:openURLs:` event uses.
#[gpui_kit::test]
fn a_running_window_opens_delivered_documents(cx: &mut TestAppContext) {
    let (handle, view) = app(cx);
    let sample = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../assets/samples/field-notes.pdf")
        .canonicalize()
        .unwrap();
    let requests = crate::open_events::OpenRequests::new();
    cx.update(|cx| {
        requests.attach(cx.to_async(), handle, view.downgrade());
    });
    cx.update_window(handle, |_, _, cx| {
        view.update(cx, |view, cx| {
            view.restoring = false;
            cx.notify();
        });
    })
    .unwrap();
    requests.receive_urls(&[format!("file://{}", sample.display())]);
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        let opened = view.read(cx);
        assert_eq!(opened.books.len(), 2);
        assert_eq!(
            opened.books.last().map(|book| book.source.clone()),
            Some(Source::File(sample))
        );
    })
    .unwrap();
}

/// A Markdown document must scroll in the virtualized reader.
///
/// Kit dispatches the list's scroll handler while the list mutably borrows its
/// own state, so recording the reading position must use the reported range
/// instead of querying the list; querying it panicked with `RefCell already
/// mutably borrowed` as soon as the first scroll arrived.
#[gpui_kit::test]
fn markdown_reader_scrolls_and_records_its_position(cx: &mut TestAppContext) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("notes.md");
    let mut markdown = String::new();
    for index in 0..200 {
        markdown.push_str(&format!(
            "## Heading {index}\n\nParagraph {index} with 中文 text and **bold** words.\n\n"
        ));
    }
    std::fs::write(&path, markdown).unwrap();
    let (handle, view) = app(cx);
    cx.update_window(handle, |_, _, cx| {
        view.update(cx, |view, cx| {
            view.books[0].source = Source::File(path.clone());
            view.books[0].title = "notes.md".into();
            view.load(cx);
        });
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.render_frame(cx);
        assert!(
            view.read(cx).reflow.markdown.is_some(),
            "the Markdown document did not load: {}",
            view.read(cx).message
        );
        assert_eq!(view.read(cx).books[0].reflow.markdown_offset, 0);
        for _ in 0..12 {
            window.scroll("reflow-viewport", ScrollDelta::Lines(point(0., -6.)), cx);
        }
        window.render_frame(cx);
        let offset = view.read(cx).books[0].reflow.markdown_offset;
        assert!(
            offset > 0,
            "scrolling the Markdown reader recorded no position: {offset}"
        );
        assert!(
            view.read(cx).books[0].reflow.progress > 0.,
            "scrolling the Markdown reader recorded no progress"
        );
        // Scrolling past the end reports the finished document instead of
        // asking the borrowed list where it landed.
        for _ in 0..80 {
            window.scroll("reflow-viewport", ScrollDelta::Lines(point(0., -20.)), cx);
        }
        assert_eq!(view.read(cx).books[0].reflow.progress, 1.);
        // The heading outline jumps and the text-size command remeasures the
        // list; both run outside the scroll dispatch and must keep working.
        window.click(("outline", 4usize), cx);
        window.render_frame(cx);
        let jumped = view.read(cx).books[0].reflow.markdown_offset;
        assert!(jumped < offset, "heading jump did not move back: {jumped}");
        window.focus(&view.read(cx).focus.clone(), cx);
        window.press("secondary-=", cx);
        window.render_frame(cx);
        assert_eq!(view.read(cx).books[0].reflow.font_size, 19);
    })
    .unwrap();
}

/// The reflow reader must place its chrome exactly where the PDF reader does:
/// the navigation toggle stays at the panel header's left edge, the text-size
/// controls live in the status bar where PDF keeps zoom, search and the
/// favorite control are right-aligned in the content toolbar, and both panel
/// headers label themselves with icons instead of words.
#[gpui_kit::test]
fn reflow_chrome_keeps_the_reader_layout(cx: &mut TestAppContext) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("chrome.md");
    std::fs::write(&path, "## One\n\ntext\n\n## Two\n\ntext\n").unwrap();
    let (handle, view) = app(cx);
    cx.update_window(handle, |_, _, cx| {
        view.update(cx, |view, cx| {
            view.books[0].source = Source::File(path.clone());
            view.books[0].title = "chrome.md".into();
            view.load(cx);
        });
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);

        // Navigation is open, so its toggle belongs to that header's left edge
        // and the outline is an icon, not a word.
        assert!(view.read(cx).sidebar);
        let header = window.find("navigation-header").bounds();
        let collapse = window.find("collapse").bounds();
        assert!(
            collapse.left() - header.left() < px(12.),
            "collapse {collapse:?} is not at the left of the panel header {header:?}"
        );
        let outline = window.within("reflow-nav-tabs").find(0usize);
        assert_eq!(outline.label(), Some(i18n::text("Outline")));
        let tabs = outline.bounds();
        assert!(
            tabs.left() >= collapse.right() && tabs.right() <= header.right(),
            "the outline icon tab {tabs:?} is not inside the header {header:?}"
        );

        // Collapsing navigation returns its toggle to the toolbar's left edge.
        window.click("collapse", cx);
        window.render_frame(cx);
        assert!(!view.read(cx).sidebar);
        let toolbar = window.find("reader-toolbar").bounds();
        let collapse = window.find("collapse").bounds();
        assert!(
            collapse.left() - toolbar.left() < px(12.),
            "collapse {collapse:?} is not at the left of the toolbar {toolbar:?}"
        );

        // Search and the favorite control share the toolbar's right half,
        // ordered search, favorite, document-panel toggle.
        let search = window.find("reader-search").bounds();
        let favorite = window.find("reader-star").bounds();
        let toggle = window.find("document-panel-toggle").bounds();
        assert!(
            search.left() > toolbar.center().x,
            "search {search:?} is not in the right half of {toolbar:?}"
        );
        assert!(
            search.right() <= favorite.left() && favorite.right() <= toggle.left(),
            "toolbar order is search {search:?}, favorite {favorite:?}, toggle {toggle:?}"
        );
        assert!(
            toggle.right() <= toolbar.right() - px(4.),
            "the document panel toggle {toggle:?} left the toolbar's right edge"
        );

        // Text size belongs to the status bar, exactly where PDF keeps zoom.
        let status = window.find("reflow-status").bounds();
        for id in ["text-smaller", "text-size", "text-larger"] {
            let control = window.find(id).bounds();
            assert!(
                control.top() >= status.top() && control.bottom() <= status.bottom(),
                "{id} {control:?} is not inside the status bar {status:?}"
            );
            assert!(
                control.top() >= toolbar.bottom(),
                "{id} {control:?} still sits in the toolbar {toolbar:?}"
            );
        }

        // The document panel header labels itself with the information icon.
        window.click("document-panel-toggle", cx);
        window.render_frame(cx);
        assert!(view.read(cx).document_panel.open);
        let panel_header = window.find("document-panel-header").bounds();
        let information = window.find("document-panel-information").bounds();
        assert!(
            information.left() - panel_header.left() < px(12.),
            "information {information:?} is not at the left of {panel_header:?}"
        );
        let toggle = window.find("document-panel-toggle").bounds();
        let inset = panel_header.right() - toggle.right();
        assert!(
            inset >= px(7.) && inset <= px(9.),
            "toggle inset {inset:?}, header {panel_header:?}, toggle {toggle:?}"
        );
    })
    .unwrap();
}

/// A Markdown image that cannot be decoded is that image's problem: the reader
/// reports it in the document information panel and leaves the status bar
/// alone. Before this, any picture in a note turned the status line into
/// "Image unavailable".
#[gpui_kit::test]
fn markdown_image_failure_stays_out_of_the_status_bar(cx: &mut TestAppContext) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("notes.md");
    std::fs::write(
        &path,
        "## Notes\n\n![diagram](missing.png)\n\n![remote](https://example.com/x.png)\n",
    )
    .unwrap();
    let (handle, view) = app(cx);
    cx.update_window(handle, |_, _, cx| {
        view.update(cx, |view, cx| {
            view.books[0].source = Source::File(path.clone());
            view.books[0].title = "notes.md".into();
            view.load(cx);
        });
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.render_frame(cx);
        assert!(view.read(cx).reflow.markdown.is_some());
        assert!(
            view.read(cx).message.is_empty(),
            "an image failure reached the reader message: {}",
            view.read(cx).message
        );
        assert!(
            window.try_find("reflow-status-notice").is_none(),
            "the status bar still reports an image problem"
        );
        // The document pane carries the honest count instead.
        view.update(cx, |view, _| {
            view.document_panel.open = true;
        });
        window.render_frame(cx);
        assert!(
            view.read(cx).reflow.images.failures > 0,
            "no image failure was recorded"
        );
    })
    .unwrap();
}

/// The reflow outline must be indistinguishable from the PDF outline: same row
/// height, same indentation at every depth, same header controls and the same
/// nesting menu.
#[gpui_kit::test]
fn reflow_outline_matches_the_pdf_outline_style(cx: &mut TestAppContext) {
    // A nested PDF outline, so both readers show the nesting chevron slot and
    // the row geometry is directly comparable.
    let worker = PdfWorker::new().unwrap();
    let mut page = futures::executor::block_on(worker.request_metadata(Source::Sample, 0).unwrap())
        .unwrap()
        .unwrap();
    page.outline = Arc::new(vec![
        breeze_pdf::Outline {
            title: "Title".into(),
            page: Some(0),
            depth: 0,
        },
        breeze_pdf::Outline {
            title: "Section".into(),
            page: Some(1),
            depth: 1,
        },
    ]);
    let (handle, view) = app(cx);
    cx.update_window(handle, |_, window, cx| {
        view.update(cx, |view, _| {
            view.reading.sizes = page.page_sizes.clone();
            let preview = Preview::from_page(page).unwrap();
            view.reading.insert(0, preview.clone());
            view.preview = Some(preview);
            view.loading = true;
        });
        window.render_frame(cx);
    })
    .unwrap();

    let measure = |_view: &Entity<Breeze>, window: &gpui_kit::Window| {
        let content = window.find("navigation-content").bounds();
        let geometry = |index: usize| {
            let row = window.find(("outline", index)).bounds() - content.origin;
            let toggle = window.try_find(("outline-toggle", index));
            (
                row.origin.x,
                row.size.height,
                toggle.map(|t| t.bounds().size),
            )
        };
        (geometry(0), geometry(1), content.size.width)
    };

    let pdf = cx
        .update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            window.render_frame(cx);
            assert!(window.try_find("outline-search").is_some());
            assert!(window.try_find("outline-more").is_some());
            let _ = view;
            measure(&view, window)
        })
        .unwrap();

    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("outline.md");
    std::fs::write(&path, "# Title\n\nintro\n\n## Section\n\nbody\n").unwrap();
    cx.update_window(handle, |_, _, cx| {
        view.update(cx, |view, cx| {
            view.books[0].source = Source::File(path.clone());
            view.books[0].title = "outline.md".into();
            view.load(cx);
        });
    })
    .unwrap();
    cx.run_until_parked();

    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.render_frame(cx);
        assert_eq!(view.read(cx).reflow.outline.len(), 2);
        let markdown = measure(&view, window);
        assert_eq!(
            markdown, pdf,
            "the reflow outline differs from the PDF outline: {markdown:?} vs {pdf:?}"
        );

        // The same header controls: title search, and the nesting menu.
        assert!(window.try_find("outline-search").is_some());
        assert!(
            window.try_find("outline-more").is_some(),
            "a nested reflow outline has no expand/collapse menu"
        );

        // A nested row jumps to its own section.
        window.click(("outline", 1usize), cx);
        window.render_frame(cx);
        assert!(
            view.read(cx).books[0].reflow.markdown_offset > 0,
            "an outline row did not move the Markdown reader"
        );

        // The nesting chevron folds its children away, exactly like PDF.
        window.click(("outline-toggle", 0usize), cx);
        window.render_frame(cx);
        assert!(view.read(cx).reflow.outline_folds.contains(&0));
        assert!(
            window.try_find(("outline", 1usize)).is_none(),
            "the folded child row is still shown"
        );
        // The menu is the same control PDF offers for nested outlines.
        assert_eq!(
            window.find("outline-more").label(),
            Some(i18n::text("More outline actions"))
        );
    })
    .unwrap();
}

/// Markdown scrolls, so it has no page-turn buttons, and the status bar carries
/// a stateful width toggle between the readable measure and the full window.
#[gpui_kit::test]
fn markdown_has_no_paging_and_toggles_its_reading_width(cx: &mut TestAppContext) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("width.md");
    std::fs::write(&path, "# Title\n\nbody\n\n## Section\n\nmore\n").unwrap();
    let (handle, view) = app(cx);
    cx.update_window(handle, |_, _, cx| {
        view.update(cx, |view, cx| {
            view.books[0].source = Source::File(path.clone());
            view.books[0].title = "width.md".into();
            view.load(cx);
        });
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert!(view.read(cx).reflow.markdown.is_some());
        // The PDF page-turn controls belong to fixed pages only.
        assert!(window.try_find("reflow-previous").is_none());
        assert!(window.try_find("reflow-next").is_none());

        // The status bar reports the rendered character count of the document.
        let characters = view.read(cx).reflow.markdown.as_ref().unwrap().characters;
        assert_eq!(characters, 22, "the width fixture renders 22 characters");
        let expected = i18n::format_message(i18n::text("{} characters"), &[characters.to_string()]);
        assert_eq!(
            window.find("markdown-characters").label(),
            Some(expected.as_str()),
            "the Markdown status bar must report its character count"
        );

        let status = window.find("reflow-status").bounds();
        let toggle = window.find("reflow-width").bounds();
        assert!(
            toggle.top() >= status.top() && toggle.bottom() <= status.bottom(),
            "the width toggle {toggle:?} left the status bar {status:?}"
        );
        assert!(!view.read(cx).books[0].reflow.full_width);
        assert_eq!(
            window.find("reflow-width").label(),
            Some(i18n::text("Use the full window width"))
        );
        let narrow = window.find("markdown-measure").bounds().size.width;
        assert!(
            narrow <= px(760.),
            "the default Markdown measure is {narrow:?}, wider than its maximum"
        );

        window.click("reflow-width", cx);
        window.render_frame(cx);
        assert!(view.read(cx).books[0].reflow.full_width);
        assert_eq!(
            window.find("reflow-width").label(),
            Some(i18n::text("Use the maximum reading width"))
        );
        let wide = window.find("markdown-measure").bounds().size.width;
        assert!(
            wide > narrow,
            "full width {wide:?} is not wider than the reading measure {narrow:?}"
        );

        // The choice is per document and survives a reload.
        window.click("reflow-width", cx);
        window.render_frame(cx);
        assert!(!view.read(cx).books[0].reflow.full_width);
    })
    .unwrap();
}

/// A picture that exists must be shown even when its Markdown reference is
/// URL-encoded, which is what an editor writes for a name with a space. The
/// reader used to open the literal `%20` path, fail, and report the image as
/// unavailable.
#[gpui_kit::test]
fn markdown_shows_images_named_with_spaces(cx: &mut TestAppContext) {
    use image::codecs::jpeg::JpegEncoder;
    let directory = tempfile::tempdir().unwrap();
    let image_path = directory.path().join("my shot.jpg");
    let source = image::RgbImage::from_fn(64, 48, |x, y| {
        image::Rgb([(x * 4) as u8, (y * 5) as u8, 128])
    });
    let mut bytes = Vec::new();
    JpegEncoder::new_with_quality(&mut bytes, 85)
        .encode_image(&source)
        .unwrap();
    std::fs::write(&image_path, &bytes).unwrap();

    let notes = directory.path().join("notes.md");
    std::fs::write(
        &notes,
        "# Notes\n\nScreenshot:\n\n![shot](my%20shot.jpg)\n\nafter the image\n",
    )
    .unwrap();
    let (handle, view) = app(cx);
    cx.update_window(handle, |_, _, cx| {
        view.update(cx, |view, cx| {
            view.books[0].source = Source::File(notes.clone());
            view.books[0].title = "notes.md".into();
            view.load(cx);
        });
    })
    .unwrap();
    cx.run_until_parked();
    // The image resolves on a background thread, then the block remeasures.
    for _ in 0..4 {
        cx.run_until_parked();
        cx.update_window(handle, |_, window, cx| window.render_frame(cx))
            .unwrap();
    }
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert_eq!(
            view.read(cx).reflow.images.failures,
            0,
            "a JPEG image was reported as unavailable: {:?}",
            view.read(cx).reflow.images.last_failure
        );
        assert!(
            view.read(cx).message.is_empty(),
            "the reader reported {}",
            view.read(cx).message
        );
    })
    .unwrap();
}

/// One wheel swipe must not re-render the EPUB page per event, and the pages it
/// crosses must actually be crossed. Every wheel event used to launch a fresh
/// full-page render that superseded the previous one, which is what made turning
/// a page crawl.
#[gpui_kit::test]
fn epub_wheel_turns_pages_without_a_render_per_event(cx: &mut TestAppContext) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("book.epub");
    std::fs::write(&path, epub_fixture(60)).unwrap();
    let (handle, view) = app(cx);
    cx.update_window(handle, |_, _, cx| {
        view.update(cx, |view, cx| {
            view.books[0].source = Source::File(path.clone());
            view.books[0].title = "book.epub".into();
            view.load(cx);
        });
    })
    .unwrap();
    // The EPUB page arrives from a dedicated worker thread, so pump the frame
    // loop until it lands rather than assuming one parked pass is enough.
    let mut rendered = false;
    for _ in 0..60 {
        cx.run_until_parked();
        cx.update_window(handle, |_, window, cx| window.render_frame(cx))
            .unwrap();
        rendered = cx
            .update_window(handle, |_, _, cx| view.read(cx).reflow.frame.is_some())
            .unwrap();
        if rendered {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    assert!(rendered, "the EPUB never produced a frame");
    // A scroll delta shorter than a page accumulates instead of re-rendering.
    let (chapter_page, pages, renders) = cx
        .update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            let view = view.read(cx);
            assert!(
                view.reflow.frame.is_some(),
                "the EPUB did not render: {}",
                view.message
            );
            let frame = view.reflow.frame.as_ref().unwrap();
            (frame.chapter_page, frame.chapter_pages, view.reflow.epoch)
        })
        .unwrap();
    assert!(pages > 3, "the fixture needs several pages, got {pages}");
    assert_eq!(chapter_page, 0);

    cx.update_window(handle, |_, window, cx| {
        // One notch of a mouse wheel: far less than a page.
        window.scroll("reflow-viewport", ScrollDelta::Lines(point(0., -3.)), cx);
    })
    .unwrap();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert_eq!(
            view.read(cx).reflow.epoch,
            renders,
            "a partial scroll launched a page render"
        );
    })
    .unwrap();

    // Enough scroll to cross a page does turn exactly one page.
    cx.update_window(handle, |_, window, cx| {
        for _ in 0..12 {
            window.scroll("reflow-viewport", ScrollDelta::Lines(point(0., -3.)), cx);
        }
    })
    .unwrap();
    for _ in 0..60 {
        cx.run_until_parked();
        cx.update_window(handle, |_, window, cx| window.render_frame(cx))
            .unwrap();
        let turned = cx
            .update_window(handle, |_, _, cx| {
                view.read(cx)
                    .reflow
                    .frame
                    .as_ref()
                    .is_some_and(|frame| frame.chapter_page >= 1)
            })
            .unwrap();
        if turned {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        let view = view.read(cx);
        let frame = view.reflow.frame.as_ref().unwrap();
        assert!(
            frame.chapter_page >= 1,
            "a full page of scrolling stayed on page {}",
            frame.chapter_page
        );
    })
    .unwrap();
}

/// A usable EPUB with one chapter of `paragraphs` paragraphs.
fn epub_fixture(paragraphs: usize) -> Vec<u8> {
    use std::io::{Cursor, Write};
    use zip::{ZipWriter, write::SimpleFileOptions};
    let mut archive = ZipWriter::new(Cursor::new(Vec::new()));
    let write = |archive: &mut ZipWriter<Cursor<Vec<u8>>>, name: &str, text: &str| {
        archive
            .start_file(name, SimpleFileOptions::default())
            .unwrap();
        archive.write_all(text.as_bytes()).unwrap();
    };
    write(&mut archive, "mimetype", "application/epub+zip");
    write(
        &mut archive,
        "META-INF/container.xml",
        r#"<?xml version="1.0"?><container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container"><rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles></container>"#,
    );
    write(
        &mut archive,
        "OEBPS/content.opf",
        r#"<?xml version="1.0"?><package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="bookid"><metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:identifier id="bookid">breeze-wheel-fixture</dc:identifier><dc:title>Wheel</dc:title><dc:language>en</dc:language><meta property="dcterms:modified">2026-10-15T00:00:00Z</meta></metadata><manifest><item id="ch" href="ch1.xhtml" media-type="application/xhtml+xml"/></manifest><spine><itemref idref="ch"/></spine></package>"#,
    );
    let body: String = (0..paragraphs)
        .map(|index| format!("<p>Paragraph {index} of a long chapter.</p>"))
        .collect();
    write(
        &mut archive,
        "OEBPS/ch1.xhtml",
        &format!(
            r#"<html xmlns="http://www.w3.org/1999/xhtml"><head><title>Chapter</title></head><body>{body}</body></html>"#
        ),
    );
    archive.finish().unwrap().into_inner()
}

/// EPUB paging is keyboard-only: the toolbar keeps the global commands, the
/// status bar shows the reading position without page-turn buttons, and the
/// Page Up and Page Down commands remain bound.
#[gpui_kit::test]
fn epub_paging_has_no_status_bar_buttons(cx: &mut TestAppContext) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("pages.epub");
    std::fs::write(&path, epub_multi_chapter_fixture(6, 40)).unwrap();
    let (handle, view) = app(cx);
    cx.update_window(handle, |_, _, cx| {
        view.update(cx, |view, cx| {
            view.books[0].source = Source::File(path.clone());
            view.books[0].title = "pages.epub".into();
            view.load(cx);
        });
    })
    .unwrap();
    assert!(
        wait_for_epub(cx, handle, &view, |_| true),
        "the EPUB never produced a frame"
    );
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        // The toolbar keeps its global commands and no chapter arrows.
        assert!(window.try_find("reader-toolbar").is_some());
        assert!(window.try_find("previous-chapter").is_none());
        assert!(window.try_find("next-chapter").is_none());
        for id in ["reader-search", "reader-star", "document-panel-toggle"] {
            assert!(window.try_find(id).is_some(), "{id} left the toolbar");
        }

        // Page turns remain available from the keyboard, but the status bar
        // no longer shows arrow buttons.
        assert!(window.try_find("reflow-previous").is_none());
        assert!(window.try_find("reflow-next").is_none());
        assert!(
            window.try_find("markdown-characters").is_none(),
            "paginated EPUB has no Markdown character count"
        );
        for (label, action) in [
            ("Previous page", &PreviousPage as &dyn Action),
            ("Next page", &NextPage),
        ] {
            assert!(
                window
                    .highest_precedence_binding_for_action_in_context(
                        action,
                        gpui_kit::KeyContext::parse("Breeze").unwrap(),
                    )
                    .is_some(),
                "{label} has no keyboard binding"
            );
        }

        // The position is a page count and never names a chapter.
        let position_snapshot = window.find("reflow-position");
        let position = position_snapshot.label().unwrap_or_default().to_owned();
        assert!(
            !position.contains("Chapter") && !position.contains('章'),
            "the position still names the chapter: {position}"
        );
        let (page, pages) = position
            .split_once(" / ")
            .unwrap_or_else(|| panic!("the position is not `page / pages`: {position}"));
        let page: usize = page.parse().unwrap();
        let pages: usize = pages.parse().unwrap();
        assert!(pages >= 1 && (1..=pages).contains(&page), "{position}");
        assert_eq!(page, 1);
    })
    .unwrap();
}

/// Page turns requested faster than a page renders must not be dropped.
/// A turn is resolved against a landed frame, so a request that arrived while
/// the previous page was still rendering used to rebuild the same request and
/// be discarded: five presses advanced one page.
#[gpui_kit::test]
fn rapid_page_turns_all_land(cx: &mut TestAppContext) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("chapters.epub");
    // Two short paragraphs per chapter: each chapter is exactly one page, so
    // five page turns land on the sixth chapter.
    std::fs::write(&path, epub_multi_chapter_fixture(8, 2)).unwrap();
    let (handle, view) = app(cx);
    cx.update_window(handle, |_, _, cx| {
        view.update(cx, |view, cx| {
            view.books[0].source = Source::File(path.clone());
            view.books[0].title = "chapters.epub".into();
            view.load(cx);
        });
    })
    .unwrap();
    assert!(
        wait_for_epub(cx, handle, &view, |frame| frame.chapter == 0),
        "the EPUB never produced its first chapter"
    );

    // Five Page Down presses with no waiting in between.
    cx.update_window(handle, |_, window, cx| {
        for _ in 0..5 {
            window.press("pagedown", cx);
        }
    })
    .unwrap();
    assert!(
        wait_for_epub(cx, handle, &view, |frame| frame.chapter == 5),
        "five rapid page turns landed on chapter {:?}, not 5",
        cx.update_window(handle, |_, _, cx| {
            view.read(cx).reflow.frame.as_ref().map(|f| f.chapter)
        })
        .unwrap()
    );
}

/// Two-page reading is a view state, not a second renderer: the status bar's
/// columns-2 button re-paginates the chapter at half the content width, fills
/// the area with a facing pair, keeps its selected look while the spread is on,
/// and turns by spreads instead of single pages.
#[gpui_kit::test]
fn epub_two_page_reading_fills_the_content_area(cx: &mut TestAppContext) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("spread.epub");
    std::fs::write(&path, epub_multi_chapter_fixture(1, 80)).unwrap();
    let (handle, view) = app(cx);
    cx.update_window(handle, |_, _, cx| {
        view.update(cx, |view, cx| {
            view.books[0].source = Source::File(path.clone());
            view.books[0].title = "spread.epub".into();
            view.load(cx);
        });
    })
    .unwrap();

    // Opening starts from a placeholder viewport; wait for the frame the reader
    // settles on so both readings are measured at one content size.
    assert!(
        wait_for_settled_epub(cx, handle, &view, |frame| frame.columns == 1),
        "the EPUB never produced a settled single-page frame"
    );
    let single_text = cx
        .update_window(handle, |_, _, cx| {
            view.read(cx)
                .reflow
                .frame
                .as_ref()
                .map(|frame| frame.text.chars().count())
        })
        .unwrap()
        .unwrap();

    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        // The toggle belongs to the status bar and starts in single-page mode.
        let status = window.find("reflow-status").bounds();
        let toggle = window.find("reflow-spread").bounds();
        assert!(
            toggle.top() >= status.top() && toggle.bottom() <= status.bottom(),
            "the two-page toggle {toggle:?} left the status bar {status:?}"
        );
        assert_eq!(window.find("reflow-spread").checked(), Some(false));
        assert_eq!(
            window.find("reflow-spread").label(),
            Some(i18n::text("Two-page reading"))
        );
        assert!(!view.read(cx).books[0].reflow.spread);

        window.click("reflow-spread", cx);
        window.render_frame(cx);
        assert!(view.read(cx).books[0].reflow.spread);
        assert_eq!(
            window.find("reflow-spread").checked(),
            Some(true),
            "the toggle does not show that two-page reading is on"
        );
        assert_eq!(
            window.find("reflow-spread").label(),
            Some(i18n::text("Single-page reading"))
        );
    })
    .unwrap();

    assert!(
        wait_for_settled_epub(cx, handle, &view, |frame| frame.columns == 2),
        "the EPUB never produced a settled two-page frame"
    );
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        let view = view.read(cx);
        let frame = view.reflow.frame.as_ref().unwrap();
        assert_eq!(frame.chapter_page % 2, 0, "the spread started mid-pair");
        assert_eq!(view.reflow.page_images.len(), 2);
        // The pair is re-paginated at half the content width and arrives as one
        // raster, so it fills the area instead of doubling it.
        let content = window.find("reflow-viewport").bounds().size;
        assert!(
            (frame.width as f32 - f32::from(content.width) * window.scale_factor()).abs() <= 2.,
            "the spread raster is {} px wide in a {} px area",
            frame.width,
            content.width
        );
        // Both facing pages were rendered, not the first one twice.
        assert!(
            frame.text.chars().count() > single_text,
            "the spread carries {} characters, one page carries {single_text}",
            frame.text.chars().count()
        );
        // The position reports the pair, not only its first page.
        let position = window.find("reflow-position");
        let position = position.label().unwrap_or_default().to_owned();
        assert!(
            position.starts_with(&format!("1\u{2013}2 / {}", frame.chapter_pages)),
            "the position does not report the pair: {position}"
        );

        // Turning once advances a whole spread.
        window.press("pagedown", cx);
    })
    .unwrap();
    assert!(
        wait_for_epub(cx, handle, &view, |frame| frame.chapter_page >= 2),
        "a turn in two-page mode did not advance a spread"
    );

    // Turning the spread off returns the reader to single pages.
    cx.update_window(handle, |_, window, cx| {
        window.click("reflow-spread", cx);
        window.render_frame(cx);
        assert!(!view.read(cx).books[0].reflow.spread);
        assert_eq!(window.find("reflow-spread").checked(), Some(false));
    })
    .unwrap();
    assert!(
        wait_for_epub(cx, handle, &view, |frame| frame.columns == 1),
        "the reader did not return to single pages"
    );
}

/// Pump frames until the reader holds a frame rasterized at the content area the
/// window currently shows. The first EPUB frame is laid out at a placeholder
/// viewport, so a plain wait can observe a page the reader has already replaced.
fn wait_for_settled_epub(
    cx: &mut TestAppContext,
    handle: AnyWindowHandle,
    view: &Entity<Breeze>,
    done: impl Fn(&breeze_reflow::epub::Frame) -> bool,
) -> bool {
    for _ in 0..80 {
        cx.run_until_parked();
        cx.update_window(handle, |_, window, cx| window.render_frame(cx))
            .unwrap();
        let reached = cx
            .update_window(handle, |_, window, cx| {
                let content = window.find("reflow-viewport").bounds().size.width;
                let expected = f32::from(content) * window.scale_factor();
                view.read(cx)
                    .reflow
                    .frame
                    .as_ref()
                    .is_some_and(|frame| done(frame) && (frame.width as f32 - expected).abs() <= 2.)
            })
            .unwrap();
        if reached {
            return true;
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    false
}

fn wait_for_epub(
    cx: &mut TestAppContext,
    handle: AnyWindowHandle,
    view: &Entity<Breeze>,
    done: impl Fn(&breeze_reflow::epub::Frame) -> bool,
) -> bool {
    for _ in 0..80 {
        cx.run_until_parked();
        cx.update_window(handle, |_, window, cx| window.render_frame(cx))
            .unwrap();
        let reached = cx
            .update_window(handle, |_, _, cx| {
                view.read(cx).reflow.frame.as_ref().is_some_and(&done)
            })
            .unwrap();
        if reached {
            return true;
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    false
}

fn epub_multi_chapter_fixture(chapters: usize, paragraphs: usize) -> Vec<u8> {
    use std::io::{Cursor, Write};
    use zip::{ZipWriter, write::SimpleFileOptions};
    let mut archive = ZipWriter::new(Cursor::new(Vec::new()));
    let write = |archive: &mut ZipWriter<Cursor<Vec<u8>>>, name: &str, text: &str| {
        archive
            .start_file(name, SimpleFileOptions::default())
            .unwrap();
        archive.write_all(text.as_bytes()).unwrap();
    };
    write(&mut archive, "mimetype", "application/epub+zip");
    write(
        &mut archive,
        "META-INF/container.xml",
        r#"<?xml version="1.0"?><container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container"><rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles></container>"#,
    );
    let items: String = (0..chapters)
        .map(|i| {
            format!(r#"<item id="c{i}" href="c{i}.xhtml" media-type="application/xhtml+xml"/>"#)
        })
        .collect();
    let refs: String = (0..chapters)
        .map(|i| format!(r#"<itemref idref="c{i}"/>"#))
        .collect();
    write(
        &mut archive,
        "OEBPS/content.opf",
        &format!(
            r#"<?xml version="1.0"?><package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="bookid"><metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:identifier id="bookid">breeze-chapters-fixture</dc:identifier><dc:title>Chapters</dc:title><dc:language>en</dc:language><meta property="dcterms:modified">2026-10-15T00:00:00Z</meta></metadata><manifest>{items}</manifest><spine>{refs}</spine></package>"#
        ),
    );
    for chapter in 0..chapters {
        let body: String = (0..paragraphs)
            .map(|i| format!("<p>Chapter {chapter} paragraph {i}.</p>"))
            .collect();
        write(
            &mut archive,
            &format!("OEBPS/c{chapter}.xhtml"),
            &format!(
                r#"<html xmlns="http://www.w3.org/1999/xhtml"><head><title>Chapter {chapter}</title></head><body><h1>Chapter {chapter}</h1>{body}</body></html>"#
            ),
        );
    }
    archive.finish().unwrap().into_inner()
}

/// Markdown editing happens inside the reader's content area.
///
/// The content region swaps between the read-only text and the editable
/// surface; the title bar, tabs, navigation panel, toolbar and status bar are
/// the reader's own and must stay in place. Entering edit mode creates the
/// draft session once and leaving it keeps that session.
#[gpui_kit::test]
fn markdown_edit_toggle_edits_inside_the_reader_content_area(cx: &mut TestAppContext) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("notes.md");
    std::fs::write(&path, "# Title\n\nBody **bold** text\n").unwrap();
    let (handle, view) = app(cx);
    cx.update_window(handle, |_, _, cx| {
        view.update(cx, |view, cx| {
            view.books[0].source = Source::File(path.clone());
            view.books[0].title = "notes.md".into();
            view.load(cx);
        });
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert_eq!(
            view.read(cx).books[0].format(),
            Some(DocumentFormat::Markdown)
        );
        // Reading mode: the read-only text owns the content area.
        assert!(!view.read(cx).books[0].markdown_editing);
        assert!(window.try_find("markdown-measure").is_some());
        assert!(window.try_find("markdown-editor").is_none());

        // Entering edit mode keeps the reader chrome and mounts the surface in
        // the same content region.
        window.click("markdown-edit-toggle", cx);
        window.render_frame(cx);
        assert!(view.read(cx).books[0].markdown_editing);
        assert!(
            window.try_find("reader-toolbar").is_some(),
            "the reader toolbar must stay in edit mode"
        );
        assert!(
            window.try_find("reflow-status").is_some(),
            "the shared status bar must stay in edit mode"
        );
        assert!(window.try_find("markdown-editor").is_some());
        assert!(window.try_find("markdown-measure").is_some());
        let session = view.read(cx).books[0]
            .markdown_edit
            .clone()
            .expect("entering edit mode creates the session");
        assert!(!session.read(cx).dirty());

        // The status bar counts the text the draft shows rather than the file
        // that was loaded, so the number follows the open session.
        let live = i18n::format_message(
            i18n::text("{} characters"),
            &[session.read(cx).characters().to_string()],
        );
        assert_eq!(
            window.find("markdown-characters").label(),
            Some(live.as_str()),
            "the status bar must count the open draft"
        );

        // The live/source toggle lives in the reader toolbar and does not edit.
        window.click("markdown-source-view", cx);
        window.render_frame(cx);
        assert_eq!(session.read(cx).view(), markdown_editor::EditView::Source);
        assert!(!session.read(cx).dirty(), "switching views must not edit");
        let source = i18n::format_message(
            i18n::text("{} characters"),
            &[session.read(cx).characters().to_string()],
        );
        assert_eq!(
            window.find("markdown-characters").label(),
            Some(source.as_str()),
            "the count follows the source view too"
        );
        window.click("markdown-live-view", cx);
        window.render_frame(cx);
        assert_eq!(session.read(cx).view(), markdown_editor::EditView::Live);
        assert_eq!(
            window.find("markdown-characters").label(),
            Some(live.as_str()),
            "returning to the live view restores its count"
        );

        // Leaving edit mode returns to reading but keeps the draft session.
        window.click("markdown-edit-toggle", cx);
        window.render_frame(cx);
        assert!(!view.read(cx).books[0].markdown_editing);
        assert!(window.try_find("markdown-editor").is_none());
        assert!(
            view.read(cx).books[0].markdown_edit.is_some(),
            "the draft session must survive leaving edit mode"
        );
    })
    .unwrap();
}

/// Two pages of the sample side by side, in Read mode, ready to drag on.
fn spread_pages(cx: &mut TestAppContext) -> (AnyWindowHandle, Entity<Breeze>) {
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
            view.reading.insert(0, first.clone());
            view.reading.insert(1, Preview::from_page(second).unwrap());
            view.preview = Some(first);
            view.books[0].state.layout = ReadingLayout::Spread;
            view.books[0].state.zoom = Zoom::FitPage;
            view.loading = false;
        });
        window.render_frame(cx);
    })
    .unwrap();
    (handle, view)
}

#[gpui_kit::test]
fn text_selection_crosses_page_surfaces_in_reading_order(cx: &mut TestAppContext) {
    let (handle, view) = spread_pages(cx);
    cx.update_window(handle, |_, window, cx| {
        let left = window
            .within(("page-frame", 0usize))
            .find("page-surface")
            .bounds();
        let right = window
            .within(("page-frame", 1usize))
            .find("page-surface")
            .bounds();
        // From the middle of the left page into the right page: the selection
        // runs to the end of the page it started on and continues on the next.
        window.drag(
            left.origin + point(px(40.), px(300.)),
            right.origin + point(px(200.), px(120.)),
            cx,
        );
        let snapshot = view.read(cx);
        assert!(
            snapshot
                .selected_text
                .contains("Keep the details that help you think"),
            "the rest of the starting page must be selected: {:?}",
            snapshot.selected_text
        );
        assert!(
            snapshot.selected_text.contains("Space to think"),
            "the page the pointer reached must be selected too: {:?}",
            snapshot.selected_text
        );
        assert!(
            snapshot
                .selected_text
                .find("PROTOTYPE EDITION")
                .is_some_and(|footer| snapshot
                    .selected_text
                    .find("ON READING WELL")
                    .is_some_and(|next| footer < next)),
            "the second page's text follows the first page's: {:?}",
            snapshot.selected_text
        );
        let pages: Vec<usize> = snapshot.selection.iter().map(|(page, _)| *page).collect();
        assert_eq!(pages, vec![0, 1], "both pages carry highlight rectangles");
        assert!(snapshot.gesture.is_empty(), "the release ends the gesture");

        // The next gesture still selects: a cross-page drag leaves no state
        // behind that a fresh selection cannot replace.
        view.update(cx, |view, _| view.clear_text_selection());
        // Over the second page's own title and body rows.
        window.drag(
            right.origin + point(px(20.), px(100.)),
            right.origin + point(px(360.), px(200.)),
            cx,
        );
        let snapshot = view.read(cx);
        assert!(
            snapshot.selected_text.contains("Space to think")
                && snapshot.selected_text.contains("A little less noise"),
            "selection still works after a cross-page drag: {:?}",
            snapshot.selected_text
        );
        assert_eq!(
            snapshot
                .selection
                .iter()
                .map(|(page, _)| *page)
                .collect::<Vec<_>>(),
            vec![1]
        );
    })
    .unwrap();
}

#[gpui_kit::test]
fn dragging_back_selects_the_same_text_as_dragging_forward(cx: &mut TestAppContext) {
    let (handle, view) = spread_pages(cx);
    cx.update_window(handle, |_, window, cx| {
        let left = window
            .within(("page-frame", 0usize))
            .find("page-surface")
            .bounds();
        let high = left.origin + point(px(320.), px(140.));
        let low = left.origin + point(px(40.), px(380.));
        window.drag(high, low, cx);
        let forward = view.read(cx).selected_text.clone();
        assert!(!forward.is_empty());
        view.update(cx, |view, _| view.clear_text_selection());
        window.drag(low, high, cx);
        assert_eq!(
            view.read(cx).selected_text,
            forward,
            "the caret model is direction-free"
        );
    })
    .unwrap();
}

/// The reported symptom: dragging onto the next page and back must follow the
/// pointer each way, not freeze where the selection first left its page.
#[gpui_kit::test]
fn a_selection_that_crosses_a_page_and_returns_follows_the_pointer(cx: &mut TestAppContext) {
    let (handle, view) = spread_pages(cx);
    cx.update_window(handle, |_, window, cx| {
        let left = window
            .within(("page-frame", 0usize))
            .find("page-surface")
            .bounds();
        let right = window
            .within(("page-frame", 1usize))
            .find("page-surface")
            .bounds();
        let (first, second) = {
            let view = view.read(cx);
            (
                view.page_preview(0).expect("page 1"),
                view.page_preview(1).expect("page 2"),
            )
        };
        let press = left.origin + point(px(20.), px(120.));
        let onto_second = right.origin + point(px(200.), px(160.));
        let back = left.origin + point(px(160.), px(120.));
        view.update(cx, |view, _| {
            // A press in flight: the gesture marker the reader also gates on.
            view.gesture
                .push(breeze_core::annotations::Point::new(0., 0.));
            assert!(
                view.begin_text_selection(press),
                "the press starts a selection"
            );
            assert!(view.select_text_at(1, &second, Some(right), onto_second));
            let crossed = view.selected_text.clone();
            assert!(
                crossed.contains("Space to think"),
                "the second page joins the selection: {crossed:?}"
            );
            assert_eq!(
                view.selection
                    .iter()
                    .map(|(page, _)| *page)
                    .collect::<Vec<_>>(),
                vec![0, 1],
                "both pages are drawn while the pointer is on the second"
            );
            // Back onto the first page: the selection follows it back.
            assert!(view.select_text_at(0, &first, Some(left), back));
            let returned = view.selected_text.clone();
            assert!(
                !returned.contains("Space to think"),
                "leaving the second page drops it again: {returned:?}"
            );
            assert_eq!(
                view.selection
                    .iter()
                    .map(|(page, _)| *page)
                    .collect::<Vec<_>>(),
                vec![0],
                "only the page the pointer is on is drawn"
            );
            assert!(
                !returned.is_empty(),
                "the selection follows the pointer in: {returned:?}"
            );
        });
        // A pointer over a page whose text is not loaded keeps the selection.
        let missing = Arc::new(Preview {
            page: 5,
            points: first.points,
            geometry: first.geometry,
            text: breeze_pdf::PageText::default(),
            text_warning: None,
            outline: first.outline.clone(),
            bytes: 0,
        });
        view.update(cx, |view, _| {
            assert!(!view.select_text_at(5, &missing, Some(left), back));
            assert_eq!(
                view.selection
                    .iter()
                    .map(|(page, _)| *page)
                    .collect::<Vec<_>>(),
                vec![0]
            );
        });
    })
    .unwrap();
}

#[gpui_kit::test]
fn a_press_that_never_moves_selects_nothing(cx: &mut TestAppContext) {
    let (handle, view) = spread_pages(cx);
    cx.update_window(handle, |_, window, cx| {
        let left = window
            .within(("page-frame", 0usize))
            .find("page-surface")
            .bounds();
        window.drag(
            left.origin + point(px(40.), px(200.)),
            left.origin + point(px(300.), px(140.)),
            cx,
        );
        assert!(!view.read(cx).selected_text.is_empty());
        window
            .within(("page-frame", 0usize))
            .click("page-surface", cx);
        let snapshot = view.read(cx);
        assert!(snapshot.text_selection.is_none());
        assert!(snapshot.selected_text.is_empty());
        assert!(snapshot.selection.is_empty());
    })
    .unwrap();
}

#[gpui_kit::test]
fn markup_a_cross_page_drag_creates_clamps_to_its_page(cx: &mut TestAppContext) {
    use breeze_core::annotations::Kind;
    let (handle, view) = spread_pages(cx);
    cx.update_window(handle, |_, window, cx| {
        view.update(cx, |view, _| {
            view.books[0].state.mode = Mode::Annotate;
            view.books[0].annotations.ready = true;
            view.tool = 0;
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
        window.drag(
            left.origin + point(px(40.), px(300.)),
            right.origin + point(px(200.), px(120.)),
            cx,
        );
        let history = &view.read(cx).books[0].annotations.history;
        let annotation = history.items.last().expect("a highlight was created");
        assert_eq!(annotation.kind, Kind::Highlight);
        assert_eq!(annotation.page, 0, "markup never spans pages");
        assert!(annotation.validate().is_ok());
        let quads = &annotation.quads;
        assert!(
            quads
                .iter()
                .all(|quad| quad.y >= 0. && quad.y + quad.height <= 792.),
            "every quad stays inside the page it marks"
        );
        assert!(
            quads.len() >= 2,
            "the markup covers the rows the drag passed over: {quads:?}"
        );
    })
    .unwrap();
}
