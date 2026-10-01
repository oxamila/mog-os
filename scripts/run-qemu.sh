#!/usr/bin/env bash
# Boot build/mog-os.img under QEMU.
#
# Usage: run-qemu.sh [kvm|atom|tcg|bios] [extra qemu args...]
#   kvm   — OVMF UEFI,  KVM, -cpu host        (fast iteration; default)
#   atom  — OVMF UEFI,  TCG, -cpu Denverton    (modern Atom-class CPU)
#   tcg   — OVMF UEFI,  TCG, -cpu max          (no accel, generic)
#   bios  — SeaBIOS legacy boot, KVM           (Dell Inspiron Mini 1018 path)
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

MODE="kvm"
if [ $# -ge 1 ]; then
    MODE="$1"
    shift
fi

IMG="$ROOT/build/mog-os.img"
VARS="$ROOT/build/OVMF_VARS.fd"
CODE="/usr/share/OVMF/OVMF_CODE_4M.fd"
VARS_SRC="/usr/share/OVMF/OVMF_VARS_4M.fd"

[ -f "$IMG" ] || { echo "run-qemu: missing $IMG — run 'make image' first" >&2; exit 1; }

ARGS=(
    -m 2G
    # 1 vCPU: OVMF occasionally hangs its 2-vCPU bring-up under KVM on this
    # host (~25% boot stall, pre-Limine). Kernel has no SMP support yet.
    -smp 1
    -no-reboot
    -drive "format=raw,file=$IMG"
    -serial stdio
    -net none
)

case "$MODE" in
    kvm|atom|tcg)
        [ -f "$CODE" ] || { echo "run-qemu: OVMF not found at $CODE (apt install ovmf)" >&2; exit 1; }
        mkdir -p "$ROOT/build"
        # Fresh writable copy of the UEFI NVRAM variables each boot.
        cp -f "$VARS_SRC" "$VARS"
        ARGS+=(
            -machine q35
            -drive "if=pflash,format=raw,readonly=on,file=$CODE"
            -drive "if=pflash,format=raw,file=$VARS"
        )
        ;;
    bios)
        # Legacy BIOS boot path (SeaBIOS) — same as the 2010 Dell netbook.
        ARGS+=( -machine pc )
        ;;
    *)
        echo "usage: $0 [kvm|atom|tcg|bios] [extra qemu args...]" >&2
        exit 1
        ;;
esac

case "$MODE" in
    kvm)
        [ -r /dev/kvm ] && [ -w /dev/kvm ] || {
            echo "run-qemu: /dev/kvm not accessible — use '$0 atom'" >&2
            exit 1
        }
        ARGS+=( -enable-kvm -cpu host )
        ;;
    bios)
        if [ -r /dev/kvm ] && [ -w /dev/kvm ]; then
            ARGS+=( -enable-kvm -cpu host )
        else
            ARGS+=( -cpu max )
        fi
        ;;
    atom)
        ARGS+=( -cpu Denverton )
        ;;
    tcg)
        ARGS+=( -cpu max )
        ;;
esac

# Headless fallback when no graphical session is available.
if [ -z "${DISPLAY:-}" ] && [ -z "${WAYLAND_DISPLAY:-}" ]; then
    echo "run-qemu: no display detected — running headless (serial on stdout)" >&2
    ARGS+=( -display none )
fi

echo "run-qemu: mode=$MODE"
exec qemu-system-x86_64 "${ARGS[@]}" "$@"
