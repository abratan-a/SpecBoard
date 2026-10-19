//! Whether a text is an SVG picture: XML that reads (one root element, every element closed in
//! order, attributes quoted, comments and sections closed) whose root is an `svg` element in any
//! namespace (`svg`, `svg:svg`). A face draws it as a picture (`PageKind::Svg`), and a text that is
//! not one would draw nothing: it is refused as it is written.
//!
//! Well-formed enough to be read as XML, not a validating parser: names are taken as anything up
//! to a space, `/`, `>` or `=`, and what a reference names is not looked up — a viewer that reads it
//! as XML is what is guarded against.

use crate::text::Keyed;

/// Why `text` is not an SVG picture, or none when it is one.
pub(crate) fn problem(text: &str) -> Option<Keyed> {
    let mut reader = Reader {
        text: text.strip_prefix('\u{feff}').unwrap_or(text),
        at: 0,
    };
    match reader.document() {
        Ok(root) => {
            let local = root.rsplit_once(':').map_or(root.as_str(), |(_, l)| l);
            (local != "svg").then(|| keyed!("kindCheck.svg.rootNotSvg", name = root))
        }
        Err((at, why)) => Some(keyed!("kindCheck.svg.notXml", line = reader.line(at), why)),
    }
}

struct Reader<'a> {
    text: &'a str,
    at: usize,
}

type Failed = (usize, Keyed);

