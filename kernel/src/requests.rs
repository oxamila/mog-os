//! Limine boot-protocol requests.
//!
//! Every static below is scanned by Limine inside the loaded image between
//! the start/end markers. `#[used]` stops rustc from dropping them and the
//! `KEEP(...)` rules in `linker-x86_64.ld` stop ld from garbage-collecting
//! the sections.

use limine::BaseRevision;
use limine::request::{
    FramebufferRequest, MemoryMapRequest, RequestsEndMarker, RequestsStartMarker, RsdpRequest,
};

/// Base revision tag. `BaseRevision::new()` requests revision 3, which the
/// Limine 12.x bootloader accepts on x86-64.
#[used]
#[unsafe(link_section = ".requests")]
pub static BASE_REVISION: BaseRevision = BaseRevision::new();

#[used]
#[unsafe(link_section = ".requests")]
pub static FRAMEBUFFER_REQUEST: FramebufferRequest = FramebufferRequest::new();

#[used]
#[unsafe(link_section = ".requests")]
pub static MEMMAP_REQUEST: MemoryMapRequest = MemoryMapRequest::new();

#[used]
#[unsafe(link_section = ".requests")]
pub static RSDP_REQUEST: RsdpRequest = RsdpRequest::new();

#[used]
#[unsafe(link_section = ".requests_start_marker")]
pub static _START_MARKER: RequestsStartMarker = RequestsStartMarker::new();

#[used]
#[unsafe(link_section = ".requests_end_marker")]
pub static _END_MARKER: RequestsEndMarker = RequestsEndMarker::new();
