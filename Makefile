# mog-os — top-level build driver. Run `make help` for the target list.

PROFILE  ?= release
QEMU_MODE ?= kvm

ifeq ($(PROFILE),release)
CARGO_FLAGS  := --release
TARGET_SUBDIR := release
else
CARGO_FLAGS  :=
TARGET_SUBDIR := debug
endif

export PROFILE

.PHONY: all build limine image run run-bios run-atom smoke smoke-bios smoke-atom smoke-power debug clean help

all: build

## build: compile the kernel for x86_64-unknown-none
build:
	cargo build $(CARGO_FLAGS)

## limine: fetch the Limine UEFI loader into third_party/
limine:
	./scripts/fetch-limine.sh

## image: build the bootable disk image (build/mog-os.img)
image: build limine
	./scripts/mkimage.sh

## run: boot the image in QEMU (KVM, -cpu host) with display + serial on terminal
run: image
	./scripts/run-qemu.sh $(QEMU_MODE)

## run-bios: boot via legacy BIOS/SeaBIOS (the Dell Inspiron Mini 1018 path)
run-bios: image
	./scripts/run-qemu.sh bios

## run-atom: boot under TCG emulation with -cpu Denverton (Atom-class CPU)
run-atom: image
	./scripts/run-qemu.sh atom

## smoke: headless boot test (KVM + OVMF); PASS/FAIL from serial output
smoke: image
	./scripts/smoke.sh kvm

## smoke-bios: headless legacy-BIOS boot test (SeaBIOS, no UEFI firmware)
smoke-bios: image
	./scripts/smoke.sh bios

## smoke-atom: headless boot test under -cpu Denverton (slower, no KVM)
smoke-atom: image
	./scripts/smoke.sh atom

## smoke-power: shutdown/restart tests (QEMU must actually exit; TCG)
smoke-power: image
	./scripts/power-test.sh atom restart
	./scripts/power-test.sh atom shutdown

## debug: start QEMU halted (-s -S); attach gdb in another terminal
debug: image
	@echo "Attach gdb now:"
	@echo "  gdb -q -ex 'target remote :1234' -ex 'symbol-file target/x86_64-unknown-none/$(TARGET_SUBDIR)/mog-kernel'"
	./scripts/run-qemu.sh $(QEMU_MODE) -s -S

## clean: remove build artifacts (keeps fetched Limine binaries)
clean:
	cargo clean
	rm -rf build

## help: list targets
help:
	@grep -E '^## ' Makefile | sed 's/^## //'
