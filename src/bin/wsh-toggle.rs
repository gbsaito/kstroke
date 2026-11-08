// wsh-toggle — single-shot Insert toggle for wshowkeys-rs.
// Mango maps Insert → this; process exits after toggling once.

use std::process::Command;

fn toggle() {
    let running = Command::new("pgrep").arg("-x").arg("wshowkeys-rs").output().map(|o| o.status.success()).unwrap_or(false);
    if running {
        let _ = Command::new("pkill").arg("-9").arg("wshowkeys-rs").status();
    } else {
        let prog = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/release/wshowkeys-rs");
        let _ = Command::new(prog).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).spawn();
    }
}

fn main() {
    const INSERT: u32 = 110;
    use evdev::{Device, EventSummary};
    let mut handles = Vec::new();
    let (tx, rx) = std::sync::mpsc::channel::<()>();
    if let Ok(dir) = std::fs::read_dir("/dev/input") {
        for entry in dir {
            let path = entry.ok().map(|e| e.path());
            let Some(path) = path else { continue };
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("").to_string();
            if !name.starts_with("event") {
                continue;
            }
            let Ok(mut dev) = Device::open(&path) else { continue };
            if !dev.supported_keys().map(|k| k.iter().nth(0).is_some()).unwrap_or(false) {
                continue;
            }
            let tx = tx.clone();
            handles.push(std::thread::spawn(move || {
                loop {
                    for ev in match dev.fetch_events() {
                        Ok(it) => it,
                        Err(_) => break,
                    } {
                        if let EventSummary::Key(_, k, v) = ev.destructure() {
                            if u32::from(k.code()) == INSERT && v == 1 && tx.send(()).is_err() {
                                return;
                            }
                        }
                    }
                }
            }));
        }
    }
    // first press wins
    let _ = rx.recv();
    toggle();
    std::process::exit(0);
}
