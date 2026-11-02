//! Native reflow readers share chrome, not PDF navigation or annotation state.
use crate::*;

/// Comfortable measure for Markdown text in logical pixels.
pub(super) const MARKDOWN_MAX_WIDTH: f32 = 760.;
/// Scrollable blank content above the first and below the last Markdown block.
pub(super) const MARKDOWN_VERTICAL_PADDING: f32 = 48.;
/// Kit's overlay scrollbar track and the matching content inset.
pub(super) const MARKDOWN_HORIZONTAL_PADDING: f32 = 16.;
const MARKDOWN_TOP_INSET_MARKER: &str = "<!--breeze:content-top-->";
const MARKDOWN_BOTTOM_INSET_MARKER: &str = "<!--breeze:content-bottom-->";
/// Wide Mermaid diagrams are fitted to the reading measure. Rewriting their
/// strokes with the same scale keeps one-point SVG outlines from collapsing
/// below a device pixel and appearing only on isolated edges.
const MERMAID_READING_WIDTH: f32 = MARKDOWN_MAX_WIDTH - 2. * MARKDOWN_HORIZONTAL_PADDING;
use breeze_reflow::{EpubWorker, epub, markdown};
use gpui_kit::base::{Scrollbar, TextView, TextViewState};
use gpui_kit::component::{
    ElementExt, h_resizable,
    menu::{ContextMenuExt, DropdownMenu, PopupMenuItem},
    resizable_panel,
    tab::{Tab, TabBar},
    text::{
        FrontmatterPlugin, InlineElement, MarkdownExtensions, MarkdownNode, MarkdownParseContext,
        MarkdownPlugin, RangeHighlight, markdown_ast as mdast,
    },
    tooltip::Tooltip,
};
use std::collections::{HashMap, HashSet};

/// The single TextView-backed Markdown reading surface.
#[derive(Clone)]
pub(super) struct MarkdownView {
    pub source: Arc<str>,
    /// Source passed to Kit, including invisible content-inset blocks.
    render_source: Arc<str>,
    render_map: MarkdownRenderMap,
    pub metadata: Arc<markdown::Metadata>,
    pub state: Entity<TextViewState>,
    pub extensions: MarkdownExtensions,
    /// A source offset the state must reveal once Kit's parser commits it.
    pending_reveal: Option<usize>,
}

/// Maps original Markdown offsets to the source handed to Kit after an inset
/// marker is inserted. YAML front matter uses padding inside its renderer so
/// the front matter remains at byte zero without adding a block between it and
/// the body. TOML and other leading metadata keep the marker after the block.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct MarkdownRenderMap {
    inserted_at: usize,
    inserted_bytes: usize,
}

impl MarkdownRenderMap {
    fn source_offset(&self, offset: usize) -> usize {
        if offset >= self.inserted_at {
            offset.saturating_add(self.inserted_bytes)
        } else {
            offset
        }
    }

    fn source_range(&self, range: std::ops::Range<usize>) -> std::ops::Range<usize> {
        self.source_offset(range.start)..self.source_offset(range.end)
    }
}

/// Adds scrollable breathing room without shrinking the reader viewport.
#[derive(Clone)]
struct ContentInsetPlugin;

/// Kit's front matter card with the reader's top breathing room included.
///
/// YAML is recognized only at byte zero, so a normal inset block cannot be
/// placed above it. Wrapping Kit's renderer keeps scrolling behavior correct
/// while leaving the ordinary block gap between the card and the first body
/// block.
struct ReaderFrontmatterPlugin;

struct ReaderFrontmatter {
    content: MarkdownNode,
}

impl MarkdownPlugin for ReaderFrontmatterPlugin {
    fn is_block(&self) -> bool {
        true
    }

    fn name(&self) -> &str {
        "breeze-frontmatter"
    }

    fn parse(&self, node: &mdast::Node, cx: &MarkdownParseContext<'_>) -> Option<MarkdownNode> {
        let content = FrontmatterPlugin::new().parse(node, cx)?;
        let text = content.as_text().to_owned();
        let markdown = content.as_markdown().to_owned();
        let accessibility = content.accessibility_name().to_owned();
        Some(
            MarkdownNode::new(self.name(), ReaderFrontmatter { content })
                .text(text)
                .markdown(markdown)
                .accessibility_label(accessibility),
        )
    }

    fn render(&self, node: &MarkdownNode, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let content = node
            .data::<ReaderFrontmatter>()
            .map(|frontmatter| {
                FrontmatterPlugin::new()
                    .render(&frontmatter.content, window, cx)
                    .into_any_element()
            })
            .unwrap_or_else(|| div().child(node.as_text().to_string()).into_any_element());
        div()
            .id("markdown-frontmatter")
            .test_support()
            .pt(px(MARKDOWN_VERTICAL_PADDING))
            .child(
                div()
                    .id("markdown-frontmatter-content")
                    .test_support()
                    .child(content),
            )
    }
}

impl MarkdownPlugin for ContentInsetPlugin {
    fn is_block(&self) -> bool {
        true
    }

    fn name(&self) -> &str {
        "breeze-content-inset"
    }

    fn parse(&self, node: &mdast::Node, cx: &MarkdownParseContext<'_>) -> Option<MarkdownNode> {
        let mdast::Node::Html(html) = node else {
            return None;
        };
        let position = node.position()?;
        let height = match html.value.trim() {
            MARKDOWN_TOP_INSET_MARKER => MARKDOWN_VERTICAL_PADDING,
            MARKDOWN_BOTTOM_INSET_MARKER if position.end.offset == cx.source().len() => {
                MARKDOWN_VERTICAL_PADDING
            }
            _ => return None,
        };
        Some(MarkdownNode::new(self.name(), height))
    }

    fn render(&self, node: &MarkdownNode, _: &mut Window, _: &mut App) -> impl IntoElement {
        let height = node
            .data::<f32>()
            .copied()
            .unwrap_or(MARKDOWN_VERTICAL_PADDING);
        div().w_full().h(px(height))
    }
}

fn markdown_render_source(
    source: &str,
    frontmatter_end: Option<usize>,
) -> (String, MarkdownRenderMap) {
    let yaml_frontmatter = frontmatter_end.is_some_and(|end| {
        source
            .get(..end)
            .is_some_and(|frontmatter| frontmatter.starts_with("---"))
    });
    let inserted_at = if yaml_frontmatter {
        // The front matter renderer owns its top inset. Inserting a block here
        // would put 48 px between the metadata card and the first body block.
        0
    } else {
        frontmatter_end.unwrap_or(0).min(source.len())
    };
    let insertion = if yaml_frontmatter {
        String::new()
    } else if inserted_at == 0 {
        format!("{MARKDOWN_TOP_INSET_MARKER}\n\n")
    } else {
        format!("\n{MARKDOWN_TOP_INSET_MARKER}\n\n")
    };
    let mut rendered = String::with_capacity(
        source.len() + insertion.len() + MARKDOWN_BOTTOM_INSET_MARKER.len() + 2,
    );
    rendered.push_str(&source[..inserted_at]);
    rendered.push_str(&insertion);
    rendered.push_str(&source[inserted_at..]);
    rendered.push_str("\n\n");
    rendered.push_str(MARKDOWN_BOTTOM_INSET_MARKER);
    (
        rendered,
        MarkdownRenderMap {
            inserted_at,
            inserted_bytes: insertion.len(),
        },
    )
}

/// Whether the rendered source starts with a dedicated top-inset block.
fn markdown_has_top_inset(source: &str, frontmatter_end: Option<usize>) -> bool {
    !frontmatter_end.is_some_and(|end| {
        source
            .get(..end)
            .is_some_and(|frontmatter| frontmatter.starts_with("---"))
    })
}

fn rendered_source_block(item: usize, block_count: usize, has_top_inset: bool) -> usize {
    let item = if has_top_inset {
        item.saturating_sub(1)
    } else {
        item
    };
    item.min(block_count.saturating_sub(1))
}

#[derive(Clone, Debug)]
pub(super) struct ReflowSearchHit {
    pub rendered: std::ops::Range<usize>,
    pub href: String,
    pub text: String,
}

/// One row of the navigation panel. EPUB chapters and Markdown headings share
/// this shape so both render exactly like the PDF outline: the same row height,
/// indentation, active highlight and nesting controls.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct OutlineEntry {
    pub title: String,
    pub depth: usize,
    pub href: String,
    pub block: usize,
    pub source_offset: usize,
}

/// The navigation-outline title search. Mirrors the PDF reader's
/// `reading::OutlineSearch`.
struct OutlineSearch {
    input: Entity<InputState>,
    collapsed: HashSet<usize>,
    saved_offset: Point<gpui_kit::Pixels>,
    _subscription: Subscription,
}

/// A caret selection over the selectable text layers in the current EPUB frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ReflowTextSelection {
    anchor: (usize, usize),
    focus: (usize, usize),
}

impl ReflowTextSelection {
    fn ordered(self) -> ((usize, usize), (usize, usize)) {
        if self.anchor <= self.focus {
            (self.anchor, self.focus)
        } else {
            (self.focus, self.anchor)
        }
    }

    fn collapsed(self) -> bool {
        self.anchor == self.focus
    }

    fn on_page(self, page: usize, glyphs: usize) -> std::ops::Range<usize> {
        let (start, end) = self.ordered();
        if page < start.0 || page > end.0 {
            return 0..0;
        }
        let low = if page == start.0 { start.1 } else { 0 }.min(glyphs);
        let high = if page == end.0 { end.1 } else { glyphs }.min(glyphs);
        low..high.max(low)
    }
}

/// A rasterized formula or diagram: the image plus its logical size.
type RenderedImage = Option<(Arc<RenderImage>, f32, f32)>;
type MathCache = HashMap<(String, u8, [u8; 3], u32), RenderedImage>;
type MermaidCache = HashMap<(String, bool, u32, u32), RenderedImage>;

pub(super) struct ReflowView {
    pub source: Option<Source>,
    pub markdown: Option<MarkdownView>,
    pub frame: Option<epub::Frame>,
    pub(super) page_images: Vec<Arc<RenderImage>>,
    worker: Option<EpubWorker>,
    task: Option<Task<()>>,
    request: Option<epub::Request>,
    viewport: (f32, f32, f32),
    /// Render generation, so a stale worker result can be discarded.
    pub epoch: u64,
    queued: bool,
    pub hits: Vec<ReflowSearchHit>,
    pub images: reflow_images::Images,
    /// Typeset display formulas, keyed by TeX, font size and text colour.
    math_cache: MathCache,
    /// Rasterize small inline formulas at a fixed preview DPR, keyed by source.
    inline_math_cache: HashMap<(String, u8, [u8; 3]), Option<Arc<RenderImage>>>,
    /// Rendered Mermaid diagrams, keyed by their source and the app theme.
    mermaid_cache: MermaidCache,
    /// Selection over the current EPUB frame, plus the rectangles to paint.
    text_selection: Option<ReflowTextSelection>,
    selection: Vec<(usize, Vec<[f32; 4]>)>,
    selection_active: bool,
    /// Where each page image was laid out last frame, and its raster size.
    page_bounds: HashMap<usize, Bounds<gpui_kit::Pixels>>,
    page_sizes: Vec<(f32, f32)>,
    /// Outline rows for the open document, built once when it loads.
    pub outline: Arc<Vec<OutlineEntry>>,
    /// Folded outline rows, addressed by their index in `outline`.
    pub outline_folds: HashSet<usize>,
    pub outline_scroll: UniformListScrollHandle,
    outline_search: Option<OutlineSearch>,
    /// Top-level TextView block at the top of the Markdown view.
    pub active_block: usize,
    /// Last TextView scroll offset reflected in Markdown progress.
    markdown_scroll_offset: f32,
    /// Wheel distance not yet worth a page turn, in logical pixels.
    scroll_accum: f32,
    /// Page steps the reader has asked for that have not been issued yet.
    pending_turn: i16,
}

impl Default for ReflowView {
    fn default() -> Self {
        Self {
            source: None,
            markdown: None,
            frame: None,
            page_images: Vec::new(),
            worker: None,
            task: None,
            request: None,
            viewport: (600., 800., 1.),
            epoch: 0,
            queued: false,
            hits: Vec::new(),
            images: Default::default(),
            math_cache: HashMap::new(),
            inline_math_cache: HashMap::new(),
            mermaid_cache: HashMap::new(),
            text_selection: None,
            selection: Vec::new(),
            selection_active: false,
            page_bounds: HashMap::new(),
            page_sizes: Vec::new(),
            outline: Arc::new(Vec::new()),
            outline_folds: HashSet::new(),
            outline_scroll: UniformListScrollHandle::new(),
            outline_search: None,
            active_block: 0,
            markdown_scroll_offset: 0.,
            scroll_accum: 0.,
            pending_turn: 0,
        }
    }
}

impl ReflowView {
    fn outline_folds(&self) -> &HashSet<usize> {
        match &self.outline_search {
            Some(search) => &search.collapsed,
            None => &self.outline_folds,
        }
    }

    pub fn clear(&mut self, cx: &mut App) {
        self.clear_inner(cx, true);
    }

    /// Replace the Markdown view while retaining decoded image resources for
    /// the same document. An edited draft must be reparsed, but its remote
    /// badges and local pictures do not need to be fetched and decoded again.
    fn clear_preserving_images(&mut self, cx: &mut App) {
        self.clear_inner(cx, false);
    }

    fn clear_inner(&mut self, cx: &mut App, clear_images: bool) {
        self.epoch += 1;
        self.task = None;
        self.request = None;
        self.queued = false;
        self.source = None;
        self.markdown = None;
        self.frame = None;
        self.hits.clear();
        self.text_selection = None;
        self.selection.clear();
        self.selection_active = false;
        self.page_bounds.clear();
        self.page_sizes.clear();
        self.outline = Arc::new(Vec::new());
        self.outline_folds.clear();
        self.outline_scroll = UniformListScrollHandle::new();
        self.outline_search = None;
        self.active_block = 0;
        self.markdown_scroll_offset = 0.;
        self.scroll_accum = 0.;
        self.pending_turn = 0;
        self.worker = None;
        if clear_images {
            self.images.clear(cx);
        } else {
            self.images.forget_failures();
        }
        self.math_cache.clear();
        self.inline_math_cache.clear();
        self.mermaid_cache.clear();
        let page_images = std::mem::take(&mut self.page_images);
        if !page_images.is_empty() {
            cx.defer(move |cx| {
                for image in page_images {
                    cx.drop_image(image, None);
                }
            });
        }
    }
}

#[derive(Clone)]
struct TocPlugin {
    headings: Arc<Vec<markdown::Heading>>,
    view: WeakEntity<Breeze>,
}

impl MarkdownPlugin for TocPlugin {
    fn is_block(&self) -> bool {
        true
    }

    fn name(&self) -> &str {
        "breeze-toc"
    }

    fn parse(&self, node: &mdast::Node, cx: &MarkdownParseContext<'_>) -> Option<MarkdownNode> {
        let source = cx.node_source(node)?.trim();
        toc_placeholder(source).then(|| {
            MarkdownNode::new(self.name(), ())
                .text(source)
                .markdown(source)
        })
    }

