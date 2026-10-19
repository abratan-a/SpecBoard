//! A ui section's mock (`specboard://SpecBoard/Docs/Design:CoreApi`「モックを組み上げる」): its parts put
//! together into one HTML page, each where it sits on the section's diagram and as big as its box
//! there. `section_read`'s `as: mock`, the picture of it and the export are all this page, and the
//! screen's map draws each part from its face (below), made here alone so they never differ.
//!
//! The page keeps to what the app lets a card's page do (`pages::PAGE_POLICY`: no script, nothing
//! from outside). The grid is the diagram's (`grid::unit_points`), a point a CSS pixel, with its
//! origin at the top left of the box round the parts. An html part's body goes into a declarative
//! shadow root, so its CSS holds inside the part alone; an svg part is an image of its body, fitted
//! to the box keeping its proportions. A frame holds the mock of the section it points at at 1:1
//! from its top left, clipped to its box; frames inside frames are drawn too, `DEPTH` deep, and past
//! that — or when a section would be drawn inside itself — the frame holds a box naming its target.
//! A note card is not drawn.
//!
//! Over the parts, for whoever looks at the mock and never in an export (`Overlay`): a part's own
//! note (`note`) as a sticky note at the corner of its box its `notePlace` names, and a dot at the top
//! right of a part to look at again (`review`), the part saying the cards that changed in
//! `data-review`. They are in the box's own section, so a frame's parts' notes are cut to the frame
//! as its parts are.
//!
//! A part's face (`faces`): a page of that part alone, drawn as the mock draws it, as big as its box
//! and from the page's top left, on nothing (transparent), with nothing laid over it. The screen's
//! map draws a ui section's parts from these, so the map and the mock are one drawing; it takes a
//! new picture when a face's page changes, so the same part gives the same page, byte for byte.

use super::Project;
use super::pages::PAGE_POLICY;
use crate::model::Card;
use crate::tools::grid::{self, Layout};
use crate::tools::review::Marks;

/// How many frames deep a mock draws the sections they hold.
pub const DEPTH: usize = 4;

/// One ui section's page, and what it holds.
#[derive(Clone, Debug, PartialEq)]
pub struct Mock {
    pub html: String,
    /// The page's size in CSS pixels: the box round the section's parts.
    pub width: i64,
    pub height: i64,
    /// The html and svg parts drawn, those inside frames too.
    pub parts: usize,
    /// The frames drawn, those inside frames too.
    pub frames: usize,
}

/// The rules every mock has: the boxes placed as the grid says, a part cut to its box, an svg fitted
/// in its box, a frame's stand-in marked as one.
const STYLE: &str = "html,body{margin:0;padding:0}\
.sb-mock{position:relative;overflow:hidden}\
.sb-part{position:absolute;overflow:hidden;box-sizing:border-box}\
.sb-part>img{display:block;width:100%;height:100%;object-fit:contain}\
.sb-cut{box-sizing:border-box;width:100%;height:100%;display:flex;align-items:center;justify-content:center;\
border:1px dashed #8C959F;color:#57606A;font:12px -apple-system,sans-serif;background:#F6F8FA}";

/// What is laid over a part for whoever looks at the mock: a note, and the dot of a part to look
/// at again. A part's own box is kept to itself (`isolation`), so nothing its page puts above its
/// own content comes over them.
const LAYER_STYLE: &str = ".sb-part{isolation:isolate}\
.sb-note{position:absolute;z-index:2;box-sizing:border-box;max-width:240px;padding:4px 6px;\
background:#FFF4B0;color:#3D3A1E;border:1px solid #E3C94F;border-radius:2px;\
box-shadow:0 1px 3px rgba(0,0,0,.25);font:11px/1.35 -apple-system,sans-serif;\
white-space:pre-wrap;overflow-wrap:anywhere}\
.sb-review{position:absolute;z-index:3;width:8px;height:8px;margin:4px;border-radius:50%;\
background:#F97316;box-shadow:0 0 0 1.5px #FFFFFF}";

