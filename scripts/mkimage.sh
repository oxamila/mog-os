#!/usr/bin/env bash
# Build the bootable disk image: 128 MB GPT disk, bootable on BOTH:
#   - UEFI machines   (EFI System partition, FAT32, \EFI\BOOT\BOOTX64.EFI)
#   - legacy BIOS     (BIOS boot partition, `limine bios-install` stages)
# The Dell Inspiron Mini 1018 (2010, no UEFI) needs the second path.
# No root, no loop mounts — sfdisk + mtools work on plain files.
#
# The resulting build/mog-os.img can be `dd`'d straight onto a USB stick.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PROFILE="${PROFILE:-release}"

if [ "$PROFILE" = "release" ]; then
    SUBDIR="release"
else
    SUBDIR="debug"
fi

KERNEL="$ROOT/target/x86_64-unknown-none/$SUBDIR/mog-kernel"
IMG="$ROOT/build/mog-os.img"
LIMINE_DIR="$ROOT/third_party/limine"
LIMINE_EFI="$LIMINE_DIR/BOOTX64.EFI"
LIMINE_BIOS_SYS="$LIMINE_DIR/limine-bios.sys"
LIMINE_TOOL="$LIMINE_DIR/limine"

# GPT partition type GUIDs.
GUID_BIOSBOOT="21686148-6449-6E6F-744E-656564454649"
GUID_ESP="C12A7328-F81F-11D2-BA4B-00A0C93EC93B"

die() { echo "mkimage: ERROR: $*" >&2; exit 1; }

[ -f "$KERNEL" ] || die "kernel not built (expected $KERNEL) — run 'make build' first"
[ -f "$LIMINE_EFI" ] || die "Limine loader missing (expected $LIMINE_EFI) — run 'make limine' first"
[ -f "$LIMINE_BIOS_SYS" ] || die "limine-bios.sys missing — run 'make limine' first"
[ -x "$LIMINE_TOOL" ] || die "limine host tool missing — run 'make limine' first"
[ -f "$ROOT/limine.conf" ] || die "limine.conf missing at repo root"

mkdir -p "$ROOT/build"
rm -f "$IMG"
dd if=/dev/zero of="$IMG" bs=1M count=128 status=none

# GPT layout:
#   p1  BIOS boot partition, 1 MiB @ 2048   (holds Limine BIOS stage 2)
#   p2  EFI System part.,  @ 4096 (2 MiB)   (FAT32: EFI loader + kernel)
printf 'label: gpt\nstart=2048, size=2048, type=%s\nstart=4096, size=258000, type=%s\n' \
    "$GUID_BIOSBOOT" "$GUID_ESP" | sfdisk -q "$IMG"

# FAT32 filesystem at the ESP offset (@@2M == sector 4096).
mformat -i "$IMG@@2M" -F ::

mmd -i "$IMG@@2M" ::/EFI ::/EFI/BOOT ::/boot
mcopy -i "$IMG@@2M" "$LIMINE_EFI"  ::/EFI/BOOT/BOOTX64.EFI
mcopy -i "$IMG@@2M" "$ROOT/limine.conf" ::/limine.conf
mcopy -i "$IMG@@2M" "$KERNEL"      ::/boot/mog-kernel

# Legacy-BIOS stage: MBR boot code + stage 2 in partition 1 (BIOS boot).
# `limine-bios.sys` lives on the FAT partition next to limine.conf —
# the search path is boot/limine, boot, limine, or partition root.
mcopy -i "$IMG@@2M" "$LIMINE_BIOS_SYS" ::/limine-bios.sys
"$LIMINE_TOOL" bios-install "$IMG" 1

echo "mkimage: built $IMG (UEFI + legacy BIOS bootable)"
echo "mkimage: contents:"
mdir -i "$IMG@@2M" ::/EFI/BOOT
mdir -i "$IMG@@2M" ::/boot
mdir -i "$IMG@@2M" ::/
