# PLAN — wshowkeys-rs

- Goal: local unprivileged Wayland key overlay (Rust rewrite of wshowkeys).
- Anchor bottom/right, padding 40, single key box, 2.5s expiry, ×N repeats.
- BR auto-detect, env override, Insert -> wsh-toggle (single-shot).
- Mods: Ctrl/Shift/Alt/Win/AltGr standalone + stack; Shift+key bare glyph when glyph changes; Shift+Space prefix kept.
- Mouse: LMB / Middle / RMB clicks display.
- Background: translucent black (alpha 0.75), rounded, per key box.

## Completed
- Overlay logic, drawing, commit, expiry, repeats, mods, safety blocklist.
- Buffer busy retry, single-kbd-node selection, draw-follows-last-key, consecutive-only repeats, refresh protocol val 0/1/2.
- (Hold) for key autorepeats and held mods; mod release clears hold; standalone mods persist till expiry.

## Next
- User live-test final behavior (punctuation, modifiers, repeats, holds, mouse, fn).
- Export /home/saito/.local/bin/wshowkeys -> target/release/wshowkeys-rs binary.
- Ask user on final branding/padding.
