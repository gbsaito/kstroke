# kstroke

Wayland key-press overlay in Rust (wshowkeys successor): shows the last
pressed key(s) with modifiers, hold marker, repeat count — bottom-right,
translucent black rounded box. Unprivileged, reads your keyboard via
`/dev/input/evdev` directly, renders with raw wayland-client + cairo.

## Components

- `kstroke` — the Wayland overlay daemon
- `kstroke-toggle` — single-shot toggle daemon: first Insert press starts or
  kills the overlay, then exits. Map any key to it in your compositor.

## Install (curl one-liner)

```sh
curl -sSL https://raw.githubusercontent.com/gbsaito/kstroke/master/install.sh | sh
```

Installs `/usr/local/bin/kstroke` and `/usr/local/bin/kstroke-toggle` from
the latest GitHub release (linux amd64/arm64 tarballs).

## Build from source

```sh
cargo build --release   # needs cairo/wayland dev libs
```

## Uninstall

```sh
curl -sSL https://raw.githubusercontent.com/gbsaito/kstroke/master/uninstall.sh | sh
```

## Input-group requirement

Unprivileged use requires your user in the `input` group.
