//! Immutable source snapshots; flattening and parsing belong on a worker.
use super::MAX_SOURCE_BYTES;
use anyhow::{Context, Result, ensure};
use ropey::{LineType, Rope};
use std::ops::Range;

#[derive(Clone, Debug)]
pub struct SourceBuffer {
    pub(super) rope: Rope,
    preferred_newline: &'static str,
}

impl Default for SourceBuffer {
    /// The empty document. Editing sessions for new files start here.
    fn default() -> Self {
        Self {
            rope: Rope::new(),
            preferred_newline: "\n",
        }
    }
}

impl SourceBuffer {
    /// Construct while loading on a worker, preserving every valid input byte.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        ensure!(bytes.len() <= MAX_SOURCE_BYTES, "Markdown exceeds 16 MiB");
        let text = std::str::from_utf8(bytes).context("Markdown is not valid UTF-8")?;
        let crlf = bytes.windows(2).filter(|pair| *pair == b"\r\n").count();
        let lf = bytes.iter().filter(|&&byte| byte == b'\n').count() - crlf;
        let cr = bytes.iter().filter(|&&byte| byte == b'\r').count() - crlf;
        let preferred_newline = if crlf > lf && crlf >= cr {
            "\r\n"
        } else if cr > lf {
            "\r"
        } else {
            "\n"
        };
        Ok(Self {
            rope: Rope::from_str(text),
            preferred_newline,
        })
    }

    pub fn len(&self) -> usize {
        self.rope.len()
    }
    pub fn is_empty(&self) -> bool {
        self.rope.len() == 0
    }

    /// Materialize only the requested range. Whole-source calls belong off-thread.
    pub fn text(&self, range: Range<usize>) -> Result<String> {
        self.validate_range(&range)?;
        Ok(self.rope.try_slice(range)?.to_string())
    }

    pub fn chunks(&self) -> impl Iterator<Item = &str> {
        self.rope.chunks()
    }

    pub fn validate_offset(&self, offset: usize) -> Result<()> {
        ensure!(
            offset <= self.len() && self.rope.is_char_boundary(offset),
            "Invalid UTF-8 source offset: {offset}"
        );
        Ok(())
    }

    pub fn validate_range(&self, range: &Range<usize>) -> Result<()> {
        ensure!(range.start <= range.end, "Reversed source range");
        self.validate_offset(range.start)?;
        self.validate_offset(range.end)
    }

    pub fn line_count(&self) -> usize {
        self.rope.len_lines(LineType::LF_CR)
    }

    pub fn line_range(&self, line: usize) -> Result<Range<usize>> {
        ensure!(line < self.line_count(), "Source line is out of range");
        let start = self.rope.line_to_byte_idx(line, LineType::LF_CR);
        let end = if line + 1 < self.line_count() {
            self.rope.line_to_byte_idx(line + 1, LineType::LF_CR)
        } else {
            self.len()
        };
        Ok(start..end)
    }

    pub fn line_at(&self, offset: usize) -> Result<usize> {
        self.validate_offset(offset)?;
        Ok(self.rope.byte_to_line_idx(offset, LineType::LF_CR))
    }

    /// Prefer this line's terminator, then its predecessor, without normalizing text.
    pub fn newline_at(&self, offset: usize) -> Result<&'static str> {
        let line = self.line_at(offset)?;
        for index in [Some(line), line.checked_sub(1)].into_iter().flatten() {
            let range = self.line_range(index)?;
            if range.is_empty() {
                continue;
            }
            match self.rope.byte(range.end - 1) {
                b'\n' if range.len() >= 2 && self.rope.byte(range.end - 2) == b'\r' => {
                    return Ok("\r\n");
                }
                b'\n' => return Ok("\n"),
                b'\r' => return Ok("\r"),
                _ => {}
            }
        }
        Ok(self.preferred_newline)
    }

    /// Equality can scan the document; use on a worker for unrelated checkpoints.
    pub fn same_content(&self, other: &Self) -> bool {
        self.rope == other.rope
    }
}
