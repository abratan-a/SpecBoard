//! A whiteboard's image (`specboard://SpecBoard/Docs/Design:CardKinds`「ホワイトボード（whiteboard）」): a picture
//! kept once for the project by the sha256 of its contents (`images`), which the card names. A card's
//! value gives the contents — `imageData` (base64) or `imageFrom` (a working directory's file) — and the
//! core writes what it reads of them: the name, the form and the size. Nothing else writes those, so a
//! card never names a picture the project does not hold.

use rusqlite::{Connection, OptionalExtension, params};
use serde_json::{Map, Value};

/// The most a picture may be, in bytes.
pub(crate) const MOST_BYTES: usize = 5 * 1024 * 1024;

/// The longest side a picture may have, in pixels: a bigger one is no picture a whiteboard shows, and decoding
/// it would take more memory than it is worth.
pub(crate) const MOST_SIDE: u32 = 16_384;

/// The longest side of a picture that is decoded whole however small it is shown — an interlaced or 16-bit PNG,
/// a GIF — in pixels: a picture is shown through a thumbnail of its own size, which for these is no cheaper than
/// the whole, so a bigger one would take gigabytes to show on a whiteboard.
pub(crate) const MOST_WHOLE_SIDE: u32 = 4_096;

/// The keys of an image card the core writes from its contents.
pub(crate) const READ_KEYS: [&str; 5] = ["image", "format", "width", "height", "bytes"];

/// What a picture is: its form and its size in pixels, read from its head.
#[derive(Debug, PartialEq)]
pub(crate) struct Head {
    pub format: &'static str,
    pub width: u32,
    pub height: u32,
    /// Decoded whole to be shown at any size (`MOST_WHOLE_SIDE`).
    pub whole: bool,
}

/// The form and size of a PNG, a JPEG or a GIF, from what they begin with; none for anything else.
pub(crate) fn head(bytes: &[u8]) -> Option<Head> {
    let be32 = |at: usize| {
        bytes
            .get(at..at + 4)
            .map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
    };
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") && bytes.get(12..16) == Some(b"IHDR") {
        // Bit depth at 24, interlace method at 28.
        let (depth, interlace) = (*bytes.get(24)?, *bytes.get(28)?);
        return Some(Head {
            format: "png",
            width: be32(16)?,
            height: be32(20)?,
            whole: depth > 8 || interlace != 0,
        });
    }
    if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        let le16 = |at: usize| {
            bytes
                .get(at..at + 2)
                .map(|b| u32::from(u16::from_le_bytes([b[0], b[1]])))
        };
        return Some(Head {
            format: "gif",
            width: le16(6)?,
            height: le16(8)?,
            whole: true,
        });
    }
    if bytes.starts_with(&[0xFF, 0xD8]) {
        // The frame's head (SOF0 to SOF15 but DHT, JPG and DAC) says its size; the marks before it are skipped
        // by their lengths.
        let mut at = 2;
        while at + 4 <= bytes.len() {
            if bytes[at] != 0xFF {
                return None;
            }
            let mark = bytes[at + 1];
            if mark == 0xFF {
                at += 1;
                continue;
            }
            let len = usize::from(u16::from_be_bytes([bytes[at + 2], bytes[at + 3]]));
            if (0xC0..=0xCF).contains(&mark) && ![0xC4, 0xC8, 0xCC].contains(&mark) {
                let b = bytes.get(at + 5..at + 9)?;
                return Some(Head {
                    format: "jpeg",
                    width: u32::from(u16::from_be_bytes([b[2], b[3]])),
                    height: u32::from(u16::from_be_bytes([b[0], b[1]])),
                    whole: false,
                });
            }
            at += 2 + len;
        }
    }
    None
}

/// The bytes base64 `text` says (padded or not, with line breaks or not); none when it is not base64.
pub(crate) fn unbase64(text: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(text.len() / 4 * 3);
    let (mut n, mut bits) = (0u32, 0);
    for c in text.bytes() {
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            b'=' | b'\n' | b'\r' | b' ' => continue,
            _ => return None,
        };
        n = (n << 6) | u32::from(v);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((n >> bits) as u8);
            n &= (1 << bits) - 1;
        }
    }
    Some(out)
}

/// Whether a card's value gives a picture's contents, or writes what only the core writes.
pub(crate) fn wanted(value: &Map<String, Value>) -> bool {
    value.contains_key("imageData")
        || value.contains_key("imageFrom")
        || READ_KEYS.iter().any(|k| value.contains_key(*k))
}

