//! Power control: ACPI S5 shutdown + reset via chipset fallbacks.
//!
//! Shutdown walks the ACPI tables Limine located (RSDP → RSDT/XSDT → FADT)
//! to find the PM1a control block — the firmware-declared way to power off —
//! with a hardcoded QEMU/ICH fallback for when parsing fails.
//!
//! Restart uses the classic PC reset paths in order of politeness:
//! keyboard-controller reset pulse → Intel chipset reset port → triple fault.

use crate::io::{outb, outw};
use crate::kprintln;
use crate::requests::{HHDM_REQUEST, RSDP_REQUEST};

/// SLP_EN bit (bit 13) of PM1a_CNT — must be set to start the transition.
const SLP_EN: u16 = 1 << 13;

/// SLP_TYP candidates for S5. Intel/real firmware declares 5 in the DSDT
/// `_S5` package; QEMU's PM1 handler powers off on type 0 (its `_S5`
/// declares 0 — type 5 lands in its default case and is ignored). The
/// correct value comes from parsing `_S5` in the DSDT, which needs an AML
/// decoder — trying both conventions covers both worlds, and a wrong type
/// is simply ignored by well-behaved chipsets.
const SLP_CANDIDATES: [u16; 2] = [(5 << 10) | SLP_EN, SLP_EN];

/// PM1a_CNT port used by QEMU (SeaBIOS and OVMF) and typical ICH firmware.
const PM1A_FALLBACK: u16 = 0x604;

/// Let an asynchronous power transition land before trying the next method.
/// A successful write kills the CPU long before this returns; a failed one
/// costs a few tens of milliseconds.
fn settle() {
    for _ in 0..2_000_000 {
        core::hint::spin_loop();
    }
}

#[inline]
fn virt(phys: u64, hhdm: u64) -> *const u8 {
    phys.wrapping_add(hhdm) as *const u8
}

unsafe fn read_u8(p: *const u8) -> u8 {
    unsafe { core::ptr::read_volatile(p) }
}

unsafe fn read_u32(p: *const u8) -> u32 {
    unsafe { core::ptr::read_unaligned(p as *const u32) }
}

unsafe fn read_u64(p: *const u8) -> u64 {
    unsafe { core::ptr::read_unaligned(p as *const u64) }
}

unsafe fn sig_is(p: *const u8, sig: &[u8; 4]) -> bool {
    let bytes = unsafe { core::slice::from_raw_parts(p, 4) };
    bytes == &sig[..]
}

/// ACPI checksum over `len` bytes must sum to 0 mod 256.
unsafe fn checksum_ok(base: *const u8, len: usize) -> bool {
    let mut sum = 0u32;
    for i in 0..len {
        sum += unsafe { read_u8(base.add(i)) } as u32;
    }
    sum & 0xFF == 0
}

/// Locate the PM1a_CNT block by walking RSDP → RSDT/XSDT → FADT.
fn fadt_pm1a_port() -> Option<u16> {
    let hhdm = HHDM_REQUEST.get_response()?.offset();
    // Base revision ≥4 reports the RSDP already translated to the HHDM
    // virtual address (Limine reported_addr = phys + hhdm) — use it as-is.
    // Pointers *inside* the tables (XSDT, entries) stay physical + hhdm.
    let rsdp = RSDP_REQUEST.get_response()?.address() as *const u8;
    kprintln!("[acpi] parse: rsdp={rsdp:p} hhdm={hhdm:#x}");
    settle();

    unsafe {
        let rsdp_sig = core::slice::from_raw_parts(rsdp, 8);
        if rsdp_sig != b"RSD PTR " || !checksum_ok(rsdp, 20) {
            kprintln!("[acpi] parse: bad RSDP signature/checksum");
            return None;
        }

        let rev = read_u8(rsdp.add(15));
        let (root_phys, stride, root_sig): (u64, usize, &[u8; 4]) = if rev >= 2 {
            let len = read_u32(rsdp.add(20)) as usize;
            if !(36..=4096).contains(&len) || !checksum_ok(rsdp, len) {
                kprintln!("[acpi] parse: bad extended RSDP checksum");
                return None;
            }
            let xsdt = read_u64(rsdp.add(24));
            if xsdt == 0 {
                kprintln!("[acpi] parse: XSDT address is zero");
                return None;
            }
            (xsdt, 8, b"XSDT")
        } else {
            (read_u32(rsdp.add(16)) as u64, 4, b"RSDT")
        };
        kprintln!("[acpi] parse: rev={rev} root={root_phys:#x}");
        settle();

        let root = virt(root_phys, hhdm);
        let rlen = read_u32(root.add(4)) as usize;
        if !(36..=4 * 1024 * 1024).contains(&rlen) || !sig_is(root, root_sig) {
            kprintln!("[acpi] parse: bad root table (len={rlen:#x})");
            return None;
        }

        let count = (rlen - 36) / stride;
        kprintln!("[acpi] parse: root ok, {count} entries");
        settle();
        for i in 0..count {
            let entry = if stride == 8 {
                read_u64(root.add(36 + i * 8))
            } else {
                read_u32(root.add(36 + i * 4)) as u64
            };
            if entry == 0 {
                continue;
            }
            let table = virt(entry, hhdm);
            if sig_is(table, b"FACP") {
                let tlen = read_u32(table.add(4)) as usize;
                if tlen < 68 {
                    continue;
                }
                let port = read_u32(table.add(64)); // PM1a_CNT_BLK
                if port != 0 && port <= 0xFFFF {
                    kprintln!("[acpi] parse: FACP len={tlen} PM1a={port:#x}");
                    settle();
                    return Some(port as u16);
                }
            }
        }
    }
    kprintln!("[acpi] parse: no FACP with PM1a block");
    None
}

/// Power off (ACPI S5). Returns `false` only if the machine is somehow
/// still running after every attempt — i.e. the user can keep typing.
pub fn shutdown() -> bool {
    settle(); // drain the caller's "[power] shutdown" log line first

    let fadt_port = fadt_pm1a_port();
    kprintln!("[power] fadt_pm1a={fadt_port:?} fallback={PM1A_FALLBACK:#x}");
    settle(); // drain diagnostics before any power write

    let mut attempts = [0u16; 2];
    if let Some(port) = fadt_port {
        attempts[0] = port;
    }
    attempts[1] = PM1A_FALLBACK;

    for (idx, &port) in attempts.iter().enumerate() {
        if port == 0 || (idx == 1 && attempts[0] == port) {
            continue;
        }
        for &val in &SLP_CANDIDATES {
            kprintln!("[power] PM1a_CNT {port:#x} <- {val:#06x}");
            settle(); // let the UART transmit before we cut the power
            outw(port, val);
            settle();
        }
    }
    false
}

/// Restart the machine. Every path either resets the CPU or never returns.
pub fn restart() -> ! {
    settle(); // drain the caller's "[power] restart" log line first
    unsafe {
        // i8042 keyboard controller: pulse the CPU reset line.
        outb(0x64, 0xFE);
        settle();

        // Intel chipset hard-reset port.
        outb(0xCF9, 0x06);
        settle();

        // Last resort: empty IDT + invalid opcode → triple fault → CPU reset.
        let idtr = [0u8; 10];
        core::arch::asm!("lidt [{}]", in(reg) &idtr, options(readonly, nostack));
        core::arch::asm!("ud2", options(noreturn, nostack));
    }
}
