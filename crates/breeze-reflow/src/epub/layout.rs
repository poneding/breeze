use super::*;
use anyhow::{Context, Result, ensure};
use mupdf::{Colorspace, Document, Matrix, TextPageFlags};
use quick_xml::{Reader as XmlReader, XmlVersion, events::Event};
use std::{
    collections::{HashMap, HashSet, VecDeque},
    io::{Cursor, Read, Write},
};
use url::Url;
use zip::{ZipArchive, ZipWriter, write::SimpleFileOptions};

pub(super) struct ChapterLayout {
    pub chapter: usize,
    pub width: f32,
    pub height: f32,
    pub font_size: f32,
    pub document: Document,
    pub pages: usize,
}
type Resource = (String, String, Vec<u8>);

fn local_references(source: &str, base: &Url) -> Result<Vec<Url>> {
    let mut reader = XmlReader::from_str(source);
    let mut references = Vec::new();
    loop {
        match reader.read_event()? {
            Event::Start(element) | Event::Empty(element) => {
                for attribute in element.attributes().with_checks(true) {
                    let attribute = attribute?;
                    let name = attribute.key.local_name();
                    if name.as_ref() != "href" && name.as_ref() != "src" {
                        continue;
                    }
                    let value = attribute.normalized_value(XmlVersion::Implicit1_0)?;
                    if value.is_empty() || value.starts_with('#') || value.starts_with("data:") {
                        continue;
                    }
                    if let Ok(url) = base.join(&value)
                        && url.host_str() == Some("breeze.invalid")
                    {
                        references.push(url);
                    }
                }
            }
            Event::Eof => break,
            _ => {}
        }
        ensure!(
            references.len() <= 4096,
            "EPUB chapter references too many resources"
        );
    }
    Ok(references)
}

fn css_references(source: &str, base: &Url) -> Result<Vec<Url>> {
    let lower = source.to_ascii_lowercase();
    let mut cursor = 0;
    let mut references = Vec::new();
    while let Some(offset) = lower[cursor..].find("url(") {
        let start = cursor + offset + 4;
        let Some(end) = source[start..].find(')') else {
            break;
        };
        let end = start + end;
        let value = source[start..end].trim().trim_matches(['\'', '"']);
        if !value.is_empty()
            && !value.starts_with("data:")
            && let Ok(url) = base.join(value)
            && url.host_str() == Some("breeze.invalid")
        {
            references.push(url);
        }
        cursor = end + 1;
        ensure!(
            references.len() <= 4096,
            "EPUB stylesheet references too many resources"
        );
    }
    Ok(references)
}

fn referenced_resources(
    book: &Book,
    chapter: usize,
    chapter_url: &Url,
    source: &str,
) -> Result<(Vec<Resource>, usize)> {
    let manifest: HashMap<String, String> = book
        .publication
        .manifest()
        .iter()
        .map(|entry| {
            (
                entry.href().decode().into_owned(),
                entry.media_type().to_owned(),
            )
        })
        .collect();
    let mut archive = ZipArchive::new(Cursor::new(book.bytes.clone()))?;
    let mut pending: VecDeque<Url> = local_references(source, chapter_url)?.into();
    let mut visited = HashSet::new();
    let mut resources = Vec::new();
    let mut resource_bytes = 0usize;
    while let Some(url) = pending.pop_front() {
        if !visited.insert(url.path().to_owned()) {
            continue;
        }
        let Some(media_type) = manifest.get(url.path()) else {
            continue;
        };
        if media_type == "application/xhtml+xml" || media_type == "text/html" {
            continue;
        }
        let decoded = percent_encoding::percent_decode_str(url.path().trim_start_matches('/'))
            .decode_utf8()?;
        let entry = archive
            .by_name(decoded.as_ref())
            .with_context(|| format!("Missing EPUB resource {decoded}"))?;
        ensure!(
            !entry.encrypted() && entry.size() <= 16 * 1024 * 1024,
            "Encrypted or oversized EPUB resource: {decoded}"
        );
        let mut bytes = Vec::with_capacity(entry.size() as usize);
        entry.take(16 * 1024 * 1024 + 1).read_to_end(&mut bytes)?;
        ensure!(
            bytes.len() <= 16 * 1024 * 1024,
            "EPUB resource exceeds its input limit"
        );
        resource_bytes = resource_bytes
            .checked_add(bytes.len())
            .context("EPUB resource size overflow")?;
        ensure!(
            resource_bytes <= 64 * 1024 * 1024,
            "EPUB chapter resources exceed the 64 MiB limit"
        );
        if media_type == "text/css" {
            let css = std::str::from_utf8(&bytes).context("EPUB stylesheet is not UTF-8")?;
            pending.extend(css_references(css, &url)?);
        }
        resources.push((
            url.path().trim_start_matches('/').to_owned(),
            media_type.clone(),
            bytes,
        ));
        ensure!(
            resources.len() <= 4096,
            "EPUB chapter references too many resources"
        );
    }
    let _ = chapter;
    Ok((resources, resource_bytes))
}

