//! Who the hub's members are (`members.json` in the hub's place: `{members: [{id, name, key,
//! added}]}`, in the order they were added), and the lines of its user's `~/.ssh/authorized_keys`
//! that let each one reach it: one a member, made from the members each time they change, the
//! other lines of the file left as they are.

use serde_json::{Value, json};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

/// One member as kept.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Member {
    /// What its agent made when it joined; what the hub writes as the sender of its lines.
    pub id: String,
    /// What a person calls it: no two members have one name, in any case.
    pub name: String,
    /// Its public key, without the comment: `ssh-ed25519 <base64>`.
    pub key: String,
    /// When it was added, UTC.
    pub added: String,
    /// When it was last linked, in seconds since 1970; none while it never was, or is now.
    pub seen: Option<u64>,
}

fn file(home: &Path) -> PathBuf {
    home.join("members.json")
}

/// The members kept; none when nothing was kept yet. What is there and cannot be read as the
/// members — cut short, or holding one whose id, name or key could be no member's — is refused:
/// taken for no members, every member's key would go from `authorized_keys`, and an id is written
/// into the command a key runs.
pub fn load(home: &Path) -> Result<Vec<Member>, String> {
    let path = file(home);
    let unreadable = || specboard_core::text!("agent.hub.listUnreadable", path = path.display());
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(_) => return Err(unreadable()),
    };
    let kept: Value = serde_json::from_str(&text).map_err(|_| unreadable())?;
    let list = kept["members"].as_array().ok_or_else(unreadable)?;
    list.iter()
        .map(|m| {
            let (Some(id), Some(name), Some(key)) =
                (m["id"].as_str(), m["name"].as_str(), m["key"].as_str())
            else {
                return Err(unreadable());
            };
            let fits = id_fits(id)
                && name_of(name).as_deref() == Some(name)
                && key_of(key).as_deref() == Some(key);
            if !fits {
                return Err(unreadable());
            }
            Ok(Member {
                id: id.to_string(),
                name: name.to_string(),
                key: key.to_string(),
                added: m["added"].as_str().unwrap_or("").to_string(),
                seen: m["seen"].as_u64(),
            })
        })
        .collect()
}

/// A file written whole or not at all, the user's alone: beside it first, then put in its place.
fn write_whole(path: &Path, text: &str) -> Result<(), String> {
    let failed = |error: std::io::Error| {
        specboard_core::text!("agent.hub.writeFailed", path = path.display(), error)
    };
    let mut beside = path.as_os_str().to_owned();
    beside.push(".writing");
    let beside = PathBuf::from(beside);
    std::fs::write(&beside, text).map_err(failed)?;
    std::fs::set_permissions(&beside, std::fs::Permissions::from_mode(0o600)).map_err(failed)?;
    std::fs::rename(&beside, path).map_err(failed)
}

pub fn save(home: &Path, members: &[Member]) -> Result<(), String> {
    let kept = json!({
        "members": members
            .iter()
            .map(|m| {
                let mut one = json!({"id": m.id, "name": m.name, "key": m.key, "added": m.added});
                if let Some(seen) = m.seen {
                    one["seen"] = seen.into();
                }
                one
            })
            .collect::<Vec<_>>(),
    });
    write_whole(&file(home), &format!("{kept:#}\n"))
}

/// Whether `id` can be a member's: letters, digits, `-` and `_`, 64 at most, beginning with a
/// letter or a digit. It is written into a command a shell runs, so nothing else is taken.
pub fn id_fits(id: &str) -> bool {
    let mut bytes = id.bytes();
    bytes.next().is_some_and(|b| b.is_ascii_alphanumeric())
        && bytes.all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        && id.len() <= 64
}

/// A member's name as kept: trimmed, there, 80 characters at most, on one line.
pub fn name_of(raw: &str) -> Option<String> {
    let name = raw.trim();
    (!name.is_empty() && name.chars().count() <= 80 && !name.chars().any(char::is_control))
        .then(|| name.to_string())
}

/// A member's public key as kept: an ed25519 key's kind and its base64, the comment left out. A
/// key is one line of `authorized_keys`, so what is not exactly a key — a line's end in it above
/// all — is not taken.
pub fn key_of(raw: &str) -> Option<String> {
    if raw.chars().any(char::is_control) {
        return None;
    }
    let mut words = raw.split(' ').filter(|w| !w.is_empty());
    let (kind, body) = (words.next()?, words.next()?);
    let base64 = body.len() >= 16
        && body
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'+' | b'/' | b'='));
    (kind == "ssh-ed25519" && base64).then(|| format!("{kind} {body}"))
}

/// This host's public host keys (`/etc/ssh/ssh_host_*_key.pub`; the folder is
/// `SPECBOARD_HUB_HOST_KEYS`'s when set), each its kind and its base64: what a member is handed to
/// know this host by, and no other.
pub fn host_keys() -> Result<Vec<String>, String> {
    let dir = std::env::var_os("SPECBOARD_HUB_HOST_KEYS")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/etc/ssh"));
    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("ssh_host_") && n.ends_with("_key.pub"))
        })
        .collect();
    files.sort();
    let keys: Vec<String> = files
        .iter()
        .filter_map(|p| std::fs::read_to_string(p).ok())
        .filter_map(|text| {
            let mut words = text.split_whitespace();
            Some(format!("{} {}", words.next()?, words.next()?))
        })
        .collect();
    if keys.is_empty() {
        return Err(specboard_core::text!(
            "agent.hub.noHostKeys",
            dir = dir.display()
        ));
    }
    Ok(keys)
}

/// What marks a line of `authorized_keys` as the hub's: its comment begins with it.
const MARK: &str = "specboard-hub:";

