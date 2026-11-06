//! GDT + TSS (one IST stack for double faults).

use core::arch::asm;
use core::cell::UnsafeCell;

pub const KCODE: u16 = 0x08;
pub const KDATA: u16 = 0x10;
const TSS_SEL: u16 = 0x18;
pub const IST_DF: u8 = 1;

#[repr(C, packed)]
struct Tss {
    _r0: u32,
    rsp: [u64; 3],
    _r1: u64,
    ist: [u64; 7],
    _r2: u64,
    _r3: u16,
    iomap: u16,
}

#[repr(C, align(16))]
struct Stack([u8; 16384]);

struct G<T>(UnsafeCell<T>);
// SAFETY: written once during single-threaded early boot.
unsafe impl<T> Sync for G<T> {}

static TSS: G<Tss> = G(UnsafeCell::new(Tss { _r0: 0, rsp: [0; 3], _r1: 0, ist: [0; 7], _r2: 0, _r3: 0, iomap: 104 }));
static DF_STACK: G<Stack> = G(UnsafeCell::new(Stack([0; 16384])));
static GDT: G<[u64; 5]> = G(UnsafeCell::new([0; 5]));

#[repr(C, packed)]
struct DescPtr {
    limit: u16,
    base: u64,
}

pub fn init() {
    // SAFETY: early boot, single CPU, nothing else touches these statics.
    unsafe {
        let tss = TSS.0.get();
        let df_top = DF_STACK.0.get() as u64 + 16384;
        (*tss).ist[(IST_DF - 1) as usize] = df_top;

        let base = tss as u64;
        let limit = (core::mem::size_of::<Tss>() - 1) as u64;
        let gdt = &mut *GDT.0.get();
        gdt[0] = 0;
        gdt[1] = 0x00AF_9A00_0000_FFFF; // 64-bit code, DPL0
        gdt[2] = 0x00CF_9200_0000_FFFF; // data, DPL0
        gdt[3] = (limit & 0xFFFF)
            | ((base & 0xFF_FFFF) << 16)
            | (0x89u64 << 40)
            | (((limit >> 16) & 0xF) << 48)
            | (((base >> 24) & 0xFF) << 56);
        gdt[4] = base >> 32;

        let dp = DescPtr { limit: (5 * 8 - 1) as u16, base: gdt.as_ptr() as u64 };
        asm!("lgdt [{}]", in(reg) &dp, options(readonly, nostack, preserves_flags));
        // Reload CS through a far return, then the data segments.
        asm!(
            "push {cs}",
            "lea {t}, [rip + 2f]",
            "push {t}",
            "retfq",
            "2:",
            cs = in(reg) KCODE as u64,
            t = out(reg) _,
            options(preserves_flags)
        );
        asm!(
            "mov ds, {d:x}", "mov es, {d:x}", "mov ss, {d:x}",
            "mov fs, {z:x}", "mov gs, {z:x}",
            d = in(reg) KDATA, z = in(reg) 0u16,
            options(nostack, preserves_flags)
        );
        asm!("ltr {:x}", in(reg) TSS_SEL, options(nostack, preserves_flags));
    }
}
