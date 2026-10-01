//! MOG-OS kernel entry point.
//!
//! Boot flow: UEFI firmware → Limine (BOOTX64.EFI) → limine.conf →
//! this ELF64, entered in 64-bit long mode with paging on, a valid GDT,
//! a ≥64 KiB stack in bootloader-reclaimable memory, interrupts off.
//!
//! Limine guarantees (x86-64, base revision 3):
//!   - kernel segments mapped write-back, framebuffer WC (PAT[5])
//!   - general purpose registers zeroed, `rsp` = top of boot stack
//!   - IDT undefined — we must load our own (phase 2 work)

#![no_std]
#![no_main]

mod fb;
mod panic;
mod requests;
mod serial;

use fb::Fb;
use limine::memory_map::EntryType;
use requests::{BASE_REVISION, FRAMEBUFFER_REQUEST, MEMMAP_REQUEST, RSDP_REQUEST};

/// Human-readable name for a Limine memory-map entry type.
fn mem_type_name(t: EntryType) -> &'static str {
    match t {
        EntryType::USABLE => "usable",
        EntryType::RESERVED => "reserved",
        EntryType::ACPI_RECLAIMABLE => "acpi-reclaim",
        EntryType::ACPI_NVS => "acpi-nvs",
        EntryType::BAD_MEMORY => "bad",
        EntryType::BOOTLOADER_RECLAIMABLE => "bootloader-reclaim",
        EntryType::EXECUTABLE_AND_MODULES => "kernel",
        EntryType::FRAMEBUFFER => "framebuffer",
        _ => "other",
    }
}

/// Park the CPU until (future) interrupts wake it. Used after boot and on panic.
pub fn halt_loop() -> ! {
    loop {
        unsafe {
            core::arch::asm!("hlt", options(nomem, nostack, preserves_flags));
        }
    }
}

#[unsafe(no_mangle)]
unsafe extern "C" fn kmain() -> ! {
    serial::init();
    kprintln!();
    kprintln!("MOG-OS 0.1.0 — x86-64 kernel (UEFI + Limine, Intel Atom target)");
    kprintln!("--------------------------------------------------");

    // --- Limine handshake -------------------------------------------------
    if BASE_REVISION.is_supported() {
        match BASE_REVISION.loaded_revision() {
            Some(rev) => kprintln!("[limine] base revision {rev} active"),
            None => kprintln!("[limine] base revision tag unanswered"),
        }
    } else {
        kprintln!("[limine] WARNING: base revision request not honoured");
    }

    // --- Physical memory map (feeds the future PMM) -----------------------
    match MEMMAP_REQUEST.get_response() {
        Some(resp) => {
            let entries = resp.entries();
            let mut usable = 0u64;
            for e in entries {
                if e.entry_type == EntryType::USABLE {
                    usable += e.length;
                }
                kprintln!(
                    "[mem] {:#016x} + {:#010x}  {}",
                    e.base,
                    e.length,
                    mem_type_name(e.entry_type)
                );
            }
            kprintln!(
                "[mem] {} entries total, {} MiB usable RAM",
                entries.len(),
                usable / (1024 * 1024)
            );
        }
        None => kprintln!("[mem] WARNING: bootloader provided no memory map"),
    }

    // --- ACPI RSDP (phase 3 will walk RSDT → MADT → SMP) -------------------
    // Base revision 3 reports a physical address.
    match RSDP_REQUEST.get_response() {
        Some(resp) => kprintln!("[acpi] RSDP at {:#x}", resp.address()),
        None => kprintln!("[acpi] RSDP not reported by bootloader"),
    }

    // --- Framebuffer: paint a banner + prove the memory is writable --------
    // Kept after the match so the success line can be drawn on-screen too —
    // real hardware has no COM1 to read a serial log from.
    let mut screen: Option<Fb> = None;
    match FRAMEBUFFER_REQUEST.get_response() {
        Some(resp) => match resp.framebuffers().next() {
            Some(info) => {
                kprintln!(
                    "[fb] {}x{}, pitch {} B, {} bpp, base {:p}",
                    info.width(),
                    info.height(),
                    info.pitch(),
                    info.bpp(),
                    info.addr()
                );

                if info.bpp() != 32 {
                    kprintln!("[fb] WARNING: expected 32 bpp, skipping render");
                } else {
                    let scr = Fb::new(info);
                    let (w, h) = (scr.width(), scr.height());

                    // Dark slate background + amber border.
                    scr.fill(0x0016_222e);
                    scr.draw_frame(8, 8, w - 16, h - 16, 0x00ff_cc33, 3);

                    // Title, centered.
                    let title = "MOG-OS";
                    let scale = 6;
                    let tx = (w - Fb::text_width(title, scale)) / 2;
                    scr.draw_text(tx, h / 4, title, 0x00ff_ffff, scale);

                    let sub = "hello from rust + limine + uefi";
                    let scale = 2;
                    let sx = (w - Fb::text_width(sub, scale)) / 2;
                    scr.draw_text(sx, h / 4 + 70, sub, 0x0099_e0ff, scale);

                    // Write-then-read a pixel: verifies GOP memory is RW.
                    const MAGIC: u32 = 0x00ff_00ff;
                    scr.put_pixel(24, 24, MAGIC);
                    let read = scr.get_pixel(24, 24);
                    let (status, status_color) = if read == MAGIC {
                        kprintln!("[fb] read-back OK ({read:#010x})");
                        ("read-back OK", 0x0066_ff66)
                    } else {
                        kprintln!("[fb] read-back FAIL (wrote {MAGIC:#010x}, read {read:#010x})");
                        ("read-back FAIL", 0x00ff_5555)
                    };
                    let cx = (w - Fb::text_width(status, 2)) / 2;
                    scr.draw_text(cx, h / 4 + 104, status, status_color, 2);

                    screen = Some(scr);
                }
            }
            None => kprintln!("[fb] WARNING: framebuffer list is empty"),
        },
        None => kprintln!("[fb] WARNING: no framebuffer response"),
    }

    kprintln!("--------------------------------------------------");
    kprintln!("MOG-OS ready.");

    // On-screen success line: full boot confirmed without needing serial.
    if let Some(scr) = &screen {
        let msg = "MOG-OS ready.";
        let x = scr.width().saturating_sub(Fb::text_width(msg, 3)) / 2;
        let y = scr.height().saturating_sub(56);
        scr.draw_text(x, y, msg, 0x0066_ff66, 3);
    }

    halt_loop()
}
