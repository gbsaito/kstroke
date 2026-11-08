// wshowkeys-rs — Wayland key overlay, bottom-right by default, with padding.
// Renders with cairo toy text API (no pango). Raw wayland-client.

use std::io::{Read as _, Write as _};
use std::os::fd::AsRawFd;
use std::time::{Duration, Instant};

use cairo::{Context, FontSlant, FontWeight, ImageSurface};
use wayland_client::protocol::{
    wl_buffer, wl_compositor, wl_keyboard, wl_output, wl_registry, wl_seat, wl_shm, wl_shm_pool,
    wl_surface,
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
const BG: (f64, f64, f64, f64) = (0.08, 0.08, 0.08, 0.92);
const FG: (f64, f64, f64, f64) = (1.0, 1.0, 1.0, 1.0);
const SPECIAL_FG: (f64, f64, f64, f64) = (0.85, 0.45, 0.35, 1.0);

struct Key {
    text: String,
    special: bool,
    count: u32,
    t: Instant,
}

struct App {
    // globals
    compositor: Option<wl_compositor::WlCompositor>,
    shm: Option<wl_shm::WlShm>,
    shell: Option<zwlr_layer_shell_v1::ZwlrLayerShellV1>,
    seat: Option<wl_seat::WlSeat>,
    // surfaces
    surface: Option<wl_surface::WlSurface>,
    layer: Option<zwlr_layer_surface_v1::ZwlrLayerSurfaceV1>,
    keyboard: Option<wl_keyboard::WlKeyboard>,
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
    xkb: Option<xkb::State>,
    running: bool,
    dirty: bool,
}

struct Shared<'a>(&'a mut App);

