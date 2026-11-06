//! Memory management: xlang heap + PMM, Rust glue.

pub mod alloc_glue;
pub mod heap;
pub mod pmm;

use crate::bootinfo::BootInfo;
use crate::kprintln;

const EFI_CONVENTIONAL: u32 = 7;
const MIB: u64 = 1 << 20;

/// Bring up PMM and heap from the firmware memory map. Returns (free MiB, heap MiB).
pub fn init(bi: &BootInfo) -> (u64, u64) {
    // Highest usable frame decides the bitmap size.
    let mut max_end = 0u64;
    let mut best = (0u64, 0u64); // largest conventional region above 1 MiB
    for d in bi.memory_map() {
        if d.kind != EFI_CONVENTIONAL || d.phys < MIB {
            continue;
        }
        let len = d.pages * 4096;
        max_end = max_end.max(d.phys + len);
        if len > best.1 {
            best = (d.phys, len);
        }
    }
    let pages = (max_end / 4096) as usize;
    let bitmap_bytes = pages.div_ceil(64) * 8;
    let bitmap_pages = bitmap_bytes.div_ceil(4096);

    // Layout of the best region: [bitmap][heap up to 16 MiB]...rest stays with the PMM.
    let heap_base = best.0 + (bitmap_pages * 4096) as u64;
    let heap_size = (best.1 - (bitmap_pages * 4096) as u64).min(16 * MIB);

    // SAFETY: regions are free RAM from the firmware map, identity mapped, used by nothing else.
    unsafe {
        pmm::pmm_init(best.0 as usize, pages);
        for d in bi.memory_map() {
            if d.kind == EFI_CONVENTIONAL && d.phys >= MIB {
                pmm::pmm_mark((d.phys / 4096) as usize, d.pages as usize, 1);
            }
        }
        pmm::pmm_mark((best.0 / 4096) as usize, bitmap_pages, 0);
        pmm::pmm_mark((heap_base / 4096) as usize, (heap_size / 4096) as usize, 0);
        heap::heap_init(heap_base as usize, heap_size as usize);
    }

    // PMM self-test.
    unsafe {
        let before = pmm::pmm_free_count();
        let a = pmm::pmm_alloc();
        let b = pmm::pmm_alloc_pages(4);
        assert!(a != 0 && b != 0 && a != b);
        pmm::pmm_free_pages(a, 1);
        pmm::pmm_free_pages(b, 4);
        assert!(pmm::pmm_free_count() == before);
        kprintln!("pmm: ok ({} MiB free of {} frames tracked)", (before * 4096) as u64 / MIB, pages);
    }
    heap_selftest();
    (unsafe { pmm::pmm_free_count() } as u64 * 4096 / MIB, heap_size / MIB)
}

fn heap_selftest() {
    use alloc::{boxed::Box, vec::Vec};
    let b = Box::new(0xC0FFEEu64);
    let mut v: Vec<u32> = Vec::new();
    for i in 0..1000 {
        v.push(i);
    }
    assert!(*b == 0xC0FFEE && v.iter().sum::<u32>() == 499_500);
    drop(v);
    drop(b);
    // SAFETY: read-only counter.
    assert!(unsafe { heap::heap_used() } == 0);
    kprintln!("heap: ok (xlang kmalloc/kfree behind Rust GlobalAlloc)");
}
