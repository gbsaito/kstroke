// wshowkeys-rs — Wayland key overlay, bottom-right by default, with padding.
// Renders with cairo toy text API (no pango). Raw wayland-client.

use std::os::fd::AsRawFd;
use std::time::{Duration, Instant};

use cairo::{Context, FontSlant, FontWeight, ImageSurface};
use wayland_client::protocol::{
    wl_buffer, wl_compositor, wl_output, wl_registry, wl_seat, wl_shm, wl_shm_pool, wl_surface,
};
use wayland_client::{Connection, Dispatch, QueueHandle, WEnum};
use wayland_protocols_wlr::layer_shell::v1::client::{
    zwlr_layer_shell_v1::{self, Layer},
    zwlr_layer_surface_v1::{self, Anchor, KeyboardInteractivity},
};
use xkbcommon::xkb;

// ---- config ----
const PANE_W: i32 = 700;
const PANE_H: i32 = 110;
const KEY_PAD: f64 = 14.0;
const KEY_GAP: f64 = 12.0;
const EXPIRY: Duration = Duration::new(2, 500000000);
const FONT_FAMILY: &str = "JetBrains Mono";
const FONT_SIZE: f64 = 34.0;
const BG: (f64, f64, f64, f64) = (0.08, 0.08, 0.08, 0.55);
const FG: (f64, f64, f64, f64) = (1.0, 1.0, 1.0, 1.0);
const SPECIAL_FG: (f64, f64, f64, f64) = (0.85, 0.45, 0.35, 1.0);

struct Key {
    text: String,
    special: bool,
    count: u32,
    hold: bool,
    t: Instant,
}

struct App {
    // globals
    compositor: Option<wl_compositor::WlCompositor>,
    seat: Option<wl_seat::WlSeat>,
    shm: Option<wl_shm::WlShm>,
    shell: Option<zwlr_layer_shell_v1::ZwlrLayerShellV1>,
    // surfaces
    surface: Option<wl_surface::WlSurface>,
    layer: Option<zwlr_layer_surface_v1::ZwlrLayerSurfaceV1>,
    // buffer
    buffer: Option<wl_buffer::WlBuffer>,
    memfd: Option<std::os::fd::RawFd>,
    buf_data: Option<&'static mut [u8]>, // mmap'd; shared_mut hack below
    // render state
    width: i32,
    height: i32,
    need_ack: Option<u32>,
    buf_busy: bool,
    // state
    keys: Vec<Key>,
    running: bool,
    dirty: bool,
}

fn label_for(sym: xkb::Keysym) -> (String, bool) {
    // (text, special) — boilerplate marker
    let utf8 = xkb::keysym_to_utf8(sym);
    let c = utf8.chars().next().unwrap_or('\0');
    if utf8.chars().count() == 1 && !c.is_control() && !c.is_whitespace() && c != '\0' {
        return (utf8, false);
    }
    if c == ' ' {
        return ("Space".into(), true);
    }
    let name = xkb::keysym_get_name(sym);
    let special = match name.trim_start_matches("XKB_KEY_") {
        "Escape" => "Esc",
        "Return" | "KP_Enter" => "Return",
        "Shift_L" | "Shift_R" => "Shift",
        "Control_L" | "Control_R" => "Ctrl",
        "Alt_L" | "Alt_R" => "Alt",
        "Meta_L" | "Meta_R" => "Meta",
        "Super_L" | "Super_R" => "⌘",
        "BackSpace" => "Backspace",
        "Tab" | "ISO_Left_Tab" => "Tab",
        "Caps_Lock" => "CapsLock",
        "Delete" => "Del",
        "Insert" => "Ins",
        "Home" => "↖",
        "End" => "↘",
        "Prior" | "Page_Up" => "PageUp",
        "Next" | "Page_Down" => "PageDn",
        "Up" => "↑",
        "Down" => "↓",
        "Left" => "←",
        "Right" => "→",
        "space" => "Space",
        "Print" => "PrtSc",
        "Execute" | "KP_Begin" => "·",
        "F1" => "F1",
        _ => "",
    };
    if !special.is_empty() {
        return (special.into(), true);
    }
    if let Some(rest) = name.strip_prefix("KP_") {
        return (rest.to_string(), true);
    }
    if let Some(rest) = name.strip_prefix("F") {
        if rest.chars().all(|c| c.is_ascii_digit()) && !rest.is_empty() {
            return (name.clone(), true);
        }
    }
    // fallback: printable-ish name
    if name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') && !name.is_empty() {
        return (name, true);
    }
    (format!("[{:x}]", u32::from(sym)), true)
}

