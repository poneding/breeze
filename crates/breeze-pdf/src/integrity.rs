//! Page-content integrity checks for damage MuPDF does not report.
//!
//! MuPDF renders malformed content defensively: a JPEG it cannot decode, a
//! resource name that resolves to nothing, or a form that references itself is
//! dropped or padded and the page still comes back as a successful pixmap with
//! an empty `Cookie`. Presenting those pixels as a complete page is exactly the
//! silent partial render the product forbids, so `breeze-pdf` walks the page
//! content, resolves what it draws, and refuses a page whose referenced content
//! cannot be produced.
//!
//! The walk is deliberately conservative. Only conditions that mean "this page
//! cannot show what it declares" fail the render: an unresolved `Do`/`sh`
//! resource, a form recursion, an image whose encoded data MuPDF cannot decode,
//! and a truncated image mask. Anything the parser cannot understand is left to
//! the renderer instead of being reported as damage.

use anyhow::{Context, Result, bail};
use mupdf::{
    Image,
    pdf::{PdfDocument, PdfObject, PdfPage},
};
use std::collections::HashMap;

/// Depth limit for nested form XObjects. MuPDF's own recursion guard stops
/// earlier; this only bounds the integrity walk.
const MAX_FORM_DEPTH: usize = 32;
/// Image masks may embed at most one nested mask check level; soft masks of
/// images are not recursive.
const MAX_IMAGE_MASK_DEPTH: usize = 4;
/// Content streams larger than this are scanned up to the limit only. Skipping
/// the remaining checks is safe: it can miss damage, never invent it.
const MAX_CONTENT_TOKENS: usize = 4_000_000;

/// Verdict cache for one loaded document. Keyed by object number so a shared
/// image is decoded at most once per document load.
#[derive(Default)]
pub(super) struct IntegrityCache {
    images: HashMap<i32, Option<String>>,
}

impl IntegrityCache {
    fn verdict(&self, xref: i32) -> Option<&Option<String>> {
        self.images.get(&xref)
    }

    fn remember(&mut self, xref: i32, verdict: Option<String>) {
        self.images.insert(xref, verdict);
    }
}

/// Verify that everything the page draws can actually be produced.
pub(super) fn verify_page_content(
    doc: &PdfDocument,
    page: &PdfPage,
    cache: &mut IntegrityCache,
) -> Result<()> {
    let resources = page.resources().context("Cannot read page resources")?;
    let contents = page
        .contents()
        .context("Cannot read the page content stream")?;
    let mut path = Vec::new();
    walk(doc, &resources, contents.as_ref(), cache, 0, &mut path)
}

fn walk(
    doc: &PdfDocument,
    resources: &PdfObject,
    content: Option<&PdfObject>,
    cache: &mut IntegrityCache,
    depth: usize,
    path: &mut Vec<i32>,
) -> Result<()> {
    if depth > MAX_FORM_DEPTH {
        bail!("Form XObjects nest more than {MAX_FORM_DEPTH} levels deep");
    }
    let Some(ops) = collect_operations(doc, content)? else {
        return Ok(());
    };
    for name in &ops.xobjects {
        let Some(object) = lookup(resources, "XObject", name)? else {
            bail!(
                "Page content references a missing object /{}",
                display_name(name)
            );
        };
        match subtype(&object)? {
            Subtype::Image => check_image(&object, cache, 0)?,
            Subtype::Form => {
                let xref = object.as_indirect().unwrap_or(0);
                if xref > 0 && path.contains(&xref) {
                    bail!("Page content contains a cyclic form XObject")
                }
                if xref > 0 {
                    path.push(xref);
                }
                let nested = object
                    .get_dict("Resources")
                    .context("Cannot read form resources")?
                    .unwrap_or_else(|| resources.try_clone().unwrap_or_else(|_| resources.clone()));
                let result = walk(doc, &nested, Some(&object), cache, depth + 1, path);
                if xref > 0 {
                    path.pop();
                }
                result?;
            }
            Subtype::Other => {}
        }
    }
    for name in &ops.shadings {
        let Some(shading) = lookup(resources, "Shading", name)? else {
            bail!(
                "Page content references a missing shading /{}",
                display_name(name)
            );
        };
        let Some(kind) = shading
            .get_dict("ShadingType")
            .context("Cannot read the shading type")?
        else {
            bail!("Shading has no type")
        };
        let kind = kind.resolve()?.unwrap_or(kind);
        let kind = kind.as_int().context("Cannot read the shading type")?;
        if !(1..=7).contains(&kind) {
            bail!("Unsupported shading type {kind}")
        }
    }
    for inline in &ops.inline_images {
        check_inline_image(inline)?;
    }
    Ok(())
}

