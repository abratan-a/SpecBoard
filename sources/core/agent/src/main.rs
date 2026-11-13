//! The agent: the projects of the user it runs as and the core over them, on
//! the host where the AI sessions run. The app puts it there over SSH and reaches it through
//! `attach`; the AI sessions there reach its MCP at their own loopback (`mcp.rs`).
//!
//! It answers MCP for its own projects only. It has no relay that hands the requests of a host
//! to the app, for the projects of a Mac (`--stdio`): the projects an AI should read are moved
//! to an agent.

// What a read lets go of is given back to the system (agent/Cargo.toml says why).
#[cfg(target_os = "linux")]
#[global_allocator]
static ALLOCATOR: mimalloc::MiMalloc = mimalloc::MiMalloc;

mod attach;
mod daemon;
mod http;
mod hub;
mod mcp;
mod screens;

fn usage() -> String {
    specboard_core::text!("agent.cli.usage")
}

fn main() {
    // What the agent says without a call to answer (its command line, its log) is in its machine's
    // language, else in the one most can read: never the tables' own default, which is the window's.
    let machine = specboard_core::text::system_from_env(|name| std::env::var(name).ok());
    specboard_core::text::set_default(machine.unwrap_or(specboard_core::text::FALLBACK));
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    // `--lang <code>`: who started it reads what it says in that language (the app, its screen's,
    // whatever the host's language is).
    if let Some(at) = args.iter().position(|a| a == "--lang") {
        if let Some(lang) = args.get(at + 1) {
            specboard_core::text::set_default(lang);
        }
        args.drain(at..(at + 2).min(args.len()));
    }
    if args.iter().any(|a| a == "--version") {
        println!("{}", env!("CARGO_PKG_VERSION"));
        return;
    }
    // The agent's own and the way to it.
    let code = match args.first().map(String::as_str) {
        Some("serve") => daemon::serve(),
        Some("attach") => attach::attach(),
        Some("start") => attach::start(),
        Some("stop") => attach::stop(),
        Some("status") => attach::status(),
        Some("port") => attach::port(args.get(1).map(String::as_str)),
        Some("hub") => hub::role(&args[1..]),
        _ => {
            eprintln!("{}", usage());
            2
        }
    };
    std::process::exit(code);
}

#[cfg(test)]
mod tests {
    #[cfg(target_os = "linux")]
    unsafe extern "C" {
        /// mimalloc's own: whether `p` is inside the regions it allocates from.
        fn mi_is_in_heap_region(p: *const std::ffi::c_void) -> bool;
    }

    /// Only the Linux agent has its own allocator (the macOS one keeps the system's): elsewhere
    /// there is nothing to hold.
    #[test]
    fn on_linux_the_agent_allocates_from_mimalloc() {
        #[cfg(target_os = "linux")]
        {
            let block = vec![0u8; 4096];
            // SAFETY: only asks whether the address is inside mimalloc's own regions; nothing is
            // read.
            let inside = unsafe { mi_is_in_heap_region(block.as_ptr().cast()) };
            assert!(inside);
        }
    }
}
