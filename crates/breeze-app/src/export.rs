//! Explicit PDF-copy export. Export receipts never acknowledge sidecar saves.
use crate::*;
use breeze_pdf::export::{ExportRequest, export_annotations};
impl Breeze {
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
        self.export_notice = Some(
            tr!("Choose a new PDF filename. Existing files are preserved. Notes export as icons with popup text.").into(),
        );
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
                    this.export_notice = Some(tf!(
                        "Cannot choose export destination: {}",
                        format!("{:#}", e)
                    ));
                    cx.notify();
                }
                Err(e) => {
                    this.export_busy = false;
                    this.export_notice = Some(tf!("File dialog interrupted: {}", e));
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
                        this.export_notice = Some(tf!(
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
                        ));
                        this.last_export = Some(receipt.destination);
                    }
                    Err(e) => {
                        this.export_notice = Some(tf!(
                            "Export failed; source and existing files are unchanged: {}",
                            format!("{:#}", e)
                        ))
                    }
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
}
