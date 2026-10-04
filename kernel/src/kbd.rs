//! Polling PS/2 keyboard driver (i8042, scan code set 1).
//!
//! The IDT and IRQs arrive in a later phase, so we poll the controller's
//! output buffer instead of taking interrupts. The firmware has already
//! enabled the keyboard; `init` only drains stale bytes.
//!
//! Only the keys the UI needs are decoded (arrows, Enter, Tab, R, S).

use crate::io::inb;

const KBC_STATUS: u16 = 0x64;
const KBC_DATA: u16 = 0x60;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Key {
    Left,
    Right,
    Enter,
    Tab,
    Letter(u8),
}

pub struct Kbd {
    /// Set after an 0xE0 prefix (extended keys: arrows).
    extended: bool,
    /// Bytes still to ignore after 0xE1 (Pause: E1 1D 45 E1 9D C5).
    skip: u8,
}

impl Kbd {
    pub const fn new() -> Self {
        Self {
            extended: false,
            skip: 0,
        }
    }

    /// Drain bytes the firmware left pending so the first poll is clean.
    pub fn init(&mut self) {
        let mut drained = 0;
        while inb(KBC_STATUS) & 0x01 != 0 && drained < 1024 {
            let _ = inb(KBC_DATA);
            drained += 1;
        }
    }

    /// Non-blocking: return the next key *press* (release codes are skipped).
    pub fn poll(&mut self) -> Option<Key> {
        while inb(KBC_STATUS) & 0x01 != 0 {
            let sc = inb(KBC_DATA);
            if self.skip > 0 {
                self.skip -= 1;
                continue;
            }
            match sc {
                0xE1 => {
                    self.skip = 5;
                    continue;
                }
                0xE0 => {
                    self.extended = true;
                    continue;
                }
                _ => {}
            }
            if sc & 0x80 != 0 {
                // Break (release) code — ignore, clear pending prefix.
                self.extended = false;
                continue;
            }
            let key = self.decode(sc);
            self.extended = false;
            if key.is_some() {
                return key;
            }
        }
        None
    }

    fn decode(&self, sc: u8) -> Option<Key> {
        if self.extended {
            return match sc {
                0x4B => Some(Key::Left),  // left arrow
                0x4D => Some(Key::Right), // right arrow
                _ => None,
            };
        }
        match sc {
            0x1C => Some(Key::Enter),
            0x0F => Some(Key::Tab),
            0x13 => Some(Key::Letter(b'R')),
            0x1F => Some(Key::Letter(b'S')),
            _ => None,
        }
    }
}

impl Default for Kbd {
    fn default() -> Self {
        Self::new()
    }
}
