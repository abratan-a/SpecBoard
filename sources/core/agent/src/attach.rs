//! The way an app reaches the agent's own (`daemon`), one process for each connection (as
//! Claude Code's SSH): the app starts `specboard-agent attach` over SSH and speaks the
//! daemon's lines on its input and output; this hands them on to the socket and back. The daemon is
//! started first when none runs — let go of the session, so it stays when the app goes — and one of
//! an older build is stopped and started again from this one. One of a newer build is left running,
//! and this one says so and ends: the database a newer core has moved on is not for an older one,
//! and two apps of different builds would each stop the other's. `start`, `stop` and `status` do
//! the same from a shell on the host.
//!
//! The hub's own (`hub::serve`) is reached, started and stopped the same way, from its own place
//! (`Place::hub`).

use crate::daemon;
use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// How long a daemon just started has to take its socket, and one told to stop to let it go.
const WAIT: Duration = Duration::from_secs(10);

/// One of this program's two daemons — the agent's own, or the hub — as `attach`, `start`, `stop`
/// and `status` reach it: where it keeps itself, and the role that runs it.
pub(crate) struct Place {
    pub home: PathBuf,
    pub socket: PathBuf,
    pub log: PathBuf,
    /// The arguments that run it (`serve`, or `hub serve`).
    serve: &'static [&'static str],
    hub: bool,
}

impl Place {
    /// The agent's own.
    pub(crate) fn agent() -> Place {
        let home = daemon::home();
        Place {
            socket: daemon::socket(&home),
            log: daemon::log(&home),
            home,
            serve: &["serve"],
            hub: false,
        }
    }

    /// The hub.
    pub(crate) fn hub() -> Place {
        let home = crate::hub::home();
        Place {
            socket: crate::hub::socket(&home),
            log: crate::hub::log(&home),
            home,
            serve: &["hub", "serve"],
            hub: true,
        }
    }

    fn no_start(&self) -> String {
        let log = self.log.display();
        if self.hub {
            specboard_core::text!("agent.hub.noStart", log)
        } else {
            specboard_core::text!("agent.cli.noStart", log)
        }
    }

    fn start_failed(&self, error: impl std::fmt::Display) -> String {
        if self.hub {
            specboard_core::text!("agent.hub.startFailed", error)
        } else {
            specboard_core::text!("agent.cli.startFailed", error)
        }
    }

    fn newer_running(&self, build: u64) -> String {
        if self.hub {
            specboard_core::text!("agent.hub.newerRunning", build)
        } else {
            specboard_core::text!("agent.cli.newerRunning", build)
        }
    }

    fn old_wont_stop(&self) -> String {
        if self.hub {
            specboard_core::text!("agent.hub.oldWontStop")
        } else {
            specboard_core::text!("agent.cli.oldWontStop")
        }
    }

    fn no_connect(&self) -> String {
        if self.hub {
            specboard_core::text!("agent.hub.noConnect")
        } else {
            specboard_core::text!("agent.cli.noConnect")
        }
    }
}

/// The daemon there is, as it says itself; none when nothing answers at the socket.
fn who(socket: &Path) -> Option<Value> {
    let mut stream = UnixStream::connect(socket).ok()?;
    stream.set_read_timeout(Some(WAIT)).ok()?;
    writeln!(stream, "{}", json!({"t": "who"})).ok()?;
    let mut line = String::new();
    BufReader::new(stream).read_line(&mut line).ok()?;
    serde_json::from_str(&line).ok()
}