/// What a face's page adds to the mock's rules: no ground of its own, so what the part does not draw
/// shows what is under it on the map.
const FACE_STYLE: &str = "html,body{background:transparent}";

/// One part's own page (`faces`).
#[derive(Clone, Debug, PartialEq)]
pub struct Face {
    /// The card's path.
    pub path: String,
    pub kind: String,
    /// Its box's size in CSS pixels.
    pub width: i64,
    pub height: i64,
    pub html: String,
}

/// What a mock lays over its parts: none in an export, which is the page alone.
#[derive(Clone, Copy, Default)]
pub struct Overlay<'r> {
    /// Each part's note, where its `notePlace` says.
    pub notes: bool,
    /// The parts to look at again, by id: each one's dot and `data-review`.
    pub review: Option<&'r Marks>,
}

/// The corners a note may sit at, as a part's `notePlace` names them; any other is the first one
/// not hidden (`topRight`, its kind's default).
const PLACES: [&str; 4] = ["topLeft", "topRight", "bottomLeft", "bottomRight"];

/// The mock of the ui section `section` (an id), laid out on the project's grid: the page alone.
pub fn assemble(project: &Project, layout: &Layout, section: &str) -> Mock {
    assemble_over(project, layout, section, Overlay::default())
}

/// The mock of the ui section `section` with `overlay` over its parts.
pub fn assemble_over(project: &Project, layout: &Layout, section: &str, overlay: Overlay) -> Mock {
    let mut made = Made::new(project, layout, overlay);
    let path = project.directory.full_section(section);
    let (inside, width, height) = made.section(section, &mut vec![section.to_string()]);
    let layer = if made.layered { LAYER_STYLE } else { "" };
    let html = page(&path, layer, &inside);
    Mock {
        html,
        width,
        height,
        parts: made.parts,
        frames: made.frames,
    }
}

/// The face of each part directly in the ui section `section` (an id), placed or not, in the order
/// the mock draws them: each one's page alone, a frame holding its target's mock as the mock has it.
pub fn faces(project: &Project, layout: &Layout, section: &str) -> Vec<Face> {
    let mut made = Made::new(project, layout, Overlay::default());
    let named = attribute(&project.directory.full_section(section));
    made.boxes(section)
        .iter()
        .map(|b| {
            let (width, height) = (made.across(b.size.0), made.down(b.size.1));
            let path = project.full(b.card);
            let style = format!(
                "left:0px;top:0px;width:{}px;height:{}px",
                number(width),
                number(height)
            );
            let part = made.drawn(b, &attribute(&path), "", &style, &mut vec![section.to_string()]);
            let inside = format!(
                "<div class=\"sb-mock\" data-section=\"{named}\" style=\"width:{}px;height:{}px\">{part}\n</div>",
                number(width),
                number(height)
            );
            Face {
                html: page(&path, FACE_STYLE, &inside),
                path,
                kind: b.card.kind.clone(),
                width: width.ceil() as i64,
                height: height.ceil() as i64,
            }
        })
        .collect()
}

/// A whole page: the mock's head, its rules with `more` after them, and `inside` as its body.
fn page(title: &str, more: &str, inside: &str) -> String {
    format!(
        "<!doctype html>\n<html>\n<head>\n<meta charset=\"utf-8\">\n\
         <meta http-equiv=\"Content-Security-Policy\" content=\"{PAGE_POLICY}\">\n\
         <title>{}</title>\n<style>{STYLE}{more}</style>\n</head>\n<body>\n{inside}\n</body>\n</html>\n",
        escaped(title)
    )
}

/// A box on the grid: where it sits and how big it is, in grid units.
struct Placed<'a> {
    card: &'a Card,
    at: (i64, i64),
    size: (i64, i64),
}

