//! Identifier shaping, shared by the code generators (`Ident` in the Swift app's Export).

use crate::ojson;
use sha2::{Digest, Sha256};
use std::collections::HashSet;

fn plain_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || c == '$'
}

/// A name made safe as a TypeScript identifier.
pub fn safe(name: &str) -> String {
    let mut cleaned: String = name
        .chars()
        .map(|c| if plain_char(c) { c } else { '_' })
        .collect();
    if cleaned.chars().next().is_some_and(|c| c.is_ascii_digit()) {
        cleaned.insert(0, '_');
    }
    if cleaned.is_empty() {
        "_".to_string()
    } else {
        cleaned
    }
}

/// A type's name from a card's name (`specboard://SpecBoard/Docs/Design:DesignSections`「名前から型・ファイル・図の
/// id を作る」): cut where anything but a letter or a digit is, the first of each piece capitalised
/// and the pieces put together (`foo_bar-baz` → `FooBarBaz`, `CardKinds.md` → `CardKindsMd`, `user
/// profile` → `UserProfile`); a letter or a digit of any script kept (`注文`). What it cut at its end
/// stays as `_` (`a_` → `A_`), a digit first has `_` before it (`1st` → `_1st`), and a name of no
/// letter nor digit is `_` and a short digest of it — but one of `_` alone, kept as it is.
pub fn pascal(text: &str) -> String {
    let mut out = String::new();
    let mut pending = 0usize;
    let mut at_boundary = true;
    for c in text.chars() {
        if !c.is_alphanumeric() {
            pending += 1;
            at_boundary = true;
            continue;
        }
        if at_boundary {
            out.extend(c.to_uppercase());
        } else {
            out.push(c);
        }
        pending = 0;
        at_boundary = false;
    }
    if out.is_empty() {
        if !text.is_empty() && text.chars().all(|c| c == '_') {
            return text.to_string();
        }
        let digest: String = Sha256::digest(text.as_bytes())[..4]
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        return format!("_{digest}");
    }
    out.extend(std::iter::repeat_n('_', pending));
    if out.starts_with(|c: char| c.is_numeric()) {
        out.insert(0, '_');
    }
    out
}

/// `items` → `item`. Crude on purpose.
pub fn singular(text: &str) -> String {
    let lower = text.to_lowercase();
    let drop = |n: usize| {
        text.chars()
            .take(text.chars().count() - n)
            .collect::<String>()
    };
    if lower.ends_with("ies") {
        return drop(3) + "y";
    }
    for ending in ["ses", "shes", "ches", "xes", "zes"] {
        if lower.ends_with(ending) {
            return drop(2);
        }
    }
    if lower.ends_with('s') && !lower.ends_with("ss") {
        return drop(1);
    }
    text.to_string()
}

/// A key, quoted only when it is not a bare identifier.
pub fn key(name: &str) -> String {
    let bare = name
        .chars()
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_' || c == '$')
        && name.chars().all(plain_char);
    if bare {
        name.to_string()
    } else {
        ojson::quote(name)
    }
}

/// The name of an interface taken out of a nested `object` field: `Order.items[]` →
/// `OrderItem`; on a clash the whole path, then a number.
pub fn nested_type(
    owner: &str,
    path: &[String],
    is_array: bool,
    taken: &mut HashSet<String>,
) -> String {
    let segment = |text: &str, plural: bool| {
        pascal(&if plural {
            singular(text)
        } else {
            text.to_string()
        })
    };
    let short = pascal(owner) + &segment(&path[path.len() - 1], is_array);
    if taken.insert(short.clone()) {
        return short;
    }
    let full = pascal(owner)
        + &path
            .iter()
            .enumerate()
            .map(|(i, p)| segment(p, i == path.len() - 1 && is_array))
            .collect::<String>();
    let mut name = full.clone();
    let mut n = 2;
    while taken.contains(&name) {
        name = format!("{full}{n}");
        n += 1;
    }
    taken.insert(name.clone());
    name
}

/// Text for a `//` comment, kept on its line.
pub fn line_comment(text: &str) -> String {
    text.chars()
        .map(|c| {
            if matches!(c, '\n' | '\r' | '\u{2028}' | '\u{2029}') {
                ' '
            } else {
                c
            }
        })
        .collect()
}

/// Text for a mermaid label, kept inside its quotes and on its line.
pub fn mermaid_text(text: &str) -> String {
    line_comment(text).replace('"', "'").replace('#', "＃")
}

