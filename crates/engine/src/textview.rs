//! `TextView.markdown(...)` and `TextView.html(...)`: rich text parsed into
//! blocks of styled runs, which `convert.rs` lays out as text leaves inside
//! views. Markdown is CommonMark (pulldown-cmark); HTML is the subset a text
//! view needs — paragraphs, headings, emphasis, links, lists, quotes, code,
//! rules — with other tags ignored and their text kept, and `<script>` /
//! `<style>` dropped. CSS is not applied, as in gpui-shell.

use pulldown_cmark::{Event, HeadingLevel, Options, Parser, Tag, TagEnd};

#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct Span {
    pub text: String,
    pub bold: bool,
    pub italic: bool,
    pub strike: bool,
    pub code: bool,
    pub url: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Block {
    Paragraph(Vec<Span>),
    Heading(u8, Vec<Span>),
    Code(String),
    Quote(Vec<Block>),
    List { ordered: bool, start: u64, items: Vec<Vec<Block>> },
    Rule,
}

enum Leaf {
    Paragraph,
    Heading(u8),
    Code,
}

enum Container {
    Root,
    Quote,
    List { ordered: bool, start: u64, items: Vec<Vec<Block>> },
    Item,
}

/// Turns parser events into blocks; shared by both formats.
struct Builder {
    containers: Vec<(Container, Vec<Block>)>,
    leaf: Option<(Leaf, Vec<Span>)>,
    bold: u32,
    italic: u32,
    strike: u32,
    links: Vec<String>,
}

impl Builder {
    fn new() -> Builder {
        Builder {
            containers: vec![(Container::Root, Vec::new())],
            leaf: None,
            bold: 0,
            italic: 0,
            strike: 0,
            links: Vec::new(),
        }
    }

    fn push_block(&mut self, b: Block) {
        if let Some((_, blocks)) = self.containers.last_mut() {
            blocks.push(b);
        }
    }

    fn open_leaf(&mut self, leaf: Leaf) {
        self.close_leaf();
        self.leaf = Some((leaf, Vec::new()));
    }

    fn close_leaf(&mut self) {
        let Some((leaf, spans)) = self.leaf.take() else { return };
        let block = match leaf {
            Leaf::Code => Block::Code(spans.into_iter().map(|s| s.text).collect()),
            _ if spans.iter().all(|s| s.text.trim().is_empty()) => return,
            Leaf::Paragraph => Block::Paragraph(trim(spans)),
            Leaf::Heading(level) => Block::Heading(level, trim(spans)),
        };
        self.push_block(block);
    }

    fn text(&mut self, text: &str, code: bool) {
        if self.leaf.is_none() {
            self.open_leaf(Leaf::Paragraph);
        }
        let span = Span {
            text: text.to_string(),
            bold: self.bold > 0,
            italic: self.italic > 0,
            strike: self.strike > 0,
            code,
            url: self.links.last().cloned(),
        };
        if let Some((_, spans)) = &mut self.leaf {
            match spans.last_mut() {
                Some(last)
                    if Span { text: String::new(), ..last.clone() } == Span { text: String::new(), ..span.clone() } =>
                {
                    last.text.push_str(&span.text)
                }
                _ => spans.push(span),
            }
        }
    }

    fn open(&mut self, c: Container) {
        self.close_leaf();
        self.containers.push((c, Vec::new()));
    }

    fn close(&mut self) {
        self.close_leaf();
        if self.containers.len() <= 1 {
            return;
        }
        let Some((c, blocks)) = self.containers.pop() else { return };
        match c {
            Container::Root => {}
            Container::Quote => self.push_block(Block::Quote(blocks)),
            Container::List { ordered, start, items } => self.push_block(Block::List { ordered, start, items }),
            Container::Item => {
                if let Some((Container::List { items, .. }, _)) = self.containers.last_mut() {
                    items.push(blocks);
                } else {
                    for b in blocks {
                        self.push_block(b);
                    }
                }
            }
        }
    }

    fn finish(mut self) -> Vec<Block> {
        while self.containers.len() > 1 {
            self.close();
        }
        self.close_leaf();
        self.containers.pop().map(|(_, b)| b).unwrap_or_default()
    }
}

/// Leading and trailing whitespace of a block goes; runs inside keep theirs.
fn trim(mut spans: Vec<Span>) -> Vec<Span> {
    if let Some(first) = spans.first_mut() {
        first.text = first.text.trim_start().to_string();
    }
    if let Some(last) = spans.last_mut() {
        last.text = last.text.trim_end().to_string();
    }
    spans.retain(|s| !s.text.is_empty());
    spans
}

