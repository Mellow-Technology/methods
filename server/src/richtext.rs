//! Rich text as a document tree.
//!
//! Work item descriptions and comments are stored as TipTap's JSON document
//! (the ProseMirror tree), not HTML. This module is the server's half of that
//! schema (the web's half is `web/lib/richtext.ts`):
//!
//! - [`sanitize`] checks a document from a client against the schema and
//!   rebuilds it with only known nodes, marks and attributes, so what we store
//!   is always something the editor can load and safe to render.
//! - [`from_markdown`] builds a document from Markdown (what bots write);
//!   [`to_markdown`] renders one back for bots to read.
//! - [`from_html`] converts the HTML stored before this change; see
//!   [`convert_legacy`].
//!
//! Output formats are rendered from the tree, so there's never stored markup
//! to sanitize.

use std::cell::RefCell;

use html5ever::tendril::StrTendril;
use html5ever::tokenizer::{
    BufferQueue, Tag, TagKind, Token, TokenSink, TokenSinkResult, Tokenizer, TokenizerOpts,
};
use pulldown_cmark::{CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag as MdTag, TagEnd};
use serde_json::{Map, Value, json};
use sqlx::PgPool;
use uuid::Uuid;

const MAX_DEPTH: usize = 32;
const MAX_NODES: usize = 20_000;

/// What a node may contain.
#[derive(Clone, Copy, PartialEq)]
enum Content {
    Blocks,
    ListItems,
    Inline,
    /// Code blocks: unmarked text only.
    Text,
    Leaf,
}

fn content_of(ty: &str) -> Option<Content> {
    Some(match ty {
        "doc" | "blockquote" | "listItem" => Content::Blocks,
        "bulletList" | "orderedList" => Content::ListItems,
        "paragraph" | "heading" => Content::Inline,
        "codeBlock" => Content::Text,
        "horizontalRule" | "hardBreak" | "text" => Content::Leaf,
        _ => return None,
    })
}

fn is_block(ty: &str) -> bool {
    matches!(
        ty,
        "paragraph" | "heading" | "blockquote" | "bulletList" | "orderedList" | "codeBlock" | "horizontalRule"
    )
}

fn allowed_in(parent: Content, ty: &str) -> bool {
    match parent {
        Content::Blocks => is_block(ty),
        Content::ListItems => ty == "listItem",
        Content::Inline => matches!(ty, "text" | "hardBreak"),
        Content::Text => ty == "text",
        Content::Leaf => false,
    }
}

/// Web and mail links only.
pub fn safe_url(url: &str) -> bool {
    let u = url.trim().to_ascii_lowercase();
    u.starts_with("https://") || u.starts_with("http://") || u.starts_with("mailto:")
}

// ---- Validation -----------------------------------------------------------

/// Checks a client's document against the schema and returns a clean copy:
/// known nodes and marks only, known attributes only, unsafe links dropped
/// (their text is kept), empty text nodes removed. Unknown node types or a
/// node in the wrong place are an error, since they mean the client and
/// server disagree about the schema.
pub fn sanitize(doc: &Value) -> Result<Value, String> {
    if doc.get("type").and_then(Value::as_str) != Some("doc") {
        return Err("rich text must be a document (type \"doc\")".into());
    }
    let mut count = 0;
    sanitize_node(doc, 0, &mut count).map(|n| n.expect("doc is never dropped"))
}

