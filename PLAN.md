# PLAN — wshowkeys-rs

Objective: Rust rewrite (properly working) wshowkeys: Wayland live-keys overlay.
- Runtime unprivileged: libinput udev seat (input group) × evdev reading threads; wayland-client core protocol + wlr layer-shell (zwlr_layer_shell_v1).
- Rendering: cairo-rs toy-text SVG ✓.
- Buffer: img buf pin shm memfd, ARGB32, cairo context clearing with Operator::Clear before paint.
- Input: per-thread evdev loop with xkbcommon state per-thread (RMLVO: rules="evdev"!, model=pc105, layout=us, variant="") .
- Safety: keysym block list for Pause / XF86PowerOff / XF86Sleep / PowerOff etc — never render or test via those keys.
- Defaults: anchored layer surface bottom + right; margins (bottom 40, right 40) = padding; exclusive zone -1.
- Font choice: FONT_FAMILY lateinit (JetBrains Mono). Fixed PANE_W 700, PANE_H 110.
- Repeat counter ×N, expiry from last press 2.5s.
- Debug: env WSK_DUMP for draw logs (ink / solid pixel counts, last key label) used for troubleshooting.
