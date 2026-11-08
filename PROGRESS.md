# PROGRESS — wshowkeys-rs

- Done: working overlay — keys shown rendered correctly (verified by user).
- Commits: ae9fed4 initial, a1f34eb fixes.
- Current: buffer clear fix and evdev input switched in, user confirmed.
- Next: remove focus-mode handling (KeyboardInteractivity::None); clean dead flags; avoid re-draw spew; possible tweaks (per user feedback).

## 2026-11-08 session (latest)

- Single key box, bottom/right, translucent black, rounded, alpha 0.75, single box.
- Auto-detect BR layout; env override; Insert -> wsh-toggle.
- Mods: Ctrl/Shift/Alt/Win/AltGr shown standalone while held, stacks like "Ctrl+G", persist 2.5s after release (except mods release -> clear).
- Shift+key: bare glyph when shifted glyph differs from base ("Shift+1" -> "!"); Shift+Space keeps prefix (glyph unchanged).
- AltGr (ISO_Level3_Shift) tracked as AltGr mod.
- Release/press protocol: (text, special, val) — val 0 refresh, 1 press, 2 repeat/hold; hold shows "(Hold)", count frozen.
- Mod release clears hold (val0 refresh) — fixed stale (Hold).
- Mod releases with remaining held mods -> refresh remaining stack.
- Base release while mods held -> show mod stack again (shift+key release -> Shift).
- Autorepeat (val==2) no longer treated as release; repeats -> (Hold) + frozen count.
- Single keyboard node via by-id (event-kbd, if01 excluded -> double count); mouse nodes (-event-mouse) opened.
- Mouse labels: LMB / Middle / RMB (val==2 repeats skipped).
- Labels: PageUp/PageDn, Home/End text, Period ".", Pause text (blocklist only for power keys), Fn-generated (XF86*) labeled.
- Fix this session: release refresh no longer re-promotes older keys (hold artifact); mod stack return after base release.
