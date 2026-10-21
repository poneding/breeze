//! One markdown-rs parse per source revision, shared by every Markdown surface.
//!
//! The parser, the reading model, the editor semantics and the display
//! projection are separate modules so that a paragraph of Markdown means the
//! same thing when it is read, edited live and shown as source. Editing state
//! (`buffer`, `edit`, `history`, `positions`, `pipeline`) never depends on the
//! parser, and no module here writes to the document.
mod ast;
mod buffer;
mod document;
mod edit;
mod history;
mod pipeline;
mod positions;
pub mod projection;
mod render;
pub mod semantic;

pub use ast::MAX_PARSE_BYTES;
pub use buffer::SourceBuffer;
pub use document::{Block, Document, Heading, RenderKind};
pub use edit::{EditKind, EditLimits, EditSession, Replacement, Snapshot};
pub use pipeline::{Pipeline, Request, Result_, project};
pub use positions::{Affinity, Position, Selection};
pub use projection::{BlockKind, DisplayLine, DisplayRun, Projection, ProjectionMode, RunStyle};
pub use semantic::{LineKind, Marker, MarkerKind, Semantic, Span};

/// The source-session ceiling. Editing keeps this even when a file is too large
/// to lay out; see [`MAX_PARSE_BYTES`].
pub const MAX_SOURCE_BYTES: usize = 16 * 1024 * 1024;
const MAX_RENDER_BYTES: usize = 64 * 1024 * 1024;
const MAX_BLOCKS: usize = 100_000;
const MAX_DEPTH: usize = 64;
