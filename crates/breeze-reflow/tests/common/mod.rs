use std::io::{Cursor, Write};
use zip::{ZipWriter, write::SimpleFileOptions};

pub fn publication(chapters: usize, paragraphs: usize) -> Vec<u8> {
    let mut archive = ZipWriter::new(Cursor::new(Vec::new()));
    let mut write = |path: &str, text: &str| {
        archive
            .start_file(path, SimpleFileOptions::default())
            .unwrap();
        archive.write_all(text.as_bytes()).unwrap();
    };
    write("mimetype", "application/epub+zip");
    write(
        "META-INF/container.xml",
        r#"<?xml version="1.0"?><container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container"><rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles></container>"#,
    );
    let manifest = (0..chapters)
        .map(|i| {
            format!(r#"<item id="ch{i}" href="ch{i}.xhtml" media-type="application/xhtml+xml"/>"#)
        })
        .collect::<String>();
    let spine = (0..chapters)
        .map(|i| format!(r#"<itemref idref="ch{i}"/>"#))
        .collect::<String>();
    write(
        "OEBPS/content.opf",
        &format!(
            r#"<?xml version="1.0"?><package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="bookid"><metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:identifier id="bookid">breeze-authored-reflow-fixture</dc:identifier><dc:title>Breeze EPUB Fixture</dc:title><dc:language>zh</dc:language><dc:creator>Breeze contributors</dc:creator><meta property="dcterms:modified">2026-10-15T00:00:00Z</meta></metadata><manifest><item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/>{manifest}</manifest><spine>{spine}</spine></package>"#
        ),
    );
    let nav = (0..chapters)
        .map(|i| format!(r#"<li><a href="ch{i}.xhtml#chapter">Chapter {i}</a></li>"#))
        .collect::<String>();
    write(
        "OEBPS/nav.xhtml",
        &format!(
            r#"<html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops"><head><title>Contents</title></head><body><nav epub:type="toc"><ol>{nav}</ol></nav></body></html>"#
        ),
    );
    for chapter in 0..chapters {
        let body = (0..paragraphs).map(|i| format!("<p>Paragraph {i}. Native reading keeps publication colors and text. 你好，Breeze。 This sentence is repeated to exercise line wrapping and page flow.</p>")).collect::<String>();
        write(
            &format!("OEBPS/ch{chapter}.xhtml"),
            &format!(
                r#"<html xmlns="http://www.w3.org/1999/xhtml"><head><title>Chapter {chapter}</title><style>h1 {{ color: #c02020; }} p {{ line-height: 1.4; }} body {{ margin: 24px; }}</style></head><body><h1 id="chapter">Chapter {chapter}</h1>{body}</body></html>"#
            ),
        );
    }
    archive.finish().unwrap().into_inner()
}
