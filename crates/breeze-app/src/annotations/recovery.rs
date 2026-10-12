//! Explicit recovery never acknowledges a normal sidecar save or applies old
//! coordinates to a changed PDF without the user's confirmation.
use super::*;
#[cfg(test)]
mod tests;

#[derive(Clone)]
struct RecoveryTarget {
    source: Source,
    identity: Arc<()>,
    fingerprint: String,
    items: Vec<Annotation>,
}
impl RecoveryTarget {
    fn index(&self, view: &Breeze) -> Option<usize> {
        view.books.iter().position(|book| {
            book.source == self.source
                && Arc::ptr_eq(&book.annotations.identity, &self.identity)
                && book.annotations.fingerprint == self.fingerprint
        })
    }
}

impl Breeze {
    fn recovery_target(&mut self, cx: &mut Context<Self>) -> Option<RecoveryTarget> {
        if self.annotation_editing.pending_text() {
            self.message = tr!("请先完成或取消文本标注，再处理恢复").into();
            cx.notify();
            return None;
        }
        self.commit_selected_annotation(cx);
        let book = &mut self.books[self.active];
        let data = &mut book.annotations;
        if !data.ready || data.saving || data.fingerprint.is_empty() {
            self.message = tr!("批注尚未就绪或正在处理，请稍后重试").into();
            cx.notify();
            return None;
        }
        data.autosave = None;
        data.saving = true;
        data.recovery_notice = None;
        self.gesture.clear();
        self.annotation_editing.clear();
        Some(RecoveryTarget {
            source: book.source.clone(),
            identity: data.identity.clone(),
            fingerprint: data.fingerprint.clone(),
            items: data.history.items.clone(),
        })
    }

    pub(super) fn retry_annotations(&mut self, cx: &mut Context<Self>) {
        if self.annotation_editing.pending_text() || self.books[self.active].annotations.saving {
            return;
        }
        if self.books[self.active].annotations.source_conflict {
            self.message = tr!("源文档冲突需要恢复副本或明确重新关联").into();
        } else if self.books[self.active].annotations.ready {
            // Manual retries recheck both fingerprints. Autosave stays paused
            // until a retry succeeds; no revision is silently refreshed.
            self.save_annotations(cx);
        } else if self.reading.source.as_ref() == Some(&self.books[self.active].source)
            && !self.reading.fingerprint.is_empty()
        {
            self.books[self.active].annotations.fingerprint.clear();
            self.prepare_annotations(self.reading.fingerprint.clone(), cx);
        }
        cx.notify();
    }

    pub(super) fn choose_annotation_recovery(&mut self, cx: &mut Context<Self>) {
        if let Some(target) = self.recovery_target(cx) {
            self.choose_recovery_path(target, None, cx);
        }
    }

    pub(super) fn confirm_annotation_reassociation(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.loading
            || self.reading.source.as_ref() != Some(&self.books[self.active].source)
            || self.reading.fingerprint.is_empty()
            || !self.books[self.active].annotations.source_conflict
        {
            return;
        }
        if self.books[self.active]
            .annotations
            .history
            .items
            .iter()
            .any(|a| a.page >= self.reading.sizes.len())
        {
            self.message =
                tr!("旧批注包含当前 PDF 不存在的页码，不能重新关联；请保留恢复副本").into();
            cx.notify();
            return;
        }
        let Some(target) = self.recovery_target(cx) else {
            return;
        };
        let fingerprint = self.reading.fingerprint.clone();
        let reply = window.prompt(
            PromptLevel::Warning,
            tr!("将保留的批注关联到当前 PDF？"),
            Some(tr!("文档内容已变化，旧批注坐标可能不再对应原文。确认后选择新的批注文件；旧文件保持不变。请先核对页码和位置。")),
            &[tr!("取消"), tr!("确认重新关联")],
            cx,
        );
        cx.notify();
        cx.spawn(async move |this, cx| {
            let answer = reply.await;
            let _ = this.update(cx, |this, cx| {
                let Some(index) = target.index(this) else {
                    return;
                };
                if answer == Ok(1) && this.books[index].annotations.history.items == target.items {
                    this.choose_recovery_path(target, Some(fingerprint), cx);
                } else {
                    this.books[index].annotations.saving = false;
                    this.message = tr!("已取消恢复操作，批注和文件保持不变").into();
                    cx.notify();
                }
            });
        })
        .detach();
    }

