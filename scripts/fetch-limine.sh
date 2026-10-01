#!/usr/bin/env bash
# Fetch the official Limine binary release (UEFI loader) into third_party/.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
VER="12.9.1"
DEST="$ROOT/third_party/limine"
URL="https://github.com/Limine-Bootloader/Limine/releases/download/v${VER}/limine-binary.tar.xz"
# Pre-seeded cache (downloaded during environment research, if present).
CACHE="/tmp/opencode/limine-research/limine-binary.tar.xz"

# Build the `limine` host tool (single C file, ships in the release tarball).
# `limine bios-install` is what makes the image bootable on legacy-BIOS
# machines such as the Dell Inspiron Mini 1018 (no UEFI at all).
build_tool() {
    if [ ! -x "$DEST/limine" ]; then
        echo "limine: building host tool (bios-install)"
        make -C "$DEST" limine >/dev/null
    fi
    [ -x "$DEST/limine" ] || { echo "limine: ERROR — host tool build failed" >&2; exit 1; }
}

if [ -f "$DEST/BOOTX64.EFI" ]; then
    echo "limine: already present ($DEST/BOOTX64.EFI)"
    build_tool
    exit 0
fi

mkdir -p "$DEST"

tarball="$CACHE"
if [ ! -f "$tarball" ]; then
    tarball="$(mktemp --suffix=.tar.xz)"
    echo "limine: downloading $URL"
    curl -fL -o "$tarball" "$URL"
else
    echo "limine: using cached tarball $tarball"
fi

tar -xJf "$tarball" -C "$DEST" --strip-components=1

if [ ! -f "$DEST/BOOTX64.EFI" ]; then
    echo "limine: ERROR — BOOTX64.EFI missing after extraction" >&2
    exit 1
fi
build_tool
echo "limine: v${VER} extracted to $DEST"
