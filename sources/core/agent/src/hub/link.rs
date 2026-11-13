//! An agent's link to the hub it joined (`specboard://SpecBoard/Docs/Design:DesignHub` §2・§9).
//!
//! What it is in the hub is kept in its place's `hub/`: a key of its own (`key`, `key.pub`: made
//! here by `ssh-keygen`, the private half never leaving this host), who it is and where the hub is
//! (`member.json`: `{id, to}`), and the hub's host's public host keys as the app handed them over
//! (`known_hosts`) — the one host its ssh takes for the hub, whatever the user's own files say.
//!
//! Joined, one `ssh` out to the hub is kept by a thread of its own, put back when it drops after
//! 1, 2, 4… seconds (60 at most; from 1 again after one that stayed up longer than that). Its key
//! may only run the hub's `hub attach --as <id>` there, whose lines this speaks (`hub::serve`):
//!
//! - who is there (`ready`, `members`) is handed to the core (`hub_state`), which lets go of what
//!   a member no longer there left;
//! - what each other member is to be told this agent shares with it (`hub_outgoing`) is sent when
//!   the member comes — by a connection that is not the one it was told by, too —, and when the
//!   shares or a project's name change (the core's notices); what another says it shares is handed
//!   to the core (`hub_incoming`);
//! - a call another member hands over is run by the core as that member (`hub_call`), on a thread
//!   of its own — one that takes long keeps no other waiting — and answered by the call's number;
//! - a call of this agent's own sessions for a project another shares is sent to its owner and
//!   waited for (`specboard_core::Hub`): until its owner — no other member — answers it, the hub
//!   says it will not be, this link drops, or `ANSWER_WAIT` is over. A call let go then is said
//!   not to be known to have arrived, never to have failed: its owner may have done it.
//!
//! The apps are told where it stands (`{"t":"hub", …}`) each time that changes.

use serde::Deserialize;
use serde_json::value::RawValue;
use serde_json::{Value, json};
use specboard_core::{Core, Hub, Listener};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// The longest wait before the link is put back.
const LONGEST: u64 = 60;

/// How often the keeper looks at whether it is let go while it waits.
const LOOK: Duration = Duration::from_millis(100);

/// How long a call asked once more waits for the link to be back.
const BACK_WAIT: Duration = Duration::from_secs(20);

/// How long a call waits for its owner to answer: more than twice the longest an owner waits
/// itself (the two minutes its person has to answer a request), so that only an owner that does
/// not answer at all is given up on — whose calls would else hold this agent's MCP connections,
/// which are counted, for good.
const ANSWER_WAIT: Duration = Duration::from_secs(300);

/// The most calls of other agents run at once: more are told to come again.
const CALLS_MOST: usize = 64;

/// The longest a line from the hub may be (`hub::serve`'s own).
const LINE_MOST: u64 = 64 << 20;

/// The name the hub's host is known by in `known_hosts`, whatever it is reached as.
const HOST_ALIAS: &str = "specboard-hub";

/// Where the link stands.
#[derive(Clone)]
enum Is {
    /// Not joined.
    Off,
    Starting,
    Up {
        since: u64,
    },
    Down {
        at: u64,
        why: String,
        retry_at: u64,
    },
}

/// Who this agent is in the hub, and where the hub is once it joined (`member.json`).
#[derive(Clone)]
struct Me {
    id: String,
    to: Option<String>,
}

struct State {
    me: Option<Me>,
    is: Is,
    /// The hub's members as it last said them.
    members: Vec<Value>,
}

/// What the threads that wait are woken for.
#[derive(Default)]
struct Woken {
    /// The link is up.
    up: bool,
    /// What each member is to be told may have changed.
    shares: bool,
}

type Tell = Box<dyn Fn() + Send + Sync>;

/// A call sent and not answered yet: the member it was sent to, and where its answer goes.
type Waiting = (String, Sender<String>);

/// The call `call` taken as answered by the member `from`: where its answer goes. None, and the
/// call left waiting, when `from` is not the member it was sent to — a call's number is no secret,
/// and no member answers for another.
fn answered(
    pending: &Mutex<HashMap<u64, Waiting>>,
    from: &str,
    call: u64,
) -> Option<Sender<String>> {
    let mut pending = locked(pending);
    pending
        .get(&call)
        .is_some_and(|(owner, _)| owner == from)
        .then(|| pending.remove(&call))
        .flatten()
        .map(|(_, sender)| sender)
}