impl App {
    fn on_label(&mut self, text: String, special: bool, val: u32) {
        if text == "\u{7}" {
            self.keys.clear();
            self.dirty = true;
            return;
        }
        if text.is_empty() {
            return;
        }
        // val: 0 = display refresh (released), 1 = press, 2 = key repeat (held)
        if val == 0 {
            if let Some(i) = self.keys.iter().position(|k| k.text == text) {
                let mut k = self.keys.remove(i);
                k.hold = false;
                k.t = Instant::now();
                self.keys.push(k);
                self.dirty = true;
            }
            return;
        }
        let prev = self.keys.last().map(|l| (l.text.clone(), l.count));
        let hold = val == 2;
        if let Some(i) = self.keys.iter().position(|k| k.text == text) {
            let mut k = self.keys.remove(i);
            // repeat counts only when consecutive (same key pressed/held repeatedly)
            k.count = match prev {
                Some((t, c)) if t == text => c + 1,
                _ => 1,
            };
            k.hold = hold;
            k.t = Instant::now();
            self.keys.push(k); // most-recently-updated last: shown box follows focus
        } else {
            self.keys.push(Key {
                text,
                special,
                count: 1,
                hold,
                t: Instant::now(),
            });
            while self.keys.len() > 10 {
                self.keys.remove(0);
            }
        }
        self.dirty = true;
    }

    fn prune(&mut self) {
        let before = self.keys.len();
        self.keys.retain(|k| k.t.elapsed() < EXPIRY);
        if self.keys.len() != before {
            self.dirty = true;
        }
    }

    fn draw(&mut self, qh: &QueueHandle<Self>) {
        let (w, h) = (self.width.max(1), self.height.max(1));
        if self.buffer.is_none() || self.memfd.is_none() {
            self.make_buffer(w, h, qh);
        }
        if self.buffer.is_none() || self.memfd.is_none() {
            return;
        }
        let stride = (w as usize * 4 + 3) & !3;

        let surface = unsafe {
            ImageSurface::create_for_data_unsafe(
                self.buf_data.as_mut().unwrap().as_mut_ptr() as *mut u8,
                cairo::Format::ARgb32,
                w,
                h,
                stride as i32,
            )
        }
        .unwrap();
        let ctx = Context::new(&surface).unwrap();

        // clear before painting (buffer is reused between frames)
        ctx.set_operator(cairo::Operator::Clear);
        ctx.paint().unwrap();
        ctx.set_operator(cairo::Operator::Over);

        // no background fill: only key boxes are drawn

        // font
        ctx.select_font_face(FONT_FAMILY, FontSlant::Normal, FontWeight::Bold);
        ctx.set_font_size(FONT_SIZE);
        ctx.set_antialias(cairo::Antialias::Subpixel);

        // one key box at a time, right-aligned bottom
        if let Some(key) = self.keys.last() {
            let mut text = key.text.clone();
            if key.hold {
                text = format!("{} (Hold)", text);
            }
            if key.count > 1 {
                text.push_str(&format!(" ×{}", key.count));
            }
            let ext = ctx.text_extents(&text).unwrap();
            let bw = ext.width() + KEY_PAD * 2.0;
            let x = (w as f64) - KEY_PAD - bw;
            let y = h as f64 / 2.0;
            ctx.set_source_rgba(BG.0 + 0.06, BG.1 + 0.06, BG.2 + 0.06, BG.3);
            ctx.rectangle(
                x,
                y - (ext.height() / 2.0 + KEY_PAD),
                bw,
                ext.height() + KEY_PAD * 2.0,
            );
            ctx.fill().unwrap();
            let col = if key.special { SPECIAL_FG } else { FG };
            ctx.set_source_rgba(col.0, col.1, col.2, col.3);
            let wb = ext.x_bearing();
            let hb = ext.y_bearing() + ext.height() / 2.0;
            ctx.move_to(x + KEY_PAD - wb, y - hb);
            ctx.show_text(&text).unwrap();
        }

        surface.flush();
        if std::env::var_os("WSK_DUMP").is_some() {
            let data = self.buf_data.as_ref().unwrap();
            let ink = (0..h as usize).map(|y| (0..w as usize).filter(|x| data[y*stride + x*4+3] > 30).count()).sum::<usize>();
            let solid = (0..h as usize).map(|y| (0..w as usize).filter(|x| data[y*stride + x*4+3] >= 250).count()).sum::<usize>();
                        if let Some(k) = self.keys.last() {
                let ext = ctx.text_extents(&k.text).unwrap();
                eprintln!("draw: {}x{} ink={} solid={} last_key={:?} ext={}x{} @{}", w, h, ink, solid, k.text, ext.width(), ext.height(), ext.x_bearing());
            } else {
                eprintln!("draw: {}x{} ink={} solid={} (no keys)", w, h, ink, solid);
            }
        }

        if self.buf_busy {
            // previous frame not released yet: retry next loop tick
            self.dirty = true;
            return;
        }
        let buffer = self.buffer.as_ref().unwrap();
        let surf = self.surface.as_ref().unwrap();
        surf.attach(Some(buffer), 0, 0);
        surf.damage(0, 0, w, h);
        surf.commit();
        self.buf_busy = true;
        if let Ok(f) = std::env::var("WSK_NORELEASE") {
            let _ = f;
            self.buf_busy = false; // compositor doesn't send Release
        }

        // schedule next frame so expiry redraws even without key events
        // wl_frame event via Dispatch<WlCallback>? Not wired; skip if unknown
    }

