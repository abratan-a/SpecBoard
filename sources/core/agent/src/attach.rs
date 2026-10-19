//! The way an app reaches the agent's own (`daemon`), one process for each connection (as
//! Claude Code's SSH): the app starts `specboard-agent attach` over SSH and speaks the
//! daemon's lines on its input and output; this hands them on to the socket and back. The daemon is
//! started first when none runs — let go of the session, so it stays when the app goes — and one of
//! an older build is stopped and started again from this one. One of a newer build is left running,
//! and this one says so and ends: the database a newer core has moved on is not for an older one,
//! and two apps of different builds would each stop the other's. `start`, `stop` and `status` do
//! the same from a shell on the host.

use crate::daemon;
use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// How long a daemon just started has to take its socket, and one told to stop to let it go.
const WAIT: Duration = Duration::from_secs(10);

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
fn start_one(home: &Path, socket: &Path) -> Result<Value, String> {
    std::fs::create_dir_all(home).map_err(|e| {
        specboard_core::text!("agent.run.cannotCreate", path = home.display(), error = e)
    })?;
    let log = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(daemon::log(home))
        .map_err(|e| specboard_core::text!("agent.cli.logFailed", error = e))?;
    let exe = std::env::current_exe()
        .map_err(|e| specboard_core::text!("agent.cli.exeUnknown", error = e))?;
    let mut command = Command::new(exe);
    command
        .arg("serve")
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
    command
        .spawn()
        .map_err(|e| specboard_core::text!("agent.cli.startFailed", error = e))?;
    let begun = Instant::now();
    while begun.elapsed() < WAIT {
        if let Some(said) = who(socket) {
            return Ok(said);
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    Err(specboard_core::text!(
        "agent.cli.noStart",
        log = daemon::log(home).display()
    ))
}

/// The app's build, as a number to tell newer from older: none for a build of the agent's own.
fn build_number(build: &Value) -> Option<u64> {
    build.as_str()?.parse().ok()
}

/// The daemon of this build running: the one there is when it is this build's, else one started.
fn ensure() -> Result<Value, String> {
    let home = daemon::home();
    let socket = daemon::socket(&home);
    let me = daemon::me();
    if let Some(running) = who(&socket) {
        if running["exe"] == me["exe"] {
            return Ok(running);
        }
        if let (Some(theirs), Some(mine)) =
            (build_number(&running["build"]), build_number(&me["build"]))
            && theirs > mine
        {
            return Err(specboard_core::text!(
                "agent.cli.newerRunning",
                build = theirs
            ));
        }
        // Another build, not newer: stopped, and this one started (every app it held reconnects).
        if !stop_one(&socket) {
            return Err(specboard_core::text!("agent.cli.oldWontStop"));
        }
    }
    start_one(&home, &socket)
}

/// `attach`: the app's lines handed to the daemon and back, until either goes.
pub fn attach() -> i32 {
    if let Err(problem) = ensure() {
        println!("{}", json!({"t": "failed", "message": problem}));
        return 1;
    }
    let socket = daemon::socket(&daemon::home());
    let Ok(stream) = UnixStream::connect(&socket) else {
        println!(
            "{}",
            json!({"t": "failed", "message": specboard_core::text!("agent.cli.noConnect")})
        );
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
    match ensure() {
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
    let socket = daemon::socket(&daemon::home());
    if who(&socket).is_none() {
        println!("{}", specboard_core::text!("agent.cli.notRunning"));
        return 0;
    }
    if stop_one(&socket) {
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
    if let Err(problem) = ensure() {
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
