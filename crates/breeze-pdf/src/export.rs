//! Export annotations into a new PDF. Original objects and page content are verified before publication.
mod appearance;
#[cfg(test)]
mod tests;
use super::{Source, read_source};
use anyhow::{Context, Result, bail, ensure};
use breeze_core::annotations::{Annotation, Kind, Rect};
use lopdf::{Dictionary, Document, Object, ObjectId, StringFormat, dictionary};
use mupdf::pdf::PdfDocument as MuPdfDocument;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::Write,
    path::PathBuf,
    time::Instant,
};

const MAX_OUTPUT_BYTES: usize = 512 * 1024 * 1024;
#[derive(Clone)]
pub struct ExportRequest {
    pub source: Source,
    pub source_fingerprint: String,
    pub annotations: Vec<Annotation>,
    pub destination: PathBuf,
}
#[derive(Debug)]
pub struct ExportReceipt {
    pub destination: PathBuf,
    pub annotations: usize,
    pub bytes: u64,
    pub elapsed_ms: f64,
    pub sticky_notes: usize,
}

/// Runs blocking work and must only be called from a background executor.
pub fn export_annotations(request: ExportRequest) -> Result<ExportReceipt> {
    let started = Instant::now();
    ensure!(
        request.destination.is_absolute(),
        "Export destination must be an absolute path"
    );
    ensure!(
        request
            .destination
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("pdf")),
        "Export destination must end in .pdf"
    );
    match fs::symlink_metadata(&request.destination) {
        Ok(_) => bail!("Destination already exists; choose a new PDF filename"),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(e.into()),
    }
    ensure!(
        !request.annotations.is_empty(),
        "There are no Breeze annotations to export"
    );
    ensure!(request.annotations.len() <= 5000, "Too many annotations");
    ensure!(
        request
            .annotations
            .iter()
            .map(|a| a.points.len() + a.quads.len())
            .sum::<usize>()
            <= 100_000,
        "Annotation geometry exceeds export budget"
    );
    for a in &request.annotations {
        a.validate().map_err(anyhow::Error::msg)?;
    }
    // The source is fingerprinted by streaming it and loaded file-backed: reading
    // the whole PDF into memory duplicated a buffer the parser builds anyway.
    let mut doc = match &request.source {
        Source::Sample => {
            let bytes = read_source(&request.source)?;
            ensure!(
                breeze_storage::bytes_fingerprint(&bytes) == request.source_fingerprint,
                "Source changed; export refused"
            );
            Document::load_mem(&bytes).context("Cannot parse PDF for annotation export")?
        }
        Source::File(path) => {
            let before = breeze_storage::source_identity(path)?;
            ensure!(
                breeze_storage::fingerprint(path)? == request.source_fingerprint,
                "Source changed; export refused"
            );
            let doc = Document::load(path).context("Cannot parse PDF for annotation export")?;
            ensure!(
                breeze_storage::source_identity(path)? == before,
                "Source changed during export; export refused"
            );
            doc
        }
    };
    preflight(&doc)?;
    // Explicit appearances use transparency and rectangle differences; never emit them under an older header.
    match doc.version.as_str() {
        "1.0" | "1.1" | "1.2" | "1.3" | "1.4" | "1.5" | "1.6" => doc.version = "1.7".into(),
        "1.7" | "2.0" => {}
        _ => bail!("Unsupported PDF header version for export"),
    }
    let pages = doc.get_pages();
    ensure!(!pages.is_empty(), "Document has no pages");
    for a in &request.annotations {
        ensure!(
            a.page < pages.len(),
            "Annotation page is outside the document"
        );
    }
    let page_ids: BTreeSet<_> = pages.values().copied().collect();
    let original_signatures = signatures(&doc, &page_ids)?;
    let original_max_id = doc.max_id;
    let mut original_annots = BTreeMap::new();
    for (&number, &id) in &pages {
        original_annots.insert(number, annotation_entries(&doc, id)?);
    }
    let mut additions: BTreeMap<u32, Vec<ObjectId>> = BTreeMap::new();
    for (index, a) in request.annotations.iter().enumerate() {
        let page_number = a.page as u32 + 1;
        let page_id = *pages.get(&page_number).context("Missing target page")?;
        let id = append_annotation(
            &mut doc,
            page_id,
            a,
            &format!("breeze-{}-{index}", &request.source_fingerprint[..16]),
        )?;
        additions.entry(page_number).or_default().push(id);
    }
    for (&number, ids) in &additions {
        let mut entries = original_annots[&number].clone();
        entries.extend(ids.iter().copied().map(Object::Reference));
        doc.get_dictionary_mut(pages[&number])?
            .set("Annots", Object::Array(entries));
    }
    let added_signatures: BTreeMap<_, _> = doc
        .objects
        .range((original_max_id + 1, 0)..)
        .map(|(id, object)| Ok((*id, signature(object, false)?)))
        .collect::<Result<_>>()?;
    let parent = request
        .destination
        .parent()
        .context("Missing output directory")?;
    let mut staged = tempfile::NamedTempFile::new_in(parent)?;
    let mut writer = BudgetWriter {
        inner: staged.as_file_mut(),
        written: 0,
    };
    doc.save_to(&mut writer)
        .context("Could not serialize export within output budget")?;
    staged.as_file().sync_all()?;
    // Writer/parser round-trip checks every original object, excluding only page Annots and xref storage.
    let reopened = Document::load(staged.path())?;
    ensure!(
        reopened.get_pages() == pages,
        "Page references/count changed during export"
    );
    for (id, hash) in original_signatures {
        let object = reopened
            .objects
            .get(&id)
            .context("An original PDF object was lost")?;
        ensure!(
            signature(object, page_ids.contains(&id))? == hash,
            "Original PDF object {id:?} changed unexpectedly"
        );
    }
    for (id, expected) in added_signatures {
        ensure!(
            signature(reopened.get_object(id)?, false)? == expected,
            "Exported annotation/appearance object {id:?} changed during serialization"
        );
    }
    for (&number, &id) in &pages {
        let entries = annotation_entries(&reopened, id)?;
        let before = &original_annots[&number];
        let added = additions.get(&number).map_or(0, Vec::len);
        ensure!(
            entries.len() == before.len() + added,
            "Annotation count changed unexpectedly"
        );
        for (old, new) in before.iter().zip(&entries) {
            ensure!(
                signature(old, false)? == signature(new, false)?,
                "An existing annotation entry changed"
            );
        }
        if let Some(ids) = additions.get(&number) {
            for (entry, &id) in entries[before.len()..].iter().zip(ids) {
                ensure!(
                    entry.as_reference()? == id,
                    "An exported annotation is not linked from its intended page"
                );
                verify_appearance(&reopened, id)?;
            }
        }
    }
    // Independently parse page/annotation structure with the reader engine, not just the writer.
    let reader = MuPdfDocument::open(staged.path())?;
    ensure!(
        usize::try_from(reader.page_count()?)? == pages.len(),
        "Independent parser rejected page count"
    );
    for (&number, ids) in &additions {
        let page = reader.load_pdf_page(number as i32 - 1)?;
        let annots = page.object().get_dict("Annots")?;
        let annotation_count = if let Some(annots) = annots {
            let annots = annots.resolve()?.unwrap_or(annots);
            ensure!(
                annots.is_array()?,
                "Independent parser found invalid Annots"
            );
            annots.len()?
        } else {
            0
        };
        ensure!(
            annotation_count == original_annots[&number].len() + ids.len(),
            "Independent parser did not read exported annotations"
        );
    }
    if let Source::File(source) = &request.source {
        ensure!(
            breeze_storage::fingerprint(source)? == request.source_fingerprint,
            "Source changed during export; output was not published"
        );
    }
    let bytes = staged.as_file().metadata()?.len();
    staged
        .persist_noclobber(&request.destination)
        .map_err(|e| e.error)
        .context("Could not publish export without overwriting destination")?;
    Ok(ExportReceipt {
        destination: request.destination,
        annotations: request.annotations.len(),
        bytes,
        elapsed_ms: started.elapsed().as_secs_f64() * 1000.,
        sticky_notes: request
            .annotations
            .iter()
            .filter(|a| a.kind == Kind::Note)
            .count(),
    })
}
struct BudgetWriter<'a> {
    inner: &'a mut fs::File,
    written: usize,
}
impl Write for BudgetWriter<'_> {
    fn write(&mut self, data: &[u8]) -> std::io::Result<usize> {
        if self.written.saturating_add(data.len()) > MAX_OUTPUT_BYTES {
            return Err(std::io::Error::other("Export exceeds 512 MiB output limit"));
        }
        let n = self.inner.write(data)?;
        self.written += n;
        Ok(n)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }
}
fn preflight(doc: &Document) -> Result<()> {
    ensure!(
        doc.max_id <= 1_000_000,
        "Sparse PDF object identifiers exceed the export budget"
    );
    ensure!(
        !doc.is_encrypted(),
        "Encrypted PDFs are not supported for export"
    );
    ensure!(
        doc.objects.len() <= 250_000,
        "PDF object count exceeds export budget"
    );
    ensure!(
        !doc.catalog()?.has(b"AcroForm"),
        "Form PDFs are not supported by this exporter"
    );
    for object in doc.objects.values() {
        reject_signatures(object, 0)?;
    }
    for id in doc.get_pages().values() {
        let page = doc.get_dictionary(*id)?;
        let media = page_box(doc, *id, b"MediaBox")?.context("Page has no MediaBox")?;
        if let Some(crop) = page_box(doc, *id, b"CropBox")? {
            ensure!(
                crop[2].min(media[2]) > crop[0].max(media[0])
                    && crop[3].min(media[3]) > crop[1].max(media[1]),
                "CropBox does not intersect MediaBox"
            );
        }
        if let Some(rotation) = inherited(doc, *id, b"Rotate")? {
            let rotation = rotation.as_i64()?;
            ensure!(
                i32::try_from(rotation).is_ok() && rotation % 90 == 0,
                "Unsupported page rotation"
            );
        }
        if let Ok(unit) = page.get(b"UserUnit") {
            ensure!(
                doc.dereference(unit)?.1.as_float()? == 1.,
                "Non-default UserUnit requires separate export validation"
            );
        }
    }
    Ok(())
}
fn inherited<'a>(doc: &'a Document, mut id: ObjectId, key: &[u8]) -> Result<Option<&'a Object>> {
    let mut seen = BTreeSet::new();
    for _ in 0..64 {
        ensure!(seen.insert(id), "Cycle in inherited page attributes");
        let d = doc.get_dictionary(id)?;
        if let Ok(value) = d.get(key) {
            return Ok(Some(doc.dereference(value)?.1));
        }
        if !d.has(b"Parent") {
            return Ok(None);
        }
        id = d.get(b"Parent")?.as_reference()?;
    }
    bail!("Page inheritance exceeds export budget")
}
fn page_box(doc: &Document, page: ObjectId, key: &[u8]) -> Result<Option<[f32; 4]>> {
    let Some(value) = inherited(doc, page, key)? else {
        return Ok(None);
    };
    let values = value.as_array()?;
    ensure!(values.len() == 4, "Invalid page box length");
    let b = [
        values[0].as_float()?,
        values[1].as_float()?,
        values[2].as_float()?,
        values[3].as_float()?,
    ];
    ensure!(
        b.iter().all(|n| n.is_finite() && n.abs() < 1e7) && b[2] > b[0] && b[3] > b[1],
        "Invalid page box geometry"
    );
    Ok(Some(b))
}
fn reject_signatures(object: &Object, depth: usize) -> Result<()> {
    ensure!(depth < 64, "PDF object nesting exceeds export budget");
    match object {
        Object::Dictionary(d) | Object::Stream(lopdf::Stream { dict: d, .. }) => {
            ensure!(
                !matches!(d.get(b"Subtype").and_then(Object::as_name), Ok(b"Widget")),
                "Form widgets are not supported for export"
            );
            ensure!(
                !d.has(b"ByteRange") && !d.has(b"DocMDP") && !d.has_type(b"Sig"),
                "Signed PDFs are not supported for annotation export"
            );
            for (_, v) in d.iter() {
                reject_signatures(v, depth + 1)?;
            }
        }
        Object::Array(items) => {
            for item in items {
                reject_signatures(item, depth + 1)?;
            }
        }
        _ => {}
    }
    Ok(())
}
fn annotation_entries(doc: &Document, page: ObjectId) -> Result<Vec<Object>> {
    let page = doc.get_dictionary(page)?;
    match page.get(b"Annots") {
        Ok(value) => {
            let items = doc.dereference(value)?.1.as_array()?.clone();
            for item in &items {
                doc.dereference(item)?
                    .1
                    .as_dict()
                    .context("Malformed existing annotation")?;
            }
            Ok(items)
        }
        Err(lopdf::Error::DictKey(_)) => Ok(Vec::new()),
        Err(e) => Err(e.into()),
    }
}
fn unicode(text: &str) -> Object {
    let mut bytes = vec![0xfe, 0xff];
    for unit in text.encode_utf16() {
        bytes.extend_from_slice(&unit.to_be_bytes());
    }
    Object::String(bytes, StringFormat::Hexadecimal)
}
fn append_annotation(
    doc: &mut Document,
    page: ObjectId,
    a: &Annotation,
    name: &str,
) -> Result<ObjectId> {
    let bounds = appearance::bounds(a)?;
    let normal = appearance::build(doc, a, bounds)?;
    let subtype = match a.kind {
        Kind::Highlight => "Highlight",
        Kind::Underline => "Underline",
        Kind::Strikeout => "StrikeOut",
        Kind::Area | Kind::Rectangle => "Square",
        Kind::Ellipse => "Circle",
        Kind::Line | Kind::Arrow => "Line",
        Kind::Ink | Kind::Squiggle => "Ink",
        Kind::Note => "Text",
    };
    let mut d = dictionary! {"Type"=>"Annot","Subtype"=>subtype,"P"=>page,"Rect"=>rect(bounds),"C"=>appearance::array(a.color),"CA"=>a.opacity,"F"=>4,"NM"=>Object::string_literal(name),"T"=>Object::string_literal("Breeze"),"BS"=>dictionary!{"Type"=>"Border","W"=>a.width,"S"=>"S"},"AP"=>dictionary!{"N"=>normal}};
    if a.kind.text_markup() {
        // Acrobat-compatible Z order: upper-left, upper-right, lower-left, lower-right.
        d.set(
            "QuadPoints",
            appearance::array(a.quads.iter().flat_map(|r| {
                [
                    r.x,
                    r.y + r.height,
                    r.x + r.width,
                    r.y + r.height,
                    r.x,
                    r.y,
                    r.x + r.width,
                    r.y,
                ]
            })),
        );
    }
    if matches!(a.kind, Kind::Ink | Kind::Squiggle) {
        let points = if a.kind == Kind::Squiggle {
            appearance::squiggle(a)
        } else {
            a.points.clone()
        };
        d.set(
            "InkList",
            Object::Array(vec![appearance::array(
                points.iter().flat_map(|p| [p.x, p.y]),
            )]),
        );
    }
    if matches!(a.kind, Kind::Line | Kind::Arrow) {
        let start = a.points[0];
        let end = a.points[a.points.len() - 1];
        d.set("L", appearance::array([start.x, start.y, end.x, end.y]));
        d.set(
            "LE",
            vec![
                Object::Name(b"None".to_vec()),
                Object::Name(if a.kind == Kind::Arrow {
                    b"OpenArrow".to_vec()
                } else {
                    b"None".to_vec()
                }),
            ],
        );
    }
    if matches!(a.kind, Kind::Area | Kind::Rectangle | Kind::Ellipse) {
        let shape = Rect::between(a.points[0], a.points[a.points.len() - 1]);
        d.set(
            "RD",
            appearance::array([
                shape.x - bounds.x,
                bounds.y + bounds.height - shape.y - shape.height,
                bounds.x + bounds.width - shape.x - shape.width,
                shape.y - bounds.y,
            ]),
        );
    }
    if a.kind == Kind::Area {
        d.set("IC", appearance::array(a.color));
        d.set("BS", dictionary! {"W"=>0});
    }
    if a.kind == Kind::Note {
        d.set("Contents", unicode(&a.text));
        d.set("Name", "Note");
        d.set("Open", false);
    }
    Ok(doc.add_object(d))
}
fn rect(r: Rect) -> Object {
    appearance::array([r.x, r.y, r.x + r.width, r.y + r.height])
}
fn verify_appearance(doc: &Document, id: ObjectId) -> Result<()> {
    let d = doc.get_dictionary(id)?;
    ensure!(
        d.get(b"Type")?.as_name()? == b"Annot",
        "Missing annotation type"
    );
    let appearance = doc.dereference(d.get(b"AP")?)?.1.as_dict()?;
    let stream = doc.dereference(appearance.get(b"N")?)?.1.as_stream()?;
    ensure!(!stream.content.is_empty(), "Empty normal appearance");
    ensure!(
        stream.dict.get(b"Subtype")?.as_name()? == b"Form",
        "Normal appearance is not a Form XObject"
    );
    let bbox = stream.dict.get(b"BBox")?.as_array()?;
    ensure!(
        bbox.len() == 4 && bbox[2].as_float()? > 0. && bbox[3].as_float()? > 0.,
        "Invalid appearance BBox"
    );
    stream.dict.get(b"Resources")?.as_dict()?;
    Ok(())
}
fn signatures(doc: &Document, pages: &BTreeSet<ObjectId>) -> Result<BTreeMap<ObjectId, [u8; 32]>> {
    doc.objects
        .iter()
        .filter(|(_, v)| !matches!(v.type_name(), Ok(b"ObjStm" | b"XRef" | b"Linearized")))
        .map(|(id, value)| Ok((*id, signature(value, pages.contains(id))?)))
        .collect()
}
fn signature(object: &Object, page: bool) -> Result<[u8; 32]> {
    let mut hash = Sha256::new();
    hash_object(object, page, 0, &mut hash)?;
    Ok(hash.finalize().into())
}
fn field(hash: &mut Sha256, value: &[u8]) {
    hash.update((value.len() as u64).to_le_bytes());
    hash.update(value);
}
fn hash_dict(
    dict: &Dictionary,
    page: bool,
    stream: bool,
    depth: usize,
    hash: &mut Sha256,
) -> Result<()> {
    let mut entries: Vec<_> = dict
        .iter()
        .filter(|(k, _)| {
            !(page && k.as_slice() == b"Annots" || stream && k.as_slice() == b"Length")
        })
        .collect();
    entries.sort_by(|a, b| a.0.cmp(b.0));
    for (key, value) in entries {
        field(hash, key);
        hash_object(value, false, depth + 1, hash)?;
    }
    field(hash, b"end-dict");
    Ok(())
}
fn hash_object(object: &Object, page: bool, depth: usize, hash: &mut Sha256) -> Result<()> {
    ensure!(depth < 64, "Object nesting exceeds validation budget");
    match object {
        Object::Null => field(hash, b"null"),
        Object::Boolean(v) => field(hash, if *v { b"true" } else { b"false" }),
        Object::Integer(n) => {
            field(hash, b"number");
            field(hash, n.to_string().as_bytes());
        }
        Object::Real(n) => {
            ensure!(n.is_finite(), "Non-finite PDF number");
            field(hash, b"number");
            field(
                hash,
                if *n == 0. { "0".into() } else { n.to_string() }.as_bytes(),
            );
        }
        Object::Name(v) => {
            field(hash, b"name");
            field(hash, v);
        }
        Object::String(v, _) => {
            field(hash, b"string");
            field(hash, v);
        }
        Object::Reference((id, generation)) => {
            field(hash, b"ref");
            field(hash, &id.to_le_bytes());
            field(hash, &generation.to_le_bytes());
        }
        Object::Array(values) => {
            field(hash, b"array");
            for value in values {
                hash_object(value, false, depth + 1, hash)?;
            }
            field(hash, b"end-array");
        }
        Object::Dictionary(d) => {
            field(hash, b"dict");
            hash_dict(d, page, false, depth, hash)?;
        }
        Object::Stream(s) => {
            field(hash, b"stream");
            hash_dict(&s.dict, false, true, depth, hash)?;
            field(hash, &s.content);
        }
    }
    Ok(())
}