/// Looks a name up in one resource subdictionary, resolving the result.
fn lookup(resources: &PdfObject, category: &str, name: &[u8]) -> Result<Option<PdfObject>> {
    let Some(dict) = resources
        .get_dict(category)
        .context("Cannot read page resources")?
    else {
        return Ok(None);
    };
    let dict = dict.resolve()?.unwrap_or(dict);
    let key =
        PdfObject::new_name(&String::from_utf8_lossy(name)).context("Invalid resource name")?;
    let Some(value) = dict.get_dict(key).context("Cannot read page resources")? else {
        return Ok(None);
    };
    Ok(Some(value))
}

fn subtype(object: &PdfObject) -> Result<Subtype> {
    let Some(value) = object
        .get_dict("Subtype")
        .context("Cannot read the XObject subtype")?
    else {
        return Ok(Subtype::Other);
    };
    let value = value.resolve()?.unwrap_or(value);
    let name = value.as_name().unwrap_or_default();
    Ok(match name.as_slice() {
        b"Image" => Subtype::Image,
        b"Form" => Subtype::Form,
        _ => Subtype::Other,
    })
}

enum Subtype {
    Image,
    Form,
    Other,
}

fn display_name(name: &[u8]) -> String {
    String::from_utf8_lossy(name).replace('#', "\\#")
}

/// Encoded image data MuPDF is known to drop without an error.
fn check_image(object: &PdfObject, cache: &mut IntegrityCache, depth: usize) -> Result<()> {
    if depth > MAX_IMAGE_MASK_DEPTH {
        return Ok(());
    }
    let xref = object.as_indirect().unwrap_or(0);
    match (xref > 0).then(|| cache.verdict(xref)).flatten() {
        Some(Some(message)) => bail!("{message}"),
        // A decoded image stays decoded for this document load.
        Some(None) => {}
        None => {
            let verdict = image_damage(object)?;
            if xref > 0 {
                cache.remember(xref, verdict.clone());
            }
            if let Some(message) = verdict {
                bail!("{message}");
            }
        }
    }
    for key in ["SMask", "Mask"] {
        let Some(mask) = object.get_dict(key).context("Cannot read the image mask")? else {
            continue;
        };
        check_image(&mask, cache, depth + 1)?;
    }
    Ok(())
}

/// Decode the image the way MuPDF will, so tolerant truncation stays acceptable
/// while data the engine cannot decode at all is reported.
fn image_damage(object: &PdfObject) -> Result<Option<String>> {
    let filters = filters(object)?;
    if filters.len() == 1
        && matches!(filters[0].as_slice(), b"DCTDecode" | b"JPXDecode")
        && Image::from_bytes(&object.read_raw_stream()?).is_err()
    {
        return Ok(Some(format!(
            "Embedded image data cannot be decoded ({})",
            filters[0].iter().map(|b| *b as char).collect::<String>()
        )));
    }
    truncated_mask(object)
}

fn filters(object: &PdfObject) -> Result<Vec<Vec<u8>>> {
    let Some(filter) = object
        .get_dict("Filter")
        .context("Cannot read the image filter")?
    else {
        return Ok(Vec::new());
    };
    let filter = filter.resolve()?.unwrap_or(filter);
    if filter.is_array().unwrap_or(false) {
        let mut names = Vec::new();
        for item in filter.array_iter()? {
            let item = item?;
            let item = item.resolve()?.unwrap_or(item);
            names.push(item.as_name().unwrap_or_default());
        }
        return Ok(names);
    }
    Ok(vec![filter.as_name().unwrap_or_default()])
}

