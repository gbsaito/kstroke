#!/bin/sh
# kstroke uninstall — removes installed binaries and kills processes.
# Usage: curl -sSL https://raw.githubusercontent.com/gbsaito/kstroke/master/uninstall.sh | sh
set -eu

PREFIX="${KSTROKE_INSTALL:-/usr/local}"
pkill -9 -x kstroke 2>/dev/null || true
pkill -9 -x kstroke-toggle 2>/dev/null || true
rm -f "$PREFIX/bin/kstroke" "$PREFIX/bin/kstroke-toggle"
echo "kstroke removed from $PREFIX/bin"