fn sanitize_node(v: &Value, depth: usize, count: &mut usize) -> Result<Option<Value>, String> {
    *count += 1;
    if *count > MAX_NODES {
        return Err("document is too large".into());
    }
    if depth > MAX_DEPTH {
        return Err("document is nested too deeply".into());
    }
    let ty = v.get("type").and_then(Value::as_str).ok_or("every node needs a type")?;
    let content = content_of(ty).ok_or_else(|| format!("unsupported node type \"{ty}\""))?;
    let mut out = Map::new();
    out.insert("type".into(), ty.into());

    let attrs = v.get("attrs");
    let attr = |k: &str| attrs.and_then(|a| a.get(k));
    match ty {
        "heading" => {
            let level = attr("level").and_then(Value::as_u64).unwrap_or(3).clamp(2, 3);
            out.insert("attrs".into(), json!({ "level": level }));
        }
        "orderedList" => {
            let start = attr("start").and_then(Value::as_u64).unwrap_or(1).min(1_000_000);
            out.insert("attrs".into(), json!({ "start": start }));
        }
        "codeBlock" => {
            let lang = attr("language").and_then(Value::as_str).filter(|l| {
                !l.is_empty() && l.len() <= 32 && l.chars().all(|c| c.is_ascii_alphanumeric() || "_+#.-".contains(c))
            });
            out.insert("attrs".into(), json!({ "language": lang }));
        }
        _ => {}
    }

    if ty == "text" {
        let text = v.get("text").and_then(Value::as_str).unwrap_or("");
        if text.is_empty() {
            return Ok(None);
        }
        out.insert("text".into(), text.into());
        let marks = sanitize_marks(v.get("marks"))?;
        if !marks.is_empty() {
            out.insert("marks".into(), Value::Array(marks));
        }
        return Ok(Some(Value::Object(out)));
    }

    if let Some(children) = v.get("content") {
        let children = children.as_array().ok_or("content must be an array")?;
        let mut kept = Vec::with_capacity(children.len());
        for child in children {
            let cty = child.get("type").and_then(Value::as_str).unwrap_or("");
            if content_of(cty).is_none() {
                return Err(format!("unsupported node type \"{cty}\""));
            }
            if !allowed_in(content, cty) {
                return Err(format!("\"{cty}\" can't go inside \"{ty}\""));
            }
            if let Some(mut c) = sanitize_node(child, depth + 1, count)? {
                if content == Content::Text {
                    // Code is plain: no bold or links inside code blocks.
                    if let Some(o) = c.as_object_mut() {
                        o.remove("marks");
                    }
                }
                kept.push(c);
            }
        }
        if !kept.is_empty() {
            out.insert("content".into(), Value::Array(kept));
        }
    }

    // Drop the empty paragraphs editors keep at the end for the caret.
    if ty == "doc" {
        if let Some(Value::Array(c)) = out.get_mut("content") {
            while c.last().is_some_and(|n| n["type"] == "paragraph" && n.get("content").is_none()) {
                c.pop();
            }
            if c.is_empty() {
                out.remove("content");
            }
        }
    }

    // Containers that ProseMirror requires to be non-empty.
    let empty = out.get("content").is_none();
    match ty {
        "bulletList" | "orderedList" | "blockquote" if empty => return Ok(None),
        "listItem" if empty => {
            out.insert("content".into(), json!([{ "type": "paragraph" }]));
        }
        _ => {}
    }
    Ok(Some(Value::Object(out)))
}

fn sanitize_marks(marks: Option<&Value>) -> Result<Vec<Value>, String> {
    let Some(marks) = marks else { return Ok(vec![]) };
    let marks = marks.as_array().ok_or("marks must be an array")?;
    let mut out = Vec::new();
    for m in marks {
        let ty = m.get("type").and_then(Value::as_str).unwrap_or("");
        match ty {
            "bold" | "italic" | "strike" | "code" | "underline" => out.push(json!({ "type": ty })),
            "link" => {
                let href = m.pointer("/attrs/href").and_then(Value::as_str).unwrap_or("");
                if safe_url(href) {
                    out.push(json!({ "type": "link", "attrs": { "href": href.trim() } }));
                }
            }
            _ => return Err(format!("unsupported mark \"{ty}\"")),
        }
    }
    Ok(out)
}

/// True when a document has nothing worth keeping.
pub fn is_empty(doc: &Value) -> bool {
    match doc.get("type").and_then(Value::as_str) {
        Some("text") => doc.get("text").and_then(Value::as_str).unwrap_or("").trim().is_empty(),
        Some("horizontalRule") => false,
        _ => doc
            .get("content")
            .and_then(Value::as_array)
            .is_none_or(|c| c.iter().all(is_empty)),
    }
}

/// Rich text as clients send it: a document, or a Markdown string (bots, or
/// anyone scripting the API). Returns a clean document, or `None` if empty.
pub fn from_input(v: &Value) -> Result<Option<Value>, String> {
    let doc = match v {
        Value::Null => return Ok(None),
        Value::String(s) => from_markdown(s),
        Value::Object(_) => sanitize(v)?,
        _ => return Err("rich text must be a document or a Markdown string".into()),
    };
    Ok((!is_empty(&doc)).then_some(doc))
}

