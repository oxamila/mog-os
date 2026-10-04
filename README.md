# mog-os

[![CI](https://github.com/oxamila/mog-os/actions/workflows/ci.yml/badge.svg)](https://github.com/oxamila/mog-os/actions/workflows/ci.yml)

Full walkthrough (controls, tests, real hardware, contribution flow):
[MANUAL.md](MANUAL.md)

A hobby x86-64 operating system written in Rust, targeting Intel Atom
machines. Boots via [Limine](https://github.com/Limine-Bootloader/Limine)
on both UEFI and legacy BIOS, paints a UEFI GOP / VESA framebuffer banner,
logs to a 16550 serial console, and shows an on-screen menu with
**Restart/Shutdown** buttons (PS/2 keyboard: ←/→/Tab select, Enter
confirm, R restart, S shutdown).

## Quick start

Requirements: Rust stable (with the `x86_64-unknown-none` target),
`qemu-system-x86`, `ovmf`, `mtools`, GNU make.

```sh
make build     # compile the kernel (release)
make smoke     # headless boot test (KVM + OVMF), PASS/FAIL from serial
make run       # interactive boot in QEMU with display + serial on stdio
make help      # list all targets
```

Other test modes: `make smoke-bios` (legacy BIOS/SeaBIOS path),
`make smoke-atom` (TCG, `-cpu Denverton`), `make smoke-power`
(restart/shutdown button tests — QEMU must actually exit),
`make run-atom`.

## Compatibility

Works on essentially any x86-64 PC from ~2006 onward:

- **64-bit CPU required** — the kernel is x86-64 only. 32-bit-only machines
  (early Atom N270/N280, Pentium M, pre-2006 PCs) will not boot.
- **Legacy BIOS PCs**: work out of the box, no firmware settings needed.
- **UEFI PCs**: disable Secure Boot first — Limine is not Microsoft-signed.
- **32-bit-only UEFI** (rare 2009–2011 boards/Macs) is not supported; the
  image ships `BOOTX64.EFI` only.
- No serial port or specific GPU required: without COM1 the kernel still
  boots and reports status on the framebuffer alone.

Verified on a Dell Inspiron Mini 1018 (Atom N450, 2010, legacy BIOS) and
under QEMU with OVMF/UEFI, SeaBIOS, and `-cpu Denverton` (TCG).

## Real hardware

```sh
make image                                    # builds build/mog-os.img
sudo dd if=build/mog-os.img of=/dev/sdX bs=4M conv=fsync status=progress
```

The image is hybrid: GPT with an EFI System partition (UEFI machines) and
a BIOS boot partition with Limine's BIOS stages (legacy machines — no
UEFI setup or Secure Boot changes needed; pick the plain USB entry in the
boot menu).

## Layout

```
kernel/     # no_std kernel: entry, Limine requests, serial, framebuffer, panic
scripts/    # fetch-limine, mkimage, run-qemu, smoke
Makefile    # build · image · run · smoke* · debug · clean
limine.conf # Limine bootloader configuration
```