/// The picture a card's value gives (`imageData` or `imageFrom`) kept in the project's store, and the
/// value made to say what was read of it: its name, form and size, in place of the contents. A value
/// that writes those itself is refused: only the contents tell them. `read_file` reads a working
/// directory's file (`body_from`).
pub(crate) fn take(
    conn: &Connection,
    project: &str,
    value: &mut Map<String, Value>,
    read_file: &mut dyn FnMut(&Value) -> Result<Vec<u8>, String>,
) -> Result<(), String> {
    if let Some(key) = READ_KEYS.iter().find(|k| value.contains_key(**k)) {
        return Err(text!("writes.image.readOnly", key));
    }
    let bytes = match (
        value.shift_remove("imageData"),
        value.shift_remove("imageFrom"),
    ) {
        (Some(_), Some(_)) => return Err(text!("writes.image.both")),
        (Some(Value::String(data)), None) => {
            unbase64(&data).ok_or_else(|| text!("writes.image.notBase64"))?
        }
        (Some(_), None) => return Err(text!("writes.image.notBase64")),
        (None, Some(from)) => read_file(&from)?,
        (None, None) => return Ok(()),
    };
    if bytes.len() > MOST_BYTES {
        return Err(text!(
            "writes.image.tooBig",
            bytes = bytes.len(),
            most = MOST_BYTES
        ));
    }
    let head = checked(&bytes)?;
    let hash = sha256(&bytes);
    conn.execute(
        "INSERT OR IGNORE INTO images (project_id, hash, format, width, height, bytes, data) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            project,
            hash,
            head.format,
            head.width,
            head.height,
            bytes.len() as i64,
            crate::export::mock::base64(&bytes)
        ],
    )
    .map_err(|e| e.to_string())?;
    value.insert("image".into(), hash.into());
    value.insert("format".into(), head.format.into());
    value.insert("width".into(), head.width.into());
    value.insert("height".into(), head.height.into());
    value.insert("bytes".into(), bytes.len().into());
    Ok(())
}

/// What a picture's contents say they are, when they are a picture a whiteboard shows: a PNG, a JPEG or a GIF,
/// each side 1 to `MOST_SIDE` pixels — `MOST_WHOLE_SIDE` for one decoded whole to be shown.
pub(crate) fn checked(bytes: &[u8]) -> Result<Head, String> {
    let Some(head) = head(bytes) else {
        return Err(text!("writes.image.form"));
    };
    let most = if head.whole {
        MOST_WHOLE_SIDE
    } else {
        MOST_SIDE
    };
    if !(1..=most).contains(&head.width) || !(1..=most).contains(&head.height) {
        let (width, height) = (head.width, head.height);
        return Err(if head.whole {
            text!("writes.image.wholeSide", width, height, most)
        } else {
            text!("writes.image.side", width, height, most)
        });
    }
    Ok(head)
}

/// The name a picture is kept by: the sha256 of its contents, in hex.
pub(crate) fn sha256(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// A picture of the project's store, by its name: its form and its contents in base64.
pub(crate) fn picture(
    conn: &Connection,
    project: &str,
    hash: &str,
) -> Result<Option<(String, String)>, String> {
    conn.query_row(
        "SELECT format, data FROM images WHERE project_id = ?1 AND hash = ?2",
        params![project, hash],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )
    .optional()
    .map_err(|e| e.to_string())
}

/// An image card's picture as MCP gives one (`card_read` `as: image`): the picture, and after it what it
/// is — where, its form, its size and what it shows.
pub(crate) fn read(
    store: &crate::store::Store,
    seen: &super::Seen,
    target: &str,
) -> Result<crate::mcp::Output, String> {
    let project = seen.project();
    let card = match super::walk::locate(&project, target, None) {
        Ok(super::walk::Target::Card(card)) if card.kind == crate::kinds::IMAGE => card,
        Ok(_) => return Err(text!("reads.image.notImage", target)),
        Err(e) => return Err(e),
    };
    let fields = card
        .form
        .as_ref()
        .map(|f| f["fields"].clone())
        .unwrap_or_default();
    let Some(hash) = fields["image"].as_str() else {
        return Err(text!("reads.image.empty", target));
    };
    let Some((format, data)) = picture(&store.conn, &seen.summary.id, hash)? else {
        return Err(text!("reads.image.gone", target));
    };
    let mut note = Map::new();
    note.insert("path".into(), project.full(card).into());
    for key in ["format", "width", "height", "bytes", "alt"] {
        if !fields[key].is_null() {
            note.insert(key.into(), fields[key].clone());
        }
    }
    Ok(crate::mcp::Output::Image {
        data,
        mime_type: mime(&format),
        note: Value::Object(note),
    })
}

/// The media type of a picture's form.
pub(crate) fn mime(format: &str) -> String {
    format!("image/{format}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_head_says_the_form_and_the_size() {
        let mut jpeg = vec![
            0xFF, 0xD8, 0xFF, 0xE0, 0, 4, 0, 0, 0xFF, 0xC0, 0, 11, 8, 0, 30, 0, 40,
        ];
        jpeg.extend_from_slice(&[0; 6]);
        assert_eq!(
            head(&jpeg),
            Some(Head {
                format: "jpeg",
                width: 40,
                height: 30,
                whole: false
            })
        );
        assert_eq!(
            head(b"GIF89a\x28\x00\x1e\x00"),
            Some(Head {
                format: "gif",
                width: 40,
                height: 30,
                whole: true
            })
        );
        assert_eq!(head(b"hello"), None);
        assert_eq!(unbase64("aGVsbG8="), Some(b"hello".to_vec()));
        assert_eq!(unbase64("a*b"), None);
    }
}