// ---- Building documents ------------------------------------------------------

/// Builds a document from a stream of open/close/text calls, as both the
/// Markdown and HTML converters produce. Lenient: text outside a paragraph
/// gets one, and blocks opened inside a paragraph close it first.
struct Builder {
    stack: Vec<(Map<String, Value>, bool)>, // (node, opened implicitly)
    marks: Vec<Value>,
}

impl Builder {
    fn new() -> Self {
        let mut doc = Map::new();
        doc.insert("type".into(), "doc".into());
        Builder { stack: vec![(doc, false)], marks: vec![] }
    }

    fn top_type(&self) -> &str {
        self.stack.last().and_then(|(n, _)| n.get("type")).and_then(Value::as_str).unwrap_or("doc")
    }

    fn top_content(&self) -> Content {
        content_of(self.top_type()).unwrap_or(Content::Blocks)
    }

    fn append(&mut self, node: Value) {
        let (parent, _) = self.stack.last_mut().expect("doc stays on the stack");
        parent.entry("content").or_insert_with(|| Value::Array(vec![])).as_array_mut().unwrap().push(node);
    }

    fn pop(&mut self) {
        if self.stack.len() > 1 {
            let (node, _) = self.stack.pop().unwrap();
            self.append(Value::Object(node));
        }
    }

    /// Opens a block, first closing anything that can't hold it.
    fn open(&mut self, ty: &str, attrs: Option<Value>) {
        loop {
            let c = self.top_content();
            if allowed_in(c, ty) {
                break;
            }
            if ty == "listItem" && c == Content::Blocks {
                // A stray list item: give it a list.
                self.open("bulletList", None);
                break;
            }
            if c == Content::ListItems {
                // A block straight inside a list: give it an item.
                self.open("listItem", None);
                break;
            }
            if self.stack.len() == 1 {
                break;
            }
            self.pop();
        }
        let mut node = Map::new();
        node.insert("type".into(), ty.into());
        if let Some(a) = attrs {
            node.insert("attrs".into(), a);
        }
        self.stack.push((node, false));
    }

    /// Closes the innermost open `ty` (and anything opened inside it).
    fn close(&mut self, ty: &str) {
        if let Some(i) = self.stack.iter().rposition(|(n, _)| n.get("type").and_then(Value::as_str) == Some(ty)) {
            if i == 0 {
                return;
            }
            while self.stack.len() > i {
                self.pop();
            }
        }
    }

    fn ensure_inline(&mut self) {
        if !matches!(self.top_content(), Content::Inline | Content::Text) {
            if self.top_content() == Content::ListItems {
                self.open("listItem", None);
            }
            let mut p = Map::new();
            p.insert("type".into(), "paragraph".into());
            self.stack.push((p, true));
        }
    }

    fn text(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        self.ensure_inline();
        let marks = if self.top_content() == Content::Inline { self.marks.clone() } else { vec![] };
        // Extend the previous text node when it has the same marks.
        let (parent, _) = self.stack.last_mut().unwrap();
        if let Some(Value::Object(last)) = parent.get_mut("content").and_then(|c| c.as_array_mut()).and_then(|c| c.last_mut()) {
            let same = last.get("type").and_then(Value::as_str) == Some("text")
                && last.get("marks").and_then(Value::as_array).map(Vec::as_slice).unwrap_or(&[]) == marks.as_slice();
            if same {
                if let Some(Value::String(t)) = last.get_mut("text") {
                    t.push_str(text);
                    return;
                }
            }
        }
        let mut node = json!({ "type": "text", "text": text });
        if !marks.is_empty() {
            node["marks"] = Value::Array(marks);
        }
        self.append(node);
    }

    fn hard_break(&mut self) {
        self.ensure_inline();
        if self.top_content() == Content::Text {
            self.append(json!({ "type": "text", "text": "\n" }));
        } else {
            self.append(json!({ "type": "hardBreak" }));
        }
    }

    fn rule(&mut self) {
        self.open("horizontalRule", None);
        self.pop();
    }

    fn push_mark(&mut self, mark: Value) {
        self.marks.push(mark);
    }

