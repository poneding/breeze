//! Document conversion and explicit PDF-copy export. Receipts do not save drafts.
use crate::*;
use breeze_pdf::export::{ExportRequest, export_annotations};
use gpui_kit::component::{
    Icon, WindowExt as _,
    menu::{DropdownMenu, PopupMenu, PopupMenuItem},
};
mod conversion;
use conversion::{ConversionRequest, DocumentSnapshot, ExportFormat};

/// Toolbar and pointer menus share format availability and export dispatch.
pub(super) fn export_menu(
    menu: PopupMenu,
    view: Entity<Breeze>,
    window: &mut Window,
    cx: &mut Context<PopupMenu>,
) -> PopupMenu {
    let format = view.read(cx).books[view.read(cx).active].format();
    let targets: &[ExportFormat] = match format {
        Some(DocumentFormat::Pdf) => &[ExportFormat::Markdown, ExportFormat::Word],
        Some(DocumentFormat::Markdown) => &[ExportFormat::Pdf, ExportFormat::Word],
        _ => {
            return menu.item(
                PopupMenuItem::new(tr!("Export"))
                    .icon(IconName::Download)
                    .disabled(true),
            );
        }
    };
    let enabled = view.read(cx).can_convert_document();
    let annotated = view.read(cx).can_export_pdf();
    menu.submenu_with_icon(
        Some(Icon::new(IconName::Download)),
        tr!("Export"),
        window,
        cx,
        move |mut menu, window, _| {
            for target in targets.iter().copied() {
                menu = menu.item(
                    PopupMenuItem::new(target.label())
                        .icon(IconName::FileOutput)
                        .disabled(!enabled)
                        .on_click(window.listener_for(&view, move |this, _, _, cx| {
                            this.choose_document_export(target, cx)
                        })),
                );
            }
            if format == Some(DocumentFormat::Pdf) {
                menu = menu.separator().item(
                    PopupMenuItem::new(tr!("Export annotated PDF"))
                        .icon(IconName::Highlighter)
                        .disabled(!annotated)
                        .on_click(
                            window.listener_for(&view, |this, _, _, cx| this.choose_pdf_export(cx)),
                        ),
                );
            }
            menu
        },
    )
}

#[cfg(test)]
mod tests;

impl Breeze {
    /// Export receipts belong to the originating window, outside reading status.
    fn show_export_result(&mut self, title: &'static str, message: String, cx: &mut Context<Self>) {
        self.export_notice = None;
        let handle = self.window_handle;
        cx.defer(move |cx| {
            let _ = handle.update(cx, |_, window, cx| {
                window.open_alert_dialog(cx, move |dialog, _, _| {
                    dialog.title(title).centered().ok_text(tr!("OK")).child(
                        div()
                            .id("export-result")
                            .test_support()
                            .aria_label(message.clone())
                            .child(message.clone()),
                    )
                });
            });
        });
    }

    pub(super) fn reader_more(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let view = cx.entity();
        Button::new("reader-more")
            .ghost()
            .small()
            .icon(IconName::Ellipsis)
            .accessibility_label(tr!("More document actions"))
            .tooltip(tr!("More document actions"))
            .dropdown_menu(move |menu, window, cx| export_menu(menu, view.clone(), window, cx))
    }

    fn can_convert_document(&self) -> bool {
        !self.export_busy
            && !self.loading
            && self.books.get(self.active).is_some_and(|book| {
                if book.is_pdf() {
                    !book.annotations.fingerprint.is_empty() && !book.annotations.source_conflict
                } else if book.is_markdown() {
                    book.markdown_edit.is_some() || self.reflow.markdown.is_some()
                } else {
                    false
                }
            })
    }

