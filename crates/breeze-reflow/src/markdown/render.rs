//! mdast to the HTML subset GPUI Kit's rich text renders.
//!
//! The reading surface virtualizes per top-level block, so each block is one
//! HTML fragment. Rendering reads the whole document's `References`, which is
//! what lets a link label defined in a later block resolve. This module never
//! reconstructs source: it only decides how the parsed meaning is drawn.
use super::ast::{self, References};
use markdown::mdast::{self, AlignKind, Node};
use std::fmt::Write as _;

/// Render one top-level block.
pub fn html(node: &Node, source: &str, references: &References, out: &mut String) {
    match node {
        Node::Paragraph(paragraph) => {
            out.push_str("<p>");
            inlines(&paragraph.children, source, references, out);
            out.push_str("</p>");
        }
        Node::Heading(heading) => {
            let _ = write!(out, "<h{}>", heading.depth.clamp(1, 6));
            inlines(&heading.children, source, references, out);
            let _ = write!(out, "</h{}>", heading.depth.clamp(1, 6));
        }
        Node::Blockquote(quote) => {
            out.push_str("<blockquote>");
            for child in &quote.children {
                html(child, source, references, out);
            }
            out.push_str("</blockquote>");
        }
        Node::List(list) => list_html(list, source, references, out),
        Node::Code(code) => {
            out.push_str("<pre><code");
            if let Some(lang) = &code.lang {
                push_attribute(out, "class", &format!("language-{lang}"));
            }
            out.push('>');
            escape(&code.value, out);
            out.push_str("</code></pre>");
        }
        Node::Table(table) => table_html(table, source, references, out),
        Node::ThematicBreak(_) => out.push_str("<hr>"),
        // Raw HTML is the author's: the reader has always handed it to the
        // text view's HTML parser unchanged.
        Node::Html(html) => out.push_str(&html.value),
        Node::FootnoteDefinition(definition) => {
            let number = references.note(&definition.identifier);
            let label = number.map_or_else(
                || definition.identifier.clone(),
                |number| number.to_string(),
            );
            out.push_str("<div class=\"footnote-definition\">");
            out.push_str("<span class=\"footnote-definition-label\">");
            escape(&label, out);
            out.push_str("</span>");
            for child in &definition.children {
                html(child, source, references, out);
            }
            out.push_str("</div>");
        }
        // Definitions produce no content; front matter and math stay literal.
        Node::Definition(_) => {}
        Node::Yaml(yaml) => {
            out.push_str("<p><code>");
            escape(&yaml.value, out);
            out.push_str("</code></p>");
        }
        Node::Toml(toml) => {
            out.push_str("<p><code>");
            escape(&toml.value, out);
            out.push_str("</code></p>");
        }
        Node::Math(math) => {
            out.push_str("<pre><code>");
            escape(&math.value, out);
            out.push_str("</code></pre>");
        }
        other => {
            // Unknown block containers still show their children, as the HTML
            // reader flattens unrecognised elements.
            if let Some(children) = other.children() {
                for child in children {
                    html(child, source, references, out);
                }
            }
        }
    }
}

fn list_html(list: &mdast::List, source: &str, references: &References, out: &mut String) {
    let tag = if list.ordered { "ol" } else { "ul" };
    out.push('<');
    out.push_str(tag);
    if list.ordered
        && let Some(start) = list.start
        && start != 1
    {
        push_attribute(out, "start", &start.to_string());
    }
    out.push('>');
    for item in &list.children {
        let Node::ListItem(item) = item else {
            continue;
        };
        out.push_str("<li>");
        // A tight item shows its paragraphs without paragraph margins, as the
        // reader always has.
        let tight = !list.spread && !item.spread;
        for child in &item.children {
            match (tight, child) {
                (true, Node::Paragraph(paragraph)) => {
                    inlines(&paragraph.children, source, references, out);
                }
                _ => html(child, source, references, out),
            }
        }
        out.push_str("</li>");
    }
    out.push_str("</");
    out.push_str(tag);
    out.push('>');
}

fn table_html(table: &mdast::Table, source: &str, references: &References, out: &mut String) {
    let mut rows = table.children.iter();
    out.push_str("<table>");
    if let Some(Node::TableRow(head)) = rows.next() {
        out.push_str("<thead><tr>");
        for (index, cell) in head.children.iter().enumerate() {
            let Node::TableCell(cell) = cell else {
                continue;
            };
            out.push_str("<th");
            if let Some(align) = table.align.get(index).and_then(alignment) {
                push_attribute(out, "align", align);
            }
            out.push('>');
            inlines(&cell.children, source, references, out);
            out.push_str("</th>");
        }
        out.push_str("</tr></thead>");
    }
    out.push_str("<tbody>");
    for row in rows {
        let Node::TableRow(row) = row else {
            continue;
        };
        out.push_str("<tr>");
        for (index, cell) in row.children.iter().enumerate() {
            let Node::TableCell(cell) = cell else {
                continue;
            };
            out.push_str("<td");
            if let Some(align) = table.align.get(index).and_then(alignment) {
                push_attribute(out, "align", align);
            }
            out.push('>');
            inlines(&cell.children, source, references, out);
            out.push_str("</td>");
        }
        out.push_str("</tr>");
    }
    out.push_str("</tbody></table>");
}