    fn pop_mark(&mut self, ty: &str) {
        if let Some(i) = self.marks.iter().rposition(|m| m["type"] == ty) {
            self.marks.remove(i);
        }
    }

    fn in_block(&self, ty: &str) -> bool {
        self.stack.iter().any(|(n, _)| n.get("type").and_then(Value::as_str) == Some(ty))
    }

    fn finish(mut self) -> Value {
        while self.stack.len() > 1 {
            self.pop();
        }
        let doc = Value::Object(self.stack.pop().unwrap().0);
        // Our own output always fits the schema; this tidies empties.
        sanitize(&doc).unwrap_or_else(|_| json!({ "type": "doc" }))
    }
}

// ---- Markdown ---------------------------------------------------------------

/// Builds a document from Markdown (CommonMark with strikethrough). Single
/// newlines are kept as line breaks, as people expect in comments.
pub fn from_markdown(md: &str) -> Value {
    let mut b = Builder::new();
    for event in Parser::new_ext(md.trim(), Options::ENABLE_STRIKETHROUGH) {
        match event {
            Event::Start(tag) => match tag {
                MdTag::Paragraph => b.open("paragraph", None),
                MdTag::Heading { level, .. } => {
                    let level = if level == HeadingLevel::H1 || level == HeadingLevel::H2 { 2 } else { 3 };
                    b.open("heading", Some(json!({ "level": level })));
                }
                MdTag::BlockQuote(_) => b.open("blockquote", None),
                MdTag::CodeBlock(kind) => {
                    let lang = match kind {
                        CodeBlockKind::Fenced(info) => info.split_whitespace().next().map(str::to_string),
                        CodeBlockKind::Indented => None,
                    };
                    b.open("codeBlock", Some(json!({ "language": lang })));
                }
                MdTag::List(Some(start)) => b.open("orderedList", Some(json!({ "start": start }))),
                MdTag::List(None) => b.open("bulletList", None),
                MdTag::Item => b.open("listItem", None),
                MdTag::Emphasis => b.push_mark(json!({ "type": "italic" })),
                MdTag::Strong => b.push_mark(json!({ "type": "bold" })),
                MdTag::Strikethrough => b.push_mark(json!({ "type": "strike" })),
                MdTag::Link { dest_url, .. } => {
                    b.push_mark(json!({ "type": "link", "attrs": { "href": dest_url.to_string() } }))
                }
                _ => {} // images become their alt text; tables etc. aren't enabled
            },
            Event::End(tag) => match tag {
                TagEnd::Paragraph => b.close("paragraph"),
                TagEnd::Heading(_) => b.close("heading"),
                TagEnd::BlockQuote(_) => b.close("blockquote"),
                TagEnd::CodeBlock => {
                    // Fenced code ends with a newline we don't want to keep.
                    if let Some((node, _)) = b.stack.last_mut() {
                        if let Some(Value::Array(c)) = node.get_mut("content") {
                            if let Some(Value::String(t)) = c.last_mut().and_then(|t| t.get_mut("text")) {
                                if t.ends_with('\n') {
                                    t.pop();
                                }
                            }
                        }
                    }
                    b.close("codeBlock");
                }
                TagEnd::List(true) => b.close("orderedList"),
                TagEnd::List(false) => b.close("bulletList"),
                TagEnd::Item => b.close("listItem"),
                TagEnd::Emphasis => b.pop_mark("italic"),
                TagEnd::Strong => b.pop_mark("bold"),
                TagEnd::Strikethrough => b.pop_mark("strike"),
                TagEnd::Link => b.pop_mark("link"),
                _ => {}
            },
            Event::Text(t) | Event::Html(t) | Event::InlineHtml(t) => b.text(&t),
            Event::Code(t) => {
                b.push_mark(json!({ "type": "code" }));
                b.text(&t);
                b.pop_mark("code");
            }
            Event::SoftBreak | Event::HardBreak => b.hard_break(),
            Event::Rule => b.rule(),
            _ => {}
        }
    }
    b.finish()
}

/// Renders a document as Markdown, for bots to read.
pub fn to_markdown(doc: &Value) -> String {
    let mut out = String::new();
    blocks_md(doc, &mut out);
    out.trim_end().to_string()
}

