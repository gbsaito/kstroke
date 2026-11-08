// wsh-toggle — global hotkey bridging:
//   Insert → toggle wshowkeys-rs only. Win+Insert intentionally unbound
//   (reserved for notification history elsewhere).
// Non-invasive: reads evdev (input group), spawns/kills the two binaries.

use std::process::Command;

fn toggle(name: &str, args: &[&str]) {
    let running = Command::new("pgrep").arg("-x").arg(name).output().map(|o| o.status.success()).unwrap_or(false);
    if running {
        let _ = Command::new("pkill").arg("-9").arg(name).status();
        eprintln!("toggle: killed {name}");
    } else {
        let prog = if name.contains("wshowkeys-rs") {
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/release/wshowkeys-rs")
        } else if name == "wshowkeys" {
            std::path::PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".local/bin/wshowkeys")
        } else {
            std::path::PathBuf::from(name)
        };
        match Command::new(prog).args(args).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).spawn() {
            Ok(_) => eprintln!("toggle: started {name}"),
            Err(e) => eprintln!("toggle: spawn {name}: {e}"),
        }
    }
}

fn code_s(ev: &evdev::InputEvent) -> u32 { let (c,_) = match ev.destructure() { evdev::EventSummary::Key(_, k, v) => (u32::from(k.code()), v), _ => (9999, 0) }; c }

fn main() {
    use evdev::{Device, EventSummary};
    use std::sync::atomic::{AtomicBool, Ordering};
    static SUPER: AtomicBool = AtomicBool::new(false);
    const INSERT: u32 = 110;   // KEY_INSERT
    const INSERT2: u32 = 247;  // KP_INSERT (ins-numpad)
    const META_L: u32 = 125;   // KEY_LEFTMETA

    let mut handles = Vec::new();
    for entry in std::fs::read_dir("/dev/input").unwrap_or_else(|e| { eprintln!("readdir: {e}"); unreachable!(); }) {
        let path = entry.unwrap().path();
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("").to_string();
        if !name.starts_with("event") {
            continue;
        }
        let Ok(mut dev) = Device::open(&path) else { continue };
        if !dev.supported_keys().map(|k| k.iter().nth(0).is_some()).unwrap_or(false) {
            continue;
        }
        handles.push(std::thread::spawn(move || {
            loop {
                for ev in match dev.fetch_events() {
                    Ok(it) => it,
                    Err(_) => break,
                } {
                    if std::env::var_os("WSH_DEBUG").is_some() {
                        eprintln!("raw ev: {:?}", ev);
                    }
                    let (code, val) = match ev.destructure() {
                        EventSummary::Key(_, k, v) => (u32::from(k.code()), v),
                        _ => continue,
                    };
                    if code == META_L || code == 126 || code == 97 {
                        SUPER.store(val != 0, Ordering::Relaxed);
                        continue;
                    }
                    if val != 1 {
                        continue;
                    }
                    if (code == INSERT || code == INSERT2) && !SUPER.load(Ordering::Relaxed) {
                        toggle("wshowkeys-rs", &[]);
                    }
                }
            }
        }));
    }
    for h in handles {
        let _ = h.join();
    }
}
