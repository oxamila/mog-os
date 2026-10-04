#!/usr/bin/env bash
# Power test: boot the image, wait for the ready banner, inject a real
# keystroke through the QEMU monitor, and verify the guest actually
# restarts / shuts down (QEMU must exit on its own — -no-reboot makes a
# guest reset end the VM too).
#
# Usage: power-test.sh [kvm|atom|bios] [restart|shutdown]
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

MODE="${1:-kvm}"
ACTION="${2:-restart}"

IMG="$ROOT/build/mog-os.img"
VARS="$ROOT/build/OVMF_VARS.fd"
CODE="/usr/share/OVMF/OVMF_CODE_4M.fd"
VARS_SRC="/usr/share/OVMF/OVMF_VARS_4M.fd"
LOG="$ROOT/build/serial-power.log"
MONLOG="$ROOT/build/monitor.log"
FIFO="$ROOT/build/monitor.fifo"

die() { echo "power: $*" >&2; exit 1; }

[ -f "$IMG" ] || die "missing $IMG — run 'make image' first"
[ -f "$CODE" ] || die "OVMF not found at $CODE (apt install ovmf)"

case "$ACTION" in
    restart)  KEY="r"; LABEL="restart" ;;
    shutdown) KEY="s"; LABEL="shutdown" ;;
    *) die "usage: power-test.sh [kvm|atom|bios] [restart|shutdown]" ;;
esac

case "$MODE" in
    kvm)
        [ -r /dev/kvm ] && [ -w /dev/kvm ] || die "/dev/kvm not accessible — use atom mode"
        CPU_ARGS=( -enable-kvm -cpu host )
        TIMEOUT="${SMOKE_TIMEOUT:-60}"
        ;;
    atom)
        CPU_ARGS=( -cpu Denverton )
        TIMEOUT="${SMOKE_TIMEOUT:-180}"
        ;;
    bios)
        if [ -r /dev/kvm ] && [ -w /dev/kvm ]; then
            CPU_ARGS=( -enable-kvm -cpu host )
            TIMEOUT="${SMOKE_TIMEOUT:-60}"
        else
            CPU_ARGS=( -cpu max )
            TIMEOUT="${SMOKE_TIMEOUT:-180}"
        fi
        ;;
    *) die "usage: power-test.sh [kvm|atom|bios] [restart|shutdown]" ;;
esac

mkdir -p "$ROOT/build"

# Firmware: OVMF for UEFI modes, plain SeaBIOS for legacy BIOS mode.
FIRMWARE_ARGS=(
    -drive "if=pflash,format=raw,readonly=on,file=$CODE"
    -drive "if=pflash,format=raw,file=$VARS"
)
MACHINE="q35"
if [ "$MODE" = "bios" ]; then
    FIRMWARE_ARGS=()
    MACHINE="pc"
fi

markers_ok() {
    [ -s "$LOG" ] \
        && grep -q "MOG-OS ready" "$LOG" \
        && grep -q "read-back OK" "$LOG"
}

QEMU_PID=""
cleanup() {
    if [ -n "$QEMU_PID" ] && kill -0 "$QEMU_PID" 2>/dev/null; then
        kill "$QEMU_PID" 2>/dev/null || true
    fi
    exec 3>&- 2>/dev/null || true
    rm -f "$FIFO"
}
trap cleanup EXIT

RESULT_RC=""

for ATTEMPT in 1 2; do
    cp -f "$VARS_SRC" "$VARS"
    rm -f "$LOG" "$MONLOG" "$FIFO"
    mkfifo "$FIFO"
    # RDWR open so neither side ever blocks waiting for the other.
    exec 3<>"$FIFO"

    echo "power: booting (mode=$MODE, action=$LABEL, attempt=$ATTEMPT)"
    qemu-system-x86_64 \
        -machine "$MACHINE" \
        -m 2G \
        -smp 1 \
        -no-reboot \
        -display none \
        "${FIRMWARE_ARGS[@]}" \
        -drive "format=raw,file=$IMG" \
        -serial "file:$LOG" \
        -monitor stdio \
        -net none \
        "${CPU_ARGS[@]}" \
        <"$FIFO" >"$MONLOG" 2>&1 &
    QEMU_PID=$!

    # 1) Wait for the ready banner (kernel is then in its key-poll loop).
    deadline=$((SECONDS + TIMEOUT))
    ready=0
    while [ "$SECONDS" -lt "$deadline" ]; do
        if markers_ok; then
            ready=1
            break
        fi
        if ! kill -0 "$QEMU_PID" 2>/dev/null; then
            break
        fi
        sleep 0.25
    done

    if [ "$ready" -ne 1 ]; then
        echo "power: attempt $ATTEMPT did not reach the kernel — retrying" >&2
        kill "$QEMU_PID" 2>/dev/null || true
        wait "$QEMU_PID" 2>/dev/null || true
        QEMU_PID=""
        continue
    fi

    sleep 0.5 # let the input loop settle after kbd::init
    printf 'sendkey %s\n' "$KEY" >&3 || true

    # 2) Guest must power off / reset → QEMU exits by itself.
    exit_deadline=$((SECONDS + 30))
    RESULT_RC=""
    while [ "$SECONDS" -lt "$exit_deadline" ]; do
        if ! kill -0 "$QEMU_PID" 2>/dev/null; then
            RESULT_RC=0
            wait "$QEMU_PID" || RESULT_RC=$?
            QEMU_PID=""
            break
        fi
        sleep 0.25
    done

    if [ -n "$RESULT_RC" ]; then
        break
    fi

    echo "power: attempt $ATTEMPT — guest did not exit within 30s after '$LABEL' key — retrying" >&2
    kill "$QEMU_PID" 2>/dev/null || true
    wait "$QEMU_PID" 2>/dev/null || true
    QEMU_PID=""
done

if [ -z "$RESULT_RC" ]; then
    echo "power: FAIL (mode=$MODE, action=$LABEL) — guest never reached '$LABEL'" >&2
    [ -f "$LOG" ] && { echo "---- serial log ----"; tail -n 25 "$LOG" >&2; }
    [ -f "$MONLOG" ] && { echo "---- monitor log ----"; tail -n 10 "$MONLOG" >&2; }
    exit 1
fi

if [ "$RESULT_RC" -ne 0 ]; then
    echo "power: QEMU exited with rc=$RESULT_RC after '$LABEL' key (expected 0)" >&2
    [ -f "$LOG" ] && { echo "---- serial log ----"; tail -n 25 "$LOG" >&2; }
    [ -f "$MONLOG" ] && { echo "---- monitor log ----"; tail -n 10 "$MONLOG" >&2; }
    exit 1
fi

# Evidence check: rc=0 alone can also mean the kernel triple-faulted (reset
# with -no-reboot looks identical). Require the proof lines in the serial log.
if [ "$ACTION" = "shutdown" ] && ! grep -q "PM1a_CNT" "$LOG"; then
    echo "power: FAIL — QEMU exited but no ACPI shutdown evidence (triple fault?)" >&2
    tail -n 25 "$LOG" >&2 || true
    exit 1
fi
if [ "$ACTION" = "restart" ] && ! grep -q "\[power\] restart" "$LOG"; then
    echo "power: FAIL — QEMU exited but the restart was never requested" >&2
    tail -n 25 "$LOG" >&2 || true
    exit 1
fi

echo "power: PASS (mode=$MODE, action=$LABEL)"
exit 0