    fn choose_recovery_path(
        &mut self,
        target: RecoveryTarget,
        reassociate: Option<String>,
        cx: &mut Context<Self>,
    ) {
        let directory = self.data_dir.clone().unwrap_or_else(std::env::temp_dir);
        let name = if reassociate.is_some() {
            "reassociated.breeze.json"
        } else {
            "recovery.breeze.json"
        };
        let picker = cx.prompt_for_new_path(&directory, Some(name));
        cx.notify();
        cx.spawn(async move |this, cx| {
            let selected = picker.await;
            let _ = this.update(cx, |this, cx| {
                let Some(index) = target.index(this) else {
                    return;
                };
                match selected {
                    Ok(Ok(Some(path))) => {
                        if reassociate.is_some()
                            && this.books[index].annotations.history.items != target.items
                        {
                            this.books[index].annotations.saving = false;
                            this.message = tr!("恢复期间批注已变化，请重新确认").into();
                            cx.notify();
                            return;
                        }
                        this.write_annotation_recovery(target, path, reassociate, cx)
                    }
                    result => {
                        this.books[index].annotations.saving = false;
                        this.message = match result {
                            Ok(Err(e)) => tf!("无法选择批注保存位置：{}", format!("{e:#}")),
                            Err(e) => tf!("批注文件对话框已中断：{}", e),
                            _ => tr!("已取消恢复操作，批注和文件保持不变").into(),
                        };
                        cx.notify();
                    }
                }
            });
        })
        .detach();
    }

    fn write_annotation_recovery(
        &mut self,
        target: RecoveryTarget,
        path: PathBuf,
        reassociate: Option<String>,
        cx: &mut Context<Self>,
    ) {
        let fingerprint = reassociate
            .clone()
            .unwrap_or_else(|| target.fingerprint.clone());
        let source = if reassociate.is_some() {
            match &target.source {
                Source::File(path) => Some(path.clone()),
                Source::Sample => None,
            }
        } else {
            None
        };
        let items = target.items.clone();
        let destination = path.clone();
        let work = cx.background_spawn(async move {
            breeze_storage::save_annotations(
                &destination,
                source.as_deref(),
                &fingerprint,
                None,
                items,
            )
        });
        cx.spawn(async move |this, cx| {
            let result = work.await;
            let _ = this.update(cx, |this, cx| {
                let Some(index) = target.index(this) else {
                    return;
                };
                let data = &mut this.books[index].annotations;
                data.saving = false;
                match result {
                    Ok(revision) => {
                        data.recovery_path = Some(path.clone());
                        if let Some(fingerprint) = reassociate {
                            if data.history.items != target.items {
                                this.message = tr!("恢复期间批注已变化，请重新确认").into();
                                cx.notify();
                                return;
                            }
                            if index == this.active
                                && (this.reading.source.as_ref() != Some(&target.source)
                                    || this.reading.fingerprint != fingerprint)
                            {
                                this.message =
                                    tr!("恢复期间源文档已变化，副本已保留，请重新打开并确认")
                                        .into();
                                data.recovery_notice = Some(this.message.clone());
                                cx.notify();
                                return;
                            }
                            data.fingerprint = fingerprint;
                            data.path = Some(path.clone());
                            data.revision = Some(revision);
                            data.saved = target.items;
                            data.source_conflict = false;
                            data.failure = None;
                            data.error = None;
                            this.message = tf!("已重新关联并保存批注：{}", path.display());
                            if data.dirty() {
                                this.schedule_annotation_save(index, cx);
                            }
                        } else {
                            // A recovery copy retains the old fingerprint. It cannot
                            // clear conflicts, change associations or acknowledge saves.
                            this.message = tf!(
                                "恢复副本已保存：{}；原关联保持不变，关闭前仍需保存或明确放弃更改",
                                path.display()
                            );
                        }
                    }
                    Err(error) => {
                        this.message = tf!("恢复操作失败，保留全部批注：{}", format!("{error:#}"));
                    }
                }
                this.books[index].annotations.recovery_notice = Some(this.message.clone());
                cx.notify();
            });
        })
        .detach();
    }

