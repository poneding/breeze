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

/// Media type for an archive path.
///
/// The original package's manifest is the authority, but it is keyed by decoded
/// hrefs while a resolved URL is percent-encoded: `my%20sheet.css` must find the
/// manifest entry for `my sheet.css`. A reference that the manifest does not
/// declare at all is still a real file in the archive, and dropping it is how a
/// stylesheet or image silently disappeared, so its type is inferred instead.
fn media_type_for(manifest: &HashMap<String, String>, decoded: &str) -> Option<String> {
    let trimmed = decoded.trim_start_matches('/');
    if let Some(media_type) = manifest
        .get(trimmed)
        .or_else(|| manifest.get(decoded))
        .or_else(|| manifest.get(&format!("/{trimmed}")))
    {
        return Some(media_type.clone());
    }
    let extension = Path::new(trimmed)
        .extension()
        .and_then(|value| value.to_str())?
        .to_ascii_lowercase();
    Some(
        match extension.as_str() {
            "css" => "text/css",
            "png" => "image/png",
            "jpg" | "jpeg" => "image/jpeg",
            "gif" => "image/gif",
            "webp" => "image/webp",
            "svg" => "image/svg+xml",
            "otf" => "application/vnd.ms-opentype",
            "ttf" => "application/font-sfnt",
            "woff" => "font/woff",
            "woff2" => "font/woff2",
            "xhtml" | "html" | "htm" => return None,
            _ => "application/octet-stream",
        }
        .to_owned(),
    )
}