fn escape_xml(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn chapter_epub(book: &Book, chapter: usize) -> Result<Vec<u8>> {
    let entry = book
        .publication
        .spine()
        .get(chapter)
        .and_then(|item| item.manifest_entry())
        .context("Missing EPUB spine item")?;
    let chapter_href = entry.href().decode().into_owned();
    let chapter_url = Url::parse("https://breeze.invalid/")?.join(&chapter_href)?;
    let source = entry.read_str()?;
    ensure!(
        source.len() <= 16 * 1024 * 1024,
        "EPUB chapter exceeds its input limit"
    );
    let (resources, resource_bytes) = referenced_resources(book, chapter, &chapter_url, &source)?;
    ensure!(
        source.len() + resource_bytes <= 64 * 1024 * 1024,
        "EPUB chapter package exceeds its resource limit"
    );
    let package_path = book.publication.package().location().decode().into_owned();
    let package_dir = package_path
        .rsplit_once('/')
        .map_or("", |(directory, _)| directory);
    let package_base = Url::parse("https://breeze.invalid/")?
        .join(&format!("{}/", package_dir.trim_start_matches('/')))?;
    let chapter_manifest_href = package_base
        .make_relative(&chapter_url)
        .context("EPUB chapter is outside the package directory")?;
    let container = format!(
        r#"<?xml version="1.0"?><container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container"><rootfiles><rootfile full-path="{}" media-type="application/oebps-package+xml"/></rootfiles></container>"#,
        escape_xml(package_path.trim_start_matches('/'))
    );
    let mut items = format!(
        r#"<item id="chapter" href="{}" media-type="application/xhtml+xml"/>"#,
        escape_xml(&chapter_manifest_href)
    );
    for (index, (href, media_type, _)) in resources.iter().enumerate() {
        let resource_url = Url::parse("https://breeze.invalid/")?.join(&format!("/{href}"))?;
        let resource_href = package_base
            .make_relative(&resource_url)
            .context("EPUB resource is outside the package directory")?;
        items.push_str(&format!(
            r#"<item id="resource{index}" href="{}" media-type="{}"/>"#,
            escape_xml(&resource_href),
            escape_xml(media_type)
        ));
    }
    let identifier = escape_xml(book.publication.package().unique_identifier());
    let title = escape_xml(&book.metadata.title);
    let opf = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?><package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="bookid"><metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:identifier id="bookid">{identifier}</dc:identifier><dc:title>{title}</dc:title><dc:language>und</dc:language></metadata><manifest>{items}</manifest><spine><itemref idref="chapter"/></spine></package>"#
    );
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    writer.start_file(
        "mimetype",
        SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored),
    )?;
    writer.write_all(b"application/epub+zip")?;
    writer.start_file("META-INF/container.xml", SimpleFileOptions::default())?;
    writer.write_all(container.as_bytes())?;
    writer.start_file(
        package_path.trim_start_matches('/'),
        SimpleFileOptions::default(),
    )?;
    writer.write_all(opf.as_bytes())?;
    writer.start_file(
        chapter_href.trim_start_matches('/'),
        SimpleFileOptions::default(),
    )?;
    writer.write_all(source.as_bytes())?;
    for (href, _, bytes) in resources {
        writer.start_file(href, SimpleFileOptions::default())?;
        writer.write_all(&bytes)?;
    }
    Ok(writer.finish()?.into_inner())
}

fn prepare(book: &mut Book, request: &Request, chapter: usize) -> Result<()> {
    if book.layout.as_ref().is_some_and(|state| {
        state.chapter == chapter
            && state.width == request.width
            && state.height == request.height
            && state.font_size == request.font_size
    }) {
        return Ok(());
    }
    let bytes = chapter_epub(book, chapter)?;
    let document = Document::from_bytes(&bytes, "application/epub+zip")
        .context("MuPDF could not open EPUB")?;
    ensure!(
        !document.needs_password()? && document.is_reflowable()? && !document.is_pdf(),
        "Encrypted or non-reflowable EPUB is not supported"
    );
    let mut document = document;
    document.layout(request.width, request.height, request.font_size)?;
    let pages = usize::try_from(document.page_count()?)?;
    ensure!(
        (1..=20_000).contains(&pages),
        "EPUB layout exceeds the page limit"
    );
    book.layout = Some(ChapterLayout {
        chapter,
        width: request.width,
        height: request.height,
        font_size: request.font_size,
        document,
        pages,
    });
    Ok(())
}