impl Reader<'_> {
    fn rest(&self) -> &str {
        &self.text[self.at..]
    }

    fn line(&self, at: usize) -> usize {
        self.text[..at.min(self.text.len())].matches('\n').count() + 1
    }

    fn skip_space(&mut self) {
        let rest = self.rest();
        self.at += rest.len() - rest.trim_start().len();
    }

    /// Past the next `end`, or why there is none.
    fn past(&mut self, end: &str, why: Keyed) -> Result<(), Failed> {
        match self.rest().find(end) {
            Some(i) => {
                self.at += i + end.len();
                Ok(())
            }
            None => Err((self.at, why)),
        }
    }

    /// A comment, a processing instruction or (before the root) a document type, when one starts
    /// here: whether one was read.
    fn misc(&mut self, before_root: bool) -> Result<bool, Failed> {
        let rest = self.rest();
        if rest.starts_with("<!--") {
            self.at += 4;
            self.past("-->", keyed!("kindCheck.svg.why.comment"))?;
        } else if rest.starts_with("<?") {
            self.past("?>", keyed!("kindCheck.svg.why.instruction"))?;
        } else if before_root && rest.starts_with("<!DOCTYPE") {
            self.doctype()?;
        } else {
            return Ok(false);
        }
        Ok(true)
    }

    /// A document type, its internal subset (`[...]`) too.
    fn doctype(&mut self) -> Result<(), Failed> {
        let start = self.at;
        let mut depth = 0usize;
        let mut quote: Option<char> = None;
        for (i, c) in self.rest().char_indices() {
            match (quote, c) {
                (Some(q), c) if c == q => quote = None,
                (Some(_), _) => {}
                (None, '"' | '\'') => quote = Some(c),
                (None, '[') => depth += 1,
                (None, ']') => depth = depth.saturating_sub(1),
                (None, '>') if depth == 0 => {
                    self.at += i + 1;
                    return Ok(());
                }
                _ => {}
            }
        }
        Err((start, keyed!("kindCheck.svg.why.doctype")))
    }

    /// The whole text: its root element's name.
    fn document(&mut self) -> Result<String, Failed> {
        loop {
            self.skip_space();
            if !self.misc(true)? {
                break;
            }
        }
        if !self.rest().starts_with('<') {
            return Err((self.at, keyed!("kindCheck.svg.why.noRoot")));
        }
        let root = self.element()?;
        loop {
            self.skip_space();
            if self.rest().is_empty() {
                return Ok(root);
            }
            if !self.misc(false)? {
                return Err((self.at, keyed!("kindCheck.svg.why.afterRoot", name = root)));
            }
        }
    }

    /// A name, up to what ends one.
    fn name(&mut self) -> Result<String, Failed> {
        let rest = self.rest();
        let end = rest
            .find(|c: char| c.is_whitespace() || "/>=<\"'".contains(c))
            .unwrap_or(rest.len());
        if end == 0 {
            return Err((self.at, keyed!("kindCheck.svg.why.tag")));
        }
        let name = rest[..end].to_string();
        self.at += end;
        Ok(name)
    }

    /// An element from its `<`, and every element inside it: its name.
    fn element(&mut self) -> Result<String, Failed> {
        let mut open: Vec<(String, usize)> = Vec::new();
        let mut root = None;
        loop {
            // A start tag here.
            let start = self.at;
            self.at += 1;
            let name = self.name()?;
            let empty = self.attributes(start)?;
            if root.is_none() {
                root = Some(name.clone());
            }
            if !empty {
                open.push((name, start));
            }
            // Its content, up to the next start tag, or to the end of what is open.
            loop {
                let Some((name, start)) = open.last().cloned() else {
                    return Ok(root.unwrap_or_default());
                };
                self.text_content()?;
                let rest = self.rest();
                if rest.is_empty() {
                    return Err((start, keyed!("kindCheck.svg.why.unclosed", name)));
                }
                if rest.starts_with("</") {
                    let at = self.at;
                    self.at += 2;
                    let closing = self.name()?;
                    self.skip_space();
                    if !self.rest().starts_with('>') {
                        return Err((self.at, keyed!("kindCheck.svg.why.tag")));
                    }
                    self.at += 1;
                    if closing != name {
                        return Err((at, keyed!("kindCheck.svg.why.mismatch", closing, name)));
                    }
                    open.pop();
                } else if rest.starts_with("<![CDATA[") {
                    self.past("]]>", keyed!("kindCheck.svg.why.cdata"))?;
                } else if !self.misc(false)? {
                    break;
                }
            }
        }
    }

    /// A start tag's attributes and its end: whether it closes itself (`/>`).
    fn attributes(&mut self, start: usize) -> Result<bool, Failed> {
        let mut names: Vec<String> = Vec::new();
        loop {
            let before = self.at;
            self.skip_space();
            let rest = self.rest();
            if rest.starts_with("/>") {
                self.at += 2;
                return Ok(true);
            }
            if rest.starts_with('>') {
                self.at += 1;
                return Ok(false);
            }
            if rest.is_empty() {
                return Err((start, keyed!("kindCheck.svg.why.tag")));
            }
            // An attribute is set apart from the name or the attribute before it.
            if self.at == before {
                return Err((self.at, keyed!("kindCheck.svg.why.attribute")));
            }
            let name = self.name()?;
            self.skip_space();
            if !self.rest().starts_with('=') {
                return Err((self.at, keyed!("kindCheck.svg.why.attribute")));
            }
            self.at += 1;
            self.skip_space();
            let Some(quote) = self
                .rest()
                .chars()
                .next()
                .filter(|c| *c == '"' || *c == '\'')
            else {
                return Err((self.at, keyed!("kindCheck.svg.why.attribute")));
            };
            self.at += 1;
            let Some(end) = self.rest().find(quote) else {
                return Err((self.at, keyed!("kindCheck.svg.why.attribute")));
            };
            let value = &self.rest()[..end];
            if value.contains('<') {
                return Err((self.at, keyed!("kindCheck.svg.why.attribute")));
            }
            self.references(value, self.at)?;
            self.at += end + 1;
            if names.contains(&name) {
                return Err((before, keyed!("kindCheck.svg.why.twice", name)));
            }
            names.push(name);
        }
    }

    /// The characters up to the next `<`.
    fn text_content(&mut self) -> Result<(), Failed> {
        let end = self.rest().find('<').unwrap_or(self.rest().len());
        let text = &self.rest()[..end];
        self.references(text, self.at)?;
        self.at += end;
        Ok(())
    }

    /// Each `&` in `text` (at `at`) starts a reference: `&name;` or `&#…;`.
    fn references(&self, text: &str, at: usize) -> Result<(), Failed> {
        let mut from = 0;
        while let Some(i) = text[from..].find('&') {
            let start = from + i + 1;
            let end = text[start..].find(';').map(|e| start + e);
            let fine = end.is_some_and(|e| {
                let name = &text[start..e];
                !name.is_empty()
                    && name
                        .chars()
                        .all(|c| c.is_alphanumeric() || "#_:.-".contains(c))
            });
            if !fine {
                return Err((at + start - 1, keyed!("kindCheck.svg.why.reference")));
            }
            from = end.unwrap_or(text.len()) + 1;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::problem;

    fn said(text: &str) -> Option<String> {
        problem(text).map(|k| k.to_string())
    }

    #[test]
    fn an_svg_root_in_any_namespace_is_a_picture() {
        for fine in [
            r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10"><rect width="10" height="10"/></svg>"#,
            "\u{feff}<?xml version=\"1.0\"?>\n<!-- 図 -->\n<!DOCTYPE svg [<!ENTITY a \"b\">]>\n<svg:svg xmlns:svg='x'>\n  <svg:text>a &amp; b &#x41;</svg:text><![CDATA[<x>]]>\n</svg:svg>\n<!-- end -->\n",
            "<svg/>",
            "<svg><style>a > b { fill: red }</style></svg>",
        ] {
            assert_eq!(said(fine), None, "{fine}");
        }
    }

    #[test]
    fn what_is_not_xml_or_not_svg_is_said_with_its_line() {
        let cases = [
            ("", "要素がありません"),
            ("ただの文", "要素がありません"),
            ("<svg><g></svg>", "<g>"),
            ("<svg>\n<g>\n</svg>", "3 行目"),
            ("<svg><rect width=10/></svg>", "属性"),
            ("<svg><rect a='1' a='2'/></svg>", "a"),
            ("<svg>a & b</svg>", "&"),
            ("<svg></svg><svg></svg>", "根の要素"),
            ("<svg><!-- open</svg>", "コメント"),
            ("<svg", "タグ"),
        ];
        for (text, part) in cases {
            let got = said(text).unwrap_or_default();
            assert!(got.contains(part), "{text}: {got}");
        }
        assert!(said("<html><body/></html>").unwrap().contains("<html>"));
    }
}
