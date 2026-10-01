//! Pixel-level access to the UEFI GOP framebuffer handed to us by Limine.
//!
//! Limine maps framebuffer memory write-combining (WC, PAT[5]) and everything
//! else write-back — exactly the caching split an Atom's small caches want.
//! We only ever touch 32-bpp surfaces (XRGB8888), which is what QEMU/OVMF and
//! every real UEFI GOP mode hand out in practice.

use font8x8::legacy::BASIC_LEGACY;
use limine::framebuffer::Framebuffer;

pub struct Fb<'a> {
    info: Framebuffer<'a>,
}

impl<'a> Fb<'a> {
    pub fn new(info: Framebuffer<'a>) -> Self {
        Self { info }
    }

    pub fn width(&self) -> u64 {
        self.info.width()
    }

    pub fn height(&self) -> u64 {
        self.info.height()
    }

    #[inline]
    fn pixel_ptr(&self, x: u64, y: u64) -> *mut u32 {
        let offset = y * self.info.pitch() + x * u64::from(self.info.bpp() / 8);
        unsafe { self.info.addr().add(offset as usize) as *mut u32 }
    }

    #[inline]
    pub fn put_pixel(&self, x: u64, y: u64, color: u32) {
        if x >= self.info.width() || y >= self.info.height() {
            return;
        }
        unsafe { core::ptr::write_volatile(self.pixel_ptr(x, y), color) };
    }

    #[inline]
    pub fn get_pixel(&self, x: u64, y: u64) -> u32 {
        unsafe { core::ptr::read_volatile(self.pixel_ptr(x, y)) }
    }

    pub fn fill(&self, color: u32) {
        for y in 0..self.info.height() {
            for x in 0..self.info.width() {
                self.put_pixel(x, y, color);
            }
        }
    }

    /// Hollow rectangle with a fixed thickness, in pixels.
    pub fn draw_frame(&self, x: u64, y: u64, w: u64, h: u64, color: u32, thickness: u64) {
        for t in 0..thickness {
            // top + bottom rows
            for px in x..x.saturating_add(w) {
                self.put_pixel(px, y + t, color);
                self.put_pixel(px, y + h - 1 - t, color);
            }
            // left + right columns
            for py in y..y.saturating_add(h) {
                self.put_pixel(x + t, py, color);
                self.put_pixel(x + w - 1 - t, py, color);
            }
        }
    }

    /// Draw ASCII text using the 8x8 bitmap font, scaled by `scale`.
    /// Bit 0 of each glyph row is the leftmost pixel (font8x8 convention).
    pub fn draw_text(&self, x: u64, y: u64, text: &str, color: u32, scale: u64) {
        let mut cursor_x = x;
        for ch in text.chars() {
            if (ch as u32) >= 128 {
                cursor_x += 8 * scale;
                continue;
            }
            let glyph = BASIC_LEGACY[ch as usize];
            for (row_idx, row) in glyph.iter().enumerate() {
                for bit in 0..8u32 {
                    if row & (1 << bit) == 0 {
                        continue;
                    }
                    let px = cursor_x + u64::from(bit) * scale;
                    let py = y + row_idx as u64 * scale;
                    for sy in 0..scale {
                        for sx in 0..scale {
                            self.put_pixel(px + sx, py + sy, color);
                        }
                    }
                }
            }
            cursor_x += 8 * scale;
        }
    }

    /// Pixel width of `text` at `scale` (for centering).
    pub fn text_width(text: &str, scale: u64) -> u64 {
        text.chars().count() as u64 * 8 * scale
    }
}