/// An uncompressed image mask must carry one byte-aligned row per scanline.
/// A shorter stream is padded by the engine, leaving part of the mask unpainted.
fn truncated_mask(object: &PdfObject) -> Result<Option<String>> {
    let is_mask = object
        .get_dict("ImageMask")
        .ok()
        .flatten()
        .and_then(|value| value.resolve().ok().flatten().or(Some(value)))
        .map(|value| value.as_bool().unwrap_or(false))
        .unwrap_or(false);
    if !is_mask || !filters(object)?.is_empty() {
        return Ok(None);
    }
    let (Some(width), Some(height)) = (
        object
            .get_dict("Width")
            .ok()
            .flatten()
            .and_then(|value| value.resolve().ok().flatten().or(Some(value)))
            .and_then(|value| value.as_int().ok()),
        object
            .get_dict("Height")
            .ok()
            .flatten()
            .and_then(|value| value.resolve().ok().flatten().or(Some(value)))
            .and_then(|value| value.as_int().ok()),
    ) else {
        return Ok(None);
    };
    if width <= 0 || height <= 0 {
        return Ok(None);
    }
    let row_bytes = (i64::from(width) + 7) / 8;
    let expected = row_bytes * i64::from(height);
    let actual = i64::try_from(object.read_raw_stream()?.len()).unwrap_or(i64::MAX);
    if actual < expected {
        return Ok(Some(format!(
            "Embedded image mask is short: {actual} bytes, expected {expected}"
        )));
    }
    Ok(None)
}

#[derive(Default)]
struct Operations {
    xobjects: Vec<Vec<u8>>,
    shadings: Vec<Vec<u8>>,
    inline_images: Vec<InlineImage>,
}

struct InlineImage {
    filter: Vec<u8>,
    data: Vec<u8>,
}

/// Read a page or form content stream and collect the resource names it draws.
fn collect_operations(
    doc: &PdfDocument,
    content: Option<&PdfObject>,
) -> Result<Option<Operations>> {
    let Some(content) = content else {
        return Ok(None);
    };
    let mut data = Vec::new();
    // Resolving an indirect stream yields its dictionary, which can no longer be
    // read, so array-ness is probed on the resolved value while the stream itself
    // is read through the original object.
    let resolved = content.resolve()?;
    let array = resolved.filter(|value| value.is_array().unwrap_or(false));
    if let Some(array) = array {
        for item in array.array_iter()? {
            let item = item?;
            if let Ok(bytes) = item.read_stream() {
                data.extend_from_slice(&bytes);
            }
        }
    } else if let Ok(bytes) = content.read_stream() {
        data.extend_from_slice(&bytes);
    }
    let _ = doc;
    Ok(Some(parse_operations(&data)))
}

/// Scan decoded content bytes for `Do` / `sh` operands and inline images.
///
/// This is intentionally a light scanner over well-formed operators, not a full
/// interpreter: an operator it cannot classify is ignored, which can only make
/// the integrity check less strict.
fn parse_operations(data: &[u8]) -> Operations {
    let mut ops = Operations::default();
    let mut scanner = Scanner::new(data);
    let mut last_name: Option<Vec<u8>> = None;
    while let Some(token) = scanner.next_token() {
        match token.kind {
            Kind::Name => last_name = Some(token.bytes.to_vec()),
            Kind::Keyword => {
                match token.bytes {
                    b"Do" => {
                        if let Some(name) = last_name.take() {
                            ops.xobjects.push(name);
                        }
                    }
                    b"sh" => {
                        if let Some(name) = last_name.take() {
                            ops.shadings.push(name);
                        }
                    }
                    b"BI" => {
                        if let Some(inline) = scanner.read_inline_image() {
                            ops.inline_images.push(inline);
                        }
                    }
                    _ => {}
                }
                last_name = None;
            }
            Kind::Number | Kind::String | Kind::Delimiter => {}
        }
    }
    ops
}

fn check_inline_image(inline: &InlineImage) -> Result<()> {
    // Inline dictionaries may abbreviate the filter: /DCT and /JPX are the
    // DCTDecode and JPXDecode shorthand from the specification.
    let compressed = matches!(
        inline.filter.as_slice(),
        b"DCTDecode" | b"DCT" | b"JPXDecode" | b"JPX"
    );
    if !compressed {
        return Ok(());
    }
    if Image::from_bytes(&inline.data).is_err() {
        bail!(
            "Embedded image data cannot be decoded ({})",
            inline.filter.iter().map(|b| *b as char).collect::<String>()
        );
    }
    Ok(())
}

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Name,
    Number,
    Keyword,
    String,
    Delimiter,
}

