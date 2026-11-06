//! `#[global_allocator]` on top of the xlang heap. The heap is not reentrant,
//! so allocation runs with interrupts disabled (single CPU).

use super::heap::{kfree, kmalloc};
use crate::arch::cpu::without_interrupts;
use core::alloc::{GlobalAlloc, Layout};

pub struct KernelAlloc;

unsafe impl GlobalAlloc for KernelAlloc {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        if l.align() > 32 {
            return core::ptr::null_mut();
        }
        // SAFETY: heap initialised in mm::init before any allocation.
        without_interrupts(|| unsafe { kmalloc(l.size()) })
    }
    unsafe fn dealloc(&self, p: *mut u8, _l: Layout) {
        // SAFETY: `p` came from `alloc`.
        without_interrupts(|| unsafe { kfree(p) })
    }
}

#[global_allocator]
static ALLOC: KernelAlloc = KernelAlloc;