/// What a mock is made of, and what it has drawn so far.
struct Made<'p, 'a> {
    project: &'p Project<'a>,
    layout: &'p Layout,
    /// Points a grid unit across and down.
    unit: (f64, f64),
    overlay: Overlay<'p>,
    /// Whether anything was laid over a part.
    layered: bool,
    parts: usize,
    frames: usize,
}

impl<'p, 'a> Made<'p, 'a> {
    fn new(project: &'p Project<'a>, layout: &'p Layout, overlay: Overlay<'p>) -> Self {
        Made {
            project,
            layout,
            unit: grid::unit_points(layout),
            overlay,
            layered: false,
            parts: 0,
            frames: 0,
        }
    }

    /// The parts and frames of `section`, in the project's order, each where it sits: one no one
    /// placed goes below those placed, one under another, as it has no place to be drawn at.
    fn boxes(&self, section: &str) -> Vec<Placed<'a>> {
        let project = self.project;
        let drawn: Vec<&'a Card> = project
            .directory
            .members(section)
            .iter()
            .map(|&i| project.nth(i))
            .filter(|c| matches!(c.kind.as_str(), "html" | "svg") || c.kind == crate::kinds::FRAME)
            .collect();
        let size = |card: &Card| {
            (
                grid::card_columns(card, project.defs.cards()),
                grid::card_rows(card, project, self.layout),
            )
        };
        let mut out: Vec<Placed<'a>> = drawn
            .iter()
            .filter_map(|&card| {
                Some(Placed {
                    card,
                    at: (card.pos_x?.round() as i64, card.pos_y?.round() as i64),
                    size: size(card),
                })
            })
            .collect();
        let left = out.iter().map(|b| b.at.0).min().unwrap_or(0);
        let mut below = out
            .iter()
            .map(|b| b.at.1 + b.size.1)
            .max()
            .map_or(0, |y| y + self.layout.margin);
        for &card in drawn
            .iter()
            .filter(|c| c.pos_x.is_none() || c.pos_y.is_none())
        {
            let (w, h) = size(card);
            out.push(Placed {
                card,
                at: (left, below),
                size: (w, h),
            });
            below += h + self.layout.margin;
        }
        out
    }

    /// `section`'s box and what is in it, with its size in pixels: `chain` the sections drawn
    /// round it, itself last.
    fn section(&mut self, section: &str, chain: &mut Vec<String>) -> (String, i64, i64) {
        let boxes = self.boxes(section);
        let left = boxes.iter().map(|b| b.at.0).min().unwrap_or(0);
        let top = boxes.iter().map(|b| b.at.1).min().unwrap_or(0);
        let right = boxes.iter().map(|b| b.at.0 + b.size.0).max().unwrap_or(0);
        let bottom = boxes.iter().map(|b| b.at.1 + b.size.1).max().unwrap_or(0);
        let width = self.across(right - left);
        let height = self.down(bottom - top);
        let mut out = format!(
            "<div class=\"sb-mock\" data-section=\"{}\" style=\"width:{}px;height:{}px\">",
            attribute(&self.project.directory.full_section(section)),
            number(width),
            number(height)
        );
        let mut layer = String::new();
        for b in &boxes {
            let at = Corners {
                left: self.across(b.at.0 - left),
                top: self.down(b.at.1 - top),
                right: width - self.across(b.at.0 - left + b.size.0),
                bottom: height - self.down(b.at.1 - top + b.size.1),
            };
            let style = format!(
                "left:{}px;top:{}px;width:{}px;height:{}px",
                number(at.left),
                number(at.top),
                number(self.across(b.size.0)),
                number(self.down(b.size.1))
            );
            let path = attribute(&self.project.full(b.card));
            let review = self.laid(b.card, &path, &at, &mut layer);
            out += &self.drawn(b, &path, &review, &style, chain);
        }
        out += &layer;
        out += "\n</div>";
        (out, width.ceil() as i64, height.ceil() as i64)
    }

    /// The box `b` (its path `path` and its `data-review` attribute `review`, as an attribute's
    /// values) at `style`: a part, or a frame holding its target's mock — `chain` the sections drawn
    /// round it, its own last.
    fn drawn(
        &mut self,
        b: &Placed,
        path: &str,
        review: &str,
        style: &str,
        chain: &mut Vec<String>,
    ) -> String {
        if b.card.kind == crate::kinds::FRAME {
            return self.frame(b.card, path, review, style, chain);
        }
        self.parts += 1;
        let kind = b.card.kind.as_str();
        let inside = if kind == "svg" {
            format!(
                "<img alt=\"\" src=\"data:image/svg+xml;base64,{}\">",
                base64(body(b.card).as_bytes())
            )
        } else {
            shadow(body(b.card))
        };
        format!(
            "\n<div class=\"sb-part\" data-path=\"{path}\" data-kind=\"{kind}\"{review} style=\"{style}\">{inside}</div>"
        )
    }

    /// A frame's box: the mock of the section it points at at 1:1, clipped to it — or, too deep, round
    /// or pointing at no ui section, a box naming its target.
    fn frame(
        &mut self,
        card: &Card,
        path: &str,
        review: &str,
        style: &str,
        chain: &mut Vec<String>,
    ) -> String {
        self.frames += 1;
        let project = self.project;
        let target = crate::tools::frames::target_of(card)
            .filter(|t| project.directory.has_section(t))
            .map(str::to_string);
        let named = target
            .as_deref()
            .map(|t| project.directory.full_section(t))
            .unwrap_or_default();
        let head = format!(
            "\n<div class=\"sb-part sb-frame\" data-path=\"{path}\" data-kind=\"frame\" data-frame=\"{}\"{review} style=\"{style}\">",
            attribute(&named)
        );
        let drawable = target.filter(|t| {
            project.section_kind(t) == Some(crate::tools::frames::UI)
                && !chain.iter().any(|s| s == t)
                && chain.len() <= DEPTH
        });
        let Some(target) = drawable else {
            let said = if named.is_empty() { "?" } else { &named };
            return format!("{head}<div class=\"sb-cut\">{}</div></div>", escaped(said));
        };
        chain.push(target.clone());
        let (inside, _, _) = self.section(&target, chain);
        chain.pop();
        format!("{head}{inside}</div>")
    }

    /// What is laid over the part `card` (its path `path`, its box at `at`), put in `layer`: its
    /// note and its review dot. Gives the part's `data-review` attribute, empty when it has none.
    fn laid(&mut self, card: &Card, path: &str, at: &Corners, layer: &mut String) -> String {
        let mut review = String::new();
        if self.overlay.notes
            && let Some(note) = text_field(card, "note").filter(|n| !n.trim().is_empty())
        {
            let place = text_field(card, "notePlace").unwrap_or_default();
            if place != "hidden" {
                let place = PLACES
                    .into_iter()
                    .find(|p| *p == place)
                    .unwrap_or(PLACES[1]);
                let x = if place.ends_with("Left") {
                    format!("left:{}px", number(at.left))
                } else {
                    format!("right:{}px", number(at.right))
                };
                let y = if place.starts_with("top") {
                    format!("top:{}px", number(at.top))
                } else {
                    format!("bottom:{}px", number(at.bottom))
                };
                *layer += &format!(
                    "\n<div class=\"sb-note\" data-path=\"{path}\" data-place=\"{place}\" style=\"{x};{y}\">{}</div>",
                    escaped(note)
                );
                self.layered = true;
            }
        }
        if let Some(marks) = self.overlay.review.and_then(|r| r.get(&card.id)) {
            let listed = crate::tools::review::listed(marks).to_string();
            let said: Vec<String> = marks
                .iter()
                .map(|m| format!("{} ({})", m.path, m.at))
                .collect();
            review = format!(" data-review=\"{}\"", attribute(&listed));
            *layer += &format!(
                "\n<div class=\"sb-review\" data-path=\"{path}\" title=\"{}\" style=\"right:{}px;top:{}px\"></div>",
                attribute(&said.join("\n")),
                number(at.right),
                number(at.top)
            );
            self.layered = true;
        }
        review
    }

    fn across(&self, units: i64) -> f64 {
        units as f64 * self.unit.0
    }

    fn down(&self, units: i64) -> f64 {
        units as f64 * self.unit.1
    }
}