fn children(v: &Value) -> &[Value] {
    v.get("content").and_then(Value::as_array).map(Vec::as_slice).unwrap_or(&[])
}

fn blocks_md(parent: &Value, out: &mut String) {
    let in_item = parent.get("type").and_then(Value::as_str) == Some("listItem");
    for (i, node) in children(parent).iter().enumerate() {
        let ty = node.get("type").and_then(Value::as_str).unwrap_or("");
        // Blank lines between blocks, except before a list nested in an
        // item, which keeps the outer list tight.
        if i > 0 && !(in_item && ty.ends_with("List")) {
            out.push('\n');
        }
        block_md(node, out);
    }
}

/// Indents every line after the first, for list item continuations.
fn indent_rest(s: &str, pad: &str) -> String {
    s.lines().enumerate().map(|(i, l)| if i == 0 || l.is_empty() { l.to_string() } else { format!("{pad}{l}") }).collect::<Vec<_>>().join("\n")
}

fn block_md(node: &Value, out: &mut String) {
    let ty = node.get("type").and_then(Value::as_str).unwrap_or("");
    match ty {
        "paragraph" => {
            out.push_str(&inline_md(node));
            out.push('\n');
        }
        "heading" => {
            let level = node.pointer("/attrs/level").and_then(Value::as_u64).unwrap_or(3) as usize;
            out.push_str(&"#".repeat(level));
            out.push(' ');
            out.push_str(&inline_md(node));
            out.push('\n');
        }
        "blockquote" => {
            let mut inner = String::new();
            blocks_md(node, &mut inner);
            for line in inner.trim_end().lines() {
                out.push_str(if line.is_empty() { ">" } else { "> " });
                out.push_str(line);
                out.push('\n');
            }
        }
        "bulletList" | "orderedList" => {
            let mut n = node.pointer("/attrs/start").and_then(Value::as_u64).unwrap_or(1);
            for item in children(node) {
                let marker = if ty == "bulletList" { "- ".to_string() } else { format!("{n}. ") };
                n += 1;
                let mut inner = String::new();
                blocks_md(item, &mut inner);
                out.push_str(&marker);
                out.push_str(&indent_rest(inner.trim_end(), &" ".repeat(marker.len())));
                out.push('\n');
            }
        }
        "codeBlock" => {
            let lang = node.pointer("/attrs/language").and_then(Value::as_str).unwrap_or("");
            let code: String = children(node).iter().filter_map(|t| t.get("text").and_then(Value::as_str)).collect();
            out.push_str(&format!("```{lang}\n{code}\n```\n"));
        }
        "horizontalRule" => out.push_str("---\n"),
        _ => {}
    }
}

fn inline_md(node: &Value) -> String {
    let mut s = String::new();
    for child in children(node) {
        match child.get("type").and_then(Value::as_str) {
            // A plain newline: from_markdown reads single newlines as breaks.
            Some("hardBreak") => s.push('\n'),
            Some("text") => {
                let mut text = child.get("text").and_then(Value::as_str).unwrap_or("").to_string();
                let mut href = None;
                for m in child.get("marks").and_then(Value::as_array).into_iter().flatten() {
                    text = match m.get("type").and_then(Value::as_str) {
                        Some("bold") => format!("**{text}**"),
                        Some("italic") => format!("_{text}_"),
                        Some("strike") => format!("~~{text}~~"),
                        Some("code") => format!("`{text}`"),
                        Some("link") => {
                            href = m.pointer("/attrs/href").and_then(Value::as_str);
                            text
                        }
                        _ => text,
                    };
                }
                match href {
                    Some(h) => s.push_str(&format!("[{text}]({h})")),
                    None => s.push_str(&text),
                }
            }
            _ => {}
        }
    }
    s
}

// ---- Legacy HTML ------------------------------------------------------------

struct HtmlSink(RefCell<Builder>);

impl TokenSink for HtmlSink {
    type Handle = ();