    fn make_buffer(&mut self, w: i32, h: i32, qh: &QueueHandle<Self>) {
        let stride = (w as usize * 4 + 3) & !3;
        let size = stride * h as usize;
        let fd = unsafe { libc::memfd_create(c"wsk-rs".as_ptr(), 0) };
        if fd < 0 {
            eprintln!("memfd failed");
            return;
        }
        unsafe { libc::ftruncate(fd, size as libc::off_t) };
        let ptr = unsafe {
            libc::mmap(
                std::ptr::null_mut(),
                size,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_SHARED,
                fd,
                0,
            )
        };
        if ptr as isize == -1 {
            eprintln!("mmap failed");
            unsafe { libc::close(fd) };
            return;
        }
        self.buf_data = unsafe { Some(std::slice::from_raw_parts_mut(ptr as *mut u8, size)) };

        let shm = self.shm.as_ref().unwrap();
        let pool = shm.create_pool(
            unsafe { std::os::fd::BorrowedFd::borrow_raw(fd) },
            size as i32,
            qh,
            (),
        );
        let buffer = pool.create_buffer(0, w, h, stride as i32, wl_shm::Format::Argb8888, qh, ());
        self.buffer = Some(buffer);
        self.memfd = Some(fd);
    }
}

fn main() {
    for arg in std::env::args().skip(1) {
        eprintln!(
            "warning: arg '{}' ignored (bottom/right is fixed default)",
            arg
        );
    }
    let conn = match Connection::connect_to_env() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("connect failed: {e}");
            std::process::exit(1);
        }
    };
    let mut queue = conn.new_event_queue();
    let qh = queue.handle();
    let display = conn.display();
    display.get_registry(&qh, ());

    let mut app = App {
        compositor: None,
        seat: None,
        shm: None,
        shell: None,
        surface: None,
        layer: None,
        buffer: None,
        memfd: None,
        buf_data: None,
        width: PANE_W,
        height: PANE_H,
        need_ack: None,
        buf_busy: false,
        keys: Vec::new(),
        running: true,
        dirty: false,
    };

    // initial registry roundtrip to get globals
    queue.roundtrip(&mut app).unwrap();
    if app.compositor.is_none() || app.shm.is_none() || app.shell.is_none() {
        eprintln!("missing required globals (compositor/shm/layer_shell)");
        std::process::exit(1);
    }

    let surface = app.compositor.as_ref().unwrap().create_surface(&qh, ());
    let layer = app.shell.as_ref().unwrap().get_layer_surface(
        &surface,
        None, // overlay on every output
        Layer::Overlay,
        "wshowkeys-rs".to_string(),
        &qh,
        (),
    );
    layer.set_anchor(Anchor::Bottom | Anchor::Right);
    layer.set_exclusive_zone(-1); // no pad from panel; margins handle padding
    layer.set_margin(140, 40, 40, 40); // padding: bottom right
    layer.set_keyboard_interactivity(KeyboardInteractivity::None);
    layer.set_size(PANE_W as u32, PANE_H as u32);
    surface.commit();

    app.surface = Some(surface);
    app.layer = Some(layer);

    queue.roundtrip(&mut app).unwrap(); // pick up initial configure

    // input thread: evdev direct read (like C fork libinput path)
    let (tx, rx) = std::sync::mpsc::channel::<(String, bool, u32)>();
    std::thread::spawn(move || input_loop(tx));

    // event loop with poll-timeout so expiry works
    let fd = conn.backend().poll_fd().as_raw_fd();
    while app.running {
        let mut fds = [libc::pollfd {
            fd,
            events: libc::POLLIN,
            revents: 0,
        }];
        let n = unsafe { libc::poll(fds.as_mut_ptr(), 1, 20) };
        if n > 0 && (fds[0].revents & libc::POLLIN) != 0 {
            let did = conn.prepare_read().map(|g| g.read().is_ok()).unwrap_or(false);
            if did {
                queue.dispatch_pending(&mut app).unwrap();
            }
        }
        while let Ok((text, special, val)) = rx.try_recv() {
            app.on_label(text, special, val);
        }
        app.prune();
        if let Some(serial) = app.need_ack.take() {
            app.layer.as_ref().unwrap().ack_configure(serial);
            app.surface.as_ref().unwrap().commit();
        }
        if app.dirty {
            app.dirty = false;
            app.draw(&qh);        }
        conn.flush().unwrap();
    }
}

