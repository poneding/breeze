//! Opening policies operate on window dimensions only, not reader zoom.
use crate::*;
#[derive(Clone, Copy, Debug, PartialEq)]
enum Change {
    None,
    Maximize,
    Resize { width: f32, height: f32 },
}
#[derive(Clone, Copy)]
pub(super) struct PendingFit {
    generation: u64,
    page: (f32, f32),
}
fn calculate(
    policy: usize,
    page: (f32, f32),
    space: (f32, f32),
    sidebar: f32,
    properties: f32,
) -> Change {
    match policy {
        1 => Change::Maximize,
        2 => {
            if ![page.0, page.1, space.0, space.1, sidebar, properties]
                .iter()
                .all(|n| n.is_finite())
                || page.0 <= 0.
                || page.1 <= 0.
            {
                return Change::None;
            }
            let available = ((space.0 - 64.).max(1.), (space.1 - 96.).max(1.));
            let chrome = (
                sidebar + properties + 24.,
                34. + style::TOOLBAR_HEIGHT + 32. + 24.,
            );
            let scale = ((available.0 - chrome.0) / page.0)
                .min((available.1 - chrome.1) / page.1)
                .clamp(0.1, 96. / 72.);
            Change::Resize {
                width: (page.0 * scale + chrome.0).clamp(900_f32.min(available.0), available.0),
                height: (page.1 * scale + chrome.1).clamp(650_f32.min(available.1), available.1),
            }
        }
        _ => Change::None,
    }
}
fn remaining_space(
    window: Bounds<gpui_kit::Pixels>,
    display: Bounds<gpui_kit::Pixels>,
) -> (f32, f32) {
    // GPUI resize preserves the origin. A display-sized height alone can put the footer off screen.
    let width = f32::from(display.size.width);
    let height = f32::from(display.size.height);
    (
        (f32::from(display.origin.x) + width - f32::from(window.origin.x)).clamp(1., width.max(1.)),
        (f32::from(display.origin.y) + height - f32::from(window.origin.y))
            .clamp(1., height.max(1.)),
    )
}
impl Breeze {
    pub(super) fn apply_opening_policy(&mut self, cx: &mut Context<Self>) {
        if !self.pending_window_policy {
            return;
        }
        self.pending_window_policy = false;
        let Some(preview) = &self.preview else {
            return;
        };
        let request = PendingFit {
            generation: self.generation,
            // Window fitting uses the page as the reader shows it.
            page: self.rotation().apply(preview.points),
        };
        let policy = self.window_policy;
        let handle = self.window_handle;
        let view = cx.weak_entity();
        cx.defer(move |cx| {
            let _ = handle.update(cx, |_, window, cx| {
                let _ = view.update(cx, |this, cx| {
                    if request.generation != this.generation
                        || this.screen != Screen::Reader
                        || window.is_fullscreen()
                    {
                        return;
                    }
                    match policy {
                        1 => {
                            if !window.is_maximized() {
                                window.zoom_window();
                            }
                        }
                        2 => {
                            if window.is_maximized() {
                                this.pending_fit = Some(request);
                                window.zoom_window();
                            } else {
                                this.fit_document_window(request.page, window, cx);
                            }
                        }
                        _ => {}
                    }
                });
            });
        });
    }
    pub(super) fn on_native_window_bounds(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(request) = self.pending_fit else {
            return;
        };
        if request.generation != self.generation
            || self.screen != Screen::Reader
            || window.is_fullscreen()
        {
            self.pending_fit = None;
            return;
        }
        if !window.is_maximized() {
            self.pending_fit = None;
            self.fit_document_window(request.page, window, cx);
        }
    }
    fn fit_document_window(&self, page: (f32, f32), window: &mut Window, cx: &App) {
        let bounds = window.bounds();
        let display = window.display(cx).map(|d| d.bounds()).unwrap_or(bounds);
        let sidebar = if self.sidebar {
            self.navigation_width
        } else {
            0.
        };
        let properties = self.document_panel.width();
        if let Change::Resize { width, height } = calculate(
            2,
            page,
            remaining_space(bounds, display),
            sidebar,
            properties,
        ) {
            window.resize(size(px(width), px(height)));
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn policies_are_distinct() {
        assert_eq!(
            calculate(0, (612., 792.), (1440., 1000.), 208., 0.),
            Change::None
        );
        assert_eq!(
            calculate(1, (612., 792.), (1440., 1000.), 208., 0.),
            Change::Maximize
        );
    }
    #[test]
    fn fit_is_finite_and_display_bounded() {
        for page in [(612., 792.), (792., 612.), (10000., 100.)] {
            let Change::Resize { width, height } = calculate(2, page, (1440., 1000.), 208., 232.)
            else {
                panic!("expected resize")
            };
            assert!((900.0..=1376.0).contains(&width));
            assert!((650.0..=904.0).contains(&height));
        }
    }
    #[test]
    fn small_display_and_invalid_geometry() {
        let Change::Resize { width, height } = calculate(2, (612., 792.), (800., 600.), 208., 0.)
        else {
            panic!("expected resize")
        };
        assert!(width <= 736. && height <= 504.);
        assert_eq!(
            calculate(2, (f32::NAN, 792.), (1440., 1000.), 208., 0.),
            Change::None
        );
    }
    #[test]
    fn fit_accounts_for_origin_and_secondary_displays() {
        let display = Bounds {
            origin: point(px(-1440.), px(0.)),
            size: size(px(1440.), px(1000.)),
        };
        let window = Bounds {
            origin: point(px(-1200.), px(200.)),
            size: size(px(1000.), px(700.)),
        };
        let space = remaining_space(window, display);
        assert_eq!(space, (1200., 800.));
        let Change::Resize { width, height } = calculate(2, (612., 792.), space, 208., 0.) else {
            panic!("expected resize")
        };
        assert!(width <= 1200.);
        assert!(height <= 800.);
    }
}
