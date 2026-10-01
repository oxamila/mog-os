//! Minimal serial driver:16550 UART on COM1 (port 0x3F8), 115200 8N1.
//!
//! Hand-rolled with raw port I/O so every byte of the init sequence is
//! visible — no black-box crate between you and the hardware.

use core::fmt::{self, Write};

const COM1: u16 = 0x3F8;

/// Write a byte to an I/O port (`out dx, al`).
#[inline]
fn outb(port: u16, val: u8) {
    unsafe {
        core::arch::asm!(
            "out dx, al",
            in("dx") port,
            in("al") val,
            options(nomem, nostack, preserves_flags)
        );
    }
}

/// Read a byte from an I/O port (`in al, dx`).
#[inline]
fn inb(port: u16) -> u8 {
    let val: u8;
    unsafe {
        core::arch::asm!(
            "in al, dx",
            out("al") val,
            in("dx") port,
            options(nomem, nostack, preserves_flags)
        );
    }
    val
}

pub struct SerialPort {
    base: u16,
}

impl SerialPort {
    pub const fn new(base: u16) -> Self {
        Self { base }
    }

    /// Full 16550 power-on sequence: disable interrupts, program the baud
    /// divisor (1 → 115200), 8 data bits, no parity, 1 stop bit, FIFO on.
    pub fn init(&self) {
        self.reg_write(1, 0x00); // disable all interrupts
        self.reg_write(3, 0x80); // enable DLAB (divisor latch access)
        self.reg_write(0, 0x01); // divisor low byte  (115200 baud)
        self.reg_write(1, 0x00); // divisor high byte
        self.reg_write(3, 0x03); // 8N1, DLAB off
        self.reg_write(2, 0xC7); // enable + clear FIFO, 14-byte threshold
        self.reg_write(4, 0x0B); // DTR + RTS + OUT2 (modem control)
    }

    #[inline]
    fn reg_write(&self, offset: u16, val: u8) {
        outb(self.base + offset, val);
    }

    #[inline]
    fn reg_read(&self, offset: u16) -> u8 {
        inb(self.base + offset)
    }

    fn write_byte(&self, byte: u8) {
        // Line status register bit 5 = transmit holding register empty.
        // Bounded wait: on machines with no decoded COM1 the status bit may
        // never appear — never hang the kernel just to print a log line.
        for _ in 0..1_000 {
            if self.reg_read(5) & 0x20 != 0 {
                break;
            }
            core::hint::spin_loop();
        }
        self.reg_write(0, byte);
    }
}

/// Adapter so `core::fmt` machinery can target our port (`&self` only —
/// no `static mut`, no interior mutability).
struct SerialWriter<'a>(&'a SerialPort);

impl Write for SerialWriter<'_> {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for byte in s.bytes() {
            if byte == b'\n' {
                self.0.write_byte(b'\r');
            }
            self.0.write_byte(byte);
        }
        Ok(())
    }
}

static COM1_PORT: SerialPort = SerialPort::new(COM1);

pub fn init() {
    COM1_PORT.init();
}

pub fn _print(args: fmt::Arguments<'_>) {
    let _ = SerialWriter(&COM1_PORT).write_fmt(args);
}

/// Print to the kernel serial console (COM1).
#[macro_export]
macro_rules! kprint {
    ($($arg:tt)*) => {
        $crate::serial::_print(core::format_args!($($arg)*))
    };
}

/// Print to the kernel serial console, followed by a newline.
#[macro_export]
macro_rules! kprintln {
    () => {
        $crate::kprint!("\n")
    };
    ($($arg:tt)*) => {
        $crate::kprint!("{}\n", core::format_args!($($arg)*))
    };
}
