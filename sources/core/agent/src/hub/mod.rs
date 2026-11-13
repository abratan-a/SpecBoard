//! The hub (`specboard://SpecBoard/Docs/Design:DesignHub`): where the agents of one person meet,
//! so that a project one of them holds is read and written from the sessions of another. A role
//! of this same program, run for a user of its own on a host that stays up: `hub serve` is its
//! own (`serve`), reached — like the agent's — through a process a connection.
//!
//! - `hub attach`: the app's, over an SSH login of the hub's user. It starts the hub when none of
//!   this build runs, and adds and removes members.
//! - `hub attach --as <id>`: a member's, started by the forced command of the key the hub put in
//!   `authorized_keys` for it — the one thing that key may do. Who it is comes from the key, so no
//!   agent passes for another. It starts nothing: a hub is started by its user alone.
//!
//! The hub keeps who its members are and which are linked, and hands their lines on. It opens no
//! port and keeps no database: nothing of a project stays here.

pub mod link;
mod members;
mod serve;

use crate::attach::Place;
use serde_json::json;
use std::io::{BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};

/// The form of the lines an agent and the hub speak. Said by both when a link begins; one that
/// speaks another is told which side to renew.
pub const PROTOCOL: u64 = 1;

/// Where the hub of the user it runs as keeps itself.
pub fn home() -> PathBuf {
    if let Some(place) = std::env::var_os("SPECBOARD_HUB_HOME") {
        return PathBuf::from(place);
    }
    let user = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_default();
    user.join(".specboard-hub")
}

pub fn socket(home: &Path) -> PathBuf {
    home.join("hub.sock")
}

pub fn log(home: &Path) -> PathBuf {
    home.join("hub.log")
}

/// `hub <role>`.
pub fn role(args: &[String]) -> i32 {
    let words: Vec<&str> = args.iter().map(String::as_str).collect();
    match words.as_slice() {
        ["serve"] => serve::serve(),
        ["attach"] => crate::attach::attach_to(&Place::hub()),
        ["attach", "--as", id] => attach_as(id),
        ["stop"] => crate::attach::stop_of(&Place::hub()),
        ["status"] => crate::attach::status_of_hub(),
        _ => {
            eprintln!("{}", specboard_core::text!("agent.hub.usage"));
            2
        }
    }
}

/// A member's lines handed to the hub and back, until either goes. The first line the hub reads
/// on the connection is written here, from the id the key's command names: what the agent sends
/// comes after it.
fn attach_as(id: &str) -> i32 {
    let refused = |message: String| {
        println!("{}", json!({"t": "refused", "message": message}));
        1
    };
    let Ok(mut to_hub) = UnixStream::connect(socket(&home())) else {
        return refused(specboard_core::text!("agent.hub.notRunning"));
    };
    if writeln!(to_hub, "{}", json!({"t": "as", "id": id})).is_err() {
        return refused(specboard_core::text!("agent.hub.notRunning"));
    }
    let Ok(from_hub) = to_hub.try_clone() else {
        return 1;
    };
    // The hub's lines to the agent; when the hub closes the link (or goes), this ends saying so,
    // for the agent to link again.
    std::thread::spawn(move || {
        let mut out = std::io::stdout();
        let _ = std::io::copy(&mut BufReader::new(from_hub), &mut out);
        let _ = out.flush();
        std::process::exit(1);
    });
    // The agent's lines to the hub; when the agent goes (its SSH session with it), so does this.
    let mut input = std::io::stdin().lock();
    let _ = std::io::copy(&mut input, &mut to_hub);
    let _ = to_hub.shutdown(std::net::Shutdown::Both);
    0
}
