//! Panic handler: shout over serial, then paint the whole screen red so a
//! failure is visible even if serial is not being watched.

use core::panic::PanicInfo;

use crate::fb::Fb;
use crate::requests::FRAMEBUFFER_REQUEST;
use crate::{halt_loop, kprintln};

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    kprintln!("\n*** KERNEL PANIC ***\n{info}");

    // Best effort: solid dark red screen.
    if let Some(resp) = FRAMEBUFFER_REQUEST.get_response()
        && let Some(fb) = resp.framebuffers().next()
    {
        Fb::new(fb).fill(0x00990000);
    }

    kprintln!("system halted");
    halt_loop()
}