    pub(super) fn confirm_discard_annotations(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.books[self.active].annotations.dirty() {
            return;
        }
        let Some(target) = self.recovery_target(cx) else {
            return;
        };
        let answer = window.prompt(
            PromptLevel::Warning,
            tr!("放弃当前文档的未保存批注更改？"),
            Some(tr!("此操作无法撤销，只恢复内存中的上次保存状态，不修改源 PDF 或已有批注文件。需要保留新批注时，请先保存恢复副本。")),
            &[tr!("取消"), tr!("放弃更改")],
            cx,
        );
        cx.notify();
        cx.spawn(async move |this, cx| {
            let answer = answer.await;
            let _ = this.update(cx, |this, cx| {
                let Some(index) = target.index(this) else {
                    return;
                };
                let data = &mut this.books[index].annotations;
                data.saving = false;
                if answer == Ok(1) && data.history.items == target.items {
                    data.history = History::default();
                    data.history.items = data.saved.clone();
                    if this.active == index {
                        this.annotation_editing.clear();
                        this.gesture.clear();
                    }
                    this.message = tr!("已放弃未保存更改；已有文件保持不变").into();
                } else {
                    this.message = tr!("已取消恢复操作，批注和文件保持不变").into();
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(crate) fn annotation_recovery_controls(&self, cx: &mut Context<Self>) -> Div {
        let data = &self.books[self.active].annotations;
        column()
            .gap_1()
            .when(
                data.error.is_some() || data.source_conflict || data.dirty(),
                |d| {
                    d.when(data.error.is_some() && !data.source_conflict, |d| {
                        d.child(
                            Button::new("retry-annotations")
                                .small()
                                .label(if data.ready {
                                    tr!("重试保存批注")
                                } else {
                                    tr!("重试载入批注")
                                })
                                .disabled(data.saving)
                                .on_click(cx.listener(|this, _, _, cx| this.retry_annotations(cx))),
                        )
                    })
                    .when(data.ready, |d| {
                        d.child(
                            Button::new("recover-annotations")
                                .small()
                                .label(tr!("保存恢复副本"))
                                .disabled(data.saving)
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.choose_annotation_recovery(cx)
                                })),
                        )
                    })
                    .when(data.source_conflict && data.ready, |d| {
                        d.child(
                            Button::new("reassociate-annotations")
                                .small()
                                .label(tr!("重新关联当前 PDF"))
                                .disabled(data.saving || self.loading)
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.confirm_annotation_reassociation(window, cx)
                                })),
                        )
                    })
                },
            )
            .when(data.dirty(), |d| {
                d.child(
                    Button::new("discard-annotation-changes")
                        .small()
                        .label(tr!("放弃未保存更改"))
                        .disabled(data.saving)
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.confirm_discard_annotations(window, cx)
                        })),
                )
            })
            .when_some(data.recovery_path.clone(), |d, path| {
                d.child(
                    Button::new("reveal-annotation-recovery")
                        .ghost()
                        .small()
                        .label(tr!("显示恢复副本"))
                        .on_click(move |_, _, cx| cx.reveal_path(&path)),
                )
            })
            .when_some(data.recovery_notice.clone(), |d, notice| {
                d.child(div().text_size(px(11.)).child(notice))
            })
    }
}
