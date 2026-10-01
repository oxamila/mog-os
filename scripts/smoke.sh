#!/usr/bin/env bash
# Headless smoke test: boot the image, capture serial output, and verify
# that the kernel reached its ready banner and the framebuffer read-back
# check passed. Exits 0 on PASS, 1 on FAIL.
#
# Up to 2 attempts: OVMF occasionally stalls forever under KVM on this host
# (firmware-side, before the kernel runs) — a known QEMU/OVMF quirk, so a
# single stalled boot is retried before declaring failure.
#
# Usage: smoke.sh [kvm|atom|bios]
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

MODE="${1:-kvm}"

IMG="$ROOT/build/mog-os.img"
VARS="$ROOT/build/OVMF_VARS.fd"
CODE="/usr/share/OVMF/OVMF_CODE_4M.fd"
VARS_SRC="/usr/share/OVMF/OVMF_VARS_4M.fd"
LOG="$ROOT/build/serial.log"

die() { echo "smoke: $*" >&2; exit 1; }

[ -f "$IMG" ] || die "missing $IMG — run 'make image' first"
[ -f "$CODE" ] || die "OVMF not found at $CODE (apt install ovmf)"

case "$MODE" in
    kvm)
        [ -r /dev/kvm ] && [ -w /dev/kvm ] || die "/dev/kvm not accessible — run 'make smoke-atom'"
        CPU_ARGS=( -enable-kvm -cpu host )
        # 60s: OVMF occasionally crawls under host CPU contention before
        # reaching the kernel (KVM timer-tick starvation); most recover.
        TIMEOUT="${SMOKE_TIMEOUT:-60}"
        ;;
    atom)
        CPU_ARGS=( -cpu Denverton )
        TIMEOUT="${SMOKE_TIMEOUT:-180}"
        ;;
    bios)
        # Legacy BIOS (SeaBIOS, no OVMF) — what the Dell Inspiron Mini 1018 has.
        if [ -r /dev/kvm ] && [ -w /dev/kvm ]; then
            CPU_ARGS=( -enable-kvm -cpu host )
            TIMEOUT="${SMOKE_TIMEOUT:-60}"
        else
            CPU_ARGS=( -cpu max )
            TIMEOUT="${SMOKE_TIMEOUT:-180}"
        fi
        ;;
    *)
        die "usage: smoke.sh [kvm|atom|bios]"
        ;;
esac

mkdir -p "$ROOT/build"

# Firmware: OVMF for UEFI modes, plain SeaBIOS for legacy BIOS mode.
FIRMWARE_ARGS=(
    -drive "if=pflash,format=raw,readonly=on,file=$CODE"
    -drive "if=pflash,format=raw,file=$VARS"
)
MACHINE="q35"
if [ "${MODE:-}" = "bios" ]; then
    FIRMWARE_ARGS=()
    MACHINE="pc"
fi

markers_ok() {
    [ -s "$LOG" ] \
        && grep -q "MOG-OS ready" "$LOG" \
        && grep -q "read-back OK" "$LOG"
}

for ATTEMPT in 1 2; do
    cp -f "$VARS_SRC" "$VARS"
    rm -f "$LOG"

    echo "smoke: booting (mode=$MODE, attempt=$ATTEMPT, timeout=${TIMEOUT}s, log=$LOG)"
    set +e
    timeout --foreground "$TIMEOUT" qemu-system-x86_64 \
        -machine "$MACHINE" \
        -m 2G \
        -smp 1 \
        -no-reboot \
        -display none \
        "${FIRMWARE_ARGS[@]}" \
        -drive "format=raw,file=$IMG" \
        -serial "file:$LOG" \
        -net none \
        "${CPU_ARGS[@]}" \
        >/dev/null 2>&1
    QEMU_RC=$?
    set -e

    # 124 = timeout killed us, which is expected: the kernel halts in a loop
    # and never powers the machine off.
    if [ "$QEMU_RC" -ne 0 ] && [ "$QEMU_RC" -ne 124 ]; then
        echo "smoke: QEMU exited early (rc=$QEMU_RC)" >&2
        [ -f "$LOG" ] && { echo "---- serial log ----"; cat "$LOG"; }
        exit 1
    fi

    if markers_ok; then
        echo "smoke: ---- serial log tail ----"
        tail -n 12 "$LOG" || true
        echo "smoke: PASS (mode=$MODE)"
        exit 0
    fi

    if [ "$ATTEMPT" -lt 2 ]; then
        echo "smoke: attempt $ATTEMPT did not reach the kernel (OVMF stall — retrying)" >&2
    fi
done

if [ ! -s "$LOG" ]; then
    die "serial log empty on both attempts — kernel never reached the serial port"
fi

echo "smoke: ---- serial log tail ----" >&2
tail -n 25 "$LOG" >&2 || true
echo "smoke: FAIL (mode=$MODE) — expected markers missing from serial log" >&2
exit 1