    fn render(&self, _: &MarkdownNode, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = Theme::global(cx).clone();
        let mut list = div().w_full().flex().flex_col().gap_1();
        for (index, heading) in self.headings.iter().enumerate() {
            let view = self.view.clone();
            let offset = heading.source.start;
            list = list.child(
                div()
                    .id(("markdown-toc-entry", index))
                    .test_support()
                    .ml(px(12. * heading.level.saturating_sub(1) as f32))
                    .text_color(theme.link)
                    .cursor_pointer()
                    .child(heading.title.clone())
                    .on_click(move |_, _, cx| {
                        let _ = view.update(cx, |this, cx| this.jump_markdown_source(offset, cx));
                    }),
            );
        }
        list.into_any_element()
    }
}

#[derive(Clone)]
struct MathBlockPlugin {
    view: WeakEntity<Breeze>,
}

impl MarkdownPlugin for MathBlockPlugin {
    fn is_block(&self) -> bool {
        true
    }

    fn name(&self) -> &str {
        "breeze-math-block"
    }

    fn parse(&self, node: &mdast::Node, _: &MarkdownParseContext<'_>) -> Option<MarkdownNode> {
        let tex = match node {
            mdast::Node::Math(math) => math.value.clone(),
            mdast::Node::Paragraph(paragraph) => {
                let [mdast::Node::InlineMath(math)] = paragraph.children.as_slice() else {
                    return None;
                };
                math.value.clone()
            }
            mdast::Node::Code(code)
                if matches!(
                    code.lang.as_deref(),
                    Some("math" | "katex" | "latex" | "tex")
                ) =>
            {
                code.value.clone()
            }
            _ => return None,
        };
        Some(MarkdownNode::new(self.name(), tex.clone()).text(tex))
    }

    fn render(&self, node: &MarkdownNode, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let tex = node.data::<String>().cloned().unwrap_or_default();
        let view = self.view.clone();
        view.update(cx, |this, cx| this.markdown_math_block(&tex, window, cx))
            .ok()
            .unwrap_or_else(|| div().child(tex).into_any_element())
    }
}

#[derive(Clone)]
struct MermaidBlockPlugin {
    view: WeakEntity<Breeze>,
}

impl MarkdownPlugin for MermaidBlockPlugin {
    fn is_block(&self) -> bool {
        true
    }

    fn name(&self) -> &str {
        "breeze-mermaid"
    }

    fn parse(&self, node: &mdast::Node, _: &MarkdownParseContext<'_>) -> Option<MarkdownNode> {
        let mdast::Node::Code(code) = node else {
            return None;
        };
        let language = code.lang.as_deref()?.to_ascii_lowercase();
        if !matches!(language.as_str(), "mermaid" | "flow" | "seq") {
            return None;
        }
        let source = mermaid_source(&language, &code.value);
        Some(MarkdownNode::new(self.name(), source.clone()).text(code.value.clone()))
    }

    fn render(&self, node: &MarkdownNode, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let source = node.data::<String>().cloned().unwrap_or_default();
        let view = self.view.clone();
        view.update(cx, |this, cx| {
            this.markdown_mermaid_block(&source, window, cx)
        })
        .ok()
        .unwrap_or_else(|| {
            div()
                .text_color(Theme::global(cx).danger)
                .child(tr!("Unsupported or invalid diagram syntax"))
                .into_any_element()
        })
    }
}

#[derive(Clone, Debug)]
struct MarkdownImage {
    url: String,
    alt: String,
}

#[derive(Clone)]
struct ImagePlugin {
    view: WeakEntity<Breeze>,
    definitions: Arc<HashMap<String, (String, Option<String>)>>,
}

impl MarkdownPlugin for ImagePlugin {
    fn name(&self) -> &str {
        "breeze-image"
    }

    fn parse(&self, node: &mdast::Node, _: &MarkdownParseContext<'_>) -> Option<MarkdownNode> {
        let image = match node {
            mdast::Node::Image(image) => MarkdownImage {
                url: image.url.clone(),
                alt: image.alt.clone(),
            },
            mdast::Node::ImageReference(image) => {
                let (url, _) = self.definitions.get(&image.identifier)?;
                MarkdownImage {
                    url: url.clone(),
                    alt: image.alt.clone(),
                }
            }
            _ => return None,
        };
        Some(MarkdownNode::new(self.name(), image.clone()).text(image.alt))
    }

    fn render_inline(
        &self,
        node: &MarkdownNode,
        _: &gpui_kit::component::text::InlineRenderContext,
        _: &mut Window,
        cx: &mut App,
    ) -> Option<InlineElement> {
        let image = node.data::<MarkdownImage>()?.clone();
        self.view
            .update(cx, |this, cx| this.markdown_rendered_image(&image, cx))
            .ok()
            .flatten()
            .map(InlineElement::new)
    }
}

#[derive(Clone)]
struct InlineMathPlugin {
    view: WeakEntity<Breeze>,
}

impl MarkdownPlugin for InlineMathPlugin {
    fn name(&self) -> &str {
        "breeze-inline-math"
    }

    fn parse(&self, node: &mdast::Node, _: &MarkdownParseContext<'_>) -> Option<MarkdownNode> {
        let mdast::Node::InlineMath(math) = node else {
            return None;
        };
        Some(MarkdownNode::new(self.name(), math.value.clone()).text(format!("${}$", math.value)))
    }

    fn render_inline(
        &self,
        node: &MarkdownNode,
        context: &gpui_kit::component::text::InlineRenderContext,
        _: &mut Window,
        cx: &mut App,
    ) -> Option<InlineElement> {
        let tex = node.data::<String>()?.clone();
        let color = context.text_style().color;
        let size = context.font_size();
        self.view
            .update(cx, |this, _cx| this.markdown_inline_math(&tex, size, color))
            .ok()
            .flatten()
            .map(|element| InlineElement::new(element).with_baseline(size * 0.8))
    }
}

fn toc_placeholder(source: &str) -> bool {
    matches!(
        source.to_ascii_lowercase().as_str(),
        "[toc]" | "[[toc]]" | "[[_toc_]]" | "{{toc}}" | "<toc>"
    )
}

impl Breeze {
    fn markdown_extensions(
        &self,
        cx: &mut Context<Self>,
        metadata: &markdown::Metadata,
    ) -> MarkdownExtensions {
        let view = cx.entity().downgrade();
        MarkdownExtensions::default()
            .frontmatter()
            .plugin(ReaderFrontmatterPlugin)
            .plugin(ContentInsetPlugin)
            .plugin(ImagePlugin {
                view: view.clone(),
                definitions: Arc::new(metadata.image_definitions.clone()),
            })
            .plugin(TocPlugin {
                headings: Arc::new(metadata.headings.clone()),
                view: view.clone(),
            })
            .plugin(MathBlockPlugin { view: view.clone() })
            .plugin(InlineMathPlugin { view: view.clone() })
            .plugin(MermaidBlockPlugin { view })
            .parser_revision(
                metadata
                    .fingerprint
                    .as_bytes()
                    .iter()
                    .fold(0u64, |hash, byte| {
                        hash.wrapping_mul(1099511628211)
                            .wrapping_add(u64::from(*byte))
                    }),
            )
    }

    fn restore_pending_markdown(&mut self, cx: &mut Context<Self>) {
        let Some(markdown) = self.reflow.markdown.as_mut() else {
            return;
        };
        let Some(offset) = markdown.pending_reveal else {
            return;
        };
        let rendered = markdown.state.read(cx).rendered_text();
        if rendered.source() != markdown.render_source.as_ref() {
            return;
        }
        let block = markdown.metadata.block_at_offset(offset);
        let source_range = markdown
            .metadata
            .headings
            .iter()
            .find(|heading| heading.source.contains(&offset))
            .map(|heading| heading.source.clone())
            .or_else(|| markdown.metadata.block_ranges.get(block).cloned());
        let Some(range) = source_range
            .map(|range| markdown.render_map.source_range(range))
            .and_then(|range| rendered.range_for_source(range))
        else {
            return;
        };
        let _ = markdown
            .state
            .update(cx, |state, cx| state.reveal_range(range, cx));
        markdown.pending_reveal = None;
        self.reflow.active_block = block;
    }

    fn jump_markdown_source(&mut self, offset: usize, cx: &mut Context<Self>) {
        let Some(markdown) = self.reflow.markdown.as_mut() else {
            return;
        };
        let block = markdown.metadata.block_at_offset(offset);
        let source_bytes = markdown.metadata.source_bytes.max(1);
        markdown.pending_reveal = Some(offset);
        self.reflow.active_block = block;
        let position = &mut self.books[self.active].reflow;
        position.markdown_offset = offset.min(source_bytes.saturating_sub(1));
        position.progress = offset as f32 / source_bytes as f32;
        self.restore_pending_markdown(cx);
        cx.notify();
    }

    fn jump_markdown_range(&mut self, range: std::ops::Range<usize>, cx: &mut Context<Self>) {
        let Some(markdown) = self.reflow.markdown.as_ref() else {
            return;
        };
        let state = markdown.state.clone();
        state.update(cx, |state, cx| {
            let _ = state.reveal_range(range, cx);
        });
        cx.notify();
    }

    fn apply_markdown_search_highlights(&mut self, cx: &mut Context<Self>) {
        let Some(markdown) = self.reflow.markdown.as_ref() else {
            return;
        };
        let color = Theme::global(cx).warning.opacity(0.28);
        let highlights = self
            .reflow
            .hits
            .iter()
            .filter(|hit| !hit.rendered.is_empty())
            .map(|hit| RangeHighlight::new(hit.rendered.clone(), color))
            .collect::<Vec<_>>();
        markdown.state.update(cx, |state, cx| {
            let _ = state.set_range_highlights(highlights, cx);
        });
    }

    fn markdown_rendered_image(
        &mut self,
        image: &MarkdownImage,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let source = self.books[self.active].source.clone();
        self.request_image(cx, &source, &image.url);
        let (render_image, logical_size) =
            self.reflow.images.cached_with_logical_size(&image.url)?;
        let size = render_image.size(0);
        let (width, height) = logical_size.unwrap_or_else(|| {
            (
                usize::from(size.width) as f32,
                usize::from(size.height) as f32,
            )
        });
        use std::hash::{Hash as _, Hasher as _};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        image.url.hash(&mut hasher);
        Some(
            img(render_image)
                .id(("markdown-image", hasher.finish()))
                .object_fit(ObjectFit::Contain)
                .max_w(relative(1.))
                .w(px(width))
                .h(px(height))
                .into_any_element(),
        )
    }

    fn markdown_math_block(
        &mut self,
        tex: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = Theme::global(cx).clone();
        let body = f32::from(self.books[self.active].reflow.font_size);
        let color = theme.foreground.to_rgb();
        let rgb = [
            (color.r * 255.0).round() as u8,
            (color.g * 255.0).round() as u8,
            (color.b * 255.0).round() as u8,
        ];
        let scale = window.scale_factor().max(1.0);
        let key = (
            tex.to_owned(),
            self.books[self.active].reflow.font_size,
            rgb,
            scale.to_bits(),
        );
        let rendered = self
            .reflow
            .math_cache
            .entry(key)
            .or_insert_with(|| {
                ratex_gpui::render::render_latex(tex, body, scale, theme.foreground)
                    .map(|formula| (formula.image, formula.width, formula.height))
            })
            .clone();
        let formula = match rendered {
            Some((image, width, height)) => {
                img(image).w(px(width)).h(px(height)).into_any_element()
            }
            None => div()
                .text_size(px(body + 2.))
                .font_family(theme.font_family)
                .child(tex.to_owned())
                .into_any_element(),
        };
        div()
            .id("markdown-math")
            .test_support()
            .w_full()
            .flex()
            .justify_center()
            .py_2()
            .child(formula)
            .into_any_element()
    }

    fn markdown_inline_math(
        &mut self,
        tex: &str,
        size: gpui_kit::Pixels,
        color: gpui_kit::Hsla,
    ) -> Option<AnyElement> {
        let rgb = color.to_rgb();
        let rgb = [
            (rgb.r * 255.0).round() as u8,
            (rgb.g * 255.0).round() as u8,
            (rgb.b * 255.0).round() as u8,
        ];
        let key = (tex.to_owned(), f32::from(size).round() as u8, rgb);
        let image = self
            .reflow
            .inline_math_cache
            .entry(key)
            .or_insert_with(|| {
                ratex_gpui::render::render_latex(tex, f32::from(size), 2.0, color)
                    .map(|formula| formula.image)
            })
            .clone()?;
        let image_size = image.size(0);
        Some(
            img(image)
                .w(px(usize::from(image_size.width) as f32 / 2.))
                .h(px(usize::from(image_size.height) as f32 / 2.))
                .into_any_element(),
        )
    }

    fn markdown_mermaid_block(
        &mut self,
        source: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = Theme::global(cx).clone();
        let scale = window.scale_factor().max(1.0);
        let (available_width, _) = self.reading_viewport_size(window);
        let content_width = available_width - 2. * MARKDOWN_HORIZONTAL_PADDING;
        let display_width = if self.books[self.active].reflow.full_width {
            content_width.max(320.)
        } else {
            content_width.clamp(320., MERMAID_READING_WIDTH)
        };
        let width_bucket = ((display_width / 32.).floor() as u32).max(10);
        let key = (
            source.to_owned(),
            theme.is_dark(),
            scale.to_bits(),
            width_bucket,
        );
        let rendered = self
            .reflow
            .mermaid_cache
            .entry(key)
            .or_insert_with(|| {
                let svg = mermaid_rs_renderer::render(source).ok()?;
                let svg = strip_mermaid_background(&svg);
                let svg = fit_svg_to_width(&svg, width_bucket as f32 * 32.);
                let image = crate::reflow_images::svg_image(svg.as_bytes(), scale).ok()?;
                let size = image.size(0);
                Some((
                    image,
                    usize::from(size.width) as f32 / scale,
                    usize::from(size.height) as f32 / scale,
                ))
            })
            .clone();
        match rendered {
            Some((image, width, height)) => div()
                .id("markdown-mermaid")
                .test_support()
                .w_full()
                .flex()
                .justify_center()
                .py_2()
                .child(
                    img(image)
                        .object_fit(ObjectFit::Contain)
                        .id("markdown-mermaid-image")
                        .test_support()
                        .max_w(relative(1.))
                        .w(px(width))
                        .h(px(height)),
                )
                .into_any_element(),
            None => div()
                .id("markdown-mermaid")
                .test_support()
                .text_color(theme.danger)
                .child(tr!("Unsupported or invalid diagram syntax"))
                .into_any_element(),
        }
    }
}

