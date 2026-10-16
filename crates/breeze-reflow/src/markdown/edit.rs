//! Source editing is independent of widgets, parsing and persistence jobs.
use super::{
    MAX_SOURCE_BYTES, Selection, SourceBuffer,
    history::{Delta, Entry, History},
};
use anyhow::{Context, Result, ensure};
use std::{
    ops::Range,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Instant,
};

static NEXT_SESSION: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditKind {
    Typing,
    Atomic,
}

#[derive(Clone, Copy, Debug)]
pub struct EditLimits {
    pub source_bytes: usize,
    pub history_bytes: usize,
    pub history_entries: usize,
}

impl Default for EditLimits {
    fn default() -> Self {
        Self {
            source_bytes: MAX_SOURCE_BYTES,
            history_bytes: 32 * 1024 * 1024,
            history_entries: 256,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Replacement {
    pub range: Range<usize>,
    pub text: Arc<str>,
}

impl Replacement {
    pub fn new(range: Range<usize>, text: impl Into<Arc<str>>) -> Self {
        Self {
            range,
            text: text.into(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Snapshot {
    session: u64,
    revision: u64,
    checkpoint: u64,
    source: SourceBuffer,
}

impl Snapshot {
    pub fn session_id(&self) -> u64 {
        self.session
    }
    pub fn revision(&self) -> u64 {
        self.revision
    }
    pub fn source(&self) -> &SourceBuffer {
        &self.source
    }
}

#[derive(Debug)]
pub struct EditSession {
    session: u64,
    revision: u64,
    checkpoint: u64,
    next_checkpoint: u64,
    source: SourceBuffer,
    selection: Selection,
    saved: Snapshot,
    equivalent_saved: Option<u64>,
    history: History,
    limits: EditLimits,
    composition: Option<Composition>,
}

/// In-flight native text composition: a contiguous source range that the input
/// method is rewriting. Preview updates never enter the history; the whole
/// composition becomes one entry when it commits.
#[derive(Debug)]
struct Composition {
    before: Snapshot,
    before_selection: Selection,
    range: Range<usize>,
    original: Arc<str>,
}

impl EditSession {
    /// Source loading and validation must take place off the UI thread.
    pub fn new(source: SourceBuffer) -> Result<Self> {
        Self::with_limits(source, EditLimits::default())
    }

    /// An empty in-memory session for a new document.
    pub fn empty() -> Self {
        match Self::with_limits(SourceBuffer::default(), EditLimits::default()) {
            Ok(session) => session,
            // The empty source satisfies every constructor precondition.
            Err(_) => unreachable!("an empty Markdown source is always a valid session"),
        }
    }

    pub fn with_limits(source: SourceBuffer, limits: EditLimits) -> Result<Self> {
        ensure!(
            limits.source_bytes <= MAX_SOURCE_BYTES && limits.source_bytes >= source.len(),
            "Markdown exceeds the session source limit"
        );
        ensure!(
            limits.history_entries > 0 && limits.history_bytes > 0,
            "Undo history must retain at least one transaction"
        );
        let mut current = NEXT_SESSION.load(Ordering::Relaxed);
        let session = loop {
            let next = current
                .checked_add(1)
                .ok_or_else(|| anyhow::anyhow!("Editor session identity exhausted"))?;
            match NEXT_SESSION.compare_exchange_weak(
                current,
                next,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => break current,
                Err(observed) => current = observed,
            }
        };
        let saved = Snapshot {
            session,
            revision: 0,
            checkpoint: 0,
            source: source.clone(),
        };
        Ok(Self {
            session,
            revision: 0,
            checkpoint: 0,
            next_checkpoint: 1,
            source,
            selection: Selection::default(),
            saved,
            equivalent_saved: None,
            history: History::default(),
            limits,
            composition: None,
        })
    }

    pub fn source(&self) -> &SourceBuffer {
        &self.source
    }
    pub fn selection(&self) -> Selection {
        self.selection
    }
    pub fn revision(&self) -> u64 {
        self.revision
    }
    pub fn session_id(&self) -> u64 {
        self.session
    }
    pub fn history_bytes(&self) -> usize {
        self.history.bytes
    }
    pub fn undo_count(&self) -> usize {
        self.history.undo.len()
    }
    pub fn redo_count(&self) -> usize {
        self.history.redo.len()
    }
    pub fn dirty(&self) -> bool {
        self.checkpoint != self.saved.checkpoint && self.equivalent_saved != Some(self.checkpoint)
    }

    pub fn snapshot(&self) -> Snapshot {
        Snapshot {
            session: self.session,
            revision: self.revision,
            checkpoint: self.checkpoint,
            source: self.source.clone(),
        }
    }
    pub fn saved_snapshot(&self) -> Snapshot {
        self.saved.clone()
    }
    pub fn accepts(&self, snapshot: &Snapshot) -> bool {
        snapshot.session == self.session && snapshot.revision == self.revision
    }
    pub fn break_undo_group(&mut self) {
        self.history.can_merge = false;
    }

    pub fn set_selection(&mut self, selection: Selection) -> Result<()> {
        selection.validate(&self.source)?;
        if selection != self.selection {
            self.break_undo_group();
        }
        self.selection = selection;
        Ok(())
    }

    /// Call before publishing a save so later typing cannot merge across its checkpoint.
    pub fn save_snapshot(&mut self) -> Snapshot {
        self.break_undo_group();
        self.snapshot()
    }

    /// A successful receipt acknowledges its captured content, never later typing.
    pub fn acknowledge_saved(&mut self, snapshot: Snapshot) -> bool {
        if snapshot.session != self.session || snapshot.revision < self.saved.revision {
            return false;
        }
        self.saved = snapshot;
        self.equivalent_saved = None;
        self.break_undo_group();
        true
    }

    /// Equality is computed off-thread; both compared snapshots must still be current.
    pub fn acknowledge_equality(
        &mut self,
        current: &Snapshot,
        saved: &Snapshot,
        equal: bool,
    ) -> bool {
        if !self.accepts(current)
            || saved.session != self.session
            || saved.revision != self.saved.revision
            || saved.checkpoint != self.saved.checkpoint
        {
            return false;
        }
        self.equivalent_saved = equal.then_some(current.checkpoint);
        true
    }

    /// The application coordinator can reclaim history without touching dirty source.
    pub fn trim_history(&mut self, bytes: usize, entries: usize) {
        self.history.trim(bytes, entries);
        self.break_undo_group();
    }

    pub fn composition_range(&self) -> Option<Range<usize>> {
        self.composition
            .as_ref()
            .map(|composition| composition.range.clone())
    }

    /// Begin a native composition over the current selection. The returned
    /// range is the source span the composition will rewrite.
    pub fn begin_composition(&mut self) -> Result<Range<usize>> {
        ensure!(
            self.composition.is_none(),
            "A composition is already active"
        );
        let range = self.selection.range();
        let original: Arc<str> = self.source.text(range.clone())?.into();
        self.composition = Some(Composition {
            before: self.snapshot(),
            before_selection: self.selection,
            range: range.clone(),
            original,
        });
        Ok(range)
    }

    /// Replace the in-flight composition's text without touching the history.
    pub fn apply_composition(&mut self, text: &str, selection: Selection) -> Result<bool> {
        let Some(composition) = self.composition.as_ref() else {
            anyhow::bail!("No composition is active");
        };
        let range = composition.range.clone();
        if range.is_empty() && text.is_empty() {
            return Ok(false);
        }
        let mut candidate = self.source.clone();
        candidate.rope.try_remove(range.clone())?;
        candidate.rope.try_insert(range.start, text)?;
        selection.validate(&candidate)?;
        let length = self.source.len() - range.len() + text.len();
        ensure!(
            length <= self.limits.source_bytes,
            "Markdown exceeds the session source limit"
        );
        ensure!(
            std::mem::size_of::<Delta>() + range.len() + text.len() <= self.limits.history_bytes,
            "This composition exceeds the undo byte budget"
        );
        let revision = self
            .revision
            .checked_add(1)
            .context("Editor revision exhausted")?;
        let checkpoint = self.next_checkpoint;
        self.next_checkpoint = self
            .next_checkpoint
            .checked_add(1)
            .context("Editor checkpoint exhausted")?;
        if let Some(composition) = self.composition.as_mut() {
            composition.range = range.start..range.start + text.len();
        }
        self.source = candidate;
        self.selection = selection;
        self.revision = revision;
        self.checkpoint = checkpoint;
        Ok(true)
    }

    /// Finish a composition as one history entry, or a no-op when the text did
    /// not change.
    pub fn commit_composition(&mut self, text: &str, selection: Selection) -> Result<bool> {
        let Some(composition) = self.composition.as_ref() else {
            return Ok(false);
        };
        let range = composition.range.clone();
        if range.is_empty() && text.is_empty() {
            self.composition = None;
            self.break_undo_group();
            return Ok(false);
        }
        self.apply_composition(text, selection)?;
        let Some(composition) = self.composition.take() else {
            return Ok(false);
        };
        let final_range = composition.range.clone();
        if final_range == range && self.source.text(range.clone())? == *composition.original {
            self.break_undo_group();
            return Ok(false);
        }
        let removed = composition.original.clone();
        let inserted: Arc<str> = self.source.text(final_range.clone())?.into();
        let entry = Entry {
            deltas: vec![Delta {
                before: range,
                after: final_range,
                removed,
                inserted,
            }],
            before_selection: composition.before_selection,
            after_selection: self.selection,
            before_checkpoint: composition.before.checkpoint,
            after_checkpoint: self.checkpoint,
            kind: EditKind::Atomic,
            when: Instant::now(),
        };
        self.history.record(
            entry,
            self.limits.history_bytes,
            self.limits.history_entries,
        );
        Ok(true)
    }

    /// Abandon a composition, restoring the source and selection it started
    /// with. The history is untouched.
    pub fn cancel_composition(&mut self) -> bool {
        let Some(composition) = self.composition.take() else {
            return false;
        };
        self.source = composition.before.source.clone();
        self.selection = composition.before_selection;
        self.checkpoint = composition.before.checkpoint;
        self.revision = self.revision.saturating_add(1);
        self.break_undo_group();
        true
    }

    pub fn replace_selection(&mut self, text: impl Into<Arc<str>>, kind: EditKind) -> Result<bool> {
        let text = text.into();
        let range = self.selection.range();
        let caret = range
            .start
            .checked_add(text.len())
            .context("Source offset overflow")?;
        self.apply(
            self.revision,
            vec![Replacement::new(range, text)],
            Selection::caret(caret),
            kind,
        )
    }

    pub fn apply(
        &mut self,
        expected_revision: u64,
        replacements: Vec<Replacement>,
        selection: Selection,
        kind: EditKind,
    ) -> Result<bool> {
        self.apply_at(
            expected_revision,
            replacements,
            selection,
            kind,
            Instant::now(),
        )
    }

    pub fn apply_at(
        &mut self,
        expected_revision: u64,
        replacements: Vec<Replacement>,
        selection: Selection,
        kind: EditKind,
        when: Instant,
    ) -> Result<bool> {
        ensure!(
            expected_revision == self.revision,
            "The Markdown edit is stale"
        );
        ensure!(
            replacements.len() <= 100_000,
            "Too many replacements in one edit"
        );
        let mut length = self.source.len();
        let mut previous: Option<&Range<usize>> = None;
        let mut estimate = std::mem::size_of::<Entry>();
        for replacement in &replacements {
            self.source.validate_range(&replacement.range)?;
            if let Some(old) = previous {
                ensure!(
                    old.end <= replacement.range.start && old.start != replacement.range.start,
                    "Source replacements must be sorted and non-overlapping"
                );
            }
            previous = Some(&replacement.range);
            length = length
                .checked_sub(replacement.range.len())
                .and_then(|length| length.checked_add(replacement.text.len()))
                .context("Source size overflow")?;
            estimate = estimate
                .checked_add(std::mem::size_of::<Delta>())
                .and_then(|bytes| bytes.checked_add(replacement.range.len()))
                .and_then(|bytes| bytes.checked_add(replacement.text.len()))
                .context("Undo size overflow")?;
        }
        ensure!(
            length <= self.limits.source_bytes,
            "Markdown exceeds the session source limit"
        );
        ensure!(
            estimate <= self.limits.history_bytes,
            "This edit exceeds the undo byte budget"
        );
        let mut candidate = self.source.clone();
        let mut deltas = Vec::with_capacity(replacements.len());
        let mut removed_before = 0;
        let mut inserted_before = 0;
        for replacement in &replacements {
            let removed = self.source.text(replacement.range.clone())?;
            let start = replacement.range.start - removed_before + inserted_before;
            if removed.as_str() != replacement.text.as_ref() {
                deltas.push(Delta {
                    before: replacement.range.clone(),
                    after: start..start + replacement.text.len(),
                    removed: removed.into(),
                    inserted: replacement.text.clone(),
                });
            }
            removed_before += replacement.range.len();
            inserted_before += replacement.text.len();
        }
        for replacement in replacements.iter().rev() {
            candidate.rope.try_remove(replacement.range.clone())?;
            candidate
                .rope
                .try_insert(replacement.range.start, &replacement.text)?;
        }
        selection.validate(&candidate)?;
        if deltas.is_empty() {
            self.set_selection(selection)?;
            return Ok(false);
        }
        let revision = self
            .revision
            .checked_add(1)
            .context("Editor revision exhausted")?;
        let next_checkpoint = self
            .next_checkpoint
            .checked_add(1)
            .context("Editor checkpoint exhausted")?;
        let entry = Entry {
            deltas,
            before_selection: self.selection,
            after_selection: selection,
            before_checkpoint: self.checkpoint,
            after_checkpoint: self.next_checkpoint,
            kind,
            when,
        };
        self.history.record(
            entry,
            self.limits.history_bytes,
            self.limits.history_entries,
        );
        self.source = candidate;
        self.selection = selection;
        self.revision = revision;
        self.checkpoint = self.next_checkpoint;
        self.next_checkpoint = next_checkpoint;
        Ok(true)
    }

    pub fn undo(&mut self) -> Result<bool> {
        self.restore(false)
    }
    pub fn redo(&mut self) -> Result<bool> {
        self.restore(true)
    }

    fn restore(&mut self, redo: bool) -> Result<bool> {
        let entry = if redo {
            self.history.redo.last()
        } else {
            self.history.undo.back()
        };
        let Some(entry) = entry else {
            return Ok(false);
        };
        let mut candidate = self.source.clone();
        for delta in entry.deltas.iter().rev() {
            let (range, insert) = if redo {
                (&delta.before, &delta.inserted)
            } else {
                (&delta.after, &delta.removed)
            };
            candidate.rope.try_remove(range.clone())?;
            candidate.rope.try_insert(range.start, insert)?;
        }
        let selection = if redo {
            entry.after_selection
        } else {
            entry.before_selection
        };
        selection.validate(&candidate)?;
        let revision = self
            .revision
            .checked_add(1)
            .context("Editor revision exhausted")?;
        self.checkpoint = if redo {
            entry.after_checkpoint
        } else {
            entry.before_checkpoint
        };
        if redo {
            if let Some(entry) = self.history.redo.pop() {
                self.history.undo.push_back(entry);
            }
        } else if let Some(entry) = self.history.undo.pop_back() {
            self.history.redo.push(entry);
        }
        self.source = candidate;
        self.selection = selection;
        self.revision = revision;
        self.break_undo_group();
        Ok(true)
    }
}