/// What its owner answered to the call `call`, waited for no longer than `wait`: `{"failed":
/// "slow"}` once that is over. The call's number then waits for nothing, and what its owner says
/// late goes nowhere.
fn awaited(
    pending: &Mutex<HashMap<u64, Waiting>>,
    call: u64,
    answer: &Receiver<String>,
    wait: Duration,
) -> String {
    match answer.recv_timeout(wait) {
        Ok(said) => said,
        Err(RecvTimeoutError::Disconnected) => r#"{"failed":"cut"}"#.to_string(),
        Err(RecvTimeoutError::Timeout) => {
            locked(pending).remove(&call);
            // One that came as the wait ended is the call's answer still.
            answer
                .try_recv()
                .unwrap_or_else(|_| r#"{"failed":"slow"}"#.to_string())
        }
    }
}

/// Where a run's calls are numbered from: past any number a run before it used, so that what an
/// owner answers late to a call of that run is not taken for the answer to one of this.
fn first_call() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_micros() as u64)
        .unwrap_or(0)
}

pub struct Link {
    /// The agent's place.
    home: PathBuf,
    core: Arc<Core>,
    /// This agent's build, said to the hub and to the members' cores.
    build: String,
    state: Mutex<State>,
    /// The way to the hub while linked: the ssh's input.
    out: Mutex<Option<ChildStdin>>,
    child: Mutex<Option<Child>>,
    /// Whether the keeper running now is let go: each run has its own.
    stop: Mutex<Arc<AtomicBool>>,
    /// The calls sent and not answered yet.
    pending: Mutex<HashMap<u64, Waiting>>,
    next: AtomicU64,
    /// What each member was last told this agent shares with it, and by which of its connections
    /// it was there then (`link`, as the hub numbers them): told again to another.
    sent: Mutex<HashMap<String, (Option<u64>, Value)>>,
    /// How many calls of other agents run now.
    running: AtomicUsize,
    told: std::sync::OnceLock<Tell>,
    woken: Mutex<Woken>,
    wake: Condvar,
}

/// One line from the hub: what this reads of it.
#[derive(Deserialize)]
struct Line<'a> {
    t: String,
    from: Option<String>,
    #[serde(borrow)]
    id: Option<&'a RawValue>,
    #[serde(borrow)]
    re: Option<&'a RawValue>,
    #[serde(borrow)]
    body: Option<&'a RawValue>,
    members: Option<Vec<Value>>,
    why: Option<String>,
    message: Option<String>,
}

/// What a member's line carries, by its kind: the rest of a call is the core's to read.
#[derive(Deserialize)]
struct Body<'a> {
    k: String,
    #[serde(borrow)]
    projects: Option<&'a RawValue>,
    lang: Option<String>,
}

/// The core's answer to a request, its halves as written.
#[derive(Deserialize)]
struct Answered<'a> {
    #[serde(borrow)]
    result: Option<&'a RawValue>,
    #[serde(borrow)]
    error: Option<&'a RawValue>,
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// A text as JSON writes it.
fn quoted(text: &str) -> String {
    Value::String(text.to_string()).to_string()
}

fn locked<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|e| e.into_inner())
}

/// The folder of the agent's place that holds what it is in the hub.
fn place(home: &Path) -> PathBuf {
    home.join("hub")
}

/// Who it is as kept; none when it has no key yet.
fn kept(home: &Path) -> Option<Me> {
    let text = std::fs::read_to_string(place(home).join("member.json")).ok()?;
    let said: Value = serde_json::from_str(&text).ok()?;
    Some(Me {
        id: said["id"].as_str().filter(|id| !id.is_empty())?.to_string(),
        to: said["to"].as_str().map(str::to_string),
    })
}

fn keep(home: &Path, me: &Me) -> Result<(), String> {
    let path = place(home).join("member.json");
    let mut said = json!({"id": me.id});
    if let Some(to) = &me.to {
        said["to"] = to.clone().into();
    }
    std::fs::write(&path, format!("{said:#}\n")).map_err(|error| {
        specboard_core::text!("agent.hub.writeFailed", path = path.display(), error)
    })
}

/// An id no other agent has: 64 bits of the system's randomness, as hex.
fn fresh_id() -> String {
    let mut bytes = [0u8; 8];
    let read = std::fs::File::open("/dev/urandom").and_then(|mut f| f.read_exact(&mut bytes));
    if read.is_err() {
        // No randomness to read: the time and this process, which no other agent shares.
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0);
        bytes = (nanos ^ (u64::from(std::process::id()) << 32)).to_le_bytes();
    }
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Where the hub is, as the host's ssh is handed it: one word, and not one ssh would read as an
/// option.
fn destination(raw: &str) -> Option<String> {
    let to = raw.trim();
    (!to.is_empty()
        && !to.starts_with('-')
        && !to.chars().any(|c| c.is_whitespace() || c.is_control()))
    .then(|| to.to_string())
}