pub(super) fn render(book: &mut Book, request: &Request) -> Result<Frame> {
    ensure!(
        request.location.progression.is_finite()
            && (0.0..=1.0).contains(&request.location.progression),
        "Invalid EPUB reading position"
    );
    ensure!(
        (240.0..=4096.).contains(&request.width)
            && (200.0..=4096.).contains(&request.height)
            && (0.5..=4.).contains(&request.scale)
            && (12.0..=32.).contains(&request.font_size)
            && request.scroll.is_finite(),
        "Unsupported EPUB viewport or text size"
    );
    let href = request.location.href.split('#').next().unwrap_or_default();
    let mut chapter = book
        .metadata
        .chapters
        .iter()
        .position(|entry| entry.href == href)
        .unwrap_or(0);
    prepare(book, request, chapter)?;
    let mut target = (book
        .layout
        .as_ref()
        .context("EPUB layout unavailable")?
        .pages
        .saturating_sub(1)) as f32
        * request.location.progression
        + f32::from(request.turn.signum())
        + request.scroll / request.height;
    let current_pages = book
        .layout
        .as_ref()
        .context("EPUB layout unavailable")?
        .pages;
    if target < 0. && chapter > 0 {
        chapter -= 1;
        prepare(book, request, chapter)?;
        target += book
            .layout
            .as_ref()
            .context("Previous EPUB chapter is unavailable")?
            .pages as f32;
    } else if target >= current_pages as f32 && chapter + 1 < book.metadata.chapters.len() {
        chapter += 1;
        prepare(book, request, chapter)?;
        target -= current_pages as f32;
    }
    let state = book.layout.as_ref().context("EPUB layout unavailable")?;
    target = target.clamp(0., state.pages.saturating_sub(1) as f32);
    let mut page = target.round() as usize;
    if let Some(query) = &request.find {
        ensure!(
            !query.is_empty() && query.len() <= 256,
            "Invalid EPUB search term"
        );
        for candidate in 0..state.pages {
            let text = state
                .document
                .load_page(candidate as i32)?
                .to_text_page(TextPageFlags::empty())?
                .to_text()?;
            if text.to_lowercase().contains(&query.to_lowercase()) {
                page = candidate;
                target = candidate as f32;
                break;
            }
        }
    }
    let native = state.document.load_page(i32::try_from(page)?)?;
    let pixmap = native.to_pixmap(
        &Matrix::new_scale(request.scale, request.scale),
        &Colorspace::device_rgb(),
        true,
        true,
    )?;
    let width = pixmap.width();
    let height = pixmap.height();
    ensure!(
        pixmap.n() == 4 && u64::from(width) * u64::from(height) <= MAX_PIXELS,
        "EPUB raster exceeds its limit"
    );
    let mut bgra = pixmap.samples().to_vec();
    ensure!(
        bgra.len() == width as usize * height as usize * 4,
        "Unexpected EPUB pixel buffer"
    );
    for pixel in bgra.as_chunks_mut::<4>().0 {
        pixel.swap(0, 2);
    }
    let text = native.to_text_page(TextPageFlags::empty())?.to_text()?;
    let links = native
        .links()?
        .take(4096)
        .map(|link| Link {
            bounds: [
                link.bounds.x0,
                link.bounds.y0,
                link.bounds.x1,
                link.bounds.y1,
            ],
            href: link.uri,
        })
        .collect();
    let count = state.pages;
    let progression = if count > 1 {
        target / (count - 1) as f32
    } else {
        0.
    };
    let progress =
        (chapter as f32 + (page as f32 + 1.) / count as f32) / book.metadata.chapters.len() as f32;
    Ok(Frame {
        metadata: book.metadata.clone(),
        location: EpubLocation {
            href: book.metadata.chapters[chapter].href.clone(),
            progression,
        },
        chapter,
        chapter_page: page,
        chapter_pages: count,
        width,
        height,
        bgra: Arc::new(bgra),
        text,
        progress,
        links,
        scroll_offset: target,
        scroll_extent: count.saturating_sub(1) as f32,
    })
}
