//! Native offsets and grapheme navigation operate directly over rope chunks.
use super::SourceBuffer;
use anyhow::{Result, bail, ensure};
use std::ops::Range;
use unicode_segmentation::{GraphemeCursor, GraphemeIncomplete};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Affinity {
    Before,
    #[default]
    After,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Position {
    pub offset: usize,
    pub affinity: Affinity,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Selection {
    pub anchor: Position,
    pub head: Position,
    pub desired_x: Option<f32>,
}

impl Selection {
    pub fn caret(offset: usize) -> Self {
        Self::between(offset, offset)
    }
    pub fn between(anchor: usize, head: usize) -> Self {
        Self {
            anchor: Position {
                offset: anchor,
                affinity: Affinity::After,
            },
            head: Position {
                offset: head,
                affinity: Affinity::After,
            },
            desired_x: None,
        }
    }
    pub fn range(&self) -> Range<usize> {
        self.anchor.offset.min(self.head.offset)..self.anchor.offset.max(self.head.offset)
    }
    pub fn is_empty(&self) -> bool {
        self.anchor.offset == self.head.offset
    }
    pub fn reversed(&self) -> bool {
        self.head.offset < self.anchor.offset
    }
    pub(super) fn validate(&self, source: &SourceBuffer) -> Result<()> {
        source.validate_offset(self.anchor.offset)?;
        source.validate_offset(self.head.offset)?;
        ensure!(
            self.desired_x.is_none_or(f32::is_finite),
            "Invalid visual cursor position"
        );
        Ok(())
    }
}

impl SourceBuffer {
    pub fn utf16_len(&self) -> usize {
        self.rope.len_utf16()
    }

    pub fn byte_to_utf16(&self, offset: usize) -> Result<usize> {
        self.validate_offset(offset)?;
        Ok(self.rope.byte_to_utf16_idx(offset))
    }

    /// Surrogate-interior positions are clipped in the requested direction.
    pub fn utf16_to_byte(&self, offset: usize, affinity: Affinity) -> Result<usize> {
        ensure!(
            offset <= self.utf16_len(),
            "Native text offset is out of range"
        );
        let byte = self.rope.utf16_to_byte_idx(offset);
        if affinity == Affinity::After && self.rope.byte_to_utf16_idx(byte) < offset {
            let (chunk, start) = self.rope.chunk(byte);
            let character = chunk[byte - start..]
                .chars()
                .next()
                .ok_or_else(|| anyhow::anyhow!("Missing surrogate character"))?;
            return Ok(byte + character.len_utf8());
        }
        Ok(byte)
    }

    pub fn previous_grapheme(&self, offset: usize) -> Result<usize> {
        self.grapheme_boundary(offset, false)
    }
    pub fn next_grapheme(&self, offset: usize) -> Result<usize> {
        self.grapheme_boundary(offset, true)
    }

    fn grapheme_boundary(&self, offset: usize, forward: bool) -> Result<usize> {
        self.validate_offset(offset)?;
        if (forward && offset == self.len()) || (!forward && offset == 0) {
            return Ok(offset);
        }
        let mut cursor = GraphemeCursor::new(offset, self.len(), true);
        let (mut chunk, mut start) = self.rope.chunk(offset);
        loop {
            let result = if forward {
                cursor.next_boundary(chunk, start)
            } else {
                cursor.prev_boundary(chunk, start)
            };
            match result {
                Ok(boundary) => {
                    return Ok(boundary.unwrap_or(if forward { self.len() } else { 0 }));
                }
                Err(GraphemeIncomplete::NextChunk) => {
                    ensure!(
                        start + chunk.len() < self.len(),
                        "Invalid grapheme continuation"
                    );
                    (chunk, start) = self.rope.chunk(start + chunk.len());
                }
                Err(GraphemeIncomplete::PrevChunk) => {
                    ensure!(start > 0, "Invalid grapheme predecessor");
                    (chunk, start) = self.rope.chunk(start - 1);
                }
                Err(GraphemeIncomplete::PreContext(end)) => {
                    ensure!(end > 0, "Invalid grapheme context");
                    let (context, context_start) = self.rope.chunk(end - 1);
                    cursor.provide_context(&context[..end - context_start], context_start);
                }
                Err(GraphemeIncomplete::InvalidOffset) => bail!("Invalid grapheme cursor offset"),
            }
        }
    }
}
