//! Document history stores changed ranges, not one full source per keystroke.
use super::{EditKind, Selection};
use std::{
    collections::VecDeque,
    ops::Range,
    sync::Arc,
    time::{Duration, Instant},
};

#[derive(Clone, Debug)]
pub(super) struct Delta {
    pub before: Range<usize>,
    pub after: Range<usize>,
    pub removed: Arc<str>,
    pub inserted: Arc<str>,
}

#[derive(Clone, Debug)]
pub(super) struct Entry {
    pub deltas: Vec<Delta>,
    pub before_selection: Selection,
    pub after_selection: Selection,
    pub before_checkpoint: u64,
    pub after_checkpoint: u64,
    pub kind: EditKind,
    pub when: Instant,
}

impl Entry {
    pub fn bytes(&self) -> usize {
        std::mem::size_of::<Self>()
            + self
                .deltas
                .iter()
                .map(|d| std::mem::size_of::<Delta>() + d.removed.len() + d.inserted.len())
                .sum::<usize>()
    }

    fn merge(&self, next: &Self) -> Option<Self> {
        if self.kind != EditKind::Typing
            || next.kind != EditKind::Typing
            || self.deltas.len() != 1
            || next.deltas.len() != 1
            || self.after_selection != next.before_selection
            || !next.before_selection.is_empty()
            || next.when.checked_duration_since(self.when)? > Duration::from_millis(500)
        {
            return None;
        }
        let a = &self.deltas[0];
        let b = &next.deltas[0];
        if !a.before.is_empty() || !b.before.is_empty() || a.after.end != b.before.start {
            return None;
        }
        let mut inserted = String::with_capacity(a.inserted.len() + b.inserted.len());
        inserted.push_str(&a.inserted);
        inserted.push_str(&b.inserted);
        Some(Self {
            deltas: vec![Delta {
                before: a.before.clone(),
                after: a.after.start..b.after.end,
                removed: a.removed.clone(),
                inserted: inserted.into(),
            }],
            before_selection: self.before_selection,
            before_checkpoint: self.before_checkpoint,
            ..next.clone()
        })
    }
}

#[derive(Default, Debug)]
pub(super) struct History {
    pub undo: VecDeque<Entry>,
    pub redo: Vec<Entry>,
    pub bytes: usize,
    pub can_merge: bool,
}

impl History {
    pub fn record(&mut self, entry: Entry, byte_limit: usize, entry_limit: usize) {
        self.bytes -= self.redo.iter().map(Entry::bytes).sum::<usize>();
        self.redo.clear();
        let merged = self
            .can_merge
            .then(|| self.undo.back().and_then(|old| old.merge(&entry)))
            .flatten()
            .filter(|merged| merged.bytes() <= byte_limit);
        let entry = if let Some(merged) = merged {
            if let Some(old) = self.undo.pop_back() {
                self.bytes -= old.bytes();
            }
            merged
        } else {
            entry
        };
        self.bytes += entry.bytes();
        self.undo.push_back(entry);
        self.trim(byte_limit, entry_limit);
        self.can_merge = true;
    }

    pub fn trim(&mut self, bytes: usize, entries: usize) {
        while self.bytes > bytes || self.undo.len() + self.redo.len() > entries {
            let removed = self.undo.pop_front().or_else(|| {
                if self.redo.is_empty() {
                    None
                } else {
                    Some(self.redo.remove(0))
                }
            });
            let Some(entry) = removed else { break };
            self.bytes -= entry.bytes();
        }
    }
}