fn heading(level: HeadingLevel) -> u8 {
    match level {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}

pub(crate) fn markdown(source: &str) -> Vec<Block> {
    let mut b = Builder::new();
    for event in Parser::new_ext(source, Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS) {
        match event {
            Event::Start(tag) => match tag {
                Tag::Paragraph => b.open_leaf(Leaf::Paragraph),
                Tag::Heading { level, .. } => b.open_leaf(Leaf::Heading(heading(level))),
                Tag::CodeBlock(_) => b.open_leaf(Leaf::Code),
                Tag::BlockQuote(_) => b.open(Container::Quote),
                Tag::List(start) => {
                    b.open(Container::List { ordered: start.is_some(), start: start.unwrap_or(1), items: Vec::new() })
                }
                Tag::Item => b.open(Container::Item),
                Tag::Emphasis => b.italic += 1,
                Tag::Strong => b.bold += 1,
                Tag::Strikethrough => b.strike += 1,
                Tag::Link { dest_url, .. } => b.links.push(dest_url.to_string()),
                _ => {}
            },
            Event::End(end) => match end {
                TagEnd::Paragraph | TagEnd::Heading(_) | TagEnd::CodeBlock => b.close_leaf(),
                TagEnd::BlockQuote(_) | TagEnd::List(_) | TagEnd::Item => b.close(),
                TagEnd::Emphasis => b.italic = b.italic.saturating_sub(1),
                TagEnd::Strong => b.bold = b.bold.saturating_sub(1),
                TagEnd::Strikethrough => b.strike = b.strike.saturating_sub(1),
                TagEnd::Link => {
                    b.links.pop();
                }
                _ => {}
            },
            Event::Text(t) => b.text(&t, false),
            Event::Code(t) => b.text(&t, true),
            Event::SoftBreak => b.text(" ", false),
            Event::HardBreak => b.text("\n", false),
            Event::Rule => {
                b.close_leaf();
                b.push_block(Block::Rule);
            }
            Event::TaskListMarker(done) => b.text(if done { "☑ " } else { "☐ " }, false),
            _ => {}
        }
    }
    b.finish()
}

fn entity(name: &str) -> Option<char> {
    Some(match name {
        "amp" => '&',
        "lt" => '<',
        "gt" => '>',
        "quot" => '"',
        "apos" | "#39" => '\'',
        "nbsp" => '\u{a0}',
        _ => {
            let n = name.strip_prefix('#')?;
            let code = match n.strip_prefix(['x', 'X']) {
                Some(hex) => u32::from_str_radix(hex, 16).ok()?,
                None => n.parse().ok()?,
            };
            char::from_u32(code)?
        }
    })
}

fn decode(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        let after = &rest[i + 1..];
        match after.find(';').filter(|&e| e <= 10).and_then(|e| entity(&after[..e]).map(|c| (c, e))) {
            Some((c, e)) => {
                out.push(c);
                rest = &after[e + 1..];
            }
            None => {
                out.push('&');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

fn attribute(tag: &str, name: &str) -> Option<String> {
    let lower = tag.to_ascii_lowercase();
    let at = lower.find(&format!("{name}="))? + name.len() + 1;
    let value = &tag[at..];
    let (quote, value) = match value.chars().next()? {
        q @ ('"' | '\'') => (Some(q), &value[1..]),
        _ => (None, value),
    };
    let end = match quote {
        Some(q) => value.find(q)?,
        None => value.find(|c: char| c.is_whitespace() || c == '>').unwrap_or(value.len()),
    };
    Some(decode(&value[..end]))
}

pub(crate) fn html(source: &str) -> Vec<Block> {
    let mut b = Builder::new();
    let mut rest = source;
    let mut pre = 0u32;
    while !rest.is_empty() {
        let Some(open) = rest.find('<') else {
            push_html_text(&mut b, rest, pre > 0);
            break;
        };
        push_html_text(&mut b, &rest[..open], pre > 0);
        let after = &rest[open + 1..];
        if let Some(comment) = after.strip_prefix("!--") {
            rest = comment.find("-->").map_or("", |e| &comment[e + 3..]);
            continue;
        }
        let Some(close) = after.find('>') else {
            push_html_text(&mut b, &rest[open..], pre > 0);
            break;
        };
        let tag = &after[..close];
        rest = &after[close + 1..];
        let closing = tag.starts_with('/');
        let name: String = tag
            .trim_start_matches('/')
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric())
            .collect::<String>()
            .to_ascii_lowercase();
        if !closing && (name == "script" || name == "style") {
            let end = format!("</{name}");
            rest = rest.to_ascii_lowercase().find(&end).map_or("", |e| &rest[e..]);
            continue;
        }
        match (name.as_str(), closing) {
            ("p" | "div", false) => b.open_leaf(Leaf::Paragraph),
            ("p" | "div", true) => b.close_leaf(),
            ("h1" | "h2" | "h3" | "h4" | "h5" | "h6", false) => {
                b.open_leaf(Leaf::Heading(name[1..].parse().unwrap_or(1)))
            }
            ("h1" | "h2" | "h3" | "h4" | "h5" | "h6", true) => b.close_leaf(),
            ("pre", false) => {
                pre += 1;
                b.open_leaf(Leaf::Code);
            }
            ("pre", true) => {
                pre = pre.saturating_sub(1);
                b.close_leaf();
            }
            ("blockquote", false) => b.open(Container::Quote),
            ("ul" | "ol", false) => b.open(Container::List { ordered: name == "ol", start: 1, items: Vec::new() }),
            ("li", false) => b.open(Container::Item),
            ("blockquote" | "ul" | "ol" | "li", true) => b.close(),
            ("br", _) => b.text("\n", false),
            ("hr", _) => {
                b.close_leaf();
                b.push_block(Block::Rule);
            }
            ("b" | "strong", false) => b.bold += 1,
            ("b" | "strong", true) => b.bold = b.bold.saturating_sub(1),
            ("i" | "em", false) => b.italic += 1,
            ("i" | "em", true) => b.italic = b.italic.saturating_sub(1),
            ("s" | "del" | "strike", false) => b.strike += 1,
            ("s" | "del" | "strike", true) => b.strike = b.strike.saturating_sub(1),
            ("a", false) => b.links.push(attribute(tag, "href").unwrap_or_default()),
            ("a", true) => {
                b.links.pop();
            }
            ("code", false) if pre == 0 => {
                let end = rest.find("</code").unwrap_or(rest.len());
                b.text(&decode(&rest[..end]), true);
                rest = &rest[end..];
            }
            _ => {}
        }
    }
    b.finish()
}

fn push_html_text(b: &mut Builder, raw: &str, pre: bool) {
    if raw.is_empty() {
        return;
    }
    let text = decode(raw);
    if pre {
        b.text(&text, false);
        return;
    }
    let collapsed: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.is_empty() {
        // Whitespace between inline elements is one space; between blocks, nothing.
        if b.leaf.is_some() && !text.is_empty() {
            b.text(" ", false);
        }
        return;
    }
    let lead = if text.starts_with(char::is_whitespace) && b.leaf.is_some() { " " } else { "" };
    let tail = if text.ends_with(char::is_whitespace) { " " } else { "" };
    b.text(&format!("{lead}{collapsed}{tail}"), false);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plain(spans: &[Span]) -> String {
        spans.iter().map(|s| s.text.as_str()).collect()
    }

    #[test]
    fn markdown_blocks_and_spans() {
        let blocks = markdown("# Title\n\nSome **bold** and [a link](https://x.dev).\n\n- one\n- two\n\n> quoted\n\n```\ncode\n```\n\n---\n");
        assert!(matches!(&blocks[0], Block::Heading(1, s) if plain(s) == "Title"));
        let Block::Paragraph(p) = &blocks[1] else { panic!("{blocks:?}") };
        assert_eq!(plain(p), "Some bold and a link.");
        assert!(p.iter().any(|s| s.bold && s.text == "bold"));
        assert!(p.iter().any(|s| s.url.as_deref() == Some("https://x.dev") && s.text == "a link"));
        assert!(matches!(&blocks[2], Block::List { ordered: false, items, .. } if items.len() == 2));
        assert!(matches!(&blocks[3], Block::Quote(q) if q.len() == 1));
        assert_eq!(blocks[4], Block::Code("code\n".into()));
        assert_eq!(blocks[5], Block::Rule);
    }

    #[test]
    fn html_subset() {
        let blocks = html("<h2>Hi &amp; bye</h2><p>One <b>two</b> <a href=\"https://x.dev\">three</a><br>four</p><ol><li>a</li><li>b</li></ol><script>evil()</script><hr>");
        assert!(matches!(&blocks[0], Block::Heading(2, s) if plain(s) == "Hi & bye"));
        let Block::Paragraph(p) = &blocks[1] else { panic!("{blocks:?}") };
        assert_eq!(plain(p), "One two three\nfour");
        assert!(p.iter().any(|s| s.url.as_deref() == Some("https://x.dev")));
        assert!(matches!(&blocks[2], Block::List { ordered: true, items, .. } if items.len() == 2));
        assert_eq!(blocks[3], Block::Rule);
        assert_eq!(blocks.len(), 4, "the script is dropped");
    }
}
