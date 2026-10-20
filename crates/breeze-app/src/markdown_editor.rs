//! Markdown source editing with syntax highlighting.
//!
//! The reading surface is the rendered preview; this surface is the editable
//! Markdown source. Editing uses GPUI Kit's code editor, which owns text,
//! selection, undo/redo, IME composition, soft wrap and the tree-sitter
//! Markdown highlighter. Breeze supplies the file, font size, dirty tracking
//! for the close guards and the status line.
//!
//! Markdown syntax highlighting comes from Kit's `tree-sitter-markdown`
//! feature; see `Cargo.toml` for the pinned component release.
use crate::*;
use breeze_reflow::markdown::SourceBuffer;
use gpui_kit::component::input::{Editor, EditorState};
use std::{
    cell::Cell,
    path::{Path, PathBuf},
};

/// The editing session behind the reader's content area.
pub(super) struct MarkdownEditor {
    state: Entity<EditorState>,
    focus: FocusHandle,
    /// The reader's body size; the editor reflows when it changes.
    font_size: u8,
    /// Text as loaded, so undoing back to it makes the draft clean again.
    saved: String,
    /// Cached state, refreshed from the editor's text when it is read.
    dirty: Cell<bool>,
    characters: Cell<usize>,
    synced_len: Cell<usize>,
    notice: Option<SharedString>,
}

impl MarkdownEditor {
    pub(super) fn open(
        path: PathBuf,
        font_size: u8,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let (text, notice) = load_text(&path);
        let saved = text.clone();
        let characters = text.chars().count();
        let state = cx.new(|cx| {
            EditorState::new(window, cx)
                // Markdown highlighting, soft wrapped like prose. Line numbers
                // and folding belong to code; a note should read as a page.
                .language("markdown")
                .soft_wrap(true)
                .line_number(false)
                .searchable(true)
                .default_value(text)
        });
        let focus = state.read(cx).focus_handle(cx);
        let synced_len = saved.len();
        Self {
            state,
            focus,
            font_size,
            saved,
            dirty: Cell::new(false),
            characters: Cell::new(characters),
            synced_len: Cell::new(synced_len),
            notice: notice.map(Into::into),
        }
    }

    /// Follow the reader's font size.
    pub(super) fn set_font_size(&mut self, font_size: u8, cx: &mut Context<Self>) {
        if self.font_size == font_size {
            return;
        }
        self.font_size = font_size;
        cx.notify();
    }

    pub(super) fn focus_handle(&self) -> FocusHandle {
        self.focus.clone()
    }

    /// Whether the draft differs from the text as loaded.
    ///
    /// A composition and a committed keystroke both change the editor's text;
    /// reading the text here rather than tracking events keeps the answer
    /// correct while an input method is still composing. The check costs one
    /// byte-length read, plus one content comparison at equal lengths.
    pub(super) fn dirty(&self, cx: &App) -> bool {
        self.sync(cx, true);
        self.dirty.get()
    }

    /// The draft's text, for the reading preview.
    pub(super) fn source_text(&self, cx: &App) -> String {
        self.state.read(cx).value().to_string()
    }

    /// Characters in the draft, refreshed when its byte length changed.
    pub(super) fn characters(&self, cx: &App) -> usize {
        self.sync(cx, false);
        self.characters.get()
    }

    pub(super) fn status(&self, cx: &App) -> SharedString {
        self.sync(cx, false);
        let mut status = tr!("Markdown source").to_string();
        if self.dirty.get() {
            status.push_str(" · ");
            status.push_str(tr!("modified"));
        }
        status.into()
    }

    /// Refresh the cached count and dirtiness from the editor's text.
    ///
    /// `deep` additionally compares contents when the byte length is unchanged,
    /// which is what recognizes an equal-length replacement and an undo back to
    /// the saved checkpoint.
    fn sync(&self, cx: &App, deep: bool) {
        let len = {
            let text = self.state.read(cx).text();
            text.len()
        };
        if len == self.synced_len.get() {
            if deep {
                let text = self.state.read(cx).value();
                let dirty = len != self.saved.len() || text.as_ref() != self.saved;
                self.dirty.set(dirty);
            }
            return;
        }
        let text = self.state.read(cx).value();
        self.synced_len.set(len);
        self.characters.set(text.chars().count());
        self.dirty
            .set(len != self.saved.len() || text.as_ref() != self.saved);
    }
}

/// Read the draft, refusing the same limits the source model always had.
fn load_text(path: &Path) -> (String, Option<String>) {
    let failure = |error: anyhow::Error| {
        (
            String::new(),
            Some(tf!(
                "Cannot edit {}: {}",
                path.display(),
                format!("{error:#}")
            )),
        )
    };
    match std::fs::read(path) {
        Ok(bytes) => match SourceBuffer::from_bytes(&bytes).and_then(|buffer| {
            let range = 0..buffer.len();
            buffer.text(range)
        }) {
            Ok(text) => (text, None),
            Err(error) => failure(error),
        },
        Err(error) => failure(error.into()),
    }
}

impl Render for MarkdownEditor {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = Theme::global(cx).clone();
        let notice = self.notice.clone();
        let body = f32::from(self.font_size);
        div()
            .id("markdown-editor")
            .test_support()
            .size_full()
            .min_h_0()
            .flex()
            .flex_col()
            .when_some(notice, |root, notice| {
                root.child(
                    div()
                        .px_3()
                        .py_1()
                        .text_size(px(12.))
                        .text_color(theme.danger)
                        .child(notice),
                )
            })
            .child(
                div().flex_1().min_h_0().w_full().child(
                    Editor::new(&self.state)
                        .appearance(false)
                        .bordered(false)
                        .text_size(px(body))
                        .h(relative(1.)),
                ),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(bytes: &[u8]) -> (tempfile::TempDir, PathBuf) {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("draft.md");
        std::fs::write(&path, bytes).unwrap();
        (directory, path)
    }

    #[test]
    fn a_readable_file_loads_without_a_notice() {
        let (_directory, path) = write("# Title\n\nbody\n".as_bytes());
        let (text, notice) = load_text(&path);
        assert_eq!(text, "# Title\n\nbody\n");
        assert!(notice.is_none());
    }

    #[test]
    fn invalid_utf8_is_reported_with_the_file() {
        let (_directory, path) = write(b"ok \xff");
        let (text, notice) = load_text(&path);
        assert!(text.is_empty(), "no partial text is edited");
        let notice = notice.expect("an explicit error");
        assert!(notice.contains("draft.md"), "{notice}");
    }
}