/// The daemon told to stop, and waited for until it is gone.
fn stop_one(socket: &Path) -> bool {
    let Ok(mut stream) = UnixStream::connect(socket) else {
        return true;
    };
    let _ = writeln!(stream, "{}", json!({"t": "stop"}));
    drop(stream);
    let begun = Instant::now();
    while begun.elapsed() < WAIT {
        if who(socket).is_none() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    false
}

/// A daemon of this build, started let go of this session; waited for until it answers.
fn start_one(place: &Place) -> Result<Value, String> {
    let (home, socket) = (&place.home, &place.socket);
    std::fs::create_dir_all(home).map_err(|e| {
        specboard_core::text!("agent.run.cannotCreate", path = home.display(), error = e)
    })?;
    let log = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&place.log)
        .map_err(|e| specboard_core::text!("agent.cli.logFailed", error = e))?;
    // Where this start's lines begin in the log: what it says when it stops at once is after it.
    let begun_at = log.metadata().map(|m| m.len()).unwrap_or(0);
    let exe = std::env::current_exe()
        .map_err(|e| specboard_core::text!("agent.cli.exeUnknown", error = e))?;
    let mut command = Command::new(exe);
    command
        .args(place.serve)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(log);
    // SAFETY: setsid in the child before exec, so the daemon is in no session of the SSH
    // connection's and stays when it goes.
    unsafe {
        command.pre_exec(|| {
            libc::setsid();
            Ok(())
        });
    }
    let mut child = command.spawn().map_err(|e| place.start_failed(e))?;
    let begun = Instant::now();
    while begun.elapsed() < WAIT {
        if let Some(said) = who(socket) {
            return Ok(said);
        }
        // One that stopped as it started (its database refused, say) is said why, as it wrote it:
        // the app shows it where it shows that the agent cannot be reached.
        if let Ok(Some(status)) = child.try_wait()
            && !status.success()
            && let Some(why) = last_line(&place.log, begun_at)
        {
            return Err(place.start_failed(why));
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    Err(place.no_start())
}

/// The last line written to the log at `path` from `from` on, if any.
fn last_line(path: &Path, from: u64) -> Option<String> {
    let text = std::fs::read(path).ok()?;
    let after = text.get(usize::try_from(from).ok()?..)?;
    String::from_utf8_lossy(after)
        .lines()
        .rev()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(str::to_string)
}

/// The app's build, as a number to tell newer from older: none for a build of the agent's own.
fn build_number(build: &Value) -> Option<u64> {
    build.as_str()?.parse().ok()
}

/// The daemon of this build running: the one there is when it is this build's, else one started.
fn ensure(place: &Place) -> Result<Value, String> {
    let socket = &place.socket;
    let me = daemon::me();
    if let Some(running) = who(socket) {
        if running["exe"] == me["exe"] {
            return Ok(running);
        }
        if let (Some(theirs), Some(mine)) =
            (build_number(&running["build"]), build_number(&me["build"]))
            && theirs > mine
        {
            return Err(place.newer_running(theirs));
        }
        // Another build, not newer: stopped, and this one started (every app it held reconnects).
        if !stop_one(socket) {
            return Err(place.old_wont_stop());
        }
    }
    start_one(place)
}

/// `attach`: the app's lines handed to the daemon and back, until either goes.
pub fn attach() -> i32 {
    attach_to(&Place::agent())
}

/// The app's lines handed to the daemon of `place` — started first when none of this build runs —
/// and back, until either goes.
pub(crate) fn attach_to(place: &Place) -> i32 {
    if let Err(problem) = ensure(place) {
        println!("{}", json!({"t": "failed", "message": problem}));
        return 1;
    }
    let Ok(stream) = UnixStream::connect(&place.socket) else {
        println!("{}", json!({"t": "failed", "message": place.no_connect()}));
        return 1;
    };
    let Ok(mut to_daemon) = stream.try_clone() else {
        return 1;
    };
    // The daemon's lines to the app; when the daemon goes, so does this.
    std::thread::spawn(move || {
        let mut out = std::io::stdout();
        let _ = std::io::copy(&mut BufReader::new(stream), &mut out);
        std::process::exit(0);
    });
    // The app's lines to the daemon; when the app goes (its SSH session with it), so does this.
    let mut input = std::io::stdin().lock();
    let _ = std::io::copy(&mut input, &mut to_daemon);
    let _ = to_daemon.shutdown(std::net::Shutdown::Both);
    0
}

/// `start`: the daemon running, from a shell on the host (after the host started again, say).
pub fn start() -> i32 {
    match ensure(&Place::agent()) {
        Ok(running) => {
            println!(
                "{}",
                specboard_core::text!(
                    "agent.cli.running",
                    pid = running["pid"],
                    exe = running["exe"]
                )
            );
            0
        }
        Err(problem) => {
            eprintln!("{problem}");
            1
        }
    }
}

/// `stop`: the daemon stopped.
pub fn stop() -> i32 {
    stop_of(&Place::agent())
}

/// The daemon of `place` stopped, from a shell.
pub(crate) fn stop_of(place: &Place) -> i32 {
    let socket = &place.socket;
    if who(socket).is_none() {
        println!("{}", specboard_core::text!("agent.cli.notRunning"));
        return 0;
    }
    if stop_one(socket) {
        println!("{}", specboard_core::text!("agent.cli.stopped"));
        0
    } else {
        eprintln!("{}", specboard_core::text!("agent.cli.wontStop"));
        1
    }
}

/// `status`: which runs, if any.
pub fn status() -> i32 {
    match who(&daemon::socket(&daemon::home())) {
        Some(running) => {
            println!(
                "{}",
                specboard_core::text!(
                    "agent.cli.runningBuild",
                    pid = running["pid"],
                    exe = running["exe"],
                    build = running["build"]
                )
            );
            println!("{}", mcp_line(&running["mcp"]));
            0
        }
        None => {
            println!("{}", specboard_core::text!("agent.cli.notRunning"));
            3
        }
    }
}

/// `hub status`: whether the hub runs.
pub(crate) fn status_of_hub() -> i32 {
    match who(&Place::hub().socket) {
        Some(running) => {
            println!(
                "{}",
                specboard_core::text!(
                    "agent.cli.runningBuild",
                    pid = running["pid"],
                    exe = running["exe"],
                    build = running["build"]
                )
            );
            0
        }
        None => {
            println!("{}", specboard_core::text!("agent.cli.notRunning"));
            3
        }
    }
}

/// Where the daemon listens for MCP, as said to a person.
fn mcp_line(mcp: &Value) -> String {
    if mcp["listening"] == true {
        specboard_core::text!("agent.cli.mcpListening", port = mcp["port"])
    } else if mcp["problem"].as_str().is_some_and(|p| !p.is_empty()) {
        specboard_core::text!(
            "agent.cli.mcpNotListeningWhy",
            problem = mcp["problem"].as_str().unwrap_or("")
        )
    } else {
        specboard_core::text!("agent.cli.mcpNotListening")
    }
}

/// `port <n>`: the daemon listens for MCP at that port from now on (the setting kept), started first
/// when none runs.
pub fn port(wanted: Option<&str>) -> i32 {
    let Some(port) = wanted
        .and_then(|p| p.parse::<u16>().ok())
        .filter(|p| *p != 0)
    else {
        eprintln!("{}", specboard_core::text!("agent.cli.portUsage"));
        return 2;
    };
    if let Err(problem) = ensure(&Place::agent()) {
        eprintln!("{problem}");
        return 1;
    }
    let socket = daemon::socket(&daemon::home());
    let Ok(mut stream) = UnixStream::connect(&socket) else {
        eprintln!("{}", specboard_core::text!("agent.cli.noConnect"));
        return 1;
    };
    let _ = stream.set_read_timeout(Some(WAIT));
    if writeln!(stream, "{}", json!({"t": "mcp_port", "port": port})).is_err() {
        eprintln!("{}", specboard_core::text!("agent.cli.noConnect"));
        return 1;
    }
    let mut line = String::new();
    let _ = BufReader::new(stream).read_line(&mut line);
    let said: Value = serde_json::from_str(&line).unwrap_or(Value::Null);
    if let Some(problem) = said["refused"].as_str() {
        eprintln!("{problem}");
        return 1;
    }
    println!("{}", mcp_line(&said["mcp"]));
    0
}
