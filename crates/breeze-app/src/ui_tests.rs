use super::*;
use gpui_kit::test::TestWindowExt;

fn app(cx: &mut TestAppContext) -> (AnyWindowHandle, Entity<Breeze>) {
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