fn alignment(kind: &AlignKind) -> Option<&'static str> {
    match kind {
        AlignKind::Left => Some("left"),
        AlignKind::Right => Some("right"),
        AlignKind::Center => Some("center"),
        AlignKind::None => None,
    }
}

fn inlines(nodes: &[Node], source: &str, references: &References, out: &mut String) {
    for node in nodes {
        match node {
            Node::Text(text) => escape(&text.value, out),
            Node::Strong(strong) => container("strong", &strong.children, source, references, out),
            Node::Emphasis(emphasis) => {
                container("em", &emphasis.children, source, references, out);
            }
            Node::Delete(delete) => container("del", &delete.children, source, references, out),
            Node::InlineCode(code) => {
                out.push_str("<code>");
                escape(&code.value, out);
                out.push_str("</code>");
            }
            Node::Link(link) => {
                open_link(&link.url, link.title.as_deref(), out);
                inlines(&link.children, source, references, out);
                out.push_str("</a>");
            }
            Node::LinkReference(reference) => match references.definition(&reference.identifier) {
                Some(definition) => {
                    open_link(&definition.url, definition.title.as_deref(), out);
                    inlines(&reference.children, source, references, out);
                    out.push_str("</a>");
                }
                // An unresolved reference is literal source, never a link to
                // nowhere.
                None => escape(source_slice(source, node), out),
            },
            Node::Image(image) => push_image(&image.url, &image.alt, image.title.as_deref(), out),
            Node::ImageReference(reference) => match references.definition(&reference.identifier) {
                Some(definition) => {
                    push_image(
                        &definition.url,
                        &reference.alt,
                        definition.title.as_deref(),
                        out,
                    );
                }
                None => escape(source_slice(source, node), out),
            },
            Node::Break(_) => out.push_str("<br>"),
            Node::Html(html) => out.push_str(&html.value),
            Node::InlineMath(math) => {
                out.push_str("<code>");
                escape(&math.value, out);
                out.push_str("</code>");
            }
            Node::FootnoteReference(reference) => match references.note(&reference.identifier) {
                Some(number) => {
                    let _ = write!(out, "<sup class=\"footnote-reference\">{number}</sup>");
                }
                None => escape(source_slice(source, node), out),
            },
            other => {
                if let Some(children) = other.children() {
                    inlines(children, source, references, out);
                }
            }
        }
    }
}

fn container(
    tag: &str,
    children: &[Node],
    source: &str,
    references: &References,
    out: &mut String,
) {
    let _ = write!(out, "<{tag}>");
    inlines(children, source, references, out);
    let _ = write!(out, "</{tag}>");
}

fn open_link(url: &str, title: Option<&str>, out: &mut String) {
    out.push_str("<a");
    push_attribute(out, "href", url);
    if let Some(title) = title {
        push_attribute(out, "title", title);
    }
    out.push('>');
}

fn push_image(url: &str, alt: &str, title: Option<&str>, out: &mut String) {
    out.push_str("<img");
    push_attribute(out, "src", url);
    push_attribute(out, "alt", alt);
    if let Some(title) = title {
        push_attribute(out, "title", title);
    }
    out.push('>');
}

fn source_slice<'a>(source: &'a str, node: &Node) -> &'a str {
    ast::span(node)
        .and_then(|span| source.get(span))
        .unwrap_or_default()
}

fn push_attribute(out: &mut String, name: &str, value: &str) {
    out.push(' ');
    out.push_str(name);
    out.push_str("=\"");
    escape_attribute(value, out);
    out.push('"');
}

fn escape(text: &str, out: &mut String) {
    for character in text.chars() {
        match character {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            _ => out.push(character),
        }
    }
}

fn escape_attribute(text: &str, out: &mut String) {
    for character in text.chars() {
        match character {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(character),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rendered(source: &str) -> String {
        let root = ast::parse(source).expect("parses");
        let references = References::new(&root);
        let mut out = String::new();
        for node in root.children().expect("children") {
            html(node, source, &references, &mut out);
        }
        out
    }

    #[test]
    fn references_tables_and_code_render_without_losing_text() {
        let html = rendered(
            "# T\n\n[see][ref] & more\n\n[ref]: https://example.com \"ti\"\n\n| a | b |\n| :- | -: |\n| 1 | 2 |\n\n```rust\nfn f() {}\n```\n",
        );
        assert!(html.contains("<h1>T</h1>"), "{html}");
        assert!(
            html.contains("<a href=\"https://example.com\" title=\"ti\">see</a> &amp; more"),
            "{html}"
        );
        assert!(
            html.contains("<table>") && html.contains("</table>"),
            "{html}"
        );
        assert!(html.contains("<th align=\"left\">a</th>"), "{html}");
        assert!(
            html.contains("<pre><code class=\"language-rust\">fn f() {}</code></pre>"),
            "{html}"
        );
    }

    #[test]
    fn unresolved_references_and_raw_html_stay_literal() {
        let html = rendered("[label][missing]\n\n<span>raw</span>\n");
        assert!(html.contains("[label][missing]"), "{html}");
        assert!(html.contains("<span>raw</span>"), "{html}");
    }

    #[test]
    fn task_items_show_their_text_and_ordered_starts_are_kept() {
        let html = rendered("- [x] done\n- [ ] later\n\n3. three\n");
        assert!(html.contains("<li>done</li>"), "{html}");
        assert!(html.contains("<li>later</li>"), "{html}");
        assert!(html.contains("<ol start=\"3\">"), "{html}");
    }
}