#[derive(Clone, Copy)]
struct Token<'a> {
    kind: Kind,
    bytes: &'a [u8],
}

/// Minimal content-stream scanner: whitespace, comments, strings, hex strings,
/// names and regular tokens.
struct Scanner<'a> {
    data: &'a [u8],
    pos: usize,
    budget: usize,
}

impl<'a> Scanner<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self {
            data,
            pos: 0,
            budget: MAX_CONTENT_TOKENS,
        }
    }

    fn next_token(&mut self) -> Option<Token<'a>> {
        if self.budget == 0 {
            return None;
        }
        self.budget -= 1;
        self.skip_trivia();
        let byte = *self.data.get(self.pos)?;
        match byte {
            b'/' => {
                self.pos += 1;
                let start = self.pos;
                while self
                    .data
                    .get(self.pos)
                    .is_some_and(|b| !is_whitespace(*b) && !is_delimiter(*b))
                {
                    self.pos += 1;
                }
                Some(Token {
                    kind: Kind::Name,
                    bytes: &self.data[start..self.pos],
                })
            }
            b'(' => {
                let start = self.pos;
                self.pos += 1;
                let mut depth = 1;
                while let Some(byte) = self.data.get(self.pos) {
                    self.pos += 1;
                    match byte {
                        b'\\' => self.pos += 1,
                        b'(' => depth += 1,
                        b')' => {
                            depth -= 1;
                            if depth == 0 {
                                break;
                            }
                        }
                        _ => {}
                    }
                }
                Some(Token {
                    kind: Kind::String,
                    bytes: &self.data[start..self.pos],
                })
            }
            b'<' if self.data.get(self.pos + 1) == Some(&b'<') => {
                self.pos += 2;
                Some(Token {
                    kind: Kind::Delimiter,
                    bytes: b"<<",
                })
            }
            b'>' if self.data.get(self.pos + 1) == Some(&b'>') => {
                self.pos += 2;
                Some(Token {
                    kind: Kind::Delimiter,
                    bytes: b">>",
                })
            }
            b'<' => {
                let start = self.pos;
                self.pos += 1;
                while let Some(byte) = self.data.get(self.pos) {
                    self.pos += 1;
                    if *byte == b'>' {
                        break;
                    }
                }
                Some(Token {
                    kind: Kind::String,
                    bytes: &self.data[start..self.pos],
                })
            }
            b'[' | b']' | b'{' | b'}' | b')' | b'>' => {
                self.pos += 1;
                Some(Token {
                    kind: Kind::Delimiter,
                    bytes: &self.data[self.pos - 1..self.pos],
                })
            }
            _ => {
                let start = self.pos;
                while self
                    .data
                    .get(self.pos)
                    .is_some_and(|b| !is_whitespace(*b) && !is_delimiter(*b))
                {
                    self.pos += 1;
                }
                if start == self.pos {
                    self.pos += 1;
                }
                let bytes = &self.data[start..self.pos];
                let kind = if is_number(bytes) {
                    Kind::Number
                } else {
                    Kind::Keyword
                };
                Some(Token { kind, bytes })
            }
        }
    }

    fn skip_trivia(&mut self) {
        loop {
            match self.data.get(self.pos) {
                Some(byte) if is_whitespace(*byte) => self.pos += 1,
                Some(b'%') => {
                    while self.data.get(self.pos).is_some_and(|b| *b != b'\n') {
                        self.pos += 1;
                    }
                }
                _ => break,
            }
        }
    }

    /// Read an inline image body after `BI`, returning it when it declares a
    /// compressed filter. Dictionary keys and values alternate, and a value may
    /// itself be a name or an array of names.
    fn read_inline_image(&mut self) -> Option<InlineImage> {
        let mut key: Option<Vec<u8>> = None;
        let mut filter: Option<Vec<u8>> = None;
        let mut length: Option<usize> = None;
        let mut expect_key = true;
        let mut depth = 0usize;
        loop {
            let token = self.next_token()?;
            match token.kind {
                Kind::Keyword if token.bytes == b"ID" => break,
                Kind::Delimiter if matches!(token.bytes, b"[" | b"<<") => {
                    depth += 1;
                    expect_key = false;
                }
                Kind::Delimiter if matches!(token.bytes, b"]" | b">>") => {
                    depth = depth.saturating_sub(1);
                    if depth == 0 {
                        expect_key = true;
                    }
                }
                Kind::Delimiter => {}
                Kind::Name if expect_key => {
                    key = Some(token.bytes.to_vec());
                    expect_key = false;
                }
                Kind::Name => {
                    // A name value: `/F /DCT` or one element of `/F [...]`.
                    if key
                        .as_deref()
                        .is_some_and(|key| matches!(key, b"F" | b"Filter"))
                    {
                        filter = Some(token.bytes.to_vec());
                    }
                    if depth == 0 {
                        expect_key = true;
                    }
                }
                Kind::Number => {
                    if key
                        .as_deref()
                        .is_some_and(|key| matches!(key, b"L" | b"Length"))
                    {
                        length = std::str::from_utf8(token.bytes)
                            .ok()
                            .and_then(|text| text.parse::<usize>().ok());
                    }
                    if depth == 0 {
                        expect_key = true;
                    }
                }
                Kind::Keyword | Kind::String => {
                    if depth == 0 {
                        expect_key = true;
                    }
                }
            }
        }
        // One whitespace byte separates `ID` from the data.
        self.pos += 1;
        let start = self.pos;
        let data = if let Some(length) = length {
            let end = start.saturating_add(length);
            if end > self.data.len() {
                self.pos = self.data.len();
                self.data.get(start..).unwrap_or_default()
            } else {
                self.pos = end;
                &self.data[start..end]
            }
        } else {
            let end = self.find_inline_end(start)?;
            // The whitespace before `EI` terminates the data instead of belonging
            // to it, matching how the engine reads an inline image.
            let end = if end > start && is_whitespace(self.data[end - 1]) {
                end - 1
            } else {
                end
            };
            let data = &self.data[start..end];
            self.pos = end;
            data
        };
        Some(InlineImage {
            filter: filter.unwrap_or_default(),
            data: data.to_vec(),
        })
    }

    /// Inline image data ends at a whitespace-delimited `EI` token.
    fn find_inline_end(&self, start: usize) -> Option<usize> {
        let mut index = start;
        while index + 1 < self.data.len() {
            if self.data[index] == b'E'
                && self.data[index + 1] == b'I'
                && self
                    .data
                    .get(index + 2)
                    .is_none_or(|byte| is_whitespace(*byte) || is_delimiter(*byte))
            {
                return Some(index);
            }
            index += 1;
        }
        None
    }
}