/// The host's ssh out to the hub: with this agent's key alone, the hub's host known by what the
/// app handed over alone, and a connection of its own (not one shared through a master of the
/// user's config, out of reach of the process this keeps). The user's config is read for how the
/// destination is reached.
fn ssh(home: &Path, to: &str) -> Command {
    let place = place(home);
    let mut command =
        Command::new(std::env::var_os("SPECBOARD_SSH").unwrap_or_else(|| "ssh".into()));
    command.arg("-T").arg("-i").arg(place.join("key"));
    for option in [
        "IdentitiesOnly=yes",
        "IdentityAgent=none",
        "PreferredAuthentications=publickey",
        "BatchMode=yes",
        "ConnectTimeout=10",
        "ControlMaster=no",
        "ControlPath=none",
        "StrictHostKeyChecking=yes",
        "GlobalKnownHostsFile=/dev/null",
        "ServerAliveInterval=15",
        "ServerAliveCountMax=3",
    ] {
        command.args(["-o", option]);
    }
    command.args(["-o", &format!("HostKeyAlias={HOST_ALIAS}")]);
    // ssh reads the value as words: a path with a space in it is quoted for it.
    let known = place.join("known_hosts").display().to_string();
    let known = if known.contains(char::is_whitespace) {
        format!("\"{known}\"")
    } else {
        known
    };
    command.args(["-o", &format!("UserKnownHostsFile={known}")]);
    command.arg("--").arg(to);
    command
}

/// On Linux, the ssh is ended when the daemon is, however it ends: one left behind would stay
/// linked as this agent with nothing behind it.
fn ends_with_the_daemon(command: &mut Command) {
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::process::CommandExt;
        // SAFETY: prctl only sets a flag of the child about to exec; nothing else is touched.
        unsafe {
            command.pre_exec(|| {
                libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM);
                Ok(())
            });
        }
    }
    #[cfg(not(target_os = "linux"))]
    let _ = command;
}

/// One line of `reader`, without its end; false when it ended, or the line is longer than a line
/// may be.
fn read_line(reader: &mut impl BufRead, line: &mut Vec<u8>) -> bool {
    line.clear();
    match reader.take(LINE_MOST + 1).read_until(b'\n', line) {
        Ok(n) if n > 0 && line.last() == Some(&b'\n') => {
            line.pop();
            true
        }
        _ => false,
    }
}

impl Link {
    /// The link of the agent at `home`: kept from now on when it has joined a hub. The core is
    /// told who this agent is there (nobody, when it joined none: what it kept of a hub goes).
    pub fn start(core: Arc<Core>, home: &Path) -> Arc<Link> {
        let me = kept(home);
        let joined = me.as_ref().is_some_and(|me| me.to.is_some());
        let link = Arc::new(Link {
            home: home.to_path_buf(),
            core,
            build: crate::daemon::me()["build"]
                .as_str()
                .unwrap_or_default()
                .to_string(),
            state: Mutex::new(State {
                me,
                is: if joined { Is::Starting } else { Is::Off },
                members: Vec::new(),
            }),
            out: Mutex::new(None),
            child: Mutex::new(None),
            stop: Mutex::new(Arc::new(AtomicBool::new(!joined))),
            pending: Mutex::new(HashMap::new()),
            next: AtomicU64::new(first_call()),
            sent: Mutex::new(HashMap::new()),
            running: AtomicUsize::new(0),
            told: std::sync::OnceLock::new(),
            woken: Mutex::new(Woken::default()),
            wake: Condvar::new(),
        });
        link.tell_core(false);
        {
            let link = link.clone();
            std::thread::spawn(move || link.keep_sharing());
        }
        link.keep_linked();
        link
    }

    /// Who is told whenever where it stands changes.
    pub fn tell_with(&self, tell: impl Fn() + Send + Sync + 'static) {
        let _ = self.told.set(Box::new(tell));
    }

    fn tell(&self) {
        if let Some(tell) = self.told.get() {
            tell();
        }
    }