    fn process_token(&self, token: Token, _line: u64) -> TokenSinkResult<()> {
        let mut b = self.0.borrow_mut();
        match token {
            Token::TagToken(Tag { kind, name, attrs, self_closing, .. }) => {
                let start = kind == TagKind::StartTag;
                let block = |t: &str| -> Option<(&'static str, Option<Value>)> {
                    Some(match t {
                        "p" | "div" => ("paragraph", None),
                        "h1" | "h2" => ("heading", Some(json!({ "level": 2 }))),
                        "h3" | "h4" | "h5" | "h6" => ("heading", Some(json!({ "level": 3 }))),
                        "ul" => ("bulletList", None),
                        "ol" => ("orderedList", Some(json!({ "start": 1 }))),
                        "li" => ("listItem", None),
                        "blockquote" => ("blockquote", None),
                        "pre" => ("codeBlock", Some(json!({ "language": null }))),
                        _ => return None,
                    })
                };
                let mark = match &*name {
                    "strong" | "b" => Some("bold"),
                    "em" | "i" => Some("italic"),
                    "s" | "strike" | "del" => Some("strike"),
                    "u" => Some("underline"),
                    "code" if !b.in_block("codeBlock") => Some("code"),
                    "a" => Some("link"),
                    _ => None,
                };
                match (&*name, start) {
                    ("br", _) => b.hard_break(),
                    ("hr", true) => b.rule(),
                    (t, true) if block(t).is_some() => {
                        let (ty, a) = block(t).unwrap();
                        b.open(ty, a);
                        if self_closing {
                            b.close(ty);
                        }
                    }
                    (t, false) if block(t).is_some() => b.close(block(t).unwrap().0),
                    (_, true) if mark.is_some() => {
                        let ty = mark.unwrap();
                        if ty == "link" {
                            let href = attrs.iter().find(|a| &*a.name.local == "href").map(|a| a.value.to_string());
                            b.push_mark(json!({ "type": "link", "attrs": { "href": href.unwrap_or_default() } }));
                        } else {
                            b.push_mark(json!({ "type": ty }));
                        }
                    }
                    (_, false) if mark.is_some() => b.pop_mark(mark.unwrap()),
                    _ => {}
                }
            }
            Token::CharacterTokens(t) => {
                if b.in_block("codeBlock") {
                    b.text(&t);
                } else {
                    // Collapse whitespace as a browser would, and skip the
                    // whitespace between blocks.
                    let collapsed = t.split_whitespace().collect::<Vec<_>>().join(" ");
                    let lead = t.starts_with(char::is_whitespace) && !collapsed.is_empty();
                    let trail = t.ends_with(char::is_whitespace) && !collapsed.is_empty();
                    let inline = matches!(b.top_content(), Content::Inline);
                    let text = if collapsed.is_empty() {
                        if inline { " ".to_string() } else { String::new() }
                    } else {
                        format!("{}{}{}", if lead { " " } else { "" }, collapsed, if trail { " " } else { "" })
                    };
                    b.text(&text);
                }
            }
            _ => {}
        }
        TokenSinkResult::Continue
    }
}

/// Converts HTML from the old editor into a document.
pub fn from_html(html: &str) -> Value {
    let sink = HtmlSink(RefCell::new(Builder::new()));
    let tok = Tokenizer::new(sink, TokenizerOpts::default());
    let input = BufferQueue::default();
    input.push_back(StrTendril::from(html));
    let _ = tok.feed(&input);
    tok.end();
    let b = tok.sink.0.into_inner();
    trim_doc(b.finish())
}

/// Trims whitespace left at the edges of paragraphs by HTML formatting.
fn trim_doc(mut v: Value) -> Value {
    fn walk(v: &mut Value) {
        let ty = v.get("type").and_then(Value::as_str).unwrap_or("").to_string();
        if let Some(Value::Array(c)) = v.get_mut("content") {
            if ty == "paragraph" || ty == "heading" {
                for n in c.iter_mut() {
                    if let Some(Value::String(t)) = n.get_mut("text") {
                        while t.contains("  ") {
                            *t = t.replace("  ", " ");
                        }
                    }
                }
                if let Some(t) = c.first_mut().and_then(|n| n.get_mut("text")) {
                    *t = t.as_str().unwrap_or("").trim_start().into();
                }
                if let Some(t) = c.last_mut().and_then(|n| n.get_mut("text")) {
                    *t = t.as_str().unwrap_or("").trim_end().into();
                }
            }
            if ty == "codeBlock" {
                if let Some(Value::String(t)) = c.last_mut().and_then(|n| n.get_mut("text")) {
                    if t.ends_with('\n') {
                        t.pop();
                    }
                }
            }
            c.iter_mut().for_each(walk);
        }
    }
    walk(&mut v);
    sanitize(&v).unwrap_or(v)
}

