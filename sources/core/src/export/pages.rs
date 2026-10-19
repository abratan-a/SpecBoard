//! An SVG card's and an HTML card's file (`specboard://SpecBoard/Docs/Design:DesignSections`
//! 「SVG と HTML のカード」): an SVG's body as it is, an HTML's body as one page whose head keeps
//! what runs in it to what the app lets run when it draws the card — no script, nothing loaded from
//! outside (images and fonts only as `data:`), no form sent — so the file opened in a browser is the
//! card as the app shows it.

use crate::model::Card;

/// What a page may do, as the app's page of an HTML card says it (`Content-Security-Policy`).
pub const PAGE_POLICY: &str = "default-src 'none'; style-src 'unsafe-inline'; img-src data:; font-src data:; base-uri 'none'; form-action 'none'";

/// An SVG card's file: its body as it is.
pub fn svg_file(card: &Card) -> String {
    card.body.clone()
}

/// An HTML card's file. A body that is a page of its own (its `<html>`) keeps its own head, and
/// the policy goes first in it — a policy holds only for what comes after it; any other body is
/// put in a page of the card's name.
pub fn html_file(card: &Card, mark: &str) -> String {
    let body = card.body.as_str();
    let marker = super::charts::marker(mark);
    let policy = format!("<meta http-equiv=\"Content-Security-Policy\" content=\"{PAGE_POLICY}\">");
    let Some(html) = tag_end(body, "html") else {
        let title = if card.label.is_empty() {
            &card.name
        } else {
            &card.label
        };
        return format!(
            "<!doctype html>\n{marker}\n<html>\n<head>\n<meta charset=\"utf-8\">\n{policy}\n<title>{}</title>\n</head>\n<body>\n{}\n</body>\n</html>\n",
            escaped(title),
            body.trim_end_matches(['\n', '\r'])
        );
    };
    let charset = if has_charset(body) {
        String::new()
    } else {
        "\n<meta charset=\"utf-8\">".to_string()
    };
    let mut page = match tag_end(body, "head") {
        Some(head) => format!("{}{charset}\n{policy}{}", &body[..head], &body[head..]),
        None => format!(
            "{}\n<head>{charset}\n{policy}\n</head>{}",
            &body[..html],
            &body[html..]
        ),
    };
    // Its doctype first, and the mark after it: nothing but a doctype may come before the page.
    let start = page.len() - page.trim_start().len();
    let doctype = page[start..]
        .get(..9)
        .is_some_and(|s| s.eq_ignore_ascii_case("<!doctype"));
    if doctype {
        if let Some(end) = page[start..].find('>') {
            page.insert_str(start + end + 1, &format!("\n{marker}"));
        }
    } else {
        page = format!("<!doctype html>\n{marker}\n{}", &page[start..]);
    }
    if !page.ends_with('\n') {
        page.push('\n');
    }
    page
}

/// Where the first start tag `<name …>` of `text` ends (past its `>`), the name in any case.
fn tag_end(text: &str, name: &str) -> Option<usize> {
    let lower = text.to_ascii_lowercase();
    let open = format!("<{name}");
    let mut from = 0;
    while let Some(i) = lower[from..].find(&open) {
        let at = from + i;
        let next = lower[at + open.len()..].chars().next();
        if next.is_some_and(|c| c == '>' || c == '/' || c.is_ascii_whitespace()) {
            return lower[at..].find('>').map(|e| at + e + 1);
        }
        from = at + open.len();
    }
    None
}

/// Whether a page says what its characters are in.
fn has_charset(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    lower.contains("<meta charset") || lower.contains("charset=")
}

fn escaped(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn card(name: &str, label: &str, body: &str) -> Card {
        Card {
            name: name.into(),
            label: label.into(),
            body: body.into(),
            kind: "html".into(),
            ..Card::default()
        }
    }

    fn policy() -> String {
        format!("<meta http-equiv=\"Content-Security-Policy\" content=\"{PAGE_POLICY}\">")
    }

    #[test]
    fn a_part_of_a_page_is_put_in_a_page_of_its_own_with_the_policy() {
        let page = html_file(&card("Mock", "画面 <案>", "<p>本文</p>\n"), "印");
        assert_eq!(
            page,
            format!(
                "<!doctype html>\n<!-- 印 -->\n<html>\n<head>\n<meta charset=\"utf-8\">\n{}\n<title>画面 &lt;案&gt;</title>\n</head>\n<body>\n<p>本文</p>\n</body>\n</html>\n",
                policy()
            )
        );
        assert!(html_file(&card("Mock", "", "x"), "印").contains("<title>Mock</title>"));
    }

    #[test]
    fn a_whole_page_keeps_its_head_and_has_the_policy_first_in_it() {
        let page = html_file(
            &card(
                "Mock",
                "",
                "<!DOCTYPE html>\n<html lang=\"ja\"><head><meta charset=\"utf-8\"><title>t</title></head><body><header>h</header></body></html>",
            ),
            "印",
        );
        assert_eq!(
            page,
            format!(
                "<!DOCTYPE html>\n<!-- 印 -->\n<html lang=\"ja\"><head>\n{}<meta charset=\"utf-8\"><title>t</title></head><body><header>h</header></body></html>\n",
                policy()
            )
        );
        // No head, no doctype: both put in, the header not taken for a head.
        let page = html_file(
            &card("Mock", "", "<HTML><body><header>h</header></body></HTML>"),
            "印",
        );
        assert_eq!(
            page,
            format!(
                "<!doctype html>\n<!-- 印 -->\n<HTML>\n<head>\n<meta charset=\"utf-8\">\n{}\n</head><body><header>h</header></body></HTML>\n",
                policy()
            )
        );
    }

    #[test]
    fn an_svg_is_written_as_it_is() {
        let body = "<svg xmlns=\"http://www.w3.org/2000/svg\"/>";
        assert_eq!(svg_file(&card("Logo", "", body)), body);
    }
}
