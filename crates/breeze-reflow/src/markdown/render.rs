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
            out.push_str("<p>");
            for (index, line) in yaml.value.lines().enumerate() {
                if index > 0 {
                    out.push_str("<br>");
                }
                escape(line, out);
            }
            out.push_str("</p>");
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

/// Render only inline children, for a table cell or a task row.
pub(crate) fn inline_html(nodes: &[Node], source: &str, references: &References) -> String {
    let mut out = String::new();
    inlines(nodes, source, references, &mut out);
    out
}

fn inlines(nodes: &[Node], source: &str, references: &References, out: &mut String) {
    let mut script: Option<Script> = None;
    for node in nodes {
        match node {
            Node::Text(text) => {
                let value = match script {
                    Some(script) => script_text(&text.value, script),
                    None => emojify(&text.value),
                };
                // A source line break is a reading line break here: the note
                // was written with deliberate lines, and HTML would otherwise
                // collapse them into spaces.
                for (index, part) in value.split('\n').enumerate() {
                    if index > 0 {
                        out.push_str("<br>");
                    }
                    escape(part, out);
                }
            }
            Node::Html(html) => {
                match html.value.trim().to_ascii_lowercase().as_str() {
                    "<sup>" => {
                        script = Some(Script::Super);
                        continue;
                    }
                    "</sup>" => {
                        script = None;
                        continue;
                    }
                    "<sub>" => {
                        script = Some(Script::Sub);
                        continue;
                    }
                    "</sub>" => {
                        script = None;
                        continue;
                    }
                    _ => {}
                }
                out.push_str(&html.value);
            }
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
    escape_attribute_impl(value, out);
    out.push('"');
}

pub(crate) fn escape_text(text: &str, out: &mut String) {
    escape(text, out)
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

pub(crate) fn escape_attribute(text: &str, out: &mut String) {
    escape_attribute_impl(text, out)
}

fn escape_attribute_impl(text: &str, out: &mut String) {
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

#[derive(Clone, Copy)]
enum Script {
    Super,
    Sub,
}

/// Map a run to Unicode super/subscript characters where they exist.
///
/// GPUI Kit's HTML reader has no baseline shift, so `<sup>`/`<sub>` markup
/// would otherwise render as ordinary text. The TeX-ish forms people write
/// (`X<sub>2</sub>`, `O<sup>2</sup>`) have exact Unicode counterparts; text
/// without a counterpart keeps its characters and only loses the raise.
fn script_text(text: &str, script: Script) -> String {
    text.chars()
        .map(|character| {
            let mapped = match script {
                Script::Super => superscript(character),
                Script::Sub => subscript(character),
            };
            mapped.unwrap_or(character)
        })
        .collect()
}

fn superscript(character: char) -> Option<char> {
    Some(match character {
        '0' => '⁰',
        '1' => '¹',
        '2' => '²',
        '3' => '³',
        '4' => '⁴',
        '5' => '⁵',
        '6' => '⁶',
        '7' => '⁷',
        '8' => '⁸',
        '9' => '⁹',
        '+' => '⁺',
        '-' => '⁻',
        '=' => '⁼',
        '(' => '⁽',
        ')' => '⁾',
        'a' => 'ᵃ',
        'b' => 'ᵇ',
        'c' => 'ᶜ',
        'd' => 'ᵈ',
        'e' => 'ᵉ',
        'f' => 'ᶠ',
        'g' => 'ᵍ',
        'h' => 'ʰ',
        'i' => 'ⁱ',
        'j' => 'ʲ',
        'k' => 'ᵏ',
        'l' => 'ˡ',
        'm' => 'ᵐ',
        'n' => 'ⁿ',
        'o' => 'ᵒ',
        'p' => 'ᵖ',
        'r' => 'ʳ',
        's' => 'ˢ',
        't' => 'ᵗ',
        'u' => 'ᵘ',
        'v' => 'ᵛ',
        'w' => 'ʷ',
        'x' => 'ˣ',
        'y' => 'ʸ',
        'z' => 'ᶻ',
        'A' => 'ᴬ',
        'B' => 'ᴮ',
        'D' => 'ᴰ',
        'E' => 'ᴱ',
        'G' => 'ᴳ',
        'H' => 'ᴴ',
        'I' => 'ᴵ',
        'J' => 'ᴶ',
        'K' => 'ᴷ',
        'L' => 'ᴸ',
        'M' => 'ᴹ',
        'N' => 'ᴺ',
        'O' => 'ᴼ',
        'P' => 'ᴾ',
        'R' => 'ᴿ',
        'T' => 'ᵀ',
        'U' => 'ᵁ',
        'V' => 'ⱽ',
        'W' => 'ᵂ',
        _ => return None,
    })
}

fn subscript(character: char) -> Option<char> {
    Some(match character {
        '0' => '₀',
        '1' => '₁',
        '2' => '₂',
        '3' => '₃',
        '4' => '₄',
        '5' => '₅',
        '6' => '₆',
        '7' => '₇',
        '8' => '₈',
        '9' => '₉',
        '+' => '₊',
        '-' => '₋',
        '=' => '₌',
        '(' => '₍',
        ')' => '₎',
        'a' => 'ₐ',
        'e' => 'ₑ',
        'h' => 'ₕ',
        'i' => 'ᵢ',
        'j' => 'ⱼ',
        'k' => 'ₖ',
        'l' => 'ₗ',
        'm' => 'ₘ',
        'n' => 'ₙ',
        'o' => 'ₒ',
        'p' => 'ₚ',
        'r' => 'ᵣ',
        's' => 'ₛ',
        't' => 'ₜ',
        'u' => 'ᵤ',
        'v' => 'ᵥ',
        'x' => 'ₓ',
        _ => return None,
    })
}

/// Replace GitHub-style emoji shortcodes with their characters.
///
/// Code spans and raw HTML are separate node kinds and are left untouched; only
/// ordinary text is scanned. Unknown names stay as written.
fn emojify(text: &str) -> String {
    if !text.contains(':') {
        return text.to_owned();
    }
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find(':') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        let Some(end) = after.find(':') else {
            out.push_str(&rest[start..]);
            return out;
        };
        let name = &after[..end];
        let valid = !name.is_empty()
            && name.len() <= 32
            && name.chars().all(|character| {
                character.is_ascii_alphanumeric() || matches!(character, '_' | '-' | '+')
            });
        if valid && let Some(emoji) = emoji(name) {
            out.push_str(emoji);
            rest = &after[end + 1..];
            continue;
        }
        out.push(':');
        rest = after;
    }
    out.push_str(rest);
    out
}

fn emoji(name: &str) -> Option<&'static str> {
    Some(match name {
        "smile" => "😄",
        "smiley" => "😃",
        "grin" => "😁",
        "joy" => "😂",
        "wink" => "😉",
        "heart_eyes" => "😍",
        "thinking" => "🤔",
        "sunglasses" => "😎",
        "cry" => "😢",
        "sob" => "😭",
        "angry" => "😠",
        "rage" => "😡",
        "heart" => "❤️",
        "broken_heart" => "💔",
        "blue_heart" => "💙",
        "green_heart" => "💚",
        "yellow_heart" => "💛",
        "purple_heart" => "💜",
        "sparkling_heart" => "💖",
        "+1" | "thumbsup" => "👍",
        "-1" | "thumbsdown" => "👎",
        "clap" => "👏",
        "pray" => "🙏",
        "muscle" => "💪",
        "wave" => "👋",
        "ok_hand" => "👌",
        "point_right" => "👉",
        "point_left" => "👈",
        "eyes" => "👀",
        "fire" => "🔥",
        "rocket" => "🚀",
        "tada" => "🎉",
        "sparkles" => "✨",
        "star" => "⭐",
        "star2" => "🌟",
        "zap" => "⚡",
        "boom" => "💥",
        "bug" => "🐛",
        "beetle" => "🪲",
        "lock" => "🔒",
        "unlock" => "🔓",
        "key" => "🔑",
        "bell" => "🔔",
        "mag" => "🔍",
        "bulb" => "💡",
        "memo" => "📝",
        "book" => "📖",
        "books" => "📚",
        "bookmark" => "🔖",
        "link" => "🔗",
        "paperclip" => "📎",
        "pushpin" => "📌",
        "round_pushpin" => "📍",
        "clipboard" => "📋",
        "calendar" => "📅",
        "clock1" => "🕐",
        "hourglass" => "⌛",
        "email" => "📧",
        "phone" => "☎️",
        "computer" => "💻",
        "desktop_computer" => "🖥️",
        "keyboard" => "⌨️",
        "gear" => "⚙️",
        "wrench" => "🔧",
        "hammer" => "🔨",
        "package" => "📦",
        "inbox_tray" => "📥",
        "outbox_tray" => "📤",
        "floppy_disk" => "💾",
        "cd" => "💿",
        "dvd" => "📀",
        "camera" => "📷",
        "movie_camera" => "🎥",
        "art" => "🎨",
        "pencil2" => "✏️",
        "scissors" => "✂️",
        "triangular_ruler" => "📐",
        "chart_with_upwards_trend" => "📈",
        "chart_with_downwards_trend" => "📉",
        "bar_chart" => "📊",
        "clipboard_with_check" => "📋",
        "white_check_mark" => "✅",
        "heavy_check_mark" => "✔️",
        "x" => "❌",
        "warning" => "⚠️",
        "no_entry" => "⛔",
        "information_source" => "ℹ️",
        "question" => "❓",
        "exclamation" => "❗",
        "sunny" => "☀️",
        "cloud" => "☁️",
        "umbrella" => "☔",
        "snowflake" => "❄️",
        "coffee" => "☕",
        "beer" => "🍺",
        "cake" => "🍰",
        "apple" => "🍎",
        "car" => "🚗",
        "taxi" => "🚕",
        "bus" => "🚌",
        "train" => "🚆",
        "airplane" => "✈️",
        "ship" => "🚢",
        "bike" => "🚲",
        "house" => "🏠",
        "office" => "🏢",
        "school" => "🏫",
        "hospital" => "🏥",
        "bank" => "🏦",
        "moneybag" => "💰",
        "dollar" => "💵",
        "credit_card" => "💳",
        "gift" => "🎁",
        "trophy" => "🏆",
        "medal_sports" => "🏅",
        "soccer" => "⚽",
        "basketball" => "🏀",
        "football" => "🏈",
        "tennis" => "🎾",
        "video_game" => "🎮",
        "dice" => "🎲",
        "musical_note" => "🎵",
        "notes" => "🎶",
        "guitar" => "🎸",
        "microphone" => "🎤",
        "headphones" => "🎧",
        "globe_with_meridians" => "🌐",
        "earth_asia" => "🌏",
        "earth_americas" => "🌎",
        "earth_africa" => "🌍",
        "moon" => "🌙",
        "100" => "💯",
        "ok" => "🆗",
        "new" => "🆕",
        "free" => "🆓",
        "top" => "🔝",
        "soon" => "🔜",
        "construction" => "🚧",
        "recycle" => "♻️",
        "dart" => "🎯",
        "flag" => "🚩",
        "checkered_flag" => "🏁",
        "pinata" => "🪅",
        _ => return None,
    })
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
