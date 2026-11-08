# PROGRESS — wshowkeys-rs

- Done: working overlay — keys shown rendered correctly (verified by user).
- Current: all reported issues fixed. Latest commit: 79dbf78.
- Shift behavior final: Shift+letter -> "Shift+A"; Shift+digit -> bare "!" ; Shift+punct -> bare shifted glyph ("." -> ">", "," -> "<"); base keys plain.

## Fixed this session (2026-11-08)
- Autorepeat latch: val==2 repeats no longer feed xkb state.update_key — libxkbcommon counts internal downs, repeats used to latch Shift after release (all keys read Shifted). commit 79dbf78.
- Shift+punct: removed leftover reassignment that sent base char; shifted glyph now kept (>. <). commit 334c00e.
- Comma/period/less/greater lowercase keysym names mapped ("period", "comma", "less", "greater", "Comma", "Period", "Full_Stop", "Greater", "gt").
- Label fixes: comma ",", period ".", Backspace, Ins, Win labels, LMB/Middle/RMB, KP_ variants for PageUp/pageDn.
- Added debug logging (removed), keylog file scratch (removed).

## Older
- Single key box, bottom/right, translucent black, rounded, alpha 0.75.
- Auto-detect BR layout; env override; Insert -> wsh-toggle (single-shot).
- Mods persistent stack + AltGr; release/refresh protocol val 0/1/2; (Hold) marker; if01 device excluded;防腐mouse.


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
