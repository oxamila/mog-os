# mog-os

A hobby x86-64 operating system written in Rust, targeting Intel Atom
machines. Boots via [Limine](https://github.com/Limine-Bootloader/Limine)
on both UEFI and legacy BIOS, paints a UEFI GOP / VESA framebuffer banner,
and logs to a 16550 serial console.

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
`make smoke-atom` (TCG, `-cpu Denverton`), `make run-atom`.

## Real hardware

```sh
make image                                    # builds build/mog-os.img
sudo dd if=build/mog-os.img of=/dev/sdX bs=4M conv=fsync status=progress
```

The image is hybrid: GPT with an EFI System partition (UEFI machines) and
a BIOS boot partition with Limine's BIOS stages (legacy machines — no
UEFI setup or Secure Boot changes needed; pick the plain USB entry in the
boot menu).

Verified on a Dell Inspiron Mini 1018 (Atom N450, 2010, legacy BIOS only).

## Layout

```
kernel/     # no_std kernel: entry, Limine requests, serial, framebuffer, panic
scripts/    # fetch-limine, mkimage, run-qemu, smoke
Makefile    # build · image · run · smoke* · debug · clean
limine.conf # Limine bootloader configuration
```