fn label_for(sym: xkb::Keysym) -> (String, bool) {
    // (text, special) — boilerplate marker
    let utf8 = xkb::keysym_to_utf8(sym);
    let c = utf8.chars().next().unwrap_or('\0');
    if utf8.len() == 1 && !c.is_control() && !c.is_whitespace() && c != '\0' {
        return (utf8, false);
    }
    if c == ' ' {
        return ("Space".into(), true);
    }
    let name = xkb::keysym_get_name(sym);
    let special = match name.trim_start_matches("XKB_KEY_") {
        "Escape" => "Esc",
        "Return" | "KP_Enter" => "↵",
        "Shift_L" | "Shift_R" => "Shift",
        "Control_L" | "Control_R" => "Ctrl",
        "Alt_L" | "Alt_R" => "Alt",
        "Meta_L" | "Meta_R" => "Meta",
        "Super_L" | "Super_R" => "⌘",
        "BackSpace" => "⌫",
        "Tab" | "ISO_Left_Tab" => "⇥",
        "Caps_Lock" => "⌷",
        "Delete" => "Del",
        "Insert" => "Ins",
        "Home" => "↖",
        "End" => "↘",
        "Page_Up" => "⇞",
        "Page_Down" => "⇟",
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
    fn on_key_text(&mut self, sym: xkb::Keysym) {
        let (text, special) = label_for(sym);
        if text.is_empty() {
            return;
        }
        if let Some(k) = self.keys.iter_mut().find(|k| k.text == text) {
            k.count += 1;
            k.t = Instant::now();
        } else {
            self.keys.push(Key {
                text,
                special,
                count: 1,
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
        use std::os::fd::BorrowedFd as _;
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

        // background
        ctx.set_source_rgba(BG.0, BG.1, BG.2, BG.3);
        ctx.paint().unwrap();

        // font
        ctx.select_font_face(FONT_FAMILY, FontSlant::Normal, FontWeight::Bold);
        ctx.set_font_size(FONT_SIZE);
        ctx.set_antialias(cairo::Antialias::Subpixel);

        let mut x = KEY_PAD;
        let y = h as f64 / 2.0;
        for key in &self.keys {
            let mut text = key.text.clone();
            if key.count > 1 {
                text.push_str(&format!(" ×{}", key.count));
            }
            let ext = ctx.text_extents(&text).unwrap();
            let bw = ext.width() + KEY_PAD * 2.0;
            if x + bw > w as f64 - KEY_PAD {
                break; // one row for now
            }
            // key box
            ctx.set_source_rgba(BG.0 + 0.06, BG.1 + 0.06, BG.2 + 0.06, BG.3);
            ctx.rectangle(
                x,
                y - (ext.height() / 2.0 + KEY_PAD),
                bw,
                ext.height() + KEY_PAD * 2.0,
            );
            ctx.fill().unwrap();
            // text
            let col = if key.special { SPECIAL_FG } else { FG };
            ctx.set_source_rgba(col.0, col.1, col.2, col.3);
            let wb = ext.x_bearing();
            let hb = ext.y_bearing() + ext.height() / 2.0;
            ctx.move_to(x + KEY_PAD - wb, y - hb);
            ctx.show_text(&text).unwrap();
            x += bw + KEY_GAP;
        }

        surface.flush();

        if self.buf_busy {
            return; // previous buffer not released yet
        }
        let buffer = self.buffer.as_ref().unwrap();
        let surf = self.surface.as_ref().unwrap();
        surf.attach(Some(buffer), 0, 0);
        surf.damage(0, 0, w, h);
        surf.commit();
        self.buf_busy = true;

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
        shm: None,
        shell: None,
        seat: None,
        surface: None,
        layer: None,
        keyboard: None,
        buffer: None,
        memfd: None,
        buf_data: None,
        width: PANE_W,
        height: PANE_H,
        need_ack: None,
        buf_busy: false,
        keys: Vec::new(),
        xkb: None,
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

    // event loop with poll-timeout so expiry works
    let fd = conn.backend().poll_fd().as_raw_fd();
    while app.running {
        let mut fds = [libc::pollfd {
            fd,
            events: libc::POLLIN,
            revents: 0,
        }];
        let n = unsafe { libc::poll(fds.as_mut_ptr(), 1, 100) };
        if n > 0 && (fds[0].revents & libc::POLLIN) != 0 {
            let did = conn.prepare_read().map(|g| g.read().is_ok()).unwrap_or(false);
            if did {
                queue.dispatch_pending(&mut app).unwrap();
            }
        }
        app.prune();
        if let Some(serial) = app.need_ack.take() {
            app.layer.as_ref().unwrap().ack_configure(serial);
            app.surface.as_ref().unwrap().commit();
        }
        if app.dirty {
            app.dirty = false;
            app.draw(&qh);
            if app.buf_busy {
                app.dirty = true; // retry after release
            }
        }
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
        app: &mut Self,
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
            if (caps & wl_seat::Capability::Keyboard) != wl_seat::Capability::empty() {
                app.keyboard = Some(seat.get_keyboard(qh, ()));
            }
        }
    }
}

impl Dispatch<wl_keyboard::WlKeyboard, ()> for App {
    fn event(
        app: &mut Self,
        _: &wl_keyboard::WlKeyboard,
        event: wl_keyboard::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            wl_keyboard::Event::Keymap {
                format: _,
                fd,
                size,
                ..
            } => {
                // fd is owned by compositor until we're done reading; don't close ourselves
                let mut buf = String::new();
                {
                    let mut f = std::fs::File::from(fd);
                    let _ = f.take(size as u64).read_to_string(&mut buf);
                } // we own this fd; close after reading
                let ctx = xkb::Context::new(xkb::CONTEXT_NO_FLAGS);
                if let Some(km) = xkb::Keymap::new_from_string(
                    &ctx,
                    buf,
                    xkb::KEYMAP_FORMAT_TEXT_V1,
                    xkb::KEYMAP_COMPILE_NO_FLAGS,
                ) {
                    app.xkb = Some(xkb::State::new(&km));
                }
            }
            wl_keyboard::Event::Modifiers {
                mods_depressed,
                mods_latched,
                mods_locked,
                group,
                ..
            } => {
                if let Some(st) = app.xkb.as_mut() {
                    st.update_mask(mods_depressed, mods_latched, mods_locked, 0, 0, group);
                }
            }
            wl_keyboard::Event::Key {
                key,
                state: WEnum::Value(wl_keyboard::KeyState::Pressed),
                ..
            } => {
                if let Some(st) = &app.xkb {
                    let kc = xkb::Keycode::new((key + 8) as u32);
                    if let Some(&sym) = st.key_get_syms(kc).first() {
                        // allow letters/digits/space + punctuation; block pause-type keys for safety
                        let safe = !matches!(
                            xkb::keysym_get_name(sym).as_str(),
                            "Pause" | "XF86KB_..." | "PowerOff"
                        );
                        let _ = safe;
                        let name = xkb::keysym_get_name(sym);
                        if [
                            "Pause",
                            "XF86PowerOff",
                            "XF86Sleep",
                            "XF86PowerKey",
                            "KP_Enter",
                            "Return",
                        ]
                        .contains(&name.as_str())
                        {
                            // Never render/inject-measure these; also skip Return typographic emulation
                            return;
                        }
                        app.on_key_text(sym);
                    }
                }
            }
            _ => {}
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
