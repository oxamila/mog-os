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
mod io;
mod kbd;
mod panic;
mod power;
mod requests;
mod serial;

use fb::Fb;
use kbd::{Kbd, Key};
use limine::memory_map::EntryType;
use requests::{BASE_REVISION, FRAMEBUFFER_REQUEST, HHDM_REQUEST, MEMMAP_REQUEST, RSDP_REQUEST};

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

    // Higher-half direct map: how power.rs reaches physical ACPI tables.
    match HHDM_REQUEST.get_response() {
        Some(resp) => kprintln!("[limine] HHDM direct map at {:#x}", resp.offset()),
        None => kprintln!("[limine] WARNING: no HHDM — ACPI tables unreadable"),
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
    // Base revision ≥4 reports it as an HHDM address; power.rs parses it.
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

    // --- Interactive menu: power buttons driven by the PS/2 keyboard ------
    // No IDT/IRQs yet, so input is polled (kbd::poll).
    kprintln!("[ui] LEFT/RIGHT select, ENTER confirm, R restart, S shutdown");
    let mut kbd = Kbd::new();
    kbd.init();
    let mut selected = 0usize; // 0 = Restart, 1 = Shutdown
    if let Some(scr) = &screen {
        draw_menu(scr, selected);
    }

    loop {
        let Some(key) = kbd.poll() else {
            core::hint::spin_loop();
            continue;
        };

        let nav = match key {
            Key::Left => Some(0),
            Key::Right => Some(1),
            Key::Tab => Some((selected + 1) % 2),
            _ => None,
        };
        if let Some(sel) = nav {
            if sel != selected {
                selected = sel;
                if let Some(scr) = &screen {
                    draw_menu(scr, selected);
                }
            }
            continue;
        }

        match key {
            Key::Enter if selected == 0 => restart_now(&screen),
            Key::Enter => shutdown_now(&screen),
            Key::Letter(b'R') => restart_now(&screen),
            Key::Letter(b'S') => shutdown_now(&screen),
            _ => {}
        }
    }
}

/// Redraw both power buttons and the key-hint line.
fn draw_menu(scr: &Fb<'_>, selected: usize) {
    let (w, h) = (scr.width(), scr.height());
    let (bw, bh, gap) = (200u64, 44u64, 32u64);
    let y = h / 4 + 170;
    let x0 = w.saturating_sub(bw * 2 + gap) / 2;
    scr.button(x0, y, bw, bh, "Restart", selected == 0);
    scr.button(x0 + bw + gap, y, bw, bh, "Shutdown", selected == 1);
    let hint = "LEFT/RIGHT select   ENTER confirm   R restart   S shutdown";
    let hx = w.saturating_sub(Fb::text_width(hint, 2)) / 2;
    scr.draw_text(hx, y + bh + 24, hint, 0x0099_aabb, 2);
}

/// Status line above the ready banner; erases any previous one.
fn announce(scr: &Fb<'_>, text: &str, color: u32) {
    let (w, h) = (scr.width(), scr.height());
    scr.fill_rect(
        16,
        h.saturating_sub(104),
        w.saturating_sub(32),
        24,
        0x0016_222e,
    );
    let x = w.saturating_sub(Fb::text_width(text, 2)) / 2;
    scr.draw_text(x, h.saturating_sub(100), text, color, 2);
}

fn restart_now(screen: &Option<Fb<'_>>) -> ! {
    if let Some(scr) = screen {
        announce(scr, "Restarting...", 0x00ff_cc33);
    }
    kprintln!("[power] restart");
    power::restart()
}

fn shutdown_now(screen: &Option<Fb<'_>>) {
    if let Some(scr) = screen {
        announce(scr, "Shutting down...", 0x0099_e0ff);
    }
    kprintln!("[power] shutdown");
    if !power::shutdown() {
        if let Some(scr) = screen {
            announce(scr, "shutdown FAILED", 0x00ff_5555);
        }
        kprintln!("[power] shutdown FAILED — machine still running");
    }
}