/// How far a box is from each side of its section's box, in pixels.
struct Corners {
    left: f64,
    top: f64,
    right: f64,
    bottom: f64,
}

/// A part's text field `name`, when written.
fn text_field<'c>(card: &'c Card, name: &str) -> Option<&'c str> {
    card.form.as_ref()?.get("fields")?.get(name)?.as_str()
}

/// A part's body: its field `body`.
fn body(card: &Card) -> &str {
    card.form
        .as_ref()
        .and_then(|f| f.get("fields"))
        .and_then(|f| f.get("body"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or(&card.body)
}

/// An html part's body in a declarative shadow root, so its CSS holds inside the part alone. A body
/// that is a page of its own gives its head's styles and its body's content: a shadow root holds no
/// page.
fn shadow(body: &str) -> String {
    let (styles, content) = match page_parts(body) {
        Some((styles, content)) => (styles, content),
        None => (String::new(), body.to_string()),
    };
    format!("<template shadowrootmode=\"open\">{styles}{content}</template>")
}

/// A page's head's `<style>` elements, and what its body holds; none for a body that is no page.
fn page_parts(text: &str) -> Option<(String, String)> {
    let lower = text.to_ascii_lowercase();
    let html = tag_start(&lower, "html", 0)?;
    let head_end = lower.find("</head>");
    let mut styles = String::new();
    if let Some(head) = tag_start(&lower, "head", html) {
        let end = head_end.unwrap_or(text.len());
        let mut from = head;
        while let Some(open) = tag_start(&lower, "style", from).filter(|&s| s < end) {
            let close = lower[open..]
                .find("</style>")
                .map_or(text.len(), |c| open + c + "</style>".len());
            styles += &text[open..close];
            from = close;
        }
    }
    let content = match tag_start(&lower, "body", html) {
        Some(start) => {
            let opened = lower[start..]
                .find('>')
                .map_or(text.len(), |e| start + e + 1);
            let closed = lower[opened..]
                .find("</body>")
                .map_or_else(|| text.len(), |c| opened + c);
            &text[opened..closed]
        }
        None => {
            let from = head_end.map_or_else(
                || lower[html..].find('>').map_or(text.len(), |e| html + e + 1),
                |h| h + "</head>".len(),
            );
            let to = lower[from..]
                .find("</html>")
                .map_or(text.len(), |c| from + c);
            &text[from..to]
        }
    };
    Some((styles, content.trim().to_string()))
}

/// Where the first start tag `<name …>` of `lower` (a text in lower case) from `from` begins.
fn tag_start(lower: &str, name: &str, from: usize) -> Option<usize> {
    let open = format!("<{name}");
    let mut at = from;
    while let Some(i) = lower.get(at..)?.find(&open) {
        let start = at + i;
        let next = lower[start + open.len()..].chars().next();
        if next.is_some_and(|c| c == '>' || c == '/' || c.is_ascii_whitespace()) {
            return Some(start);
        }
        at = start + open.len();
    }
    None
}

/// The links in a page to `sb-nav:<path>` (`href="sb-nav:…"`) each as `to` says it, for the path
/// it names; one `to` says nothing of is kept as it is.
pub fn rewrite_nav(html: &str, to: impl Fn(&str) -> Option<String>) -> String {
    const NAV: &str = "sb-nav:";
    let mut out = String::with_capacity(html.len());
    let mut rest = html;
    while let Some(i) = rest.find(NAV) {
        let before = &rest[..i];
        let quote = before.chars().last().filter(|q| *q == '"' || *q == '\'');
        let is_href = quote.is_some_and(|_| {
            before[..before.len() - 1]
                .trim_end()
                .strip_suffix('=')
                .is_some_and(|b| b.trim_end().to_ascii_lowercase().ends_with("href"))
        });
        let value_end = quote.and_then(|q| rest[i..].find(q)).map(|e| i + e);
        match (is_href, value_end) {
            (true, Some(end)) => {
                let path = decoded(&rest[i + NAV.len()..end]);
                out += before;
                match to(&path) {
                    Some(link) => out += &attribute(&link),
                    None => out += &rest[i..end],
                }
                rest = &rest[end..];
            }
            _ => {
                out += &rest[..i + NAV.len()];
                rest = &rest[i + NAV.len()..];
            }
        }
    }
    out += rest;
    out
}

/// A link's text with its `%XX` read and the entities an attribute's quotes and markup are written
/// with (`&amp;`, `&quot;`, `&apos;`, `&lt;`, `&gt;`) as their characters.
fn decoded(text: &str) -> String {
    let text = text
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&");
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && let Some(b) = text
                .get(i + 1..i + 3)
                .and_then(|h| u8::from_str_radix(h, 16).ok())
        {
            out.push(b);
            i += 3;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8(out).unwrap_or(text)
}

/// A length in pixels as CSS reads it: whole, or to two places.
fn number(value: f64) -> String {
    let rounded = (value * 100.0).round() / 100.0;
    if rounded.fract() == 0.0 {
        format!("{}", rounded as i64)
    } else {
        format!("{rounded}")
    }
}

fn escaped(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// A text as an attribute's value in double quotes.
fn attribute(text: &str) -> String {
    escaped(text).replace('"', "&quot;")
}

/// Bytes in base64, padded (a `data:` URL's).
fn base64(bytes: &[u8]) -> String {
    const ABC: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = (u32::from(chunk[0]) << 16)
            | (u32::from(*chunk.get(1).unwrap_or(&0)) << 8)
            | u32::from(*chunk.get(2).unwrap_or(&0));
        for (i, shift) in [18, 12, 6, 0].into_iter().enumerate() {
            if i <= chunk.len() {
                out.push(ABC[((n >> shift) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bytes_are_said_in_base64() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64("ア".as_bytes()), "44Ki");
    }

    #[test]
    fn a_whole_page_gives_its_heads_styles_and_its_bodys_content() {
        let (styles, content) = page_parts(
            "<!doctype html><html><head><title>t</title><style>p{color:red}</style></head>\
             <body class=\"x\"><p>本文</p></body></html>",
        )
        .unwrap();
        assert_eq!(styles, "<style>p{color:red}</style>");
        assert_eq!(content, "<p>本文</p>");
        assert!(page_parts("<p>断片</p>").is_none());
        let (_, content) = page_parts("<html><p>体の無い</p></html>").unwrap();
        assert_eq!(content, "<p>体の無い</p>");
    }

    #[test]
    fn only_the_links_to_sb_nav_are_rewritten() {
        let page = "<a href=\"sb-nav:UI/Home\">h</a><a href='sb-nav:UI/%E7%94%BB'>g</a>\
                    <p>sb-nav:UI/Home</p><a href=\"https://x\">x</a><a href=\"sb-nav:Other\">o</a>";
        let out = rewrite_nav(page, |p| match p {
            "UI/Home" => Some("Home.html".into()),
            "UI/画" => Some("画.html".into()),
            _ => None,
        });
        assert_eq!(
            out,
            "<a href=\"Home.html\">h</a><a href='画.html'>g</a>\
             <p>sb-nav:UI/Home</p><a href=\"https://x\">x</a><a href=\"sb-nav:Other\">o</a>"
        );
    }

    #[test]
    fn a_length_is_whole_or_to_two_places() {
        assert_eq!(number(352.0), "352");
        assert_eq!(number(35.2 * 3.0), "105.6");
        assert_eq!(number(1.0 / 3.0), "0.33");
    }
}