impl Breeze {
    pub(super) fn run_reflow_search(&mut self, cx: &mut Context<Self>) {
        use std::sync::atomic::AtomicBool;
        let query = self.search_input.read(cx).value().to_string();
        if query.trim().is_empty() || query.len() > 256 {
            self.search_notice = tr!("Enter a search term of 1–256 bytes").into();
            cx.notify();
            return;
        }
        self.cancel_search();
        let generation = self.search_generation;
        let cancel = Arc::new(AtomicBool::new(false));
        self.search_cancel = Some(cancel.clone());
        self.search_busy = true;
        self.search_notice = tr!("Searching…").into();

        let markdown_text = self.reflow.markdown.as_ref().map(|markdown| {
            let rendered = markdown.state.read(cx).rendered_text().as_str().to_owned();
            if rendered.is_empty() {
                markdown.source.to_string()
            } else {
                rendered
            }
        });
        let Source::File(path) = self.books[self.active].source.clone() else {
            return;
        };
        let fingerprint = self
            .reflow
            .frame
            .as_ref()
            .map(|frame| frame.metadata.fingerprint.clone());
        let search_query = query.clone();
        let work = cx.background_spawn(async move {
            if let Some(text) = markdown_text {
                Ok::<_, anyhow::Error>(search_markdown_text(&text, &search_query))
            } else {
                let (revision, hits) = epub::search(&path, &search_query, &cancel)?;
                anyhow::ensure!(
                    fingerprint.as_ref() == Some(&revision),
                    "The EPUB changed; reopen it before searching"
                );
                Ok(hits
                    .into_iter()
                    .map(|hit| ReflowSearchHit {
                        rendered: 0..0,
                        text: reflow_search_result_text(&hit.title, &hit.href, &hit.snippet),
                        href: hit.href,
                    })
                    .collect())
            }
        });
        let view = cx.entity().downgrade();
        cx.spawn(async move |_, cx| {
            let result = work.await;
            let _ = view.update(cx, |this, cx| {
                if this.search_generation != generation {
                    return;
                }
                this.search_busy = false;
                this.search_cancel = None;
                match result {
                    Ok(hits) => {
                        this.search_notice = tf!("{} matching sections", hits.len());
                        this.reflow.hits = hits;
                        this.apply_markdown_search_highlights(cx);
                    }
                    Err(error) => this.search_notice = format!("{error:#}"),
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    pub(super) fn reflow_search_results(&self, cx: &mut Context<Self>) -> Div {
        column()
            .flex_1()
            .min_h_0()
            .p_2()
            .gap_2()
            .child(div().text_size(px(12.)).child(self.search_notice.clone()))
            .child(
                column()
                    .id("search-results")
                    .test_support()
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .children(self.reflow.hits.iter().enumerate().map(|(index, hit)| {
                        let rendered = hit.rendered.clone();
                        let href = hit.href.clone();
                        let text = hit.text.clone();
                        Button::new(("search-result", index))
                            .ghost()
                            .small()
                            .w_full()
                            .justify_start()
                            .accessibility_label(text.clone())
                            .tooltip(text.clone())
                            .child(div().w_full().text_left().text_ellipsis().child(text))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                if this.books[this.active].format() == Some(DocumentFormat::Epub) {
                                    this.jump_reflow(href.clone(), 0, cx);
                                } else {
                                    this.jump_markdown_range(rendered.clone(), cx);
                                }
                            }))
                    })),
            )
    }

    pub(super) fn reflow_sidebar(&self, cx: &mut Context<Self>) -> AnyElement {
        let p = self.p(cx);
        let book = &self.books[self.active];
        let field = |name: &str, value: String| {
            column()
                .gap_1()
                .child(
                    div()
                        .text_size(px(11.))
                        .text_color(p.muted)
                        .child(name.to_owned()),
                )
                .child(div().text_size(px(12.)).child(value))
        };
        let info = column()
            .id("document-information")
            .test_support()
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .p_3()
            .gap_3()
            .child(field(tr!("Name"), book.title.clone()))
            .child(field(tr!("Format"), book.format_label().into()))
            .when_some(self.reflow.frame.as_ref(), |info, frame| {
                info.child(field(tr!("Title"), frame.metadata.title.clone()))
                    .child(field(tr!("Author"), frame.metadata.author.clone()))
                    .child(field(
                        tr!("Chapters"),
                        frame.metadata.chapters.len().to_string(),
                    ))
            })
            .when_some(self.reflow.markdown.as_ref(), |info, markdown| {
                info.child(field(
                    tr!("Text"),
                    tf!("{} characters", markdown.metadata.characters),
                ))
            })
            .child(field(
                tr!("Reading progress"),
                format!("{:.0}%", book.reflow.progress * 100.),
            ))
            .child(document_panel::path_field(
                "document-file-location",
                "reveal-document",
                tr!("File location"),
                match &book.source {
                    Source::File(path) => path.display().to_string(),
                    _ => String::new(),
                },
                match &book.source {
                    Source::File(path) => Some(path.clone()),
                    Source::Sample => None,
                },
                p.muted,
            ))
            // A Markdown image that could not be shown is a property of that one
            // image. It is reported here rather than as a status notice, which
            // describes the document as a whole.
            .when(self.reflow.images.failures > 0, |info| {
                info.child(field(
                    tr!("Images not shown"),
                    self.reflow.images.failures.to_string(),
                ))
            });
        column()
            .id("document-panel")
            .test_support()
            .w(px(256.))
            .h_full()
            .min_h_0()
            .flex_shrink_0()
            .bg(p.sidebar)
            .border_l_1()
            .border_color(p.line)
            .child(
                row()
                    .id("document-panel-header")
                    .test_support()
                    .h(px(style::TOOLBAR_HEIGHT))
                    .flex_shrink_0()
                    .px_2()
                    .border_b_1()
                    .border_color(p.line)
                    .when(self.search_open, |bar| bar.child(self.search_bar(cx)))
                    .when(!self.search_open, |bar| {
                        bar.child(
                            Button::new("document-panel-information")
                                .ghost()
                                .small()
                                .icon(IconName::Info)
                                .selected(true)
                                .accessibility_label(tr!("Document information"))
                                .tooltip(tr!("Document information"))
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.cancel_search();
                                    this.search_open = false;
                                    this.search_restore_document_panel_open = None;
                                    this.document_panel.open = true;
                                    cx.notify();
                                })),
                        )
                        .child(div().flex_1())
                    })
                    .child(self.command(
                        "document-panel-toggle",
                        IconName::PanelRight,
                        tr!("Document sidebar"),
                        &ToggleDocumentPanel,
                        cx,
                    )),
            )
            .child(if self.search_open {
                self.reflow_search_results(cx).into_any_element()
            } else {
                info.into_any_element()
            })
            .into_any_element()
    }
    pub(super) fn load_reflow(&mut self, cx: &mut Context<Self>) {
        self.load_reflow_inner(cx, false);
    }

    fn load_reflow_preserving_images(&mut self, cx: &mut Context<Self>) {
        self.load_reflow_inner(cx, true);
    }

    fn load_reflow_inner(&mut self, cx: &mut Context<Self>, preserve_images: bool) {
        self.pending_window_policy = false;
        self.preview = None;
        self.reading.release_images(cx);
        self.reading = reading::ReadingView::default();
        self.raster.clear(cx);
        let source = self.books[self.active].source.clone();
        if self.reflow.source.as_ref() == Some(&source) {
            self.loading = false;
            cx.notify();
            return;
        }
        if preserve_images {
            self.reflow.clear_preserving_images(cx);
        } else {
            self.reflow.clear(cx);
        }
        self.reflow.source = Some(source.clone());
        self.loading = true;
        let Source::File(path) = source else { return };
        match self.books[self.active].format() {
            Some(DocumentFormat::Markdown) => {
                // A draft is the reader's source whenever an editing session
                // exists: switching back previews what was typed.
                let draft = self.books[self.active]
                    .markdown_edit
                    .as_ref()
                    .map(|editor| editor.read(cx).source_text(cx));
                if draft
                    .as_ref()
                    .is_some_and(|source| source.len() > markdown::MAX_PARSE_BYTES)
                {
                    self.loading = false;
                    self.message = tf!(
                        "Markdown over {} MiB is available only as source",
                        markdown::MAX_PARSE_BYTES / (1024 * 1024)
                    );
                    cx.notify();
                    return;
                }
                if draft.is_none()
                    && std::fs::metadata(&path)
                        .is_ok_and(|metadata| metadata.len() > markdown::MAX_PARSE_BYTES as u64)
                {
                    self.loading = false;
                    self.message = tf!(
                        "Markdown over {} MiB is available only as source",
                        markdown::MAX_PARSE_BYTES / (1024 * 1024)
                    );
                    cx.notify();
                    return;
                }
                let generation = self.generation;
                let work = cx.background_spawn(async move {
                    match draft {
                        Some(source) => {
                            markdown::Metadata::parse(&source).map(|metadata| (source, metadata))
                        }
                        None => markdown::Metadata::open(&path)
                            .and_then(|metadata| Ok((std::fs::read_to_string(&path)?, metadata))),
                    }
                });
                self.reflow.task = Some(cx.spawn(async move |this, cx| {
                    let result = work.await;
                    let _ = this.update(cx, |this, cx| {
                        if this.generation != generation {
                            return;
                        }
                        this.loading = false;
                        match result {
                            Ok((source, metadata)) => {
                                let metadata = Arc::new(metadata);
                                let extensions = this.markdown_extensions(cx, &metadata);
                                let (render_source, render_map) =
                                    markdown_render_source(&source, metadata.frontmatter_end);
                                let state = cx.new(|cx| {
                                    TextViewState::markdown(&render_source, cx)
                                        .selectable(true)
                                        .scrollable(true)
                                });
                                let position = this.books[this.active].reflow.markdown_offset;
                                let restored = metadata.block_at_offset(position);
                                this.reflow.active_block = restored;
                                this.reflow.outline = Arc::new(
                                    metadata
                                        .headings
                                        .iter()
                                        .map(|heading| OutlineEntry {
                                            title: heading.title.clone(),
                                            depth: heading.level.saturating_sub(1),
                                            href: heading.anchor.clone(),
                                            block: heading.block,
                                            source_offset: heading.source.start,
                                        })
                                        .collect(),
                                );

                                let observer = cx.observe(&state, move |this, state, cx| {
                                    if this.reflow.markdown.as_ref().is_some_and(|markdown| {
                                        markdown.state.entity_id() == state.entity_id()
                                    }) {
                                        this.remember_markdown_position(cx);
                                        this.restore_pending_markdown(cx);
                                    }
                                });
                                this._subscriptions.push(observer);
                                this.reflow.markdown = Some(MarkdownView {
                                    source: Arc::from(source),
                                    render_source: Arc::from(render_source),
                                    render_map,
                                    metadata,
                                    state,
                                    extensions,
                                    pending_reveal: Some(position),
                                });
                                this.restore_pending_markdown(cx);
                            }
                            Err(error) => this.message = format!("{error:#}"),
                        }
                        cx.notify();
                    });
                }));
            }
            Some(DocumentFormat::Epub) => match EpubWorker::new() {
                Ok(worker) => {
                    self.reflow.worker = Some(worker);
                    self.request_epub(0, 0., None, cx);
                }
                Err(error) => {
                    self.loading = false;
                    self.message = format!("{error:#}");
                }
            },
            _ => {
                self.loading = false;
                self.message = tr!("Supported formats: PDF, EPUB and Markdown").into();
            }
        }
        cx.notify();
    }

    fn request_epub(
        &mut self,
        turn: i8,
        scroll: f32,
        find: Option<String>,
        cx: &mut Context<Self>,
    ) {
        let Source::File(path) = &self.books[self.active].source else {
            return;
        };
        if self.books[self.active].format() != Some(DocumentFormat::Epub) {
            return;
        }
        let (width, height, scale) = self.reflow.viewport;
        // Queue the step and, while a request is on its way, let it land first:
        // its result carries the position the next step starts from.
        self.reflow.pending_turn = self.reflow.pending_turn.saturating_add(i16::from(turn));
        if turn != 0 && self.reflow.task.is_some() {
            return;
        }
        let step = self.reflow.pending_turn.signum() as i8;
        self.reflow.pending_turn -= i16::from(step);
        let request = epub::Request {
            path: path.clone(),
            location: self.books[self.active].reflow.epub.clone(),
            width,
            height,
            scale,
            font_size: self.books[self.active].reflow.font_size as f32,
            turn: step,
            scroll,
            find,
            // Two-page reading is the document's own setting.
            spread: self.books[self.active].reflow.spread,
        };
        if self.reflow.request.as_ref() == Some(&request) && self.reflow.task.is_some() {
            return;
        }
        let Some(worker) = &self.reflow.worker else {
            return;
        };
        let receiver = match worker.request(request.clone()) {
            Ok(receiver) => receiver,
            Err(error) => {
                self.message = format!("{error:#}");
                self.loading = false;
                return;
            }
        };
        self.reflow.request = Some(request);
        self.reflow.epoch += 1;
        let epoch = self.reflow.epoch;
        self.reflow.task = Some(cx.spawn(async move |this, cx| {
            let result = receiver.await;
            let _ = this.update(cx, |this, cx| {
                if this.reflow.epoch != epoch || this.screen != Screen::Reader {
                    return;
                }
                this.loading = false;
                this.reflow.task = None;
                match result
                    .map_err(anyhow::Error::from)
                    .and_then(|result| result)
                {
                    Ok(mut frame) => {
                        // The buffer is moved into the texture: copying the page
                        // on the UI thread is what made a turn stutter.
                        // Raster dimensions are device pixels at the request
                        // scale; layout and the text layer are in logical page
                        // space, so normalize the displayed aspect only.
                        let page_sizes = frame
                            .pages
                            .iter()
                            .map(|page| {
                                let scale = scale.max(f32::EPSILON);
                                (page.width as f32 / scale, page.height as f32 / scale)
                            })
                            .collect::<Vec<_>>();
                        let images = frame
                            .pages
                            .drain(..)
                            .map(|page| {
                                texture(Pixels {
                                    width: page.width,
                                    height: page.height,
                                    bgra: page.bgra,
                                })
                            })
                            .collect::<Result<Vec<_>>>();
                        match images {
                            Ok(images) => {
                                this.clear_reflow_text_selection();
                                this.reflow.page_sizes = page_sizes;
                                let old = std::mem::replace(&mut this.reflow.page_images, images);
                                if !old.is_empty() {
                                    cx.defer(move |cx| {
                                        for image in old {
                                            cx.drop_image(image, None);
                                        }
                                    });
                                }
                                this.books[this.active].reflow.epub = frame.location.clone();
                                this.books[this.active].reflow.progress = frame.progress;
                                // The queue is consumed when a step is issued, so
                                // it survives this landing and the steps behind
                                // it are issued below.
                                this.reflow.outline = Arc::new(
                                    frame
                                        .metadata
                                        .outline
                                        .iter()
                                        .map(|entry| OutlineEntry {
                                            title: entry.title.clone(),
                                            depth: entry.depth,
                                            href: entry.href.clone(),
                                            block: 0,
                                            source_offset: 0,
                                        })
                                        .collect(),
                                );
                                this.reflow.frame = Some(frame);
                                this.reflow.scroll_accum = 0.;
                                this.message.clear();
                                this.prefetch_epub(cx);
                            }
                            Err(error) => this.message = format!("{error:#}"),
                        }
                    }
                    Err(error) => this.message = format!("{error:#}"),
                }
                // A burst of clicks queued steps while this one rendered; issue
                // the next from the position that just landed.
                if this.reflow.pending_turn != 0 {
                    this.request_epub(0, 0., None, cx);
                }
                cx.notify();
            });
        }));
    }