    fn conversion_snapshot(&self, format: ExportFormat, cx: &App) -> Option<ConversionRequest> {
        if !self.can_convert_document() {
            return None;
        }
        let book = self.books.get(self.active)?;
        let snapshot = if book.is_pdf()
            && matches!(format, ExportFormat::Markdown | ExportFormat::Word)
        {
            DocumentSnapshot::Pdf {
                fingerprint: book.annotations.fingerprint.clone(),
            }
        } else if book.is_markdown() && matches!(format, ExportFormat::Pdf | ExportFormat::Word) {
            let (text, fingerprint) = if let Some(editor) = &book.markdown_edit {
                editor.read(cx).export_source(cx)?
            } else {
                (
                    self.reflow.markdown.as_ref()?.source.to_string(),
                    self.reflow.markdown.as_ref()?.metadata.fingerprint.clone(),
                )
            };
            DocumentSnapshot::Markdown { text, fingerprint }
        } else {
            return None;
        };
        Some(ConversionRequest {
            source: book.source.clone(),
            snapshot,
            format,
            destination: PathBuf::new(),
        })
    }

    fn choose_document_export(&mut self, format: ExportFormat, cx: &mut Context<Self>) {
        let Some(mut request) = self.conversion_snapshot(format, cx) else {
            return;
        };
        let (directory, stem) = match &request.source {
            Source::File(path) => (
                path.parent()
                    .map(PathBuf::from)
                    .unwrap_or_else(std::env::temp_dir),
                path.file_stem()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "document".into()),
            ),
            Source::Sample => (
                self.data_dir.clone().unwrap_or_else(std::env::temp_dir),
                "field-notes".into(),
            ),
        };
        let name = format!("{stem}-export.{}", format.extension());
        let picker = cx.prompt_for_new_path(&directory, Some(&name));
        self.export_busy = true;
        self.export_notice = None;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let selection = picker.await;
            let _ = this.update(cx, |this, cx| match selection {
                Ok(Ok(Some(path))) => {
                    request.destination = path;
                    this.start_document_export(request, cx);
                }
                Ok(Ok(None)) => {
                    this.export_busy = false;
                    this.export_notice = None;
                    cx.notify();
                }
                error => {
                    this.export_busy = false;
                    this.show_export_result(
                        tr!("Export failed"),
                        tf!("Cannot choose export destination: {}", format!("{error:?}")),
                        cx,
                    );
                    cx.notify();
                }
            });
        })
        .detach();
    }

    fn start_document_export(&mut self, request: ConversionRequest, cx: &mut Context<Self>) {
        self.export_busy = true;
        self.export_notice = Some(
            if matches!(request.snapshot, DocumentSnapshot::Pdf { .. }) {
                tr!("Exporting PDF text, images and page positions…").into()
            } else {
                tr!("Exporting the current Markdown snapshot…").into()
            },
        );
        let work = cx.background_spawn(async move { conversion::convert(request) });
        cx.spawn(async move |this, cx| {
            let result = work.await;
            let _ = this.update(cx, |this, cx| {
                this.export_busy = false;
                match result {
                    Ok(receipt) => {
                        let mut notice = tf!("Exported {} ({} bytes)", receipt.destination.display(), receipt.bytes);
                        match receipt.pdf_conversion {
                            Some(ExportFormat::Word) => notice.push_str(tr!("; page positions and text styles retained; installed fonts may affect layout")),
                            Some(ExportFormat::Markdown) => notice.push_str(tr!("; Markdown reflows the text; keep the sibling image folder with this file")),
                            _ => {}
                        }
                        if receipt.empty_pages > 0 { notice.push_str(&tf!("; {} pages had no extractable text", receipt.empty_pages)); }
                        if receipt.simplified > 0 { notice.push_str(&tf!("; {} constructs retained as source or image links", receipt.simplified)); }
                        this.show_export_result(tr!("Export complete"), notice, cx);
                        this.last_export = Some(receipt.destination);
                    }
                    Err(error) => this.show_export_result(tr!("Export failed"), tf!("Export failed; source and existing files are unchanged: {}", format!("{error:#}")), cx),
                }
                cx.notify();
            });
        }).detach();
        cx.notify();
    }

    pub(super) fn can_export_pdf(&self) -> bool {
        !self.export_busy
            && !self.annotation_editing.pending_text()
            && !self.loading
            && self.books.get(self.active).is_some_and(|book| {
                let a = &book.annotations;
                a.ready
                    && !a.source_conflict
                    && a.error.is_none()
                    && !a.history.items.is_empty()
                    && !a.fingerprint.is_empty()
            })
    }
    pub(super) fn export_snapshot(&self, destination: PathBuf) -> Option<ExportRequest> {
        if !self.can_export_pdf() {
            return None;
        }
        let book = &self.books[self.active];
        Some(ExportRequest {
            source: book.source.clone(),
            source_fingerprint: book.annotations.fingerprint.clone(),
            annotations: book.annotations.history.items.clone(),
            destination,
        })
    }
    pub(super) fn choose_pdf_export(&mut self, cx: &mut Context<Self>) {
        if !self.books[self.active].is_pdf() {
            return;
        }
        self.commit_selected_annotation(cx);
        let Some(mut request) = self.export_snapshot(PathBuf::new()) else {
            self.message =
                tr!("No annotations to export, or the document is loading or has a conflict")
                    .into();
            cx.notify();
            return;
        };
        let (directory, name) = match &request.source {
            Source::File(path) => (
                path.parent()
                    .map(PathBuf::from)
                    .unwrap_or_else(std::env::temp_dir),
                format!(
                    "{}-annotated.pdf",
                    path.file_stem()
                        .map(|s| s.to_string_lossy())
                        .unwrap_or_default()
                ),
            ),
            Source::Sample => (
                self.data_dir.clone().unwrap_or_else(std::env::temp_dir),
                "field-notes-annotated.pdf".into(),
            ),
        };
        let picker = cx.prompt_for_new_path(&directory, Some(&name));
        self.export_busy = true;
        self.export_notice = None;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let selected = picker.await;
            let _ = this.update(cx, |this, cx| match selected {
                Ok(Ok(Some(path))) => {
                    request.destination = path;
                    this.start_pdf_export(request, cx);
                }
                Ok(Ok(None)) => {
                    this.export_busy = false;
                    // Cancelling the picker is not an outcome worth a notice.
                    this.export_notice = None;
                    cx.notify();
                }
                Ok(Err(e)) => {
                    this.export_busy = false;
                    this.show_export_result(
                        tr!("Export failed"),
                        tf!("Cannot choose export destination: {}", format!("{:#}", e)),
                        cx,
                    );
                    cx.notify();
                }
                Err(e) => {
                    this.export_busy = false;
                    this.show_export_result(
                        tr!("Export failed"),
                        tf!("File dialog interrupted: {}", e),
                        cx,
                    );
                    cx.notify();
                }
            });
        })
        .detach();
    }
    pub(super) fn start_pdf_export(&mut self, request: ExportRequest, cx: &mut Context<Self>) {
        self.export_busy = true;
        let label = match &request.source {
            Source::Sample => self.books[self.active].title.clone(),
            Source::File(p) => p
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| "PDF".into()),
        };
        self.export_notice = Some(tf!("Exporting {} and verifying content…", label));
        let work = cx.background_spawn(async move { export_annotations(request) });
        cx.spawn(async move |this, cx| {
            let result = work.await;
            let _ = this.update(cx, |this, cx| {
                this.export_busy = false;
                match result {
                    Ok(receipt) => {
                        this.show_export_result(
                            tr!("Export complete"),
                            tf!(
                                "Exported {}: {} annotations, {} bytes.{}{}",
                                label,
                                receipt.annotations,
                                receipt.bytes,
                                receipt.destination.display(),
                                if receipt.sticky_notes > 0 {
                                    tr!("; note text is available in annotation popups")
                                } else {
                                    ""
                                }
                            ),
                            cx,
                        );
                        this.last_export = Some(receipt.destination);
                    }
                    Err(e) => this.show_export_result(
                        tr!("Export failed"),
                        tf!(
                            "Export failed; source and existing files are unchanged: {}",
                            format!("{:#}", e)
                        ),
                        cx,
                    ),
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
}
