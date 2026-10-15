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
}

#[derive(Clone)]
pub struct Link {
    pub bounds: [f32; 4],
    pub href: String,
}

#[derive(Clone)]
pub struct Frame {
    pub metadata: Arc<Metadata>,
    pub location: EpubLocation,
    pub chapter: usize,
    pub chapter_page: usize,
    pub chapter_pages: usize,
    pub width: u32,
    pub height: u32,
    pub bgra: Arc<Vec<u8>>,
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
                snippet: text.chars().take(160).collect(),
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