    /// Store the reading position the list reports for a scrolled Markdown view.
    ///
    /// This must not call into [`ListState`]: Kit runs the scroll handler while
    /// the list mutably borrows its own state, so any list query here panics
    /// with `RefCell already mutably borrowed`. The caller passes the reported
    /// block instead.
    fn record_markdown_progress(
        &mut self,
        source_offset: usize,
        source_bytes: usize,
        at_end: bool,
        cx: &mut Context<Self>,
    ) {
        if self
            .books
            .get(self.active)
            .is_none_or(|book| book.format() != Some(DocumentFormat::Markdown))
        {
            return;
        }
        let Some(markdown) = &self.reflow.markdown else {
            return;
        };
        self.reflow.active_block = markdown.metadata.block_at_offset(source_offset);
        let position = &mut self.books[self.active].reflow;
        position.markdown_offset = source_offset.min(source_bytes.saturating_sub(1));
        position.scroll_offset = 0.;
        position.progress = if at_end {
            1.
        } else {
            source_offset as f32 / source_bytes.max(1) as f32
        };
        cx.notify();
    }

    /// Remember the current position after a command moved the list.
    ///
    /// Commands run outside Kit's scroll dispatch, so they may read the list.
    /// Prepare the next page while the reader looks at this one, so a turn
    /// hands over a raster that already exists.
    fn prefetch_epub(&mut self, cx: &mut Context<Self>) {
        if self.books[self.active].format() != Some(DocumentFormat::Epub) {
            return;
        }
        let Source::File(path) = &self.books[self.active].source else {
            return;
        };
        let Some(worker) = &self.reflow.worker else {
            return;
        };
        let mut next = match &self.reflow.request {
            Some(request) => request.clone(),
            None => return,
        };
        next.path = path.clone();
        next.location = self.books[self.active].reflow.epub.clone();
        next.turn = 1;
        next.scroll = 0.;
        next.find = None;
        let _ = worker.prefetch(next);
        let _ = cx;
    }

    fn record_markdown_scroll(&mut self, offset: f32, cx: &mut Context<Self>) {
        if (offset - self.reflow.markdown_scroll_offset).abs() < 0.5 {
            return;
        }
        self.reflow.markdown_scroll_offset = offset;
        self.remember_markdown_position(cx);
    }

    fn remember_markdown_position(&mut self, cx: &mut Context<Self>) {
        let Some(markdown) = &self.reflow.markdown else {
            return;
        };
        let list = markdown.state.read(cx).list_state();
        let top = list.logical_scroll_top().item_ix;
        let at_end = list.is_scrolled_to_end() == Some(true);
        let block = rendered_source_block(
            top,
            markdown.metadata.block_starts.len(),
            markdown_has_top_inset(&markdown.source, markdown.metadata.frontmatter_end),
        );
        let offset = markdown
            .metadata
            .block_starts
            .get(block)
            .copied()
            .unwrap_or_else(|| {
                let source = markdown.source.len().max(1);
                (markdown.metadata.source_bytes as f32 * block as f32
                    / list.item_count().max(1) as f32) as usize
                    * source
                    / markdown.metadata.source_bytes.max(1)
            });
        self.record_markdown_progress(offset, markdown.metadata.source_bytes, at_end, cx);
    }

    pub(super) fn turn_reflow(&mut self, delta: i32, cx: &mut Context<Self>) {
        if self.books[self.active].format() == Some(DocumentFormat::Epub) {
            self.request_epub(delta.signum() as i8, 0., None, cx);
        } else if let Some(markdown) = &self.reflow.markdown {
            markdown
                .state
                .read(cx)
                .list_state()
                .scroll_by(px(delta as f32 * self.reflow.viewport.1 * 0.85));
            self.remember_markdown_position(cx);
        }
        cx.notify();
    }

    pub(super) fn scroll_markdown(&mut self, distance: gpui_kit::Pixels, cx: &mut Context<Self>) {
        let Some(markdown) = &self.reflow.markdown else {
            return;
        };
        let list = markdown.state.read(cx).list_state();
        let mut offset = list.scroll_px_offset_for_scrollbar();
        offset.y -= distance;
        list.set_offset_from_scrollbar(offset);
        self.remember_markdown_position(cx);
    }

    /// Two-page EPUB reading.
    ///
    /// The chapter is re-paginated at half the content width, so a toggle is a
    /// new layout rather than a re-scale of the page already on screen. Only the
    /// fraction read is preserved, exactly as when the viewport or text size
    /// changes.
    pub(super) fn toggle_reflow_spread(&mut self, cx: &mut Context<Self>) {
        if self.books[self.active].format() != Some(DocumentFormat::Epub) {
            return;
        }
        let spread = &mut self.books[self.active].reflow.spread;
        *spread = !*spread;
        self.reflow.scroll_accum = 0.;
        self.request_epub(0, 0., None, cx);
        cx.notify();
    }

    pub(super) fn resize_reflow_text(&mut self, delta: i16, cx: &mut Context<Self>) {
        let current = &mut self.books[self.active].reflow.font_size;
        *current = (*current as i16 + delta).clamp(12, 32) as u8;
        let font_size = *current;
        // An open draft keeps its own layout, so it follows the same control.
        if let Some(editor) = self.books[self.active].markdown_edit.clone() {
            editor.update(cx, |editor, cx| editor.set_font_size(font_size, cx));
        }
        self.request_epub(0, 0., None, cx);
        cx.notify();
    }

    pub(super) fn toggle_reflow_width(&mut self, cx: &mut Context<Self>) {
        if self.books[self.active].format() != Some(DocumentFormat::Markdown) {
            return;
        }
        let full_width = &mut self.books[self.active].reflow.full_width;
        *full_width = !*full_width;
        let full_width = *full_width;
        if let Some(editor) = self.books[self.active].markdown_edit.clone() {
            editor.update(cx, |editor, cx| editor.set_full_width(full_width, cx));
        }
        cx.notify();
    }

    fn jump_reflow(&mut self, href: String, block: usize, cx: &mut Context<Self>) {
        if self.books[self.active].format() == Some(DocumentFormat::Epub) {
            self.books[self.active].reflow.epub = breeze_core::document::EpubLocation {
                href,
                progression: 0.,
            };
            self.request_epub(0, 0., None, cx);
        } else if let Some(markdown) = &self.reflow.markdown {
            let offset = markdown
                .metadata
                .block_starts
                .get(block)
                .copied()
                .unwrap_or(0);
            self.jump_markdown_source(offset, cx);
        }
        cx.notify();
    }

    pub(super) fn reflow_reader(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Div {
        if self.full_screen {
            return column().size_full().child(self.reflow_content(window, cx));
        }
        let p = self.p(cx);
        let epub = self.books[self.active].format() == Some(DocumentFormat::Epub);
        let main = column()
            .size_full()
            .min_w_0()
            .min_h_0()
            .child(
                row()
                    .id("reader-toolbar")
                    .test_support()
                    .overflow_x_scroll()
                    .h(px(style::TOOLBAR_HEIGHT))
                    .flex_shrink_0()
                    .px_2()
                    .gap_1()
                    .border_b_1()
                    .border_color(p.line)
                    .when(!self.sidebar, |bar| {
                        bar.child(self.command(
                            "collapse",
                            IconName::PanelLeft,
                            tr!("Navigation panel"),
                            &ToggleSidebar,
                            cx,
                        ))
                    })
                    // Markdown authors in the same content area the reader uses;
                    // these controls stay inside the existing toolbar.
                    .when(self.books[self.active].is_markdown(), |bar| {
                        let editing = self.books[self.active].markdown_editing;
                        bar.child(
                            TabBar::new("markdown-modes")
                                .segmented()
                                .small()
                                .selected_index(usize::from(editing))
                                .on_click(cx.listener(|this, index, window, cx| {
                                    if this.books[this.active].markdown_editing != (*index == 1) {
                                        this.toggle_markdown_editing(window, cx);
                                    }
                                }))
                                .child(
                                    Tab::new()
                                        .icon(IconName::BookOpenText)
                                        .aria_label(tr!("Reading view"))
                                        .tooltip(|window, cx| {
                                            Tooltip::new(tr!("Reading view")).build(window, cx)
                                        }),
                                )
                                .child(
                                    Tab::new()
                                        .icon(IconName::Pencil)
                                        .aria_label(tr!("Edit document"))
                                        .tooltip(|window, cx| {
                                            Tooltip::new(tr!("Edit document")).build(window, cx)
                                        }),
                                ),
                        )
                    })
                    .child(self.reflow_toolbar_tools(cx))
                    .child(
                        self.command(
                            "reader-search",
                            IconName::Search,
                            tr!("Search document"),
                            &SearchDocument,
                            cx,
                        )
                        .selected(self.search_open),
                    )
                    .child(
                        self.command(
                            "reader-star",
                            IconName::Bookmark,
                            tr!("Favorites"),
                            &Favorite,
                            cx,
                        )
                        .icon(document_icons::bookmark_icon(
                            self.books[self.active].favorite,
                        )),
                    )
                    .when(!self.document_panel.open, |bar| {
                        bar.child(self.command(
                            "document-panel-toggle",
                            IconName::PanelRight,
                            tr!("Document sidebar"),
                            &ToggleDocumentPanel,
                            cx,
                        ))
                    }),
            )
            .child(self.reflow_content(window, cx))
            .when(self.status_bar_visible, |main| {
                main.child(
                    row()
                        .id("reflow-status")
                        .test_support()
                        .h(px(32.))
                        .flex_shrink_0()
                        .px_2()
                        .gap_1()
                        .border_t_1()
                        .border_color(p.line)
                        .bg(p.bg)
                        // Page turns stay on the keyboard; the status bar starts
                        // with the draft's editor status when one is open.
                        .when(
                            self.books[self.active].markdown_editing
                                && self.books[self.active].markdown_edit.is_some(),
                            |bar| {
                                let status = self.books[self.active]
                                    .markdown_edit
                                    .as_ref()
                                    .map(|editor| editor.read(cx).status(cx));
                                bar.child(
                                    div()
                                        .id("markdown-editor-status")
                                        .test_support()
                                        .text_size(px(11.))
                                        .text_color(p.muted)
                                        .when_some(status, |item, status| item.child(status)),
                                )
                            },
                        )
                        // Only a paginated EPUB has a page position. Markdown
                        // scrolls without pages, so it reports no reading progress
                        // here rather than a percentage that duplicates the
                        // scrollbar.
                        .when_some(self.reflow_position_text(), |bar, position| {
                            bar.child(
                                div()
                                    .id("reflow-position")
                                    .test_support()
                                    .min_w(px(56.))
                                    .text_size(px(12.))
                                    .text_color(p.muted)
                                    // The visible text is a page count; name it for
                                    // assistive technology and for the tests that pin it.
                                    .aria_label(position.clone())
                                    .child(position),
                            )
                        })
                        .when(!epub, |bar| match self.markdown_characters(cx) {
                            Some(characters) => {
                                // The accessibility label repeats the visible text
                                // so tests and assistive technology read one value.
                                let text: SharedString =
                                    tf!("{} characters", grouped_characters(characters)).into();
                                bar.child(
                                    div()
                                        .id("markdown-characters")
                                        .test_support()
                                        .flex_shrink_0()
                                        .text_size(px(11.))
                                        .text_color(p.muted)
                                        .aria_label(text.clone())
                                        .tooltip(|window, cx| {
                                            Tooltip::new(tr!("Characters in the Markdown text"))
                                                .build(window, cx)
                                        })
                                        .child(text),
                                )
                            }
                            None => bar,
                        })
                        .child(
                            div()
                                .flex_1()
                                .min_w(px(50.))
                                .overflow_hidden()
                                .text_size(px(11.))
                                .text_color(p.muted)
                                .when_some(self.reflow_notice(), |area, notice| {
                                    area.child(
                                        div()
                                            .id("reflow-status-notice")
                                            .test_support()
                                            .child(notice),
                                    )
                                }),
                        )
                        .when(epub, |bar| {
                            // Two-page reading is a view state: the button keeps the
                            // pressed look while the spread is on.
                            let spread = self.books[self.active].reflow.spread;
                            bar.child({
                                let color = self.status_color("reflow-spread", cx);
                                let button = Button::new("reflow-spread")
                                    .ghost()
                                    .small()
                                    .icon(IconName::Columns2)
                                    // The pressed look and the toggle state are
                                    // separate in Kit: one paints, one is announced.
                                    .selected(spread)
                                    .toggled(spread)
                                    .accessibility_label(if spread {
                                        tr!("Single-page reading")
                                    } else {
                                        tr!("Two-page reading")
                                    })
                                    .tooltip(if spread {
                                        tr!("Single-page reading")
                                    } else {
                                        tr!("Two-page reading")
                                    })
                                    .on_click(
                                        cx.listener(|this, _, _, cx| this.toggle_reflow_spread(cx)),
                                    )
                                    .text_color(if spread { p.text } else { color });
                                self.status_control("reflow-spread", button, cx)
                            })
                        })
                        .when(!epub, |bar| {
                            let full_width = self.books[self.active].reflow.full_width;
                            bar.child({
                                let color = self.status_color("reflow-width", cx);
                                let button = Button::new("reflow-width")
                                    .ghost()
                                    .small()
                                    .icon(if full_width {
                                        IconName::ChevronsRightLeft
                                    } else {
                                        IconName::ChevronsLeftRight
                                    })
                                    .selected(full_width)
                                    .accessibility_label(if full_width {
                                        tr!("Use the maximum reading width")
                                    } else {
                                        tr!("Use the full window width")
                                    })
                                    .tooltip(if full_width {
                                        tr!("Use the maximum reading width")
                                    } else {
                                        tr!("Use the full window width")
                                    })
                                    .on_click(
                                        cx.listener(|this, _, _, cx| this.toggle_reflow_width(cx)),
                                    )
                                    .text_color(if full_width { p.text } else { color });
                                self.status_control("reflow-width", button, cx)
                            })
                        })
                        .child({
                            let color = self.status_color("text-smaller", cx);
                            let button = Button::new("text-smaller")
                                .ghost()
                                .small()
                                .icon(IconName::Minus)
                                .accessibility_label(tr!("Decrease text size"))
                                .tooltip(tr!("Decrease text size"))
                                .disabled(self.books[self.active].reflow.font_size <= 12)
                                .on_click(
                                    cx.listener(|this, _, _, cx| this.resize_reflow_text(-1, cx)),
                                )
                                .text_color(color);
                            self.status_control("text-smaller", button, cx)
                        })
                        .child(
                            div()
                                .id("text-size")
                                .test_support()
                                .min_w(px(48.))
                                .text_center()
                                .text_size(px(12.))
                                .text_color(p.muted)
                                .child(format!("{} px", self.books[self.active].reflow.font_size)),
                        )
                        .child({
                            let color = self.status_color("text-larger", cx);
                            let button = Button::new("text-larger")
                                .ghost()
                                .small()
                                .icon(IconName::Plus)
                                .accessibility_label(tr!("Increase text size"))
                                .tooltip(tr!("Increase text size"))
                                .disabled(self.books[self.active].reflow.font_size >= 32)
                                .on_click(
                                    cx.listener(|this, _, _, cx| this.resize_reflow_text(1, cx)),
                                )
                                .text_color(color);
                            self.status_control("text-larger", button, cx)
                        })
                        .child(self.fullscreen_button(cx)),
                )
            });
        let center = if self.sidebar {
            h_resizable("reader-split")
                .with_state(&self.navigation_split)
                .with_handle_appearance(reader::navigation_handle_appearance(p))
                .child(
                    resizable_panel()
                        .size(px(self.navigation_width))
                        .size_range(
                            px(reader::NAVIGATION_MIN_WIDTH)..px(reader::NAVIGATION_MAX_WIDTH),
                        )
                        .child(self.reflow_navigation(cx)),
                )
                .child(
                    resizable_panel()
                        .size_range(
                            px(if self.is_split() { 160. } else { 300. })..gpui_kit::Pixels::MAX,
                        )
                        .child(main),
                )
                .into_any_element()
        } else {
            main.into_any_element()
        };
        row()
            .flex_1()
            .min_w(px(if self.is_split() {
                self.navigation_width * f32::from(self.sidebar) + self.document_panel.width() + 160.
            } else {
                0.
            }))
            .min_h_0()
            .items_stretch()
            .child(div().size_full().min_w_0().min_h_0().child(center))
            .when(self.document_panel.open, |row| {
                row.child(self.document_sidebar(cx))
            })
    }

    /// The page the reader is on, as `current / total`.
    ///
    /// EPUB is paginated one chapter at a time — laying out the whole book to
    /// number its pages would cost what the chapter-local design exists to avoid
    /// — so the count is the page within the open chapter. Markdown has no pages
    /// and reports no position, and neither does a chapter whose frame has not
    /// landed yet.
    fn reflow_position_text(&self) -> Option<String> {
        let frame = self.reflow.frame.as_ref()?;
        let left = frame.chapter_page + 1;
        let right = (frame.chapter_page + frame.columns).min(frame.chapter_pages);
        Some(if right > left {
            format!("{left}\u{2013}{right} / {}", frame.chapter_pages)
        } else {
            format!("{left} / {}", frame.chapter_pages)
        })
    }

    /// Characters the Markdown reading surface shows, or `None` for EPUB.
    ///
    /// An open draft reports its own projection, so the count follows the text
    /// on screen while it is edited. The loaded document supplies the count
    /// otherwise, which is the same rendered-text measure the document details
    /// panel already reports.
    fn markdown_characters(&self, cx: &App) -> Option<usize> {
        let book = &self.books[self.active];
        book.markdown_editing
            .then_some(book.markdown_edit.as_ref())
            .flatten()
            .map(|editor| editor.read(cx).characters(cx))
            .or_else(|| {
                self.reflow
                    .markdown
                    .as_ref()
                    .map(|markdown| markdown.metadata.characters)
            })
    }

    /// The one status notice the reflow reader shows, or `None` when there is
    /// nothing to report. A healthy document leaves the area empty instead of
    /// announcing that nothing is wrong.
    fn reflow_notice(&self) -> Option<String> {
        if self.loading {
            Some(tr!("Reading…").into())
        } else if !self.message.is_empty() {
            Some(self.message.clone())
        } else {
            None
        }
    }

    /// Which outline row the reader is inside of.
    ///
    /// EPUB rows match the open chapter; Markdown rows are the last heading at
    /// or before the block at the top of the view.
    fn reflow_active_outline(&self) -> Option<usize> {
        let entries = &self.reflow.outline;
        if entries.is_empty() {
            return None;
        }
        match self.books[self.active].format() {
            Some(DocumentFormat::Epub) => {
                let frame = self.reflow.frame.as_ref()?;
                let chapter = &frame.metadata.chapters[frame.chapter].href;
                entries
                    .iter()
                    .position(|entry| entry.href.split('#').next() == Some(chapter.as_str()))
                    .or(Some(0))
            }
            _ => entries
                .iter()
                .rposition(|entry| entry.block <= self.reflow.active_block),
        }
    }

    /// The toolbar's flexible middle space for the reflow readers.
    ///
    /// Markdown editing centers its Undo, Redo, and Save group there, the same
    /// way the PDF reader centers its annotation tools between the mode tabs and
    /// the trailing document commands. Equal zero-basis spacers keep the group
    /// centered and collapse instead of overflowing a narrow window, and a
    /// format with no group of its own leaves the space empty.
    fn reflow_toolbar_tools(&self, cx: &mut Context<Self>) -> AnyElement {
        let actions = (self.books[self.active].is_markdown()
            && self.books[self.active].markdown_editing)
            .then(|| self.markdown_edit_actions(cx));
        row()
            .id("reflow-tools")
            .test_support()
            .flex_1()
            .min_w_0()
            .overflow_x_scroll()
            .child(div().flex_1().min_w_0())
            .children(actions)
            .child(div().flex_1().min_w_0())
            .into_any_element()
    }

    /// Undo, Redo, and Save for the open Markdown draft.
    ///
    /// The commands are the reader's document actions, dispatched to Kit's
    /// editor history or to the Markdown save path while the draft has focus.
    /// Each one is disabled when it would do nothing: no step to take back or
    /// replay, no unsaved revision, or a publication already in flight.
    fn markdown_edit_actions(&self, cx: &mut Context<Self>) -> AnyElement {
        let (can_undo, can_redo, can_save) = match self.books[self.active].markdown_edit.as_ref() {
            Some(editor) => {
                let editor = editor.read(cx);
                (editor.can_undo(), editor.can_redo(), editor.can_save(cx))
            }
            None => (false, false, false),
        };
        row()
            .id("markdown-actions")
            .test_support()
            .flex_shrink_0()
            .gap_1()
            .child(
                self.command(
                    "markdown-undo",
                    IconName::Undo2,
                    tr!("Undo Markdown edit"),
                    &UndoAnnotation,
                    cx,
                )
                .disabled(!can_undo),
            )
            .child(
                self.command(
                    "markdown-redo",
                    IconName::Redo2,
                    tr!("Redo Markdown edit"),
                    &RedoAnnotation,
                    cx,
                )
                .disabled(!can_redo),
            )
            .child(
                self.command(
                    "markdown-save",
                    IconName::Save,
                    tr!("Save Markdown draft"),
                    &SaveAnnotations,
                    cx,
                )
                .disabled(!can_save),
            )
            .into_any_element()
    }

    fn reflow_outline_search_open(&self) -> bool {
        self.reflow.outline_search.is_some()
    }

    fn reflow_toggle_outline_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(search) = self.reflow.outline_search.take() {
            let mut scroll = self.reflow.outline_scroll.0.borrow_mut();
            scroll.deferred_scroll_to_item = None;
            scroll.base_handle.set_offset(search.saved_offset);
            window.focus(&self.focus, cx);
        } else {
            let input =
                cx.new(|cx| InputState::new(window, cx).placeholder(tr!("Search outline titles")));
            let subscription = cx.subscribe(&input, |this, input, event, cx| {
                if matches!(event, InputEvent::Change)
                    && let Some(search) = &mut this.reflow.outline_search
                    && search.input == input
                {
                    search.collapsed = if input.read(cx).value().trim().is_empty() {
                        this.reflow.outline_folds.clone()
                    } else {
                        HashSet::new()
                    };
                    this.reflow
                        .outline_scroll
                        .scroll_to_item(0, ScrollStrategy::Top);
                    cx.notify();
                }
            });
            input.update(cx, |input, cx| input.focus(window, cx));
            self.reflow.outline_search = Some(OutlineSearch {
                input,
                collapsed: self.reflow.outline_folds.clone(),
                saved_offset: self.reflow.outline_scroll.0.borrow().base_handle.offset(),
                _subscription: subscription,
            });
        }
        cx.notify();
    }