/// The program a member's key runs: the hub's place's own name for it (`specboard-agent`, a link
/// to the build running, which `serve` points anew each time it starts), so that a build put in
/// place of another leaves the keys' lines as they are.
pub fn link(home: &Path) -> PathBuf {
    home.join("specboard-agent")
}

/// The line of `authorized_keys` that lets `member` reach the hub and nothing else: its key may
/// only run the hub's link as that member, with nothing else of SSH (`restrict`).
fn line_of(home: &Path, member: &Member) -> Result<String, String> {
    let link = link(home);
    let path = link.to_string_lossy();
    // Inside `command="…"` a quote is written `\"` and a backslash stands for itself: a path with
    // either, or a line's end, is not written at all rather than written wrongly.
    if path
        .chars()
        .any(|c| c == '"' || c == '\\' || c.is_control())
    {
        return Err(specboard_core::text!(
            "agent.hub.placeUnwritable",
            path = path
        ));
    }
    let plain = path
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b"_@%+=:,./-".contains(&b));
    let program = if plain {
        path.into_owned()
    } else {
        format!("'{}'", path.replace('\'', "'\\''"))
    };
    Ok(format!(
        "command=\"{program} hub attach --as {id}\",restrict {key} {MARK}{id}",
        id = member.id,
        key = member.key
    ))
}

/// The hub's user's `authorized_keys`.
fn authorized_keys() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_default()
        .join(".ssh")
        .join("authorized_keys")
}

/// The hub's lines of `authorized_keys` made to be the members': one each, after the lines that
/// are not the hub's, which stay as they are. Written only when it changes anything.
pub fn sync_keys(home: &Path, members: &[Member]) -> Result<(), String> {
    let path = authorized_keys();
    let had = std::fs::read_to_string(&path).unwrap_or_default();
    let mut lines: Vec<String> = had
        .lines()
        .filter(|line| {
            !line
                .split_whitespace()
                .next_back()
                .is_some_and(|comment| comment.starts_with(MARK))
        })
        .map(str::to_string)
        .collect();
    for member in members {
        lines.push(line_of(home, member)?);
    }
    let mut now = lines.join("\n");
    if !now.is_empty() {
        now.push('\n');
    }
    if now == had {
        return Ok(());
    }
    if let Some(folder) = path.parent()
        && !folder.exists()
    {
        std::fs::create_dir_all(folder)
            .and_then(|()| std::fs::set_permissions(folder, std::fs::Permissions::from_mode(0o700)))
            .map_err(|error| {
                specboard_core::text!("agent.hub.writeFailed", path = folder.display(), error)
            })?;
    }
    write_whole(&path, &now)
}

/// Now, in seconds since 1970.
pub fn seconds() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Now, UTC, as `2026-01-02T03:04:05Z`.
pub fn now() -> String {
    let seconds = seconds() as libc::time_t;
    // SAFETY: gmtime_r writes only into `tm`, which this owns.
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    unsafe { libc::gmtime_r(&seconds, &mut tm) };
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        tm.tm_year + 1900,
        tm.tm_mon + 1,
        tm.tm_mday,
        tm.tm_hour,
        tm.tm_min,
        tm.tm_sec
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_id_a_name_and_a_key_are_held_to_what_a_line_of_authorized_keys_can_carry() {
        for id in ["a", "A1", "mac-1", "9f3c_2", &"x".repeat(64)] {
            assert!(id_fits(id), "{id}");
        }
        for id in [
            "",
            "-a",
            "_a",
            "a b",
            "a;b",
            "a\"b",
            "a/b",
            "é",
            &"x".repeat(65),
        ] {
            assert!(!id_fits(id), "{id}");
        }
        assert_eq!(name_of("  Mac mini  ").as_deref(), Some("Mac mini"));
        assert_eq!(name_of(" "), None);
        assert_eq!(name_of("a\nb"), None);
        assert_eq!(name_of(&"あ".repeat(81)), None);
        let body = "AAAAC3NzaC1lZDI1NTE5AAAAIabc+/=";
        assert_eq!(
            key_of(&format!("ssh-ed25519 {body} someone@host")).as_deref(),
            Some(format!("ssh-ed25519 {body}").as_str())
        );
        for key in [
            format!("ssh-rsa {body}"),
            format!("ssh-ed25519 {body}\ncommand=\"sh\" ssh-ed25519 {body}"),
            format!("ssh-ed25519\t{body}"),
            "ssh-ed25519 short".to_string(),
            format!("ssh-ed25519 {body}\"x"),
            body.to_string(),
        ] {
            assert_eq!(key_of(&key), None, "{key:?}");
        }
    }

    #[test]
    fn a_keys_line_runs_the_hubs_link_as_its_member_and_nothing_else() {
        let member = Member {
            id: "mac-1".into(),
            name: "Mac".into(),
            key: "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIabc".into(),
            added: String::new(),
            seen: None,
        };
        assert_eq!(
            line_of(Path::new("/home/hub/.specboard-hub"), &member).unwrap(),
            "command=\"/home/hub/.specboard-hub/specboard-agent hub attach --as mac-1\",restrict ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIabc specboard-hub:mac-1"
        );
        // A place a shell would read as more than one word is quoted for it.
        assert_eq!(
            line_of(Path::new("/Users/hub user/it's/.specboard-hub"), &member).unwrap(),
            "command=\"'/Users/hub user/it'\\''s/.specboard-hub/specboard-agent' hub attach --as mac-1\",restrict ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIabc specboard-hub:mac-1"
        );
        assert!(line_of(Path::new("/home/a\"b"), &member).is_err());
        assert!(line_of(Path::new("/home/a\\b"), &member).is_err());
    }
}