fn is_whitespace(byte: u8) -> bool {
    matches!(byte, 0 | b'\t' | b'\n' | b'\x0c' | b'\r' | b' ')
}

fn is_delimiter(byte: u8) -> bool {
    matches!(
        byte,
        b'/' | b'(' | b')' | b'<' | b'>' | b'[' | b']' | b'{' | b'}' | b'%'
    )
}

fn is_number(bytes: &[u8]) -> bool {
    !bytes.is_empty()
        && bytes
            .iter()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'+' | b'-' | b'.'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scanner_collects_drawn_resource_names() {
        let ops = parse_operations(b"q /Im1 Do /Sh1 sh Q /Im1 gs /Unused Tf Q");
        assert_eq!(ops.xobjects, vec![b"Im1".to_vec()]);
        assert_eq!(ops.shadings, vec![b"Sh1".to_vec()]);
    }

    #[test]
    fn scanner_ignores_comments_and_strings() {
        let ops = parse_operations(b"% /Comment Do\n(/String) /Real Do");
        assert_eq!(ops.xobjects, vec![b"Real".to_vec()]);
    }

    #[test]
    fn scanner_reads_inline_image_filter_and_data() {
        let ops = parse_operations(b"BI /W 1 /H 1 /F /DCTDecode ID data EI Q");
        assert_eq!(ops.inline_images.len(), 1);
        assert_eq!(ops.inline_images[0].filter, b"DCTDecode".to_vec());
        assert_eq!(ops.inline_images[0].data, b"data".to_vec());
    }

    #[test]
    fn scanner_honours_an_explicit_inline_length() {
        let ops = parse_operations(b"BI /L 2 /F /DCT ID xy EI");
        assert_eq!(ops.inline_images.len(), 1);
        assert_eq!(ops.inline_images[0].data, b"xy".to_vec());
    }
}
