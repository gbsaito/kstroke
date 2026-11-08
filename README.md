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

Install script source (`install.sh`):

```sh
#!/bin/sh
# kstroke install — fetches prebuilt binaries from the latest GitHub release.
# Usage: curl -sSL https://raw.githubusercontent.com/gbsaito/kstroke/master/install.sh | sh
set -eu

PREFIX="${KSTROKE_INSTALL:-/usr/local}"
ARCH="$(uname -m)"
case "$ARCH" in x86_64) A=amd64 ;; aarch64|arm64) A=arm64 ;; *) echo "unsupported arch: $ARCH"; exit 1;; esac

TMP=$(mktemp -d); trap 'rm -rf "$TMP"' EXIT
URL="https://github.com/gbsaito/kstroke/releases/latest/download/kstroke-$A.tar.gz"
curl -fsSL "$URL" -o "$TMP/kstroke.tar.gz"
tar -xzf "$TMP/kstroke.tar.gz" -C "$TMP"

mkdir -p "$PREFIX/bin"
install -m 0755 "$TMP/kstroke/kstroke" "$PREFIX/bin/kstroke"
install -m 0755 "$TMP/kstroke/kstroke-toggle" "$PREFIX/bin/kstroke-toggle"

echo "installed: $PREFIX/bin/kstroke + kstroke-toggle"
echo "toggle daemon: map your compositor's Insert (or any key) to kstroke-toggle"
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

Uninstall script source (`uninstall.sh`):

```sh
#!/bin/sh
# kstroke uninstall — removes installed binaries and kills processes.
# Usage: curl -sSL https://raw.githubusercontent.com/gbsaito/kstroke/master/uninstall.sh | sh
set -eu

PREFIX="${KSTROKE_INSTALL:-/usr/local}"
pkill -9 -x kstroke 2>/dev/null || true
pkill -9 -x kstroke-toggle 2>/dev/null || true
rm -f "$PREFIX/bin/kstroke" "$PREFIX/bin/kstroke-toggle"
echo "kstroke removed from $PREFIX/bin"
```

## Input-group requirement

Unprivileged use requires your user in the `input` group.
