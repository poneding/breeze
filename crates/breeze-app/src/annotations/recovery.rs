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
            self.message = tr!("Finish or cancel the text annotation before recovery.").into();
            cx.notify();
            return None;
        }
        self.commit_selected_annotation(cx);
        let book = &mut self.books[self.active];
        let data = &mut book.annotations;
        if !data.ready || data.saving || data.fingerprint.is_empty() {
            self.message =
                tr!("Annotations are not ready or an operation is pending. Try again shortly.")
                    .into();
            cx.notify();
            return None;
        }
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
            self.message =
                tr!("A source conflict requires a recovery copy or explicit reassociation.").into();
        } else if self.books[self.active].annotations.ready {
            // Manual retries recheck both fingerprints; no revision is silently
            // refreshed.
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
                tr!("Old annotations refer to pages missing from this PDF. Reassociation is unavailable; keep a recovery copy.").into();
            cx.notify();
            return;
        }
        let Some(target) = self.recovery_target(cx) else {
            return;
        };
        let fingerprint = self.reading.fingerprint.clone();
        let reply = window.prompt(
            PromptLevel::Warning,
            tr!("Reassociate the retained annotations with the current PDF?"),
            Some(tr!("The document changed; old coordinates may no longer match its contents. Choose a new annotation file after confirming. Existing files are preserved. Check page numbers and positions first.")),
            &[tr!("Cancel"), tr!("Confirm reassociation")],
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
                    // Cancelling changes nothing, so it says nothing.
                    this.message.clear();
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
                            this.message =
                                tr!("Annotations changed during recovery; confirm again.").into();
                            cx.notify();
                            return;
                        }
                        this.write_annotation_recovery(target, path, reassociate, cx)
                    }
                    result => {
                        this.books[index].annotations.saving = false;
                        // Only a real problem earns a message; a declined dialog
                        // leaves the previous state untouched.
                        this.message = match result {
                            Ok(Err(e)) => tf!(
                                "Cannot choose annotation save location: {}",
                                format!("{e:#}")
                            ),
                            Err(e) => tf!("Annotation file dialog interrupted: {}", e),
                            _ => String::new(),
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
                                this.message = tr!("Annotations changed during recovery; confirm again.").into();
                                cx.notify();
                                return;
                            }
                            if index == this.active
                                && (this.reading.source.as_ref() != Some(&target.source)
                                    || this.reading.fingerprint != fingerprint)
                            {
                                this.message =
                                    tr!("The source changed during recovery. The copy is retained; reopen and confirm again.")
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
                            this.message = tf!("Annotations reassociated and saved: {}", path.display());
                        } else {
                            // A recovery copy retains the old fingerprint. It cannot
                            // clear conflicts, change associations or acknowledge saves.
                            this.message = tf!("Recovery copy saved: {}. The original association is unchanged; save or explicitly discard changes before closing.",
                                path.display()
                            );
                        }
                    }
                    Err(error) => {
                        this.message = tf!("Recovery failed; all annotations are retained: {}", format!("{error:#}"));
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
            tr!("Discard unsaved annotation changes for this document?"),
            Some(tr!("This cannot be undone. It restores the last saved state in memory and does not modify the source PDF or existing annotation files. Save a recovery copy first if you need to retain new annotations.")),
            &[tr!("Cancel"), tr!("Discard changes")],
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
                }
                // Discarding and cancelling both leave the files untouched, and the
                // confirmation dialog already stated that; no receipt follows.
                this.message.clear();
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
                                    tr!("Retry annotation save")
                                } else {
                                    tr!("Retry annotation load")
                                })
                                .disabled(data.saving)
                                .on_click(cx.listener(|this, _, _, cx| this.retry_annotations(cx))),
                        )
                    })
                    .when(data.ready, |d| {
                        d.child(
                            Button::new("recover-annotations")
                                .small()
                                .label(tr!("Save recovery copy"))
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
                                .label(tr!("Reassociate with current PDF"))
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
                        .label(tr!("Discard unsaved changes"))
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
                        .label(tr!("Reveal recovery copy"))
                        .on_click(move |_, _, cx| cx.reveal_path(&path)),
                )
            })
            .when_some(data.recovery_notice.clone(), |d, notice| {
                d.child(div().text_size(px(11.)).child(notice))
            })
    }
}