    /// Where it stands, as the apps are told it: `{joined, id?, to?, link: {is, since?, at?, why?,
    /// retryAt?}, members}`; the times in seconds since 1970.
    pub fn said(&self) -> Value {
        let state = locked(&self.state);
        let mut said = json!({
            "joined": state.me.as_ref().is_some_and(|me| me.to.is_some()),
        });
        if let Some(me) = &state.me {
            said["id"] = me.id.clone().into();
            if let Some(to) = &me.to {
                said["to"] = to.clone().into();
            }
        }
        said["link"] = match &state.is {
            Is::Off => json!({"is": "off"}),
            Is::Starting => json!({"is": "starting"}),
            Is::Up { since } => json!({"is": "up", "since": since}),
            Is::Down { at, why, retry_at } => {
                json!({"is": "down", "at": at, "why": why, "retryAt": retry_at})
            }
        };
        said["members"] = Value::Array(state.members.clone());
        said
    }

    /// This agent's key and id in the hub, made when it has none yet: `(id, public key)`.
    pub fn key(&self) -> Result<(String, String), String> {
        let place = place(&self.home);
        let failed = |path: &Path, error: std::io::Error| {
            specboard_core::text!("agent.hub.writeFailed", path = path.display(), error)
        };
        std::fs::create_dir_all(&place).map_err(|e| failed(&place, e))?;
        std::fs::set_permissions(&place, std::fs::Permissions::from_mode(0o700))
            .map_err(|e| failed(&place, e))?;
        let key = place.join("key");
        if !key.exists() || !place.join("key.pub").exists() {
            let _ = std::fs::remove_file(&key);
            let _ = std::fs::remove_file(place.join("key.pub"));
            let made = Command::new(
                std::env::var_os("SPECBOARD_SSH_KEYGEN").unwrap_or_else(|| "ssh-keygen".into()),
            )
            .args(["-q", "-t", "ed25519", "-N", "", "-C", HOST_ALIAS, "-f"])
            .arg(&key)
            .stdin(Stdio::null())
            .output();
            match made {
                Ok(out) if out.status.success() => {}
                Ok(out) => {
                    return Err(specboard_core::text!(
                        "agent.hub.keygenFailed",
                        error = String::from_utf8_lossy(&out.stderr).trim()
                    ));
                }
                Err(error) => {
                    return Err(specboard_core::text!("agent.hub.keygenFailed", error));
                }
            }
            std::fs::set_permissions(&key, std::fs::Permissions::from_mode(0o600))
                .map_err(|e| failed(&key, e))?;
        }
        let public = std::fs::read_to_string(place.join("key.pub"))
            .map_err(|e| failed(&place.join("key.pub"), e))?;
        let mut words = public.split_whitespace();
        let (Some(kind), Some(body)) = (words.next(), words.next()) else {
            return Err(specboard_core::text!(
                "agent.hub.keygenFailed",
                error = "key.pub"
            ));
        };
        let mut state = locked(&self.state);
        let id = match &state.me {
            Some(me) => me.id.clone(),
            None => {
                let me = Me {
                    id: fresh_id(),
                    to: None,
                };
                keep(&self.home, &me)?;
                let id = me.id.clone();
                state.me = Some(me);
                id
            }
        };
        Ok((id, format!("{kind} {body}")))
    }

    /// Joined: where the hub is (`to`, as this host's ssh names it) and its host's public host
    /// keys kept, and the link made from now on.
    pub fn join(self: &Arc<Self>, to: &str, host_keys: &Value) -> Result<(), String> {
        let to = destination(to).ok_or_else(|| specboard_core::text!("agent.hub.badTo"))?;
        let mut known = String::new();
        for key in host_keys.as_array().into_iter().flatten() {
            let line = key.as_str().unwrap_or("");
            let mut words = line.split(' ');
            let fits = matches!((words.next(), words.next(), words.next()), (Some(kind), Some(body), None)
                if !kind.is_empty() && !body.is_empty())
                && !line.chars().any(char::is_control);
            if !fits {
                return Err(specboard_core::text!("agent.hub.noHostKeysGiven"));
            }
            known.push_str(&format!("{HOST_ALIAS} {line}\n"));
        }
        if known.is_empty() {
            return Err(specboard_core::text!("agent.hub.noHostKeysGiven"));
        }
        let me = {
            let state = locked(&self.state);
            let Some(me) = &state.me else {
                return Err(specboard_core::text!("agent.hub.noKeyYet"));
            };
            Me {
                id: me.id.clone(),
                to: Some(to),
            }
        };
        let path = place(&self.home).join("known_hosts");
        std::fs::write(&path, known).map_err(|error| {
            specboard_core::text!("agent.hub.writeFailed", path = path.display(), error)
        })?;
        keep(&self.home, &me)?;
        self.let_go();
        {
            let mut state = locked(&self.state);
            state.me = Some(me);
            state.is = Is::Starting;
        }
        *locked(&self.stop) = Arc::new(AtomicBool::new(false));
        self.tell_core(false);
        self.keep_linked();
        self.tell();
        Ok(())
    }

