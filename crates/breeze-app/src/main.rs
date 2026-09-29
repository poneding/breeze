#[cfg(test)]
use core::prelude::v1::test;
use gpui_kit::prelude::FluentBuilder;
mod appearance;
mod commands;
mod home;
mod reader;
mod settings;
mod style;
use anyhow::Result;
use appearance::{ColorScheme, ThemePreference};
use breeze_core::{Mode, ReaderState, Zoom};
use breeze_pdf::{PdfWorker, Pixels, Source};
use gpui_kit::component::{ResizableState, TitleBar};
use gpui_kit::{
    assets::{AllAssets, IconName},
    component::{
        Disableable, Selectable, Sizable,
        button::{Button, ButtonVariants},
        input::{Input, InputEvent, InputState},
    },
    *,
};
use std::{path::PathBuf, sync::Arc};
use style::{Palette, column, row};

actions!(
    breeze,
    [
        Open,
        Preferences,
        NextPage,
        PreviousPage,
        Home,
        Quit,
        CloseTab,
        NextTab,
        PreviousTab,
        ToggleSidebar,
        FocusPage,
        Favorite,
        CycleTheme,
        ReadMode,
        AnnotateMode,
        EditMode,
        ShowOutline,
        ShowThumbnails,
        ZoomIn,
        ZoomOut,
        ActualSize,
        FitPage,
        FitWidth
    ]
);
#[derive(Clone, Copy, PartialEq, Eq)]
enum Screen {
    Home,
    Reader,
    Settings,
}
struct Book {
    source: Source,
    title: String,
    favorite: bool,
    category: usize,
    state: ReaderState,
    opened: bool,
}
struct Preview {
    image: Arc<RenderImage>,
    points: (f32, f32),
    outline: Vec<breeze_pdf::Outline>,
    thumbnails: Vec<(usize, Arc<RenderImage>)>,
    elapsed_ms: f64,
}
struct Breeze {
    worker: PdfWorker,
    _subscriptions: Vec<Subscription>,
    books: Vec<Book>,
    active: usize,
    screen: Screen,
    dark: bool,
    theme_preference: ThemePreference,
    light_scheme: ColorScheme,
    dark_scheme: ColorScheme,
    navigation_split: Entity<ResizableState>,
    navigation_width: f32,
    sidebar: bool,
    thumbs: bool,
    library_filter: usize,
    generation: u64,
    loading: bool,
    preview: Option<Preview>,
    message: String,
    page_input: Entity<InputState>,
    font_input: Entity<InputState>,
    text_input: Entity<InputState>,
    focus: FocusHandle,
    tool: usize,
    bold: bool,
    italic: bool,
    font_size: u16,
    opacity: u16,
    alignment: usize,
    color: usize,
    background: usize,
    remember_files: bool,
    remember_position: bool,
    language: usize,
    update_policy: usize,
    window_policy: usize,
}
impl Breeze {
    fn new(worker: PdfWorker, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let text_input =
            cx.new(|cx| InputState::new(window, cx).default_value("让思绪，在此停留。"));
        let subscription = cx.observe(&text_input, |_, _, cx| cx.notify());
        let page_input = cx.new(|cx| InputState::new(window, cx).default_value("1"));
        let page_submission = cx.subscribe_in(&page_input, window, |this, _, event, window, cx| {
            if matches!(event, InputEvent::PressEnter { .. }) && this.screen == Screen::Reader {
                this.submit_page(cx);
                window.focus(&this.focus, cx);
            }
        });
        let navigation_split = cx.new(|_| ResizableState::default());
        let resize_observer = cx.observe(&navigation_split, |this, state, cx| {
            if let Some(width) = state.read(cx).sizes().first() {
                let width = f32::from(*width);
                if (this.navigation_width - width).abs() > 0.5 {
                    this.navigation_width = width;
                    cx.notify();
                }
            }
        });
        let appearance_observer = cx.observe_window_appearance(window, |this, window, cx| {
            if this.theme_preference == ThemePreference::System {
                let dark = this.theme_preference.resolve(window.appearance());
                if this.dark != dark {
                    this.dark = dark;
                    this.apply_colors(cx);
                    cx.notify();
                }
            }
        });
        Self {
            worker,
            _subscriptions: vec![
                subscription,
                page_submission,
                resize_observer,
                appearance_observer,
            ],
            books: vec![Book {
                source: Source::Sample,
                title: "Field notes · 阅读的留白".into(),
                favorite: false,
                category: 1,
                state: ReaderState::default(),
                opened: false,
            }],
            active: 0,
            screen: Screen::Home,
            dark: ThemePreference::System.resolve(window.appearance()),
            theme_preference: ThemePreference::System,
            light_scheme: ColorScheme::Neutral,
            dark_scheme: ColorScheme::Neutral,
            navigation_split,
            navigation_width: 208.,
            sidebar: true,
            thumbs: false,
            library_filter: 0,
            generation: 0,
            loading: false,
            preview: None,
            message: "".into(),
            page_input,
            font_input: cx.new(|cx| InputState::new(window, cx).default_value("系统默认")),
            text_input,
            focus: cx.focus_handle(),
            tool: 0,
            bold: false,
            italic: false,
            font_size: 18,
            opacity: 100,
            alignment: 0,
            color: 0,
            background: 0,
            remember_files: true,
            remember_position: true,
            language: 0,
            update_policy: 0,
            window_policy: 0,
        }
    }
    fn p(&self) -> Palette {
        Palette::new(
            self.dark,
            if self.dark {
                self.dark_scheme
            } else {
                self.light_scheme
            },
        )
    }
    fn show_home(&mut self, cx: &mut Context<Self>) {
        self.screen = Screen::Home;
        self.generation += 1;
        self.loading = false;
        cx.notify();
    }
    fn open_book(&mut self, index: usize, cx: &mut Context<Self>) {
        self.active = index;
        self.tool = 0;
        self.books[index].opened = true;
        self.screen = Screen::Reader;
        self.message.clear();
        self.load(cx);
    }
    fn load(&mut self, cx: &mut Context<Self>) {
        self.generation += 1;
        let generation = self.generation;
        self.loading = true;
        self.preview = None;
        let receiver = match self.worker.request(
            self.books[self.active].source.clone(),
            self.books[self.active].state.navigation.page(),
        ) {
            Ok(r) => r,
            Err(e) => {
                self.loading = false;
                self.message = format!("{e:#}");
                cx.notify();
                return;
            }
        };
        cx.spawn(async move |this, cx| {
            let result = receiver.await;
            let _ = this.update(cx, |this, cx| {
                if generation != this.generation {
                    return;
                }
                this.loading = false;
                match result {
                    Ok(Ok(page)) => {
                        let nav = &mut this.books[this.active].state.navigation;
                        nav.set_total(page.total);
                        nav.go_to(page.page);
                        let converted = (|| -> Result<Preview> {
                            Ok(Preview {
                                image: texture(page.pixels)?,
                                points: page.points,
                                outline: page.outline,
                                thumbnails: page
                                    .thumbnails
                                    .into_iter()
                                    .map(|(n, p)| Ok((n, texture(p)?)))
                                    .collect::<Result<Vec<_>>>()?,
                                elapsed_ms: page.elapsed_ms,
                            })
                        })();
                        match converted {
                            Ok(preview) => this.preview = Some(preview),
                            Err(e) => this.message = format!("{e:#}"),
                        }
                    }
                    Ok(Err(error)) => this.message = format!("{error:#}"),
                    Err(_) => this.message = "请求已取消，请重新打开文档".into(),
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
    fn next_page(&mut self, delta: i32, cx: &mut Context<Self>) {
        if self.screen != Screen::Reader {
            return;
        }
        let n = &mut self.books[self.active].state.navigation;
        let old = n.page();
        if delta > 0 {
            n.next()
        } else {
            n.previous()
        }
        if n.page() != old {
            self.load(cx);
        }
    }
    fn choose_file(&mut self, cx: &mut Context<Self>) {
        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("打开文档 · 当前原型支持 PDF".into()),
        });
        cx.spawn(async move |this, cx| match receiver.await {
            Ok(Ok(Some(paths))) => {
                if let Some(path) = paths.into_iter().next() {
                    let _ = this.update(cx, |this, cx| this.open_path(path, cx));
                }
            }
            Ok(Err(e)) => {
                let _ = this.update(cx, |this, cx| {
                    this.message = format!("文件对话框失败：{e}");
                    cx.notify();
                });
            }
            _ => {}
        })
        .detach();
    }
    fn open_path(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        if let Some(index) = self
            .books
            .iter()
            .position(|b| b.source == Source::File(path.clone()))
        {
            self.open_book(index, cx);
            return;
        }
        let title = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "文档".into());
        self.books.push(Book {
            source: Source::File(path),
            title,
            favorite: false,
            category: 0,
            state: ReaderState::default(),
            opened: true,
        });
        self.open_book(self.books.len() - 1, cx);
    }
    fn apply_colors(&self, cx: &mut App) {
        style::apply(
            self.dark,
            if self.dark {
                self.dark_scheme
            } else {
                self.light_scheme
            },
            cx,
        );
    }
    fn set_appearance(
        &mut self,
        preference: ThemePreference,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        self.theme_preference = preference;
        self.dark = preference.resolve(window.appearance());
        self.apply_colors(cx);
        cx.notify();
    }
}
impl Render for Breeze {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = self.p();
        let body = match self.screen {
            Screen::Home => self.home(cx).into_any_element(),
            Screen::Reader => self.reader(window, cx).into_any_element(),
            Screen::Settings => self.settings(cx).into_any_element(),
        };
        column()
            .size_full()
            .bg(p.bg)
            .text_color(p.text)
            .text_size(px(12.))
            .track_focus(&self.focus)
            .key_context("Breeze")
            .on_action(
                cx.listener(|this, action: &Open, window, cx| this.execute(action, window, cx)),
            )
            .on_action(cx.listener(|this, action: &Preferences, window, cx| {
                this.execute(action, window, cx)
            }))
            .on_action(
                cx.listener(|this, action: &Home, window, cx| this.execute(action, window, cx)),
            )
            .on_action(
                cx.listener(|this, action: &CloseTab, window, cx| this.execute(action, window, cx)),
            )
            .on_action(
                cx.listener(|this, action: &NextTab, window, cx| this.execute(action, window, cx)),
            )
            .on_action(cx.listener(|this, action: &PreviousTab, window, cx| {
                this.execute(action, window, cx)
            }))
            .on_action(cx.listener(|this, action: &ToggleSidebar, window, cx| {
                this.execute(action, window, cx)
            }))
            .on_action(
                cx.listener(|this, action: &FocusPage, window, cx| {
                    this.execute(action, window, cx)
                }),
            )
            .on_action(
                cx.listener(|this, action: &Favorite, window, cx| this.execute(action, window, cx)),
            )
            .on_action(
                cx.listener(|this, action: &CycleTheme, window, cx| {
                    this.execute(action, window, cx)
                }),
            )
            .on_action(
                cx.listener(|this, action: &ReadMode, window, cx| this.execute(action, window, cx)),
            )
            .on_action(cx.listener(|this, action: &AnnotateMode, window, cx| {
                this.execute(action, window, cx)
            }))
            .on_action(
                cx.listener(|this, action: &EditMode, window, cx| this.execute(action, window, cx)),
            )
            .on_action(cx.listener(|this, action: &ShowOutline, window, cx| {
                this.execute(action, window, cx)
            }))
            .on_action(cx.listener(|this, action: &ShowThumbnails, window, cx| {
                this.execute(action, window, cx)
            }))
            .on_action(
                cx.listener(|this, action: &NextPage, window, cx| this.execute(action, window, cx)),
            )
            .on_action(cx.listener(|this, action: &PreviousPage, window, cx| {
                this.execute(action, window, cx)
            }))
            .on_action(
                cx.listener(|this, action: &ZoomIn, window, cx| this.execute(action, window, cx)),
            )
            .on_action(
                cx.listener(|this, action: &ZoomOut, window, cx| this.execute(action, window, cx)),
            )
            .on_action(
                cx.listener(|this, action: &ActualSize, window, cx| {
                    this.execute(action, window, cx)
                }),
            )
            .on_action(
                cx.listener(|this, action: &FitPage, window, cx| this.execute(action, window, cx)),
            )
            .on_action(
                cx.listener(|this, action: &FitWidth, window, cx| this.execute(action, window, cx)),
            )
            .child(self.header(cx))
            .child(body)
    }
}
fn texture(p: Pixels) -> Result<Arc<RenderImage>> {
    let image = image::RgbaImage::from_raw(p.width, p.height, p.bgra)
        .ok_or_else(|| anyhow::anyhow!("页面像素大小不一致"))?;
    Ok(Arc::new(RenderImage::new(vec![image::Frame::new(image)])))
}
fn main() -> Result<()> {
    let worker = PdfWorker::new()?;
    let initial = std::env::args_os().nth(1).map(PathBuf::from);
    gpui_kit::application()
        .with_assets(AllAssets)
        .run(move |cx| {
            gpui_kit::init(cx);
            gpui_kit::component::set_locale("zh-CN");
            style::apply(
                ThemePreference::System.resolve(cx.window_appearance()),
                ColorScheme::Neutral,
                cx,
            );
            commands::bind(cx);
            cx.on_action(|_: &Quit, cx| cx.quit());
            cx.on_window_closed(|cx, _| {
                if cx.windows().is_empty() {
                    cx.quit();
                }
            })
            .detach();
            let result = gpui_kit::open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                        None,
                        size(px(1340.), px(900.)),
                        cx,
                    ))),
                    window_min_size: Some(size(px(900.), px(650.))),
                    ..TitleBar::window_options()
                },
                cx,
                move |window, cx| {
                    let view = cx.new(|cx| Breeze::new(worker, window, cx));
                    view.update(cx, |this, cx| {
                        window.focus(&this.focus, cx);
                        window.set_window_title("Breeze · 紧凑原型 R2");
                        this.apply_colors(cx);
                        if let Some(path) = initial {
                            this.open_path(path, cx);
                        }
                    });
                    view
                },
            );
            if let Err(error) = result {
                eprintln!("无法启动 Breeze：{error:#}");
                cx.quit();
            }
            cx.activate(true);
        });
    Ok(())
}

#[cfg(test)]
mod ui_tests;
