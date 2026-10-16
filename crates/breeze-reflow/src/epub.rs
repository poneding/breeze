//! EPUB package navigation and bounded, chapter-local MuPDF rendering.
mod layout;

use crate::{fingerprint, read_bounded};
use anyhow::{Context, Result, bail, ensure};
use breeze_core::document::EpubLocation;
use std::{
    io::Cursor,
    path::{Component, Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

pub const MAX_ARCHIVE_BYTES: usize = 128 * 1024 * 1024;
pub(super) const MAX_RESOURCE_BYTES: u64 = 16 * 1024 * 1024;
const MAX_UNCOMPRESSED_BYTES: u64 = 256 * 1024 * 1024;
pub(super) const MAX_PIXELS: u64 = 12_000_000;

#[derive(Clone, Debug)]
pub struct Chapter {
    pub href: String,
    pub title: String,
}

#[derive(Clone, Debug)]
pub struct Outline {
    pub title: String,
    pub href: String,
    pub depth: usize,
}

#[derive(Clone, Debug)]
pub struct Metadata {
    pub title: String,
    pub author: String,
    pub chapters: Vec<Chapter>,
    pub outline: Vec<Outline>,
    pub fingerprint: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Request {
    pub path: PathBuf,
    pub location: EpubLocation,
    pub width: f32,
    pub height: f32,
    pub font_size: f32,
    pub scale: f32,
    pub turn: i8,
    pub scroll: f32,
    pub find: Option<String>,
    /// Two consecutive pages side by side. The chapter is paginated at half the
    /// viewport width, and a turn then advances a spread rather than a page.
    pub spread: bool,
}

#[derive(Clone)]
pub struct Link {
    pub bounds: [f32; 4],
    pub href: String,
}

#[derive(Clone)]
pub struct PagePixels {
    pub width: u32,
    pub height: u32,
    pub bgra: Vec<u8>,
}

#[derive(Clone)]
pub struct Frame {
    pub metadata: Arc<Metadata>,
    pub location: EpubLocation,
    pub chapter: usize,
    pub chapter_page: usize,
    pub chapter_pages: usize,
    /// Pages the frame shows side by side: 1 for single-page reading, 2 for a
    /// spread. `chapter_page` is the left page of the pair.
    pub columns: usize,
    pub width: u32,
    pub height: u32,
    /// One opaque BGRA raster per visible page. Keeping the pages separate lets
    /// the native UI lay out a spread explicitly instead of scaling a combined
    /// image as one page.
    pub pages: Vec<PagePixels>,
    pub text: String,
    pub progress: f32,
    pub links: Vec<Link>,
    pub scroll_offset: f32,
    pub scroll_extent: f32,
}

pub struct Book {
    pub(super) bytes: Arc<[u8]>,
    pub(super) publication: rbook::Epub,
    pub metadata: Arc<Metadata>,
    layout: Option<layout::ChapterLayout>,
}

fn validate_archive(bytes: &[u8]) -> Result<()> {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).context("Invalid EPUB archive")?;
    ensure!(archive.len() <= 20_000, "EPUB has too many archive entries");
    let mut total = 0u64;
    let mut names = std::collections::HashSet::new();
    for index in 0..archive.len() {
        let (name, size, encrypted) = {
            let entry = archive
                .by_index(index)
                .context("Encrypted or invalid EPUB entry")?;
            (entry.name()?.into_owned(), entry.size(), entry.encrypted())
        };
        ensure!(!encrypted, "Encrypted EPUB entries are not supported");
        ensure!(
            names.insert(name.to_owned()),
            "EPUB contains duplicate resource names"
        );
        let path = Path::new(&name);
        ensure!(
            !name.contains('\\')
                && !path.components().any(|part| matches!(
                    part,
                    Component::ParentDir | Component::RootDir | Component::Prefix(_)
                )),
            "Unsafe EPUB resource path"
        );
        ensure!(
            size <= MAX_RESOURCE_BYTES,
            "EPUB resource exceeds the 16 MiB limit"
        );
        total = total
            .checked_add(size)
            .context("EPUB resource size overflow")?;
        ensure!(
            total <= MAX_UNCOMPRESSED_BYTES,
            "EPUB exceeds the 256 MiB expanded-resource limit"
        );
        if name == "META-INF/encryption.xml" {
            bail!("Encrypted EPUB publications and obfuscated fonts are not supported");
        }
    }
    Ok(())
}

fn package(bytes: Arc<[u8]>) -> Result<Metadata> {
    validate_archive(&bytes)?;
    let fingerprint = fingerprint(&bytes);
    let epub = rbook::Epub::read(Cursor::new(bytes)).context("Cannot parse EPUB package")?;
    ensure!(
        !epub
            .metadata()
            .by_property("rendition:layout")
            .any(|entry| entry.value() == "pre-paginated"),
        "Fixed-layout EPUB is not supported by this reflow reader"
    );
    let title = epub
        .metadata()
        .title()
        .map(|value| value.value().to_owned())
        .unwrap_or_default();
    let author = epub
        .metadata()
        .creators()
        .map(|value| value.value().to_owned())
        .collect::<Vec<_>>()
        .join(", ");
    ensure!(
        title.len() <= 4096 && author.len() <= 8192,
        "EPUB metadata exceeds its text limit"
    );
    let mut chapters = Vec::new();
    for item in epub.spine().iter() {
        let entry = item
            .manifest_entry()
            .context("EPUB spine resource is missing")?;
        ensure!(
            !item
                .properties()
                .has_property("rendition:layout-pre-paginated"),
            "Fixed-layout EPUB spine items are not supported"
        );
        ensure!(
            matches!(entry.media_type(), "application/xhtml+xml" | "text/html"),
            "Unsupported EPUB spine media type: {}",
            entry.media_type()
        );
        let href = entry.href().decode().trim_start_matches('/').to_owned();
        chapters.push(Chapter {
            title: href.clone(),
            href,
        });
    }
    ensure!(
        !chapters.is_empty() && chapters.len() <= 4096,
        "EPUB must contain 1–4096 chapters"
    );
    let mut outline = Vec::new();
    if let Some(contents) = epub.toc().contents() {
        for item in contents.flatten() {
            if item.is_root() {
                continue;
            }
            ensure!(
                item.depth() <= 64 && outline.len() < 20_000,
                "EPUB navigation exceeds its limits"
            );
            if let Some(href) = item.href() {
                let href = href.to_string().trim_start_matches('/').to_owned();
                if let Some(chapter) = chapters
                    .iter_mut()
                    .find(|entry| entry.href == href.split('#').next().unwrap_or(&href))
                    && chapter.title == chapter.href
                {
                    chapter.title = item.label().to_owned();
                }
                outline.push(Outline {
                    title: item.label().to_owned(),
                    href,
                    depth: item.depth().saturating_sub(1),
                });
            }
        }
    }
    if outline.is_empty() {
        outline = chapters
            .iter()
            .map(|entry| Outline {
                title: entry.title.clone(),
                href: entry.href.clone(),
                depth: 0,
            })
            .collect();
    }
    Ok(Metadata {
        title,
        author,
        chapters,
        outline,
        fingerprint,
    })
}

impl Book {
    pub fn open(path: &Path) -> Result<Self> {
        let bytes: Arc<[u8]> = read_bounded(path, MAX_ARCHIVE_BYTES)?.into();
        let metadata = Arc::new(package(bytes.clone())?);
        let publication = rbook::Epub::read(Cursor::new(bytes.clone()))?;
        Ok(Self {
            bytes,
            publication,
            metadata,
            layout: None,
        })
    }

    pub fn render(&mut self, request: &Request) -> Result<Frame> {
        layout::render(self, request)
    }
}

pub struct SearchHit {
    pub href: String,
    pub title: String,
    pub snippet: String,
}

const SEARCH_SNIPPET_CHARS: usize = 160;
const SEARCH_SNIPPET_LEAD_CHARS: usize = 48;

/// Builds a single-line result preview around the first match.
///
/// Chapter markup often leaves layout newlines and indentation at the start of
/// extracted text. Keeping them made a result button paint its first empty line,
/// so the panel showed a row with no visible text.
fn search_snippet(text: &str, query: &str) -> String {
    let normalized = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if normalized.is_empty() {
        return normalized;
    }
    let lowercase = normalized.to_lowercase();
    let needle = query.to_lowercase();
    let Some(byte_offset) = lowercase.find(&needle) else {
        return normalized.chars().take(SEARCH_SNIPPET_CHARS).collect();
    };
    let match_start = source_char_index(&normalized, byte_offset);
    let match_len = needle.chars().count().max(1);
    let characters = normalized.chars().collect::<Vec<_>>();
    let limit = SEARCH_SNIPPET_CHARS.max(match_len);
    let mut start = match_start.saturating_sub(SEARCH_SNIPPET_LEAD_CHARS);
    let mut end = (start + limit).min(characters.len());
    if end < match_start + match_len {
        end = (match_start + match_len).min(characters.len());
        start = end.saturating_sub(limit);
    }
    let mut snippet = String::new();
    if start > 0 {
        snippet.push('…');
    }
    snippet.extend(characters[start..end].iter());
    if end < characters.len() {
        snippet.push('…');
    }
    snippet
}

/// Maps a byte offset in a lowercase copy back to the source character index.
///
/// Lowercasing can expand one character into several (`İ`), so byte offsets are
/// accumulated rather than assuming a one-to-one mapping.
fn source_char_index(text: &str, lowercase_byte_offset: usize) -> usize {
    let mut lowercase_bytes = 0;
    for (index, character) in text.chars().enumerate() {
        let next = lowercase_bytes + character.to_lowercase().map(char::len_utf8).sum::<usize>();
        if next > lowercase_byte_offset {
            return index;
        }
        lowercase_bytes = next;
    }
    text.chars().count()
}

pub fn search(path: &Path, query: &str, cancel: &AtomicBool) -> Result<(String, Vec<SearchHit>)> {
    ensure!(
        !query.trim().is_empty() && query.len() <= 256,
        "Enter a search term of 1–256 bytes"
    );
    let bytes: Arc<[u8]> = read_bounded(path, MAX_ARCHIVE_BYTES)?.into();
    let metadata = package(bytes.clone())?;
    let publication = rbook::Epub::read(Cursor::new(bytes))?;
    let needle = query.to_lowercase();
    let mut hits = Vec::new();
    for (index, item) in publication.spine().iter().enumerate() {
        ensure!(!cancel.load(Ordering::Relaxed), "Search cancelled");
        let entry = item
            .manifest_entry()
            .context("EPUB spine resource is missing")?;
        let text = xhtml_text(&entry.read_str()?)?;
        if text.to_lowercase().contains(&needle) {
            hits.push(SearchHit {
                href: metadata.chapters[index].href.clone(),
                title: metadata.chapters[index].title.clone(),
                snippet: search_snippet(&text, &needle),
            });
            if hits.len() == 300 {
                break;
            }
        }
    }
    Ok((metadata.fingerprint, hits))
}

pub(crate) fn xhtml_text(source: &str) -> Result<String> {
    use quick_xml::events::Event;
    let mut reader = quick_xml::Reader::from_str(source);
    let mut text = String::new();
    let mut hidden = 0usize;
    loop {
        match reader.read_event()? {
            Event::Start(tag)
                if matches!(tag.local_name().as_ref(), "head" | "script" | "style") =>
            {
                hidden += 1
            }
            Event::End(tag) if matches!(tag.local_name().as_ref(), "head" | "script" | "style") => {
                hidden = hidden.saturating_sub(1)
            }
            Event::Text(value) if hidden == 0 => text.push_str(&value.xml10_content()),
            Event::GeneralRef(value) if hidden == 0 => {
                if let Some(character) = value.resolve_char_ref()? {
                    text.push(character);
                } else if let Some(entity) = quick_xml::escape::resolve_xml_entity(&value) {
                    text.push_str(entity);
                }
            }
            Event::End(_) if hidden == 0 => text.push(' '),
            Event::Eof => break,
            _ => {}
        }
        ensure!(
            text.len() <= MAX_RESOURCE_BYTES as usize,
            "EPUB chapter text exceeds its limit"
        );
    }
    Ok(text)
}

#[cfg(test)]
mod tests {
    use super::{search_snippet, source_char_index};

    #[test]
    fn search_snippet_removes_leading_layout_whitespace() {
        let snippet = search_snippet(
            "\n\n  \n  \n简要内容\n1 ■ 了解大型语言模型\n4.2 使用层归一化归一化激活函数",
            "层归一化",
        );
        assert!(!snippet.starts_with(char::is_whitespace), "{snippet:?}");
        assert!(snippet.contains("层归一化"), "{snippet:?}");
    }

    #[test]
    fn lowercase_offsets_map_back_across_expanding_characters() {
        assert_eq!(source_char_index("İx", "i̇".len()), 1);
    }
}