    fn reflow_outline_search_bar(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let search = self.reflow.outline_search.as_ref()?;
        Some(
            search::search_row(self.p(cx).line)
                .id("outline-search-bar")
                .test_support()
                .w_full()
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .child(Input::new(&search.input).small()),
                )
                .child(
                    Button::new("close-outline-search")
                        .ghost()
                        .small()
                        .icon(IconName::X)
                        .accessibility_label(tr!("Close outline search"))
                        .tooltip(tr!("Close outline search"))
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.reflow_toggle_outline_search(window, cx)
                        })),
                )
                .on_action(cx.listener(
                    |this, _: &gpui_kit::component::input::Escape, window, cx| {
                        this.reflow_toggle_outline_search(window, cx);
                    },
                ))
                .into_any_element(),
        )
    }

    fn reflow_outline_menu(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let view = cx.entity();
        Button::new("outline-more")
            .ghost()
            .small()
            .icon(IconName::EllipsisVertical)
            .accessibility_label(tr!("More outline actions"))
            .tooltip(tr!("More outline actions"))
            .dropdown_menu(move |menu, window, _| {
                menu.item(
                    PopupMenuItem::new(tr!("Expand all"))
                        .icon(IconName::ListChevronsUpDown)
                        .on_click(window.listener_for(&view, |this, _, _, cx| {
                            this.reflow_set_outline_expanded(true, cx);
                        })),
                )
                .item(
                    PopupMenuItem::new(tr!("Collapse all"))
                        .icon(IconName::ListChevronsDownUp)
                        .on_click(window.listener_for(&view, |this, _, _, cx| {
                            this.reflow_set_outline_expanded(false, cx);
                        })),
                )
            })
    }

    fn reflow_set_outline_expanded(&mut self, expanded: bool, cx: &mut Context<Self>) {
        let outline = self.reflow.outline.clone();
        self.reflow.outline_folds.clear();
        if !expanded {
            self.reflow
                .outline_folds
                .extend((0..outline.len()).filter(|&index| outline_has_children(&outline, index)));
        }
        self.reflow
            .outline_scroll
            .scroll_to_item(0, ScrollStrategy::Top);
        cx.notify();
    }

    fn reflow_toggle_outline(&mut self, index: usize, cx: &mut Context<Self>) {
        if !self.reflow.outline_folds.insert(index) {
            self.reflow.outline_folds.remove(&index);
        }
        cx.notify();
    }

    fn jump_reflow_outline(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(entry) = self.reflow.outline.get(index).cloned() else {
            return;
        };
        if self.books[self.active].format() == Some(DocumentFormat::Epub) {
            self.jump_reflow(entry.href, 0, cx);
        } else {
            self.jump_markdown_source(entry.source_offset, cx);
        }
    }

    fn reflow_navigation(&self, cx: &mut Context<Self>) -> AnyElement {
        let p = self.p(cx);
        let query = self
            .reflow
            .outline_search
            .as_ref()
            .map(|search| search.input.read(cx).value().trim().to_lowercase())
            .unwrap_or_default();
        let nested = (0..self.reflow.outline.len())
            .any(|index| outline_has_children(&self.reflow.outline, index));
        let visible = visible_outline(&self.reflow.outline, self.reflow.outline_folds(), &query);
        column()
            .size_full()
            .min_w_0()
            .min_h_0()
            .bg(p.sidebar)
            .child(
                row()
                    .id("navigation-header")
                    .test_support()
                    .h(px(style::TOOLBAR_HEIGHT))
                    .flex_shrink_0()
                    .px(px(8.))
                    .border_b_1()
                    .border_color(p.line)
                    // The leading slot pins the collapse control to the header's
                    // left edge, the way the PDF reader's panel header does, and
                    // the trailing slot right-aligns the outline search.
                    .child(
                        row()
                            .id("navigation-header-leading")
                            .test_support()
                            .flex_1()
                            .min_w_0()
                            .justify_start()
                            .child(self.command(
                                "collapse",
                                IconName::PanelLeft,
                                tr!("Navigation panel"),
                                &ToggleSidebar,
                                cx,
                            )),
                    )
                    .child(
                        TabBar::new("reflow-nav-tabs")
                            .segmented()
                            .small()
                            .flex_shrink_0()
                            .selected_index(0)
                            .child(
                                Tab::new()
                                    .icon(IconName::TableOfContents)
                                    .aria_label(tr!("Outline"))
                                    .tooltip(|window, cx| {
                                        Tooltip::new(tr!("Outline")).build(window, cx)
                                    }),
                            ),
                    )
                    .child(
                        row()
                            .id("navigation-header-trailing")
                            .test_support()
                            .flex_1()
                            .min_w_0()
                            .justify_end()
                            .child(
                                row()
                                    .gap(px(4.))
                                    .flex_shrink_0()
                                    .child(
                                        Button::new("outline-search")
                                            .ghost()
                                            .small()
                                            .icon(IconName::Search)
                                            .selected(self.reflow_outline_search_open())
                                            .accessibility_label(tr!("Search outline titles"))
                                            .tooltip(tr!("Search outline titles"))
                                            .on_click(cx.listener(|this, _, window, cx| {
                                                this.reflow_toggle_outline_search(window, cx);
                                            })),
                                    )
                                    .when(nested, |actions| {
                                        actions.child(self.reflow_outline_menu(cx))
                                    }),
                            ),
                    ),
            )
            .children(self.reflow_outline_search_bar(cx))
            .child(
                column()
                    .id("navigation-content")
                    .test_support()
                    .flex_1()
                    .min_h_0()
                    .child(self.reflow_outline_view(visible, cx)),
            )
            .into_any_element()
    }

    fn reflow_outline_view(&self, visible: Vec<usize>, cx: &mut Context<Self>) -> AnyElement {
        if visible.is_empty() {
            return div()
                .id("outline-no-results")
                .test_support()
                .p_2()
                .text_color(self.p(cx).muted)
                .child(tr!("No matching outline titles"))
                .into_any_element();
        }
        let outline = self.reflow.outline.clone();
        let nested = (0..outline.len()).any(|index| outline_has_children(&outline, index));
        let active = self.reflow_active_outline();
        let active = visible_active_outline(&visible, active);
        let folds = self.reflow.outline_folds();
        let folds = folds.clone();
        div()
            .size_full()
            .pt_1()
            .child(
                // A distinct id keeps this list from inheriting the PDF
                // outline element's cached measurements when a library switches
                // between a PDF and a reflowable document.
                uniform_list(
                    "reflow-outline-list",
                    visible.len(),
                    cx.processor(move |_this, range: std::ops::Range<usize>, _, cx| {
                        range
                            .map(|row_index| {
                                let index = visible[row_index];
                                let item = &outline[index];
                                let has_children = outline_has_children(&outline, index);
                                let collapsed = folds.contains(&index);
                                row()
                                    .w_full()
                                    .h(px(24.))
                                    .px_2()
                                    .pl(px(8. + item.depth.min(12) as f32 * 10.))
                                    .when(nested, |row| {
                                        row.child(if has_children {
                                            Button::new(("outline-toggle", index))
                                                .ghost()
                                                .small()
                                                .size(px(24.))
                                                .flex_shrink_0()
                                                .icon(if collapsed {
                                                    IconName::ChevronRight
                                                } else {
                                                    IconName::ChevronDown
                                                })
                                                .accessibility_label(format!(
                                                    "{}：{}",
                                                    if collapsed {
                                                        tr!("Expand")
                                                    } else {
                                                        tr!("Collapse")
                                                    },
                                                    item.title
                                                ))
                                                .tooltip(if collapsed {
                                                    tr!("Expand child entries")
                                                } else {
                                                    tr!("Collapse child entries")
                                                })
                                                .on_click(cx.listener(move |this, _, _, cx| {
                                                    this.reflow_toggle_outline(index, cx)
                                                }))
                                                .into_any_element()
                                        } else {
                                            div().w(px(24.)).flex_shrink_0().into_any_element()
                                        })
                                    })
                                    .child(
                                        Button::new(("outline", index))
                                            .ghost()
                                            .small()
                                            .h(px(24.))
                                            .flex_1()
                                            .min_w_0()
                                            .px_1()
                                            .accessibility_label(item.title.clone())
                                            .child(
                                                row().w_full().child(
                                                    div()
                                                        .flex_1()
                                                        .min_w_0()
                                                        .text_ellipsis()
                                                        .text_left()
                                                        .child(item.title.clone()),
                                                ),
                                            )
                                            .selected(active == Some(index))
                                            .tooltip(item.title.clone())
                                            .on_click(cx.listener(move |this, _, _, cx| {
                                                this.jump_reflow_outline(index, cx)
                                            })),
                                    )
                            })
                            .collect::<Vec<_>>()
                    }),
                )
                .track_scroll(&self.reflow.outline_scroll)
                .size_full(),
            )
            .into_any_element()
    }

    /// Reading and editing are two views of the same tab. The draft session is
    /// created on first entry and kept while the tab stays open, so switching
    /// tabs or returning to reading never discards it.
    pub(super) fn toggle_markdown_editing(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.books[self.active].is_markdown() {
            return;
        }
        if self.books[self.active].markdown_editing {
            let draft = self.books[self.active]
                .markdown_edit
                .as_ref()
                .map(|editor| editor.read(cx).source_text(cx));
            let offset = if let Some(editor) = self.books[self.active].markdown_edit.as_ref() {
                let offset = editor.read(cx).source_offset_at_viewport(cx);
                let source_bytes = draft.as_ref().map_or(1, |draft| draft.len().max(1));
                let position = &mut self.books[self.active].reflow;
                position.markdown_offset = offset.min(source_bytes.saturating_sub(1));
                position.progress = offset as f32 / source_bytes as f32;
                Some(offset)
            } else {
                None
            };
            self.books[self.active].markdown_editing = false;
            if let (Some(draft), Some(offset)) = (draft, offset) {
                if self
                    .reflow
                    .markdown
                    .as_ref()
                    .is_some_and(|markdown| markdown.source.as_ref() == draft.as_str())
                {
                    // An unchanged preview already owns the parsed document and
                    // decoded images. Reuse it; rebuilding the TextView here made
                    // every mode switch reparse the file and refetch remote badges.
                    self.jump_markdown_source(offset, cx);
                    cx.notify();
                    return;
                }
                // The reader is the preview of the draft: re-read it instead of
                // the file on disk.
                self.reflow.source = None;
                self.reflow.scroll_accum = 0.;
                self.load_reflow_preserving_images(cx);
            }
            cx.notify();
            return;
        }
        // The reader's scroll handler records the top visible source block. Use
        // that checkpoint so entering the editor does not jump back to the file
        // start, even when the editor session already exists.
        self.remember_markdown_position(cx);
        let offset = self.books[self.active].reflow.markdown_offset;
        if self.books[self.active].markdown_edit.is_none() {
            let Source::File(path) = self.books[self.active].source.clone() else {
                self.message = tr!("Editing needs a Markdown file on disk").into();
                cx.notify();
                return;
            };
            let body = self.books[self.active].reflow.font_size;
            let full_width = self.books[self.active].reflow.full_width;
            let editor = cx.new(|cx| {
                markdown_editor::MarkdownEditor::open(path, body, full_width, window, cx)
            });
            self.books[self.active].markdown_edit = Some(editor);
        }
        self.books[self.active].markdown_editing = true;
        if let Some(editor) = self.books[self.active].markdown_edit.clone() {
            let focus = editor.read(cx).focus_handle();
            window.focus(&focus, cx);
            editor.update(cx, |editor, cx| {
                editor.reveal_source_offset(offset, window, cx)
            });
        }
        cx.notify();
    }

    /// Maps a pointer in a laid-out page image to normalized page coordinates.
    fn reflow_page_point(
        &self,
        page: usize,
        position: Point<gpui_kit::Pixels>,
    ) -> Option<[f32; 2]> {
        let bounds = *self.reflow.page_bounds.get(&page)?;
        let raster = *self.reflow.page_sizes.get(page)?;
        let display = contained_page_bounds(bounds, raster);
        if display.size.width <= px(0.) || display.size.height <= px(0.) {
            return None;
        }
        Some([
            (f32::from(position.x - display.origin.x) / f32::from(display.size.width))
                .clamp(0., 1.),
            (f32::from(position.y - display.origin.y) / f32::from(display.size.height))
                .clamp(0., 1.),
        ])
    }

    /// The laid-out page under a pointer, or the nearest page when the pointer
    /// is in the gutter or padding.
    fn reflow_page_at(&self, position: Point<gpui_kit::Pixels>) -> Option<usize> {
        self.reflow
            .page_bounds
            .iter()
            .min_by(|(page_a, a), (page_b, b)| {
                let a_display = contained_page_bounds(
                    **a,
                    self.reflow
                        .page_sizes
                        .get(**page_a)
                        .copied()
                        .unwrap_or((1., 1.)),
                );
                let b_display = contained_page_bounds(
                    **b,
                    self.reflow
                        .page_sizes
                        .get(**page_b)
                        .copied()
                        .unwrap_or((1., 1.)),
                );
                page_bounds_distance(a_display, position)
                    .total_cmp(&page_bounds_distance(b_display, position))
                    .then(page_a.cmp(page_b))
            })
            .map(|(page, _)| *page)
    }

    fn begin_reflow_text_selection(
        &mut self,
        page: usize,
        position: Point<gpui_kit::Pixels>,
        cx: &mut Context<Self>,
    ) {
        if self.books[self.active].state.mode != Mode::Read {
            return;
        }
        let caret = self.reflow_page_point(page, position).and_then(|point| {
            self.reflow
                .frame
                .as_ref()?
                .text_pages
                .get(page)?
                .caret(point)
        });
        let Some(caret) = caret else {
            return;
        };
        self.text_selection = None;
        self.selection.clear();
        self.selected_text.clear();
        self.reflow.text_selection = Some(ReflowTextSelection {
            anchor: (page, caret),
            focus: (page, caret),
        });
        self.reflow.selection_active = true;
        cx.notify();
    }

    fn update_reflow_text_selection(
        &mut self,
        page: usize,
        position: Point<gpui_kit::Pixels>,
        cx: &mut Context<Self>,
    ) {
        if !self.reflow.selection_active {
            return;
        }
        let caret = self.reflow_page_point(page, position).and_then(|point| {
            self.reflow
                .frame
                .as_ref()?
                .text_pages
                .get(page)?
                .caret(point)
        });
        let Some(caret) = caret else {
            return;
        };
        if let Some(selection) = &mut self.reflow.text_selection {
            selection.focus = (page, caret);
            self.refresh_reflow_text_selection();
            cx.notify();
        }
    }

    fn end_reflow_text_selection(&mut self, cx: &mut Context<Self>) {
        if !self.reflow.selection_active {
            return;
        }
        self.reflow.selection_active = false;
        if self
            .reflow
            .text_selection
            .is_some_and(ReflowTextSelection::collapsed)
        {
            self.clear_reflow_text_selection();
            self.message.clear();
        } else {
            self.refresh_reflow_text_selection();
            self.message = tf!(
                "{} characters selected · ⌘/Ctrl+C to copy",
                self.selected_text.chars().count()
            );
        }
        cx.notify();
    }

    fn refresh_reflow_text_selection(&mut self) {
        let Some(selection) = self.reflow.text_selection else {
            return;
        };
        let Some(frame) = self.reflow.frame.as_ref() else {
            return;
        };
        let mut selected = Vec::new();
        let mut rectangles = Vec::new();
        for (page, text) in frame.text_pages.iter().enumerate() {
            let range = selection.on_page(page, text.glyphs.len());
            if range.is_empty() {
                continue;
            }
            let piece = text.text(range.clone());
            if !piece.is_empty() {
                selected.push(piece);
                rectangles.push((page, text.rects(range)));
            }
        }
        self.selected_text = selected.join("\n");
        self.reflow.selection = rectangles;
    }

    fn clear_reflow_text_selection(&mut self) {
        self.reflow.text_selection = None;
        self.reflow.selection.clear();
        self.reflow.selection_active = false;
        self.selected_text.clear();
    }

    fn reflow_content(&mut self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let p = self.p(cx);
        let epub = self.books[self.active].format() == Some(DocumentFormat::Epub);
        let scale = window.scale_factor();
        let view = cx.entity().downgrade();
        let menu_state = context_menus::ContentMenu::Reflow {
            epub,
            spread: self.books[self.active].reflow.spread,
            full_width: self.books[self.active].reflow.full_width,
            font_size: self.books[self.active].reflow.font_size,
            full_screen: self.full_screen,
        };
        let menu_view = cx.entity();
        let mut content = div()
            .id("reflow-viewport")
            .test_support()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .overflow_hidden()
            .bg(if epub { rgb(0xffffff).into() } else { p.bg })
            .on_prepaint(move |bounds, _, cx| {
                let _ = view.update(cx, |this, cx| {
                    let viewport = (
                        f32::from(bounds.size.width).max(240.).round(),
                        f32::from(bounds.size.height).max(200.).round(),
                        scale,
                    );
                    if this.reflow.viewport != viewport {
                        this.reflow.viewport = viewport;
                        if epub && !this.reflow.queued {
                            this.reflow.queued = true;
                            let epoch = this.reflow.epoch;
                            let queued_view = view.clone();
                            cx.defer(move |cx| {
                                let _ = queued_view.update(cx, |this, cx| {
                                    this.reflow.queued = false;
                                    if this.reflow.epoch == epoch {
                                        this.request_epub(0, 0., None, cx);
                                    }
                                });
                            });
                        }
                    }
                });
            });
        if epub {
            content = content.on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, _, cx| {
                if !this.reflow.selection_active {
                    return;
                }
                if let Some(page) = this.reflow_page_at(event.position) {
                    this.update_reflow_text_selection(page, event.position, cx);
                }
            }));
            content = content.on_scroll(cx.listener(|this, event: &ScrollEvent, _, cx| {
                // A trackpad emits dozens of wheel events for one swipe.
                // Requesting a render for each of them re-laid out and
                // superseded the page continuously, so the accumulator only
                // turns a page when a whole viewport has been crossed.
                let delta = event.delta.pixel_delta(px(20.));
                // A third of a viewport per page: a mouse notch turns a page
                // after a few clicks, and a trackpad flick crosses one or two
                // without ever launching a render per event.
                let page = (this.reflow.viewport.1 / 3.).max(120.);
                this.reflow.scroll_accum -= f32::from(delta.y);
                let mut turn = 0i8;
                while this.reflow.scroll_accum >= page {
                    this.reflow.scroll_accum -= page;
                    turn = turn.saturating_add(1);
                }
                while this.reflow.scroll_accum <= -page {
                    this.reflow.scroll_accum += page;
                    turn = turn.saturating_sub(1);
                }
                if turn != 0 {
                    this.request_epub(turn.clamp(-8, 8), 0., None, cx);
                }
            }));
            if !self.reflow.page_images.is_empty() {
                let spread = self
                    .reflow
                    .frame
                    .as_ref()
                    .is_some_and(|frame| frame.columns == 2);
                let images = self.reflow.page_images.clone();
                let page_sizes = self.reflow.page_sizes.clone();
                let selections = (0..images.len())
                    .map(|page| {
                        self.reflow
                            .selection
                            .iter()
                            .find(|(candidate, _)| *candidate == page)
                            .map(|(_, rects)| rects.clone())
                            .unwrap_or_default()
                    })
                    .collect::<Vec<_>>();
                let mut pages = row()
                    .id("reflow-pages")
                    .size_full()
                    .min_w_0()
                    .items_center()
                    .justify_center();
                for (index, image) in images.iter().enumerate() {
                    let page_size = page_sizes.get(index).copied().unwrap_or((1., 1.));
                    let rects = selections.get(index).cloned().unwrap_or_default();
                    let view = cx.entity();
                    pages = pages.child(
                        div()
                            .id(("reflow-page", index))
                            .test_support()
                            .relative()
                            .h_full()
                            .min_w_0()
                            .flex_1()
                            .cursor(CursorStyle::IBeam)
                            .on_prepaint(move |bounds, _, cx| {
                                view.update(cx, |this, _| {
                                    this.reflow.page_bounds.insert(index, bounds);
                                });
                            })
                            .capture_any_mouse_down(cx.listener(
                                move |this, event: &MouseDownEvent, _, cx| {
                                    if event.button == MouseButton::Left {
                                        this.begin_reflow_text_selection(index, event.position, cx);
                                    }
                                },
                            ))
                            .capture_any_mouse_up(cx.listener(
                                move |this, _: &MouseUpEvent, _, cx| {
                                    this.end_reflow_text_selection(cx);
                                },
                            ))
                            .child(
                                img(image.clone())
                                    .size_full()
                                    .object_fit(ObjectFit::Contain),
                            )
                            .child(
                                canvas(
                                    move |_, _, _| rects,
                                    move |bounds, rects, window, _| {
                                        let display = contained_page_bounds(bounds, page_size);
                                        for rect in rects {
                                            let x = f32::from(display.origin.x)
                                                + rect[0] * f32::from(display.size.width);
                                            let y = f32::from(display.origin.y)
                                                + rect[1] * f32::from(display.size.height);
                                            window.paint_quad(fill(
                                                Bounds {
                                                    origin: point(px(x), px(y)),
                                                    size: size(
                                                        px((rect[2] - rect[0])
                                                            * f32::from(display.size.width)),
                                                        px((rect[3] - rect[1])
                                                            * f32::from(display.size.height)),
                                                    ),
                                                },
                                                Rgba {
                                                    a: 0.28,
                                                    ..rgb(annotation_editing::SELECTION_COLOR)
                                                },
                                            ));
                                        }
                                    },
                                )
                                .absolute()
                                .top_0()
                                .left_0()
                                .size_full(),
                            ),
                    );
                }
                // Keep an odd final spread centered at the same half-width as
                // its preceding pairs instead of enlarging its only page.
                if spread && images.len() == 1 {
                    pages = pages.child(div().h_full().flex_1().min_w_0());
                }
                content = content.child(pages);
            }
        } else if let Some(editor) = self.books[self.active]
            .markdown_editing
            .then(|| self.books[self.active].markdown_edit.clone())
            .flatten()
        {
            // The editor owns the complete content panel so its scrollbar stays
            // at the outer edge. Its source column applies the standard measure.
            content = content.child(
                div()
                    .id("markdown-column")
                    .test_support()
                    .size_full()
                    .min_h_0()
                    .child(editor),
            );
        } else if let Some(markdown) = self.reflow.markdown.clone() {
            // One TextView owns the complete Markdown document: parsing,
            // virtualized block layout, selection, links, tables, tasks, images
            // and code cards all come from GPUI Kit.
            let full_width = self.books[self.active].reflow.full_width;
            let body = f32::from(self.books[self.active].reflow.font_size);
            let view = cx.entity().downgrade();
            let image_view = cx.entity().downgrade();
            let source = self.books[self.active].source.clone();
            let extensions = markdown.extensions.clone();
            let state = markdown.state.clone();
            let scroll_view = cx.entity().downgrade();
            let block_starts = markdown.metadata.block_starts.clone();
            let source_bytes = markdown.metadata.source_bytes.max(1);
            let has_top_inset =
                markdown_has_top_inset(&markdown.source, markdown.metadata.frontmatter_end);
            state
                .read(cx)
                .list_state()
                .set_scroll_handler(move |event, _, cx| {
                    let first = rendered_source_block(
                        event.visible_range.start,
                        block_starts.len(),
                        has_top_inset,
                    );
                    let source_offset = block_starts.get(first).copied().unwrap_or(0);
                    let at_end = event.visible_range.end >= event.count;
                    let _ = scroll_view.update(cx, |this, cx| {
                        this.record_markdown_progress(source_offset, source_bytes, at_end, cx)
                    });
                });
            let list_state = state.read(cx).list_state().clone();
            let scrollbar_cover_color = p.bg;
            content = content.child(
                div()
                    .id("markdown-column")
                    .test_support()
                    .relative()
                    .flex()
                    .size_full()
                    .min_h_0()
                    .justify_center()
                    .child(
                        div()
                            .id("markdown-measure")
                            .test_support()
                            .h_full()
                            .min_h_0()
                            .w_full()
                            .text_size(px(body))
                            .when(!full_width, |column| column.max_w(px(MARKDOWN_MAX_WIDTH)))
                            .child(
                                div()
                                    .id("markdown-document")
                                    .test_support()
                                    .size_full()
                                    .on_prepaint({
                                        let scroll_state = state.clone();
                                        let scroll_view = cx.entity().downgrade();
                                        move |_, _, cx| {
                                            let offset = -f32::from(
                                                scroll_state
                                                    .read(cx)
                                                    .list_state()
                                                    .scroll_px_offset_for_scrollbar()
                                                    .y,
                                            );
                                            let _ = scroll_view.update(cx, |this, cx| {
                                                this.record_markdown_scroll(offset, cx)
                                            });
                                        }
                                    })
                                    .child(
                                        div()
                                            .id("markdown-text-viewport")
                                            .test_support()
                                            .relative()
                                            .size_full()
                                            .min_h_0()
                                            .child(
                                                TextView::new(&state)
                                                    .scrollable(true)
                                                    // Padding keeps text clear of the
                                                    // overlay scrollbar. Vertical inset
                                                    // blocks scroll with the document.
                                                    .px(px(MARKDOWN_HORIZONTAL_PADDING))
                                                    .style(markdown_style::text_style(cx, body))
                                                    .markdown_extensions(extensions)
                                                    .image_source(move |uri| {
                                                        reflow_images::source(
                                                            image_view.clone(),
                                                            source.clone(),
                                                            uri.to_string(),
                                                        )
                                                    })
                                                    .on_link_click(move |href, _, _, cx| {
                                                        let _ = view.update(cx, |this, cx| {
                                                            this.follow_reflow_link(href.as_ref(), cx)
                                                        });
                                                    })
                                                    .code_block_actions(move |code, _, _cx| {
                                                        use std::hash::{Hash as _, Hasher as _};
                                                        let mut hasher = std::collections::hash_map::DefaultHasher::new();
                                                        code.code().hash(&mut hasher);
                                                        let text = code.code().to_string();
                                                        Button::new(("markdown-code-copy", hasher.finish()))
                                                            .ghost()
                                                            .small()
                                                            .icon(IconName::Copy)
                                                            .accessibility_label(tr!("Copy code"))
                                                            .tooltip(tr!("Copy code"))
                                                            .on_click(move |_, _, cx| {
                                                                cx.write_to_clipboard(
                                                                    gpui_kit::ClipboardItem::new_string(
                                                                        text.clone(),
                                                                    ),
                                                                );
                                                            })
                                                    }),
                                            )
                                            // Kit paints its overlay scrollbar at the
                                            // TextView edge. Cover that private track; the
                                            // visible, draggable scrollbar below sits on
                                            // the panel's outer edge.
                                            .child(
                                                div()
                                                    .id("markdown-scrollbar-cover")
                                                    .test_support()
                                                    .absolute()
                                                    .top_0()
                                                    .right_0()
                                                    .bottom_0()
                                                    .w(px(MARKDOWN_HORIZONTAL_PADDING))
                                                    .child(
                                                        canvas(
                                                            |_, _, _| (),
                                                            move |bounds, _, window, _| {
                                                                // TextView paints its scrollbar
                                                                // into an overlay layer, so the
                                                                // cover must use that layer too.
                                                                window.paint_layer(
                                                                    bounds,
                                                                    |window| {
                                                                        window.paint_quad(fill(
                                                                            bounds,
                                                                            scrollbar_cover_color,
                                                                        ));
                                                                    },
                                                                );
                                                            },
                                                        )
                                                        .size_full(),
                                                    ),
                                            ),
                                    ),
                            ),
                    )
                    .child(
                        div()
                            .id("markdown-scrollbar")
                            .test_support()
                            .absolute()
                            .top_0()
                            .right_0()
                            .bottom_0()
                            .w(px(MARKDOWN_HORIZONTAL_PADDING))
                            .child(
                                Scrollbar::vertical(&list_state)
                                    .id("markdown-scrollbar-control")
                                    .viewport_from_layout(),
                            ),
                    ),
            );
        }
        if self.loading || (self.reflow.frame.is_none() && self.reflow.markdown.is_none()) {
            content = content.child(div().p_4().text_color(p.muted).child(if self.loading {
                tr!("Reading…").into()
            } else {
                self.message.clone()
            }));
        }
        content
            .context_menu(move |menu, window, menu_cx| {
                context_menus::content(menu, menu_view.clone(), menu_state, window, menu_cx)
            })
            .into_any_element()
    }

    fn follow_reflow_link(&mut self, href: &str, cx: &mut Context<Self>) {
        if let Some(anchor) = href.strip_prefix('#') {
            if let Some(offset) = self.reflow.markdown.as_ref().and_then(|markdown| {
                markdown
                    .metadata
                    .headings
                    .iter()
                    .find(|heading| heading.anchor == anchor)
                    .map(|heading| heading.source.start)
            }) {
                self.jump_markdown_source(offset, cx);
            }
        } else if href.starts_with("https://")
            || href.starts_with("http://")
            || href.starts_with("mailto:")
        {
            cx.open_url(href);
        } else if self.books[self.active].format() == Some(DocumentFormat::Epub) {
            self.jump_reflow(href.to_owned(), 0, cx);
        } else if !href.contains(':')
            && let Source::File(path) = &self.books[self.active].source
            && let Some(parent) = path.parent()
        {
            self.open_path(parent.join(href.split('#').next().unwrap_or(href)), cx);
        }
    }
}
/// Formats an EPUB result without exposing the spine filename when navigation
/// has no title for that chapter.
fn reflow_search_result_text(title: &str, href: &str, snippet: &str) -> String {
    if title.is_empty() || title == href {
        snippet.to_owned()
    } else {
        format!("{title} · {snippet}")
    }
}