    /// Left: the link let go, and nothing of the hub kept — the key, who it was, where the hub is,
    /// and what the core kept of the members, the shares and the projects shared with it.
    pub fn leave(&self) {
        self.let_go();
        let _ = std::fs::remove_dir_all(place(&self.home));
        {
            let mut state = locked(&self.state);
            state.me = None;
            state.is = Is::Off;
            state.members.clear();
        }
        self.tell_core(false);
        self.tell();
    }

    /// The keeper and its ssh let go: as the daemon ends, and before the link is made anew.
    pub fn let_go(&self) {
        // Held against what the run does to what the next run shares with it (`while_kept`): it
        // has done it by now, or will not.
        locked(&self.stop).store(true, Ordering::SeqCst);
        if let Some(mut child) = locked(&self.child).take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        self.dropped();
    }

    /// The core told who this agent is in the hub, whether it is linked, and who is there.
    fn tell_core(&self, up: bool) {
        let (me, members) = {
            let state = locked(&self.state);
            (
                state.me.as_ref().map(|me| me.id.clone()),
                if up {
                    state.members.clone()
                } else {
                    Vec::new()
                },
            )
        };
        self.core.call(
            &json!({"op": "hub_state", "self": me, "build": self.build, "up": up, "members": members})
                .to_string(),
        );
    }

    /// One line to the hub; false when not linked, or it went as this was written.
    fn say(&self, line: &str) -> bool {
        let mut out = locked(&self.out);
        let Some(to_hub) = out.as_mut() else {
            return false;
        };
        let written = to_hub
            .write_all(line.as_bytes())
            .and_then(|()| to_hub.write_all(b"\n"))
            .and_then(|()| to_hub.flush());
        if written.is_err() {
            *out = None;
        }
        written.is_ok()
    }

