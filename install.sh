#!/bin/sh
# kstroke install — fetches prebuilt binaries from the latest GitHub release.
# Usage: curl -sSL https://raw.githubusercontent.com/gbsaito/kstroke/master/install.sh | sh
set -eu

PREFIX="${KSTROKE_INSTALL:-/usr/local}"
ARCH="$(uname -m)" || true
case "$ARCH" in x86_64) A=amd64 ;; aarch64|arm64) A=arm64 ;; *) echo "unsupported arch: $ARCH"; exit 1;; esac

TMP=$(mktemp -d); trap 'rm -rf "$TMP"' EXIT
URL="https://github.com/gbsaito/kstroke/releases/latest/download/kstroke-$A.tar.gz"
curl -fsSL "$URL" -o "$TMP/kstroke.tar.gz"
tar -xzf "$TMP/kstroke.tar.gz" -C "$TMP"

install -m 0755 "$TMP/kstroke/kstroke" "$PREFIX/bin/kstroke"
install -m 0755 "$TMP/kstroke/kstroke-toggle" "$PREFIX/bin/kstroke-toggle"

echo "installed: $PREFIX/bin/kstroke + kstroke-toggle"
echo "toggle daemon: map your compositor's Insert (or any key) to kstroke-toggle"