/// Fit a generated Mermaid SVG to the available reading width while preserving
/// its viewBox and visual stroke weight. Without the stroke rewrite, a wide SVG
/// is reduced below one device pixel per outline in places, so some rectangle
/// edges disappear while others survive.
fn fit_svg_to_width(svg: &str, target_width: f32) -> String {
    let Some((root_start, root_end)) = svg_root_bounds(svg) else {
        return svg.to_owned();
    };
    let root = &svg[root_start..root_end];
    let Some(width) = svg_length_attribute(root, "width") else {
        return svg.to_owned();
    };
    let Some(height) = svg_length_attribute(root, "height") else {
        return svg.to_owned();
    };
    if width <= 0. || height <= 0. || target_width <= 0. || width <= target_width {
        return svg.to_owned();
    }

    let factor = width / target_width;
    let fitted_height = height / factor;
    let Some(root) = replace_svg_attribute(root, "width", &svg_number(target_width)) else {
        return svg.to_owned();
    };
    let Some(root) = replace_svg_attribute(&root, "height", &svg_number(fitted_height)) else {
        return svg.to_owned();
    };
    let mut fitted = String::with_capacity(svg.len());
    fitted.push_str(&svg[..root_start]);
    fitted.push_str(&root);
    fitted.push_str(&svg[root_end..]);
    let fitted = scale_svg_numeric_attribute(fitted, "stroke-width", factor, false);
    scale_svg_numeric_attribute(fitted, "stroke-dasharray", factor, true)
}