/// Converts rows still holding HTML from before rich text became JSON
/// (migration 20261102000002 renamed those columns `legacy_*_html`). Runs at
/// start-up; each row is converted once, then its HTML is cleared.
pub async fn convert_legacy(db: &PgPool) -> anyhow::Result<()> {
    let items: Vec<(Uuid, String)> =
        sqlx::query_as("SELECT id, legacy_description_html FROM work_items WHERE legacy_description_html IS NOT NULL")
            .fetch_all(db)
            .await?;
    for (id, html) in &items {
        let doc = from_html(html);
        sqlx::query("UPDATE work_items SET description = $2, legacy_description_html = NULL WHERE id = $1")
            .bind(id)
            .bind((!is_empty(&doc)).then_some(doc))
            .execute(db)
            .await?;
    }
    let comments: Vec<(Uuid, String)> =
        sqlx::query_as("SELECT id, legacy_body_html FROM comments WHERE legacy_body_html IS NOT NULL")
            .fetch_all(db)
            .await?;
    for (id, html) in &comments {
        sqlx::query("UPDATE comments SET body = $2, legacy_body_html = NULL WHERE id = $1")
            .bind(id)
            .bind(from_html(html))
            .execute(db)
            .await?;
    }
    if !items.is_empty() || !comments.is_empty() {
        tracing::info!(items = items.len(), comments = comments.len(), "converted legacy HTML rich text to documents");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markdown_round_trip() {
        let md = "## Plan\n\nSome **bold** and _italic_ text with a [link](https://example.com).\n\n- one\n- two\n  - nested\n\n1. first\n2. second\n\n> quoted\n\n```rust\nfn main() {}\n```\n\n---\n\nline one\nline two";
        let doc = from_markdown(md);
        assert_eq!(to_markdown(&doc), md);
    }

    #[test]
    fn plain_text_becomes_paragraphs() {
        let doc = from_markdown("Hello there\nsecond line\n\nNew paragraph");
        let c = children(&doc);
        assert_eq!(c.len(), 2);
        assert_eq!(children(&c[0])[1]["type"], "hardBreak");
    }

    #[test]
    fn sanitize_drops_unsafe_links_and_unknown_attrs() {
        let doc = json!({ "type": "doc", "content": [{ "type": "paragraph", "attrs": { "onclick": "x" }, "content": [
            { "type": "text", "text": "hi", "marks": [{ "type": "link", "attrs": { "href": "javascript:alert(1)" } }] }
        ]}]});
        let clean = sanitize(&doc).unwrap();
        assert_eq!(clean, json!({ "type": "doc", "content": [{ "type": "paragraph", "content": [{ "type": "text", "text": "hi" }] }] }));
    }

    #[test]
    fn sanitize_rejects_unknown_and_misplaced_nodes() {
        assert!(sanitize(&json!({ "type": "doc", "content": [{ "type": "image" }] })).is_err());
        assert!(sanitize(&json!({ "type": "doc", "content": [{ "type": "text", "text": "loose" }] })).is_err());
        assert!(sanitize(&json!({ "type": "paragraph" })).is_err());
    }

    #[test]
    fn empty_documents() {
        assert!(is_empty(&json!({ "type": "doc", "content": [{ "type": "paragraph" }] })));
        assert!(from_input(&json!("   ")).unwrap().is_none());
        assert!(!is_empty(&from_markdown("---")));
    }

    #[test]
    fn legacy_html() {
        let doc = from_html("<h3>Title</h3><p>Some <strong>bold</strong> &amp; <a href=\"https://x.dev\">link</a><br>next</p><ul><li>a</li><li>b</li></ul><pre>let x = 1;\n</pre><p><a href=\"javascript:1\">bad</a></p>");
        assert_eq!(
            to_markdown(&doc),
            "### Title\n\nSome **bold** & [link](https://x.dev)\nnext\n\n- a\n- b\n\n```\nlet x = 1;\n```\n\nbad"
        );
    }
}