/// A name on a class's member line of mermaid, kept from being read as its syntax: what mermaid
/// reads there (`{ } ( ) ~ < > : ; " #`) put as its full-width look-alike.
pub fn member_text(text: &str) -> String {
    line_comment(text)
        .chars()
        .map(|c| match c {
            '{' => '｛',
            '}' => '｝',
            '(' => '（',
            ')' => '）',
            '~' => '～',
            '<' => '＜',
            '>' => '＞',
            ':' => '：',
            ';' => '；',
            '"' => '\'',
            '#' => '＃',
            other => other,
        })
        .collect()
}

/// A name as it goes after `#/$defs/` in a `$ref`: a JSON Pointer's token (`~` as `~0`, `/` as
/// `~1`), then what a URI's fragment may not hold as `%XX`.
pub fn pointer_token(name: &str) -> String {
    const ALLOWED: &str = "-._~!$&'()*+,;=:@/?";
    let token = name.replace('~', "~0").replace('/', "~1");
    let mut out = String::new();
    for c in token.chars() {
        if c.is_ascii_alphanumeric() || ALLOWED.contains(c) {
            out.push(c);
        } else {
            let mut buf = [0u8; 4];
            for b in c.encode_utf8(&mut buf).bytes() {
                out.push_str(&format!("%{b:02X}"));
            }
        }
    }
    out
}

/// A property path read off `base`: `.name` where a part is bare, `["…"]` where not. The path's
/// names are split as a path's are (a name holding a dot is quoted).
pub fn access(base: &str, path: &str) -> String {
    let mut out = base.to_string();
    for part in crate::pathtext::names(path) {
        let part = part.as_str();
        let quoted = key(part);
        if quoted == part {
            out.push('.');
            out.push_str(part);
        } else {
            out.push('[');
            out.push_str(&quoted);
            out.push(']');
        }
    }
    out
}

/// A `/** … */` line from the parts that are there, joined with ` — `.
pub fn doc_comment(parts: &[Option<String>], indent: &str) -> String {
    let text = parts
        .iter()
        .flatten()
        .filter(|p| !p.is_empty())
        .cloned()
        .collect::<Vec<_>>()
        .join(" — ");
    if text.is_empty() {
        return String::new();
    }
    format!("{indent}/** {} */\n", text.replace("*/", "*\\/"))
}

/// A doc comment as `doc_comment` makes it, with `lines` under it one to a line (an enum's described
/// values); without them, the same one line.
pub fn doc_comment_lines(parts: &[Option<String>], lines: &[String], indent: &str) -> String {
    if lines.is_empty() {
        return doc_comment(parts, indent);
    }
    let head = parts
        .iter()
        .flatten()
        .filter(|p| !p.is_empty())
        .cloned()
        .collect::<Vec<_>>()
        .join(" — ");
    let mut out = format!("{indent}/**\n");
    for line in std::iter::once(&head)
        .filter(|h| !h.is_empty())
        .chain(lines)
    {
        out.push_str(&format!("{indent} * {}\n", line.replace("*/", "*\\/")));
    }
    out.push_str(&format!("{indent} */\n"));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_type_s_name_is_made_of_the_letters_and_digits_of_the_card_s_name() {
        for (name, typed) in [
            // Names of letters, digits and `_` give the type names they always gave.
            ("Order", "Order"),
            ("order_item", "OrderItem"),
            ("_private", "Private"),
            ("a_", "A_"),
            ("v0_2_21", "V0221"),
            ("_", "_"),
            ("__", "__"),
            // Other names: cut at what is neither a letter nor a digit.
            ("CardKinds.md", "CardKindsMd"),
            ("user profile", "UserProfile"),
            ("e-mail", "EMail"),
            ("Order.v2", "OrderV2"),
            ("注文", "注文"),
            ("注文 明細", "注文明細"),
            ("élan", "Élan"),
            ("1st", "_1st"),
            ("2026 plan", "_2026Plan"),
        ] {
            assert_eq!(pascal(name), typed, "{name}");
        }
        // Nothing to make it of: `_` and a digest, the same for the same name.
        let none = pascal("!!!");
        assert!(none.starts_with('_') && none.len() == 9, "{none}");
        assert_eq!(pascal("!!!"), none);
        assert_ne!(pascal("???"), none);
    }

    #[test]
    fn a_name_is_kept_from_being_read_as_syntax() {
        assert_eq!(key("user profile"), "\"user profile\"");
        assert_eq!(key("name"), "name");
        assert_eq!(pointer_token("Order"), "Order");
        assert_eq!(pointer_token("a/b~c"), "a~1b~0c");
        assert_eq!(pointer_token("注文 v2"), "%E6%B3%A8%E6%96%87%20v2");
        assert_eq!(member_text("f(x): {a}"), "f（x）： ｛a｝");
        assert_eq!(access("v", "'a.b'.c"), "v[\"a.b\"].c");
    }
}