fn svg_root_bounds(svg: &str) -> Option<(usize, usize)> {
    let start = svg.find("<svg")?;
    let end = start + svg[start..].find('>')? + 1;
    Some((start, end))
}

fn svg_length_attribute(tag: &str, name: &str) -> Option<f32> {
    let value = svg_attribute(tag, name)?;
    value
        .trim_end_matches("px")
        .parse::<f32>()
        .ok()
        .filter(|value| value.is_finite())
}

fn svg_attribute<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
    let needle = format!(" {name}=\"");
    let start = tag.find(&needle)? + needle.len();
    let end = start + tag[start..].find('"')?;
    Some(&tag[start..end])
}

fn replace_svg_attribute(tag: &str, name: &str, value: &str) -> Option<String> {
    let needle = format!(" {name}=\"");
    let value_start = tag.find(&needle)? + needle.len();
    let value_end = value_start + tag[value_start..].find('"')?;
    let mut replaced = String::with_capacity(tag.len() + value.len());
    replaced.push_str(&tag[..value_start]);
    replaced.push_str(value);
    replaced.push_str(&tag[value_end..]);
    Some(replaced)
}

fn svg_number(value: f32) -> String {
    let formatted = format!("{value:.3}");
    formatted
        .trim_end_matches('0')
        .trim_end_matches('.')
        .to_owned()
}

fn scale_svg_numeric_attribute(mut svg: String, name: &str, factor: f32, list: bool) -> String {
    let needle = format!(" {name}=\"");
    let mut search_from = 0;
    while let Some(relative) = svg[search_from..].find(&needle) {
        let value_start = search_from + relative + needle.len();
        let Some(relative_end) = svg[value_start..].find('"') else {
            break;
        };
        let value_end = value_start + relative_end;
        let value = &svg[value_start..value_end];
        let scaled = if list {
            let numbers = value
                .split([' ', '\t', ','])
                .filter(|number| !number.is_empty())
                .map(|number| {
                    number
                        .parse::<f32>()
                        .ok()
                        .filter(|number| number.is_finite())
                        .map(|number| svg_number(number * factor))
                })
                .collect::<Option<Vec<_>>>();
            numbers.map(|numbers| numbers.join(" "))
        } else {
            value
                .parse::<f32>()
                .ok()
                .filter(|number| number.is_finite())
                .map(|number| svg_number(number * factor))
        };
        if let Some(scaled) = scaled {
            svg.replace_range(value_start..value_end, &scaled);
            search_from = value_start + scaled.len();
        } else {
            search_from = value_end + 1;
        }
    }
    svg
}

fn strip_mermaid_background(svg: &str) -> String {
    let Some(start) = svg.find("<rect x=\"0\" y=\"0\"") else {
        return svg.to_owned();
    };
    let Some(end) = svg[start..].find('>').map(|offset| start + offset + 1) else {
        return svg.to_owned();
    };
    if svg[start..end].contains("fill=\"#FFFFFF\"") {
        let mut out = String::with_capacity(svg.len() - (end - start));
        out.push_str(&svg[..start]);
        out.push_str(&svg[end..]);
        out
    } else {
        svg.to_owned()
    }
}

fn search_markdown_text(text: &str, query: &str) -> Vec<ReflowSearchHit> {
    let needle = query.to_lowercase();
    if needle.is_empty() {
        return Vec::new();
    }
    let folded = text.to_lowercase();
    let ranges = if folded.len() == text.len() {
        folded
            .match_indices(&needle)
            .map(|(offset, matched)| offset..offset + matched.len())
            .collect::<Vec<_>>()
    } else {
        text.match_indices(query)
            .map(|(offset, matched)| offset..offset + matched.len())
            .collect::<Vec<_>>()
    };
    ranges
        .into_iter()
        .filter(|range| text.is_char_boundary(range.start) && text.is_char_boundary(range.end))
        .take(300)
        .map(|range| {
            let line_start = text[..range.start]
                .rfind(['\n', '\r'])
                .map_or(0, |offset| offset + 1);
            let line_end = text[range.end..]
                .find(['\n', '\r'])
                .map_or(text.len(), |offset| range.end + offset);
            let mut snippet = text[line_start..line_end]
                .trim()
                .chars()
                .take(160)
                .collect::<String>();
            if line_start > 0 {
                snippet.insert(0, '…');
            }
            if line_end < text.len() {
                snippet.push('…');
            }
            ReflowSearchHit {
                rendered: range,
                href: String::new(),
                text: snippet,
            }
        })
        .collect()
}

fn mermaid_source(language: &str, source: &str) -> String {
    match language.to_ascii_lowercase().as_str() {
        "flow" => legacy_flow_to_mermaid(source),
        "seq" => legacy_sequence_to_mermaid(source),
        _ => source.trim().to_owned(),
    }
}

fn legacy_flow_to_mermaid(source: &str) -> String {
    let mut nodes = Vec::new();
    let mut edges = Vec::new();
    for raw in source.lines() {
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }
        let Some((left, right)) = line.split_once("=>") else {
            if let Some((from, to)) = line.split_once("->") {
                edges.push(format!("  {} --> {}", from.trim(), to.trim()));
            }
            continue;
        };
        let id = left.trim();
        let Some((kind, label)) = right.split_once(':') else {
            continue;
        };
        let label = label.trim();
        let node = match kind.trim().to_ascii_lowercase().as_str() {
            "start" | "end" => format!("  {id}([{label}])"),
            "condition" => format!("  {id}{{{label}}}"),
            "inputoutput" => format!("  {id}[/{label}/]"),
            _ => format!("  {id}[{label}]"),
        };
        nodes.push(node);
    }
    std::iter::once("flowchart TD".to_owned())
        .chain(nodes)
        .chain(edges)
        .collect::<Vec<_>>()
        .join("\n")
}

fn legacy_sequence_to_mermaid(source: &str) -> String {
    let mut lines = vec!["sequenceDiagram".to_owned()];
    for raw in source.lines() {
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }
        if line.to_ascii_lowercase().starts_with("note ") {
            lines.push(line.to_owned());
            continue;
        }
        if let Some((participants, message)) = line.split_once(':') {
            let (from, arrow, to) = ["->>", "-->", "->", "--"]
                .iter()
                .find_map(|arrow| {
                    participants
                        .split_once(arrow)
                        .map(|(from, to)| (from.trim(), *arrow, to.trim()))
                })
                .unwrap_or(("", "", ""));
            if !from.is_empty() {
                let arrow = match arrow {
                    "->" => "->>",
                    "-->" => "-->>",
                    "--" => "-->>",
                    _ => arrow,
                };
                lines.push(format!("  {from}{arrow}{to}: {}", message.trim()));
                continue;
            }
        }
        lines.push(line.to_owned());
    }
    lines.join("\n")
}

/// Distance from a point to page bounds; zero inside them.
fn page_bounds_distance(
    bounds: Bounds<gpui_kit::Pixels>,
    position: gpui_kit::Point<gpui_kit::Pixels>,
) -> f32 {
    let dx = f32::from(
        (bounds.left() - position.x)
            .max(position.x - bounds.right())
            .max(px(0.)),
    );
    let dy = f32::from(
        (bounds.top() - position.y)
            .max(position.y - bounds.bottom())
            .max(px(0.)),
    );
    dx * dx + dy * dy
}

