//! Explicit PDF-copy export. Export receipts never acknowledge sidecar saves.
use crate::*;
use breeze_pdf::export::{ExportRequest, export_annotations};
impl Breeze {
    pub(super) fn can_export_pdf(&self) -> bool {
        !self.export_busy
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
        let Some(mut request) = self.export_snapshot(PathBuf::new()) else {
            self.message = "没有可导出的批注，或文档仍在加载/存在冲突".into();
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
        self.export_notice =
            Some("选择新的 PDF 文件名；不会覆盖原件或已有文件。便笺以图标和弹窗内容导出。".into());
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
                    this.export_notice = Some("已取消导出".into());
                    cx.notify();
                }
                Ok(Err(e)) => {
                    this.export_busy = false;
                    this.export_notice = Some(format!("无法选择导出位置：{e:#}"));
                    cx.notify();
                }
                Err(e) => {
                    this.export_busy = false;
                    this.export_notice = Some(format!("文件对话框已中断：{e}"));
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
        self.export_notice = Some(format!("正在导出 {label} 并验证内容…"));
        let work = cx.background_spawn(async move { export_annotations(request) });
        cx.spawn(async move |this, cx| {
            let result = work.await;
            let _ = this.update(cx, |this, cx| {
                this.export_busy = false;
                match result {
                    Ok(receipt) => {
                        this.export_notice = Some(format!(
                            "已导出 {label}：{} 条批注，{} 字节。{}{}",
                            receipt.annotations,
                            receipt.bytes,
                            receipt.destination.display(),
                            if receipt.sticky_notes > 0 {
                                "；便笺内容保留在批注弹窗中"
                            } else {
                                ""
                            }
                        ));
                        this.last_export = Some(receipt.destination);
                    }
                    Err(e) => {
                        this.export_notice = Some(format!("导出失败，未替换原件或已有文件：{e:#}"))
                    }
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
    pub(super) fn export_controls(&self, cx: &mut Context<Self>) -> Div {
        let p = self.p(cx);
        column().gap_2()
            .child(self.command("export-pdf",IconName::Download,"导出批注 PDF（验证版）",&ExportPdf,cx).label(if self.export_busy{"正在导出…"}else{"导出 PDF · 验证版"}).disabled(!self.can_export_pdf()))
            .child(div().text_size(px(11.)).text_color(p.muted).child("新文件、不覆盖原件。文本便笺导出为图标，全文保留在批注弹窗中。表单、签名、加密文档暂不支持。"))
            .when_some(self.export_notice.clone(),|d,message|d.child(div().text_size(px(12.)).child(message)))
            .when_some(self.last_export.clone(),|d,path|d.child(Button::new("reveal-export").ghost().small().icon(IconName::FolderOpen).label("显示上次导出文件").on_click(move|_,_,cx|cx.reveal_path(&path))))
    }
}