// ---- dispatch impls ----

impl Dispatch<wl_registry::WlRegistry, ()> for App {
    fn event(
        app: &mut Self,
        registry: &wl_registry::WlRegistry,
        event: wl_registry::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        if let wl_registry::Event::Global {
            name,
            interface,
            version,
        } = event
        {
            match &*interface {
                "wl_compositor" => {
                    app.compositor = Some(registry.bind(name, version.min(4), qh, ()))
                }
                "wl_shm" => app.shm = Some(registry.bind(name, version.min(1), qh, ())),
                "wl_seat" => app.seat = Some(registry.bind(name, version.min(1), qh, ())),
                "wl_output" => {
                    let _out: wl_output::WlOutput = registry.bind(name, version.min(1), qh, ());
                }
                "zwlr_layer_shell_v1" => {
                    app.shell = Some(registry.bind(name, version.min(1), qh, ()))
                }
                _ => {}
            }
        }
    }
}

impl Dispatch<wl_compositor::WlCompositor, ()> for App {
    fn event(
        _: &mut Self,
        _: &wl_compositor::WlCompositor,
        _: wl_compositor::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<wl_shm::WlShm, ()> for App {
    fn event(
        _: &mut Self,
        _: &wl_shm::WlShm,
        _: wl_shm::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<wl_shm_pool::WlShmPool, ()> for App {
    fn event(
        _: &mut Self,
        _: &wl_shm_pool::WlShmPool,
        _: wl_shm_pool::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<wl_buffer::WlBuffer, ()> for App {
    fn event(
        app: &mut Self,
        _: &wl_buffer::WlBuffer,
        event: wl_buffer::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wl_buffer::Event::Release {} = event {
            app.buf_busy = false;
        }
    }
}

impl Dispatch<wl_output::WlOutput, ()> for App {
    fn event(
        _: &mut Self,
        _: &wl_output::WlOutput,
        _: wl_output::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<wl_seat::WlSeat, ()> for App {
    fn event(
        _: &mut Self,
        seat: &wl_seat::WlSeat,
        event: wl_seat::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        if let wl_seat::Event::Capabilities {
            capabilities: WEnum::Value(caps),
        } = event
        {
            if std::env::var_os("WSK_DUMP").is_some() {
                eprintln!("seat caps: {:?}", caps);
            }
            if (caps & wl_seat::Capability::Keyboard) != wl_seat::Capability::empty() {
                // NOP: keys come from evdev directly (like the C fork), not wl_keyboard.
            }
        }
    }
}


impl Dispatch<zwlr_layer_shell_v1::ZwlrLayerShellV1, ()> for App {
    fn event(
        _: &mut Self,
        _: &zwlr_layer_shell_v1::ZwlrLayerShellV1,
        _: zwlr_layer_shell_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<zwlr_layer_surface_v1::ZwlrLayerSurfaceV1, ()> for App {
    fn event(
        app: &mut Self,
        _: &zwlr_layer_surface_v1::ZwlrLayerSurfaceV1,
        event: zwlr_layer_surface_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let zwlr_layer_surface_v1::Event::Configure {
            serial,
            width,
            height,
        } = event
        {
            if width > 0 {
                app.width = width.max(1) as i32;
            }
            if height > 0 {
                app.height = height.max(1) as i32;
            }
            if app.width != PANE_W || app.height != PANE_H {
                // size changed, rebuild buffer
                app.buffer = None;
            }
            app.need_ack = Some(serial);
            app.dirty = true;
        }
        if let zwlr_layer_surface_v1::Event::Closed {} = event {
            app.running = false;
        }
    }
}

impl Dispatch<wl_surface::WlSurface, ()> for App {
    fn event(
        _: &mut Self,
        _: &wl_surface::WlSurface,
        _: wl_surface::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}



fn detect_xkb() -> (String, String, String) {
    let mut rules = "evdev".to_string();
    let mut layout = std::env::var("XKB_DEFAULT_LAYOUT").unwrap_or_else(|_| "br".into());
    let mut variant = std::env::var("XKB_DEFAULT_VARIANT").unwrap_or_default();
    if let Ok(toml) = std::fs::read_to_string("/home/saito/.config/mango/config.toml") {
        for line in toml.lines() {
            let t = line.trim();
            if let Some(v) = t.strip_prefix("xkb_rules_layout") {
                let v = v.trim().trim_start_matches('=').trim().trim_matches('"');
                if !v.is_empty() { layout = v.to_string(); }
            }
            if let Some(v) = t.strip_prefix("xkb_rules_variant") {
                let v = v.trim().trim_start_matches('=').trim().trim_matches('"');
                if !v.is_empty() { variant = v.to_string(); }
            }
        }
    }
    if let Ok(l) = std::env::var("XKB_DEFAULT_LAYOUT") { if !l.is_empty() { layout = l; } }
    (rules, layout, variant)
}

fn input_loop(tx: std::sync::mpsc::Sender<(String, bool, u32)>) {
    use evdev::{Device, EventSummary};
    let mut handles = Vec::new();
    // one -event-kbd node per physical keyboard; event* enumeration double-counts
    //hid devices exposing if01 clones
    let mut paths: Vec<std::path::PathBuf> = Vec::new();
    if let Ok(dir) = std::fs::read_dir("/dev/input/by-id") {
        for e in dir {
            if let Ok(e) = e {
                if let Ok(name) = e.file_name().into_string() {
                    if name.ends_with("-event-kbd") && !name.contains("-if01-") {
                        paths.push(e.path());
                    }
                }
            }
        }
    }
    if paths.is_empty() {
        // fallback: any event node carrying keys (unchanged original behavior)
        if let Ok(dir) = std::fs::read_dir("/dev/input") {
            for e in dir {
                let path = e.ok().map(|e| e.path());
                let Some(path) = path else { continue };
                let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                if !name.starts_with("event") {
                    continue;
                }
                if let Ok(dev) = Device::open(&path) {
                    if dev.supported_keys().map(|k| k.iter().nth(0).is_some()).unwrap_or(false) {
                        paths.push(path);
                    }
                }
            }
        }
    }
    for p in paths {
        let Ok(mut dev) = Device::open(&p) else { continue };
        if !dev.supported_keys().map(|k| k.iter().nth(0).is_some()).unwrap_or(false) {
            continue;
        }
        let tx = tx.clone();
        let p = p.clone();
        eprintln!("listening on {:?}", p);
        handles.push(std::thread::spawn(move || {
            let mut mods: Vec<String> = Vec::new();
            let ctx = xkb::Context::new(xkb::CONTEXT_NO_FLAGS);
            let (rules, layout, variant) = detect_xkb();
            let km = xkb::Keymap::new_from_names(
                &ctx, &rules, &"pc105".to_string(), &layout, &variant, None, xkb::KEYMAP_COMPILE_NO_FLAGS)
                .expect("keymap");
            let mut state = xkb::State::new(&km);
            let mut last_base: Option<(String, bool)> = None;
            loop {
                for ev in match dev.fetch_events() {
                    Ok(it) => it,
                    Err(_) => break,
                } {
                    let (code, val) = match ev.destructure() {
                        EventSummary::Key(_, k, v) => (u32::from(k.code()), v),
                        _ => continue,
                    };
                    let pressed = val == 1;
                    let repeating = val == 2;
                    let kc = xkb::Keycode::new(code + 8);
                    state.update_key(kc, if pressed { xkb::KeyDirection::Down } else { xkb::KeyDirection::Up });
                    if !pressed {
                        if let Some(&r) = state.key_get_syms(kc).first() {
                            let rn = xkb::keysym_get_name(r);
                            eprintln!("DBG up code={} last_base={:?}", code, last_base.is_some());
                            let rm = match rn.trim_start_matches("XKB_KEY_").to_lowercase().as_str() {
                                "shift_l" | "shift_r" => Some("Shift"),
                                "control_l" | "control_r" => Some("Ctrl"),
                                "alt_l" | "alt_r" => Some("Alt"),
                                _ => None,
                            };
                            let is_mod = rm.is_some();
                            if is_mod {
                                let n = mods.len();
                                mods.retain(|x| x != &rm.unwrap().to_string());
                                let n2 = mods.len();
                                if let Some((t, sp)) = &last_base {
                                    let _ = tx.send((t.clone(), *sp, 0u32));
                                } else if !mods.is_empty() {
                                    let _ = tx.send((mods.join("+"), true, 0u32));
                                } else if n != n2 {
                                    let _ = tx.send(("\u{7}".to_string(), false, 0u32));
                                }
                            } else if let Some((t, sp)) = &last_base {
                                // base key released: display persists (no extra count)
                                let _ = tx.send((t.clone(), *sp, 0u32));
                            }
                        }
                        continue;
                    }
                    if repeating {
                        eprintln!("DBG repeat code={}", code);
                        if let Some((t, sp)) = &last_base {
                            let _ = tx.send((t.clone(), *sp, 2u32));
                        }
                        continue;
                    }
                    let Some(&sym) = state.key_get_syms(kc).first() else {
                        continue;
                    };
                    let name = xkb::keysym_get_name(sym);
                    // SAFETY: never process power/sleep/pause-class keys
                    if [
                        "Pause", "XF86PowerOff", "XF86Sleep", "XF86PowerKey",
                        "XF86Standby", "XF86Hibernate", "XF86_Suspend",
                    ]
                    .contains(&name.as_str())
                    {
                        continue;
                    }
                    let (mut text, special) = label_for(sym);
                    if text.is_empty() {
                        continue;
                    }
                    let nm = xkb::keysym_get_name(sym);
                    let mname = match nm.trim_start_matches("XKB_KEY_").to_lowercase().as_str() {
                        "shift_l" | "shift_r" => Some("Shift".to_string()),
                        "control_l" | "control_r" => Some("Ctrl".to_string()),
                        "alt_l" | "alt_r" => Some("Alt".to_string()),
                        _ => None,
                    };
                    if let Some(m) = mname {
                        if !mods.contains(&m) { mods.push(m); }
                        // standalone/holding display: show current mod stack
                        eprintln!("DBG down stack={:?}", mods);
                        last_base = None;
                        let _ = tx.send((mods.join("+"), true, 1u32));
                        continue;
                    }
                    last_base = Some((text.clone(), special));
                    eprintln!("DBG down code={} text={}", code, text);
                    if !mods.is_empty() {
                        text = format!("{}+{}", mods.join("+"), text);
                    }
                    if tx.send((text, special, 1u32)).is_err() {
                        return;
                    }
                }
            }
        }));
    }

    for h in handles {
        let _ = h.join();
    }
}
