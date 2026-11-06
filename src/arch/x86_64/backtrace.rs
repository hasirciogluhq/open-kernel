//! Frame-pointer backtrace (kernel is built with `force-frame-pointers`).

use crate::kprintln;

pub fn print_from(mut rbp: u64) {
    kprintln!("backtrace:");
    let mut prev = 0u64;
    for i in 0..24 {
        // Plausible frame: aligned, above the kernel image, increasing, not absurdly far.
        if rbp < 0x20_0000 || rbp % 8 != 0 || (prev != 0 && (rbp <= prev || rbp - prev > (1 << 20))) {
            break;
        }
        // SAFETY: bounds-checked heuristically; RAM is identity mapped.
        let (next, ret) = unsafe { (*(rbp as *const u64), *((rbp + 8) as *const u64)) };
        if ret == 0 {
            break;
        }
        kprintln!("  #{:<2} {:#x}", i, ret);
        prev = rbp;
        rbp = next;
    }
}