/// The image rectangle inside a laid-out page cell when `Contain` scales it.
fn contained_page_bounds(
    bounds: Bounds<gpui_kit::Pixels>,
    raster: (f32, f32),
) -> Bounds<gpui_kit::Pixels> {
    let scale = (f32::from(bounds.size.width) / raster.0)
        .min(f32::from(bounds.size.height) / raster.1)
        .max(0.);
    let width = raster.0 * scale;
    let height = raster.1 * scale;
    Bounds {
        origin: point(
            bounds.origin.x + px((f32::from(bounds.size.width) - width) / 2.),
            bounds.origin.y + px((f32::from(bounds.size.height) - height) / 2.),
        ),
        size: size(px(width), px(height)),
    }
}

/// Whether the row owns nested rows, the same test the PDF outline uses.
fn outline_has_children(outline: &[OutlineEntry], index: usize) -> bool {
    outline
        .get(index)
        .zip(outline.get(index + 1))
        .is_some_and(|(item, next)| next.depth > item.depth)
}

/// Outline rows to show, honouring folds and an optional title search. A row is
/// kept when it matches or when an ancestor matches, exactly like the PDF
/// outline filter.
fn visible_outline(
    outline: &[OutlineEntry],
    collapsed: &HashSet<usize>,
    query: &str,
) -> Vec<usize> {
    let visible = visible_outline_rows(outline, collapsed);
    if query.is_empty() {
        return visible;
    }
    let mut ancestors: Vec<usize> = Vec::new();
    let mut parents = Vec::with_capacity(outline.len());
    let mut included = Vec::with_capacity(outline.len());
    for (index, item) in outline.iter().enumerate() {
        while ancestors
            .last()
            .is_some_and(|&parent| outline[parent].depth >= item.depth)
        {
            ancestors.pop();
        }
        parents.push(ancestors.last().copied());
        included.push(item.title.to_lowercase().contains(query));
        ancestors.push(index);
    }
    for index in (0..outline.len()).rev() {
        if included[index]
            && let Some(parent) = parents[index]
        {
            included[parent] = true;
        }
    }
    visible
        .into_iter()
        .filter(|&index| included[index])
        .collect()
}

fn visible_outline_rows(outline: &[OutlineEntry], collapsed: &HashSet<usize>) -> Vec<usize> {
    let mut hidden_below = None;
    outline
        .iter()
        .enumerate()
        .filter_map(|(index, item)| {
            if hidden_below.is_some_and(|depth| item.depth > depth) {
                return None;
            }
            hidden_below = collapsed.contains(&index).then_some(item.depth);
            Some(index)
        })
        .collect()
}

/// Group a character count so a long document stays readable at a glance.
fn grouped_characters(count: usize) -> String {
    let digits = count.to_string();
    let mut grouped = String::with_capacity(digits.len() + digits.len() / 3);
    for (index, digit) in digits.char_indices() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            grouped.push(',');
        }
        grouped.push(digit);
    }
    grouped
}

/// A hidden active row belongs to the last visible ancestor preceding it.
fn visible_active_outline(visible: &[usize], active: Option<usize>) -> Option<usize> {
    active.and_then(|active| {
        visible
            .iter()
            .rev()
            .find(|&&index| index <= active)
            .copied()
    })
}

#[cfg(test)]
mod tests {
    use super::{
        MARKDOWN_TOP_INSET_MARKER, ReflowTextSelection, contained_page_bounds, fit_svg_to_width,
        grouped_characters, markdown_has_top_inset, markdown_render_source,
        reflow_search_result_text, rendered_source_block, scale_svg_numeric_attribute,
        strip_mermaid_background, svg_attribute, svg_length_attribute, svg_root_bounds,
    };
    use crate::{Mode, Source, texture};
    use breeze_reflow::epub::{TextGlyph, TextLayer, TextLine};
    use gpui_kit::AppContext;
    use gpui_kit::TestAppContext;
    use gpui_kit::test::TestWindowExt;
    use gpui_kit::{Bounds, point, px, size};
    use std::sync::Arc;

    /// A two-line text layer with one glyph per character, laid out left to
    /// right at 10 px per glyph.
    fn text_layer(lines: &[&str]) -> TextLayer {
        let mut layer = TextLayer::default();
        for (row, text) in lines.iter().enumerate() {
            let start = layer.glyphs.len();
            for (column, character) in text.chars().enumerate() {
                let x = column as f32 / 10.;
                let y = row as f32 / 2.;
                layer.glyphs.push(TextGlyph {
                    character,
                    bounds: [x, y, x + 0.1, y + 0.1],
                });
            }
            layer.lines.push(TextLine {
                glyphs: start..layer.glyphs.len(),
                bounds: [
                    0.,
                    row as f32 / 2.,
                    text.chars().count() as f32 / 10.,
                    row as f32 / 2. + 0.1,
                ],
            });
        }
        layer
    }

    #[test]
    fn mermaid_root_background_is_removed_without_removing_nodes() {
        let svg = r##"<svg width="10" height="10"><rect x="0" y="0" width="10" height="10" fill="#FFFFFF"/><rect x="2" y="2" width="6" height="6" fill="#F8FAFC"/></svg>"##;
        let stripped = strip_mermaid_background(svg);
        assert!(!stripped.contains("fill=\"#FFFFFF\""));
        assert!(stripped.contains("fill=\"#F8FAFC\""));
    }

    #[test]
    fn yaml_frontmatter_keeps_the_body_adjacent_and_maps_directly() {
        let source = "---\ntitle: 格式测试\n---\n\n# 标题\n";
        let frontmatter_end = source.find("\n\n#").unwrap();
        let (rendered, map) = markdown_render_source(source, Some(frontmatter_end));

        assert!(rendered.starts_with(&source[..frontmatter_end]));
        assert!(
            !rendered.starts_with(MARKDOWN_TOP_INSET_MARKER),
            "a top-inset block would separate YAML front matter from the body"
        );
        assert!(!rendered[..frontmatter_end + 3].contains(MARKDOWN_TOP_INSET_MARKER));
        assert_eq!(
            map.source_offset(frontmatter_end + 2),
            rendered.find("# 标题").unwrap()
        );
        assert_eq!(map.source_range(0..source.len()), 0..source.len());
        assert!(!markdown_has_top_inset(source, Some(frontmatter_end)));
        assert_eq!(
            rendered_source_block(0, 2, markdown_has_top_inset(source, Some(frontmatter_end))),
            0
        );
    }

    #[test]
    fn toml_frontmatter_keeps_the_inset_after_its_block() {
        let source = "+++\ntitle = \"Test\"\n+++\n\n# Heading\n";
        let frontmatter_end = source.find("\n\n#").unwrap();
        let (rendered, map) = markdown_render_source(source, Some(frontmatter_end));

        assert!(rendered.starts_with(&source[..frontmatter_end]));
        assert!(rendered[frontmatter_end..].contains(MARKDOWN_TOP_INSET_MARKER));
        assert!(markdown_has_top_inset(source, Some(frontmatter_end)));
        assert_eq!(rendered_source_block(0, 2, true), 0);
        assert_eq!(rendered_source_block(1, 2, true), 0);
        assert_eq!(
            map.source_offset(frontmatter_end + 2),
            rendered.find("# Heading").unwrap()
        );
    }

    #[test]
    fn markdown_without_frontmatter_keeps_its_leading_inset() {
        let source = "# 标题\n";
        let (rendered, map) = markdown_render_source(source, None);

        assert!(rendered.starts_with(MARKDOWN_TOP_INSET_MARKER));
        assert_eq!(map.source_offset(0), MARKDOWN_TOP_INSET_MARKER.len() + 2);
        assert_eq!(
            &rendered[map.source_offset(0)..map.source_offset(source.len())],
            source
        );
    }

    #[test]
    fn wide_mermaid_svg_preserves_stroke_weights_when_fitted() {
        let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" width="2050" height="587" viewBox="-50 -10 2050 587"><rect x="0" y="0" width="150" height="65" stroke-width="1.0"/><path stroke-width="1.5" stroke-dasharray="3 3"/></svg>"##;
        let fitted = fit_svg_to_width(svg, 512.);
        let (start, end) = svg_root_bounds(&fitted).unwrap();
        let root = &fitted[start..end];
        assert!((svg_length_attribute(root, "width").unwrap() - 512.).abs() < 0.001);
        let fitted_height = svg_length_attribute(root, "height").unwrap();
        assert!(
            (fitted_height - 146.586).abs() < 0.1,
            "height is {fitted_height}"
        );

        let rect = &fitted[fitted.find("<rect").unwrap()..fitted.find("/>").unwrap()];
        let stroke = svg_attribute(rect, "stroke-width").unwrap();
        assert!((stroke.parse::<f32>().unwrap() - 4.004).abs() < 0.01);
        assert!(fitted.contains("stroke-dasharray=\"12.012 12.012\""));
        assert_eq!(fit_svg_to_width(svg, 2050.), svg);
    }

    #[test]
    fn svg_attribute_scaling_leaves_unrelated_numbers_alone() {
        let svg = r#"<svg width="20"><rect x="10" stroke-width="1.5"/></svg>"#.to_owned();
        let scaled = scale_svg_numeric_attribute(svg, "stroke-width", 2., false);
        assert!(scaled.contains("x=\"10\""));
        assert!(scaled.contains("stroke-width=\"3\""));
    }

    #[test]
    fn mermaid_diagrams_rasterize() {
        let svg = mermaid_rs_renderer::render("flowchart LR; A-->B").expect("a flowchart renders");
        assert!(svg.contains("<svg"), "{svg}");
        let image =
            crate::reflow_images::svg_image(svg.as_bytes(), 2.0).expect("the SVG rasterizes");
        assert!(usize::from(image.size(0).width) > 0);
    }

    #[test]
    fn block_formulas_typeset() {
        let rendered =
            ratex_gpui::render::render_latex("E = mc^2", 18., 2., gpui_kit::hsla(0., 0., 0., 1.))
                .expect("a simple formula typesets");
        assert!(rendered.width > 0. && rendered.height > 0.);
    }

    #[test]
    fn epub_selection_maps_carets_across_lines() {
        let text = text_layer(&["hello", "world"]);
        let first = text.caret([0.55, 0.05]).unwrap();
        let last = text.caret([0.45, 0.55]).unwrap();
        assert_eq!(first, 5, "a point past the line end lands at its caret end");
        assert_eq!(
            last, 10,
            "a point past the last glyph lands at the line end"
        );
        assert_eq!(text.text(first..last), "world");
        let rects = text.rects(first..last);
        assert_eq!(rects.len(), 1);
        assert!(rects[0][2] > rects[0][0]);
    }

    #[test]
    fn epub_selection_spans_visible_pages() {
        let selection = ReflowTextSelection {
            anchor: (0, 3),
            focus: (1, 2),
        };
        assert_eq!(selection.on_page(0, 5), 3..5);
        assert_eq!(selection.on_page(1, 5), 0..2);
        assert_eq!(selection.on_page(2, 5), 0..0);
    }

    #[test]
    fn contained_image_bounds_centre_the_page() {
        let bounds = Bounds {
            origin: point(px(10.), px(20.)),
            size: size(px(400.), px(400.)),
        };
        let display = contained_page_bounds(bounds, (200., 100.));
        assert_eq!(display.origin.x, px(10.));
        assert_eq!(display.origin.y, px(120.));
        assert_eq!(display.size.width, px(400.));
        assert_eq!(display.size.height, px(200.));
    }

    #[test]
    fn search_result_hides_spine_filenames() {
        assert_eq!(
            reflow_search_result_text("index_split_004.html", "index_split_004.html", "…命中…"),
            "…命中…"
        );
        assert_eq!(
            reflow_search_result_text("鸣谢", "index_split_004.html", "…命中…"),
            "鸣谢 · …命中…"
        );
    }

    #[gpui_kit::test]
    fn epub_pointer_drag_selects_text_and_paints_rects(cx: &mut TestAppContext) {
        use breeze_reflow::epub::{self, TextGlyph, TextLine};

        fn layer(lines: &[(&str, f32)]) -> epub::TextLayer {
            let mut layer = epub::TextLayer::default();
            for (text, y) in lines {
                let start = layer.glyphs.len();
                for (column, character) in text.chars().enumerate() {
                    let x = 0.1 + column as f32 * 0.06;
                    layer.glyphs.push(TextGlyph {
                        character,
                        bounds: [x, *y, x + 0.05, *y + 0.05],
                    });
                }
                layer.lines.push(TextLine {
                    glyphs: start..layer.glyphs.len(),
                    bounds: [0.1, *y, 0.1 + text.chars().count() as f32 * 0.06, *y + 0.05],
                });
            }
            layer
        }

        let (handle, view) = crate::ui_tests::app(cx);
        let image = texture(breeze_pdf::Pixels {
            width: 10,
            height: 10,
            bgra: vec![255; 10 * 10 * 4],
        })
        .unwrap();
        cx.update_window(handle, |_, window, cx| {
            view.update(cx, |view, cx| {
                view.books[0].source = Source::File("/tmp/select.epub".into());
                view.books[0].state.mode = Mode::Read;
                view.reflow.frame = Some(epub::Frame {
                    metadata: Arc::new(epub::Metadata {
                        title: "Selectable".into(),
                        author: String::new(),
                        chapters: vec![epub::Chapter {
                            href: "select.xhtml".into(),
                            title: "Select".into(),
                        }],
                        outline: Vec::new(),
                        fingerprint: "test".into(),
                    }),
                    location: Default::default(),
                    chapter: 0,
                    chapter_page: 0,
                    chapter_pages: 1,
                    columns: 1,
                    width: 100,
                    height: 100,
                    pages: Vec::new(),
                    text_pages: vec![layer(&[("hello", 0.2), ("world", 0.4)])],
                    text: String::new(),
                    progress: 0.,
                    links: Vec::new(),
                    scroll_offset: 0.,
                    scroll_extent: 0.,
                });
                view.reflow.page_images = vec![image.clone()];
                view.reflow.page_sizes = vec![(100., 100.)];
                view.loading = false;
                cx.notify();
            });
            window.render_frame(cx);
        })
        .unwrap();
        let bounds = cx
            .update_window(handle, |_, window, _| {
                window.find(("reflow-page", 0usize)).bounds()
            })
            .unwrap();
        let start = point(
            bounds.origin.x + bounds.size.width * 0.2,
            bounds.origin.y + bounds.size.height * 0.25,
        );
        let end = point(
            bounds.origin.x + bounds.size.width * 0.8,
            bounds.origin.y + bounds.size.height * 0.45,
        );
        cx.update_window(handle, |_, window, cx| {
            window.drag(start, end, cx);
        })
        .unwrap();
        cx.update_window(handle, |_, _, cx| {
            let view = view.read(cx);
            assert!(!view.selected_text.is_empty(), "drag did not select text");
            assert!(
                view.reflow
                    .selection
                    .iter()
                    .any(|(_, rects)| !rects.is_empty()),
                "selection did not paint any rectangles"
            );
        })
        .unwrap();
    }

    #[test]
    fn character_counts_group_by_thousands() {
        assert_eq!(grouped_characters(0), "0");
        assert_eq!(grouped_characters(22), "22");
        assert_eq!(grouped_characters(999), "999");
        assert_eq!(grouped_characters(1_000), "1,000");
        assert_eq!(grouped_characters(47_025), "47,025");
        assert_eq!(grouped_characters(1_234_567), "1,234,567");
    }
}