    /// The link gone: nothing more is sent by it, every call waiting is told its way was cut, and
    /// what each member was told is forgotten (told again when it is back).
    fn dropped(&self) {
        *locked(&self.out) = None;
        locked(&self.woken).up = false;
        let waiting: Vec<Sender<String>> = locked(&self.pending)
            .drain()
            .map(|(_, (_, sender))| sender)
            .collect();
        for sender in waiting {
            let _ = sender.send(r#"{"failed":"cut"}"#.to_string());
        }
        locked(&self.sent).clear();
        // Who is there is not known while it is not linked.
        for member in locked(&self.state).members.iter_mut() {
            if let Some(member) = member.as_object_mut() {
                member.insert("online".into(), false.into());
                member.remove("build");
                member.remove("since");
                member.remove("link");
            }
        }
    }

    /// `act` done unless the run `stop` is the flag of was let go: what a run does to the link's
    /// own — where it stands, its ssh, the way to the hub — it does through this, so that a run
    /// let go, still ending on its thread, never undoes what the run after it has set up.
    fn while_kept<T>(&self, stop: &AtomicBool, act: impl FnOnce() -> T) -> Option<T> {
        let _held = locked(&self.stop);
        (!stop.load(Ordering::SeqCst)).then(act)
    }

    /// Where the link stands, said to the apps.
    fn stand(&self, stop: &AtomicBool, is: Is) {
        if self
            .while_kept(stop, || locked(&self.state).is = is)
            .is_some()
        {
            self.tell();
        }
    }

    /// The link kept by a thread of its own, when this agent has joined a hub.
    fn keep_linked(self: &Arc<Self>) {
        let me = locked(&self.state).me.clone();
        let Some(Me { to: Some(to), .. }) = me else {
            return;
        };
        let stop = locked(&self.stop).clone();
        let link = self.clone();
        std::thread::spawn(move || {
            let mut delay = 1;
            while !stop.load(Ordering::SeqCst) {
                let (why, up_since) = link.linked_once(&stop, &to);
                let gone = link.while_kept(&stop, || {
                    link.dropped();
                    link.tell_core(false);
                });
                if gone.is_none() {
                    return;
                }
                if up_since.is_some_and(|since| now().saturating_sub(since) > LONGEST) {
                    delay = 1;
                }
                let at = now();
                link.stand(
                    &stop,
                    Is::Down {
                        at,
                        why,
                        retry_at: at + delay,
                    },
                );
                let end = Instant::now() + Duration::from_secs(delay);
                while Instant::now() < end {
                    if stop.load(Ordering::SeqCst) {
                        return;
                    }
                    std::thread::sleep(LOOK);
                }
                link.stand(&stop, Is::Starting);
                delay = (delay * 2).min(LONGEST);
            }
        });
    }

    /// One ssh to the hub, its lines answered until it ends: why it ended, and since when it was
    /// up if it ever was.
    fn linked_once(self: &Arc<Self>, stop: &AtomicBool, to: &str) -> (String, Option<u64>) {
        let mut command = ssh(&self.home, to);
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        ends_with_the_daemon(&mut command);
        let mut spawned = match command.spawn() {
            Ok(spawned) => spawned,
            Err(error) => return (error.to_string(), None),
        };
        let (to_hub, from_hub, said) = (
            spawned.stdin.take(),
            spawned.stdout.take(),
            spawned.stderr.take(),
        );
        // The link's own from here on — unless this run was let go while it was being started:
        // `let_go` found none to end.
        let mut spawned = Some(spawned);
        let kept = self.while_kept(stop, || {
            *locked(&self.out) = to_hub;
            *locked(&self.child) = spawned.take();
        });
        if kept.is_none() {
            if let Some(mut spawned) = spawned {
                let _ = spawned.kill();
                let _ = spawned.wait();
            }
            return (String::new(), None);
        }
        // What ssh says of why it could not, read apart: it may say more than a pipe holds.
        let said = std::thread::spawn(move || {
            let mut text = String::new();
            if let Some(said) = said {
                let _ = said.take(64 << 10).read_to_string(&mut text);
            }
            text
        });
        self.say(
            &json!({"t": "hello", "protocol": super::PROTOCOL, "build": self.build}).to_string(),
        );
        let mut refused = None;
        let mut up_since = None;
        if let Some(from_hub) = from_hub {
            let mut reader = BufReader::new(from_hub);
            let mut line = Vec::new();
            while read_line(&mut reader, &mut line) {
                let Ok(said) = serde_json::from_slice::<Line>(&line) else {
                    continue;
                };
                match said.t.as_str() {
                    "ready" | "members" => {
                        let first = said.t == "ready";
                        let members = said.members.unwrap_or_default();
                        let kept = self.while_kept(stop, || {
                            let since = {
                                let mut state = locked(&self.state);
                                state.members = members;
                                first.then(|| {
                                    let since = now();
                                    state.is = Is::Up { since };
                                    since
                                })
                            };
                            self.tell_core(true);
                            let mut woken = locked(&self.woken);
                            woken.up = true;
                            woken.shares = true;
                            since
                        });
                        let Some(since) = kept else {
                            break;
                        };
                        up_since = since.or(up_since);
                        self.wake.notify_all();
                        self.tell();
                    }
                    "from" => self.from(&said),
                    "undelivered" => {
                        let call = said.id.and_then(|id| id.get().parse::<u64>().ok());
                        if let Some((_, sender)) =
                            call.and_then(|id| locked(&self.pending).remove(&id))
                        {
                            // Handed to its owner and lost with the owner's link: whether it was
                            // done is not known. Never handed: the owner is away.
                            let why = match said.why.as_deref() {
                                Some("cut") => "cut",
                                _ => "offline",
                            };
                            let _ = sender.send(format!(r#"{{"failed":"{why}"}}"#));
                        }
                    }
                    "ping" => {
                        self.say(r#"{"t":"pong"}"#);
                    }
                    "refused" => refused = said.message,
                    _ => {}
                }
            }
        }
        let code = self
            .while_kept(stop, || {
                *locked(&self.out) = None;
                locked(&self.child).take().and_then(|mut child| {
                    let _ = child.kill();
                    child.wait().ok().and_then(|status| status.code())
                })
            })
            .flatten();
        let said = said.join().unwrap_or_default();
        let why = refused.unwrap_or_else(|| {
            said.lines()
                .map(str::trim)
                .rfind(|l| !l.is_empty())
                .map(str::to_string)
                .unwrap_or_else(|| {
                    specboard_core::text!(
                        "agent.hub.linkEnded",
                        code = code.map_or("-".to_string(), |c| c.to_string())
                    )
                })
        });
        (why, up_since)
    }

    /// A line another member sent: the answer to a call of this agent's, what it shares with this
    /// agent, or a call for this agent's core to run as that member.
    fn from(self: &Arc<Self>, said: &Line) {
        let (Some(from), Some(body)) = (&said.from, said.body) else {
            return;
        };
        if let Some(re) = said.re {
            let call = re.get().parse::<u64>().ok();
            if let Some(sender) = call.and_then(|id| answered(&self.pending, from, id)) {
                let _ = sender.send(body.get().to_string());
            }
            return;
        }
        let Ok(head) = serde_json::from_str::<Body>(body.get()) else {
            return;
        };
        match head.k.as_str() {
            "shares" => {
                self.core.call(&format!(
                    r#"{{"op":"hub_incoming","owner":{},"projects":{}}}"#,
                    quoted(from),
                    head.projects.map_or("[]", RawValue::get)
                ));
            }
            "call" => {
                let Some(call) = said.id else {
                    return;
                };
                let answer_to = format!(r#"{{"t":"to","to":{},"re":{}"#, quoted(from), call.get());
                if self.running.fetch_add(1, Ordering::SeqCst) >= CALLS_MOST {
                    self.running.fetch_sub(1, Ordering::SeqCst);
                    let _lang = specboard_core::text::bind(head.lang.as_deref());
                    let busy = quoted(&specboard_core::text!("agent.hub.busy"));
                    self.say(&format!(r#"{answer_to},"body":{{"error":{busy}}}}}"#));
                    return;
                }
                let request = format!(
                    r#"{{"op":"hub_call","member":{},"request":{}}}"#,
                    quoted(from),
                    body.get()
                );
                let link = self.clone();
                std::thread::spawn(move || {
                    let answer = link.core.call(&request);
                    link.running.fetch_sub(1, Ordering::SeqCst);
                    let body = match serde_json::from_str::<Answered>(&answer) {
                        Ok(Answered {
                            result: Some(result),
                            ..
                        }) => result.get().to_string(),
                        Ok(Answered {
                            error: Some(error), ..
                        }) => format!(r#"{{"error":{}}}"#, error.get()),
                        _ => r#"{"error":"?"}"#.to_string(),
                    };
                    link.say(&format!(r#"{answer_to},"body":{body}}}"#));
                });
            }
            _ => {}
        }
    }

    /// Each member there told what this agent shares with it, when that is not what it was told
    /// last: as the link comes up, as a member comes, and as the core says the shares or a project
    /// changed.
    fn keep_sharing(&self) {
        loop {
            {
                let mut woken = locked(&self.woken);
                while !woken.shares {
                    woken = self.wake.wait(woken).unwrap_or_else(|e| e.into_inner());
                }
                woken.shares = false;
            }
            // What changes together is told together.
            std::thread::sleep(Duration::from_millis(50));
            let (me, there): (Option<String>, HashMap<String, Option<u64>>) = {
                let state = locked(&self.state);
                (
                    state.me.as_ref().map(|me| me.id.clone()),
                    state
                        .members
                        .iter()
                        .filter(|m| m["online"] == true)
                        .filter_map(|m| Some((m["id"].as_str()?.to_string(), m["link"].as_u64())))
                        .collect(),
                )
            };
            if !locked(&self.woken).up {
                continue;
            }
            let answer: Value = serde_json::from_str(&self.core.call(r#"{"op":"hub_outgoing"}"#))
                .unwrap_or(Value::Null);
            let Some(outgoing) = answer["result"].as_object() else {
                continue;
            };
            // Held while it is sent: a link that drops meanwhile forgets what was told after this.
            let mut sent = locked(&self.sent);
            // One that went, or is there by another connection than the one that was told — what
            // was said to a connection half gone may never have been read — is told anew.
            sent.retain(|member, (link, _)| there.get(member) == Some(link));
            for (member, projects) in outgoing {
                let Some(link) = there.get(member) else {
                    continue;
                };
                if Some(member) == me.as_ref()
                    || sent.get(member).is_some_and(|(_, told)| told == projects)
                {
                    continue;
                }
                let line = format!(
                    r#"{{"t":"to","to":{},"body":{{"k":"shares","projects":{projects}}}}}"#,
                    quoted(member)
                );
                if self.say(&line) {
                    sent.insert(member.clone(), (*link, projects.clone()));
                }
            }
        }
    }
}

impl Hub for Link {
    fn call(&self, owner: &str, request: &str, wait: bool) -> String {
        let failed = |why: &str| format!(r#"{{"failed":"{why}"}}"#);
        if wait {
            // Asked once more after its way was cut: once the link is back, if it comes back.
            let end = Instant::now() + BACK_WAIT;
            let mut woken = locked(&self.woken);
            while !woken.up {
                let left = end.saturating_duration_since(Instant::now());
                if left.is_zero() {
                    return failed("down");
                }
                woken = self
                    .wake
                    .wait_timeout(woken, left)
                    .unwrap_or_else(|e| e.into_inner())
                    .0;
            }
        }
        let call = self.next.fetch_add(1, Ordering::SeqCst) + 1;
        let (sender, answer) = channel();
        locked(&self.pending).insert(call, (owner.to_string(), sender));
        let line = format!(
            r#"{{"t":"to","to":{},"id":{call},"body":{request}}}"#,
            quoted(owner)
        );
        if !self.say(&line) {
            locked(&self.pending).remove(&call);
            return failed("down");
        }
        awaited(&self.pending, call, &answer, ANSWER_WAIT)
    }
}

impl Listener for Link {
    /// What the core tells: the shares, the members or a project changed — what each member is
    /// to be told may have.
    fn changed(&self, notice: &str) {
        if notice.contains(r#""op":"hub""#) || notice.contains(r#""op":"changed""#) {
            locked(&self.woken).shares = true;
            self.wake.notify_all();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_call_is_answered_only_by_the_member_it_was_sent_to() {
        let pending = Mutex::new(HashMap::new());
        let (sender, _answer) = channel();
        locked(&pending).insert(7, ("srv".to_string(), sender));
        // Another member's answer by the call's number leaves it waiting for its owner's.
        assert!(answered(&pending, "other", 7).is_none());
        assert!(answered(&pending, "srv", 8).is_none());
        assert!(answered(&pending, "srv", 7).is_some());
        assert!(answered(&pending, "srv", 7).is_none());
    }

    #[test]
    fn a_call_its_owner_does_not_answer_is_let_go_when_the_wait_is_over() {
        let pending = Arc::new(Mutex::new(HashMap::new()));
        let (sender, answer) = channel();
        locked(&pending).insert(7, ("srv".to_string(), sender));
        // Waited for on a thread of its own: a call never let go fails the test, and does not hold it.
        let (done, said) = channel();
        let waiting = pending.clone();
        std::thread::spawn(move || {
            let _ = done.send(awaited(&waiting, 7, &answer, Duration::from_millis(50)));
        });
        assert_eq!(
            said.recv_timeout(Duration::from_secs(5)).as_deref(),
            Ok(r#"{"failed":"slow"}"#)
        );
        // Its number waits for nothing: what its owner says late goes nowhere.
        assert!(answered(&pending, "srv", 7).is_none());

        // Answered within the wait, it is what its owner said.
        let (sender, answer) = channel();
        locked(&pending).insert(8, ("srv".to_string(), sender));
        answered(&pending, "srv", 8)
            .unwrap()
            .send("{}".to_string())
            .unwrap();
        assert_eq!(awaited(&pending, 8, &answer, Duration::from_secs(5)), "{}");
    }

    #[test]
    fn a_destination_is_one_word_that_is_no_option() {
        assert_eq!(
            destination(" sbhub@hub.example ").as_deref(),
            Some("sbhub@hub.example")
        );
        assert_eq!(destination("hub").as_deref(), Some("hub"));
        for to in ["", " ", "-oProxyCommand=id", "a b", "a\nb", "-"] {
            assert_eq!(destination(to), None, "{to:?}");
        }
    }

    #[test]
    fn the_ssh_to_the_hub_uses_this_agents_key_and_the_hubs_host_keys_alone() {
        let command = ssh(Path::new("/home/me/.specboard"), "sbhub@hub");
        let args: Vec<String> = command
            .get_args()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        let said = args.join(" ");
        assert!(
            said.starts_with("-T -i /home/me/.specboard/hub/key "),
            "{said}"
        );
        assert!(said.ends_with(" -- sbhub@hub"), "{said}");
        for option in [
            "IdentitiesOnly=yes",
            "IdentityAgent=none",
            "BatchMode=yes",
            "ControlMaster=no",
            "ControlPath=none",
            "StrictHostKeyChecking=yes",
            "GlobalKnownHostsFile=/dev/null",
            "HostKeyAlias=specboard-hub",
            "UserKnownHostsFile=/home/me/.specboard/hub/known_hosts",
        ] {
            assert!(args.iter().any(|a| a == option), "{option}: {said}");
        }
        // A place with a space in it, as ssh reads one path.
        let spaced = ssh(Path::new("/Users/Me Too/.specboard"), "hub");
        assert!(
            spaced
                .get_args()
                .any(|a| a == "UserKnownHostsFile=\"/Users/Me Too/.specboard/hub/known_hosts\""),
            "{spaced:?}"
        );
    }
}