/// Read one archive entry by its decoded path.
///
/// Archives normally store the decoded name, but a producer that stored the
/// percent-encoded text is tolerated here rather than losing the resource.
fn read_entry(archive: &mut ZipArchive<Cursor<Arc<[u8]>>>, path: &str) -> Option<Vec<u8>> {
    let encoded =
        percent_encoding::utf8_percent_encode(path, percent_encoding::NON_ALPHANUMERIC).to_string();
    // Resolve the index first: a `ZipFile` borrows the archive, so probing by
    // name and then reopening would need two live mutable borrows.
    let index = archive
        .index_for_name(path)
        .or_else(|| archive.index_for_name(&encoded))?;
    let entry = archive.by_index(index).ok()?;
    if entry.encrypted() || entry.size() > 16 * 1024 * 1024 {
        return None;
    }
    let mut bytes = Vec::with_capacity(entry.size() as usize);
    entry
        .take(16 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .ok()?;
    (bytes.len() <= 16 * 1024 * 1024).then_some(bytes)
}

/// Local references in markup: `href`, `src`, and `url(...)` inside a `style`
/// attribute, which is where a background image on a paragraph hides.
fn local_references(source: &str, base: &Url) -> Result<Vec<Url>> {
    let mut reader = XmlReader::from_str(source);
    let mut references = Vec::new();
    loop {
        match reader.read_event()? {
            Event::Start(element) | Event::Empty(element) => {
                for attribute in element.attributes().with_checks(true) {
                    let attribute = attribute?;
                    let name = attribute.key.local_name();
                    let value = attribute.normalized_value(XmlVersion::Implicit1_0)?;
                    if name.as_ref() == "style" {
                        references.extend(css_references(&value, base)?);
                        continue;
                    }
                    if name.as_ref() != "href" && name.as_ref() != "src" {
                        continue;
                    }
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

/// Local references in a stylesheet: `url(...)` and `@import "..."`.
///
/// At-imports pull in the rest of a publication's styles, so missing them
/// leaves the chapter with only part of its stylesheet.
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
    cursor = 0;
    while let Some(offset) = lower[cursor..].find("@import") {
        let start = cursor + offset + "@import".len();
        let tail = source[start..].trim_start();
        let value = if let Some(rest) = tail.strip_prefix("url(") {
            let Some(end) = rest.find(')') else { break };
            rest[..end].trim().trim_matches(['\'', '"']).to_owned()
        } else if let Some(rest) = tail.strip_prefix(['\'', '"']) {
            let quote = tail.chars().next().unwrap_or('\'');
            rest[..rest.find(quote).unwrap_or(rest.len())].to_owned()
        } else {
            String::new()
        };
        if !value.is_empty()
            && !value.starts_with("data:")
            && let Ok(url) = base.join(&value)
            && url.host_str() == Some("breeze.invalid")
        {
            references.push(url);
        }
        cursor = start + 1;
        ensure!(
            references.len() <= 4096,
            "EPUB stylesheet imports too many resources"
        );
    }
    Ok(references)
}

/// Every resource the chapter needs, read out of the publication archive.
fn referenced_resources(
    book: &Book,
    chapter_url: &Url,
    source: &str,
) -> Result<(Vec<Resource>, usize)> {
    let manifest: HashMap<String, String> = book
        .publication
        .manifest()
        .iter()
        .map(|entry| {
            (
                entry.href().decode().trim_start_matches('/').to_owned(),
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
        // The archive path is decoded, and it is also the manifest key.
        let decoded = percent_encoding::percent_decode_str(url.path().trim_start_matches('/'))
            .decode_utf8()?
            .into_owned();
        if !visited.insert(decoded.clone()) {
            continue;
        }
        let Some(media_type) = media_type_for(&manifest, &decoded) else {
            continue;
        };
        let Some(bytes) = read_entry(&mut archive, &decoded) else {
            // A declared-but-absent file is not fatal: the rest of the chapter
            // still has to render, and the reference is reported by its absence.
            continue;
        };
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
        resources.push((decoded, media_type, bytes));
        ensure!(
            resources.len() <= 4096,
            "EPUB chapter references too many resources"
        );
    }
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
    let (resources, resource_bytes) = referenced_resources(book, &chapter_url, &source)?;
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
    let language = escape_xml(
        book.publication
            .metadata()
            .language()
            .map(|language| language.value().to_owned())
            .unwrap_or_else(|| "und".to_owned())
            .as_str(),
    );
    let direction = if book.publication.spine().page_direction().is_rtl() {
        " page-progression-direction=\"rtl\""
    } else {
        ""
    };
    let opf = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?><package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="bookid"><metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:identifier id="bookid">{identifier}</dc:identifier><dc:title>{title}</dc:title><dc:language>{language}</dc:language></metadata><manifest>{items}</manifest><spine{direction}><itemref idref="chapter"/></spine></package>"#
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

/// Pages a frame shows side by side: one, or a facing pair in two-page mode.
fn columns(request: &Request) -> usize {
    if request.spread { 2 } else { 1 }
}

/// One page of the chapter. Two-page reading paginates at half the viewport so
/// the pair fills the content area and the combined raster keeps the same pixel
/// budget as a single page.
fn column_width(request: &Request) -> f32 {
    request.width / columns(request) as f32
}

/// Reading steps a chapter holds: one per page, or one per facing pair.
fn units(pages: usize, columns: usize) -> usize {
    pages.div_ceil(columns)
}

fn prepare(book: &mut Book, request: &Request, chapter: usize) -> Result<()> {
    let width = column_width(request);
    if book.layout.as_ref().is_some_and(|state| {
        state.chapter == chapter
            && state.width == width
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
    document.layout(width, request.height, request.font_size)?;
    let pages = usize::try_from(document.page_count()?)?;
    ensure!(
        (1..=20_000).contains(&pages),
        "EPUB layout exceeds the page limit"
    );
    book.layout = Some(ChapterLayout {
        chapter,
        width,
        height: request.height,
        font_size: request.font_size,
        document,
        pages,
    });
    Ok(())
}

fn layout_pages(book: &Book) -> Result<usize> {
    Ok(book
        .layout
        .as_ref()
        .context("EPUB layout unavailable")?
        .pages)
}

/// One page rasterized at the requested device scale, as opaque BGRA.
struct Raster {
    width: u32,
    height: u32,
    bgra: Vec<u8>,
}

/// The page is rendered opaque: device RGB yields three channels, and the
/// texture needs BGRA, so the alpha byte is added here rather than making the
/// page premultiplied and transparent.
fn raster(page: &mupdf::Page, scale: f32) -> Result<Raster> {
    let pixmap = page.to_pixmap(
        &Matrix::new_scale(scale, scale),
        &Colorspace::device_rgb(),
        false,
        true,
    )?;
    let width = pixmap.width();
    let height = pixmap.height();
    ensure!(
        u64::from(width) * u64::from(height) <= MAX_PIXELS,
        "EPUB raster exceeds its limit"
    );
    let mut bgra = Vec::with_capacity(width as usize * height as usize * 4);
    match pixmap.n() {
        3 => {
            ensure!(
                pixmap.samples().len() >= width as usize * height as usize * 3,
                "Unexpected EPUB pixel buffer"
            );
            for rgb in pixmap.samples().as_chunks::<3>().0 {
                bgra.extend_from_slice(&[rgb[2], rgb[1], rgb[0], 255]);
            }
        }
        4 => {
            bgra.extend_from_slice(pixmap.samples());
            ensure!(
                bgra.len() == width as usize * height as usize * 4,
                "Unexpected EPUB pixel buffer"
            );
            for pixel in bgra.as_chunks_mut::<4>().0 {
                pixel.swap(0, 2);
            }
        }
        channels => anyhow::bail!("Unexpected EPUB raster channel count {channels}"),
    }
    Ok(Raster {
        width,
        height,
        bgra,
    })
}

/// Place one page inside a wider spread buffer at `offset_x`.
fn blit(dst: &mut [u8], dst_width: u32, offset_x: u32, page: &Raster) {
    let row_bytes = page.width as usize * 4;
    let dst_stride = dst_width as usize * 4;
    let offset = offset_x as usize * 4;
    for row in 0..page.height as usize {
        let source = &page.bgra[row * row_bytes..(row + 1) * row_bytes];
        let start = row * dst_stride + offset;
        dst[start..start + row_bytes].copy_from_slice(source);
    }
}

/// The requested page, or that page and its facing page on white ground.
///
/// The pair shares one texture so a turn still hands the reader a single raster.
/// Pages laid out by the same reflow document share their size, but a differing
/// height is padded rather than allowed to leave a transparent band.
fn spread_buffer(pages: Vec<Raster>) -> Result<(u32, u32, Vec<u8>)> {
    let mut iter = pages.into_iter();
    let Some(first) = iter.next() else {
        anyhow::bail!("EPUB page is missing");
    };
    let Some(second) = iter.next() else {
        return Ok((first.width, first.height, first.bgra));
    };
    let width = first.width + second.width;
    let height = first.height.max(second.height);
    ensure!(
        u64::from(width) * u64::from(height) <= MAX_PIXELS,
        "EPUB raster exceeds its limit"
    );
    // The publication's own page background is white, so the gutter is white
    // too and never composites as a transparent or black band.
    let mut bgra = vec![255u8; width as usize * height as usize * 4];
    blit(&mut bgra, width, 0, &first);
    blit(&mut bgra, width, first.width, &second);
    Ok((width, height, bgra))
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
            && request.scroll.is_finite()
            && column_width(request).is_finite(),
        "Unsupported EPUB viewport or text size"
    );
    let columns = columns(request);
    let href = request.location.href.split('#').next().unwrap_or_default();
    let mut chapter = book
        .metadata
        .chapters
        .iter()
        .position(|entry| entry.href == href)
        .unwrap_or(0);
    prepare(book, request, chapter)?;
    let mut pages = layout_pages(book)?;
    let mut count = units(pages, columns);
    // `turn` is a step delta, not a direction: one fast flick can cross several
    // pages or spreads. The location names a page, so it is divided back into
    // steps and, in two-page mode, snapped to the start of a spread before the
    // delta applies. A location saved while single pages were laid out can
    // otherwise name the second page of a face.
    let mut target = (pages.saturating_sub(1) as f32 * request.location.progression
        + request.scroll / request.height)
        / columns as f32;
    if columns > 1 {
        target = target.floor();
    }
    target += f32::from(request.turn);
    if target < 0. && chapter > 0 {
        chapter -= 1;
        prepare(book, request, chapter)?;
        pages = layout_pages(book)?;
        count = units(pages, columns);
        target += count as f32;
    } else if target >= count as f32 && chapter + 1 < book.metadata.chapters.len() {
        target -= count as f32;
        chapter += 1;
        prepare(book, request, chapter)?;
        pages = layout_pages(book)?;
        count = units(pages, columns);
    }
    let target = target.clamp(0., count.saturating_sub(1) as f32).round();
    let mut page = (target as usize * columns).min(pages.saturating_sub(1));
    if let Some(query) = &request.find {
        ensure!(
            !query.is_empty() && query.len() <= 256,
            "Invalid EPUB search term"
        );
        let document = &book
            .layout
            .as_ref()
            .context("EPUB layout unavailable")?
            .document;
        for candidate in 0..pages {
            let text = document
                .load_page(candidate as i32)?
                .to_text_page(TextPageFlags::empty())?
                .to_text()?;
            if text.to_lowercase().contains(&query.to_lowercase()) {
                page = candidate / columns * columns;
                break;
            }
        }
    }
    let document = &book
        .layout
        .as_ref()
        .context("EPUB layout unavailable")?
        .document;
    let mut rasters = Vec::with_capacity(columns);
    let mut text = String::new();
    let mut links = Vec::new();
    // Bounds of the next page are offset by the width of the pages before it, so
    // a link in the right half keeps its position in the combined raster.
    let mut left_points = 0f32;
    for offset in 0..columns {
        let index = page + offset;
        if index >= pages {
            break;
        }
        let native = document.load_page(i32::try_from(index)?)?;
        if offset > 0 {
            text.push('\n');
        }
        text.push_str(&native.to_text_page(TextPageFlags::empty())?.to_text()?);
        for link in native.links()?.take(4096) {
            links.push(Link {
                bounds: [
                    link.bounds.x0 + left_points,
                    link.bounds.y0,
                    link.bounds.x1 + left_points,
                    link.bounds.y1,
                ],
                href: link.uri,
            });
        }
        left_points += native.bounds()?.width();
        rasters.push(raster(&native, request.scale)?);
    }
    let (width, height, bgra) = spread_buffer(rasters)?;
    let progression = if pages > 1 {
        page as f32 / (pages - 1) as f32
    } else {
        0.
    };
    let progress = (chapter as f32
        + (page as f32 + columns.min(pages - page) as f32) / pages as f32)
        / book.metadata.chapters.len() as f32;
    Ok(Frame {
        metadata: book.metadata.clone(),
        location: EpubLocation {
            href: book.metadata.chapters[chapter].href.clone(),
            progression,
        },
        chapter,
        chapter_page: page,
        chapter_pages: pages,
        columns,
        width,
        height,
        bgra,
        text,
        progress,
        links,
        scroll_offset: page as f32,
        scroll_extent: pages.saturating_sub(1) as f32,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Cursor, Write};
    use zip::{ZipWriter, write::SimpleFileOptions};

    /// Build a publication whose stylesheet and image names contain a space, so
    /// the package manifest stores them percent-encoded.
    fn encoded_resource_publication() -> Vec<u8> {
        let mut archive = ZipWriter::new(Cursor::new(Vec::new()));
        fn write(archive: &mut ZipWriter<Cursor<Vec<u8>>>, path: &str, text: &str) {
            archive
                .start_file(path, SimpleFileOptions::default())
                .unwrap();
            archive.write_all(text.as_bytes()).unwrap();
        }
        fn write_bytes(archive: &mut ZipWriter<Cursor<Vec<u8>>>, path: &str, bytes: &[u8]) {
            archive
                .start_file(path, SimpleFileOptions::default())
                .unwrap();
            archive.write_all(bytes).unwrap();
        }
        write(&mut archive, "mimetype", "application/epub+zip");
        write(
            &mut archive,
            "META-INF/container.xml",
            r#"<?xml version="1.0"?><container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container"><rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles></container>"#,
        );
        write(
            &mut archive,
            "OEBPS/content.opf",
            r#"<?xml version="1.0"?><package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="bookid"><metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:identifier id="bookid">breeze-encoded-fixture</dc:identifier><dc:title>Encoded resources</dc:title><dc:language>zh</dc:language><meta property="dcterms:modified">2026-10-15T00:00:00Z</meta></metadata><manifest><item id="ch" href="ch1.xhtml" media-type="application/xhtml+xml"/><item id="css" href="Styles/main%20sheet.css" media-type="text/css"/><item id="img" href="Images/cover%20art.png" media-type="image/png"/><item id="extra" href="fonts/Body.otf" media-type="application/vnd.ms-opentype"/></manifest><spine><itemref idref="ch"/></spine></package>"#,
        );
        write(
            &mut archive,
            "OEBPS/Styles/main sheet.css",
            "@font-face { font-family: Body; src: url('../fonts/Body.otf'); } p { color: #c02020; } @import \"extra.css\"; body { background-image: url('../Images/cover%20art.png'); }",
        );
        // An at-imported stylesheet and an undeclared font: both are real files
        // the chapter needs, and neither may be dropped.
        write(
            &mut archive,
            "OEBPS/Styles/extra.css",
            "h1 { margin: 30px 0; }",
        );
        write(
            &mut archive,
            "OEBPS/fonts/Body.otf",
            "not a real font, but a declared resource",
        );
        write_bytes(&mut archive, "OEBPS/Images/cover art.png", &[0u8; 8]);
        write(
            &mut archive,
            "OEBPS/ch1.xhtml",
            r#"<html xmlns="http://www.w3.org/1999/xhtml"><head><title>Chapter</title><link rel="stylesheet" type="text/css" href="Styles/main%20sheet.css"/></head><body><p style="background-image: url('Images/cover%20art.png')">Styled paragraph</p></body></html>"#,
        );
        archive.finish().unwrap().into_inner()
    }

    fn packaged(publication: Vec<u8>) -> Vec<String> {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("fixture.epub");
        std::fs::write(&path, &publication).unwrap();
        let book = Book::open(&path).unwrap();
        let bytes = chapter_epub(&book, 0).unwrap();
        let archive = ZipArchive::new(Cursor::new(bytes)).unwrap();
        (0..archive.len())
            .map(|index| archive.name_for_index(index).unwrap().unwrap().into_owned())
            .collect()
    }

    #[test]
    fn encoded_references_keep_their_stylesheets_images_and_fonts() {
        let names = packaged(encoded_resource_publication());
        for expected in [
            "OEBPS/Styles/main sheet.css",
            "OEBPS/Styles/extra.css",
            "OEBPS/Images/cover art.png",
            "OEBPS/fonts/Body.otf",
        ] {
            assert!(
                names.iter().any(|name| name == expected),
                "{expected} was dropped from the chapter package; entries: {names:?}"
            );
        }
    }

    #[test]
    fn undeclared_but_present_assets_are_still_packaged() {
        // Remove the image from the manifest while keeping the file: the
        // stylesheet still references it, so the chapter needs it.
        let publication = remove_manifest_item(encoded_resource_publication(), "img");
        let names = packaged(publication);
        assert!(
            names
                .iter()
                .any(|name| name == "OEBPS/Images/cover art.png"),
            "an undeclared asset was dropped; entries: {names:?}"
        );
    }

    /// Rewrite the OPF inside the archive, dropping one manifest item.
    fn remove_manifest_item(publication: Vec<u8>, id: &str) -> Vec<u8> {
        let mut archive = ZipArchive::new(Cursor::new(publication)).unwrap();
        let mut files: Vec<(String, Vec<u8>)> = Vec::new();
        for index in 0..archive.len() {
            let mut entry = archive.by_index(index).unwrap();
            let name = entry.name().unwrap().into_owned();
            let mut bytes = Vec::new();
            entry.read_to_end(&mut bytes).unwrap();
            if name == "OEBPS/content.opf" {
                let text = String::from_utf8(bytes).unwrap();
                let marker = format!(r#"<item id="{id}" href="#);
                let start = text.find(&marker).unwrap();
                let end = text[start..].find("/>").unwrap() + start + 2;
                bytes = text.replacen(&text[start..end], "", 1).into_bytes();
            }
            files.push((name, bytes));
        }
        let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
        for (name, bytes) in files {
            writer
                .start_file(name, SimpleFileOptions::default())
                .unwrap();
            writer.write_all(&bytes).unwrap();
        }
        writer.finish().unwrap().into_inner()
    }
}
